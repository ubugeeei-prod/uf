//! Function-level `"use server"` directives and the name they attach to.
//!
//! A directive at the top of a function body turns that one closure into a
//! server action, so the pass has to decide which `{` opens a function body at
//! all, and then walk backwards to whatever names the function, falling back to
//! a stable ordinal when nothing does.
//!
//! Both of those questions are asked elsewhere too — `scan::owner` needs the
//! same two answers to say which body a `useState` call sits in — so they live
//! in `crate::scan::owner` and this module reads them. There is one place that
//! decides whether a `{` opens a function body, for the reason there is one
//! lexer: a second opinion is a second set of edge cases to keep in step.

use uf_infra::LineIndex;

use crate::scan::owner::{FunctionHead, function_head, function_name};
use crate::scan::{Token, TokenKind};

use super::{
    DirectiveIssue, DirectiveKind, DirectiveScan, FunctionDirective, FunctionOwner, line_column,
    terminates_statement,
};

/// Collect `"use server"` directives at the top of function bodies.
pub(crate) fn scan_function_directives(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
    consumed: &mut [bool],
    scan: &mut DirectiveScan,
) {
    let mut anonymous = 0u32;

    for position in 0..tokens.len() {
        if !tokens[position].is_punct(b'{') {
            continue;
        }
        let Some(head) = function_head(source, tokens, position) else {
            continue;
        };
        let body = position + 1;
        let Some(token) = tokens.get(body) else {
            continue;
        };
        if token.kind != TokenKind::String {
            continue;
        }
        let Some(kind) = DirectiveKind::from_content(token.quoted_content(source)) else {
            continue;
        };
        let (line, column) = line_column(index, token);

        if !terminates_statement(tokens, body) {
            consumed[body] = true;
            scan.issues
                .push(DirectiveIssue::NotAStringLiteral { kind, line, column });
            continue;
        }

        consumed[body] = true;
        match kind {
            DirectiveKind::UseServer => {
                let owner = function_owner(source, tokens, head, &mut anonymous);
                scan.function_directives.push(FunctionDirective {
                    owner,
                    line,
                    column,
                });
            }
            DirectiveKind::UseClient => {
                scan.issues
                    .push(DirectiveIssue::ClientDirectiveInFunction { line, column });
            }
        }
    }
}

/// Best-effort name for the function whose head is at `head`.
///
/// An inline closure has no name to give, and numbering it is what makes an
/// anonymous action addressable at all: the ordinal is its position among the
/// module's other anonymous actions, which is stable as long as the module is.
fn function_owner(
    source: &str,
    tokens: &[Token],
    head: FunctionHead,
    anonymous: &mut u32,
) -> FunctionOwner {
    if let Some(name) = function_name(source, tokens, head) {
        return FunctionOwner::Named(name);
    }

    let ordinal = *anonymous;
    *anonymous = anonymous.saturating_add(1);
    FunctionOwner::Anonymous { ordinal }
}
