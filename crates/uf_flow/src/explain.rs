//! Parser errors uf can say more about than the parser did.
//!
//! The Flow parser reports the token it did not expect. That is the right
//! answer for a typo and the wrong one for a construct it does not implement:
//! the reader is shown a token that is fine, in a line that is fine, with no
//! hint that the limit is the parser's rather than the file's.
//!
//! There is one of those today, [`TOP_LEVEL_AWAIT`], and the shape of this
//! module is set by it: recognise a *specific* failure from the source around
//! it, and leave every other error exactly as the parser phrased it. A layer
//! that rewrote messages in general would be a second, worse parser.

use flow_parser::loc::{Loc, Position};

use crate::scan::tokenize;

/// What uf says when a module uses `await` outside an `async` function.
///
/// Node has run top-level `await` in a module since ES2022, and the parser uf
/// vendors — Meta's own, the one Flow ships — does not accept it: `ParseOptions`
/// has no member for it and `ParserEnvFlags::allow_await` is turned on only by
/// entering an `async` function. So a module Node runs cannot be checked,
/// formatted, transformed or tested here, and what the reader was told was
/// `Unexpected identifier, expected the token ';'`, with the caret on the
/// operand — a token that is not the problem, in a line that is not the
/// problem. See ubugeeei-prod/uf#204 for what the fix upstream would be.
const TOP_LEVEL_AWAIT: &str = "`await` outside an `async` function is not supported: Node runs it at the top level of a \
     module, and the Flow parser uf vendors does not parse it";

/// A parser error, as uf reports it: the parser's message and position unless
/// uf recognises the failure and can say something truer.
///
/// The message is compared rather than the error's variant because
/// `ParseError` is the port's type and matching on it here would be a
/// dependency on which of its variants happens to be produced for a token the
/// parser reached in a state it did not expect — which is a detail of the
/// parser's recovery, not of the language.
#[must_use]
pub fn explained(source: &str, loc: &Loc, message: String) -> (String, Position) {
    if let Some(position) = top_level_await(source, loc, &message) {
        return (TOP_LEVEL_AWAIT.to_owned(), position);
    }
    (message, loc.start)
}

/// Where the `await` is, when this error is the parser refusing one.
///
/// The test is that the token *before* the one the parser tripped on is the
/// identifier `await`, and that is exact rather than a guess: `allow_await` is
/// off only outside an `async` function, and inside one `await` is an operator
/// and reaches no error at all. So an `await` immediately before an unexpected
/// token is this and nothing else.
///
/// It is deliberately not "the source contains `await`". A module with an
/// `await` on line 3 and a missing brace on line 90 must still be told about
/// the brace.
fn top_level_await(source: &str, loc: &Loc, message: &str) -> Option<Position> {
    // The parser recovers, so it produces this message for every token that
    // cannot start a statement — a gate rather than a decision, and cheap
    // enough to run before tokenizing.
    if !message.starts_with("Unexpected ") {
        return None;
    }

    let offset = byte_offset(source, loc.start)?;
    let tokens = tokenize(source);
    // "The token before" is a step in the token list, not a distance in bytes:
    // the scanner leaves out whitespace and comments, so `await /* soon */ x`
    // still has `await` immediately before `x`.
    let index = tokens.iter().position(|token| token.start >= offset)?;
    let previous = tokens.get(index.checked_sub(1)?)?;
    if !previous.is_ident(source, "await") {
        return None;
    }

    Some(position_of(source, previous.start))
}

/// The byte offset of a parser position, or `None` when the source has no such
/// place.
///
/// The port counts lines from one and columns in *bytes* from zero — "the
/// column offset are measured by bytes", `flow_parser::loc`. Not code points,
/// and not the UTF-16 code units the transform converts to for source maps, so
/// the conversion is arithmetic on lines and nothing else. It is here rather
/// than at the call sites so there is one place to be wrong.
fn byte_offset(source: &str, position: Position) -> Option<usize> {
    let line = usize::try_from(position.line).ok()?.checked_sub(1)?;
    let column = usize::try_from(position.column).ok()?;

    let mut start = 0;
    for _ in 0..line {
        start += source[start..].find('\n')? + 1;
    }
    let offset = start.checked_add(column)?;
    let end = source[start..]
        .find('\n')
        .map_or(source.len(), |newline| start + newline);

    // A column past the end of its line, or inside a character, is not a place
    // in this source: the caller reports what the parser said instead of
    // inventing a position from a number it could not use.
    (offset <= end && source.is_char_boundary(offset)).then_some(offset)
}

/// The parser position of a byte offset: the inverse of [`byte_offset`].
fn position_of(source: &str, offset: usize) -> Position {
    let before = &source[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rfind('\n')
        .map_or(offset, |newline| offset - newline - 1);

    Position {
        line: i32::try_from(line).unwrap_or(i32::MAX),
        column: i32::try_from(column).unwrap_or(i32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The diagnostic uf reports for `source`, as (message, line, column).
    fn report(source: &str) -> (String, i32, i32) {
        let outcome = crate::validate_source(source).expect("the parser is always available");
        let first = outcome
            .diagnostics
            .first()
            .expect("the source is meant to be refused")
            .clone();
        (
            first.message,
            first.line.unwrap_or(0) as i32,
            first.column.unwrap_or(0) as i32,
        )
    }

    #[test]
    fn says_what_is_wrong_with_top_level_await() {
        let (message, line, column) = report("// @flow\nconst value = await load();\n");
        assert_eq!(message, TOP_LEVEL_AWAIT);
        assert_eq!((line, column), (2, 14), "the caret belongs on `await`");
    }

    #[test]
    fn says_it_for_an_await_that_is_a_statement_of_its_own() {
        let (message, ..) = report("// @flow\nawait ready();\n");
        assert_eq!(message, TOP_LEVEL_AWAIT);
    }

    #[test]
    fn an_await_before_a_line_break_is_not_an_error_to_explain() {
        // Not an oversight, and worth knowing: outside an `async` function
        // `await` is an ordinary identifier, so a line break after it ends the
        // statement and the operand becomes a statement of its own. The module
        // parses, means something else entirely, and there is no error here for
        // this module to improve on.
        let outcome = crate::validate_source("// @flow\nconst value = await\n  load();\n")
            .expect("the parser is always available");
        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn says_it_through_a_comment_between_the_two() {
        let (message, ..) = report("// @flow\nconst value = await /* soon */ load();\n");
        assert_eq!(message, TOP_LEVEL_AWAIT);
    }

    #[test]
    fn leaves_an_await_inside_an_async_function_alone() {
        let outcome = crate::validate_source("// @flow\nasync function f() { await load(); }\n")
            .expect("the parser is always available");
        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn leaves_an_unrelated_error_in_a_module_that_also_awaits() {
        // The whole risk of this module: a file with an `await` in it must
        // still be told about the error it actually has.
        let (message, line, _) =
            report("// @flow\nasync function f() { await load(); }\nconst a = ;\n");
        assert_ne!(message, TOP_LEVEL_AWAIT);
        assert_eq!(line, 3);
    }

    #[test]
    fn leaves_an_identifier_named_await_alone() {
        // `await` is a plain identifier outside an async function, so this
        // parses; nothing to explain, and nothing to get wrong.
        let outcome = crate::validate_source("// @flow\nconst await = 1;\nconst b = await;\n")
            .expect("the parser is always available");
        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn counts_columns_in_bytes_the_way_the_parser_does() {
        // Four bytes for the wave, one column each, because that is what the
        // port means by a column. Getting this wrong puts the caret inside a
        // character on any line that has one.
        let (message, line, column) =
            report("// @flow\nconst s = \"🌊\"; const v = await load();\n");
        assert_eq!(message, TOP_LEVEL_AWAIT);
        assert_eq!((line, column), (2, 28));
    }
}
