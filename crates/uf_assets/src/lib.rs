#![deny(missing_docs)]
//! Image and font transformation for uniflowed.
//!
//! `@uniflowed/web`'s `Image` and `Font` render the right elements. This crate
//! is the work behind them: the decode, the resize, the re-encode, the content
//! hash, and the font metrics that decide whether a page moves when a face
//! swaps in. Without it those components are markup, and markup is something a
//! project can write by hand.
//!
//! - [`image`] decodes a source once and writes a variant at every width a
//!   layout asks for, in the source's own format and in WebP when the WebP is
//!   actually smaller. It also produces the blur placeholder, which the decode
//!   has already paid for.
//! - [`font`] reads `head`, `hhea` and `OS/2` out of a TTF, OTF, WOFF or WOFF2,
//!   copies the file under a content-hashed name, and emits the `@font-face`
//!   pair — the real face, and a `local()` fallback carrying the `size-adjust`
//!   and the three overrides that make the swap invisible.
//! - [`subset`] cuts a font down to the characters a page uses, or splits its
//!   coverage into `unicode-range` buckets so a reader downloads the script
//!   they are reading rather than the family. The work is `skera`'s, Google
//!   Fonts' Rust port of `hb-subset`; the decision about what to keep is uf's.
//! - [`icon`] turns an SVG into a `<symbol>` a component can reference, and
//!   assembles one sprite from the icons a build actually reached — which is
//!   the thing a runtime icon library cannot do, because it does not know
//!   which of its icons you used.
//! - [`og`] draws an Open Graph card from a declared template. It is
//!   deliberately not a renderer; read its documentation before expecting JSX.
//! - [`name`] is the one place a name is minted, so a file's name changes
//!   exactly when its bytes do.
//! - [`cache`] is what stops the second build doing any of it again.
//! - [`config`] is what a project declares about all of them.
//!
//! # One pipeline, two schedules
//!
//! `uf build` and `uf dev` call the same functions with the same parameters and
//! write to the same directory layout. They are not two implementations that
//! have to be kept in step: a build reads the emitted files and hands them to
//! the bundler, a dev server reads the same emitted files and serves them, and
//! a variant already on disk under its content-hashed name is reused by both.
//! That is what makes the markup a dev server renders the markup a build
//! writes.
//!
//! # What this crate cannot do
//!
//! Stated here because a pipeline that quietly does less than it claims is
//! worse than no pipeline: **no AVIF**, **no lossy WebP**, **no subsetting of
//! a CFF font or a WOFF2**, and **no image from JSX**. [`image`], [`subset`]
//! and [`og`] each say why in their own documentation, and none of them
//! pretends otherwise at runtime — a format that was encoded and rejected is
//! reported as [`image::Declined`], a file that could not be decoded carries
//! [`image::ImageAsset::note`], a fallback that could not be computed carries
//! [`font::FontAsset::fallback_declined`], a subset that could not be produced
//! carries [`font::FontAsset::subset_declined`], and a card uf cannot lay out
//! is [`og::OgError::Unsupported`] rather than a picture of the wrong glyphs.

pub mod cache;
pub mod config;
pub mod font;
pub mod icon;
pub mod image;
pub mod name;
pub mod og;
pub mod subset;

pub use cache::{Cached, cache_key};
pub use config::{
    DEFAULT_QUALITY, DEFAULT_WIDTHS, FontsConfig, IconsConfig, ImagesConfig, OgConfig,
};
pub use font::{
    EmittedFace, FallbackFace, FontAsset, FontContainer, FontError, FontMetrics, FontRequest,
    LOCAL_FACES, LocalFace, MAX_FONT_BYTES, local_face, read_metrics, self_host,
};
pub use icon::{IconAsset, IconError, IconRequest, Sprite, SpriteRequest, icon, sprite};
pub use image::{
    BLUR_WIDTH, Declined, Emitted, ImageAsset, ImageError, ImageRequest, MAX_SOURCE_BYTES,
    MAX_SOURCE_PIXELS, Variant, transform,
};
pub use name::{NAME_DIGITS, hashed_name, hashed_name_with_suffix};
pub use og::{OG_EXTENSION, OgAsset, OgError, OgRequest, OgTemplate, draw};
pub use subset::{SubsetFace, SubsetMode, SubsetPlan};

mod testfont;
#[cfg(test)]
mod tests;

pub use testfont::font_with as test_font;
