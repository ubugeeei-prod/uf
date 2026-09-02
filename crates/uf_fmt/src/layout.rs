//! Formatting through Flow's own layout generator.
//!
//! `flow_parser` produces the AST, `js_layout_generator` turns it into a layout
//! tree, and `pretty_printer` renders that tree. This is the code `flow format`
//! runs, vendored in `upstream/flow`.
//!
//! It replaces a formatter that worked from a token stream and had to *guess*
//! at the grammar. That guessing is what produced the defects: `<T extends U>`
//! came out as `< T extends U >` because a speculative scan could not tell a
//! type-parameter bracket from a comparison, and a tuple return type grew a
//! stray `;` because a brace classifier read a type as a block. An AST printer
//! cannot make either mistake, because it is never guessing.
//!
//! # What Flow's printer does not offer
//!
//! `pretty_printer` hard-codes an 80-column target and a two-space indent step,
//! and `js_layout_generator::Opts` exposes only bracket spacing, quote style,
//! trailing commas and whether to preserve existing formatting. There is no
//! setting for line width, indent width, blank-line runs or semicolons. uf's
//! `FmtConfig` has fields for all four. Rather than accept them and quietly
//! print something else, [`format_source`] rejects a configuration it cannot
//! honour and names the field.

use flow_parser::PERMISSIVE_PARSE_OPTIONS;
use flow_parser_utils_output::{js_layout_generator as generator, pretty_printer};
use uf_config::{FmtConfig, QuoteStyle};

/// Columns Flow's pretty printer targets. Not configurable upstream.
pub const FLOW_LINE_WIDTH: u16 = 80;

/// Indent step Flow's pretty printer emits. Not configurable upstream.
pub const FLOW_INDENT_WIDTH: u8 = 2;

/// Deepest bracket nesting `uf fmt` will parse.
///
/// Flow's parser and layout generator both recurse on the call stack, so depth
/// is bounded by stack size rather than by anything they check. Measured on an
/// 8 MB stack, 500 levels succeed and 1000 abort the process — and a formatter
/// that aborts is a denial of service on any project that formats what it is
/// given. The cap is well under the measured limit, and far beyond anything
/// hand-written: 256 is the depth the previous formatter also stopped at.
pub const MAX_NESTING_DEPTH: u32 = 256;

/// Stack the parse and print run on.
///
/// Fixed here rather than inherited, so the depth this crate accepts does not
/// change with the caller — a test harness thread gets 2 MB by default and the
/// main thread 8 MB, which would otherwise make the safe depth a property of
/// who called.
const FORMAT_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Deepest bracket nesting in `source`, counting only real delimiters.
///
/// A byte scan rather than a parse: this runs *before* the parser, so it cannot
/// use one. Strings and comments are skipped so a `{` inside them does not
/// count.
fn nesting_depth(source: &str) -> u32 {
    let bytes = source.as_bytes();
    let (mut depth, mut deepest, mut index) = (0u32, 0u32, 0usize);
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += memchr::memchr(b'\n', &bytes[index..]).unwrap_or(bytes.len() - index);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let rest = &bytes[index + 2..];
                index += 2 + rest
                    .windows(2)
                    .position(|pair| pair == b"*/")
                    .map_or(rest.len(), |at| at + 2);
            }
            quote @ (b'"' | b'\'' | b'`') => {
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
            }
            b'{' | b'[' | b'(' => {
                depth += 1;
                deepest = deepest.max(depth);
                index += 1;
            }
            b'}' | b']' | b')' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            _ => index += 1,
        }
    }
    deepest
}

/// Render `source` the way `flow format` would.
pub(crate) fn print(source: &str, config: &FmtConfig) -> Result<String, LayoutError> {
    let depth = nesting_depth(source);
    if depth > MAX_NESTING_DEPTH {
        return Err(LayoutError::TooDeep {
            depth,
            limit: MAX_NESTING_DEPTH,
        });
    }

    // Own the inputs so the worker outlives this frame's borrows.
    let source = source.to_owned();
    let config = config.clone();
    std::thread::Builder::new()
        .name("uf-fmt".to_owned())
        .stack_size(FORMAT_STACK_BYTES)
        .spawn(move || print_on_this_stack(&source, &config))
        .map_err(|error| LayoutError::Spawn(error.to_string()))?
        .join()
        .map_err(|_| LayoutError::Spawn("the formatter thread panicked".to_owned()))?
}

fn print_on_this_stack(source: &str, config: &FmtConfig) -> Result<String, LayoutError> {
    let (ast, errors) = flow_parser::parse_program_without_file(
        false,
        None,
        Some(PERMISSIVE_PARSE_OPTIONS),
        Ok(source),
    );
    if let Some((loc, error)) = errors.first() {
        return Err(LayoutError::Parse {
            line: u32::try_from(loc.start.line).unwrap_or(0),
            column: u32::try_from(loc.start.column).unwrap_or(0),
            message: error.to_string(),
        });
    }

    let opts = generator::Opts {
        bracket_spacing: true,
        preserve_formatting: false,
        single_quotes: matches!(config.quotes, QuoteStyle::Single),
        trailing_commas: generator::TrailingCommas::ES5,
    };
    // `preserve_docblock` keeps the `// @flow` pragma, which every uf module
    // carries and which the type checker reads. Turning it off drops the pragma
    // outright, so it stays on and the blank line it leaves at the head of a
    // file that has no docblock is trimmed here instead.
    let node = generator::program(&opts, true, None, &ast);
    let printed = pretty_printer::print(false, &node).contents();
    let body = printed.trim_start_matches('\n');
    Ok(if body.trim().is_empty() {
        String::new()
    } else {
        body.to_owned()
    })
}

/// Why Flow's layout generator could not produce output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LayoutError {
    /// The source nests deeper than the parser can recurse.
    TooDeep {
        /// Deepest nesting found.
        depth: u32,
        /// What the formatter accepts.
        limit: u32,
    },
    /// The formatter's worker thread could not be started or finished.
    Spawn(String),
    /// The source is not valid Flow, so there is no AST to print.
    Parse {
        /// One-based line.
        line: u32,
        /// Zero-based column.
        column: u32,
        /// The parser's message.
        message: String,
    },
}
