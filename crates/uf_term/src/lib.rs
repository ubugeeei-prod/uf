#![deny(missing_docs)]
//! Native terminal rendering for the Unified Toolchain for Flow (React).
//!
//! The first thing anyone judges about a toolchain is its terminal. This crate
//! is the output layer every `uf` command renders through, written from
//! scratch on top of `std`: it has **no third-party dependencies at all**, and
//! the ANSI, width, and layout rules are implemented here rather than pulled
//! in.
//!
//! # The shape of it
//!
//! * [`Capabilities`] resolves — **once**, at start-up — how much colour the
//!   stream can carry ([`ColorLevel`]), whether Unicode box drawing is safe
//!   ([`GlyphSet`]), and whether a human is watching ([`Tty`]). Every renderer
//!   is handed that value, so no write path re-probes the environment.
//! * [`Style`] renders through a [`ColorLevel`], downgrading 24-bit accents to
//!   the 256-colour cube and then to the sixteen base colours. At
//!   [`ColorLevel::Never`] it writes nothing at all, so a redirected stream
//!   contains no escape byte anywhere.
//! * [`Renderer`] draws the primitives: a banner, a key/value block, a tree, a
//!   table with per-column alignment, a rule, status marks, phase timings, and
//!   a rustc-shaped [`CodeFrame`].
//! * [`Progress`] is a spinner that writes nothing unless the stream is an
//!   interactive terminal, so CI logs stay clean.
//!
//! # Alignment
//!
//! Every layout decision goes through [`display_width`], which implements the
//! East Asian Width and emoji presentation rules natively. A path containing
//! Japanese, an emoji, or a combining mark lines up in a column exactly like an
//! ASCII one.
//!
//! # Allocation
//!
//! Every primitive appends to a caller-owned `String`. Nothing allocates per
//! cell or per row, so a lint run over thousands of diagnostics is dominated by
//! the linting rather than by the formatting.
//!
//! ```
//! use uf_term::{Capabilities, ColorChoice, Renderer, Status, TerminalEnv, Tty};
//!
//! let capabilities = Capabilities::detect(
//!     ColorChoice::Never,
//!     Tty::Piped,
//!     &TerminalEnv::default(),
//! );
//! let renderer = Renderer::new(capabilities);
//! let mut out = String::new();
//! renderer.banner(&mut out, "uf build", Some("demo-app"));
//! renderer.status(&mut out, Status::Success, "build succeeded");
//!
//! assert!(!out.contains('\u{1b}'));
//! ```

mod capability;
mod diagnostic;
mod glyph;
mod progress;
mod render;
mod style;
mod table;
mod text;
mod theme;
mod timing;
mod tree;

pub use crate::capability::{Capabilities, ColorChoice, ColorLevel, GlyphSet, TerminalEnv, Tty};
pub use crate::diagnostic::{CodeFrame, DiagnosticLevel};
pub use crate::glyph::{ASCII_GLYPHS, Glyphs, Status, UNICODE_GLYPHS};
pub use crate::progress::{DEFAULT_TICK, Progress};
pub use crate::render::{KeyValue, Renderer};
pub use crate::style::{Attributes, Color, Style};
pub use crate::table::{Cell, Column, Table};
pub use crate::text::{
    Align, char_width, display_width, push_padded, push_repeat, push_repeat_str, push_spaces,
    push_u32, push_usize, truncate_to_width,
};
pub use crate::theme::{Theme, Tone};
pub use crate::timing::{Phase, PhaseTimer, format_duration, push_duration};
pub use crate::tree::Tree;
