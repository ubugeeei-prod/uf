#![deny(missing_docs)]
//! Native formatter for Flow-typed JavaScript, and the rest of a project's
//! files beside it.
//!
//! `uf fmt` does not shell out to Prettier, Babel or any JavaScript runtime.
//! A `.js` file is parsed by Meta's official Flow parser (through
//! [`uf_flow`]), its comments are attached to the nodes they belong to, and
//! the tree is printed through a Wadler-style document IR ([`doc`]) with the
//! layout rules of Prettier 3 — the same groups, the same conditional
//! groups for hugged arguments, the same member-chain and ternary rules — so
//! the output is what Prettier's `hermes` parser would produce. Everything
//! else a project holds (JSON, CSS, TypeScript) is routed to Biome's
//! formatters.
//!
//! The formatter is built around four guarantees, each covered by tests:
//!
//! * **Idempotent.** `format(format(x)) == format(x)` for every input.
//! * **Tree preserving.** The output re-parses to the same tree as the
//!   input, locations, comments and parentheses aside. Parentheses are
//!   recomputed from the tree, so a pair the grammar needs is always there
//!   and a pair it does not is not.
//! * **Comment preserving.** Every comment in the input is in the output
//!   exactly once, `// @flow` docblocks and `$FlowFixMe` suppressions
//!   included; a printer that forgot one returns an error instead.
//! * **Total.** Invalid syntax returns a typed error and the caller leaves
//!   the file alone; no input panics, and nesting is bounded by an explicit
//!   ceiling rather than by the stack.
//!
//! ```
//! use uf_config::FmtConfig;
//! use uf_fmt::format_source;
//!
//! let result = format_source("const x = {a:1,b:2}\n", &FmtConfig::default()).unwrap();
//! assert_eq!(result.output, "const x = { a: 1, b: 2 };\n");
//! ```

pub mod doc;
mod flow;

use std::borrow::Cow;

use thiserror::Error;
use uf_config::FmtConfig;

pub mod non_flow;

pub use flow::FlowFormatError;
pub use non_flow::{Invocation, NonFlowError};

/// The largest [`FmtConfig::indent_width`] the formatter accepts.
///
/// Indentation is emitted once per line and multiplied by the nesting depth, so
/// an unbounded width is an unbounded allocation on adversarial input.
pub const MAX_INDENT_WIDTH: u8 = 16;

/// Byte order mark, which is preserved verbatim at the head of a file.
const BOM: &str = "\u{feff}";

/// The outcome of formatting one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatResult {
    /// The formatted source text.
    pub output: String,
    /// Whether [`FormatResult::output`] differs from the input.
    pub changed: bool,
}

/// Why a source file could not be formatted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FormatError {
    /// `indentWidth` was zero, which cannot produce nested indentation, or
    /// larger than [`MAX_INDENT_WIDTH`], which would let deep nesting emit an
    /// unbounded amount of leading whitespace.
    #[error("indent width must be between 1 and {MAX_INDENT_WIDTH}")]
    InvalidIndentWidth,
    /// `lineWidth` was zero, which no layout can satisfy.
    #[error("line width must be at least 1")]
    InvalidLineWidth,
    /// The Flow printer refused the file: a syntax error, a size or nesting
    /// ceiling, or a comment it could not place.
    #[error(transparent)]
    Flow(#[from] FlowFormatError),
    /// The formatting thread could not be started.
    #[error("failed to start the formatting thread")]
    Thread,
}

/// Format one Flow source file.
///
/// A leading byte order mark is preserved, CRLF and lone CR line endings are
/// normalized to LF, and the output always ends with exactly one newline unless
/// the input contained nothing to print.
///
/// # Errors
///
/// Returns [`FormatError::InvalidIndentWidth`] when [`FmtConfig::indent_width`]
/// is zero or larger than [`MAX_INDENT_WIDTH`], and [`FormatError::Flow`] when
/// the source does not parse or exceeds a ceiling; in either case the caller
/// should leave the file untouched.
pub fn format_source(source: &str, config: &FmtConfig) -> Result<FormatResult, FormatError> {
    if config.indent_width == 0 || config.indent_width > MAX_INDENT_WIDTH {
        return Err(FormatError::InvalidIndentWidth);
    }
    if config.line_width == 0 {
        return Err(FormatError::InvalidLineWidth);
    }

    let (bom, body) = split_bom(source);
    let normalized = normalize_line_endings(body);

    // The parser and the printer both recurse once per level of nesting,
    // and the parser's frames are large, so the work runs on a thread with
    // the stack `uf_flow` documents for its nesting ceiling. The
    // reservation is virtual; only the pages touched cost anything.
    let printed = std::thread::scope(|scope| -> Result<String, FormatError> {
        let worker = std::thread::Builder::new()
            .name("uf-fmt".into())
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, || flow::format(&normalized, config))
            .map_err(|_| FormatError::Thread)?;
        let result = worker.join().map_err(|_| FormatError::Thread)?;
        result.map_err(FormatError::Flow)
    })?;

    let mut output = String::with_capacity(bom.len() + printed.len() + 1);
    output.push_str(bom);
    output.push_str(&printed);

    Ok(FormatResult {
        changed: output != source,
        output,
    })
}

/// Split a leading byte order mark off the source.
fn split_bom(source: &str) -> (&str, &str) {
    match source.strip_prefix(BOM) {
        Some(rest) => (BOM, rest),
        None => ("", source),
    }
}

/// Rewrite CRLF and lone CR line endings as LF, borrowing when there is nothing
/// to change.
fn normalize_line_endings(source: &str) -> Cow<'_, str> {
    if memchr::memchr(b'\r', source.as_bytes()).is_none() {
        return Cow::Borrowed(source);
    }

    let mut normalized = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while let Some(offset) = memchr::memchr(b'\r', &bytes[cursor..]) {
        let at = cursor + offset;
        normalized.push_str(&source[cursor..at]);
        normalized.push('\n');
        cursor = if bytes.get(at + 1) == Some(&b'\n') {
            at + 2
        } else {
            at + 1
        };
    }
    normalized.push_str(&source[cursor..]);
    Cow::Owned(normalized)
}
