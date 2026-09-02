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
    /// `indentWidth` was zero, which cannot produce nested indentation, or
    /// larger than [`MAX_INDENT_WIDTH`], which would let deep nesting emit an
    /// unbounded amount of leading whitespace.
    #[error("indent width must be between 1 and {MAX_INDENT_WIDTH}")]
    InvalidIndentWidth,
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
    if config.indent_width == 0 || config.indent_width > MAX_INDENT_WIDTH {
        return Err(FormatError::InvalidIndentWidth);
    }

    let (bom, body) = split_bom(source);
    let normalized = normalize_line_endings(body);
    let tokens = lexer::tokenize(&normalized);
    let printed = printer::print(&normalized, &tokens, config);

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
mod tests {
    use super::*;
    use crate::lexer::{Token, TokenKind, tokenize};
    use uf_config::QuoteStyle;

    /// A default config with `mutate` applied.
    ///
    /// `FmtConfig` is `#[non_exhaustive]`, because every feature that lands adds
    /// a knob to it and a struct literal would break on each one.
    fn config_with(mutate: impl FnOnce(&mut FmtConfig)) -> FmtConfig {
        let mut config = FmtConfig::default();
        mutate(&mut config);
        config
    }

    fn format(source: &str) -> String {
        format_source(source, &FmtConfig::default())
            .expect("default config formats")
            .output
    }

    fn format_with(source: &str, config: &FmtConfig) -> String {
        format_source(source, config)
            .expect("config formats")
            .output
    }

    /// Canonical spelling of a token, so that a requoted string compares equal to
    /// the literal it came from.
    fn canonical(token: Token, source: &str) -> String {
        let text = token.text(source);
        match token.kind {
            TokenKind::String | TokenKind::JsxString => {
                let body = text
                    .strip_prefix(['\'', '"'])
                    .and_then(|rest| rest.strip_suffix(['\'', '"']))
                    .unwrap_or(text);
                let mut canonical = String::with_capacity(body.len());
                let mut chars = body.chars();
                while let Some(ch) = chars.next() {
                    if ch == '\\' {
                        match chars.next() {
                            Some(next @ ('"' | '\'')) => canonical.push(next),
                            Some(next) => {
                                canonical.push('\\');
                                canonical.push(next);
                            }
                            None => canonical.push('\\'),
                        }
                    } else {
                        canonical.push(ch);
                    }
                }
                canonical
            }
            _ => text.to_string(),
        }
    }

    /// The formatter may only change trivia, quote style and semicolons. Anything
    /// else is a bug that could silently corrupt a program.
    ///
    /// `>>` and `>>>` are compared as the `>` characters they stand for, because
    /// closing nested type arguments legally joins `> >` into `>>`, exactly as a
    /// Flow parser splits it again. The printer never inserts characters inside a
    /// token, so the character count can only be preserved.
    fn assert_token_preserving(input: &str, output: &str) {
        use crate::lexer::Punctuator;

        let interesting = |source: &str| -> Vec<(TokenKind, String)> {
            let mut tokens = Vec::new();
            for token in tokenize(source) {
                if token.kind.is_trivia()
                    || token.kind == TokenKind::Punctuator(Punctuator::Semicolon)
                {
                    continue;
                }
                let repeats = match token.kind {
                    TokenKind::Punctuator(Punctuator::Greater) => 1,
                    TokenKind::Punctuator(Punctuator::GreaterGreater) => 2,
                    TokenKind::Punctuator(Punctuator::GreaterGreaterGreater) => 3,
                    _ => 0,
                };
                if repeats == 0 {
                    tokens.push((token.kind, canonical(token, source)));
                    continue;
                }
                for _ in 0..repeats {
                    tokens.push((TokenKind::Punctuator(Punctuator::Greater), ">".to_string()));
                }
            }
            tokens
        };
        similar_asserts::assert_eq!(
            interesting(input),
            interesting(output),
            "token stream changed while formatting {input:?}"
        );
    }

    /// The weaker guarantee that still holds for input that does not lex cleanly.
    ///
    /// Full token preservation cannot: the printer emits exactly one final
    /// newline, and for a source ending inside an unterminated comment or string
    /// that newline necessarily lands *inside* that token, changing its text.
    /// The input was already not valid JavaScript, and the comment stays
    /// unterminated either way, so that is harmless.
    ///
    /// What must still hold is that recovery neither drops nor invents a token,
    /// which is the failure a formatter could actually cause here.
    fn assert_token_kinds_preserved(input: &str, output: &str) {
        use crate::lexer::Punctuator;

        let kinds = |source: &str| -> Vec<TokenKind> {
            tokenize(source)
                .into_iter()
                .map(|token| token.kind)
                .filter(|kind| {
                    !kind.is_trivia() && *kind != TokenKind::Punctuator(Punctuator::Semicolon)
                })
                .collect()
        };
        similar_asserts::assert_eq!(
            kinds(input),
            kinds(output),
            "a token was dropped or invented while formatting {input:?}"
        );
    }

    fn assert_comments_preserved(input: &str, output: &str) {
        let comments = |source: &str| -> Vec<String> {
            tokenize(source)
                .into_iter()
                .filter(|token| token.kind.is_comment())
                .map(|token| token.text(source).to_string())
                .collect()
        };
        assert_eq!(comments(input), comments(output), "comments changed");
    }

    // ------------------------------------------------------------ properties

    #[test]
    fn formatting_is_idempotent_over_the_corpus() {
        for source in SOURCE_CORPUS {
            let once = format(source);
            let twice = format(&once);
            similar_asserts::assert_eq!(once, twice, "not idempotent for {source:?}");
        }
    }

    #[test]
    fn formatting_preserves_the_token_stream_over_the_corpus() {
        for source in SOURCE_CORPUS {
            assert_token_preserving(source, &format(source));
        }
    }

    #[test]
    fn formatting_preserves_comments_verbatim() {
        for source in SOURCE_CORPUS {
            assert_comments_preserved(source, &format(source));
        }
    }

    #[test]
    fn formatting_is_idempotent_for_every_configuration() {
        let configs = [
            FmtConfig::default(),
            config_with(|config| {
                config.indent_width = 4;
                config.quotes = QuoteStyle::Single;
                config.semicolons = false;
                config.line_width = 40;
                config.max_blank_lines = 0;
            }),
            config_with(|config| {
                config.indent_width = 1;
                config.quotes = QuoteStyle::Single;
                config.semicolons = true;
                config.line_width = 20;
                config.max_blank_lines = 2;
            }),
        ];
        for config in &configs {
            for source in SOURCE_CORPUS {
                let once = format_with(source, config);
                let twice = format_with(&once, config);
                similar_asserts::assert_eq!(once, twice, "not idempotent for {source:?}");
                assert_token_preserving(source, &once);
            }
        }
    }

    // ------------------------------------------------------------- structure

    #[test]
    fn removes_trailing_whitespace_and_adds_a_final_newline() {
        similar_asserts::assert_eq!(
            format("const x = 1;  \nconst y = 2;"),
            "const x = 1;\nconst y = 2;\n"
        );
    }

    #[test]
    fn reindents_using_the_configured_indent_width() {
        let config = config_with(|config| {
            config.indent_width = 4;
        });
        similar_asserts::assert_eq!(
            format_with(
                "function f() {\n\tif (x) {\n\t\treturn 1;\n\t}\n}\n",
                &config
            ),
            "function f() {\n    if (x) {\n        return 1;\n    }\n}\n"
        );
    }

    #[test]
    fn indents_by_bracket_depth_regardless_of_the_input_indentation() {
        similar_asserts::assert_eq!(
            format("const a = [\n1,\n        2,\n];\n"),
            "const a = [\n  1,\n  2,\n];\n"
        );
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(format(""), "");
        assert_eq!(format("   \n\n"), "");
    }

    #[test]
    fn collapses_runs_of_blank_lines() {
        similar_asserts::assert_eq!(format("a;\n\n\n\n\nb;\n"), "a;\n\nb;\n");
    }

    #[test]
    fn max_blank_lines_is_configurable() {
        let config = config_with(|config| {
            config.max_blank_lines = 0;
        });
        similar_asserts::assert_eq!(format_with("a;\n\n\nb;\n", &config), "a;\nb;\n");
    }

    #[test]
    fn blank_lines_inside_a_delimiter_pair_are_dropped() {
        similar_asserts::assert_eq!(
            format("function f() {\n\n  return 1;\n\n}\n"),
            "function f() {\n  return 1;\n}\n"
        );
    }

    #[test]
    fn leading_blank_lines_are_dropped() {
        similar_asserts::assert_eq!(format("\n\n\nconst x = 1;\n"), "const x = 1;\n");
    }

    #[test]
    fn trailing_blank_lines_collapse_to_one_newline() {
        similar_asserts::assert_eq!(format("const x = 1;\n\n\n\n"), "const x = 1;\n");
    }

    // -------------------------------------------------------------- spacing

    #[test]
    fn normalizes_spacing_around_operators_and_separators() {
        similar_asserts::assert_eq!(format("const x=a+b*c;\n"), "const x = a + b * c;\n");
        similar_asserts::assert_eq!(format("f( a ,b );\n"), "f(a, b);\n");
        similar_asserts::assert_eq!(format("const a=[ 1 ,2 ];\n"), "const a = [1, 2];\n");
    }

    #[test]
    fn keeps_unary_operators_attached_to_their_operand() {
        similar_asserts::assert_eq!(format("const x = - 1;\n"), "const x = -1;\n");
        similar_asserts::assert_eq!(format("const y = ! ok;\n"), "const y = !ok;\n");
        similar_asserts::assert_eq!(format("f(... args);\n"), "f(...args);\n");
        similar_asserts::assert_eq!(format("const z = a - -b;\n"), "const z = a - -b;\n");
    }

    #[test]
    fn keeps_increments_attached_to_their_operand() {
        similar_asserts::assert_eq!(format("for (;;) i ++;\n"), "for (;;) i++;\n");
        similar_asserts::assert_eq!(format("++ i;\n"), "++i;\n");
    }

    #[test]
    fn statement_headers_keep_a_space_before_the_parenthesis() {
        similar_asserts::assert_eq!(format("if(x){f(y);}\n"), "if (x) { f(y); }\n");
        similar_asserts::assert_eq!(format("while(x)f();\n"), "while (x) f();\n");
    }

    #[test]
    fn for_headers_keep_their_semicolons() {
        similar_asserts::assert_eq!(
            format("for(let i=0;i<n;i++){}\n"),
            "for (let i = 0; i < n; i++) {}\n"
        );
        similar_asserts::assert_eq!(format("for( ; ; ){}\n"), "for (;;) {}\n");
    }

    #[test]
    fn object_literals_keep_padded_braces_and_arrays_do_not() {
        similar_asserts::assert_eq!(format("const o={a:1};\n"), "const o = { a: 1 };\n");
        similar_asserts::assert_eq!(format("const o={};\n"), "const o = {};\n");
        similar_asserts::assert_eq!(format("const a=[1];\n"), "const a = [1];\n");
    }

    #[test]
    fn member_access_and_calls_stay_tight() {
        similar_asserts::assert_eq!(format("a . b ( c ) [ 0 ];\n"), "a.b(c)[0];\n");
        similar_asserts::assert_eq!(format("a ?. b;\n"), "a?.b;\n");
    }

    #[test]
    fn adjacent_operators_are_never_glued_into_a_different_token() {
        // Removing these spaces would turn `+ +` into `++` and change the program.
        for source in [
            "a + +b;\n",
            "a - -b;\n",
            "a + ++b;\n",
            "a - --b;\n",
            "a / /re/.source;\n",
            "a < <T>(x) => x;\n",
        ] {
            let output = format(source);
            assert_token_preserving(source, &output);
        }
    }

    #[test]
    fn closing_type_arguments_may_be_joined_into_one_token() {
        // `Array<Array<T> >` legally becomes `Array<Array<T>>`; a Flow parser
        // splits the `>>` again when it closes type arguments.
        let output = format("type A = Array<Array<Array<T> > >;\n");
        similar_asserts::assert_eq!(output, "type A = Array<Array<Array<T>>>;\n");
        assert_token_preserving("type A = Array<Array<Array<T> > >;\n", &output);
    }

    #[test]
    fn shift_operators_keep_their_spacing() {
        similar_asserts::assert_eq!(
            format("const x = a >> b >>> c << d;\n"),
            "const x = a >> b >>> c << d;\n"
        );
    }

    #[test]
    fn ternaries_are_spaced_and_object_keys_are_not() {
        similar_asserts::assert_eq!(format("const x=a?b:c;\n"), "const x = a ? b : c;\n");
        similar_asserts::assert_eq!(
            format("const x={k:a?1:2,j:3};\n"),
            "const x = { k: a ? 1 : 2, j: 3 };\n"
        );
    }

    #[test]
    fn continuation_lines_of_a_call_chain_are_indented() {
        similar_asserts::assert_eq!(
            format("promise\n.then(a)\n.catch(b);\n"),
            "promise\n  .then(a)\n  .catch(b);\n"
        );
    }

    #[test]
    fn switch_cases_sit_one_level_inside_the_switch() {
        similar_asserts::assert_eq!(
            format("switch (x) {\ncase 1:\nf();\nbreak;\ndefault:\ng();\n}\n"),
            "switch (x) {\n  case 1:\n    f();\n    break;\n  default:\n    g();\n}\n"
        );
    }

    #[test]
    fn generator_stars_hug_the_function_keyword() {
        similar_asserts::assert_eq!(
            format("function * gen() { yield * other(); }\n"),
            "function* gen() { yield* other(); }\n"
        );
    }

    // ----------------------------------------------------------- flow types

    #[test]
    fn type_arguments_are_printed_without_spaces() {
        similar_asserts::assert_eq!(
            format("const m: Map < string , number > = new Map();\n"),
            "const m: Map<string, number> = new Map();\n"
        );
        similar_asserts::assert_eq!(
            format("type Nested = Array<Map<string, Array<number>>>;\n"),
            "type Nested = Array<Map<string, Array<number>>>;\n"
        );
    }

    #[test]
    fn comparisons_are_not_mistaken_for_type_arguments() {
        similar_asserts::assert_eq!(
            format("const ok=a<b&&c>d;\n"),
            "const ok = a < b && c > d;\n"
        );
        similar_asserts::assert_eq!(format("if (a<b) f();\n"), "if (a < b) f();\n");
    }

    #[test]
    fn nullable_and_optional_flow_markers_hug_their_type() {
        similar_asserts::assert_eq!(format("type A = ? string;\n"), "type A = ?string;\n");
        similar_asserts::assert_eq!(
            format("function f(x ? : ?number) {}\n"),
            "function f(x?: ?number) {}\n"
        );
    }

    #[test]
    fn exact_object_types_keep_their_pipes_attached() {
        similar_asserts::assert_eq!(
            format("type E = {| a: number |};\n"),
            "type E = {| a: number |};\n"
        );
    }

    #[test]
    fn unions_and_variance_are_spaced_like_operators() {
        similar_asserts::assert_eq!(format("type U = | A|B & C;\n"), "type U = | A | B & C;\n");
        similar_asserts::assert_eq!(
            format("type V = { +read: string, -write: number };\n"),
            "type V = { +read: string, -write: number };\n"
        );
    }

    #[test]
    fn opaque_types_and_generics_survive() {
        similar_asserts::assert_eq!(
            format("opaque type   Id<T> =  Wrapped<T>;\n"),
            "opaque type Id<T> = Wrapped<T>;\n"
        );
    }

    // ------------------------------------------------------------------ jsx

    #[test]
    fn jsx_attributes_are_normalized_without_spaces_around_equals() {
        similar_asserts::assert_eq!(
            format("const a = <div className = \"x\" id={ y } />;\n"),
            "const a = <div className=\"x\" id={y} />;\n"
        );
    }

    #[test]
    fn jsx_children_keep_their_significant_whitespace() {
        similar_asserts::assert_eq!(
            format("const a = <p>hello   world {name} !</p>;\n"),
            "const a = <p>hello   world {name} !</p>;\n"
        );
    }

    #[test]
    fn jsx_children_are_indented_by_nesting_depth() {
        similar_asserts::assert_eq!(
            format("const a = (\n<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n);\n"),
            "const a = (\n  <ul>\n    <li>one</li>\n    <li>two</li>\n  </ul>\n);\n"
        );
    }

    #[test]
    fn nested_jsx_expression_containers_are_formatted_as_javascript() {
        similar_asserts::assert_eq!(
            format("const a = <ul>{items.map((i)=>(<li key={i}>{i}</li>))}</ul>;\n"),
            "const a = <ul>{items.map((i) => (<li key={i}>{i}</li>))}</ul>;\n"
        );
    }

    #[test]
    fn jsx_text_is_never_reflowed() {
        let source = "const a = (\n  <p>\n    a long line of prose that would otherwise be wrapped by a formatter\n  </p>\n);\n";
        similar_asserts::assert_eq!(format(source), source);
    }

    // -------------------------------------------------------------- quotes

    #[test]
    fn quotes_are_normalized_to_the_configured_style() {
        similar_asserts::assert_eq!(format("const s = 'a';\n"), "const s = \"a\";\n");
        let single = config_with(|config| {
            config.quotes = QuoteStyle::Single;
        });
        similar_asserts::assert_eq!(
            format_with("const s = \"a\";\n", &single),
            "const s = 'a';\n"
        );
    }

    #[test]
    fn quotes_are_left_alone_when_converting_would_add_escapes() {
        similar_asserts::assert_eq!(
            format("const s = 'say \"hi\"';\n"),
            "const s = 'say \"hi\"';\n"
        );
    }

    #[test]
    fn template_literals_are_never_requoted() {
        similar_asserts::assert_eq!(
            format("const t = `a 'b' \"c\"`;\n"),
            "const t = `a 'b' \"c\"`;\n"
        );
    }

    #[test]
    fn directives_keep_their_meaning() {
        similar_asserts::assert_eq!(format("'use client';\nf();\n"), "\"use client\";\nf();\n");
        similar_asserts::assert_eq!(format("\"use server\"\nf()\n"), "\"use server\";\nf();\n");
    }

    // ---------------------------------------------------------- semicolons

    #[test]
    fn missing_statement_semicolons_are_added() {
        similar_asserts::assert_eq!(
            format("const x = 1\nconst y = 2\n"),
            "const x = 1;\nconst y = 2;\n"
        );
        similar_asserts::assert_eq!(
            format("function f() {\n  return 1\n}\n"),
            "function f() {\n  return 1;\n}\n"
        );
    }

    #[test]
    fn semicolons_are_not_added_where_the_next_line_continues_the_expression() {
        // `a\n[0]` is a single index expression: a semicolon would change it.
        similar_asserts::assert_eq!(format("const x = a\n[0].b()\n"), "const x = a\n[0].b();\n");
        similar_asserts::assert_eq!(format("const x = a\n(0)\n"), "const x = a\n(0);\n");
        similar_asserts::assert_eq!(format("const x = a\n+ b\n"), "const x = a\n+ b;\n");
    }

    #[test]
    fn semicolons_are_not_added_inside_object_literals_or_argument_lists() {
        similar_asserts::assert_eq!(
            format("const o = {\n  a: 1,\n  b: 2\n};\n"),
            "const o = {\n  a: 1,\n  b: 2\n};\n"
        );
        similar_asserts::assert_eq!(format("f(\n  a,\n  b\n);\n"), "f(\n  a,\n  b\n);\n");
    }

    #[test]
    fn semicolons_are_not_added_after_a_trailing_comment() {
        similar_asserts::assert_eq!(
            format("const x = 1 // note\nconst y = 2;\n"),
            "const x = 1 // note\nconst y = 2;\n"
        );
    }

    #[test]
    fn semicolons_can_be_removed() {
        let config = config_with(|config| {
            config.semicolons = false;
        });
        similar_asserts::assert_eq!(
            format_with("const x = 1;\nconst y = 2;\n", &config),
            "const x = 1\nconst y = 2\n"
        );
        similar_asserts::assert_eq!(
            format_with("for (let i = 0; i < 3; i++) {}\n", &config),
            "for (let i = 0; i < 3; i++) {}\n"
        );
    }

    #[test]
    fn an_empty_statement_after_a_header_is_never_dropped() {
        let config = config_with(|config| {
            config.semicolons = false;
        });
        similar_asserts::assert_eq!(format_with("while (f());\n", &config), "while (f());\n");
    }

    // ------------------------------------------------------------ comments

    #[test]
    fn comments_are_preserved_verbatim_and_in_place() {
        let source = "/**\n * Doc comment.\n *   Indented continuation.\n */\nfunction f() {\n  // leading\n  return 1; // trailing\n  /* block */\n}\n";
        similar_asserts::assert_eq!(format(source), source);
    }

    #[test]
    fn a_block_comment_between_tokens_keeps_one_space_on_each_side() {
        similar_asserts::assert_eq!(
            format("const x = /* why */ 1;\n"),
            "const x = /* why */ 1;\n"
        );
    }

    // ------------------------------------------------------- line breaking

    #[test]
    fn long_argument_lists_are_exploded_to_fit_the_line_width() {
        let config = config_with(|config| {
            config.line_width = 30;
        });
        similar_asserts::assert_eq!(
            format_with("call(alpha, beta, gamma, delta, epsilon);\n", &config),
            "call(\n  alpha,\n  beta,\n  gamma,\n  delta,\n  epsilon\n);\n"
        );
    }

    #[test]
    fn short_groups_are_left_on_one_line() {
        let config = config_with(|config| {
            config.line_width = 30;
        });
        similar_asserts::assert_eq!(format_with("call(a, b);\n", &config), "call(a, b);\n");
    }

    #[test]
    fn exploding_a_group_never_adds_a_trailing_comma() {
        let config = config_with(|config| {
            config.line_width = 12;
        });
        let output = format_with("const xs = [alpha, beta];\n", &config);
        assert!(!output.contains(",\n]"), "{output}");
        assert_token_preserving("const xs = [alpha, beta];\n", &output);
    }

    #[test]
    fn author_line_breaks_are_preserved() {
        let source = "const xs = [\n  1,\n  2,\n];\n";
        similar_asserts::assert_eq!(format(source), source);
    }

    // ------------------------------------------------------------ encoding

    #[test]
    fn a_leading_byte_order_mark_is_preserved() {
        let formatted = format("\u{feff}const x = 1;\n");
        assert!(formatted.starts_with('\u{feff}'));
        similar_asserts::assert_eq!(formatted, "\u{feff}const x = 1;\n");
    }

    #[test]
    fn crlf_is_normalized_to_lf() {
        similar_asserts::assert_eq!(
            format("const x = 1;\r\nconst y = 2;\r\n"),
            "const x = 1;\nconst y = 2;\n"
        );
        similar_asserts::assert_eq!(format("a;\rb;\r"), "a;\nb;\n");
    }

    #[test]
    fn multi_byte_characters_are_never_split() {
        let source = "const s = \"日本語のテキスト 🎉🎈\";\nconst ünïcödé = 1;\n";
        let formatted = format(source);
        assert!(formatted.contains("日本語のテキスト 🎉🎈"));
        assert!(formatted.is_char_boundary(formatted.len()));
        assert_token_preserving(source, &formatted);
    }

    #[test]
    fn non_ascii_widths_are_measured_in_characters() {
        let config = config_with(|config| {
            config.line_width = 20;
        });
        let formatted = format_with("f(\"日本語日本語日本語\", \"日本語日本語\");\n", &config);
        assert!(formatted.contains('\n'));
        assert_token_preserving("f(\"日本語日本語日本語\", \"日本語日本語\");\n", &formatted);
    }

    // ------------------------------------------------------------- totality

    #[test]
    fn malformed_input_never_panics_and_stays_token_preserving() {
        let long_line = format!("const x = [{}];\n", "1, ".repeat(60_000));
        let deep = {
            let depth = 10_000;
            let mut source = String::with_capacity(depth * 2 + 1);
            for _ in 0..depth {
                source.push('{');
            }
            for _ in 0..depth {
                source.push('}');
            }
            source.push('\n');
            source
        };
        let cases: Vec<String> = vec![
            "'unterminated".to_string(),
            "\"unterminated".to_string(),
            "`unterminated ${".to_string(),
            "`${`${`${".to_string(),
            "/* unterminated".to_string(),
            "/** unterminated".to_string(),
            "\\".to_string(),
            "\\\\\\".to_string(),
            "}}}}".to_string(),
            "((((".to_string(),
            "[[[[".to_string(),
            "<div>".to_string(),
            "</div>".to_string(),
            "<div><span></div>".to_string(),
            "const x = /unterminated".to_string(),
            "\0\u{1}\u{2}".to_string(),
            "#!".to_string(),
            "?:".to_string(),
            "a\u{2028}b".to_string(),
            long_line,
            deep,
        ];

        for source in &cases {
            let first = format(source);
            // Idempotence alone is too weak here: a formatter can make a stable
            // token-changing rewrite and pass it. Malformed input is exactly
            // where a recovery path might silently drop or invent a token, so
            // the stream has to be checked too.
            assert_token_kinds_preserved(source, &first);
            let second = format(&first);
            similar_asserts::assert_eq!(first, second, "not idempotent for {source:?}");
        }
    }

    #[test]
    fn a_one_megabyte_single_line_is_formatted() {
        let source = format!("const x = \"{}\";\n", "a".repeat(1_000_000));
        let formatted = format(&source);
        assert_eq!(formatted, source);
    }

    #[test]
    fn deeply_nested_indentation_is_capped() {
        let depth = 2_000;
        let mut source = String::new();
        for _ in 0..depth {
            source.push_str("{\n");
        }
        for _ in 0..depth {
            source.push_str("}\n");
        }
        let formatted = format(&source);
        let widest = formatted
            .lines()
            .map(|line| line.len() - line.trim_start().len())
            .max()
            .unwrap_or(0);
        assert!(widest <= 256 * 2, "indent grew to {widest} columns");
    }

    // -------------------------------------------------------------- errors

    #[test]
    fn a_zero_indent_width_is_rejected() {
        let config = config_with(|config| {
            config.indent_width = 0;
        });
        assert_eq!(
            format_source("x;\n", &config),
            Err(FormatError::InvalidIndentWidth)
        );
    }

    #[test]
    fn an_oversized_indent_width_is_rejected() {
        let config = config_with(|config| {
            config.indent_width = 200;
        });
        assert_eq!(
            format_source("x;\n", &config),
            Err(FormatError::InvalidIndentWidth)
        );
        let allowed = config_with(|config| {
            config.indent_width = MAX_INDENT_WIDTH;
        });
        assert!(format_source("x;\n", &allowed).is_ok());
    }

    #[test]
    fn changed_reports_whether_the_output_differs() {
        let config = FmtConfig::default();
        assert!(!format_source("const x = 1;\n", &config).unwrap().changed);
        assert!(format_source("const x = 1;  \n", &config).unwrap().changed);
    }

    #[test]
    fn line_endings_are_normalized_before_lexing() {
        assert_eq!(normalize_line_endings("a\nb"), "a\nb");
        assert_eq!(normalize_line_endings("a\r\nb"), "a\nb");
        assert_eq!(normalize_line_endings("a\rb"), "a\nb");
        assert_eq!(normalize_line_endings("a\r\n\r\nb"), "a\n\nb");
    }

    #[test]
    fn the_byte_order_mark_is_split_off_only_at_the_start() {
        assert_eq!(split_bom("\u{feff}x"), ("\u{feff}", "x"));
        assert_eq!(split_bom("x\u{feff}"), ("", "x\u{feff}"));
    }

    /// A body brace after a tuple or array return type is a block, not an object.
    ///
    /// The formatter used to read the `{` in
    /// `hook useX(): [string, () => void] {` as an object literal, because the
    /// token before it is `]`, and then emitted a `;` after the closing brace —
    /// a token the input never had. Two golden fixtures had the extra semicolon
    /// baked in as expected output, which is how it survived: a fixture that
    /// records a bug turns the bug into the specification.
    #[test]
    fn a_body_after_a_tuple_return_type_gains_no_semicolon() {
        for source in [
            r#"// @flow
hook useX(): [string, (next: string) => void] {
  return ["", () => {}];
}
"#,
            r#"// @flow
function pair(): [number, number] {
  return [1, 2];
}
"#,
            r#"// @flow
function rows(): Array<[string, number]> {
  return [];
}
"#,
            r#"// @flow
export function tuple(): [A] {
  return [a];
}
"#,
        ] {
            let output = format(source);

            assert_eq!(
                output.matches(';').count(),
                source.matches(';').count(),
                "formatting added or removed a semicolon:\n--- input\n{source}--- output\n{output}"
            );
            assert_token_preserving(source, &output);
        }
    }

    /// The same shape after an angle-bracket return type, which already worked —
    /// kept so the two cases cannot drift apart again.
    #[test]
    fn a_body_after_a_generic_return_type_gains_no_semicolon() {
        let source = r#"// @flow
function load(): Promise<void> {
  return go();
}
"#;

        let output = format(source);

        assert_eq!(output.matches(';').count(), source.matches(';').count());
        assert_token_preserving(source, &output);
    }

    /// An object literal directly after `]` is not valid JavaScript, so nothing
    /// legitimate regresses from treating that brace as a block. An index
    /// expression followed by a block statement still formats.
    #[test]
    fn an_indexed_access_followed_by_a_block_still_formats() {
        let source = r#"// @flow
const x = items[0];
{
  run();
}
"#;

        let output = format(source);

        assert_token_preserving(source, &output);
    }
}
