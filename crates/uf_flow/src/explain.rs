//! Parser errors uf can say more about than the parser did.
//!
//! The Flow parser reports the token it did not expect. That is the right
//! answer for a typo and the wrong one for a construct it does not implement:
//! the reader is shown a token that is fine, in a line that is fine, with no
//! hint that the limit is the parser's rather than the file's.
//!
//! There is one of those today, [`AWAIT_OUTSIDE_ASYNC`], and the shape of this
//! module is set by it: recognise a *specific* failure from the source around
//! it, and leave every other error exactly as the parser phrased it. A layer
//! that rewrote messages in general would be a second, worse parser.

use flow_parser::loc::{Loc, Position};

use crate::scan::tokenize;

/// What uf says when `await` stands somewhere no goal symbol allows it.
///
/// Two places are left after [`module`](crate::module) has read the file:
/// inside a function that is not `async`, and anywhere in a *script* — a file
/// with no `import` and no `export`, where `await` is an ordinary identifier
/// and `await x` is two expressions with nothing between them.
///
/// The parser describes neither. `ParserEnvFlags::allow_await` is off, so
/// `await` lexes as an identifier and the error lands on the *operand*:
/// `Unexpected identifier, expected the token ';'`, with the caret on a token
/// that is fine, in a line that is fine. This says which of the two rules was
/// broken, and puts the caret on the `await`.
pub(crate) const AWAIT_OUTSIDE_ASYNC: &str = "`await` outside an `async` function is only allowed at the top level of a module — a file \
     with an `import` or an `export`";

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
    if let Some(position) = await_outside_async(source, loc, &message) {
        return (AWAIT_OUTSIDE_ASYNC.to_owned(), position);
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
/// It reaches only the `await`s [`module`](crate::module) has already declined
/// to read as a module's, which is the same set for the same reason — the
/// parser decides, twice, and neither decision is a guess about the source.
///
/// It is deliberately not "the source contains `await`". A module with an
/// `await` on line 3 and a missing brace on line 90 must still be told about
/// the brace.
fn await_outside_async(source: &str, loc: &Loc, message: &str) -> Option<Position> {
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

    Some(crate::module::positions(source, &[previous.start])[0])
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
    fn says_what_is_wrong_with_await_in_a_script() {
        // A script: no `import`, no `export`, so `await` is an identifier and
        // this is two expressions with nothing between them.
        let (message, line, column) = report("// @flow\nconst value = await load();\n");
        assert_eq!(message, AWAIT_OUTSIDE_ASYNC);
        assert_eq!((line, column), (2, 14), "the caret belongs on `await`");
    }

    #[test]
    fn says_it_for_an_await_that_is_a_statement_of_its_own() {
        let (message, ..) = report("// @flow\nawait ready();\n");
        assert_eq!(message, AWAIT_OUTSIDE_ASYNC);
    }

    #[test]
    fn says_it_inside_a_function_that_is_not_async() {
        // The other half of the rule, and the one a module reaches: a module's
        // top level is not every line of the module.
        let (message, line, column) =
            report("// @flow\nexport function read() {\n  return await load();\n}\n");
        assert_eq!(message, AWAIT_OUTSIDE_ASYNC);
        assert_eq!((line, column), (3, 9), "the caret belongs on `await`");
    }

    #[test]
    fn an_await_before_a_line_break_is_not_an_error_to_explain() {
        // Not an oversight, and worth knowing: in a script `await` is an
        // ordinary identifier, so a line break after it ends the statement and
        // the operand becomes a statement of its own. The file parses, means
        // something else entirely, and there is no error here for this module
        // to improve on. A *module* reads the same three lines as one `await`
        // — see `module::tests`.
        let outcome = crate::validate_source("// @flow\nconst value = await\n  load();\n")
            .expect("the parser is always available");
        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn says_it_through_a_comment_between_the_two() {
        let (message, ..) = report("// @flow\nconst value = await /* soon */ load();\n");
        assert_eq!(message, AWAIT_OUTSIDE_ASYNC);
    }

    #[test]
    fn leaves_an_await_inside_an_async_function_alone() {
        let outcome = crate::validate_source("// @flow\nasync function f() { await load(); }\n")
            .expect("the parser is always available");
        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn leaves_an_await_at_the_top_level_of_a_module_alone() {
        // There is nothing to explain about a module that awaits: it parses.
        let outcome = crate::validate_source("// @flow\nexport const value = await load();\n")
            .expect("the parser is always available");
        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn leaves_an_unrelated_error_in_a_file_that_also_awaits() {
        // The whole risk of this module: a file with an `await` in it must
        // still be told about the error it actually has.
        let (message, line, _) =
            report("// @flow\nasync function f() { await load(); }\nconst a = ;\n");
        assert_ne!(message, AWAIT_OUTSIDE_ASYNC);
        assert_eq!(line, 3);
    }

    #[test]
    fn leaves_an_identifier_named_await_alone() {
        // `await` is a plain identifier in a script, so this parses; nothing to
        // explain, and nothing to get wrong.
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
            report("// @flow\nconst s = \"\u{1f30a}\"; const v = await load();\n");
        assert_eq!(message, AWAIT_OUTSIDE_ASYNC);
        assert_eq!((line, column), (2, 28));
    }
}
