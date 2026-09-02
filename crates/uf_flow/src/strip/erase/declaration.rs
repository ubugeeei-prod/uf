//! The declaration rules: what a Flow keyword at the start of a statement does.
//!
//! Every rule here proves what it is looking at from the two or three tokens
//! around the keyword before it touches a byte. A rule that cannot returns
//! [`None`], the walk moves on by one token, and the declaration is left in the
//! output — under-erasing rather than rewriting something nobody understood.

use crate::scan::{Token, TokenKind, matching_close, starts_statement};

use super::super::span::{matching_angle, statement_end};
use super::Edit;

/// Longest import or export clause the eraser will scan.
const MAX_CLAUSE_TOKENS: usize = 4096;

/// Erase or rewrite a Flow declaration introduced by the identifier at `at`.
pub(super) fn erase_declaration(
    source: &str,
    tokens: &[Token],
    at: usize,
    expect_class_body: &mut bool,
    edits: &mut Vec<Edit>,
) -> Option<usize> {
    let keyword = tokens[at].text(source);
    let exported = is_export_prefixed(source, tokens, at);
    let statement = starts_statement(tokens, at) || exported;

    match keyword {
        "import" if starts_statement(tokens, at) => erase_import(source, tokens, at, edits),
        "export" if starts_statement(tokens, at) => erase_export(source, tokens, at, edits),
        "type" | "opaque" if statement && names_a_type(source, tokens, at) => {
            let end = statement_end(tokens, at, false);
            blank_statement(tokens, at, end, exported, edits)
        }
        "interface" | "declare" if statement && followed_by_ident(tokens, at) => {
            let end = statement_end(tokens, at, true);
            blank_statement(tokens, at, end, exported, edits)
        }
        "component" if statement => rewrite_component(source, tokens, at, edits),
        "hook" if statement && followed_by_ident(tokens, at) => {
            edits.push(Edit::text(tokens[at].start, tokens[at].end, "function"));
            Some(at + 1)
        }
        "class" => {
            *expect_class_body = true;
            erase_class_heritage(source, tokens, at, edits);
            Some(at + 1)
        }
        _ => None,
    }
}

/// Whether `type`/`opaque` here introduces a type alias rather than being used
/// as an ordinary identifier such as `const o = { type };`.
fn names_a_type(source: &str, tokens: &[Token], at: usize) -> bool {
    if tokens[at].is_ident(source, "opaque") {
        return tokens
            .get(at + 1)
            .is_some_and(|next| next.is_ident(source, "type"));
    }
    followed_by_ident(tokens, at)
}

/// Whether the token after `at` is an identifier.
fn followed_by_ident(tokens: &[Token], at: usize) -> bool {
    tokens
        .get(at + 1)
        .is_some_and(|next| next.kind == TokenKind::Ident)
}

fn is_export_prefixed(source: &str, tokens: &[Token], at: usize) -> bool {
    let Some(previous) = at.checked_sub(1) else {
        return false;
    };
    if tokens[previous].is_ident(source, "export") && starts_statement(tokens, previous) {
        return true;
    }
    if !tokens[previous].is_ident(source, "default") {
        return false;
    }
    previous.checked_sub(1).is_some_and(|index| {
        tokens[index].is_ident(source, "export") && starts_statement(tokens, index)
    })
}

/// Blank a whole statement, taking the `export` keyword in front of it too.
fn blank_statement(
    tokens: &[Token],
    at: usize,
    end: usize,
    exported: bool,
    edits: &mut Vec<Edit>,
) -> Option<usize> {
    let first = if exported { at.saturating_sub(1) } else { at };
    let last = end.checked_sub(1)?;
    edits.push(Edit::blank(tokens[first].start, tokens.get(last)?.end));
    Some(end)
}

/// `import type …` and `import { type A, b } from "…"`.
fn erase_import(source: &str, tokens: &[Token], at: usize, edits: &mut Vec<Edit>) -> Option<usize> {
    let next = tokens.get(at + 1)?;
    if (next.is_ident(source, "type") || next.is_ident(source, "typeof"))
        && tokens.get(at + 2).is_some_and(|after| {
            after.is_punct(b'{') || after.is_punct(b'*') || !after.is_ident(source, "from")
        })
    {
        let end = statement_end(tokens, at, false);
        return blank_statement(tokens, at, end, false, edits);
    }
    erase_named_type_specifiers(source, tokens, at, import_clause_brace(tokens, at)?, edits)
}

/// `export type …` and `export { type A, b }`.
fn erase_export(source: &str, tokens: &[Token], at: usize, edits: &mut Vec<Edit>) -> Option<usize> {
    let next = tokens.get(at + 1)?;
    if next.is_ident(source, "type") || next.is_ident(source, "interface") {
        let brace_terminates = next.is_ident(source, "interface");
        let end = statement_end(tokens, at, brace_terminates);
        return blank_statement(tokens, at, end, false, edits);
    }
    if !next.is_punct(b'{') {
        return None;
    }
    erase_named_type_specifiers(source, tokens, at, at + 1, edits)
}

/// The `{` of an `import` clause: `import { … }` or `import a, { … }`.
///
/// Deliberately not a forward scan for the next `{`: the first brace after
/// `export default function f()` is a function body, and treating that as a
/// specifier list would blank code rather than types.
fn import_clause_brace(tokens: &[Token], at: usize) -> Option<usize> {
    if tokens.get(at + 1)?.is_punct(b'{') {
        return Some(at + 1);
    }
    if tokens.get(at + 1)?.kind == TokenKind::Ident
        && tokens.get(at + 2)?.is_punct(b',')
        && tokens.get(at + 3)?.is_punct(b'{')
    {
        return Some(at + 3);
    }
    None
}

/// Blank the `type`/`typeof` specifiers of a named import or export clause.
///
/// When every specifier was type-only the whole statement goes, so a module
/// that only ever supplied types is not pulled into the bundle for its side
/// effects.
fn erase_named_type_specifiers(
    source: &str,
    tokens: &[Token],
    at: usize,
    brace: usize,
    edits: &mut Vec<Edit>,
) -> Option<usize> {
    let close = matching_close(tokens, brace, b'{', b'}')?;

    let mut removed: Vec<Edit> = Vec::new();
    let mut kept = 0usize;
    let mut specifier = brace + 1;

    while specifier < close {
        let end = specifier_end(tokens, specifier, close);
        if end == specifier {
            break;
        }
        let is_type = tokens[specifier].is_ident(source, "type")
            || tokens[specifier].is_ident(source, "typeof");
        if is_type && end > specifier + 1 {
            let (from, to) = with_separator(tokens, specifier, end, brace, close);
            removed.push(Edit::blank(tokens[from].start, tokens[to].end));
        } else {
            kept += 1;
        }
        specifier = if tokens.get(end).is_some_and(|token| token.is_punct(b',')) {
            end + 1
        } else {
            end
        };
    }

    if removed.is_empty() {
        return None;
    }
    if kept == 0 && brace == at + 1 {
        let end = statement_end(tokens, at, false);
        return blank_statement(tokens, at, end, false, edits);
    }
    edits.extend(removed);
    Some(close + 1)
}

/// Token index one past a specifier that starts at `from`.
fn specifier_end(tokens: &[Token], from: usize, close: usize) -> usize {
    let mut at = from;
    while at < close && !tokens[at].is_punct(b',') {
        at += 1;
    }
    at
}

/// Widen a specifier span to swallow the comma that separated it.
fn with_separator(
    tokens: &[Token],
    from: usize,
    end: usize,
    brace: usize,
    close: usize,
) -> (usize, usize) {
    if end < close && tokens[end].is_punct(b',') {
        return (from, end);
    }
    if from > brace + 1 && tokens[from - 1].is_punct(b',') {
        return (from - 1, end - 1);
    }
    (from, end - 1)
}

/// Blank type parameters and an `implements` clause on a class declaration.
fn erase_class_heritage(source: &str, tokens: &[Token], at: usize, edits: &mut Vec<Edit>) {
    let limit = (at + MAX_CLAUSE_TOKENS).min(tokens.len());
    let mut index = at + 1;
    while index < limit {
        let token = tokens[index];
        if token.is_punct(b'{') || token.is_punct(b';') {
            return;
        }
        if token.is_punct(b'<')
            && let Some(close) = matching_angle(tokens, index)
        {
            edits.push(Edit::blank(token.start, tokens[close].end));
            index = close + 1;
            continue;
        }
        if token.is_ident(source, "implements") {
            let body = body_brace(tokens, index).unwrap_or(limit);
            if let Some(last) = body.checked_sub(1) {
                edits.push(Edit::blank(token.start, tokens[last].end));
            }
            return;
        }
        index += 1;
    }
}

/// `component Name(props) renders Node { … }` becomes a plain function whose
/// single parameter destructures the props it declared.
fn rewrite_component(
    source: &str,
    tokens: &[Token],
    at: usize,
    edits: &mut Vec<Edit>,
) -> Option<usize> {
    if tokens.get(at + 1)?.kind != TokenKind::Ident {
        return None;
    }
    let mut open = at + 2;
    if tokens.get(open)?.is_punct(b'<') {
        let close = matching_angle(tokens, open)?;
        edits.push(Edit::blank(tokens[open].start, tokens[close].end));
        open = close + 1;
    }
    if !tokens.get(open)?.is_punct(b'(') {
        return None;
    }
    let close = matching_close(tokens, open, b'(', b')')?;

    // `component` and `function ` are both nine bytes, so the rewrite keeps
    // every later offset on this line exactly where it was.
    edits.push(Edit::text(tokens[at].start, tokens[at].end, "function "));

    if close > open + 1 && destructurable(source, tokens, open, close) {
        edits.push(Edit::insert(tokens[open].end, "{"));
        edits.push(Edit::insert(tokens[close].start, "}"));
    }

    if tokens
        .get(close + 1)
        .is_some_and(|token| token.is_ident(source, "renders"))
        && let Some(body) = body_brace(tokens, close + 1)
    {
        edits.push(Edit::blank(tokens[close + 1].start, tokens[body].start));
    }

    Some(at + 1)
}

/// Whether a component's parameter list is a plain list of prop names.
///
/// A string-keyed or `as`-renamed parameter has no destructuring spelling that
/// is also valid JavaScript, so those components keep their positional
/// parameter list rather than being rewritten into something that would not
/// parse.
fn destructurable(source: &str, tokens: &[Token], open: usize, close: usize) -> bool {
    for token in &tokens[open + 1..close] {
        if token.kind == TokenKind::String || token.is_ident(source, "as") {
            return false;
        }
    }
    true
}

/// Index of the `{` that opens a body at or after `from`.
fn body_brace(tokens: &[Token], from: usize) -> Option<usize> {
    let mut depth: usize = 0;
    let limit = (from + MAX_CLAUSE_TOKENS).min(tokens.len());
    for (index, token) in tokens.iter().enumerate().take(limit).skip(from) {
        match token.kind {
            TokenKind::Punct(b'(' | b'[' | b'<') => depth += 1,
            TokenKind::Punct(b')' | b']' | b'>') => depth = depth.saturating_sub(1),
            TokenKind::Punct(b'{') if depth == 0 => return Some(index),
            TokenKind::Punct(b';') if depth == 0 => return None,
            _ => {}
        }
    }
    None
}
