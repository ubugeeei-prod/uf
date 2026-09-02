//! Deciding which byte spans are Flow-only, and what takes their place.
//!
//! The walk is a single forward pass over the token vector with a bracket
//! stack, and every rule is a local decision about a handful of neighbouring
//! tokens. Rules that cannot prove what they are looking at do nothing, so the
//! eraser under-erases rather than producing bytes it did not understand.

use super::span::{call_type_arguments, type_end};
use crate::scan::{Token, TokenKind};

mod declaration;

use declaration::erase_declaration;

/// What replaces an erased span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Replacement {
    /// Overwrite with spaces, keeping every line terminator, so byte offsets
    /// and line numbers survive erasure untouched.
    Blank,
    /// Replace with fixed text that carries no line terminator.
    Text(&'static str),
}

/// One rewrite, as a byte range and what goes in its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) replacement: Replacement,
}

impl Edit {
    pub(super) fn blank(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            replacement: Replacement::Blank,
        }
    }

    pub(super) fn text(start: usize, end: usize, text: &'static str) -> Self {
        Self {
            start,
            end,
            replacement: Replacement::Text(text),
        }
    }

    pub(super) fn insert(at: usize, text: &'static str) -> Self {
        Self::text(at, at, text)
    }
}

/// What kind of bracket the walk is currently inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bracket {
    /// `(` — a parameter list, a call, or a parenthesized expression.
    Paren,
    /// `[` — an array or a computed member.
    Square,
    /// `{` — an object literal, a block, or a body.
    Brace,
    /// `{` that opens a class body, where `name: Type` is an annotation.
    ClassBody,
}

/// Collect every erasure the source needs, ordered and non-overlapping.
pub(crate) fn collect_edits(source: &str, tokens: &[Token]) -> Vec<Edit> {
    let mut edits: Vec<Edit> = Vec::new();
    let mut stack: Vec<Bracket> = Vec::new();
    let mut expect_class_body = false;
    let mut at = 0;

    while at < tokens.len() {
        if let Some(next) = apply_rule(
            source,
            tokens,
            at,
            &stack,
            &mut expect_class_body,
            &mut edits,
        ) {
            debug_assert!(next > at);
            at = next;
            continue;
        }

        match tokens[at].kind {
            TokenKind::Punct(b'(') => stack.push(Bracket::Paren),
            TokenKind::Punct(b'[') => stack.push(Bracket::Square),
            TokenKind::Punct(b'{') => {
                stack.push(if expect_class_body {
                    Bracket::ClassBody
                } else {
                    Bracket::Brace
                });
                expect_class_body = false;
            }
            TokenKind::Punct(b')' | b']' | b'}') => {
                stack.pop();
            }
            _ => {}
        }
        at += 1;
    }

    edits.sort_by_key(|edit| (edit.start, edit.end));
    edits.dedup();
    edits
}

/// Try every rule at `at`, returning where to resume when one fired and
/// consumed more than the current token.
fn apply_rule(
    source: &str,
    tokens: &[Token],
    at: usize,
    stack: &[Bracket],
    expect_class_body: &mut bool,
    edits: &mut Vec<Edit>,
) -> Option<usize> {
    match tokens[at].kind {
        TokenKind::Punct(b':') => erase_annotation(source, tokens, at, stack, edits),
        TokenKind::Punct(b'<') => erase_call_type_arguments(tokens, at, edits),
        TokenKind::Ident => erase_declaration(source, tokens, at, expect_class_body, edits),
        _ => None,
    }
}

/// Erase a type annotation, and the `?` of an optional binding with it.
fn erase_annotation(
    source: &str,
    tokens: &[Token],
    at: usize,
    stack: &[Bracket],
    edits: &mut Vec<Edit>,
) -> Option<usize> {
    let start = annotation_start(source, tokens, at, stack)?;
    let end = type_end(tokens, at + 1);
    if end <= at + 1 {
        return None;
    }
    edits.push(Edit::blank(tokens[start].start, tokens[end - 1].end));
    Some(end)
}

/// The first token of an annotation ending at the `:` at `at`, if this `:`
/// really introduces a type.
///
/// A `:` is also an object-literal separator, a ternary arm, a `case` label and
/// a labelled statement, so the rules below only fire where JavaScript itself
/// has no other reading: after a `)`, in a parameter list, on a `const`/`let`/
/// `var` declarator, and on a class member.
fn annotation_start(source: &str, tokens: &[Token], at: usize, stack: &[Bracket]) -> Option<usize> {
    let previous = at.checked_sub(1)?;
    let token = tokens[previous];

    // `):` is not valid JavaScript in any position, so it is always a return type.
    if token.is_punct(b')') {
        return Some(at);
    }

    // A destructured parameter: `({ a, b }: Props) => …`.
    if (token.is_punct(b'}') || token.is_punct(b']')) && stack.last() == Some(&Bracket::Paren) {
        return Some(at);
    }

    // `name:` and the optional form `name?:`.
    let (name_index, start) = if token.is_punct(b'?') {
        (previous.checked_sub(1)?, previous)
    } else {
        (previous, at)
    };
    if tokens[name_index].kind != TokenKind::Ident {
        return None;
    }
    // Step back over the dots of a rest parameter, `(...rest: Array<T>)`.
    let mut before_index = name_index.checked_sub(1)?;
    while tokens[before_index].is_punct(b'.') {
        before_index = before_index.checked_sub(1)?;
    }
    let before = &tokens[before_index];

    if stack.last() == Some(&Bracket::Paren) && (before.is_punct(b'(') || before.is_punct(b',')) {
        return Some(start);
    }
    if before.is_ident(source, "const") || before.is_ident(source, "let") {
        return Some(start);
    }
    if before.is_ident(source, "var") {
        return Some(start);
    }
    if stack.last() == Some(&Bracket::ClassBody) && starts_class_member(before) {
        return Some(start);
    }
    None
}

fn starts_class_member(before: &Token) -> bool {
    matches!(before.kind, TokenKind::Ident)
        || before.is_punct(b'{')
        || before.is_punct(b'}')
        || before.is_punct(b';')
        || before.is_punct(b'+')
        || before.is_punct(b'-')
}

/// Erase the type arguments of a generic call: `createQuery<string>({ … })`.
fn erase_call_type_arguments(tokens: &[Token], at: usize, edits: &mut Vec<Edit>) -> Option<usize> {
    let previous = tokens.get(at.checked_sub(1)?)?;
    if previous.kind != TokenKind::Ident {
        return None;
    }
    let close = call_type_arguments(tokens, at)?;
    edits.push(Edit::blank(tokens[at].start, tokens[close].end));
    Some(close + 1)
}
