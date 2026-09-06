//! What a project declares about its images and fonts.
//!
//! These live here rather than in `uf_config` for the reason `BundleBudgets`
//! lives in `uf_bundle`: the crate that acts on a setting owns its shape, and
//! `uf_config` re-exports it. A setting whose only definition is in the config
//! crate is a setting the crate that reads it has to agree with by hand.
//!
//! Every key here is uf's own. None of them is a second name for something
//! Vite already names, which is the line `docs/red-lines.md` draws around
//! re-declaring an upstream tool's schema — Vite has no image pipeline to
//! re-declare.

use serde::{Deserialize, Serialize};

/// The widths uf emits when a project does not say.
///
/// Two device-pixel-ratio ladders rather than one: 640/828/1200/1920 is the
/// common CSS-pixel set at 1x, and 750/1080/1440 covers the same layouts on the
/// phones that report 2x and 3x. A project with one fixed-width column should
/// replace the list rather than keep paying for the ones it never serves.
pub const DEFAULT_WIDTHS: &[u32] = &[640, 750, 828, 1080, 1200, 1440, 1920];

/// The JPEG quality uf encodes at when a project does not say.
///
/// 75 is where the artefacts stop being visible on a photograph at the sizes
/// above while the file is a fraction of a visually lossless encode.
pub const DEFAULT_QUALITY: u8 = 75;

/// What a project declares about its images.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ImagesConfig {
    /// Whether imported images go through the pipeline at all.
    pub enabled: bool,
    /// The widths a layout asks for.
    pub widths: Vec<u32>,
    /// JPEG quality, 1–100.
    pub quality: u8,
    /// Whether to generate the blur placeholder.
    pub placeholder: bool,
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            widths: DEFAULT_WIDTHS.to_vec(),
            quality: DEFAULT_QUALITY,
            placeholder: true,
        }
    }
}

/// What a project declares about its fonts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FontsConfig {
    /// Whether imported fonts are self-hosted and declared.
    pub enabled: bool,
    /// The `font-display` every generated `@font-face` carries.
    ///
    /// `swap` by default: text the reader can read immediately, with the
    /// reflow that usually costs removed by the metric-matched fallback rather
    /// than by making them wait for it.
    pub display: String,
    /// The local face a metric-matched fallback is scaled from.
    ///
    /// One of `crates/uf_assets::font::LOCAL_FACES`. A project whose font is a
    /// serif should say so here, because scaling Arial to match a serif matches
    /// the line box and not the texture.
    pub fallback: String,
}

impl Default for FontsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            display: String::from("swap"),
            fallback: String::from("Arial"),
        }
    }
}
