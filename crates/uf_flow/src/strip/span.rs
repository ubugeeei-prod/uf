//! Where a Flow construct ends.
//!
//! Erasure is only ever as safe as its end-of-construct answers, so every scan
//! here is bounded, bracket-aware and refuses rather than guesses: a scan that
//! cannot find its end returns [`None`] and the eraser leaves those bytes
//! alone. Nothing in this module allocates.

use crate::scan::{Token, TokenKind};

/// Token index one past the end of a type that starts at `from`.
///
/// Types end at a `,`, `;`, `=`, or a closing bracket at depth zero, and at a
/// `{` that opens a function body rather than an object type. A `=>` continues
/// the type only when it follows a `)`, which is what separates the function
/// type `(a: A) => B` from an arrow whose return type is `U` in `(): U => a`.
pub(crate) fn type_end(tokens: &[Token], from: usize) -> usize {
    let mut depth: usize = 0;
    let mut at = from;
    let mut first = true;

    while at < tokens.len() {
        let token = tokens[at];
        match token.kind {
            TokenKind::Punct(b'(' | b'[' | b'<') => depth += 1,
            TokenKind::Punct(b'{') => {
                if depth == 0 && !first {
                    return at;
                }
                depth += 1;
            }
            TokenKind::Punct(b')' | b']' | b'}' | b'>') => {
                if depth == 0 {
                    return at;
                }
                depth -= 1;
            }
            TokenKind::Punct(b',' | b';' | b'=') if depth == 0 => return at,
            // `(a: A) => B` keeps going; `(): U => value` stops before the
            // arrow, because `U` was the whole return type.
            TokenKind::Arrow if depth == 0 && (at == from || !tokens[at - 1].is_punct(b')')) => {
                return at;
            }
            _ => {}
        }
        first = false;
        at += 1;
    }

    tokens.len()
}

/// Token index one past the end of a statement that starts at `from`.
///
/// `brace_terminates` says whether a `{` at depth zero opens the statement's
/// body — true for `interface X { … }` and `declare module x { … }`, false for
/// `type X = { … };` and `import type { A } from "…";`, where the statement
/// carries on after the closing brace.
pub(crate) fn statement_end(tokens: &[Token], from: usize, brace_terminates: bool) -> usize {
    let mut depth: usize = 0;
    let mut opened_body = false;
    let mut at = from;

    while at < tokens.len() {
        let token = tokens[at];
        if depth == 0 && at > from && token.newline_before && breaks_statement(tokens, at) {
            return at;
        }
        match token.kind {
            TokenKind::Punct(b'(' | b'[') => depth += 1,
            TokenKind::Punct(b'{') => {
                if depth == 0 && brace_terminates {
                    opened_body = true;
                }
                depth += 1;
            }
            TokenKind::Punct(b')' | b']') => depth = depth.saturating_sub(1),
            TokenKind::Punct(b'}') => {
                depth = depth.saturating_sub(1);
                if depth == 0 && opened_body {
                    return with_trailing_semicolon(tokens, at + 1);
                }
            }
            TokenKind::Punct(b';') if depth == 0 => return at + 1,
            _ => {}
        }
        at += 1;
    }

    tokens.len()
}

fn with_trailing_semicolon(tokens: &[Token], at: usize) -> usize {
    match tokens.get(at) {
        Some(token) if token.is_punct(b';') => at + 1,
        _ => at,
    }
}

/// Whether a line break before `at` really ends the statement before it.
///
/// This is the automatic-semicolon-insertion question, answered conservatively:
/// a break only ends a statement when neither side is obviously mid-expression.
/// It keeps the leading-`|` union style — `type X =\n  | A\n  | B;` — in one
/// piece instead of erasing only its first line.
fn breaks_statement(tokens: &[Token], at: usize) -> bool {
    let Some(previous) = at.checked_sub(1).and_then(|index| tokens.get(index)) else {
        return false;
    };
    let current = &tokens[at];
    !continues_before(previous) && !continues_after(current)
}

fn continues_before(token: &Token) -> bool {
    match token.kind {
        TokenKind::Arrow => true,
        TokenKind::Punct(byte) => matches!(
            byte,
            b'=' | b'|'
                | b'&'
                | b','
                | b'('
                | b'['
                | b'<'
                | b':'
                | b'?'
                | b'.'
                | b'+'
                | b'-'
                | b'*'
        ),
        _ => false,
    }
}

fn continues_after(token: &Token) -> bool {
    match token.kind {
        TokenKind::Arrow => true,
        TokenKind::Punct(byte) => matches!(
            byte,
            b'|' | b'&' | b'>' | b')' | b']' | b'}' | b',' | b'.' | b'=' | b';' | b'?' | b'['
        ),
        _ => false,
    }
}

/// Token index of the `>` closing a `<` at `from`, if there is one.
///
/// Angle brackets nest, and everything else must stay balanced: a scan that
/// meets a closer it never opened, a statement terminator, or the end of the
/// token vector gives up rather than picking a plausible `>`.
pub(crate) fn matching_angle(tokens: &[Token], from: usize) -> Option<usize> {
    debug_assert!(tokens[from].is_punct(b'<'));

    let mut angles: usize = 0;
    let mut brackets: usize = 0;
    let mut at = from;
    let limit = (from + MAX_TYPE_TOKENS).min(tokens.len());

    while at < limit {
        let token = tokens[at];
        match token.kind {
            TokenKind::Punct(b'<') if brackets == 0 => angles += 1,
            TokenKind::Punct(b'>') if brackets == 0 => {
                angles = angles.checked_sub(1)?;
                if angles == 0 {
                    return Some(at);
                }
            }
            TokenKind::Punct(b'(' | b'[' | b'{') => brackets += 1,
            TokenKind::Punct(b')' | b']' | b'}') => brackets = brackets.checked_sub(1)?,
            TokenKind::Punct(b';' | b'!') => return None,
            TokenKind::Regex | TokenKind::Invalid => return None,
            _ => {}
        }
        at += 1;
    }

    None
}

/// Longest run of tokens a single type is allowed to span.
///
/// Sources are untrusted, and an unbalanced `<` in a hostile file would
/// otherwise make every later `<` re-scan to the end of the token vector.
const MAX_TYPE_TOKENS: usize = 4096;

/// Whether the `<` at `from` opens the type arguments of a call.
///
/// `createQuery<string>({ … })` is a call with type arguments;
/// `a < b` is a comparison. The two are told apart by requiring a balanced `>`
/// whose next token is `(`, which no comparison chain produces without also
/// looking exactly like a generic call.
pub(crate) fn call_type_arguments(tokens: &[Token], from: usize) -> Option<usize> {
    let close = matching_angle(tokens, from)?;
    match tokens.get(close + 1) {
        Some(token) if token.is_punct(b'(') => Some(close),
        _ => None,
    }
}
