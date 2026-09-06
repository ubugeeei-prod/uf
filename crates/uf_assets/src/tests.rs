//! The whole point of the crate, end to end.
//!
//! The tests in each module are about that module's decisions. These are about
//! the property the rest of uf depends on: **a build and a dev server that ask
//! for the same thing get the same files**, byte for byte and name for name.
//! Everything else in the feature — the `srcset` a component writes, the URL a
//! middleware serves — is downstream of that being true.

use camino::Utf8PathBuf;

use crate::config::{FontsConfig, ImagesConfig};
use crate::font::{FontRequest, self_host};
use crate::image::{Emitted, ImageRequest, transform};

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

fn checkerboard(path: &camino::Utf8Path, width: u32, height: u32) {
    let mut buffer = image::RgbaImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let on = ((x / 32) + (y / 32)) % 2 == 0;
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
fn a_build_and_a_dev_server_emit_the_same_files() {
    // Two runs into two directories, standing in for `uf build` writing into
    // `dist/` and `uf dev` writing into its cache. The names are content
    // hashes and the bytes are a pure function of the source and the
    // parameters, so the two directories have to agree completely — which is
    // what lets a dev server serve what a build would have written, and lets a
    // build reuse what a dev server already made.
    let (_guard, dir) = temp();
    let source = dir.join("hero.png");
    checkerboard(&source, 800, 600);

    let run = |out: &camino::Utf8Path| {
        transform(&ImageRequest {
            source: &source,
            widths: &[200, 400],
            quality: 75,
            blur: true,
            out_dir: out,
        })
        .unwrap()
    };

    let build = run(&dir.join("dist"));
    let dev = run(&dir.join("cache"));

    assert_eq!(build, dev);
    for variant in &build.variants {
        assert_eq!(
            std::fs::read(dir.join("dist").join(&variant.file)).unwrap(),
            std::fs::read(dir.join("cache").join(&variant.file)).unwrap(),
            "{} differs between the two runs",
            variant.file
        );
    }
}

#[test]
fn the_defaults_produce_a_ladder_a_layout_can_choose_from() {
    // The out-of-the-box experience, since almost every project takes it: a
    // source wider than the whole ladder, with no configuration at all, has to
    // come out as several widths rather than one.
    //
    // Two megapixels rather than the six a real photograph has: every rung is
    // encoded twice and this file's assertions are about *which* rungs exist,
    // so a larger source would buy nothing and cost this suite a minute in a
    // debug build.
    let (_guard, dir) = temp();
    let source = dir.join("wide.png");
    checkerboard(&source, 2000, 1000);
    let out = dir.join("out");
    let config = ImagesConfig::default();

    let asset = transform(&ImageRequest {
        source: &source,
        widths: &config.widths,
        quality: config.quality,
        blur: config.placeholder,
        out_dir: &out,
    })
    .unwrap();

    let widths: Vec<u32> = asset
        .in_format(Emitted::Png)
        .iter()
        .map(|variant| variant.width)
        .collect();
    assert_eq!(widths, vec![640, 750, 828, 1080, 1200, 1440, 1920, 2000]);
    assert!(asset.blur.is_some());

    // And every one of them is on disk at the size it says.
    for variant in &asset.variants {
        let bytes = std::fs::read(out.join(&variant.file)).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(decoded.width(), variant.width, "{}", variant.file);
    }
}

#[test]
fn a_font_and_an_image_can_share_one_output_directory() {
    // They do, in `dist/assets`. Two different name spaces sharing a directory
    // is where a collision would show up.
    let (_guard, dir) = temp();
    let out = dir.join("out");

    let image_source = dir.join("mark.png");
    checkerboard(&image_source, 64, 64);
    let asset = transform(&ImageRequest {
        source: &image_source,
        widths: &[32],
        quality: 75,
        blur: false,
        out_dir: &out,
    })
    .unwrap();

    let font_source = dir.join("Face.ttf");
    std::fs::write(&font_source, super::font::tests::sfnt_fixture()).unwrap();
    let fonts = FontsConfig::default();
    let font = self_host(&FontRequest {
        source: &font_source,
        family: "Face",
        weight: "400",
        style: "normal",
        display: &fonts.display,
        fallback: Some(&fonts.fallback),
        out_dir: &out,
        base_url: "/assets/",
    })
    .unwrap();

    let mut names: Vec<String> = asset
        .variants
        .iter()
        .map(|variant| variant.file.clone())
        .collect();
    names.push(font.file.clone());
    let unique: std::collections::BTreeSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "{names:?}");
    for name in &names {
        assert!(out.join(name).exists(), "{name} was not written");
    }
}
