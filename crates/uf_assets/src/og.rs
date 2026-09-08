//! The Open Graph image, and the renderer uf deliberately is not.
//!
//! # The decision this module records
//!
//! `docs/roadmap.md` named `OgImage` as absent because "generating an image
//! from JSX needs a text-shaping and rasterising path uf does not ship", and
//! there were three ways out of that:
//!
//! 1. ship the whole path — a shaper, a line breaker and a rasteriser — and
//!    get JSX-to-image;
//! 2. render with a headless browser at build time, which makes a browser a
//!    dependency of building an application;
//! 3. draw a **template** instead of rendering a document: a fixed
//!    arrangement — background, rule, eyebrow, title, subtitle — that a
//!    rasteriser with no layout engine can draw.
//!
//! This is (3), and the reason is (2)'s cost and (1)'s scope. A headless
//! browser is out because `ubugeeei-redundancy.md` requires a deployment not
//! to depend on a provider and a build not to depend on a runtime it did not
//! ask for; a browser is the largest such dependency there is. Full JSX is out
//! because "an image from JSX" is a CSS layout engine — flow, flex, grid,
//! floats, `line-height`, `text-overflow` — and an approximation of one
//! silently produces images that are wrong in ways nobody looks at, because
//! nobody looks at an Open Graph image until it is on somebody else's website.
//!
//! So: a declared template, drawn natively, and a refusal with a message for
//! everything the template cannot express. The refusal is the feature. An
//! Open Graph image that is *wrong* is worse than one that does not exist,
//! because the build said it succeeded.
//!
//! # What is native, and what is borrowed
//!
//! Drawing is [`ab_glyph`]: glyph outlines out of the font's own `glyf`/`CFF`
//! and coverage rasterisation of those outlines, both pure Rust, no C
//! toolchain, no system font machinery. uf composites the coverage itself
//! because the composite is a fill of one colour and a `memcpy`-shaped loop,
//! which is not worth a 2D graphics library.
//!
//! `ab_glyph` does **no shaping**, and that is the boundary this module refuses
//! to cross rather than approximate — see [`unsupported_reason`].
//!
//! # What this cannot do, stated once
//!
//! * **Not JSX, and not HTML.** The input is a `.og.json` file: a fixed set of
//!   fields, not a document. There is no element tree, no CSS, no cascade.
//! * **Left-to-right, uncomplicated scripts only.** Arabic joins, Hebrew runs
//!   right to left, Devanagari reorders, Thai has no word spaces, and a
//!   combining mark needs `GPOS` to sit over its base. Every one of those
//!   needs a shaper. uf has none, so it **rejects** text containing them,
//!   naming the character, rather than laying the codepoints out left to right
//!   and producing a picture of nonsense. A character the *font* cannot draw
//!   as an outline — a missing glyph, or an emoji it has only in colour — is
//!   refused too, by asking the font rather than by guessing from the block.
//! * **No font of its own.** uf embeds no typeface: a template says which font
//!   file to draw with, and a template with text and no font is refused. A
//!   toolchain that shipped a default font would ship a licence with it.
//! * **One template, not a design system.** Background, optional rule, eyebrow,
//!   title, subtitle. A project that needs more should draw a PNG and import
//!   that; `Image` has handled one since the pipeline existed.

use ab_glyph::{Font as _, GlyphId, PxScale, ScaleFont as _};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::name::hashed_name;

#[cfg(test)]
mod tests;

/// The Open Graph image size every consumer scales from.
///
/// 1200x630 is the 1.91:1 Facebook, X, LinkedIn and Slack all crop to. A
/// project may say otherwise; nothing here depends on the number.
pub const DEFAULT_WIDTH: u32 = 1200;
/// The height that goes with [`DEFAULT_WIDTH`].
pub const DEFAULT_HEIGHT: u32 = 630;

/// Largest image this module will allocate, in pixels.
///
/// A template is a file in the project, but it is also a file a dependency can
/// ship, and `width * height * 4` is allocated before anything is drawn.
/// 16 megapixels is twenty times the standard card.
pub const MAX_OG_PIXELS: u64 = 16 * 1024 * 1024;

/// The extension an Open Graph template is written under.
///
/// A compound extension, so the claim is as narrow as a claim can be. Ordinary
/// `.json` stays Vite's — only a file a project *named* `.og.json` becomes a
/// card, and `./card.og.json?raw` and `?url` still reach the file itself,
/// because a query is Vite's. That is the shape red line 8 asks for: uf adds a
/// meaning to a name nothing else had, rather than taking one away.
pub const OG_EXTENSION: &str = ".og.json";

/// An Open Graph image that could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum OgError {
    /// The template or its font could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The template is not the JSON this module accepts.
    #[error("{path} is not an Open Graph template uf can draw: {reason}")]
    Malformed {
        /// Path that failed.
        path: Utf8PathBuf,
        /// What was wrong with it.
        reason: String,
    },
    /// The template has text and no font to draw it with.
    #[error(
        "{path} has text but no \"font\": uf ships no typeface of its own, so a template that \
         draws text has to name a font file to draw it with"
    )]
    NoFont {
        /// Path that failed.
        path: Utf8PathBuf,
    },
    /// The named font could not be used.
    #[error("{path} names the font {font}, which uf cannot draw with: {reason}")]
    Font {
        /// The template.
        path: Utf8PathBuf,
        /// The font it named.
        font: Utf8PathBuf,
        /// Why it is unusable.
        reason: String,
    },
    /// The text needs shaping uf does not do.
    ///
    /// The whole point of the module. See [`unsupported_reason`].
    #[error(
        "{path} cannot be drawn: {field} contains {character:?} (U+{codepoint:04X}), and {reason}. \
         uf draws a declared template rather than rendering a document, so it refuses this rather \
         than emitting a card that is wrong in a way nobody looks at. Draw this card yourself and \
         import the PNG"
    )]
    Unsupported {
        /// The template.
        path: Utf8PathBuf,
        /// Which field held it.
        field: &'static str,
        /// The offending character.
        character: char,
        /// Its code point.
        codepoint: u32,
        /// Why uf cannot lay it out.
        reason: &'static str,
    },
    /// The text does not fit the card at any size uf will shrink to.
    #[error(
        "{path} cannot be drawn: {field} still needs {lines} lines at the smallest size uf will \
         shrink to ({size:.0}px), and the card has room for {allowed}. Shorten it, raise \
         \"width\", or lower \"titleSize\""
    )]
    Overflow {
        /// The template.
        path: Utf8PathBuf,
        /// Which field overflowed.
        field: &'static str,
        /// How many lines it needed.
        lines: usize,
        /// How many it is allowed.
        allowed: usize,
        /// The size it was measured at.
        size: f32,
    },
    /// The declared card is larger than [`MAX_OG_PIXELS`].
    #[error("{path} declares {width}x{height}, more than the {MAX_OG_PIXELS} pixel limit")]
    TooLarge {
        /// The template.
        path: Utf8PathBuf,
        /// Declared width.
        width: u32,
        /// Declared height.
        height: u32,
    },
    /// The image could not be encoded.
    #[error("failed to encode {path}: {source}")]
    Encode {
        /// The template.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: image::ImageError,
    },
    /// The image could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// What a `.og.json` file says.
///
/// Deliberately a fixed set of fields rather than anything nestable: this is a
/// template, and the moment it grows children it is a document and needs a
/// layout engine to place them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct OgTemplate {
    /// The headline. The only required field.
    pub title: String,
    /// A line under the title, at a smaller size.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// A short label above the title — a section, a site name.
    #[serde(default)]
    pub eyebrow: Option<String>,
    /// The font file to draw with, relative to the template.
    #[serde(default)]
    pub font: Option<Utf8PathBuf>,
    /// The background: one colour, or two for a vertical gradient.
    #[serde(default)]
    pub background: Background,
    /// The text colour.
    #[serde(default = "default_foreground")]
    pub foreground: String,
    /// A short rule above the eyebrow. Omitted when absent.
    #[serde(default)]
    pub accent: Option<String>,
    /// Card width.
    #[serde(default = "default_width")]
    pub width: u32,
    /// Card height.
    #[serde(default = "default_height")]
    pub height: u32,
    /// Title size in pixels, before any shrink-to-fit.
    #[serde(default = "default_title_size")]
    pub title_size: f32,
    /// Subtitle size in pixels.
    #[serde(default = "default_subtitle_size")]
    pub subtitle_size: f32,
    /// The margin on every side.
    #[serde(default = "default_padding")]
    pub padding: f32,
    /// How many lines the title may wrap to.
    #[serde(default = "default_title_lines")]
    pub title_lines: usize,
    /// The `og:image:alt` text. Falls back to the title.
    #[serde(default)]
    pub alt: Option<String>,
}

fn default_foreground() -> String {
    String::from("#f8fafc")
}
const fn default_width() -> u32 {
    DEFAULT_WIDTH
}
const fn default_height() -> u32 {
    DEFAULT_HEIGHT
}
const fn default_title_size() -> f32 {
    72.0
}
const fn default_subtitle_size() -> f32 {
    34.0
}
const fn default_padding() -> f32 {
    80.0
}
const fn default_title_lines() -> usize {
    3
}

/// What is painted behind the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Background {
    /// One colour.
    Solid(String),
    /// A vertical gradient, top to bottom.
    Gradient {
        /// The colour at the top edge.
        from: String,
        /// The colour at the bottom edge.
        to: String,
    },
}

impl Default for Background {
    fn default() -> Self {
        Self::Solid(String::from("#0b1020"))
    }
}

/// What a project asked uf to draw.
#[derive(Debug, Clone)]
pub struct OgRequest<'a> {
    /// The `.og.json` file, as the author wrote it.
    pub source: &'a Utf8Path,
    /// Where the PNG is written.
    pub out_dir: &'a Utf8Path,
}

/// One drawn Open Graph image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OgAsset {
    /// The emitted file's name, content-hashed.
    pub file: String,
    /// Its media type. Always `image/png`.
    pub mime: String,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// Its size on disk.
    pub bytes: u64,
    /// The `og:image:alt` text.
    pub alt: String,
}

/// Draw one template and write the PNG.
///
/// # Errors
///
/// [`OgError`] when the template cannot be read or parsed, names no font while
/// carrying text, names a font uf cannot draw with, carries text needing a
/// shaper uf does not have, does not fit the card, or cannot be written.
pub fn draw(request: &OgRequest<'_>) -> Result<OgAsset, OgError> {
    let raw = std::fs::read(request.source).map_err(|source| OgError::Read {
        path: request.source.to_owned(),
        source,
    })?;
    let template: OgTemplate =
        serde_json::from_slice(&raw).map_err(|error| OgError::Malformed {
            path: request.source.to_owned(),
            reason: error.to_string(),
        })?;
    render(request, &template, &raw)
}

/// Draw an already-parsed template.
///
/// `key` is whatever the caller wants folded into the emitted file's name
/// alongside the pixels' inputs — [`draw`] passes the template's own bytes.
///
/// # Errors
///
/// As [`draw`], minus the parse.
pub fn render(
    request: &OgRequest<'_>,
    template: &OgTemplate,
    key: &[u8],
) -> Result<OgAsset, OgError> {
    let path = request.source;
    if u64::from(template.width) * u64::from(template.height) > MAX_OG_PIXELS
        || template.width == 0
        || template.height == 0
    {
        return Err(OgError::TooLarge {
            path: path.to_owned(),
            width: template.width,
            height: template.height,
        });
    }

    let lines = check_and_collect(path, template)?;
    let font_path = resolve_font(path, template)?;
    let sfnt = load_font(path, &font_path)?;
    let font = ab_glyph::FontRef::try_from_slice(&sfnt).map_err(|error| OgError::Font {
        path: path.to_owned(),
        font: font_path.clone(),
        reason: error.to_string(),
    })?;
    reject_missing_glyphs(path, &font, &lines)?;

    let canvas = paint(path, template, &font, &lines)?;

    let mut encoded = Vec::new();
    {
        use image::ImageEncoder as _;
        image::codecs::png::PngEncoder::new_with_quality(
            &mut encoded,
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::Adaptive,
        )
        .write_image(
            canvas.as_raw(),
            template.width,
            template.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|source| OgError::Encode {
            path: path.to_owned(),
            source,
        })?;
    }

    // The font's bytes are in the digest as well as the template's: two
    // projects with the same words and different typefaces are two different
    // pictures, and a name that did not say so would serve one of them for
    // both.
    let stem = path.file_name().unwrap_or("og").trim_end_matches(".json");
    let file = hashed_name(stem, key, &[&sfnt], "png");
    std::fs::create_dir_all(request.out_dir).map_err(|source| OgError::Write {
        path: request.out_dir.to_owned(),
        source,
    })?;
    let target = request.out_dir.join(&file);
    std::fs::write(&target, &encoded).map_err(|source| OgError::Write {
        path: target,
        source,
    })?;

    Ok(OgAsset {
        file,
        mime: String::from("image/png"),
        width: template.width,
        height: template.height,
        bytes: encoded.len() as u64,
        alt: template
            .alt
            .clone()
            .unwrap_or_else(|| template.title.clone()),
    })
}

/// Every text field, in draw order, checked for what uf cannot lay out.
fn check_and_collect<'a>(
    path: &Utf8Path,
    template: &'a OgTemplate,
) -> Result<Vec<(&'static str, &'a str)>, OgError> {
    let mut fields: Vec<(&'static str, &'a str)> = Vec::new();
    if let Some(eyebrow) = template.eyebrow.as_deref() {
        fields.push(("eyebrow", eyebrow));
    }
    fields.push(("title", template.title.as_str()));
    if let Some(subtitle) = template.subtitle.as_deref() {
        fields.push(("subtitle", subtitle));
    }
    for (field, text) in &fields {
        for character in text.chars() {
            if let Some(reason) = unsupported_reason(character) {
                return Err(OgError::Unsupported {
                    path: path.to_owned(),
                    field,
                    character,
                    codepoint: character as u32,
                    reason,
                });
            }
        }
    }
    Ok(fields)
}

/// Why uf cannot lay this character out, if it cannot.
///
/// This is the honest boundary of a renderer with no shaper, written as a
/// predicate so it is testable and so the message names the character rather
/// than the feature. Each arm is a thing a shaping engine does that placing
/// glyphs left to right at their advance widths does not:
///
/// * **bidirectional** — Hebrew, Arabic, Syriac, Thaana and N'Ko run right to
///   left, and a run of them inside a left-to-right line is reordered by the
///   Unicode bidirectional algorithm before anything is placed;
/// * **joining** — an Arabic letter has four contextual forms and `GSUB`
///   chooses between them from its neighbours;
/// * **reordering and clustering** — Devanagari and the Brahmic scripts move
///   a vowel sign to the other side of its consonant and form conjuncts;
/// * **no word spaces** — Thai, Lao, Khmer and Myanmar break lines by
///   dictionary, not by U+0020, so greedy wrapping on spaces produces one line
///   that runs off the card;
/// * **mark positioning** — a combining mark is placed over its base by
///   `GPOS`, and without it the mark lands at the base's advance width, next
///   to the letter instead of on it;
/// * **sequences** — a zero-width joiner or a variation selector says that
///   several code points are one picture, which is a decision `GSUB` makes.
///
/// Latin, Greek, Cyrillic, CJK and the punctuation around them are placed
/// correctly by advance width and legacy `kern`, which is why they are not
/// here.
///
/// What is deliberately *not* here is a list of emoji and symbol blocks. This
/// predicate is about layout; whether the font can draw a character at all is
/// a question about the font, and `reject_missing_glyphs` asks it directly —
/// a character the font has no glyph for, or draws from a `COLR`, `sbix` or
/// `CBDT` table rather than an outline, is refused there. Testing the font
/// beats testing a block range in both directions: a check mark or an arrow
/// the font has an outline for is drawn instead of being refused for being
/// near the emoji, and an emoji the font has only in colour is refused
/// instead of being drawn as a blank of the right width.
#[must_use]
pub fn unsupported_reason(character: char) -> Option<&'static str> {
    let code = character as u32;
    let reason = match code {
        // Combining diacritical marks, and the three later blocks of them.
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF => {
            "a combining mark is positioned over its base by the font's GPOS table, which uf does \
             not read"
        }
        // Hebrew, Arabic, Syriac, Thaana, N'Ko and the Arabic presentation
        // forms.
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF => {
            "it belongs to a right-to-left script, which has to be reordered and — in Arabic —
             joined by a shaping engine"
        }
        // Devanagari through Sinhala: reordering, conjuncts, matras.
        0x0900..=0x0DFF => {
            "it belongs to a Brahmic script, whose vowel signs are reordered around their \
             consonant by a shaping engine"
        }
        // Thai, Lao, Tibetan, Myanmar, Khmer: no word spaces, stacked marks.
        0x0E00..=0x0FFF | 0x1000..=0x109F | 0x1780..=0x17FF => {
            "it belongs to a script that does not separate words with spaces, so uf cannot break \
             its lines"
        }
        // Zero-width joiner and the variation selectors: each says that
        // several code points are one glyph, which is a GSUB substitution.
        0x200D | 0xFE00..=0xFE0F | 0xE0100..=0xE01EF => {
            "it joins the characters around it into one glyph, which a shaping engine substitutes"
        }
        // A control character has no glyph and no defined placement. Tab is
        // included: its width depends on a tab stop nothing here defines, and
        // so is a newline: this is a template with a wrapping algorithm of its
        // own, and a line break inside a field would fight it.
        0x00..=0x1F | 0x7F..=0x9F => {
            "a control character has no glyph and no width uf can place it at"
        }
        _ => return None,
    };
    Some(reason)
}

/// Refuse a character this font cannot put an outline on the page for.
///
/// Two failures, both silent if they are not caught here, and both a property
/// of the font rather than of the script — which is why they are asked of the
/// font instead of guessed from a block range in [`unsupported_reason`]:
///
/// * **no glyph at all.** Drawing `.notdef` produces a row of empty boxes,
///   which looks like a bug in whatever is displaying the card rather than a
///   missing glyph in the font.
/// * **a glyph with no outline.** A colour or bitmap glyph — `COLR`, `sbix`,
///   `CBDT` — has metrics `ab_glyph` will happily advance past and nothing to
///   rasterise, so it draws as a gap of exactly the right width. That is the
///   worst of the failure modes available here, because the card looks
///   deliberate.
///
/// Whitespace is exempt from the second: a space is a glyph with an advance
/// and no outline by definition, and so are the other separators.
fn reject_missing_glyphs(
    path: &Utf8Path,
    font: &ab_glyph::FontRef<'_>,
    lines: &[(&'static str, &str)],
) -> Result<(), OgError> {
    for (field, text) in lines {
        for character in text.chars() {
            let id = font.glyph_id(character);
            let reason = if id == GlyphId(0) {
                "the font has no glyph for it, and uf will not draw the empty box a missing \
                 glyph renders as"
            } else if !character.is_whitespace() && font.outline(id).is_none() {
                "the font draws it from a colour or bitmap table (COLR, sbix or CBDT) rather \
                 than as an outline, and uf rasterises outlines — drawing it would leave a gap \
                 the exact width of the character"
            } else {
                continue;
            };
            return Err(OgError::Unsupported {
                path: path.to_owned(),
                field,
                character,
                codepoint: character as u32,
                reason,
            });
        }
    }
    Ok(())
}

/// The font file the template names, as an absolute path.
fn resolve_font(path: &Utf8Path, template: &OgTemplate) -> Result<Utf8PathBuf, OgError> {
    let Some(declared) = template.font.as_ref() else {
        return Err(OgError::NoFont {
            path: path.to_owned(),
        });
    };
    if declared.is_absolute() {
        return Ok(declared.clone());
    }
    let base = path.parent().unwrap_or(Utf8Path::new("."));
    Ok(base.join(declared))
}

/// The font's bytes as a bare SFNT, whatever container they arrived in.
fn load_font(path: &Utf8Path, font: &Utf8Path) -> Result<Vec<u8>, OgError> {
    let bytes = std::fs::read(font).map_err(|source| OgError::Read {
        path: font.to_owned(),
        source,
    })?;
    crate::font::to_sfnt(font, &bytes).map_err(|reason| OgError::Font {
        path: path.to_owned(),
        font: font.to_owned(),
        reason,
    })
}

/// One laid-out line of text.
///
/// A newtype rather than a bare `String` so that [`wrap`]'s return type says
/// what it holds: lines, not words and not the original text.
struct Line {
    text: String,
}

/// Break `text` into lines no wider than `limit` at `size`.
///
/// Greedy on U+0020, which is the correct algorithm for every script this
/// module accepts and is exactly why it rejects the ones where it is not. A
/// single word wider than the limit gets a line of its own and overflows it:
/// the alternative is breaking inside a word without the hyphenation
/// dictionary that would tell you where.
fn wrap(font: &ab_glyph::FontRef<'_>, text: &str, size: f32, limit: f32) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut current = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if measure(font, size, &candidate) <= limit || current.is_empty() {
            current = candidate;
        } else {
            lines.push(Line {
                text: std::mem::take(&mut current),
            });
            current = word.to_owned();
        }
    }
    if !current.is_empty() {
        lines.push(Line { text: current });
    }
    lines
}

/// The advance width of one string at one size.
///
/// Advances plus the font's legacy `kern` table, which is the whole of what
/// placing glyphs without a shaper can account for. `GPOS` kerning — which is
/// where a modern font puts it — is not read, so a pair the font would have
/// tightened is drawn a fraction of an em apart. That is a difference of a few
/// pixels across a headline rather than a wrong picture, which is why it is a
/// documented approximation and the items in [`unsupported_reason`] are not.
fn measure(font: &ab_glyph::FontRef<'_>, size: f32, text: &str) -> f32 {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut width = 0.0;
    let mut previous: Option<GlyphId> = None;
    for character in text.chars() {
        let glyph = font.glyph_id(character);
        if let Some(before) = previous {
            width += scaled.kern(before, glyph);
        }
        width += scaled.h_advance(glyph);
        previous = Some(glyph);
    }
    width
}

/// The whole card, drawn.
fn paint(
    path: &Utf8Path,
    template: &OgTemplate,
    font: &ab_glyph::FontRef<'_>,
    fields: &[(&'static str, &str)],
) -> Result<image::RgbaImage, OgError> {
    let mut canvas = image::RgbaImage::new(template.width, template.height);
    background(&mut canvas, &template.background);

    let limit = px(template.width) - template.padding * 2.0;
    let foreground = colour(&template.foreground);

    // Shrink-to-fit before anything is placed, because the title's height is
    // what the rest of the stack is positioned around. The floor is 60% of the
    // declared size: below that the card stops looking like the one the
    // project designed, and a card nobody recognises is worse than a build
    // error that says the headline is too long.
    let mut title_size = template.title_size;
    let floor = template.title_size * 0.6;
    let mut title = wrap(font, template.title.as_str(), title_size, limit);
    while title.len() > template.title_lines && title_size > floor {
        title_size = (title_size * 0.92).max(floor);
        title = wrap(font, template.title.as_str(), title_size, limit);
    }
    if title.len() > template.title_lines {
        return Err(OgError::Overflow {
            path: path.to_owned(),
            field: "title",
            lines: title.len(),
            allowed: template.title_lines,
            size: title_size,
        });
    }

    let eyebrow = fields
        .iter()
        .find(|(field, _)| *field == "eyebrow")
        .map(|(_, text)| wrap(font, text, template.subtitle_size * 0.72, limit))
        .unwrap_or_default();
    let subtitle = fields
        .iter()
        .find(|(field, _)| *field == "subtitle")
        .map(|(_, text)| wrap(font, text, template.subtitle_size, limit))
        .unwrap_or_default();

    let eyebrow_size = template.subtitle_size * 0.72;
    let line_height = |size: f32| size * 1.25;
    let rule = if template.accent.is_some() { 8.0 } else { 0.0 };
    let gap = template.subtitle_size * 0.8;

    let mut total = rule + if rule > 0.0 { gap } else { 0.0 };
    if !eyebrow.is_empty() {
        total += eyebrow.len() as f32 * line_height(eyebrow_size) + gap;
    }
    total += title.len() as f32 * line_height(title_size);
    if !subtitle.is_empty() {
        total += gap + subtitle.len() as f32 * line_height(template.subtitle_size);
    }

    let mut y = (px(template.height) - total) / 2.0;
    let x = template.padding;

    if let Some(accent) = template.accent.as_deref() {
        fill_rect(&mut canvas, x, y, 96.0, rule, colour(accent));
        y += rule + gap;
    }
    for line in &eyebrow {
        draw_line(&mut canvas, font, line, eyebrow_size, x, y, foreground);
        y += line_height(eyebrow_size);
    }
    if !eyebrow.is_empty() {
        y += gap;
    }
    for line in &title {
        draw_line(&mut canvas, font, line, title_size, x, y, foreground);
        y += line_height(title_size);
    }
    if !subtitle.is_empty() {
        y += gap;
        for line in &subtitle {
            draw_line(
                &mut canvas,
                font,
                line,
                template.subtitle_size,
                x,
                y,
                foreground,
            );
            y += line_height(template.subtitle_size);
        }
    }

    Ok(canvas)
}

/// A pixel count as a coordinate.
///
/// `as f32` and not `f32::from`: `u32` does not convert losslessly, and the
/// loss begins at sixteen million — past [`MAX_OG_PIXELS`] in either axis. A
/// narrowing conversion through `u16` would silently clamp a legal card
/// instead, which is the failure this exists to avoid.
#[expect(
    clippy::cast_precision_loss,
    reason = "bounded by MAX_OG_PIXELS, well inside f32's exact integer range"
)]
fn px(value: u32) -> f32 {
    value as f32
}

/// Fill the whole canvas: one colour, or a vertical gradient.
fn background(canvas: &mut image::RgbaImage, declared: &Background) {
    let height = canvas.height().max(1);
    match declared {
        Background::Solid(value) => {
            let pixel = image::Rgba(colour(value));
            for target in canvas.pixels_mut() {
                *target = pixel;
            }
        }
        Background::Gradient { from, to } => {
            let start = colour(from);
            let end = colour(to);
            // Interpolated in sRGB rather than linear light. Two dark brand
            // colours a card is likely to use differ by little enough that the
            // banding a linear ramp would avoid is not visible, and a linear
            // ramp between them reads as a lighter middle than either was
            // chosen for.
            for (y, row) in canvas.enumerate_rows_mut() {
                let t = px(y) / px(height - 1).max(1.0);
                let mut mixed = [0u8; 4];
                for (channel, value) in mixed.iter_mut().enumerate() {
                    let a = f32::from(start[channel]);
                    let b = f32::from(end[channel]);
                    *value = (a + (b - a) * t).round().clamp(0.0, 255.0) as u8;
                }
                let pixel = image::Rgba(mixed);
                for (_, _, target) in row {
                    *target = pixel;
                }
            }
        }
    }
}

/// An axis-aligned rectangle, composited over what is there.
fn fill_rect(
    canvas: &mut image::RgbaImage,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    colour: [u8; 4],
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let x0 = x.max(0.0) as u32;
    let y0 = y.max(0.0) as u32;
    let x1 = ((x + width).max(0.0) as u32).min(canvas.width());
    let y1 = ((y + height).max(0.0) as u32).min(canvas.height());
    for py in y0..y1 {
        for px in x0..x1 {
            blend(canvas.get_pixel_mut(px, py), colour, 1.0);
        }
    }
}

/// One line of glyphs, at `y` as the line box's top.
fn draw_line(
    canvas: &mut image::RgbaImage,
    font: &ab_glyph::FontRef<'_>,
    line: &Line,
    size: f32,
    x: f32,
    y: f32,
    colour: [u8; 4],
) {
    let scaled = font.as_scaled(PxScale::from(size));
    // `y` is the top of the line box and glyphs are positioned on a baseline,
    // so the ascent is the offset between them. Using the font's own ascent
    // rather than the size is what keeps two fonts at the same declared size
    // sitting on the same line.
    let baseline = y + scaled.ascent();
    let mut pen = x;
    let mut previous: Option<GlyphId> = None;
    for character in line.text.chars() {
        let id = font.glyph_id(character);
        if let Some(before) = previous {
            pen += scaled.kern(before, id);
        }
        let glyph = id.with_scale_and_position(size, ab_glyph::point(pen, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|dx, dy, coverage| {
                let px = bounds.min.x + dx as f32;
                let py = bounds.min.y + dy as f32;
                if px < 0.0 || py < 0.0 {
                    return;
                }
                let (px, py) = (px as u32, py as u32);
                if px >= canvas.width() || py >= canvas.height() {
                    return;
                }
                blend(canvas.get_pixel_mut(px, py), colour, coverage);
            });
        }
        pen += scaled.h_advance(id);
        previous = Some(id);
    }
}

/// Composite one source colour over one destination pixel.
///
/// `coverage` is the rasteriser's antialiasing, in 0..=1, and it multiplies
/// the colour's own alpha. Straight (non-premultiplied) alpha over an opaque
/// background, which is what the background always is here.
fn blend(target: &mut image::Rgba<u8>, source: [u8; 4], coverage: f32) {
    let alpha = (f32::from(source[3]) / 255.0) * coverage.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    for (channel, value) in target.0.iter_mut().take(3).enumerate() {
        let base = f32::from(*value);
        let over = f32::from(source[channel]);
        *value = (base + (over - base) * alpha).round().clamp(0.0, 255.0) as u8;
    }
    let base_alpha = f32::from(target.0[3]) / 255.0;
    target.0[3] = ((base_alpha + (1.0 - base_alpha) * alpha) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8;
}

/// A `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` colour as RGBA bytes.
fn colour(value: &str) -> [u8; 4] {
    let hex = value.trim().trim_start_matches('#');
    if !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return DEFAULT_INK;
    }
    let parse = |at: usize, len: usize| -> u8 {
        let slice = &hex[at..at + len];
        let doubled = if len == 1 {
            format!("{slice}{slice}")
        } else {
            slice.to_owned()
        };
        u8::from_str_radix(&doubled, 16).unwrap_or(0)
    };
    match hex.len() {
        3 => [parse(0, 1), parse(1, 1), parse(2, 1), 0xFF],
        4 => [parse(0, 1), parse(1, 1), parse(2, 1), parse(3, 1)],
        6 => [parse(0, 2), parse(2, 2), parse(4, 2), 0xFF],
        8 => [parse(0, 2), parse(2, 2), parse(4, 2), parse(6, 2)],
        // Not an error. A colour uf cannot read is a typo in one field, and
        // failing the build for it would be a worse trade than drawing the
        // card in the default — this is the one field where a wrong value is
        // obvious at a glance rather than subtly wrong.
        _ => DEFAULT_INK,
    }
}

/// The colour an unreadable one falls back to.
const DEFAULT_INK: [u8; 4] = [0x0B, 0x10, 0x20, 0xFF];
