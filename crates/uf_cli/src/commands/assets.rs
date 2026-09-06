//! `uf assets` — the image and font pipeline, as a service.
//!
//! The same shape as `uf transform` and for the same reason: Vite runs its
//! plugins in JavaScript, the work is native, and a plugin that spawned `uf`
//! per image would pay process start-up once per asset per build. One
//! long-lived process instead, newline-delimited JSON in, newline-delimited
//! JSON out, replies in request order.
//!
//! This is the only channel. `uf build` and `uf dev` both drive the Vite
//! plugin, the plugin drives this, and this drives `uf_assets` — so a dev
//! server and a build cannot disagree about what an image becomes, because
//! there is one implementation and one set of parameters and both come from
//! `uf.config.js`.
//!
//! Request, one per line:
//!
//! ```json
//! {"kind": "image", "id": "/abs/hero.png", "outDir": "/abs/dist/assets"}
//! {"kind": "image", "id": "/abs/hero.png", "outDir": "…", "widths": [400], "quality": 90}
//! {"kind": "font",  "id": "/abs/Inter.woff2", "outDir": "…", "family": "Inter",
//!  "baseUrl": "/assets/"}
//! ```
//!
//! Everything but `kind`, `id` and `outDir` is optional and falls back to the
//! project's `app.builtins.images` / `app.builtins.fonts`. A caller that sends
//! nothing gets the project's defaults, which is what makes an unannotated
//! `import hero from "./hero.png"` work.
//!
//! Reply, one per line, in order:
//!
//! ```json
//! {"id": "…", "image": {"width": 3000, "height": 2000, "variants": [ … ], … }}
//! {"id": "…", "font":  {"file": "…", "css": "…", "fallback": { … }, … }}
//! {"id": "…", "error": "…"}
//! ```
//!
//! An error is a reply and not a crash: one unreadable image must not take the
//! build's whole asset pass with it, and the plugin turns the message into a
//! diagnostic naming the import.

use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use uf_assets::{FontRequest, ImageRequest};
use uf_config::{FontsConfig, ImagesConfig, load_config};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    kind: Kind,
    id: String,
    out_dir: Utf8PathBuf,
    /// Image: the widths this import asks for, overriding the project's.
    #[serde(default)]
    widths: Option<Vec<u32>>,
    /// Image: the quality this import asks for.
    #[serde(default)]
    quality: Option<u8>,
    /// Image: whether this import wants a blur placeholder.
    #[serde(default)]
    blur: Option<bool>,
    /// Font: the family the face is declared under.
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    weight: Option<String>,
    #[serde(default)]
    style: Option<String>,
    #[serde(default)]
    display: Option<String>,
    /// Font: what the emitted file's name is joined to for `src: url()`.
    #[serde(default)]
    base_url: Option<String>,
    /// Font: the local face to scale, or an explicit `null` for none.
    ///
    /// Two levels of `Option` say two different things, and both are needed:
    /// the outer one is "the request did not mention a fallback, use the
    /// project's", the inner is "this import asked for no fallback at all".
    #[serde(default)]
    fallback: Option<Option<String>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Image,
    Font,
}

#[derive(Debug, Default, Serialize)]
struct Reply {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<uf_assets::ImageAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font: Option<uf_assets::FontAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// What the project's config says about every asset.
#[derive(Debug, Clone)]
struct ProjectAssets {
    images: ImagesConfig,
    fonts: FontsConfig,
}

/// Serve asset requests until stdin closes.
pub(crate) fn assets_service(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let project = ProjectAssets {
        images: resolved.config.app.builtins.images.clone(),
        fonts: resolved.config.app.builtins.fonts.clone(),
    };
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    serve(stdin, &mut stdout, &project)
}

fn serve(input: impl Read, out: &mut impl Write, project: &ProjectAssets) -> Result<()> {
    for line in BufReader::new(input).lines() {
        let line = line.context("reading an asset request")?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle(&request, project),
            Err(error) => Reply {
                error: Some(format!("malformed request: {error}")),
                ..Reply::default()
            },
        };
        serde_json::to_writer(&mut *out, &reply).context("writing an asset reply")?;
        out.write_all(b"\n")?;
        // A build blocks on this reply, so it cannot wait for a full buffer.
        out.flush()?;
    }
    Ok(())
}

fn handle(request: &Request, project: &ProjectAssets) -> Reply {
    let source = Utf8Path::new(&request.id);
    match request.kind {
        Kind::Image => {
            let widths = request
                .widths
                .clone()
                .unwrap_or_else(|| project.images.widths.clone());
            let result = uf_assets::transform(&ImageRequest {
                source,
                widths: &widths,
                quality: request.quality.unwrap_or(project.images.quality),
                blur: request.blur.unwrap_or(project.images.placeholder),
                out_dir: &request.out_dir,
            });
            match result {
                Ok(image) => Reply {
                    id: request.id.clone(),
                    image: Some(image),
                    ..Reply::default()
                },
                Err(error) => Reply {
                    id: request.id.clone(),
                    error: Some(error.to_string()),
                    ..Reply::default()
                },
            }
        }
        Kind::Font => {
            // The family is what the `@font-face` is declared under, so it has
            // to be *something*. The file's stem is the answer a person would
            // give if asked, and it keeps an unannotated import working.
            let family = request
                .family
                .clone()
                .unwrap_or_else(|| source.file_stem().unwrap_or("Font").to_owned());
            let fallback = match &request.fallback {
                Some(explicit) => explicit.clone(),
                None => Some(project.fonts.fallback.clone()),
            };
            let result = uf_assets::self_host(&FontRequest {
                source,
                family: &family,
                weight: request.weight.as_deref().unwrap_or("400"),
                style: request.style.as_deref().unwrap_or("normal"),
                display: request.display.as_deref().unwrap_or(&project.fonts.display),
                fallback: fallback.as_deref(),
                out_dir: &request.out_dir,
                base_url: request.base_url.as_deref().unwrap_or(""),
            });
            match result {
                Ok(font) => Reply {
                    id: request.id.clone(),
                    font: Some(font),
                    ..Reply::default()
                },
                Err(error) => Reply {
                    id: request.id.clone(),
                    error: Some(error.to_string()),
                    ..Reply::default()
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectAssets {
        ProjectAssets {
            images: ImagesConfig::default(),
            fonts: FontsConfig::default(),
        }
    }

    fn replies(input: &str) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        serve(input.as_bytes(), &mut out, &project()).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        (dir, path)
    }

    fn graphic(path: &Utf8Path, width: u32, height: u32) {
        let mut buffer = image::RgbaImage::new(width, height);
        for (x, y, pixel) in buffer.enumerate_pixels_mut() {
            let on = ((x / 16) + (y / 16)) % 2 == 0;
            *pixel = if on {
                image::Rgba([17, 24, 39, 255])
            } else {
                image::Rgba([249, 250, 251, 255])
            };
        }
        image::DynamicImage::ImageRgba8(buffer)
            .save_with_format(path, image::ImageFormat::Png)
            .unwrap();
    }

    #[test]
    fn an_image_comes_back_with_the_files_that_are_on_disk() {
        let (_guard, dir) = temp();
        let source = dir.join("hero.png");
        graphic(&source, 400, 300);
        let out = dir.join("out");

        let request = serde_json::json!({
            "kind": "image",
            "id": source,
            "outDir": out,
            "widths": [100, 200],
        });
        let replies = replies(&format!("{request}\n"));

        let image = &replies[0]["image"];
        assert!(replies[0]["error"].is_null(), "{}", replies[0]);
        assert_eq!(image["width"], 400);
        assert_eq!(image["height"], 300);

        let variants = image["variants"].as_array().unwrap();
        assert!(!variants.is_empty());
        for variant in variants {
            let file = variant["file"].as_str().unwrap();
            let on_disk = std::fs::metadata(out.join(file))
                .unwrap_or_else(|_| panic!("{file} was named but not written"));
            assert_eq!(on_disk.len(), variant["bytes"].as_u64().unwrap());
        }
    }

    #[test]
    fn a_request_that_says_nothing_gets_the_project_defaults() {
        // The unannotated `import hero from "./hero.png"` case, which is what
        // almost every import is.
        let (_guard, dir) = temp();
        let source = dir.join("hero.png");
        graphic(&source, 900, 600);
        let out = dir.join("out");

        let request = serde_json::json!({ "kind": "image", "id": source, "outDir": out });
        let replies = replies(&format!("{request}\n"));

        let widths: Vec<u64> = replies[0]["image"]["variants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|variant| variant["width"].as_u64().unwrap())
            .collect();
        // 640 and 750 are in the default ladder and below the source's width;
        // everything above 900 is dropped rather than upscaled.
        assert!(widths.contains(&640), "{widths:?}");
        assert!(widths.contains(&750), "{widths:?}");
        assert!(widths.contains(&900), "{widths:?}");
        assert!(!widths.iter().any(|width| *width > 900), "{widths:?}");
        // The default asks for a placeholder.
        assert!(replies[0]["image"]["blur"].as_str().is_some());
    }

    #[test]
    fn a_font_comes_back_with_its_stylesheet_and_its_overrides() {
        let (_guard, dir) = temp();
        let source = dir.join("Inter.ttf");
        std::fs::write(&source, font_fixture()).unwrap();
        let out = dir.join("out");

        let request = serde_json::json!({
            "kind": "font",
            "id": source,
            "outDir": out,
            "family": "Inter",
            "baseUrl": "/assets/",
        });
        let replies = replies(&format!("{request}\n"));

        let font = &replies[0]["font"];
        assert!(replies[0]["error"].is_null(), "{}", replies[0]);
        let file = font["file"].as_str().unwrap();
        assert!(out.join(file).exists(), "{file} was named but not written");

        let css = font["css"].as_str().unwrap();
        assert!(css.contains("@font-face"), "{css}");
        assert!(css.contains("size-adjust:"), "{css}");
        assert!(css.contains(&format!(r#"url("/assets/{file}")"#)), "{css}");
        assert_eq!(font["fallbackFamily"], "Inter Fallback");
    }

    #[test]
    fn an_unreadable_image_is_one_error_rather_than_a_dead_service() {
        // A build with a broken image has to report the broken image and go on
        // transforming the rest; a service that exits takes every asset after
        // it as well.
        let (_guard, dir) = temp();
        let broken = dir.join("broken.png");
        std::fs::write(&broken, b"not a png").unwrap();
        let good = dir.join("good.png");
        graphic(&good, 64, 64);
        let out = dir.join("out");

        let first = serde_json::json!({ "kind": "image", "id": broken, "outDir": out });
        let second = serde_json::json!({ "kind": "image", "id": good, "outDir": out });
        let replies = replies(&format!("{first}\n{second}\n"));

        assert_eq!(replies.len(), 2);
        // Unreadable bytes with an image extension are passed through with a
        // note rather than failing — the file is served as the author wrote it.
        assert!(replies[0]["error"].is_null(), "{}", replies[0]);
        assert!(
            replies[0]["image"]["note"].as_str().is_some(),
            "{}",
            replies[0]
        );
        assert!(replies[1]["image"]["width"].as_u64().is_some());
    }

    #[test]
    fn a_missing_file_is_an_error_naming_it() {
        let (_guard, dir) = temp();
        let request = serde_json::json!({
            "kind": "image",
            "id": dir.join("nowhere.png"),
            "outDir": dir.join("out"),
        });
        let replies = replies(&format!("{request}\n"));

        let error = replies[0]["error"].as_str().unwrap();
        assert!(error.contains("nowhere.png"), "{error}");
    }

    #[test]
    fn replies_come_back_in_request_order() {
        let (_guard, dir) = temp();
        let out = dir.join("out");
        let mut input = String::new();
        for index in 0..6 {
            let source = dir.join(format!("i{index}.png"));
            graphic(&source, 32 + index * 8, 32);
            let request = serde_json::json!({
                "kind": "image", "id": source, "outDir": out, "widths": [16],
            });
            input.push_str(&format!("{request}\n"));
        }
        let replies = replies(&input);

        assert_eq!(replies.len(), 6);
        for (index, reply) in replies.iter().enumerate() {
            assert!(
                reply["id"]
                    .as_str()
                    .unwrap()
                    .ends_with(&format!("i{index}.png")),
                "{reply}"
            );
        }
    }

    #[test]
    fn a_blank_line_is_skipped_and_garbage_is_named() {
        let replies = replies("\nnot json\n");

        assert_eq!(replies.len(), 1);
        assert!(replies[0]["error"].as_str().unwrap().contains("malformed"));
    }

    /// A minimal but real SFNT with the three tables the pipeline reads.
    fn font_fixture() -> Vec<u8> {
        let mut head = vec![0u8; 54];
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        let mut hhea = vec![0u8; 36];
        hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        let mut os2 = vec![0u8; 96];
        os2[0..2].copy_from_slice(&4u16.to_be_bytes());
        os2[2..4].copy_from_slice(&500i16.to_be_bytes());
        let tables = [(*b"head", head), (*b"hhea", hhea), (*b"OS/2", os2)];

        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0u8; 6]);
        let mut offset = 12 + tables.len() * 16;
        for (tag, data) in &tables {
            out.extend_from_slice(tag);
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&(offset as u32).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            offset += data.len();
        }
        for (_, data) in &tables {
            out.extend_from_slice(data);
        }
        out
    }
}
