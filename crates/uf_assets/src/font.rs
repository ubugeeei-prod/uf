//! Reading a font's real metrics, and the fallback face that matches them.
//!
//! # The mistake this exists to prevent
//!
//! A page paints with a fallback face and then swaps to the real one. The two
//! are different sizes, so every line of text moves, and the move happens after
//! the reader has started reading. `font-display: swap` chose that trade
//! deliberately — text the reader can read beats text that is not there — but
//! the shift is not part of the trade. It is avoidable, and CSS has had the
//! mechanism since 2020: `size-adjust`, `ascent-override`, `descent-override`
//! and `line-gap-override` on a second `@font-face` that names a *local* face
//! as its source. Scale the fallback until its metrics match the real font's
//! and the swap moves nothing.
//!
//! Computing those four numbers is the whole reason this module reads the font
//! binary at all. They are ratios between two fonts' metrics, so uf needs the
//! real font's `head`, `hhea` and `OS/2` tables, and a table of the same
//! numbers for whatever local face is being scaled.
//!
//! # What is read, and from where
//!
//! * `head.unitsPerEm` — the denominator for everything else.
//! * `hhea.ascender`, `hhea.descender`, `hhea.lineGap` — the line box. These
//!   rather than `OS/2`'s `sTypo*` pair, because they are what the overrides
//!   are defined against and what `capsize` and `fontaine` use; `sTypo*` is the
//!   fallback when a font leaves `hhea` at zero, which some CFF fonts do.
//! * `OS/2.xAvgCharWidth` — the width `size-adjust` is a ratio of. A font whose
//!   `OS/2` table is missing or too short for the field gets no `size-adjust`
//!   at all rather than a guessed one; see [`FontMetrics::fallback_for`].
//!
//! # Containers
//!
//! `.ttf`/`.otf` are read directly. `.woff` inflates each table with zlib and
//! `.woff2` brotli-decompresses one stream holding all of them. WOFF2's `glyf`
//! and `loca` transform is deliberately not implemented and does not need to
//! be: it touches those two tables and none of the three read here, so the
//! three come out of the stream unchanged.
//!
//! # What this module does not do
//!
//! It does not **subset**. Reading three fixed-size tables out of a container
//! and rebuilding a font from a glyph closure are different jobs by orders of
//! magnitude — `cmap` lookup, `GSUB`/`GPOS` closure, `glyf` component
//! recursion, `loca`/`hmtx` rebuilds, and a WOFF2 re-encode on the way out.
//! `uf` self-hosts the bytes the author supplied, at the size the author
//! supplied them. See `docs/security.md` and the issue this module was written
//! for.

use std::io::Read as _;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::name::hashed_name;
use crate::subset::SubsetMode;

#[cfg(test)]
pub(crate) mod tests;

/// Largest font file uf will read.
///
/// A font path can come from a dependency, and every byte of it is parsed, so
/// the work is bounded rather than trusted. No shipping font is close to this.
pub const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

/// Largest table directory uf will walk, in entries.
///
/// A real font has a few dozen tables. The bound is on the *declared* count,
/// which is the number a malformed file gets to choose.
const MAX_TABLES: usize = 512;

/// A font that could not be read.
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    /// The file could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file is larger than [`MAX_FONT_BYTES`].
    #[error("{path} is {bytes} bytes, larger than the {MAX_FONT_BYTES} byte limit for a font")]
    TooLarge {
        /// Path that was too large.
        path: Utf8PathBuf,
        /// Its size.
        bytes: u64,
    },
    /// The bytes are not a font this crate can read.
    #[error("{path} is not a font uf can read: {reason}")]
    Unreadable {
        /// Path that could not be parsed.
        path: Utf8PathBuf,
        /// What was wrong with it.
        reason: String,
    },
    /// The emitted file could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that failed.
        path: Utf8PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// How a font file is packaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FontContainer {
    /// A bare SFNT: `.ttf` or `.otf`.
    Sfnt,
    /// WOFF 1.0, each table zlib-compressed.
    Woff,
    /// WOFF 2.0, all tables in one brotli stream.
    Woff2,
}

impl FontContainer {
    /// The CSS `format()` token for this container.
    ///
    /// What goes inside `src: url(…) format(<here>)`, so a browser can skip a
    /// source it cannot use without fetching it.
    #[must_use]
    pub const fn css_format(self) -> &'static str {
        match self {
            Self::Sfnt => "truetype",
            Self::Woff => "woff",
            Self::Woff2 => "woff2",
        }
    }
}

/// The metrics that decide whether a font swap moves the page.
///
/// Font units, as the tables store them; [`unitsPerEm`](Self::units_per_em) is
/// the denominator that turns any of them into a fraction of the em.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontMetrics {
    /// `head.unitsPerEm`.
    pub units_per_em: u16,
    /// `hhea.ascender`, or `OS/2.sTypoAscender` when `hhea` is degenerate.
    pub ascent: i16,
    /// `hhea.descender`. Negative in every real font.
    pub descent: i16,
    /// `hhea.lineGap`.
    pub line_gap: i16,
    /// `OS/2.xAvgCharWidth`, when the table carries it.
    ///
    /// `None` is not zero and must not be treated as it: a ratio with an
    /// unknown denominator has no value, and the code that would have used it
    /// declines to emit `size-adjust` instead of emitting a wrong one.
    pub average_width: Option<i16>,
}

impl FontMetrics {
    /// The overrides that make `fallback` occupy the same space as this font.
    ///
    /// # The arithmetic
    ///
    /// ```text
    /// sizeAdjust      = (avgWidth(real) / upem(real)) / (avgWidth(local) / upem(local))
    /// ascentOverride  =  ascent(real)   / upem(real)  / sizeAdjust
    /// descentOverride = |descent(real)| / upem(real)  / sizeAdjust
    /// lineGapOverride =  lineGap(real)  / upem(real)  / sizeAdjust
    /// ```
    ///
    /// `size-adjust` scales the local face so a line of text is the same
    /// *width*; the three overrides then restore the line *box*, and each is
    /// divided by the adjust because CSS applies the overrides to the
    /// already-adjusted em.
    ///
    /// Returns `None` when either font's average width is unknown or zero.
    /// That is the honest answer and not a degraded one: without a width ratio
    /// there is nothing to scale by, and a fallback face carrying three
    /// overrides and no `size-adjust` matches the line box while getting the
    /// line length wrong, which moves text horizontally instead of vertically.
    #[must_use]
    pub fn fallback_for(&self, fallback: LocalFace) -> Option<FallbackFace> {
        let real_width = f64::from(self.average_width?);
        let local_width = f64::from(fallback.metrics.average_width?);
        if real_width <= 0.0 || local_width <= 0.0 {
            return None;
        }
        let real_upem = f64::from(self.units_per_em);
        let local_upem = f64::from(fallback.metrics.units_per_em);
        if real_upem <= 0.0 || local_upem <= 0.0 {
            return None;
        }
        let size_adjust = (real_width / real_upem) / (local_width / local_upem);
        if !size_adjust.is_finite() || size_adjust <= 0.0 {
            return None;
        }
        let per_em = |value: i16| f64::from(value) / real_upem / size_adjust;
        Some(FallbackFace {
            local: fallback.name.to_owned(),
            size_adjust: percent(size_adjust),
            ascent_override: percent(per_em(self.ascent)),
            descent_override: percent(per_em(self.descent).abs()),
            line_gap_override: percent(per_em(self.line_gap)),
        })
    }
}

/// A percentage, rounded to two decimals and written the way CSS wants it.
///
/// Two decimals because a third moves a 1000px line of text by a hundredth of
/// a pixel, and because a shorter string is fewer shipped bytes. Trailing
/// zeroes are trimmed so `100.00%` is `100%`.
fn percent(ratio: f64) -> String {
    let rounded = (ratio * 10_000.0).round() / 100.0;
    let mut text = format!("{rounded:.2}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    format!("{text}%")
}

/// A metric-matched `@font-face` over a face the reader already has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FallbackFace {
    /// The local face being scaled, for `src: local(…)`.
    pub local: String,
    /// `size-adjust`.
    pub size_adjust: String,
    /// `ascent-override`.
    pub ascent_override: String,
    /// `descent-override`.
    pub descent_override: String,
    /// `line-gap-override`.
    pub line_gap_override: String,
}

/// One local face uf knows the metrics of, so it can be scaled to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalFace {
    /// The family name to write into `src: local(…)`.
    pub name: &'static str,
    /// Its metrics.
    pub metrics: FontMetrics,
}

/// The local faces uf can compute a metric-matched fallback against.
///
/// # Where these numbers came from
///
/// Every one was read out of a real font file by [`read_metrics`] — the same
/// function that reads the project's font — rather than copied from a table
/// somewhere. The files were macOS's, under
/// `/System/Library/Fonts/Supplemental/`, and
/// `crates/uf_assets/src/font/tests.rs` re-reads any of them that exist on the
/// machine running the tests and fails if a number here disagrees. The
/// provenance is therefore checkable rather than asserted, which is the point:
/// both `xAvgCharWidth` values below were wrong when they were first written
/// from memory — Arial by 24% and Times New Roman by 25%, each of which would
/// have put every `size-adjust` computed against it out by that much — and the
/// test is what said so.
///
/// A "local" fallback is by definition whatever the reader's machine has under
/// that name, and the same family is not byte-identical across platforms, so
/// the numbers are close rather than exact everywhere. That is the nature of
/// the mechanism and not a defect in the table — a fallback scaled to within a
/// percent of the real face is the difference between a page that visibly
/// reflows and one that does not.
///
/// # Why Helvetica is not here
///
/// macOS ships it only as `Helvetica.ttc`, a collection holding several faces,
/// and nothing in the file says which of them a browser resolves `local(
/// "Helvetica")` to. uf would have to guess, and a guessed `size-adjust` is
/// worse than none: it moves the text confidently. A face uf cannot measure is
/// a face uf declines to scale — see [`FontAsset::fallback_declined`].
pub const LOCAL_FACES: &[LocalFace] = &[
    LocalFace {
        name: "Arial",
        metrics: FontMetrics {
            units_per_em: 2048,
            ascent: 1854,
            descent: -434,
            line_gap: 67,
            average_width: Some(904),
        },
    },
    LocalFace {
        name: "Times New Roman",
        metrics: FontMetrics {
            units_per_em: 2048,
            ascent: 1825,
            descent: -443,
            line_gap: 87,
            average_width: Some(821),
        },
    },
    LocalFace {
        name: "Courier New",
        metrics: FontMetrics {
            units_per_em: 2048,
            ascent: 1705,
            descent: -615,
            line_gap: 0,
            average_width: Some(1229),
        },
    },
];

/// The local face uf knows by that name, if it knows it.
#[must_use]
pub fn local_face(name: &str) -> Option<LocalFace> {
    LOCAL_FACES
        .iter()
        .find(|face| face.name.eq_ignore_ascii_case(name))
        .copied()
}

/// What a project asked uf to do with one font file.
#[derive(Debug, Clone)]
pub struct FontRequest<'a> {
    /// The font file, as the author wrote it.
    pub source: &'a Utf8Path,
    /// The `font-family` the face is declared under.
    pub family: &'a str,
    /// `font-weight`.
    pub weight: &'a str,
    /// `font-style`.
    pub style: &'a str,
    /// `font-display`.
    pub display: &'a str,
    /// The local face to scale into a metric-matched fallback, if any.
    pub fallback: Option<&'a str>,
    /// Where the self-hosted copy is written.
    pub out_dir: &'a Utf8Path,
    /// What the emitted file's name is joined to, to make the `src: url()`.
    ///
    /// The caller's, because this crate has no idea what base a build serves
    /// the directory under and guessing one is how a stylesheet comes out
    /// right in development and broken behind a CDN path. A dev server passes
    /// its own prefix and a build passes the bundler's; the rule of the
    /// stylesheet is the same either way.
    pub base_url: &'a str,
    /// How the face should be cut down before it is hosted.
    ///
    /// [`SubsetMode::Off`] is the default everywhere and the only value that
    /// cannot change what a page is able to render. See [`crate::subset`].
    pub subset: SubsetMode,
    /// Whether the emitted stylesheet's primary face should be preloaded.
    ///
    /// Exactly one face is ever marked, even when the family was split into
    /// eight: a preload for every bucket downloads the whole family up front,
    /// which is what the split existed to stop.
    pub preload: bool,
}

/// A self-hosted font, with the CSS that declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontAsset {
    /// The primary emitted file's name, content-hashed.
    ///
    /// The one a page preloads and the one a single-file import has. When the
    /// family was split, the rest are in [`faces`](Self::faces) — this is the
    /// bucket covering Latin, because that is the text a page paints first.
    pub file: String,
    /// Its media type.
    pub mime: String,
    /// Its size on disk.
    pub bytes: u64,
    /// What the author supplied, for comparison with the sum of the faces.
    pub source_bytes: u64,
    /// The family the `@font-face` declares.
    pub family: String,
    /// The family name of the metric-matched fallback, when there is one.
    ///
    /// A page puts this after `family` in its `font-family` list, which is the
    /// only way the overrides ever apply.
    pub fallback_family: Option<String>,
    /// The container the bytes are in.
    pub container: FontContainer,
    /// What the tables said.
    pub metrics: FontMetrics,
    /// The overrides that stop the swap moving the page, when computable.
    pub fallback: Option<FallbackFace>,
    /// Why there is no metric-matched fallback, when there is not one.
    ///
    /// Absent when [`fallback`](Self::fallback) is present. A reason rather
    /// than silence: a project that asked for a fallback and got none has to
    /// be able to find out why without reading this crate.
    pub fallback_declined: Option<String>,
    /// Every file emitted for this import, widest coverage first.
    ///
    /// One entry for a font hosted as supplied; one per bucket for a family
    /// split by [`SubsetMode::Ranges`]. A caller that only wants a URL reads
    /// [`file`](Self::file); a caller emitting the files reads this.
    pub faces: Vec<EmittedFace>,
    /// The subsetting that was applied, when any was.
    pub subset: Option<String>,
    /// Why the font was hosted whole, when a subset was asked for and refused.
    ///
    /// Present exactly when a subset was requested and not produced. The font
    /// still works — this is the sentence that says it is the entire font, and
    /// what would have to change for it not to be.
    pub subset_declined: Option<String>,
    /// The stylesheet: every real face, and the matched fallback after them.
    pub css: String,
}

/// One file this import put in the output directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmittedFace {
    /// The file's name, content-hashed.
    pub file: String,
    /// Its media type.
    pub mime: String,
    /// Its size on disk.
    pub bytes: u64,
    /// The container it is in.
    pub container: FontContainer,
    /// Which script bucket it is, when the family was split.
    pub bucket: Option<String>,
    /// The exact `unicode-range` of what is in it, when it is a bucket.
    ///
    /// Computed from the subset's own `cmap`, so it describes the file rather
    /// than the script the file was cut for.
    pub unicode_range: Option<String>,
    /// Whether a page should preload this one.
    pub preload: bool,
}

/// Read one font, emit its files under content-hashed names, and describe it.
///
/// One file when the font is hosted as supplied, one per script bucket when
/// [`SubsetMode::Ranges`] split it. A subset that could not be produced is not
/// an error: the whole font is hosted and
/// [`FontAsset::subset_declined`] says why.
///
/// # Errors
///
/// [`FontError`] when the file cannot be read, is larger than
/// [`MAX_FONT_BYTES`], is not a font this crate understands, or cannot be
/// written to `out_dir`.
pub fn self_host(request: &FontRequest<'_>) -> Result<FontAsset, FontError> {
    let bytes = read_bounded(request.source)?;
    let (container, metrics) = read_metrics(request.source, &bytes)?;

    let stem = request.source.file_stem().unwrap_or("font");
    std::fs::create_dir_all(request.out_dir).map_err(|source| FontError::Write {
        path: request.out_dir.to_owned(),
        source,
    })?;

    // The subset is attempted first because whether it succeeded decides what
    // is written. A refusal is a sentence and not an error: the font still has
    // to be hosted, and a build that failed because a face could not be cut
    // down would be a build that failed for an optimisation.
    let (mut faces, subset_declined) = match &request.subset {
        SubsetMode::Off => (Vec::new(), None),
        mode => match subset_faces(request, &bytes, stem, mode) {
            Ok(emitted) => (emitted, None),
            Err(reason) => (Vec::new(), Some(reason)),
        },
    };
    let subset = if faces.is_empty() {
        None
    } else {
        Some(match &request.subset {
            SubsetMode::Ranges => String::from("ranges"),
            SubsetMode::Text(_) => String::from("text"),
            SubsetMode::Off => unreachable!("Off produces no faces"),
        })
    };

    if faces.is_empty() {
        let extension = request
            .source
            .extension()
            .unwrap_or(match container {
                FontContainer::Sfnt => "ttf",
                FontContainer::Woff => "woff",
                FontContainer::Woff2 => "woff2",
            })
            .to_ascii_lowercase();
        let file = hashed_name(stem, &bytes, &[request.family.as_bytes()], &extension);
        let target = request.out_dir.join(&file);
        // Written every time rather than only when absent: the name is a hash
        // of the bytes, so a rewrite is the same bytes, and a half-written file
        // left by a killed build would otherwise be served forever under a
        // name that says it is complete.
        std::fs::write(&target, &bytes).map_err(|source| FontError::Write {
            path: target.clone(),
            source,
        })?;
        faces.push(EmittedFace {
            mime: uf_bundle::content_type(Utf8Path::new(&file)).to_owned(),
            file,
            bytes: bytes.len() as u64,
            container,
            bucket: None,
            unicode_range: None,
            preload: request.preload,
        });
    }

    let (fallback, fallback_declined) = match request.fallback {
        None => (None, None),
        Some(name) => match local_face(name) {
            None => (
                None,
                Some(format!(
                    "uf has no metrics for the local face {name:?}; known faces are {}",
                    LOCAL_FACES
                        .iter()
                        .map(|face| face.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ),
            Some(face) => match metrics.fallback_for(face) {
                Some(matched) => (Some(matched), None),
                None => (
                    None,
                    Some(format!(
                        "{} has no OS/2 xAvgCharWidth, so there is no width ratio to scale \
                         {name:?} by",
                        request.source
                    )),
                ),
            },
        },
    };

    let fallback_family = fallback
        .as_ref()
        .map(|_| format!("{} Fallback", request.family));
    let css = stylesheet(request, &faces, fallback.as_ref());
    let primary = faces
        .iter()
        .position(|face| face.preload)
        .unwrap_or_default();

    Ok(FontAsset {
        file: faces[primary].file.clone(),
        mime: faces[primary].mime.clone(),
        bytes: faces[primary].bytes,
        source_bytes: bytes.len() as u64,
        family: request.family.to_owned(),
        fallback_family,
        container: faces[primary].container,
        metrics,
        fallback,
        fallback_declined,
        faces,
        subset,
        subset_declined,
        css,
    })
}

/// Cut the font down and write one file per piece.
///
/// `Err` is a sentence for [`FontAsset::subset_declined`], not a failure: every
/// reason a subset cannot be produced leaves a perfectly serviceable font.
fn subset_faces(
    request: &FontRequest<'_>,
    bytes: &[u8],
    stem: &str,
    mode: &SubsetMode,
) -> Result<Vec<EmittedFace>, String> {
    let sfnt = to_sfnt(request.source, bytes)?;
    let plan = crate::subset::plan(&sfnt, mode)?;

    let mode_key: &[u8] = match mode {
        SubsetMode::Ranges => b"ranges",
        SubsetMode::Text(text) => text.as_bytes(),
        SubsetMode::Off => b"",
    };
    let mut faces = Vec::with_capacity(plan.faces.len());
    for (index, cut) in plan.faces.iter().enumerate() {
        // The digest covers the source bytes, the family, the mode and the
        // bucket. Not the subset's own output: `skera`'s byte layout is
        // allowed to move between versions, and a name that tracked it would
        // invalidate every cached face in every project on an upgrade that
        // changed nothing a reader can see.
        let file = crate::name::hashed_name_with_suffix(
            stem,
            bytes,
            &[request.family.as_bytes(), mode_key, cut.bucket.as_bytes()],
            &format!("-{}", cut.bucket),
            "woff",
        );
        let target = request.out_dir.join(&file);
        std::fs::write(&target, &cut.bytes)
            .map_err(|error| format!("failed to write {target}: {error}"))?;
        faces.push(EmittedFace {
            mime: uf_bundle::content_type(Utf8Path::new(&file)).to_owned(),
            file,
            bytes: cut.bytes.len() as u64,
            container: FontContainer::Woff,
            bucket: Some(cut.bucket.clone()),
            unicode_range: Some(cut.unicode_range.clone()),
            preload: request.preload && index == plan.primary,
        });
    }
    Ok(faces)
}

/// The `@font-face` rules for one self-hosted font.
///
/// One rule per emitted face, each carrying its own `unicode-range` when the
/// family was split, and then the metric-matched fallback. The split rules all
/// declare the same `font-family`: that is what makes them one face to CSS,
/// with the browser choosing between the files by which characters the page
/// actually renders.
fn stylesheet(
    request: &FontRequest<'_>,
    faces: &[EmittedFace],
    fallback: Option<&FallbackFace>,
) -> String {
    let family = css_string(request.family);
    let mut css = String::new();
    for face in faces {
        css.push_str(&format!(
            "@font-face{{font-family:{family};font-style:{style};font-weight:{weight};\
             font-display:{display};src:url({url}) format({format});",
            url = css_string(&format!("{}{}", request.base_url, face.file)),
            style = request.style,
            weight = request.weight,
            display = request.display,
            format = css_string(face.container.css_format()),
        ));
        if let Some(range) = face.unicode_range.as_deref() {
            css.push_str(&format!("unicode-range:{range};"));
        }
        css.push('}');
    }
    if let Some(matched) = fallback {
        // A second face, named so a page can list it after the real one. It
        // has no `src: url()` at all: the bytes are already on the reader's
        // machine, and a fallback that had to be downloaded would be pointless.
        //
        // No `unicode-range` either, even when the real family was split: the
        // fallback is what the reader sees for *any* character until the right
        // bucket lands, so restricting it to one bucket's range would leave
        // every other character with no metric matching at all.
        css.push_str(&format!(
            "@font-face{{font-family:{fallback_family};font-style:{style};font-weight:{weight};\
             src:local({local});size-adjust:{size_adjust};ascent-override:{ascent};\
             descent-override:{descent};line-gap-override:{line_gap};}}",
            fallback_family = css_string(&format!("{} Fallback", request.family)),
            style = request.style,
            weight = request.weight,
            local = css_string(&matched.local),
            size_adjust = matched.size_adjust,
            ascent = matched.ascent_override,
            descent = matched.descent_override,
            line_gap = matched.line_gap_override,
        ));
    }
    css
}

/// A CSS string literal.
///
/// Family names come from `uf.config.js` and from imports, so they are the
/// project's text rather than uf's: quoted, with the two characters that can
/// end a CSS string escaped. Without this a family name containing a quote
/// would close the string and the rest of the rule would be whatever the name
/// said.
fn css_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' | '\\' => {
                out.push('\\');
                out.push(character);
            }
            // A newline cannot appear in a CSS string at all; escaped as a
            // code point so the rule stays one rule.
            '\n' => out.push_str("\\a "),
            _ => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Read a font file, refusing one larger than [`MAX_FONT_BYTES`].
fn read_bounded(path: &Utf8Path) -> Result<Vec<u8>, FontError> {
    let metadata = std::fs::metadata(path).map_err(|source| FontError::Read {
        path: path.to_owned(),
        source,
    })?;
    if metadata.len() > MAX_FONT_BYTES {
        return Err(FontError::TooLarge {
            path: path.to_owned(),
            bytes: metadata.len(),
        });
    }
    std::fs::read(path).map_err(|source| FontError::Read {
        path: path.to_owned(),
        source,
    })
}

/// The container and the metrics of one font's bytes.
///
/// # Errors
///
/// [`FontError::Unreadable`] when the bytes are not a font, are a container
/// this crate does not read, or are truncated inside a table it needs.
pub fn read_metrics(
    path: &Utf8Path,
    bytes: &[u8],
) -> Result<(FontContainer, FontMetrics), FontError> {
    let unreadable = |reason: String| FontError::Unreadable {
        path: path.to_owned(),
        reason,
    };
    let tag = bytes
        .get(0..4)
        .ok_or_else(|| unreadable("empty file".into()))?;
    let (container, tables) = match tag {
        b"wOFF" => (FontContainer::Woff, woff_tables(bytes).map_err(unreadable)?),
        b"wOF2" => (
            FontContainer::Woff2,
            woff2_tables(bytes).map_err(unreadable)?,
        ),
        b"ttcf" => {
            return Err(unreadable(
                "a TrueType collection holds several fonts and does not say which one this is; \
                 extract the face you want first"
                    .into(),
            ));
        }
        _ => (FontContainer::Sfnt, sfnt_tables(bytes).map_err(unreadable)?),
    };
    let metrics = metrics_from(&tables).map_err(unreadable)?;
    Ok((container, metrics))
}

/// One table's bytes, by its four-character tag.
type Tables = Vec<([u8; 4], Vec<u8>)>;

/// The font's bytes as a bare SFNT, whatever container they arrived in.
///
/// Two things in this crate need a font as an SFNT rather than as tables:
/// [`crate::og`] hands the bytes to `ab_glyph`, and [`crate::subset`] hands
/// them to `skera`. Both are upstream readers that take a whole font, so a
/// `.woff` has to be reassembled into one rather than picked apart.
///
/// # Errors
///
/// A message, when the container is one this cannot flatten:
///
/// * **WOFF2** — its `glyf` and `loca` are stored in a *transformed* shape and
///   this crate deliberately does not reverse the transform. [`read_metrics`]
///   does not need to, because the three tables it reads are never
///   transformed; a subsetter and a rasteriser both do. Reversing it means a
///   second brotli major version and a bit-level point decoder in the
///   dependency tree to reconstruct outlines that are then immediately thrown
///   away — so the answer is to ask for the `.ttf` the `.woff2` was built
///   from, which is the file a build wanted anyway.
/// * **a TrueType collection** — several fonts in one file, with nothing
///   saying which one was meant.
pub(crate) fn to_sfnt(path: &Utf8Path, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let tag = bytes.get(0..4).ok_or_else(|| String::from("empty file"))?;
    match tag {
        b"wOF2" => Err(format!(
            "{path} is WOFF2, whose glyf table is stored in a transformed form uf does not \
             reverse. Point this at the .ttf or .otf the .woff2 was built from"
        )),
        b"ttcf" => Err(format!(
            "{path} is a TrueType collection and does not say which of its faces was meant; \
             extract the one you want first"
        )),
        b"wOFF" => {
            let flavor = be_u32(bytes, 4).ok_or_else(|| String::from("truncated WOFF header"))?;
            let tables = woff_tables(bytes)?;
            Ok(pack_sfnt(flavor, &tables))
        }
        _ => {
            // Validated rather than trusted: `sfnt_tables` is what decides
            // whether these bytes are an SFNT at all, and its error is the one
            // worth reporting.
            sfnt_tables(bytes)?;
            Ok(bytes.to_vec())
        }
    }
}

/// Lay a set of tables out as a bare SFNT.
///
/// The directory is sorted by tag, every table is padded to a four-byte
/// boundary, and both checksums are computed — a reader is entitled to check
/// them, and a font that fails its own checksum is a font some tool will
/// refuse for reasons that take a day to find.
pub(crate) fn pack_sfnt(flavor: u32, tables: &Tables) -> Vec<u8> {
    let mut sorted: Vec<&([u8; 4], Vec<u8>)> = tables.iter().collect();
    sorted.sort_by_key(|(tag, _)| *tag);

    let count = sorted.len();
    let directory = 12 + count * 16;
    let mut body = Vec::new();
    let mut records: Vec<([u8; 4], u32, u32, u32)> = Vec::with_capacity(count);
    for (tag, data) in &sorted {
        let offset = directory + body.len();
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        records.push((
            *tag,
            checksum(data),
            u32::try_from(offset).unwrap_or(u32::MAX),
            u32::try_from(data.len()).unwrap_or(u32::MAX),
        ));
    }

    let mut out = Vec::with_capacity(directory + body.len());
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    // The three fields a binary search over the directory would use. Every
    // reader ignores them and computes its own; they are written correctly
    // anyway because a validator does not ignore them.
    let entry_selector = usize::BITS - 1 - count.max(1).leading_zeros();
    let search_range = (1u32 << entry_selector) * 16;
    out.extend_from_slice(&(search_range as u16).to_be_bytes());
    out.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    out.extend_from_slice(&((count as u32 * 16).saturating_sub(search_range) as u16).to_be_bytes());
    for (tag, sum, offset, length) in &records {
        out.extend_from_slice(tag);
        out.extend_from_slice(&sum.to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
        out.extend_from_slice(&length.to_be_bytes());
    }
    out.extend_from_slice(&body);

    // `head.checkSumAdjustment` is 0xB1B0AFBA minus the checksum of the whole
    // file computed with that field itself zeroed, which is why it is written
    // last and why the field was left at whatever it was.
    if let Some(head_at) = records
        .iter()
        .position(|(tag, _, _, _)| tag == b"head")
        .map(|index| records[index].2 as usize)
        && out.len() >= head_at + 12
    {
        out[head_at + 8..head_at + 12].copy_from_slice(&0u32.to_be_bytes());
        let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
        out[head_at + 8..head_at + 12].copy_from_slice(&adjustment.to_be_bytes());
    }
    out
}

/// The tables of an SFNT, for a test that has to take one apart.
#[cfg(test)]
pub(crate) fn sfnt_tables_for_test(bytes: &[u8]) -> Tables {
    sfnt_tables(bytes).expect("the fixture is an sfnt")
}

/// An SFNT table checksum: big-endian `u32` words, wrapping, zero-padded.
fn checksum(data: &[u8]) -> u32 {
    let mut sum = 0u32;
    let (words, rest) = data.as_chunks::<4>();
    for word in words {
        sum = sum.wrapping_add(u32::from_be_bytes(*word));
    }
    if !rest.is_empty() {
        let mut word = [0u8; 4];
        word[..rest.len()].copy_from_slice(rest);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
    }
    sum
}

/// Pack an SFNT as WOFF 1.0.
///
/// # Why WOFF 1.0 and not WOFF2
///
/// This is what a subsetted face is emitted as, and the choice is between the
/// container uf can write correctly and the one the web prefers. WOFF 1.0 is
/// a table directory and one zlib stream per table — [`flate2`] is already in
/// this crate for reading them, and the result round-trips through
/// [`woff_tables`] in this crate's own tests. WOFF2 needs its `glyf`
/// transform, or a null-transform encoding whose only proof of correctness
/// would be a browser uf cannot run in CI.
///
/// The container is the smaller half of the decision anyway: brotli beats
/// zlib by roughly a fifth on a font, and subsetting a family to the range a
/// page uses removes rather more than that. Support is universal — WOFF 1.0
/// is the format every browser that has `@font-face` at all can read.
pub(crate) fn pack_woff(sfnt: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Write as _;

    let flavor = be_u32(sfnt, 0).ok_or_else(|| String::from("truncated font"))?;
    let tables = sfnt_tables(sfnt)?;
    let count = tables.len();
    let directory = 44 + count * 20;

    let mut body = Vec::new();
    let mut records: Vec<[u8; 20]> = Vec::with_capacity(count);
    for (tag, data) in &tables {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        encoder
            .write_all(data)
            .and_then(|()| encoder.finish())
            .map_err(|error| format!("a WOFF table would not deflate: {error}"))
            .map(|compressed| {
                // WOFF says a table that did not get smaller is stored raw,
                // and says so by making the two lengths equal. Storing a
                // larger deflate stream would be legal and pointless.
                let stored: &[u8] = if compressed.len() < data.len() {
                    &compressed
                } else {
                    data
                };
                let offset = directory + body.len();
                body.extend_from_slice(stored);
                while body.len() % 4 != 0 {
                    body.push(0);
                }
                let mut record = [0u8; 20];
                record[0..4].copy_from_slice(tag);
                record[4..8].copy_from_slice(&(offset as u32).to_be_bytes());
                record[8..12].copy_from_slice(&(stored.len() as u32).to_be_bytes());
                record[12..16].copy_from_slice(&(data.len() as u32).to_be_bytes());
                record[16..20].copy_from_slice(&checksum(data).to_be_bytes());
                records.push(record);
            })?;
    }

    let mut out = Vec::with_capacity(directory + body.len());
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&((directory + body.len()) as u32).to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(sfnt.len() as u32).to_be_bytes());
    // The two version fields, then the five words describing the optional
    // metadata and private blocks — offset, compressed length and original
    // length for the metadata, offset and length for the private data — none
    // of which uf writes. Forty-four bytes of header in total, which is what
    // the first record's offset is measured from.
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&[0u8; 20]);
    for record in &records {
        out.extend_from_slice(record);
    }
    out.extend_from_slice(&body);
    Ok(out)
}

fn table<'a>(tables: &'a Tables, tag: &[u8; 4]) -> Option<&'a [u8]> {
    tables
        .iter()
        .find(|(name, _)| name == tag)
        .map(|(_, data)| data.as_slice())
}

fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    data.get(at..at + 2)
        .map(|slice| u16::from_be_bytes([slice[0], slice[1]]))
}

fn be_i16(data: &[u8], at: usize) -> Option<i16> {
    be_u16(data, at).map(|value| value as i16)
}

fn be_u32(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// Pull the four numbers out of `head`, `hhea` and `OS/2`.
fn metrics_from(tables: &Tables) -> Result<FontMetrics, String> {
    let head = table(tables, b"head").ok_or("the font has no head table")?;
    let units_per_em = be_u16(head, 18).ok_or("the head table is truncated")?;
    if units_per_em == 0 {
        return Err(String::from("the font declares unitsPerEm 0"));
    }

    let os2 = table(tables, b"OS/2");
    // `hhea` first, `OS/2.sTypo*` when `hhea` is degenerate — which a few CFF
    // fonts are, and a zero line box is worse than the other table's answer.
    let hhea = table(tables, b"hhea");
    let (ascent, descent, line_gap) = match hhea {
        Some(data) => {
            let ascent = be_i16(data, 4).ok_or("the hhea table is truncated")?;
            let descent = be_i16(data, 6).ok_or("the hhea table is truncated")?;
            let line_gap = be_i16(data, 8).ok_or("the hhea table is truncated")?;
            if ascent == 0 && descent == 0 {
                typo_metrics(os2).ok_or("hhea is empty and OS/2 has no usable metrics")?
            } else {
                (ascent, descent, line_gap)
            }
        }
        None => typo_metrics(os2).ok_or("the font has neither hhea nor a usable OS/2 table")?,
    };

    Ok(FontMetrics {
        units_per_em,
        ascent,
        descent,
        line_gap,
        average_width: os2.and_then(|data| be_i16(data, 2)),
    })
}

fn typo_metrics(os2: Option<&[u8]>) -> Option<(i16, i16, i16)> {
    let data = os2?;
    Some((be_i16(data, 68)?, be_i16(data, 70)?, be_i16(data, 72)?))
}

/// The table directory of a bare SFNT.
fn sfnt_tables(bytes: &[u8]) -> Result<Tables, String> {
    let version = be_u32(bytes, 0).ok_or("not a font")?;
    // 0x00010000 is TrueType outlines, `OTTO` is CFF, `true` is Apple's older
    // spelling of the first.
    if version != 0x0001_0000 && &bytes[0..4] != b"OTTO" && &bytes[0..4] != b"true" {
        return Err(format!("unrecognised sfnt version {version:#010x}"));
    }
    let count = be_u16(bytes, 4).ok_or("truncated table directory")? as usize;
    if count > MAX_TABLES {
        return Err(format!("{count} tables is more than uf will walk"));
    }
    let mut tables = Tables::with_capacity(count);
    for index in 0..count {
        let at = 12 + index * 16;
        let tag = bytes
            .get(at..at + 4)
            .ok_or("truncated table directory")?
            .try_into()
            .map_err(|_| "truncated table directory")?;
        let offset = be_u32(bytes, at + 8).ok_or("truncated table directory")? as usize;
        let length = be_u32(bytes, at + 12).ok_or("truncated table directory")? as usize;
        // A table that runs off the end is skipped rather than fatal: the three
        // this crate reads are checked for by name afterwards, and a font with
        // a broken table uf never opens is still a font uf can host.
        if let Some(data) = bytes.get(offset..offset.saturating_add(length)) {
            tables.push((tag, data.to_vec()));
        }
    }
    Ok(tables)
}

/// The table directory of a WOFF 1.0 file, each table inflated.
fn woff_tables(bytes: &[u8]) -> Result<Tables, String> {
    let count = be_u16(bytes, 12).ok_or("truncated WOFF header")? as usize;
    if count > MAX_TABLES {
        return Err(format!("{count} tables is more than uf will walk"));
    }
    let mut tables = Tables::with_capacity(count);
    for index in 0..count {
        let at = 44 + index * 20;
        let tag: [u8; 4] = bytes
            .get(at..at + 4)
            .ok_or("truncated WOFF directory")?
            .try_into()
            .map_err(|_| "truncated WOFF directory")?;
        let offset = be_u32(bytes, at + 4).ok_or("truncated WOFF directory")? as usize;
        let compressed = be_u32(bytes, at + 8).ok_or("truncated WOFF directory")? as usize;
        let original = be_u32(bytes, at + 12).ok_or("truncated WOFF directory")? as usize;
        let Some(raw) = bytes.get(offset..offset.saturating_add(compressed)) else {
            continue;
        };
        // WOFF stores a table uncompressed when compression did not help, and
        // says so by making the two lengths equal.
        let data = if compressed == original {
            raw.to_vec()
        } else {
            let mut out = Vec::with_capacity(original.min(MAX_FONT_BYTES as usize));
            flate2::read::ZlibDecoder::new(raw)
                .take(MAX_FONT_BYTES)
                .read_to_end(&mut out)
                .map_err(|error| format!("a WOFF table would not inflate: {error}"))?;
            out
        };
        tables.push((tag, data));
    }
    Ok(tables)
}

/// The known table tags of WOFF2, in the order the flag byte indexes them.
///
/// A directory entry spends six bits naming its table, and 63 means "the tag
/// follows in full". Everything else is an index into this list, which is the
/// order the WOFF2 specification fixes.
const WOFF2_TAGS: [&[u8; 4]; 63] = [
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"cvt ", b"fpgm",
    b"glyf", b"loca", b"prep", b"CFF ", b"VORG", b"EBDT", b"EBLC", b"gasp", b"hdmx", b"kern",
    b"LTSH", b"PCLT", b"VDMX", b"vhea", b"vmtx", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"EBSC",
    b"JSTF", b"MATH", b"CBDT", b"CBLC", b"COLR", b"CPAL", b"SVG ", b"sbix", b"acnt", b"avar",
    b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar", b"gvar", b"hsty",
    b"just", b"lcar", b"mort", b"morx", b"opbd", b"prop", b"trak", b"Zapf", b"Silf", b"Glat",
    b"Gloc", b"Feat", b"Sill",
];

/// The tables of a WOFF 2.0 file.
///
/// The directory is read from the uncompressed header and the tables come out
/// of one brotli stream, laid end to end in directory order. `glyf` and `loca`
/// may be *transformed* — stored in a different shape than the font they
/// rebuild to — and this does not untransform them; it does not need to, since
/// the three tables it is here for are never transformed, and their lengths in
/// the stream are the ones the directory gives.
fn woff2_tables(bytes: &[u8]) -> Result<Tables, String> {
    let count = be_u16(bytes, 12).ok_or("truncated WOFF2 header")? as usize;
    if count > MAX_TABLES {
        return Err(format!("{count} tables is more than uf will walk"));
    }
    let total = be_u32(bytes, 16).ok_or("truncated WOFF2 header")? as usize;
    if total as u64 > MAX_FONT_BYTES {
        return Err(format!(
            "the WOFF2 declares {total} bytes of tables, more than uf will decompress"
        ));
    }

    let mut cursor = 48;
    let mut directory: Vec<([u8; 4], usize)> = Vec::with_capacity(count);
    for _ in 0..count {
        let flags = *bytes.get(cursor).ok_or("truncated WOFF2 directory")?;
        cursor += 1;
        let index = usize::from(flags & 0x3f);
        let tag: [u8; 4] = if index == 63 {
            let raw = bytes
                .get(cursor..cursor + 4)
                .ok_or("truncated WOFF2 directory")?;
            cursor += 4;
            raw.try_into().map_err(|_| "truncated WOFF2 directory")?
        } else {
            *WOFF2_TAGS[index]
        };
        // The original length always follows; a transformed table adds a
        // transformed length after it, which is the one that occupies the
        // stream.
        let original = read_base128(bytes, &mut cursor)?;
        let transform = (flags >> 6) & 0x03;
        let transformed = if is_transformed(&tag, transform) {
            read_base128(bytes, &mut cursor)?
        } else {
            original
        };
        directory.push((tag, transformed as usize));
    }

    let compressed = bytes.get(cursor..).ok_or("truncated WOFF2 stream")?;
    let mut stream = Vec::with_capacity(total);
    brotli::Decompressor::new(compressed, 4096)
        .take(MAX_FONT_BYTES)
        .read_to_end(&mut stream)
        .map_err(|error| format!("the WOFF2 table stream would not decompress: {error}"))?;

    let mut tables = Tables::with_capacity(count);
    let mut at = 0usize;
    for (tag, length) in directory {
        let end = at.saturating_add(length);
        let Some(data) = stream.get(at..end) else {
            // The stream ended early. Everything before this point is still
            // good, and the caller checks for the tables it needs by name.
            break;
        };
        tables.push((tag, data.to_vec()));
        at = end;
    }
    Ok(tables)
}

/// Whether this table's bytes in the stream are a transform of the table.
///
/// Transform 0 means "transformed" for `glyf` and `loca` and "not transformed"
/// for everything else — the one place WOFF2 gives the same number two
/// meanings, and the reason this is a function rather than a comparison.
fn is_transformed(tag: &[u8; 4], transform: u8) -> bool {
    if tag == b"glyf" || tag == b"loca" {
        transform == 0
    } else {
        transform != 0
    }
}

/// WOFF2's `UIntBase128`: base-128, big-endian, high bit continues.
fn read_base128(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let mut value: u32 = 0;
    for index in 0..5 {
        let byte = *bytes.get(*cursor).ok_or("truncated WOFF2 directory")?;
        *cursor += 1;
        // Leading zeroes are not allowed to pad the encoding, and the value
        // must fit in 32 bits: both are spec requirements, and both are how a
        // crafted file would otherwise make this loop disagree with a real
        // WOFF2 reader about where the next entry starts.
        if index == 0 && byte == 0x80 {
            return Err(String::from("a WOFF2 length has a leading zero"));
        }
        if value & 0xfe00_0000 != 0 {
            return Err(String::from("a WOFF2 length overflows 32 bits"));
        }
        value = (value << 7) | u32::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(String::from("a WOFF2 length is longer than five bytes"))
}
