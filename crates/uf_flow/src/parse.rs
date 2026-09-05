//! The parse entry point: the official port's syntax tree for one source file.
//!
//! [`validate_source`](crate::validate_source) answers "does this parse", which
//! is what the linter asks. Anything that *rewrites* source — the formatter, a
//! transform — needs the tree itself, and it needs the parser's own tree rather
//! than a copy: a second definition of Flow syntax is a second place for the
//! grammar to drift, which is the mistake this crate exists to prevent. So the
//! port's types are re-exported here as [`ast`] and [`Loc`], and [`parse`]
//! hands back a [`Parsed`] built from them.
//!
//! # Ceilings
//!
//! The port is a recursive-descent parser, and a stack overflow cannot be
//! caught, so two limits are enforced *before* it runs: [`MAX_PARSE_BYTES`] on
//! the size of the source and [`MAX_NESTING_DEPTH`] on how deeply brackets
//! nest. Both come back as a typed [`ParseFailure`] so a caller formatting a
//! whole project sees one refused file rather than its own crash. Syntax errors
//! are deliberately not failures: the port recovers and reports them, and they
//! ride along as [`Parsed::diagnostics`].

use std::panic::{AssertUnwindSafe, catch_unwind};

use flow_parser::ParseOptions;
use flow_parser::parse_error::ParseError;
use thiserror::Error;

pub use flow_parser::ast;
pub use flow_parser::loc::{Loc, Position};

use crate::ParseDiagnostic;

/// Longest source [`parse`] will accept, in bytes.
///
/// The same ceiling as [`MAX_STRIP_BYTES`](crate::MAX_STRIP_BYTES), for the
/// same reason: a dependency can put a generated or hostile file in
/// `node_modules`, and every scan in `uf` has an explicit limit above it rather
/// than trusting the input to be a reasonable size.
pub const MAX_PARSE_BYTES: usize = 8 * 1024 * 1024;

/// Deepest bracket nesting [`parse`] will accept.
///
/// The port spends stack in proportion to this number, and a formatter that
/// walks the tree recursively again doubles the exposure. A file that nests
/// brackets hundreds deep is not something a person wrote; refusing it with a
/// typed error is the alternative to a stack overflow. The number is generous
/// for real code — generated tables rarely pass fifty — and, together with
/// [`PARSE_STACK_BYTES`], one the parser has been measured to survive.
pub const MAX_NESTING_DEPTH: usize = 300;

/// Longest chain of operators [`parse`] will accept at one bracket level.
///
/// `1 + 1 + 1 + …` and `a.f().f()…` nest one AST node per link and open no
/// bracket that stays open, so [`MAX_NESTING_DEPTH`] does not see them at
/// all: a 3 MB file of the first measured **0** and aborted `uf fmt` with a
/// stack overflow. See ubugeeei-prod/uf#136.
///
/// A separate number because a level of brackets and a level of `+` cost
/// very different amounts of stack — the port's object-literal frame is
/// about 150 KiB and a binary operand is a small fraction of that, which is
/// why this ceiling is thirty times the other one and still lower than
/// where the printer gives out.
///
/// Measured from both sides.
///
/// **What real code reaches.** The 15,971 files in `tests/fixtures/git` —
/// React, React Native, Metro, Relay, Parcel, Yarn, Prepack and eight more,
/// with minified third-party bundles among them — reach **1,958**, in
/// CodeMirror's bundle. So this is five times the deepest chain anyone in the
/// corpus wrote.
///
/// **What the formatter survives.** `uf fmt` runs the parser, the printer and
/// the tree's `Drop` on a thread of [`PARSE_STACK_BYTES`], and formats a
/// chain of 40,000 there; it gives out somewhere before 100,000. So this is
/// four times under the measured floor of the path that ships.
///
/// # A caller that holds the tree on a small stack
///
/// Freeing the tree recurses once per level, and it happens on whatever
/// thread holds the [`Parsed`]. A 2 MiB thread — an unoptimized test thread
/// is one — overflows on a chain of about 4,000, which is *below* this
/// ceiling. That is a hazard of the AST's `Drop` rather than of the ceiling,
/// it applies equally to bracket nesting, and it is filed separately; a
/// caller that parses deep sources should do it on a thread sized like the
/// one [`parse`] uses. The tests below do.
pub const MAX_CHAIN_DEPTH: usize = 10_000;

/// Stack a thread needs to run [`parse`] on any source under
/// [`MAX_NESTING_DEPTH`].
///
/// The port's frames are large: measured on an unoptimized build, an object
/// literal costs about 150 KiB of stack per level of nesting, so the ceiling
/// alone needs some 45 MiB — far more than the 8 MiB a main thread or the
/// 2 MiB a test thread gets. Callers run the parser (and whatever recursive
/// walk they do over its tree) on a thread of this size; the reservation is
/// virtual and only the pages actually touched cost anything. A test below
/// parses at the ceiling on such a thread, so the two constants cannot drift
/// apart unnoticed.
pub const PARSE_STACK_BYTES: usize = 128 * 1024 * 1024;

/// Parse options aligned with the `uf` project defaults.
///
/// Every syntax `uf` ships in templates or lints is on — component and hook
/// syntax, enums, pattern matching, records, Flow types, and types in
/// comments. Decorators stay off because generated projects never emit them.
/// This mirrors the options [`validate_source`](crate::validate_source) uses,
/// so the formatter and the linter always agree on what parses.
const UF_PARSE_OPTIONS: ParseOptions = ParseOptions {
    components: true,
    enums: true,
    pattern_matching: true,
    records: true,
    esproposal_decorators: false,
    types: true,
    ambiguous_types: true,
    enable_types_in_comments: true,
    use_strict: false,
    assert_operator: false,
    module_ref_prefix: None,
    ambient: false,
    allow_return_outside_function: false,
};

/// One Flow source file, parsed by the official port.
///
/// The port recovers from syntax errors, so a program is always produced;
/// [`Parsed::diagnostics`] says whether it is the program the author meant.
/// Anything that rewrites source from the tree must refuse to when there are
/// diagnostics, because a recovered tree is the parser's best guess and
/// printing a guess loses code.
#[derive(Debug, Clone)]
pub struct Parsed {
    /// The syntax tree, in the port's own types.
    pub program: ast::Program<Loc, Loc>,
    /// Syntax errors in source order; empty for a clean parse.
    pub diagnostics: Vec<ParseDiagnostic>,
}

impl Parsed {
    /// Whether the source parsed without a single syntax error.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Every comment in the file, in source order, whether or not the parser
    /// attached it to a node.
    ///
    /// Comment text excludes its delimiters: `// a` carries `" a"` and
    /// `/* b */` carries `" b "`. A printer adds them back from
    /// [`ast::Comment::kind`].
    #[must_use]
    pub fn comments(&self) -> &[ast::Comment<Loc>] {
        &self.program.all_comments
    }
}

/// Why [`parse`] refused a source before the parser saw it.
///
/// Syntax errors are not a failure: they come back as
/// [`Parsed::diagnostics`], with the recovered tree beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseFailure {
    /// The source is larger than [`MAX_PARSE_BYTES`].
    #[error("source is {bytes} bytes, over the {limit} byte ceiling")]
    SourceTooLarge {
        /// Size of the rejected source.
        bytes: usize,
        /// The ceiling, always [`MAX_PARSE_BYTES`].
        limit: usize,
    },
    /// Brackets nest deeper than [`MAX_NESTING_DEPTH`].
    #[error("brackets nest {depth} deep, over the {limit} level ceiling")]
    TooDeeplyNested {
        /// The nesting the scanner measured.
        depth: usize,
        /// The ceiling, always [`MAX_NESTING_DEPTH`].
        limit: usize,
    },
    /// Operators chain deeper than [`MAX_CHAIN_DEPTH`].
    ///
    /// Separate from [`TooDeeplyNested`](Self::TooDeeplyNested) because the
    /// two are different shapes and a message that says "brackets" about
    /// `1 + 1 + 1 + …` sends whoever reads it looking for brackets.
    #[error("operators chain {depth} deep, over the {limit} level ceiling")]
    TooDeeplyChained {
        /// The chain the scanner measured.
        depth: usize,
        /// The ceiling, always [`MAX_CHAIN_DEPTH`].
        limit: usize,
    },
    /// The port panicked instead of reporting a diagnostic.
    ///
    /// Not expected for any input, but a parser is the part of a toolchain
    /// most exposed to hostile bytes, and a caller that formats files in bulk
    /// must see one bad file as an error, not as its own crash.
    #[error("the Flow parser failed on this source")]
    ParserPanicked,
}

/// Parse `source` with the official Flow port and return its syntax tree.
///
/// Run it on a thread with [`PARSE_STACK_BYTES`] of stack: the port recurses
/// once per level of nesting, and [`MAX_NESTING_DEPTH`] levels do not fit in
/// a default thread.
///
/// # Errors
///
/// Returns [`ParseFailure::SourceTooLarge`] past [`MAX_PARSE_BYTES`],
/// [`ParseFailure::TooDeeplyNested`] past [`MAX_NESTING_DEPTH`] and
/// [`ParseFailure::TooDeeplyChained`] past [`MAX_CHAIN_DEPTH`]; all three are
/// decided before the parser runs, which is the point — the tree that would
/// overflow the stack is never built. Syntax errors are not errors here: see
/// [`Parsed::diagnostics`].
pub fn parse(source: &str) -> Result<Parsed, ParseFailure> {
    if source.len() > MAX_PARSE_BYTES {
        return Err(ParseFailure::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_PARSE_BYTES,
        });
    }
    let depths = depths(source);
    if depths.brackets > MAX_NESTING_DEPTH {
        return Err(ParseFailure::TooDeeplyNested {
            depth: depths.brackets,
            limit: MAX_NESTING_DEPTH,
        });
    }
    if depths.chain > MAX_CHAIN_DEPTH {
        return Err(ParseFailure::TooDeeplyChained {
            depth: depths.chain,
            limit: MAX_CHAIN_DEPTH,
        });
    }

    let (program, errors) = catch_unwind(AssertUnwindSafe(|| {
        flow_parser::parse_program_without_file(false, None, Some(UF_PARSE_OPTIONS), Ok(source))
    }))
    .map_err(|_| ParseFailure::ParserPanicked)?;

    Ok(Parsed {
        program,
        diagnostics: errors.iter().map(diagnostic_from_error).collect(),
    })
}

fn diagnostic_from_error((loc, error): &(Loc, ParseError)) -> ParseDiagnostic {
    ParseDiagnostic {
        message: error.to_string(),
        line: u32::try_from(loc.start.line).ok(),
        column: u32::try_from(loc.start.column).ok(),
    }
}

/// What the depth scanner is inside: JavaScript, with the number of `{`
/// opened since the frame began, or the literal text of a template.
#[derive(Clone, Copy)]
enum DepthFrame {
    Js { braces: usize },
    TemplateText,
}

/// What the previous significant token was, as far as deciding whether a `/`
/// starts a regular expression needs to know.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Previous {
    /// Nothing yet, or something a division cannot follow.
    Operator,
    /// A value: an identifier, a literal, or a closing bracket.
    Value,
}

/// How deeply `source` nests, in brackets and in operators.
///
/// This is the guard behind [`MAX_NESTING_DEPTH`], so it has to see what the
/// parser sees: brackets inside strings, comments and regular expressions are
/// text, and the nesting hidden inside a template's `${ … }` counts to any
/// depth or the ceiling could be walked around. It is a single pass of its own
/// rather than a walk over [`scan::tokenize`](crate::scan::tokenize), which
/// hands back a whole template literal as one token.
///
/// # Operators nest too
///
/// `(`, `[` and `{` are not the only things that add a level.
/// `1 + 1 + 1 + …` is one `Binary` node per operand and `a.f().f()…` is one
/// `Member` and one `Call` per link, and neither opens a bracket that stays
/// open. Counting brackets alone reported **0** for a 3 MB file of
/// `1 + 1 + …` that aborted `uf fmt` with a stack overflow — inside every
/// limit this module declares. See ubugeeei-prod/uf#136.
///
/// So each bracket level also carries a *run*: how many operator tokens have
/// been seen since the last `,` or `;` at that level. The reset is what keeps
/// the measure from confusing width with depth — `[a + b, c + d, …]` is a
/// thousand siblings of depth one, not a chain of a thousand — and the
/// per-level stack is what keeps an inner expression from being charged for
/// the one it sits in.
///
/// A run of operator bytes counts once, so `===` is one level and not three.
/// The answer is an upper bound: `a + -b` counts two where the tree nests
/// two, and an object literal's `:` counts one where nothing nests. Erring
/// high is the safe direction for a ceiling.
///
/// Unbalanced closers are ignored rather than reported: the answer is an upper
/// bound on how deep the parser will recurse, and the parser is the one that
/// diagnoses the mismatch.
pub fn nesting_depth(source: &str) -> usize {
    depths(source).brackets
}

/// The longest run of operator tokens between two separators, in the deepest
/// bracket level it appears at.
///
/// The guard behind [`MAX_CHAIN_DEPTH`]. See [`nesting_depth`] for why this
/// is a second number rather than part of the first: a level of brackets and
/// a level of `+` cost the parser very different amounts of stack, so one
/// ceiling cannot be right for both.
#[must_use]
pub fn chain_depth(source: &str) -> usize {
    depths(source).chain
}

/// How deeply a source nests, by both measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Depths {
    /// Deepest bracket nesting.
    pub brackets: usize,
    /// Longest operator run, plus the brackets it sits inside.
    pub chain: usize,
}

/// Both measures, in one pass.
#[must_use]
pub fn depths(source: &str) -> Depths {
    let bytes = source.as_bytes();
    let mut at = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    };
    if bytes[at..].starts_with(b"#!") {
        at = line_end(bytes, at);
    }

    let mut frames: Vec<DepthFrame> = vec![DepthFrame::Js { braces: 0 }];
    let mut depth = 0usize;
    let mut deepest = 0usize;
    let mut deepest_chain = 0usize;
    let mut previous = Previous::Operator;
    // One `(base, run)` per open bracket, innermost last: how deep this
    // level starts, and how many operator tokens have been seen in it since
    // the last `,` or `;`. The outermost entry is the file's top level,
    // which has no bracket of its own.
    //
    // The base is what makes the answer an upper bound on tree depth rather
    // than on depth-within-a-level: in `a + f(b + c)` the inner `+` sits
    // under a `+`, a call and a paren, and the level it opens says so.
    let mut levels: Vec<(usize, usize)> = vec![(0, 0)];

    while at < bytes.len() {
        let Some(&frame) = frames.last() else {
            frames.push(DepthFrame::Js { braces: 0 });
            continue;
        };

        if let DepthFrame::TemplateText = frame {
            match bytes[at] {
                b'\\' => at += 2,
                b'`' => {
                    frames.pop();
                    previous = Previous::Value;
                    at += 1;
                }
                b'$' if bytes.get(at + 1) == Some(&b'{') => {
                    frames.push(DepthFrame::Js { braces: 0 });
                    depth += 1;
                    levels.push((level_base(&levels) + 1, 0));
                    deepest = deepest.max(depth);
                    previous = Previous::Operator;
                    at += 2;
                }
                _ => at += 1,
            }
            continue;
        }

        let byte = bytes[at];
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c => at += 1,
            b'/' if bytes.get(at + 1) == Some(&b'/') => at = line_end(bytes, at),
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at = match find(bytes, at + 2, b"*/") {
                    Some(end) => end + 2,
                    None => bytes.len(),
                };
            }
            b'"' | b'\'' => {
                at = quoted_end(bytes, at, byte);
                previous = Previous::Value;
            }
            b'`' => {
                frames.push(DepthFrame::TemplateText);
                at += 1;
            }
            b'/' => {
                match previous {
                    Previous::Operator => at = regex_end(bytes, at).unwrap_or(at + 1),
                    // A division, which nests like any other operator. The
                    // regex branch above is why this cannot be left to the
                    // operator arm: `/` is decided here or not at all.
                    Previous::Value => {
                        at += 1;
                        if let Some((base, run)) = levels.last_mut() {
                            *run += 1;
                            deepest_chain = deepest_chain.max(*base + *run);
                        }
                    }
                }
                previous = Previous::Value;
            }
            b'(' | b'[' => {
                // After a value these open a call or an index, and each of
                // those is a node: `f()()()…` and `a[0][0][0]…` nest as
                // deeply as they are long while the bracket depth stays at
                // one. After an operator they are a grouping paren or an
                // array literal, which the bracket count already has.
                if previous == Previous::Value
                    && let Some((base, run)) = levels.last_mut()
                {
                    *run += 1;
                    deepest_chain = deepest_chain.max(*base + *run);
                }
                depth += 1;
                levels.push((level_base(&levels) + 1, 0));
                deepest = deepest.max(depth);
                previous = Previous::Operator;
                at += 1;
            }
            b'{' => {
                if let Some(DepthFrame::Js { braces }) = frames.last_mut() {
                    *braces += 1;
                }
                depth += 1;
                levels.push((level_base(&levels) + 1, 0));
                deepest = deepest.max(depth);
                previous = Previous::Operator;
                at += 1;
            }
            b')' | b']' => {
                depth = depth.saturating_sub(1);
                if levels.len() > 1 {
                    levels.pop();
                }
                previous = Previous::Value;
                at += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                previous = Previous::Value;
                let closes_substitution = match frames.last_mut() {
                    Some(DepthFrame::Js { braces }) if *braces > 0 => {
                        *braces -= 1;
                        false
                    }
                    Some(DepthFrame::Js { .. }) => frames.len() > 1,
                    _ => false,
                };
                if closes_substitution {
                    frames.pop();
                }
                if levels.len() > 1 {
                    levels.pop();
                }
                at += 1;
            }
            b'0'..=b'9' => {
                at = number_end(bytes, at);
                previous = Previous::Value;
            }
            b'.' if matches!(bytes.get(at + 1), Some(b'0'..=b'9')) => {
                at = number_end(bytes, at);
                previous = Previous::Value;
            }
            _ if is_ident_start(byte) => {
                let start = at;
                at += 1;
                while at < bytes.len() && is_ident_part(bytes[at]) {
                    at += 1;
                }
                let word = &bytes[start..at];
                // A prefix operator spelled as a word nests exactly like one
                // spelled in punctuation: `typeof typeof … x` and
                // `new new … Foo` are one node per word, and neither opens a
                // bracket.
                if is_prefix_keyword(word)
                    && let Some((base, run)) = levels.last_mut()
                {
                    *run += 1;
                    deepest_chain = deepest_chain.max(*base + *run);
                }
                previous = if precedes_expression(word) {
                    Previous::Operator
                } else {
                    Previous::Value
                };
            }
            b',' | b';' => {
                if let Some((_, run)) = levels.last_mut() {
                    *run = 0;
                }
                previous = Previous::Operator;
                at += 1;
            }
            _ if is_operator(byte) => {
                // A whole run of operator bytes is one token: `===` nests
                // once, not three times.
                while at < bytes.len() && is_operator(bytes[at]) {
                    at += 1;
                }
                if let Some((base, run)) = levels.last_mut() {
                    *run += 1;
                    deepest_chain = deepest_chain.max(*base + *run);
                }
                previous = Previous::Operator;
            }
            _ => {
                previous = Previous::Operator;
                at += 1;
            }
        }
    }

    Depths {
        brackets: deepest,
        chain: deepest_chain,
    }
}

/// Where a bracket opened inside `levels` starts counting from: everything
/// its enclosing level has already nested.
fn level_base(levels: &[(usize, usize)]) -> usize {
    levels.last().map_or(0, |(base, run)| base + run)
}

/// Whether `word` is a prefix operator: one that takes an expression and is
/// itself an expression, so a run of them nests.
///
/// `typeof`, `void`, `delete`, `await`, `yield` and `new`. Not `return` or
/// `case`, which take an expression and are not one — they cannot repeat.
fn is_prefix_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"typeof" | b"void" | b"delete" | b"await" | b"yield" | b"new"
    )
}

/// Whether `byte` is punctuation that can nest one expression inside
/// another: an operator, a member access, or a ternary's `?` and `:`.
///
/// Brackets, `,` and `;` are deliberately absent — the first are counted as
/// depth already and the second two end a run rather than extend it.
const fn is_operator(byte: u8) -> bool {
    matches!(
        byte,
        b'.' | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'<'
            | b'>'
            | b'='
            | b'!'
            | b'&'
            | b'|'
            | b'^'
            | b'~'
            | b'?'
            | b':'
    )
}

/// Keywords after which a `/` begins a regular expression rather than dividing.
fn precedes_expression(word: &[u8]) -> bool {
    matches!(
        word,
        b"await"
            | b"case"
            | b"delete"
            | b"do"
            | b"else"
            | b"in"
            | b"instanceof"
            | b"new"
            | b"of"
            | b"return"
            | b"throw"
            | b"typeof"
            | b"void"
            | b"yield"
    )
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

fn is_ident_part(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() && !matches!(bytes[at], b'\n' | b'\r') {
        at += 1;
    }
    at
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| from + offset)
}

/// One past the closing quote of the string starting at `from`, or the end
/// of its line for an unterminated one.
fn quoted_end(bytes: &[u8], from: usize, quote: u8) -> usize {
    let mut at = from + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'\n' | b'\r' => return at,
            byte if byte == quote => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

fn number_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() {
        let byte = bytes[at];
        let exponent_sign = matches!(byte, b'+' | b'-')
            && at > from
            && matches!(bytes[at - 1], b'e' | b'E')
            && !bytes[from..at].starts_with(b"0x")
            && !bytes[from..at].starts_with(b"0X");
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || exponent_sign {
            at += 1;
        } else {
            break;
        }
    }
    at
}

/// One past the flags of the regular expression starting at `from`, or
/// [`None`] when the slash turns out to be a division after all.
fn regex_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut at = from + 1;
    let mut in_class = false;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'\n' | b'\r' => return None,
            b'[' => {
                in_class = true;
                at += 1;
            }
            b']' => {
                in_class = false;
                at += 1;
            }
            b'/' if !in_class => {
                at += 1;
                while at < bytes.len() && is_ident_part(bytes[at]) {
                    at += 1;
                }
                return Some(at);
            }
            _ => at += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_tree_and_every_comment() {
        let source = "// @flow\n/* a */ const x = 1; // b\n";

        let parsed = parse(source).expect("parses");

        assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.statements.len(), 1);
        let comments: Vec<(&str, bool)> = parsed
            .comments()
            .iter()
            .map(|comment| {
                (
                    &*comment.text,
                    matches!(comment.kind, ast::CommentKind::Line),
                )
            })
            .collect();
        assert_eq!(comments, [(" @flow", true), (" a ", false), (" b", true)]);
    }

    #[test]
    fn reports_syntax_errors_as_diagnostics_not_failures() {
        let parsed = parse("const = ;\n").expect("the parser recovers");

        assert!(!parsed.is_ok());
        assert_eq!(parsed.diagnostics[0].line, Some(1));
    }

    #[test]
    fn parses_every_modern_flow_construct_the_port_accepts() {
        let source = r#"// @flow
export component Page(a: string, ...rest: Props) renders React.Node {
  return match (a) { "x" => <div>{a}</div>, _ => null };
}
hook useX(): number { return 1; }
enum E of string { A = "a", B = "b" }
type T<+A, -B, in C, out D, E extends string, F = number> = {| +a: A, b?: ?B, ...C |};
declare module.exports: { x: number };
const y = (x: any) as const;
"#;

        let parsed = parse(source).expect("parses");

        assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.program.statements.len(), 6);
    }

    #[test]
    fn refuses_oversized_sources_before_parsing() {
        let source = "x".repeat(MAX_PARSE_BYTES + 1);

        assert_eq!(
            parse(&source).map(|_| ()),
            Err(ParseFailure::SourceTooLarge {
                bytes: MAX_PARSE_BYTES + 1,
                limit: MAX_PARSE_BYTES,
            })
        );
    }

    #[test]
    fn refuses_nesting_past_the_ceiling() {
        let deep = "[".repeat(MAX_NESTING_DEPTH + 1) + &"]".repeat(MAX_NESTING_DEPTH + 1);

        assert_eq!(
            parse(&deep).map(|_| ()),
            Err(ParseFailure::TooDeeplyNested {
                depth: MAX_NESTING_DEPTH + 1,
                limit: MAX_NESTING_DEPTH,
            })
        );
    }

    /// The ceiling has to be one the parser actually survives on the stack
    /// callers are told to give it, or it is not a ceiling at all. Object
    /// literals are the most expensive shape per level, so they are the
    /// measurement that matters.
    #[test]
    fn survives_nesting_at_the_ceiling_on_the_documented_stack() {
        let worker = std::thread::Builder::new()
            .stack_size(PARSE_STACK_BYTES)
            .spawn(|| {
                for (open, close) in [
                    ("[", "]"),
                    ("(", ")"),
                    ("{a:", "}"),
                    ("`${", "}`"),
                    ("f(", ")"),
                ] {
                    let source = format!(
                        "x = {}1{};\n",
                        open.repeat(MAX_NESTING_DEPTH),
                        close.repeat(MAX_NESTING_DEPTH)
                    );
                    let parsed = parse(&source).expect("at the ceiling");
                    assert!(parsed.is_ok(), "{open}: {:?}", parsed.diagnostics);
                }
            })
            .expect("spawns");
        worker.join().expect("parses at the ceiling");
    }

    #[test]
    fn nesting_depth_counts_brackets_and_template_substitutions() {
        assert_eq!(nesting_depth("f(a, [b])"), 2);
        assert_eq!(nesting_depth("`${`${`${x}`}`}`"), 3);
        assert_eq!(nesting_depth("`${(\"}\", `${x}`)}`"), 3);
        assert_eq!(nesting_depth("`a${b}c${d}`"), 1);
    }

    #[test]
    fn nesting_depth_ignores_brackets_that_are_text() {
        assert_eq!(nesting_depth("f(\"{{{{\", /* [[[[ */ '(((')"), 1);
        assert_eq!(nesting_depth("x = a / b / c; // ((((\n"), 0);
        assert_eq!(nesting_depth("const r = /\\(/; const s = /[(]/;"), 0);
        assert_eq!(nesting_depth("return /(/.test(x)"), 1);
        assert_eq!(nesting_depth("`\\${(`"), 0);
    }

    #[test]
    fn nesting_depth_never_underflows() {
        assert_eq!(nesting_depth("}}}}((("), 3);
        assert_eq!(nesting_depth(")))]]]"), 0);
    }

    #[test]
    fn chain_depth_counts_operators_that_open_no_bracket() {
        // The shape that measured zero and aborted the formatter.
        assert_eq!(nesting_depth("x = 1 + 1 + 1 + 1;"), 0);
        assert_eq!(chain_depth("x = 1 + 1 + 1 + 1;"), 4);

        // A run of operator bytes is one level, not one per byte.
        assert_eq!(chain_depth("x = a === b;"), 2);
        assert_eq!(chain_depth("x = a && b && c;"), 3);

        // Member chains. With the calls it is six, not three: `.f()` is a
        // member *and* a call, and both are nodes.
        assert_eq!(chain_depth("a.b.c.d"), 3);
        assert_eq!(chain_depth("a.b().c().d()"), 6);

        // A call and an index nest even though their brackets close again.
        // `f()()()…` and `a[0][0]…` keep the bracket depth at one.
        assert_eq!(chain_depth("f()()()"), 3);
        assert_eq!(chain_depth("a[0][0][0]"), 3);
        // After an operator the same brackets are a group or an array
        // literal, which the bracket count already has.
        assert_eq!(chain_depth("x = (((a)))"), 1);
        assert_eq!(chain_depth("x = [[[a]]]"), 1);

        // A prefix operator spelled as a word nests like one spelled in
        // punctuation.
        assert_eq!(chain_depth("typeof typeof typeof x"), 3);
        assert_eq!(chain_depth("new new new Foo"), 3);
        assert_eq!(chain_depth("await await x"), 2);

        // Division is decided in the branch that also reads regular
        // expressions, so it is counted there or not at all.
        assert_eq!(chain_depth("x = a / b / c;"), 3);
        assert_eq!(chain_depth("x = /a/ + /b/;"), 2);
    }

    #[test]
    fn chain_depth_is_depth_and_not_width() {
        // Five hundred siblings are not a chain of five hundred: `,` and `;`
        // end a run, which is what keeps a wide array from measuring deep.
        // Three is the `=`, the `[` and the `+` that any one element needs.
        let wide = format!("x = [{}];", vec!["a + b"; 500].join(", "));
        assert_eq!(chain_depth(&wide), 3);
        let statements = "x = a + b;\n".repeat(500);
        assert_eq!(chain_depth(&statements), 2);

        // An inner expression is charged for what it sits in, and no more:
        // `f(a + b)` is a call, its parentheses, and a `+`.
        assert_eq!(chain_depth("f(a + b)"), 3);
        assert_eq!(chain_depth("a + f(b + c)"), 4);
    }

    #[test]
    fn a_chain_past_the_ceiling_is_refused_rather_than_overflowing() {
        let deep = MAX_CHAIN_DEPTH + 2;
        for source in [
            format!("x = {};", vec!["1"; deep].join(" + ")),
            format!("x = a{};", ".f()".repeat(deep)),
            // Each of these keeps the bracket depth at one, or opens no
            // bracket at all, and each was reaching the parser.
            format!("x = f{};", "()".repeat(deep)),
            format!("x = a{};", "[0]".repeat(deep)),
            format!("x = {}y;", "typeof ".repeat(deep)),
            format!("x = {}Foo;", "new ".repeat(deep)),
        ] {
            // Refused before the tree exists, so nothing deep is built and
            // nothing deep is freed: this one needs no stack of its own.
            let error = parse(&source).expect_err("refused");
            assert!(
                matches!(error, ParseFailure::TooDeeplyChained { .. }),
                "{error:?}"
            );
            // And it says which kind of nesting, because "brackets" about
            // `1 + 1 + …` sends whoever reads it looking for brackets.
            assert!(error.to_string().starts_with("operators chain"), "{error}");
        }
    }

    #[test]
    fn a_chain_at_the_ceiling_still_parses() {
        // On a thread the size `parse` uses. The tree is freed where it is
        // held, and freeing recurses once per level, so a default test
        // thread's 2 MiB is not enough for a chain this deep — which is a
        // property of the AST's `Drop` and not of the ceiling. See the note
        // on `MAX_CHAIN_DEPTH`.
        std::thread::Builder::new()
            .stack_size(PARSE_STACK_BYTES)
            .spawn(|| {
                let source = format!("x = {};", vec!["1"; MAX_CHAIN_DEPTH].join(" + "));
                let parsed = parse(&source).expect("parses");
                assert!(parsed.is_ok(), "{:?}", parsed.diagnostics);
            })
            .expect("spawns")
            .join()
            .expect("no overflow at the ceiling");
    }
}
