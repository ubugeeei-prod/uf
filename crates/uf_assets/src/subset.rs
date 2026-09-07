//! Rebuilding a font from the characters a page actually uses.
//!
//! # What changed, and why it is here now
//!
//! [`crate::font`] used to say plainly that it does not subset: "reading three
//! fixed-size tables out of a container and rebuilding a font from a glyph
//! closure are different jobs by orders of magnitude". That is still true, and
//! it is the reason none of the work below is uf's. `skera` — Google Fonts'
//! Rust port of HarfBuzz's `hb-subset`, from the `fontations` project — does
//! the closure, the `GSUB`/`GPOS` pruning, the `glyf`/`loca`/`hmtx`/`cmap`
//! rebuild and the offset repacking. uf decides *what* to keep and packages
//! the result.
//!
//! # Two modes, and the one that is safe
//!
//! Subsetting is lossy by construction: a glyph that was removed is a
//! character the page can no longer draw. So there are two modes and the
//! default is neither.
//!
//! * [`SubsetMode::Ranges`] is **coverage-preserving**. The font's whole
//!   `cmap` is partitioned into script buckets, each bucket becomes its own
//!   file, and each file's `@font-face` carries the exact `unicode-range` of
//!   what is in it. Nothing is lost — a page that renders Cyrillic still gets
//!   Cyrillic — but a page that renders none of it never downloads it. This is
//!   what makes a large family usable: the reader pays for the script they are
//!   reading, not for the family.
//! * [`SubsetMode::Text`] is the lossy one. The author names the characters,
//!   uf keeps those and their layout closure, and everything else is gone. It
//!   is right for a heading face used for six words and wrong for anything
//!   rendering text a user typed, which is why it is never a default and why
//!   it has to be written out per import.
//!
//! The ranges uf writes are computed from the subset's own `cmap` rather than
//! copied from a published table, so a browser never fetches a bucket for a
//! character that is not in it and never skips one that is. Google's own
//! `unicode-range` values are static and overlap; these are exact and
//! disjoint.
//!
//! # What this refuses
//!
//! * **CFF outlines** — an `.otf` whose outlines are in `CFF `/`CFF2`. `skera`
//!   does not subset those tables, and a font that came back without them is a
//!   font with no glyphs at all. Refused with a message rather than emitted.
//! * **WOFF2 input** — see [`crate::font::to_sfnt`].
//! * **A font whose `cmap` uf cannot read**, which is a font that has no
//!   character-to-glyph mapping to partition.
//!
//! Every refusal leaves the *unsubsetted* self-hosted face in place, so a
//! project that asked for a subset and cannot have one still gets a working
//! font and a sentence saying why it is the whole one.

use ab_glyph::Font as _;
use serde::{Deserialize, Serialize};
use write_fonts::read::{
    FontRef,
    collections::IntSet,
    types::{GlyphId, NameId, Tag},
};

#[cfg(test)]
mod tests;

/// How a font import wants its bytes cut down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubsetMode {
    /// Self-host the file as supplied. The default, and the only mode that
    /// cannot change what a page can render.
    Off,
    /// Split the font's own coverage into script buckets with exact
    /// `unicode-range` values. Loses nothing; defers most of the download.
    Ranges,
    /// Keep only the characters in this string, and their layout closure.
    Text(String),
}

impl SubsetMode {
    /// The mode named by a configuration string, if it names one.
    ///
    /// `"none"`/`"off"`/`"false"` and `"ranges"` are the two a project can
    /// write in `uf.config.js`; [`SubsetMode::Text`] comes from an import,
    /// because the characters a face is cut to are a property of the face's
    /// use and not of the project.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "false" => Some(Self::Off),
            "ranges" | "range" => Some(Self::Ranges),
            _ => None,
        }
    }
}

/// One script bucket a family's coverage is split into.
struct Bucket {
    name: &'static str,
    ranges: &'static [(u32, u32)],
}

/// The buckets, in the order a code point is offered to them.
///
/// Order is what makes them disjoint: a code point joins the first bucket that
/// claims it and no other, so the emitted `unicode-range` values partition the
/// font's coverage exactly. That is the difference from Google Fonts' published
/// ranges, which overlap and leave the browser to resolve the overlap by
/// declaration order.
///
/// The ranges themselves follow Google's, because they are the ones the web's
/// fonts are actually divided along and because a bucket boundary in an unusual
/// place makes a page fetch two files where it used to fetch one. `vietnamese`
/// sits before `latin-ext` deliberately: its letters are inside latin-ext's
/// block, and a Vietnamese page should fetch the small bucket rather than the
/// large one that contains it.
const BUCKETS: &[Bucket] = &[
    Bucket {
        name: "latin",
        ranges: &[
            (0x0000, 0x00FF),
            (0x0131, 0x0131),
            (0x0152, 0x0153),
            (0x02BB, 0x02BC),
            (0x02C6, 0x02C6),
            (0x02DA, 0x02DA),
            (0x02DC, 0x02DC),
            (0x2000, 0x206F),
            (0x2074, 0x2074),
            (0x20AC, 0x20AC),
            (0x2122, 0x2122),
            (0x2191, 0x2191),
            (0x2193, 0x2193),
            (0x2212, 0x2212),
            (0x2215, 0x2215),
            (0xFEFF, 0xFEFF),
            (0xFFFD, 0xFFFD),
        ],
    },
    Bucket {
        name: "vietnamese",
        ranges: &[
            (0x0102, 0x0103),
            (0x0110, 0x0111),
            (0x0128, 0x0129),
            (0x0168, 0x0169),
            (0x01A0, 0x01A1),
            (0x01AF, 0x01B0),
            (0x1EA0, 0x1EF9),
            (0x20AB, 0x20AB),
        ],
    },
    Bucket {
        name: "latin-ext",
        ranges: &[
            (0x0100, 0x02AF),
            (0x0300, 0x036F),
            (0x1E00, 0x1EFF),
            (0x2020, 0x2020),
            (0x20A0, 0x20C0),
            (0x2113, 0x2113),
            (0x2C60, 0x2C7F),
            (0xA720, 0xA7FF),
        ],
    },
    Bucket {
        name: "greek",
        ranges: &[(0x0370, 0x03FF)],
    },
    Bucket {
        name: "greek-ext",
        ranges: &[(0x1F00, 0x1FFF)],
    },
    Bucket {
        name: "cyrillic",
        ranges: &[
            (0x0400, 0x045F),
            (0x0490, 0x0491),
            (0x04B0, 0x04B1),
            (0x2116, 0x2116),
        ],
    },
    Bucket {
        name: "cyrillic-ext",
        ranges: &[
            (0x0460, 0x052F),
            (0x1C80, 0x1C8F),
            (0x20B4, 0x20B4),
            (0x2DE0, 0x2DFF),
            (0xA640, 0xA69F),
            (0xFE2E, 0xFE2F),
        ],
    },
];

/// The name the code points no bucket claimed are emitted under.
///
/// Everything from CJK to mathematical operators lands here, in one file. A
/// CJK family therefore gets no useful split, and that is stated rather than
/// papered over: Google divides CJK into about a hundred buckets by character
/// frequency, which needs a frequency table uf does not have and would have to
/// keep current.
const REST: &str = "rest";

/// One emitted piece of a split family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsetFace {
    /// Which bucket this is — `latin`, `cyrillic`, `rest`, or `text` for an
    /// explicit subset.
    pub bucket: String,
    /// The face's bytes, as WOFF 1.0.
    #[serde(skip)]
    pub bytes: Vec<u8>,
    /// The exact `unicode-range` of what is in it.
    pub unicode_range: String,
    /// How many code points it covers.
    pub codepoints: u32,
}

/// Everything one subsetting pass produced.
#[derive(Debug, Clone)]
pub struct SubsetPlan {
    /// The faces, largest coverage first.
    pub faces: Vec<SubsetFace>,
    /// The bucket a page should preload, when preloading is on.
    ///
    /// The one covering `A`, which is the text almost every page starts with.
    /// Preloading all of them would defeat the split — the whole point is that
    /// the reader downloads one of them.
    pub primary: usize,
}

/// Cut a font down, or say why it cannot be.
///
/// `sfnt` must already be a bare SFNT; see [`crate::font::to_sfnt`].
///
/// # Errors
///
/// A sentence naming what stopped it, for a caller to put on the asset as
/// `subsetDeclined`. Never fatal: the caller keeps the whole font.
pub fn plan(sfnt: &[u8], mode: &SubsetMode) -> Result<SubsetPlan, String> {
    if *mode == SubsetMode::Off {
        return Err(String::from("subsetting is off"));
    }
    let font =
        FontRef::new(sfnt).map_err(|error| format!("uf could not read the font: {error}"))?;
    if font.table_data(Tag::new(b"glyf")).is_none() {
        return Err(String::from(
            "the font's outlines are in a CFF table, which uf's subsetter (skera) does not \
             rebuild — a CFF font that went through it would come back with no glyphs. uf \
             self-hosted the whole font instead",
        ));
    }

    let readable = ab_glyph::FontRef::try_from_slice(sfnt)
        .map_err(|error| format!("uf could not read the font's character map: {error}"))?;
    let covered: Vec<u32> = readable
        .codepoint_ids()
        .map(|(_, character)| character as u32)
        .collect();
    if covered.is_empty() {
        return Err(String::from(
            "the font's cmap maps no characters, so there is nothing to split it along",
        ));
    }

    let groups = match mode {
        SubsetMode::Off => unreachable!("returned above"),
        SubsetMode::Ranges => partition(&covered),
        SubsetMode::Text(text) => {
            let mut wanted: Vec<u32> = text
                .chars()
                .map(|character| character as u32)
                .filter(|code| covered.binary_search(code).is_ok() || covered.contains(code))
                .collect();
            wanted.sort_unstable();
            wanted.dedup();
            if wanted.is_empty() {
                return Err(String::from(
                    "none of the characters in `text` are in this font, so a subset of them \
                     would have no glyphs",
                ));
            }
            vec![("text", wanted)]
        }
    };

    let mut faces = Vec::with_capacity(groups.len());
    for (bucket, codepoints) in groups {
        let cut = cut(&font, &codepoints)?;
        let packed = crate::font::pack_woff(&cut)?;
        faces.push(SubsetFace {
            bucket: bucket.to_owned(),
            unicode_range: unicode_range(&codepoints),
            codepoints: u32::try_from(codepoints.len()).unwrap_or(u32::MAX),
            bytes: packed,
        });
    }
    // Largest first, which is the order they are declared in and therefore the
    // order a browser resolves an overlap in. The ranges are disjoint so there
    // is no overlap to resolve; the order is for a person reading the
    // stylesheet.
    faces.sort_by_key(|face| std::cmp::Reverse(face.codepoints));

    let primary = faces
        .iter()
        .position(|face| face.bucket == "latin" || face.bucket == "text")
        .unwrap_or(0);
    Ok(SubsetPlan { faces, primary })
}

/// Split a font's coverage into buckets, dropping the empty ones.
fn partition(covered: &[u32]) -> Vec<(&'static str, Vec<u32>)> {
    let mut groups: Vec<(&'static str, Vec<u32>)> = Vec::new();
    let mut rest: Vec<u32> = Vec::new();
    for bucket in BUCKETS {
        groups.push((bucket.name, Vec::new()));
    }
    'code: for code in covered {
        for (index, bucket) in BUCKETS.iter().enumerate() {
            if bucket
                .ranges
                .iter()
                .any(|(start, end)| code >= start && code <= end)
            {
                groups[index].1.push(*code);
                continue 'code;
            }
        }
        rest.push(*code);
    }
    if !rest.is_empty() {
        groups.push((REST, rest));
    }
    groups.retain(|(_, codes)| !codes.is_empty());
    groups
}

/// Run `skera` over one code-point set.
fn cut(font: &FontRef<'_>, codepoints: &[u32]) -> Result<Vec<u8>, String> {
    let gids = IntSet::<GlyphId>::empty();
    let mut unicodes = IntSet::<u32>::empty();
    for code in codepoints {
        unicodes.insert(*code);
    }
    let drop_tables: IntSet<Tag> = skera::DEFAULT_DROP_TABLES.iter().copied().collect();
    // Every script and the default feature set: a subset that dropped `liga`
    // or `kern` would render the characters it kept differently from the font
    // it was cut out of, which is a change nobody asked for.
    let mut layout_scripts = IntSet::<Tag>::empty();
    layout_scripts.invert();
    let layout_features: IntSet<Tag> = skera::DEFAULT_LAYOUT_FEATURES.iter().copied().collect();
    // Name records 0-6 in English: copyright, family, subfamily, unique id,
    // full name, version, PostScript name. A licence that travelled with the
    // font travels with the subset.
    let mut name_ids = IntSet::<NameId>::empty();
    name_ids.insert_range(NameId::from(0)..=NameId::from(6));
    let mut name_languages = IntSet::<u16>::empty();
    name_languages.insert(0x0409);

    let plan = skera::Plan::new(
        &gids,
        &unicodes,
        font,
        // Hinting instructions are dropped: they are a large fraction of a
        // TrueType font's bytes, no browser on a modern display uses them
        // (macOS ignores them entirely and Windows uses ClearType's own
        // hinting), and keeping them would undo much of the saving that is the
        // whole point of getting here.
        skera::SubsetFlags::SUBSET_FLAGS_NO_HINTING,
        &drop_tables,
        &layout_scripts,
        &layout_features,
        &name_ids,
        &name_languages,
    );
    skera::subset_font(font, &plan).map_err(|error| format!("the subsetter failed: {error}"))
}

/// The CSS `unicode-range` for one sorted, deduplicated code-point set.
///
/// Consecutive code points are collapsed into `U+aaaa-bbbb` runs, because a
/// list of four thousand single values is a stylesheet nobody can serve.
#[must_use]
pub fn unicode_range(codepoints: &[u32]) -> String {
    let mut sorted = codepoints.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut parts: Vec<String> = Vec::new();
    let mut index = 0;
    while index < sorted.len() {
        let start = sorted[index];
        let mut end = start;
        while index + 1 < sorted.len() && sorted[index + 1] == end + 1 {
            index += 1;
            end = sorted[index];
        }
        if start == end {
            parts.push(format!("U+{start:X}"));
        } else {
            parts.push(format!("U+{start:X}-{end:X}"));
        }
        index += 1;
    }
    parts.join(",")
}
