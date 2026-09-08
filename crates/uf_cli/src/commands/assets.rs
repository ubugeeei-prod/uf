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
//! {"kind": "font",  "id": "/abs/Inter.ttf", "outDir": "…", "family": "Inter",
//!  "baseUrl": "/assets/", "subset": "ranges"}
//! {"kind": "icon",  "id": "/abs/icons/star.svg", "outDir": "…", "name": "star"}
//! {"kind": "og",    "id": "/abs/app/card.og.json", "outDir": "…"}
//! {"kind": "sprite", "id": "uf:icon-sprite", "outDir": "…", "icons": [ … ]}
//! ```
//!
//! Everything but `kind`, `id` and `outDir` is optional and falls back to the
//! project's `app.builtins.images` / `.fonts` / `.icons` / `.og`. A caller that
//! sends nothing gets the project's defaults, which is what makes an
//! unannotated `import hero from "./hero.png"` work.
//!
//! Reply, one per line, in order:
//!
//! ```json
//! {"id": "…", "image": {"width": 3000, "height": 2000, "variants": [ … ], … }}
//! {"id": "…", "font":  {"file": "…", "css": "…", "faces": [ … ], … }}
//! {"id": "…", "icon":  {"id": "uf-icon-star-…", "viewBox": "0 0 24 24", … }}
//! {"id": "…", "og":    {"file": "…", "width": 1200, "height": 630, … }}
//! {"id": "…", "sprite": {"file": "…", "markup": "<svg …>", "symbols": 7}}
//! {"id": "…", "error": "…"}
//! ```
//!
//! An error is a reply and not a crash: one unreadable image must not take the
//! build's whole asset pass with it, and the plugin turns the message into a
//! diagnostic naming the import.
//!
//! # The sprite, and why the caller carries the set
//!
//! An icon sprite has to hold exactly the icons a build reached, which is
//! knowable only after the build has resolved its imports. The obvious way to
//! do that here is to accumulate every `icon` reply in this process and answer
//! a `sprite` request from what has accumulated — and it is wrong, because
//! this process does not live as long as a build. The Vite plugin closes it in
//! `buildEnd`, which runs *before* `generateBundle`, so the sprite would be
//! assembled by a fresh process that had seen nothing; and a build with a
//! client environment and a server one has two bundles and one set of icons
//! between them.
//!
//! So a `sprite` request carries its own `icons`, and every request in this
//! protocol is answerable from itself. The plugin is what spans the build and
//! is therefore what remembers; this is a function. A `sprite` request with no
//! icons is not an error — it is an empty sprite, which is the correct answer
//! for a project that imported none.
//!
//! # Caching
//!
//! Every reply goes through [`uf_assets::cache`], keyed on the source bytes
//! and every parameter that reached the pipeline. A second build of an
//! unchanged project reads one small JSON document per asset and decodes,
//! resizes, subsets and rasterises nothing. The key is the whole of the
//! invalidation; see that module for why there is no other step.

use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use uf_assets::{FontRequest, IconRequest, ImageRequest, OgRequest, SpriteRequest, SubsetMode};
use uf_config::{FontsConfig, IconsConfig, ImagesConfig, OgConfig, load_config};

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
    /// Font: `"none"` or `"ranges"`, overriding the project's.
    #[serde(default)]
    subset: Option<String>,
    /// Font: the characters to cut the face down to.
    ///
    /// Present only when the import asked for it, and it wins over `subset`:
    /// naming the text is a stronger statement than naming a mode.
    #[serde(default)]
    text: Option<String>,
    /// Font: whether this face is preloaded.
    #[serde(default)]
    preload: Option<bool>,
    /// Icon: the name it was imported under.
    #[serde(default)]
    name: Option<String>,
    /// Sprite: every icon the build reached, as this service described them.
    ///
    /// Sent back rather than remembered here; see the module documentation for
    /// why this process is the wrong place to accumulate them.
    #[serde(default)]
    icons: Vec<uf_assets::IconAsset>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Image,
    Font,
    Icon,
    Og,
    Sprite,
}

#[derive(Debug, Default, Serialize)]
struct Reply {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<uf_assets::ImageAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    font: Option<uf_assets::FontAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<uf_assets::IconAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    og: Option<uf_assets::OgAsset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sprite: Option<uf_assets::Sprite>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// Whether the answer came out of the cache rather than being produced.
    ///
    /// Always present, because a build that cannot tell a warm pass from a
    /// cold one cannot report what its asset stage cost.
    cached: bool,
}

/// What the project's config says about every asset.
#[derive(Debug, Clone)]
struct ProjectAssets {
    images: ImagesConfig,
    fonts: FontsConfig,
    icons: IconsConfig,
    og: OgConfig,
    /// The project root, so a relative `og.font` resolves.
    root: Utf8PathBuf,
}

/// Serve asset requests until stdin closes.
pub(crate) fn assets_service(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let project = ProjectAssets {
        images: resolved.config.app.builtins.images.clone(),
        fonts: resolved.config.app.builtins.fonts.clone(),
        icons: resolved.config.app.builtins.icons.clone(),
        og: resolved.config.app.builtins.og.clone(),
        root: cwd.to_owned(),
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

/// The digest of a source file, for the cache key.
///
/// The file's bytes rather than its modification time: a checkout has no
/// useful mtimes, two files can share one, and the work this decides whether
/// to skip is orders of magnitude more expensive than a hash of the input to
/// it. The transform reads the file again; that second read is a fraction of
/// one decode.
fn source_digest(path: &Utf8Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))
}

fn failed(id: &str, error: impl std::fmt::Display) -> Reply {
    Reply {
        id: id.to_owned(),
        error: Some(error.to_string()),
        ..Reply::default()
    }
}

fn handle(request: &Request, project: &ProjectAssets) -> Reply {
    let source = Utf8Path::new(&request.id);
    match request.kind {
        Kind::Image => image(request, project, source),
        Kind::Font => font(request, project, source),
        Kind::Icon => icon(request, project, source),
        Kind::Og => og(request, project, source),
        Kind::Sprite => sprite(request),
    }
}

fn image(request: &Request, project: &ProjectAssets, source: &Utf8Path) -> Reply {
    let widths = request
        .widths
        .clone()
        .unwrap_or_else(|| project.images.widths.clone());
    let quality = request.quality.unwrap_or(project.images.quality);
    let blur = request.blur.unwrap_or(project.images.placeholder);
    let bytes = match source_digest(source) {
        Ok(bytes) => bytes,
        Err(error) => return failed(&request.id, error),
    };
    let key = uf_assets::cache_key(
        "image",
        &[
            &bytes,
            &widths
                .iter()
                .flat_map(|width| width.to_le_bytes())
                .collect::<Vec<u8>>(),
            &[quality, u8::from(blur)],
        ],
    );
    let produced = uf_assets::cache::memoise(
        &request.out_dir,
        &key,
        |asset: &uf_assets::ImageAsset| {
            asset
                .variants
                .iter()
                .map(|variant| variant.file.clone())
                .collect()
        },
        || {
            uf_assets::transform(&ImageRequest {
                source,
                widths: &widths,
                quality,
                blur,
                out_dir: &request.out_dir,
            })
        },
    );
    match produced {
        Ok(cached) => Reply {
            id: request.id.clone(),
            image: Some(cached.value),
            cached: cached.hit,
            ..Reply::default()
        },
        Err(error) => failed(&request.id, error),
    }
}

fn font(request: &Request, project: &ProjectAssets, source: &Utf8Path) -> Reply {
    // The family is what the `@font-face` is declared under, so it has to be
    // *something*. The file's stem is the answer a person would give if asked,
    // and it keeps an unannotated import working.
    let family = request
        .family
        .clone()
        .unwrap_or_else(|| source.file_stem().unwrap_or("Font").to_owned());
    let fallback = match &request.fallback {
        Some(explicit) => explicit.clone(),
        None => Some(project.fonts.fallback.clone()),
    };
    // `text` beats `subset`: naming the characters is a stronger statement
    // than naming a mode, and an import that did both meant the characters.
    let subset = match &request.text {
        Some(text) => SubsetMode::Text(text.clone()),
        None => {
            let declared = request.subset.as_deref().unwrap_or(&project.fonts.subset);
            match SubsetMode::from_name(declared) {
                Some(mode) => mode,
                None => {
                    return failed(
                        &request.id,
                        format!(
                            "{declared:?} is not a subset mode; app.builtins.fonts.subset is \
                             \"none\" or \"ranges\""
                        ),
                    );
                }
            }
        }
    };
    let preload = request.preload.unwrap_or(project.fonts.preload);
    let display = request.display.as_deref().unwrap_or(&project.fonts.display);
    let weight = request.weight.as_deref().unwrap_or("400");
    let style = request.style.as_deref().unwrap_or("normal");
    let base_url = request.base_url.as_deref().unwrap_or("");

    let bytes = match source_digest(source) {
        Ok(bytes) => bytes,
        Err(error) => return failed(&request.id, error),
    };
    let mode_key = match &subset {
        SubsetMode::Off => String::from("none"),
        SubsetMode::Ranges => String::from("ranges"),
        SubsetMode::Text(text) => format!("text:{text}"),
    };
    let key = uf_assets::cache_key(
        "font",
        &[
            &bytes,
            family.as_bytes(),
            weight.as_bytes(),
            style.as_bytes(),
            display.as_bytes(),
            base_url.as_bytes(),
            fallback.as_deref().unwrap_or("").as_bytes(),
            mode_key.as_bytes(),
            &[u8::from(preload), u8::from(fallback.is_some())],
        ],
    );
    let produced = uf_assets::cache::memoise(
        &request.out_dir,
        &key,
        |asset: &uf_assets::FontAsset| asset.faces.iter().map(|face| face.file.clone()).collect(),
        || {
            uf_assets::self_host(&FontRequest {
                source,
                family: &family,
                weight,
                style,
                display,
                fallback: fallback.as_deref(),
                out_dir: &request.out_dir,
                base_url,
                subset,
                preload,
            })
        },
    );
    match produced {
        Ok(cached) => Reply {
            id: request.id.clone(),
            font: Some(cached.value),
            cached: cached.hit,
            ..Reply::default()
        },
        Err(error) => failed(&request.id, error),
    }
}

fn icon(request: &Request, project: &ProjectAssets, source: &Utf8Path) -> Reply {
    if !project.icons.enabled {
        return failed(
            &request.id,
            "app.builtins.icons.enabled is false, so uf:icon/… resolves to nothing",
        );
    }
    let name = request
        .name
        .clone()
        .unwrap_or_else(|| source.file_stem().unwrap_or("icon").to_owned());
    let bytes = match source_digest(source) {
        Ok(bytes) => bytes,
        Err(error) => return failed(&request.id, error),
    };
    let key = uf_assets::cache_key("icon", &[&bytes, name.as_bytes()]);
    // An icon emits no file of its own — the sprite is the file — so there is
    // nothing on disk for a hit to be checked against.
    let produced = uf_assets::cache::memoise(
        &request.out_dir,
        &key,
        |_: &uf_assets::IconAsset| Vec::new(),
        || {
            uf_assets::icon(&IconRequest {
                source,
                name: &name,
            })
        },
    );
    match produced {
        // The reply carries the whole `<symbol>`, on a cache hit as well as a
        // miss: it is what the caller puts in the sprite, and a build whose
        // second run cached everything still reached every icon.
        Ok(cached) => Reply {
            id: request.id.clone(),
            icon: Some(cached.value),
            cached: cached.hit,
            ..Reply::default()
        },
        Err(error) => failed(&request.id, error),
    }
}

fn og(request: &Request, project: &ProjectAssets, source: &Utf8Path) -> Reply {
    if !project.og.enabled {
        return failed(
            &request.id,
            "app.builtins.og.enabled is false, so *.og.json is not drawn",
        );
    }
    let raw = match source_digest(source) {
        Ok(bytes) => bytes,
        Err(error) => return failed(&request.id, error),
    };
    let mut template: uf_assets::OgTemplate = match serde_json::from_slice(&raw) {
        Ok(template) => template,
        Err(error) => {
            return failed(
                &request.id,
                format!("{source} is not a template uf can draw: {error}"),
            );
        }
    };
    // The project's font is the default and the template's the override, so a
    // project points at one typeface once and its cards carry text and colour.
    if template.font.is_none()
        && let Some(declared) = project.og.font.as_deref()
    {
        template.font = Some(project.root.join(declared));
    }

    let key = uf_assets::cache_key(
        "og",
        &[
            &raw,
            project.og.font.as_deref().unwrap_or("").as_bytes(),
            // The font's own bytes: a template that did not change still draws
            // a different card when the typeface under it does.
            &template
                .font
                .as_ref()
                .and_then(|path| {
                    let resolved = if path.is_absolute() {
                        path.clone()
                    } else {
                        source.parent().unwrap_or(&project.root).join(path)
                    };
                    std::fs::read(resolved).ok()
                })
                .unwrap_or_default(),
        ],
    );
    let produced = uf_assets::cache::memoise(
        &request.out_dir,
        &key,
        |asset: &uf_assets::OgAsset| vec![asset.file.clone()],
        || {
            uf_assets::og::render(
                &OgRequest {
                    source,
                    out_dir: &request.out_dir,
                },
                &template,
                &raw,
            )
        },
    );
    match produced {
        Ok(cached) => Reply {
            id: request.id.clone(),
            og: Some(cached.value),
            cached: cached.hit,
            ..Reply::default()
        },
        Err(error) => failed(&request.id, error),
    }
}

fn sprite(request: &Request) -> Reply {
    match uf_assets::sprite(&SpriteRequest {
        icons: &request.icons,
        out_dir: &request.out_dir,
    }) {
        Ok(sprite) => Reply {
            id: request.id.clone(),
            sprite: Some(sprite),
            ..Reply::default()
        },
        Err(error) => failed(&request.id, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectAssets {
        ProjectAssets {
            images: ImagesConfig::default(),
            fonts: FontsConfig::default(),
            icons: IconsConfig::default(),
            og: OgConfig::default(),
            root: Utf8PathBuf::from("."),
        }
    }

    fn replies(input: &str) -> Vec<serde_json::Value> {
        replies_for(input, &project())
    }

    fn replies_for(input: &str, project: &ProjectAssets) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        serve(input.as_bytes(), &mut out, project).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// A real TrueType font covering the ASCII letters and a Greek one.
    ///
    /// Built here rather than checked in for the reason `uf_assets` gives:
    /// a binary fixture is a blob nobody can read a diff of.
    fn outline_font() -> Vec<u8> {
        let mut characters: Vec<char> = ('A'..='Z').chain('a'..='z').collect();
        characters.extend([' ', '.', ',', '-', 'Ω', 'Д']);
        uf_assets::test_font(&characters)
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

    #[test]
    fn a_font_import_can_ask_for_the_unicode_range_split() {
        let (_guard, dir) = temp();
        let source = dir.join("Wide.ttf");
        std::fs::write(&source, outline_font()).unwrap();
        let out = dir.join("out");

        let request = serde_json::json!({
            "kind": "font", "id": source, "outDir": out,
            "family": "Wide", "baseUrl": "/assets/", "subset": "ranges",
        });
        let replies = replies(&format!("{request}\n"));

        let font = &replies[0]["font"];
        assert!(replies[0]["error"].is_null(), "{}", replies[0]);
        assert_eq!(font["subset"], "ranges");
        let faces = font["faces"].as_array().unwrap();
        assert!(faces.len() >= 2, "{faces:?}");
        let mut preloaded = 0;
        for face in faces {
            let file = face["file"].as_str().unwrap();
            assert!(out.join(file).exists(), "{file} was named but not written");
            assert!(face["unicodeRange"].as_str().is_some(), "{face}");
            if face["preload"] == serde_json::Value::Bool(true) {
                preloaded += 1;
            }
        }
        assert_eq!(preloaded, 1, "exactly one face is preloaded");
        assert!(font["css"].as_str().unwrap().contains("unicode-range:"));
    }

    #[test]
    fn a_font_import_can_name_the_characters_it_needs() {
        let (_guard, dir) = temp();
        let source = dir.join("Wide.ttf");
        std::fs::write(&source, outline_font()).unwrap();
        let out = dir.join("out");

        let request = serde_json::json!({
            "kind": "font", "id": source, "outDir": out,
            "family": "Wide", "text": "ABC", "subset": "ranges",
        });
        let replies = replies(&format!("{request}\n"));

        // `text` wins over `subset`: naming the characters is the stronger
        // statement, and an import that did both meant the characters.
        assert_eq!(replies[0]["font"]["subset"], "text");
        let faces = replies[0]["font"]["faces"].as_array().unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0]["unicodeRange"], "U+41-43");
    }

    #[test]
    fn a_subset_uf_cannot_produce_leaves_a_working_font_and_a_reason() {
        // A CFF font: `skera` does not rebuild `CFF `, so a subset of one
        // would come back with metrics and no outlines. The whole font is
        // hosted and the import is told why.
        let (_guard, dir) = temp();
        let source = dir.join("Face.otf");
        std::fs::write(&source, cff_font()).unwrap();
        let out = dir.join("out");

        let request = serde_json::json!({
            "kind": "font", "id": source, "outDir": out, "subset": "ranges",
        });
        let replies = replies(&format!("{request}\n"));

        assert!(replies[0]["error"].is_null(), "{}", replies[0]);
        assert!(replies[0]["font"]["subset"].is_null());
        let reason = replies[0]["font"]["subsetDeclined"].as_str().unwrap();
        assert!(reason.contains("CFF"), "{reason}");
        assert!(
            out.join(replies[0]["font"]["file"].as_str().unwrap())
                .exists()
        );
        assert!(
            replies[0]["font"]["css"]
                .as_str()
                .unwrap()
                .contains("@font-face")
        );
    }

    #[test]
    fn an_unknown_subset_mode_is_a_message_rather_than_a_silent_no() {
        let (_guard, dir) = temp();
        let source = dir.join("Inter.ttf");
        std::fs::write(&source, font_fixture()).unwrap();
        let request = serde_json::json!({
            "kind": "font", "id": source, "outDir": dir.join("out"), "subset": "everything",
        });
        let replies = replies(&format!("{request}\n"));
        let error = replies[0]["error"].as_str().unwrap();
        assert!(error.contains("not a subset mode"), "{error}");
    }

    #[test]
    fn an_icon_becomes_a_symbol_and_the_sprite_holds_the_set_that_was_reached() {
        // The part a runtime icon library cannot do: the sprite is exactly the
        // icons this build asked for, and no others.
        let (_guard, dir) = temp();
        let out = dir.join("out");
        for (name, body) in [
            (
                "star.svg",
                r#"<svg viewBox="0 0 24 24"><path d="M1 1h2v2z"/></svg>"#,
            ),
            (
                "dot.svg",
                r#"<svg viewBox="0 0 8 8"><circle cx="4" cy="4" r="3"/></svg>"#,
            ),
            (
                "unused.svg",
                r#"<svg viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"#,
            ),
        ] {
            std::fs::write(dir.join(name), body).unwrap();
        }

        let mut input = String::new();
        for name in ["star", "dot"] {
            let request = serde_json::json!({
                "kind": "icon", "id": dir.join(format!("{name}.svg")), "outDir": out, "name": name,
            });
            input.push_str(&format!("{request}\n"));
        }
        let asked = replies(&input);
        // The caller carries the set back, which is what makes every request
        // answerable from itself — see the module documentation.
        let sprite = serde_json::json!({
            "kind": "sprite", "id": "uf:icon-sprite", "outDir": out,
            "icons": [asked[0]["icon"].clone(), asked[1]["icon"].clone()],
        });
        input.push_str(&format!("{sprite}\n"));
        let replies = replies(&input);

        assert_eq!(replies[0]["icon"]["viewBox"], "0 0 24 24");
        assert_eq!(replies[1]["icon"]["viewBox"], "0 0 8 8");
        let built = &replies[2]["sprite"];
        assert_eq!(built["symbols"], 2, "{built}");
        let markup = built["markup"].as_str().unwrap();
        assert!(markup.contains(replies[0]["icon"]["id"].as_str().unwrap()));
        assert!(markup.contains(replies[1]["icon"]["id"].as_str().unwrap()));
        assert!(!markup.contains("unused"), "{markup}");
        assert!(out.join(built["file"].as_str().unwrap()).exists());
    }

    #[test]
    fn a_sprite_with_no_icons_is_an_empty_sprite_rather_than_an_error() {
        let (_guard, dir) = temp();
        let request = serde_json::json!({ "kind": "sprite", "id": "uf:icon-sprite", "outDir": dir.join("out") });
        let replies = replies(&format!("{request}\n"));
        assert!(replies[0]["error"].is_null(), "{}", replies[0]);
        assert_eq!(replies[0]["sprite"]["symbols"], 0);
    }

    #[test]
    fn an_open_graph_template_is_drawn_from_the_projects_font() {
        // The project names the typeface once; the template carries text.
        let (_guard, dir) = temp();
        std::fs::write(dir.join("Brand.ttf"), outline_font()).unwrap();
        let source = dir.join("card.og.json");
        std::fs::write(
            &source,
            serde_json::json!({ "title": "Hello there", "subtitle": "a card" }).to_string(),
        )
        .unwrap();
        let out = dir.join("out");

        let mut project = project();
        project.root = dir.clone();
        project.og.font = Some(String::from("Brand.ttf"));
        let request = serde_json::json!({ "kind": "og", "id": source, "outDir": out });
        let replies = replies_for(&format!("{request}\n"), &project);

        assert!(replies[0]["error"].is_null(), "{}", replies[0]);
        let og = &replies[0]["og"];
        assert_eq!(og["width"], 1200);
        assert_eq!(og["height"], 630);
        assert_eq!(og["alt"], "Hello there");
        assert!(out.join(og["file"].as_str().unwrap()).exists());
    }

    #[test]
    fn a_card_uf_cannot_lay_out_is_one_error_naming_the_character() {
        let (_guard, dir) = temp();
        std::fs::write(dir.join("Brand.ttf"), outline_font()).unwrap();
        let source = dir.join("card.og.json");
        std::fs::write(&source, serde_json::json!({ "title": "مرحبا" }).to_string()).unwrap();

        let mut project = project();
        project.root = dir.clone();
        project.og.font = Some(String::from("Brand.ttf"));
        let request = serde_json::json!({ "kind": "og", "id": source, "outDir": dir.join("out") });
        let replies = replies_for(&format!("{request}\n"), &project);

        let error = replies[0]["error"].as_str().unwrap();
        assert!(error.contains("right-to-left"), "{error}");
    }

    #[test]
    fn the_second_request_for_the_same_asset_is_answered_from_the_cache() {
        // The whole of the warm-build story: same reply, no work.
        let (_guard, dir) = temp();
        let source = dir.join("hero.png");
        graphic(&source, 200, 100);
        let out = dir.join("out");
        let request =
            serde_json::json!({ "kind": "image", "id": source, "outDir": out, "widths": [100] });

        let replies = replies(&format!("{request}\n{request}\n"));
        assert_eq!(replies[0]["cached"], false);
        assert_eq!(replies[1]["cached"], true);
        assert_eq!(replies[0]["image"], replies[1]["image"]);
    }

    #[test]
    fn changing_a_parameter_invalidates_the_cache() {
        // There is no invalidation step because the key is the whole of it.
        let (_guard, dir) = temp();
        let source = dir.join("hero.png");
        graphic(&source, 200, 100);
        let out = dir.join("out");
        let first =
            serde_json::json!({ "kind": "image", "id": source, "outDir": out, "widths": [100] });
        let second =
            serde_json::json!({ "kind": "image", "id": source, "outDir": out, "widths": [50] });

        let replies = replies(&format!("{first}\n{second}\n{first}\n"));
        assert_eq!(replies[0]["cached"], false);
        assert_eq!(replies[1]["cached"], false, "a new width must not hit");
        assert_eq!(replies[2]["cached"], true);
    }

    #[test]
    fn editing_the_source_invalidates_the_cache() {
        let (_guard, dir) = temp();
        let source = dir.join("hero.png");
        graphic(&source, 200, 100);
        let out = dir.join("out");
        let request =
            serde_json::json!({ "kind": "image", "id": source, "outDir": out, "widths": [100] });

        assert_eq!(replies(&format!("{request}\n"))[0]["cached"], false);
        assert_eq!(replies(&format!("{request}\n"))[0]["cached"], true);
        graphic(&source, 200, 120);
        assert_eq!(replies(&format!("{request}\n"))[0]["cached"], false);
    }

    /// A font whose outlines are declared to be in `CFF `.
    ///
    /// `OTTO` is what says so, and it is the whole of what the refusal reads.
    fn cff_font() -> Vec<u8> {
        let mut bytes = font_fixture();
        bytes[0..4].copy_from_slice(b"OTTO");
        bytes
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
