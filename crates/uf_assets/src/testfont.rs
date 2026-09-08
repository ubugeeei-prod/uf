//! A real TrueType font, built in memory, for tests.
//!
//! The existing font fixture in `font/tests.rs` is three fixed-size tables and
//! nothing else, which is all [`crate::font::read_metrics`] reads. Everything
//! added since needs a font with *glyphs*: [`crate::og`] rasterises outlines
//! and [`crate::subset`] hands the file to `skera`, and neither can be tested
//! against a header.
//!
//! So this builds one — `head`, `hhea`, `maxp`, `hmtx`, `cmap`, `loca`, `glyf`
//! and `OS/2`, with a square outline per character — rather than checking a
//! binary into the repository. A generated font is inspectable, it is diffable
//! when it changes, its coverage is chosen by the test that wants it, and it
//! carries no licence.
//!
//! Exported as `uf_assets::test_font` rather than kept behind `#[cfg(test)]`:
//! `crates/uf_cli` tests the `uf assets` protocol end to end and needs the same
//! font, and a fixture built twice in two crates is a fixture that drifts.

/// A font covering exactly `characters`, with one square glyph each.
///
/// Glyph ids are assigned in code-point order, after `.notdef`, which is what
/// lets the `cmap` be written as `idDelta` segments with no glyph array — and
/// is why the input is sorted here rather than trusted to be. `characters`
/// must be inside the basic multilingual plane: the format 4 `cmap` written
/// below covers no more than that.
pub fn font_with(characters: &[char]) -> Vec<u8> {
    font_with_blanks(characters, &[])
}

/// As [`font_with`], but the characters in `blank` get an empty glyph.
///
/// Two equal `loca` offsets is how a font says "this character has metrics and
/// no outline", which is exactly what a colour or bitmap glyph looks like to an
/// outline rasteriser. [`crate::og`] refuses those rather than advancing past
/// them and leaving a gap, and that refusal needs a font that has one.
pub fn font_with_blanks(characters: &[char], blank: &[char]) -> Vec<u8> {
    let mut characters = characters.to_vec();
    characters.sort_unstable();
    characters.dedup();
    let characters = characters.as_slice();
    let count = characters.len() + 1;

    let mut glyf = Vec::new();
    let mut loca = Vec::new();
    // `loca` has one more entry than the font has glyphs, because an entry is
    // the *start* of a glyph and the last glyph needs an end. Getting that
    // wrong costs exactly the last glyph in the font, silently, which is a
    // thing to write down rather than to rediscover.
    //
    // The first two are both zero: `.notdef` is an empty glyph, which is what
    // two equal offsets mean, and it is glyph 0 while the `cmap` below assigns
    // characters from glyph 1.
    loca.extend_from_slice(&0u32.to_be_bytes());
    loca.extend_from_slice(&0u32.to_be_bytes());
    for character in characters {
        if !blank.contains(character) {
            glyf.extend_from_slice(&square());
        }
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
    }

    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    head[36..38].copy_from_slice(&100i16.to_be_bytes());
    head[38..40].copy_from_slice(&0i16.to_be_bytes());
    head[40..42].copy_from_slice(&700i16.to_be_bytes());
    head[42..44].copy_from_slice(&700i16.to_be_bytes());
    // `indexToLocFormat` 1: `loca` holds 32-bit offsets, which is the format
    // written above.
    head[50..52].copy_from_slice(&1i16.to_be_bytes());

    let mut hhea = vec![0u8; 36];
    hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
    hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    hhea[8..10].copy_from_slice(&0i16.to_be_bytes());
    hhea[10..12].copy_from_slice(&800u16.to_be_bytes());
    hhea[34..36].copy_from_slice(&(count as u16).to_be_bytes());

    let mut maxp = vec![0u8; 32];
    maxp[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    maxp[6..8].copy_from_slice(&4u16.to_be_bytes());
    maxp[8..10].copy_from_slice(&1u16.to_be_bytes());

    let mut hmtx = Vec::new();
    for _ in 0..count {
        hmtx.extend_from_slice(&800u16.to_be_bytes());
        hmtx.extend_from_slice(&100i16.to_be_bytes());
    }

    let mut os2 = vec![0u8; 96];
    os2[0..2].copy_from_slice(&4u16.to_be_bytes());
    os2[2..4].copy_from_slice(&500i16.to_be_bytes());
    os2[68..70].copy_from_slice(&800i16.to_be_bytes());
    os2[70..72].copy_from_slice(&(-200i16).to_be_bytes());

    let tables = vec![
        (*b"OS/2", os2),
        (*b"cmap", cmap(characters)),
        (*b"glyf", glyf),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"loca", loca),
        (*b"maxp", maxp),
    ];
    crate::font::pack_sfnt(0x0001_0000, &tables)
}

/// One simple glyph: a square from (100,0) to (700,700).
fn square() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1i16.to_be_bytes()); // one contour
    out.extend_from_slice(&100i16.to_be_bytes());
    out.extend_from_slice(&0i16.to_be_bytes());
    out.extend_from_slice(&700i16.to_be_bytes());
    out.extend_from_slice(&700i16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes()); // last point of contour 0
    out.extend_from_slice(&0u16.to_be_bytes()); // no instructions
    // Four on-curve points, each with 16-bit deltas: flag bit 0 is
    // "on curve" and the short/same bits are left clear.
    out.extend_from_slice(&[0x01, 0x01, 0x01, 0x01]);
    for delta in [100i16, 600, 0, -600] {
        out.extend_from_slice(&delta.to_be_bytes());
    }
    for delta in [0i16, 0, 700, -700] {
        out.extend_from_slice(&delta.to_be_bytes());
    }
    out
}

/// A format 4 `cmap`, one segment per run of consecutive characters.
fn cmap(characters: &[char]) -> Vec<u8> {
    let mut segments: Vec<(u16, u16, u16)> = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let start = characters[index] as u16;
        let start_glyph = index as u16 + 1;
        let mut end = start;
        while index + 1 < characters.len() && characters[index + 1] as u16 == end + 1 {
            index += 1;
            end = characters[index] as u16;
        }
        segments.push((start, end, start_glyph));
        index += 1;
    }
    // The terminating segment every format 4 table has to end with.
    segments.push((0xFFFF, 0xFFFF, 0));

    let count = segments.len();
    let mut subtable = Vec::new();
    subtable.extend_from_slice(&4u16.to_be_bytes());
    subtable.extend_from_slice(&((16 + count * 8) as u16).to_be_bytes());
    subtable.extend_from_slice(&0u16.to_be_bytes());
    subtable.extend_from_slice(&((count * 2) as u16).to_be_bytes());
    let entry_selector = u32::BITS - 1 - (count as u32).leading_zeros();
    let search_range = (1u16 << entry_selector) * 2;
    subtable.extend_from_slice(&search_range.to_be_bytes());
    subtable.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    subtable.extend_from_slice(&((count as u16 * 2).saturating_sub(search_range)).to_be_bytes());
    for (_, end, _) in &segments {
        subtable.extend_from_slice(&end.to_be_bytes());
    }
    subtable.extend_from_slice(&0u16.to_be_bytes());
    for (start, _, _) in &segments {
        subtable.extend_from_slice(&start.to_be_bytes());
    }
    for (start, _, glyph) in &segments {
        // `idDelta` maps the whole run at once, which is only correct because
        // glyph ids were assigned in character order.
        subtable.extend_from_slice(&glyph.wrapping_sub(*start).to_be_bytes());
    }
    for _ in &segments {
        subtable.extend_from_slice(&0u16.to_be_bytes());
    }

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&3u16.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&12u32.to_be_bytes());
    out.extend_from_slice(&subtable);
    out
}
