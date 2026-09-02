//! Function-level `"use server"` directives and the name they attach to.
//!
//! A directive at the top of a function body turns that one closure into a
//! server action, so the pass has to decide which `{` opens a function body at
//! all — the single place the scanner reasons about syntax rather than tokens
//! — and then walk backwards to whatever names the function, falling back to a
//! stable ordinal when nothing does.

use compact_str::CompactString;
use uf_infra::LineIndex;

use crate::scan::{Token, TokenKind, matching_open};

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

/// Where the head of a function whose body opens at `brace` sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionHead {
    /// Token index of the `=>` of an arrow function.
    Arrow(usize),
    /// Token index of the `)` closing the parameter list.
    Params(usize),
}

/// Decide whether the `{` at `brace` opens a function body.
///
/// This is the one place the lexer has to reason about syntax. The walk goes
/// backwards from the brace, skipping a Flow return-type annotation, and stops
/// at the first token that decides the question:
///
/// * `=>` — an arrow function body;
/// * `)`  — a parameter list, unless the token before its `(` is a control-flow
///   keyword, which is what separates `function f() {` from `if (c) {`;
/// * anything else — a block, an object literal, or a class body.
fn function_head(source: &str, tokens: &[Token], brace: usize) -> Option<FunctionHead> {
    const MAX_TYPE_TOKENS: usize = 128;

    let mut at = brace.checked_sub(1)?;
    for _ in 0..MAX_TYPE_TOKENS {
        let token = tokens.get(at)?;
        match token.kind {
            TokenKind::Arrow => return Some(FunctionHead::Arrow(at)),
            TokenKind::Punct(b')') => {
                let open = matching_open(tokens, at, b'(', b')')?;
                let previous = open.checked_sub(1)?;
                let head = tokens.get(previous)?;
                if head.kind == TokenKind::Ident
                    && matches!(
                        head.text(source),
                        "if" | "for" | "while" | "switch" | "catch" | "with"
                    )
                {
                    return None;
                }
                return Some(FunctionHead::Params(at));
            }
            // Tokens a Flow return-type annotation is made of.
            TokenKind::Ident
            | TokenKind::String
            | TokenKind::Number
            | TokenKind::Punct(
                b':' | b'<' | b'>' | b'|' | b'&' | b'?' | b'.' | b'[' | b']' | b'+',
            ) => {
                if token.kind == TokenKind::Ident
                    && matches!(token.text(source), "else" | "try" | "do" | "finally")
                {
                    return None;
                }
                at = at.checked_sub(1)?;
            }
            _ => return None,
        }
    }
    None
}

/// Best-effort name for the function whose head is at `head`.
fn function_owner(
    source: &str,
    tokens: &[Token],
    head: FunctionHead,
    anonymous: &mut u32,
) -> FunctionOwner {
    let params_start = match head {
        FunctionHead::Arrow(arrow) => arrow
            .checked_sub(1)
            .map(|before| {
                if tokens[before].is_punct(b')') {
                    matching_open(tokens, before, b'(', b')').unwrap_or(before)
                } else {
                    before
                }
            })
            .unwrap_or(arrow),
        FunctionHead::Params(close) => matching_open(tokens, close, b'(', b')').unwrap_or(close),
    };

    if let Some(name) = binding_name(source, tokens, params_start) {
        return FunctionOwner::Named(name);
    }

    let ordinal = *anonymous;
    *anonymous = anonymous.saturating_add(1);
    FunctionOwner::Anonymous { ordinal }
}

/// Walk backwards from the parameter list to whatever names the function.
fn binding_name(source: &str, tokens: &[Token], params_start: usize) -> Option<CompactString> {
    const MAX_HEAD_TOKENS: usize = 16;

    let mut at = params_start;
    for _ in 0..MAX_HEAD_TOKENS {
        let previous = at.checked_sub(1)?;
        let token = tokens.get(previous)?;
        match token.kind {
            TokenKind::Ident => {
                let text = token.text(source);
                if matches!(text, "function" | "async" | "hook" | "component") {
                    at = previous;
                    continue;
                }
                return Some(CompactString::from(text));
            }
            TokenKind::Punct(b'*') => {
                at = previous;
                continue;
            }
            // Generic parameter list of a method or function.
            TokenKind::Punct(b'>') => {
                at = matching_open(tokens, previous, b'<', b'>')?;
                continue;
            }
            TokenKind::Punct(b'=' | b':') => {
                let name = tokens.get(previous.checked_sub(1)?)?;
                return match name.kind {
                    TokenKind::Ident => Some(CompactString::from(name.text(source))),
                    TokenKind::String => Some(CompactString::from(name.quoted_content(source))),
                    _ => None,
                };
            }
            _ => return None,
        }
    }
    None
}
