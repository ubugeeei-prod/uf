//! Which lint diagnostics `uf lsp` can offer to fix, and which it will not.
//!
//! A code action is a promise. The editor applies the edit without asking
//! again, often on save and often to a file nobody is looking at, so the bar
//! here is higher than "the message suggests something": the replacement has
//! to be the *only* right answer, and it has to still be the right answer for
//! the text the document holds now — which is not always the text the linter
//! saw, because an editor may ask for actions against a range it computed
//! before the last keystroke. Every fix below therefore re-reads the line at
//! the position the rule reported and offers nothing when it does not find
//! what the rule found. That is what makes the resulting `WorkspaceEdit` apply
//! cleanly rather than land four bytes into someone's identifier.
//!
//! # The rule that has a fix
//!
//! `flow/deprecated-type` — `bool` is Flow's retired spelling of `boolean` and
//! means exactly it, so the replacement carries no judgement about intent. The
//! rule refuses to fire inside a string, after a `.`, or on a property name
//! (see `uf_lint`'s `run_flow_deprecated_type`), so the four bytes it points
//! at are a type annotation and nothing else.
//!
//! # The rules whose fix is the formatter
//!
//! [`FORMATTED_AWAY`] — whitespace hygiene has no targeted edit worth writing,
//! because `uf fmt` already reprints the file from its syntax tree and that is
//! uf's actual answer to "what should this whitespace be".
//!
//! # The rules that deliberately have none
//!
//! Each of these has a fix a person could write and a machine should not:
//!
//! - `flow/unclear-type` — `any` becomes `mixed`, an opaque type, or a
//!   generated router type depending on what the author meant. Three answers
//!   is none.
//! - `flow/ambiguous-object-type` — `{|` and `...` are opposite claims about
//!   the same object, and the rule fires precisely because the author never
//!   made one.
//! - `flow/internal-type` — `React$Node` has a public equivalent, but reaching
//!   it needs an import that may or may not already be in the file under a
//!   name uf does not know.
//! - `flow/non-const-var-export` — turning `export let` into `export const` is
//!   only correct if nothing reassigns the binding, which is the question the
//!   rule could not answer either.
//! - `flow/unnecessary-optional-chain` — the edit itself is trivial (drop one
//!   `?`), but the rule matches `this?.` inside string literals too, so a fix
//!   would rewrite the contents of a string. `const s = "this?.foo";` is
//!   reported today; its sibling rules in the same runner guard the same
//!   search with an in-string test and this one does not.
//! - `server/use-client-directive-position`, `server/use-server-actions` — the
//!   directive has to land before the first *statement* but after the docblock
//!   comment that carries `@flow`, and finding that line means re-running the
//!   comment scanner that `uf_lint` owns.
//! - `security/*`, `fetch/no-global-override`, `react/*`,
//!   `uniflowed/no-npm-script-invocation` — these ask for a different design,
//!   not a different spelling.

use uf_lint::Diagnostic;

/// Rules whose answer is `uf fmt`, not a targeted edit.
///
/// Offering the formatter for these is only honest when the formatter actually
/// removes them, which it does not always do: `uf fmt` reprints from the
/// syntax tree and so preserves the inside of a template literal, while
/// `uniflowed/no-trailing-whitespace` reports the raw line and therefore fires
/// on trailing spaces inside one. The caller checks before offering; see
/// `super::rules_the_formatter_clears`.
pub(super) const FORMATTED_AWAY: [&str; 2] =
    ["uniflowed/no-tabs", "uniflowed/no-trailing-whitespace"];

/// A replacement for a byte range on one line of the document.
///
/// One line, because every fix uf has is a word swap. A fix that spanned lines
/// would need the document rather than the line and can be added when one
/// exists; inventing the shape first would be inventing the fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fix {
    /// What the editor puts in the lightbulb menu.
    pub(super) title: &'static str,
    /// Zero-based line the edit lands on, as the protocol counts lines.
    pub(super) line: usize,
    /// Byte offset within that line's text where the replaced text starts.
    pub(super) start: usize,
    /// Byte offset just past the replaced text.
    pub(super) end: usize,
    /// Text to put in its place. Empty would be a deletion; nothing needs one yet.
    pub(super) replacement: &'static str,
}

/// The fix for `diagnostic`, given the line the document currently holds there.
///
/// [`None`] means "uf has no mechanical answer for this", which covers both a
/// rule with no fix at all and a rule whose fix no longer applies because the
/// line changed underneath it.
pub(super) fn fix_for(diagnostic: &Diagnostic, line: &str) -> Option<Fix> {
    match diagnostic.rule {
        "flow/deprecated-type" => deprecated_type(diagnostic, line),
        _ => None,
    }
}

/// `bool` → `boolean`, at the column the rule reported.
fn deprecated_type(diagnostic: &Diagnostic, line: &str) -> Option<Fix> {
    const DEPRECATED: &str = "bool";

    let start = diagnostic.column.checked_sub(1)?;
    let end = start.checked_add(DEPRECATED.len())?;
    // `get` rather than indexing: the column is a byte offset into the line the
    // *linter* read, and a stale request can point past the end of this one or
    // into the middle of a multi-byte character.
    if line.get(start..end) != Some(DEPRECATED) {
        return None;
    }
    // `boolean` and `boolish` both start with `bool`; only the standalone word
    // is the deprecated alias.
    if line
        .as_bytes()
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    {
        return None;
    }

    Some(Fix {
        title: "Replace `bool` with `boolean`",
        line: diagnostic.line.saturating_sub(1),
        start,
        end,
        replacement: "boolean",
    })
}

#[cfg(test)]
mod tests {
    use uf_lint::Severity;

    use super::*;

    fn diagnostic(rule: &'static str, line: usize, column: usize) -> Diagnostic {
        Diagnostic {
            rule,
            severity: Severity::Error,
            path: Some("app/index.js".to_owned()),
            line,
            column,
            message: String::new(),
        }
    }

    #[test]
    fn a_deprecated_bool_is_replaced_where_the_rule_pointed() {
        let fix =
            fix_for(&diagnostic("flow/deprecated-type", 2, 10), "type B = bool;").expect("a fix");

        assert_eq!(fix.line, 1);
        assert_eq!((fix.start, fix.end), (9, 13));
        assert_eq!(fix.replacement, "boolean");
    }

    /// A stale range is the normal case, not the exotic one: an editor asks for
    /// actions over the range it had before the keystroke that changed the line.
    #[test]
    fn a_line_that_no_longer_says_bool_gets_no_fix() {
        assert!(
            fix_for(
                &diagnostic("flow/deprecated-type", 2, 10),
                "type B = boolean;"
            )
            .is_none()
        );
        assert!(fix_for(&diagnostic("flow/deprecated-type", 2, 10), "type B =").is_none());
        assert!(fix_for(&diagnostic("flow/deprecated-type", 2, 99), "type B = bool;").is_none());
        assert!(fix_for(&diagnostic("flow/deprecated-type", 2, 0), "type B = bool;").is_none());
    }

    /// A column landing inside a multi-byte character must not panic.
    #[test]
    fn a_column_inside_a_character_gets_no_fix() {
        assert!(fix_for(&diagnostic("flow/deprecated-type", 1, 2), "π = bool;").is_none());
    }

    /// `boolish` starts with `bool` and is not the deprecated alias.
    #[test]
    fn a_longer_identifier_beginning_with_bool_is_not_the_alias() {
        assert!(
            fix_for(
                &diagnostic("flow/deprecated-type", 1, 10),
                "type B = boolish;"
            )
            .is_none()
        );
    }

    #[test]
    fn a_rule_without_a_mechanical_answer_offers_nothing() {
        assert!(fix_for(&diagnostic("flow/unclear-type", 1, 10), "type A = any;").is_none());
        assert!(
            fix_for(
                &diagnostic("flow/unnecessary-optional-chain", 1, 1),
                "this?.x;"
            )
            .is_none()
        );
    }
}
