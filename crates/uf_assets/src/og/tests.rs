use camino::{Utf8Path, Utf8PathBuf};

use super::{Background, OgError, OgRequest, OgTemplate, draw, unsupported_reason};
use crate::testfont::{font_with, font_with_blanks};

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

/// A directory holding a font and a template that names it.
fn project(template: serde_json::Value) -> (tempfile::TempDir, Utf8PathBuf, Utf8PathBuf) {
    let (guard, dir) = temp();
    std::fs::write(
        dir.join("Test.ttf"),
        font_with(
            &"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz .,-"
                .chars()
                .collect::<Vec<_>>(),
        ),
    )
    .unwrap();
    let source = dir.join("card.og.json");
    std::fs::write(&source, serde_json::to_vec_pretty(&template).unwrap()).unwrap();
    let out = dir.join("out");
    (guard, source, out)
}

fn basic() -> serde_json::Value {
    serde_json::json!({
        "title": "Images and fonts",
        "subtitle": "Resized and self hosted at build time",
        "eyebrow": "Writing code",
        "accent": "#7c8cff",
        "font": "Test.ttf",
    })
}

#[test]
fn a_template_becomes_a_png_of_the_declared_size() {
    let (_guard, source, out) = project(basic());
    let asset = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .expect("the card should draw");

    assert_eq!(asset.width, 1200);
    assert_eq!(asset.height, 630);
    assert_eq!(asset.mime, "image/png");
    assert_eq!(asset.alt, "Images and fonts");

    let written = out.join(&asset.file);
    assert_eq!(std::fs::metadata(&written).unwrap().len(), asset.bytes);
    let decoded = image::open(written.as_std_path()).expect("uf wrote something that is not a PNG");
    assert_eq!(decoded.width(), 1200);
    assert_eq!(decoded.height(), 630);
}

#[test]
fn the_text_is_actually_drawn() {
    // The difference between a card and a rectangle. The background is one
    // colour, so any pixel that is not it is ink — and a renderer that placed
    // no glyphs would produce a canvas of one colour.
    let mut template = basic();
    template["background"] = serde_json::json!("#000000");
    template["foreground"] = serde_json::json!("#ffffff");
    let (_guard, source, out) = project(template);
    let asset = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap();

    let decoded = image::open(out.join(&asset.file).as_std_path())
        .unwrap()
        .to_rgba8();
    let ink = decoded.pixels().filter(|pixel| pixel.0[0] > 128).count();
    assert!(ink > 1000, "only {ink} light pixels: nothing was drawn");
}

#[test]
fn the_same_template_and_font_produce_the_same_name() {
    // A content hash that moved between builds would invalidate every cached
    // card on every deploy.
    let (_guard, source, out) = project(basic());
    let first = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap();
    let second = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap();
    assert_eq!(first.file, second.file);
}

#[test]
fn a_different_font_is_a_different_card_and_a_different_name() {
    let (_guard, source, out) = project(basic());
    let first = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap();

    let directory = source.parent().unwrap();
    std::fs::write(
        directory.join("Test.ttf"),
        font_with(
            &"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz .,-!"
                .chars()
                .collect::<Vec<_>>(),
        ),
    )
    .unwrap();
    let second = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap();
    assert_ne!(first.file, second.file, "the font is not in the digest");
}

#[test]
fn a_gradient_background_is_a_gradient() {
    let mut template = basic();
    template["background"] = serde_json::json!({ "from": "#000000", "to": "#ffffff" });
    let (_guard, source, out) = project(template);
    let asset = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap();

    let decoded = image::open(out.join(&asset.file).as_std_path())
        .unwrap()
        .to_rgba8();
    // The corners, away from any text.
    assert!(decoded.get_pixel(4, 2).0[0] < 16, "the top is not dark");
    assert!(
        decoded.get_pixel(4, 627).0[0] > 240,
        "the bottom is not light"
    );
}

#[test]
fn a_template_with_no_font_is_refused_because_uf_ships_none() {
    let mut template = basic();
    template.as_object_mut().unwrap().remove("font");
    let (_guard, source, out) = project(template);
    let error = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap_err();
    assert!(matches!(error, OgError::NoFont { .. }), "{error}");
    assert!(error.to_string().contains("ships no typeface"), "{error}");
}

#[test]
fn arabic_is_refused_rather_than_laid_out_left_to_right() {
    // The rejection this whole module exists for. Placing these code points at
    // their advance widths produces disconnected, mirrored nonsense that looks
    // like text to anybody who does not read the script.
    let mut template = basic();
    template["title"] = serde_json::json!("مرحبا");
    let (_guard, source, out) = project(template);
    let error = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap_err();

    let message = error.to_string();
    assert!(
        matches!(error, OgError::Unsupported { field: "title", .. }),
        "{message}"
    );
    assert!(message.contains("right-to-left"), "{message}");
    assert!(message.contains("import the PNG"), "{message}");
    // And nothing was written: a refusal that still emitted a file would leave
    // a broken card in the output directory.
    assert!(!out.exists() || std::fs::read_dir(out.as_std_path()).unwrap().count() == 0);
}

#[test]
fn every_script_uf_cannot_shape_is_named_by_the_predicate() {
    for (character, expected) in [
        ('\u{0645}', "right-to-left"),           // Arabic meem
        ('\u{05D0}', "right-to-left"),           // Hebrew alef
        ('\u{0915}', "Brahmic"),                 // Devanagari ka
        ('\u{0E01}', "does not separate words"), // Thai ko kai
        ('\u{0301}', "combining mark"),          // combining acute
        ('\u{200D}', "joins the characters"),    // zero-width joiner
        ('\u{FE0F}', "joins the characters"),    // emoji variation selector
        ('\t', "control character"),
        ('\n', "control character"),
    ] {
        let reason = unsupported_reason(character)
            .unwrap_or_else(|| panic!("{character:?} should be refused"));
        assert!(reason.contains(expected), "{character:?}: {reason}");
    }
    // And the ones it must not refuse, or the feature draws nothing. The
    // symbols are here on purpose: whether a check mark or an arrow can be
    // drawn is a question about the font, which the font is asked, and not
    // about the block it happens to share with the emoji.
    for character in ['A', 'z', '0', ' ', '—', 'Ω', 'Д', '中', 'é', '✓', '→', '★'] {
        assert!(
            unsupported_reason(character).is_none(),
            "{character:?} was refused"
        );
    }
}

#[test]
fn a_glyph_the_font_has_no_outline_for_is_refused_rather_than_left_as_a_gap() {
    // A colour or bitmap glyph — COLR, sbix, CBDT — has metrics an outline
    // rasteriser will happily advance past and nothing to draw, so the card
    // comes out with a hole the exact width of the character and looks
    // deliberate. An empty glyph is what that looks like to `ab_glyph`.
    let (_guard, dir) = temp();
    let covered: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz .,-★"
        .chars()
        .collect();
    std::fs::write(
        dir.join("Test.ttf"),
        font_with_blanks(&covered, &['\u{2605}']),
    )
    .unwrap();
    let source = dir.join("card.og.json");
    std::fs::write(
        &source,
        serde_json::to_vec(&serde_json::json!({ "title": "Rated \u{2605}", "font": "Test.ttf" }))
            .unwrap(),
    )
    .unwrap();

    let error = draw(&OgRequest {
        source: &source,
        out_dir: &dir.join("out"),
    })
    .unwrap_err();

    assert!(error.to_string().contains("colour or bitmap"), "{error}");
    // And the same font draws the characters it does have outlines for.
    std::fs::write(
        &source,
        serde_json::to_vec(&serde_json::json!({ "title": "Rated", "font": "Test.ttf" })).unwrap(),
    )
    .unwrap();
    draw(&OgRequest {
        source: &source,
        out_dir: &dir.join("out"),
    })
    .expect("the rest of the font is fine");
}

#[test]
fn a_character_the_font_has_no_glyph_for_is_refused() {
    // Drawing `.notdef` gives a row of empty boxes, which reads as a bug in
    // whatever is showing the card rather than as a missing glyph.
    let mut template = basic();
    template["title"] = serde_json::json!("Hello 中");
    let (_guard, source, out) = project(template);
    let error = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap_err();
    assert!(error.to_string().contains("no glyph for it"), "{error}");
}

#[test]
fn a_title_that_cannot_fit_is_refused_rather_than_clipped() {
    let mut template = basic();
    template["title"] = serde_json::json!(
        "a very long headline that goes on and on and cannot possibly fit inside three lines of \
         a card at any size this renderer is willing to shrink to and so has to be refused"
    );
    template["titleLines"] = serde_json::json!(2);
    let (_guard, source, out) = project(template);
    let error = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap_err();
    assert!(
        matches!(error, OgError::Overflow { field: "title", .. }),
        "{error}"
    );
}

#[test]
fn a_card_larger_than_the_limit_is_refused_before_it_is_allocated() {
    let mut template = basic();
    template["width"] = serde_json::json!(60_000);
    template["height"] = serde_json::json!(60_000);
    let (_guard, source, out) = project(template);
    let error = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap_err();
    assert!(matches!(error, OgError::TooLarge { .. }), "{error}");
}

#[test]
fn an_unknown_field_is_a_message_rather_than_a_silently_ignored_setting() {
    let mut template = basic();
    template["backgroundColour"] = serde_json::json!("#fff");
    let (_guard, source, out) = project(template);
    let error = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap_err();
    assert!(matches!(error, OgError::Malformed { .. }), "{error}");
    assert!(error.to_string().contains("backgroundColour"), "{error}");
}

#[test]
fn a_woff2_font_is_refused_with_the_file_to_use_instead() {
    let mut template = basic();
    template["font"] = serde_json::json!("Test.woff2");
    let (_guard, source, out) = project(template);
    std::fs::write(source.parent().unwrap().join("Test.woff2"), b"wOF2rest").unwrap();
    let error = draw(&OgRequest {
        source: &source,
        out_dir: &out,
    })
    .unwrap_err();
    assert!(error.to_string().contains(".ttf"), "{error}");
}

#[test]
fn the_defaults_are_the_open_graph_card_every_consumer_crops_to() {
    let template: OgTemplate = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
    assert_eq!(template.width, super::DEFAULT_WIDTH);
    assert_eq!(template.height, super::DEFAULT_HEIGHT);
    assert_eq!(
        template.background,
        Background::Solid(String::from("#0b1020"))
    );
    assert!(template.font.is_none());
    assert!(
        Utf8Path::new("card.og.json")
            .as_str()
            .ends_with(super::OG_EXTENSION)
    );
}
