use camino::Utf8PathBuf;

use super::{SubsetMode, plan, unicode_range};
use crate::testfont::font_with;

/// Latin, Greek and Cyrillic, so a partition has three buckets to find.
fn mixed() -> Vec<u8> {
    font_with(&[
        'A', 'B', 'C', 'a', 'b', 'c', ' ', 'É', 'Ω', 'α', 'β', 'Д', 'д', '中',
    ])
}

#[test]
fn the_synthetic_font_is_a_font_both_upstreams_accept() {
    // Everything below rests on this: if the fixture is not a real font, a
    // green subset test proves nothing.
    let bytes = mixed();
    let read = write_fonts::read::FontRef::new(&bytes).expect("read-fonts rejected the fixture");
    assert!(
        read.table_data(write_fonts::read::types::Tag::new(b"glyf"))
            .is_some()
    );
    let drawable =
        ab_glyph::FontRef::try_from_slice(&bytes).expect("ab_glyph rejected the fixture");
    use ab_glyph::Font as _;
    assert_ne!(drawable.glyph_id('A'), ab_glyph::GlyphId(0));
}

#[test]
fn ranges_split_a_family_into_the_scripts_it_covers() {
    let outcome = plan(&mixed(), &SubsetMode::Ranges).expect("the fixture is subsettable");
    let buckets: Vec<&str> = outcome
        .faces
        .iter()
        .map(|face| face.bucket.as_str())
        .collect();

    assert!(buckets.contains(&"latin"), "{buckets:?}");
    assert!(buckets.contains(&"greek"), "{buckets:?}");
    assert!(buckets.contains(&"cyrillic"), "{buckets:?}");
    // U+4E2D is in none of the named buckets and must not be dropped.
    assert!(buckets.contains(&"rest"), "{buckets:?}");
    assert_eq!(outcome.faces[outcome.primary].bucket, "latin");
}

#[test]
fn every_covered_character_lands_in_exactly_one_bucket() {
    // The property that makes the split lossless. A character in no bucket is
    // a character the page can no longer draw; a character in two is a byte
    // paid for twice.
    let outcome = plan(&mixed(), &SubsetMode::Ranges).unwrap();
    let mut seen: Vec<char> = Vec::new();
    for face in &outcome.faces {
        for part in face.unicode_range.split(',') {
            for code in expand(part) {
                let character = char::from_u32(code).unwrap();
                assert!(
                    !seen.contains(&character),
                    "{character:?} is in two buckets"
                );
                seen.push(character);
            }
        }
    }
    for character in ['A', 'Ω', 'Д', '中', 'É'] {
        assert!(seen.contains(&character), "{character:?} was dropped");
    }
}

#[test]
fn a_subset_face_is_smaller_than_the_font_it_came_out_of() {
    // The whole reason to do any of this. Not a fixed ratio — the fixture is
    // small enough that the header is a real fraction of it — but a latin
    // bucket that is not smaller than a font also carrying CJK is a subsetter
    // that did nothing.
    let whole = mixed();
    let outcome = plan(&whole, &SubsetMode::Ranges).unwrap();
    let latin = outcome
        .faces
        .iter()
        .find(|face| face.bucket == "latin")
        .unwrap();
    assert!(
        (latin.bytes.len() as u64) < whole.len() as u64,
        "latin {} vs whole {}",
        latin.bytes.len(),
        whole.len()
    );
}

#[test]
fn a_text_subset_keeps_the_characters_it_was_given() {
    let outcome = plan(&mixed(), &SubsetMode::Text(String::from("ABC"))).unwrap();
    assert_eq!(outcome.faces.len(), 1);
    assert_eq!(outcome.faces[0].bucket, "text");
    assert_eq!(outcome.faces[0].codepoints, 3);
    assert_eq!(outcome.faces[0].unicode_range, "U+41-43");
}

#[test]
fn a_text_subset_of_characters_the_font_does_not_have_is_refused() {
    // Rather than an empty font, which would render every page in tofu while
    // the build said it succeeded.
    let error = plan(&mixed(), &SubsetMode::Text(String::from("한국어"))).unwrap_err();
    assert!(error.contains("none of the characters"), "{error}");
}

#[test]
fn a_subset_face_is_a_woff_uf_can_read_back() {
    // The container this crate writes is one it already parses, which is what
    // makes "uf emits WOFF 1.0" checkable without a browser.
    let outcome = plan(&mixed(), &SubsetMode::Text(String::from("ABC"))).unwrap();
    let woff = &outcome.faces[0].bytes;
    assert_eq!(&woff[0..4], b"wOFF");
    let path = Utf8PathBuf::from("subset.woff");
    let (container, metrics) = crate::font::read_metrics(&path, woff).expect("uf cannot read it");
    assert_eq!(container, crate::font::FontContainer::Woff);
    assert_eq!(metrics.units_per_em, 1000);
    assert_eq!(metrics.ascent, 800);
}

#[test]
fn a_cff_font_is_refused_rather_than_emptied() {
    // `skera` does not subset `CFF `, so an OTF that went through it would
    // come back with a cmap, metrics, and no outlines at all.
    let mut otto = font_with(&['A', 'B']);
    otto[0..4].copy_from_slice(b"OTTO");
    // The glyf table has to go too, or the refusal would be about something
    // else: an OTTO font's outlines are in `CFF ` by definition.
    let bytes = strip_glyf(&otto);
    let error = plan(&bytes, &SubsetMode::Ranges).unwrap_err();
    assert!(error.contains("CFF"), "{error}");
    assert!(error.contains("self-hosted the whole font"), "{error}");
}

#[test]
fn subsetting_is_off_by_default_and_says_so() {
    let error = plan(&mixed(), &SubsetMode::Off).unwrap_err();
    assert_eq!(error, "subsetting is off");
}

#[test]
fn a_range_is_written_as_runs_rather_than_a_list() {
    assert_eq!(unicode_range(&[0x41, 0x42, 0x43, 0x61]), "U+41-43,U+61");
    assert_eq!(unicode_range(&[0x20AC]), "U+20AC");
    assert_eq!(unicode_range(&[]), "");
    // Out of order and duplicated, which is what a partition hands it.
    assert_eq!(unicode_range(&[0x43, 0x41, 0x42, 0x41]), "U+41-43");
}

#[test]
fn the_configuration_names_map_to_the_two_modes() {
    assert_eq!(SubsetMode::from_name("none"), Some(SubsetMode::Off));
    assert_eq!(SubsetMode::from_name("Ranges"), Some(SubsetMode::Ranges));
    assert_eq!(SubsetMode::from_name("everything"), None);
}

/// Every code point one `U+…` or `U+…-…` term names.
fn expand(term: &str) -> Vec<u32> {
    let body = term.trim().trim_start_matches("U+");
    match body.split_once('-') {
        None => vec![u32::from_str_radix(body, 16).unwrap()],
        Some((start, end)) => (u32::from_str_radix(start, 16).unwrap()
            ..=u32::from_str_radix(end, 16).unwrap())
            .collect(),
    }
}

/// The same font with its `glyf` table removed.
fn strip_glyf(bytes: &[u8]) -> Vec<u8> {
    let tables = crate::font::sfnt_tables_for_test(bytes);
    let kept: Vec<([u8; 4], Vec<u8>)> = tables
        .into_iter()
        .filter(|(tag, _)| tag != b"glyf" && tag != b"loca")
        .collect();
    let mut out = crate::font::pack_sfnt(0x4F54_544F, &kept);
    out[0..4].copy_from_slice(b"OTTO");
    out
}
