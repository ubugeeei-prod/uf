//! What `uf build` emits for an imported image and an imported font.
//!
//! The unit tests in `crates/uf_assets` prove the pipeline resizes and
//! re-encodes; `tests/library/assets.test.js` proves the components write the
//! right attributes from a manifest. Neither proves the two halves are wired
//! to each other, and that is the whole of this file: a real project, a real
//! `uf build`, and assertions that go to the bytes in `dist/` and the string in
//! the prerendered document.
//!
//! Every assertion here is on an artefact:
//!
//! * the variant files exist, and decoding one gives the width its name claims;
//! * the `srcset` in the HTML names those files and no others;
//! * the `<link rel="preload">` for the priority image asks for the same
//!   variant the `<img>` will;
//! * the font is in `dist/` under a hashed name and the `@font-face` in the
//!   document points at it and carries the metric overrides;
//! * `uf-bundle-report.json` counts them, which is what proves they went
//!   through the bundler rather than being copied past it.
//!
//! Separate from `vite.rs` for the reason `vite_styles.rs` is: the fixture is
//! its own, it is a project written to exercise one feature, and it needs
//! binary files that no other test wants.

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::{Project, uf};

/// Whether a Vite build can run here: Node on PATH and the workspace
/// installed.
///
/// A missing fixture is a failure, not a skip, for the reason `vite.rs` gives:
/// cargo hides a passing test's output, so a suite that quietly stopped
/// building anything still prints "ok". `UF_ALLOW_FIXTURE_SKIP=1` opts out on
/// a machine that genuinely cannot; CI sets nothing and so can never skip.
fn fixture_ready() -> bool {
    let mut missing = Vec::new();
    if !Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        missing.push("`node` is not on PATH".to_owned());
    }
    let driver = support::repo_root().join("node_modules/@uniflowed/vite/driver.js");
    if !driver.is_file() {
        missing.push(format!("{} does not exist; run `npm ci`", driver.display()));
    }

    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "the JavaScript host is not available, so this test would prove nothing: {}",
        missing.join("; ")
    );
    eprintln!("skipping: {}", missing.join("; "));
    false
}

/// A smoothly varying PNG, which is the case where a lossless WebP wins on
/// size at every width — so this fixture exercises the `<picture>` path.
///
/// Written by the test rather than checked in: a binary fixture is a blob
/// nobody can read a diff of, and every property this file asserts on is a
/// consequence of the two numbers below. Which *shape* it is matters, and
/// `crates/uf_assets/src/image/tests.rs` carries the measurements that say
/// why: a hard-edged graphic would be declined at the resampled widths and
/// there would be no `<source>` to assert on.
fn write_png(path: &Path, width: u32, height: u32) {
    let mut buffer = image::RgbaImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let red = (u64::from(x) * 255 / u64::from(width.max(1))) as u8;
        let green = (u64::from(y) * 255 / u64::from(height.max(1))) as u8;
        *pixel = image::Rgba([red, green, 128, 255]);
    }
    image::DynamicImage::ImageRgba8(buffer)
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}

/// A hard-edged PNG, which is the case where the lossless WebP loses.
///
/// The shape of a UI screenshot, and it behaves like one: resampling turns
/// every edge into a ring of near-colours, PNG's adaptive filtering absorbs
/// those and this build's WebP encoder does not, so the WebP comes out several
/// times larger at the narrow rungs and the whole ladder is declined. Measured,
/// not assumed — the numbers are in `crates/uf_assets/src/image.rs`.
fn write_hard_edged_png(path: &Path, width: u32, height: u32) {
    let mut buffer = image::RgbaImage::new(width, height);
    for (x, y, pixel) in buffer.enumerate_pixels_mut() {
        let block = ((x / 40) + (y / 40)) % 3;
        *pixel = match block {
            0 => image::Rgba([12, 74, 156, 255]),
            1 => image::Rgba([243, 244, 246, 255]),
            _ => image::Rgba([220, 38, 38, 255]),
        };
    }
    image::DynamicImage::ImageRgba8(buffer)
        .save_with_format(path, image::ImageFormat::Png)
        .unwrap();
}

/// A minimal but real SFNT carrying the three tables the pipeline reads.
fn write_font(path: &Path) {
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    let mut hhea = vec![0u8; 36];
    hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
    hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    hhea[8..10].copy_from_slice(&0i16.to_be_bytes());
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
    fs::write(path, out).unwrap();
}

/// A project whose one page imports an image and a font.
fn asset_app() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "uf.config.js",
            r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
    builtins: {
      // A short ladder, so the assertions can name every file the build is
      // expected to write rather than a subset of a long one.
      images: { widths: [200, 400], quality: 70 },
      fonts: { display: "swap", fallback: "Arial" },
    },
  },
  build: { entries: ["app.js"], outDir: "dist" },
});
"#,
        ),
        (
            "app.js",
            r#"// @flow
import { routerView } from "@uniflowed/router";

export default routerView("./app");
"#,
        ),
        (
            "app/_uf.layout.js",
            r#"// @flow
import * as React from "@uniflowed/react";

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
"#,
        ),
        (
            "app/_uf.page.js",
            r#"// @flow
import * as React from "@uniflowed/react";
import { Font, Image } from "@uniflowed/web";

import hero from "../hero.png";
import face from "../Fixture.ttf";

export component Page() {
  return (
    <main style={{ fontFamily: face.fontFamily }}>
      <Font src={face} />
      <Image src={hero} alt="a hero" priority={true} sizes="(max-width: 480px) 100vw, 480px" />
    </main>
  );
}
"#,
        ),
    ]
}

/// Build the project and return its stdout, insisting that it succeeded.
fn build(root: &Path) -> String {
    let output = uf().arg("--cwd").arg(root).arg("build").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "the build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// Every file name inside one `srcset` attribute of `html`.
///
/// The attribute is found case-insensitively, and deliberately: React
/// serialises the prop as `srcSet` and the preload's as `imageSrcSet`, which
/// an HTML parser lowercases and a `str::find` does not. A case-sensitive
/// search here would have found nothing and reported it as a missing
/// `srcset` — a test failing for a reason that is not the one it is about.
fn srcset_files(html: &str, after: &str) -> Vec<String> {
    let lowered = html.to_lowercase();
    let at = lowered
        .find(&after.to_lowercase())
        .unwrap_or_else(|| panic!("no {after} in:\n{html}"));
    let rest = &html[at..];
    let lowered_rest = &lowered[at..];
    let start = lowered_rest
        .find("srcset=\"")
        .unwrap_or_else(|| panic!("no srcset after {after} in:\n{html}"))
        + "srcset=\"".len();
    let end = start
        + rest[start..]
            .find('"')
            .unwrap_or_else(|| panic!("unterminated srcset in:\n{html}"));
    rest[start..end]
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let url = entry.split_whitespace().next().unwrap_or_default();
            url.rsplit('/').next().unwrap_or(url).to_owned()
        })
        .collect()
}

#[test]
fn a_build_emits_the_widths_the_srcset_promises() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    write_png(&project.path().join("hero.png"), 400, 300);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());

    let dist = project.path().join("dist");
    let html = fs::read_to_string(dist.join("index.html")).expect("the page is prerendered");

    // The `srcset` names files, and every one of them is on disk at the width
    // its descriptor claims. This is the assertion the whole feature is for: a
    // component that merely passed a `srcSet` prop through would produce a
    // string exactly like this one and no files at all.
    let files = srcset_files(&html, "<img");
    assert!(!files.is_empty(), "no srcset in:\n{html}");
    for name in &files {
        let path = dist.join("assets").join(name);
        let bytes = fs::read(&path).unwrap_or_else(|_| {
            panic!("the srcset names {name}, which the build did not write:\n{html}")
        });
        let declared: u32 = name
            .rsplit_once("-")
            .and_then(|(_, tail)| tail.split('w').next().map(str::to_owned))
            .and_then(|width| width.parse().ok())
            .unwrap_or_else(|| panic!("{name} does not carry its width"));
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!(
            decoded.width(),
            declared,
            "{name} is {} pixels wide but its srcset entry says {declared}",
            decoded.width()
        );
    }

    // The declared ladder, and the source's own width so there is a largest.
    let widths: Vec<u32> = files
        .iter()
        .filter(|name| name.ends_with(".png"))
        .filter_map(|name| {
            name.rsplit_once('-')
                .and_then(|(_, tail)| tail.split('w').next()?.parse().ok())
        })
        .collect();
    assert_eq!(widths, vec![200, 400], "{files:?}");
}

#[test]
fn the_intrinsic_size_comes_from_the_file_and_reaches_the_markup() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    write_png(&project.path().join("hero.png"), 400, 300);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let html = fs::read_to_string(project.path().join("dist/index.html"))
        .expect("the page is prerendered");

    // Nobody typed these. The pipeline decoded the file and the component used
    // what it found, which is what stops the page reflowing when the image
    // lands.
    assert!(html.contains(r#"width="400""#), "{html}");
    assert!(html.contains(r#"height="300""#), "{html}");
    assert!(html.contains(r#"decoding="async""#), "{html}");
}

#[test]
fn a_priority_image_is_preloaded_and_asks_for_the_same_variant() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    write_png(&project.path().join("hero.png"), 400, 300);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let html = fs::read_to_string(project.path().join("dist/index.html"))
        .expect("the page is prerendered");

    assert!(html.contains(r#"rel="preload""#), "no preload in:\n{html}");
    assert!(html.contains(r#"as="image""#), "{html}");
    assert!(html.contains(r#"loading="eager""#), "{html}");

    // A preload that asks for a different variant than the `<img>` costs the
    // reader both.
    let preloaded = srcset_files(&html, "as=\"image\"");
    let requested = srcset_files(&html, "<img");
    assert_eq!(preloaded, requested, "{html}");
}

#[test]
fn a_modern_format_is_offered_only_when_it_came_out_smaller() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    // Bigger than the other fixtures on purpose. A gradient this size wins on
    // every rung of the declared ladder, so the `<picture>` branch is the one
    // that runs; the same gradient at 400x300 loses at the narrowest rung by
    // eighty bytes of container overhead and is declined outright, which is
    // the other branch and is asserted below just as hard.
    write_png(&project.path().join("hero.png"), 800, 600);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let dist = project.path().join("dist");
    let html = fs::read_to_string(dist.join("index.html")).expect("the page is prerendered");

    assert!(
        html.contains(r#"type="image/webp""#),
        "a gradient wins at every width and should have got a webp source:\n{html}"
    );

    let fallbacks = srcset_files(&html, "<img");
    let alternatives = srcset_files(&html, "<source");

    // All the rungs. A `<picture>` source carrying only the wide ones sends a
    // phone a wide one, because a browser that takes a source then chooses a
    // width from that source's `srcset` alone.
    assert_eq!(
        alternatives.len(),
        fallbacks.len(),
        "a partial ladder: {alternatives:?} beside {fallbacks:?}"
    );

    // And each is really smaller than the fallback at the same width. Offering
    // a bigger file in a newer format is a pessimisation dressed as an
    // optimisation, and it is the one thing the format name would hide.
    for name in &alternatives {
        let webp = fs::metadata(dist.join("assets").join(name)).unwrap().len();
        let width = name
            .rsplit_once('-')
            .and_then(|(_, tail)| tail.split('w').next()?.parse::<u32>().ok())
            .unwrap();
        let png = fallbacks
            .iter()
            .find(|other| other.contains(&format!("-{width}w.png")))
            .map(|other| fs::metadata(dist.join("assets").join(other)).unwrap().len())
            .expect("a fallback at the same width");
        assert!(
            webp < png,
            "{name} is {webp} bytes beside a {png} byte png; it should not have been emitted"
        );
    }
}

#[test]
fn a_format_that_loses_at_any_width_leaves_no_file_behind() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    // Hard edges, which lose badly once resampled. The whole ladder is then
    // declined — and the point of this test is that nothing is left in `dist/`
    // for it: a format that was made and rejected must not also be shipped.
    write_hard_edged_png(&project.path().join("hero.png"), 800, 600);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let dist = project.path().join("dist");
    let html = fs::read_to_string(dist.join("index.html")).expect("the page is prerendered");

    assert!(
        !html.contains(r#"type="image/webp""#),
        "this fixture's webp loses at the resampled rungs:\n{html}"
    );
    assert!(
        html.contains("<img"),
        "the fallback still has to render:\n{html}"
    );
    let stray: Vec<String> = fs::read_dir(dist.join("assets"))
        .expect("an assets directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".webp"))
        .collect();
    assert!(
        stray.is_empty(),
        "a declined format left files in the build: {stray:?}"
    );
}

#[test]
fn a_font_is_self_hosted_and_declared_with_its_fallback_metrics() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    write_png(&project.path().join("hero.png"), 400, 300);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let dist = project.path().join("dist");
    let html = fs::read_to_string(dist.join("index.html")).expect("the page is prerendered");

    // The `@font-face` is in the document, and the file it names is on disk
    // under a name that is not the one the author wrote — which is what
    // "self-hosted, content-hashed" means when it is true rather than claimed.
    assert!(html.contains("@font-face"), "no @font-face in:\n{html}");
    let emitted: Vec<String> = fs::read_dir(dist.join("assets"))
        .expect("an assets directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".ttf"))
        .collect();
    assert_eq!(emitted.len(), 1, "{emitted:?}");
    assert_ne!(emitted[0], "Fixture.ttf", "the name carries no digest");
    assert!(html.contains(&emitted[0]), "{html}");

    // And the second face, which is the half that stops the swap moving the
    // page. Every one of these four numbers came out of the font's own tables.
    for property in [
        "size-adjust:",
        "ascent-override:",
        "descent-override:",
        "line-gap-override:",
        "Fixture Fallback",
        r#"local(\"Arial\")"#,
    ] {
        assert!(
            html.contains(property) || html.contains(&property.replace(r#"\""#, "\"")),
            "missing {property} in:\n{html}"
        );
    }

    // The preload is still there, and still says `crossorigin` — the thing
    // `Font` existed to get right before it did anything else.
    assert!(html.contains(r#"as="font""#), "{html}");
    assert!(html.contains("crossorigin"), "{html}");
}

#[test]
fn the_emitted_assets_go_through_the_bundler_and_are_counted() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    write_png(&project.path().join("hero.png"), 400, 300);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());

    // `uf_bundle` walks the output directory, so an image copied past the
    // bundler would still be measured. What this actually proves is the other
    // half: the files are in the output directory at all, under the names the
    // manifest promised, and `build.budgets` therefore sees an image's weight.
    //
    // The report itself is *not* in the output directory — it names every
    // asset and its size, which is a description of the application rather
    // than a part of it (ubugeeei-prod/uf#339).
    let report: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(project.path().join(".uf/build/meta/uf-bundle-report.json")).unwrap(),
    )
    .unwrap();
    let paths: Vec<&str> = report["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|asset| asset["path"].as_str().unwrap())
        .collect();

    assert!(
        paths.iter().any(|path| path.ends_with(".png")),
        "the size report counts no image: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with(".ttf")),
        "the size report counts no font: {paths:?}"
    );
}

#[test]
fn a_second_build_of_the_same_source_writes_the_same_names() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&asset_app());
    write_png(&project.path().join("hero.png"), 400, 300);
    write_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let first = fs::read_to_string(project.path().join("dist/index.html")).unwrap();
    build(project.path());
    let second = fs::read_to_string(project.path().join("dist/index.html")).unwrap();

    // The names are content hashes, so a rebuild that changed nothing must not
    // invalidate a single cache entry a reader already has.
    assert_eq!(
        srcset_files(&first, "<img"),
        srcset_files(&second, "<img"),
        "a rebuild renamed the variants"
    );
}

/// A project whose one page imports an icon, the sprite, and a card.
///
/// Separate from [`asset_app`] rather than folded into it: these three go
/// through hooks the image and font path never touches — a virtual module
/// resolved by uf's own scheme, a `generateBundle` rewrite, and a compound
/// extension — and a failure in one of them should name the feature that
/// broke.
fn icon_and_card_app() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "uf.config.js",
            r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
    builtins: { icons: { dir: "icons" }, og: { font: "Fixture.ttf" } },
  },
  build: { entries: ["app.js"], outDir: "dist" },
});
"#,
        ),
        (
            "app.js",
            r#"// @flow
import { routerView } from "@uniflowed/router";

export default routerView("./app");
"#,
        ),
        (
            "app/_uf.layout.js",
            r#"// @flow
import * as React from "@uniflowed/react";

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
"#,
        ),
        (
            "app/_uf.page.js",
            r#"// @flow
import * as React from "@uniflowed/react";
import { Icon, IconSprite, OgImage } from "@uniflowed/web";

import card from "../card.og.json";
import sprite from "uf:icon-sprite";
import star from "uf:icon/star";

export component Page() {
  return (
    <main>
      <OgImage card={card} origin="https://example.com" />
      <IconSprite sprite={sprite} />
      <button type="button"><Icon icon={star} label="Favourite" /></button>
    </main>
  );
}
"#,
        ),
        (
            "icons/star.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="m12 2 3 7h7l-6 4 2 7-6-4-6 4 2-7-6-4h7z"/></svg>"#,
        ),
        (
            "icons/dot.svg",
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><circle cx="4" cy="4" r="3"/></svg>"#,
        ),
        (
            "card.og.json",
            // `r##` and not `r#`: the accent is a hex colour, so the file
            // contains the sequence `"#`, which is what closes an `r#"` raw
            // string.
            r##"{
  "eyebrow": "Testing",
  "title": "A card the build drew",
  "subtitle": "from a template and a font",
  "accent": "#7c8cff"
}
"##,
        ),
    ]
}

/// A real TrueType font with outlines, which a card needs and metrics do not.
fn write_outline_font(path: &Path) {
    let mut characters: Vec<char> = ('A'..='Z').chain('a'..='z').collect();
    characters.extend([' ', '.', ',', '-']);
    fs::write(path, uf_assets::test_font(&characters)).unwrap();
}

#[test]
fn a_build_emits_a_sprite_of_the_icons_it_reached_and_no_others() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&icon_and_card_app());
    write_outline_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let html = fs::read_to_string(project.path().join("dist/index.html"))
        .expect("the page is prerendered");

    // The icon is a `<use>` into the sprite, not another copy of the path.
    let at = html
        .find("<use")
        .unwrap_or_else(|| panic!("no <use> in:\n{html}"));
    // To the end of the tag rather than a fixed number of bytes: a fixed
    // window runs off the end of a document that is shorter than expected,
    // and then the failure is a slice panic instead of the assertion that
    // would have said what was actually missing.
    let tag = html[at..].split_once('>').map_or("", |(open, _)| open);
    assert!(tag.contains("#uf-icon-star-"), "{tag}");
    // The sprite is inline and holds the symbol that `<use>` points at. This
    // is the assertion that catches an empty sprite, which is what a service
    // that accumulated the icons itself produced: the plugin closes it in
    // `buildEnd`, before `generateBundle` asks for the sprite.
    assert!(
        html.contains("<symbol id=\"uf-icon-star-"),
        "the sprite has no symbol in:\n{html}"
    );
    // `dot.svg` exists in the project and nothing imported it. A sprite that
    // held it would be the whole directory, which is what a runtime icon
    // library does and what this exists not to do.
    assert!(
        !html.contains("uf-icon-dot-"),
        "the sprite holds an icon nothing imported"
    );
}

#[test]
fn a_build_draws_the_card_and_points_the_meta_tags_at_it() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&icon_and_card_app());
    write_outline_font(&project.path().join("Fixture.ttf"));

    build(project.path());
    let dist = project.path().join("dist");
    let html = fs::read_to_string(dist.join("index.html")).expect("the page is prerendered");

    let at = html
        .find("og:image\"")
        .unwrap_or_else(|| panic!("no og:image in:\n{html}"));
    let tag = &html[at..at + 200];
    assert!(tag.contains("https://example.com/assets/"), "{tag}");
    assert!(html.contains("og:image:width\" content=\"1200\""), "{html}");
    assert!(html.contains("summary_large_image"), "{html}");

    // And the file the tag names is on disk, at the size the tag claims.
    let name = tag
        .split("content=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .and_then(|url| url.rsplit('/').next())
        .expect("the og:image has no file name");
    let written = dist.join("assets").join(name);
    assert!(written.exists(), "{name} was named but not written");
    assert!(fs::metadata(&written).unwrap().len() > 0);
}

#[test]
fn a_card_uf_cannot_lay_out_fails_the_build_rather_than_drawing_it() {
    // The whole point of the template: a wrong Open Graph card is worse than
    // no card, because nobody looks at one until it is on another website.
    if !fixture_ready() {
        return;
    }
    let mut files = icon_and_card_app();
    for entry in &mut files {
        if entry.0 == "card.og.json" {
            entry.1 = r#"{ "title": "مرحبا بالعالم" }"#;
        }
    }
    let project = Project::new(&files);
    write_outline_font(&project.path().join("Fixture.ttf"));

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .arg("build")
        .output()
        .unwrap();
    assert!(!output.status.success(), "the build should have failed");
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(message.contains("right-to-left"), "{message}");
}
