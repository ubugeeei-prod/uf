#![deny(missing_docs)]
//! Native formatter for Flow-typed JavaScript.
//!
//! `uf fmt` does not shell out to Prettier, Babel or any JavaScript runtime. This
//! crate scans the source once into a lossless token stream ([`lexer`]) and prints
//! it back with normalized trivia. There is no syntax tree, no per-node
//! allocation and no backtracking: two linear passes over a flat token vector,
//! which `cargo bench -p uf_fmt` measures in tens of megabytes per second per
//! core for formatting and hundreds for tokenizing.
//!
//! The formatter is built around three invariants, each covered by tests:
//!
//! * **Token preserving.** Formatting only rewrites whitespace, string quotes and
//!   statement-terminating semicolons. Every other token comes out of the printer
//!   exactly as it went in, so the formatter cannot corrupt a program.
//! * **Idempotent.** `format(format(x)) == format(x)` for every input.
//! * **Total.** No input panics or hangs, including truncated literals, unpaired
//!   delimiters and sources nested ten thousand braces deep. Both passes use
//!   explicit stacks rather than recursion, because unbounded recursion over
//!   attacker-controlled nesting is a well-worn crash vector for formatters.
//!
//! ```
//! use uf_config::FmtConfig;
//! use uf_fmt::format_source;
//!
//! let result = format_source("const x = {a:1,b:2}\n", &FmtConfig::default()).unwrap();
//! assert_eq!(result.output, "const x = { a: 1, b: 2 };\n");
//! ```

mod layout;
pub mod lexer;
mod printer;

use std::borrow::Cow;

use thiserror::Error;
use uf_config::FmtConfig;

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
    /// The source is not valid Flow, so there is no AST to print. A formatter
    /// that guessed at the grammar would reformat it anyway, and could change
    /// what it means.
    #[error("{line}:{column}: {message}")]
    Parse {
        /// One-based line.
        line: u32,
        /// Zero-based column.
        column: u32,
        /// The parser's message.
        message: String,
    },
    /// The source nests deeper than the parser can recurse. Formatting it
    /// would abort the process, so it is refused with a location instead.
    #[error("nesting is {depth} deep; `uf fmt` accepts {limit}")]
    TooDeep {
        /// Deepest nesting found.
        depth: u32,
        /// What the formatter accepts.
        limit: u32,
    },
    /// The formatter's worker thread could not be started or finished.
    #[error("the formatter could not run: {0}")]
    Spawn(String),
    /// The configuration asks for something Flow's printer does not offer.
    #[error("`fmt.{field}` is not configurable; Flow's formatter is fixed at {fixed_at}")]
    Unsupported {
        /// The `uf.config.js` field.
        field: &'static str,
        /// What Flow's printer does instead.
        fixed_at: u16,
    },
}

/// Format one source file.
///
/// A leading byte order mark is preserved, CRLF and lone CR line endings are
/// normalized to LF, and the output always ends with exactly one newline unless
/// the input contained nothing to print.
///
/// # Errors
///
/// Returns [`FormatError::InvalidIndentWidth`] when [`FmtConfig::indent_width`]
/// is zero or larger than [`MAX_INDENT_WIDTH`].
pub fn format_source(source: &str, config: &FmtConfig) -> Result<FormatResult, FormatError> {
    if config.indent_width != layout::FLOW_INDENT_WIDTH {
        return Err(FormatError::Unsupported {
            field: "indentWidth",
            fixed_at: u16::from(layout::FLOW_INDENT_WIDTH),
        });
    }
    if config.line_width != layout::FLOW_LINE_WIDTH {
        return Err(FormatError::Unsupported {
            field: "lineWidth",
            fixed_at: layout::FLOW_LINE_WIDTH,
        });
    }

    let (bom, body) = split_bom(source);
    let normalized = normalize_line_endings(body);
    let printed = layout::print(&normalized, config).map_err(|error| match error {
        layout::LayoutError::Parse {
            line,
            column,
            message,
        } => FormatError::Parse {
            line,
            column,
            message,
        },
        layout::LayoutError::TooDeep { depth, limit } => FormatError::TooDeep { depth, limit },
        layout::LayoutError::Spawn(message) => FormatError::Spawn(message),
    })?;

    let output = if bom.is_empty() {
        printed
    } else {
        let mut output = String::with_capacity(bom.len() + printed.len());
        output.push_str(bom);
        output.push_str(&printed);
        output
    };

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

/// Sources exercised by the lexer round-trip and formatter property tests.
#[cfg(test)]
pub(crate) const SOURCE_CORPUS: &[&str] = &[
    "",
    "\n",
    "// @flow\n",
    "const x = 1;\n",
    "const x = 1 / 2 / 3;\n",
    "const re = /ab+c/gi;\n",
    "if (x) /re/.test(y);\n",
    "const a = b / c, d = /re/;\n",
    "const t = `a${b}c${`nested ${d}`}e`;\n",
    "const s = 'it\\'s';\nconst d = \"say \\\"hi\\\"\";\n",
    "const c = 'line \\\ncontinued';\n",
    "/* block */ /** doc */ // line\n",
    "const n = [0x1f, 0b1010, 0o777, 1_000, 1e10, 1n, .5, 1.5e-3];\n",
    "type A = ?string;\ntype B = Array<Map<string, number>>;\n",
    "opaque type Id = string;\n",
    "component Greeting(name: string) renders React.Node { return <p>hi</p>; }\n",
    "const el = <div className=\"a\" data-testid='b'>text {value} more</div>;\n",
    "const frag = <>{items.map((i) => <Item key={i} />)}</>;\n",
    "switch (x) { case 1: return 2; default: break; }\n",
    "class A extends B { #x = 1; static y = 2; *gen() { yield* other(); } }\n",
    "const emoji = \"日本語 🎉\";\nconst ident = ünïcödé;\n",
    "async function f() { await g(); for (let i = 0; i < 3; i++) {} }\n",
    "export default function App() {}\n",
    "const div = a < b > c;\n",
    "let x = 1\nlet y = 2\n",
    "const o = { a: 1, 'b': 2, [c]: 3, ...rest };\n",
    "label: for (const item of items) { continue label; }\n",
    "const f = (x) => ({ ...x });\n",
    "declare export function f(): void;\n",
    "\"use client\";\n",
    "const tagged = html`<b>${x}</b>`;\n",
    "x = a ? b : c;\ny = { k: cond ? 1 : 2 };\n",
    "const nested = <ul>{list.map((n) => (<li>{n}</li>))}</ul>;\n",
    "\u{feff}const withBom = 1;\n",
    "const crlf = 1;\r\nconst second = 2;\r\n",
    "function f() {\n\n\n  return 1;\n\n\n}\n",
    "const deep = { a: { b: { c: [1, 2, [3, { d: 4 }]] } } };\n",
    "obj\n  .method()\n  .chain()\n  .end();\n",
    "type Exact = {| +read: string, -write?: ?number |};\n",
];

#[cfg(test)]
mod tests;
