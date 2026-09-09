//! Which declaration's body a token sits inside.
//!
//! A line and a column say *where* a use of `useState` is. They do not say
//! *whose* it is, and that is the fact an export-graph fixpoint needs: an
//! exported function is a client hook when its body reaches a client-only API,
//! so every use site has to be attributable to the body that contains it or
//! there is nothing to propagate. See ubugeeei-prod/uf#388.
//!
//! # The rule
//!
//! The owner of a token is the **module-level declaration whose body contains
//! it**, and there is at most one: a body opened by a `{` that no other
//! bracket encloses runs to that brace's match, and two such spans are
//! therefore disjoint. A token no such span contains — module top-level code,
//! an import clause, the initializer of a plain `const` — has no owner, which
//! is the honest answer and not a fallback to the nearest name.
//!
//! Nested bodies are deliberately not spans of their own. A `useEffect`
//! callback inside `useTheme` belongs to `useTheme` for every question this
//! crate asks, because `useTheme` is the name another module can import; the
//! callback has no name at all. Skipping the body once its span is recorded is
//! what implements that, and it is also what keeps the walk linear.
//!
//! # What it is wrong about
//!
//! * A method body — `class Store { read() { … } }`, or the same shape in an
//!   object literal — is enclosed by the class or object brace, so it is not a
//!   module-level body and its uses have no owner. Naming the method would be
//!   worse: `read` is not a name any importer can reach, and it can collide
//!   with an export that has nothing to do with it.
//! * A declaration the lexer cannot name — `export default (() => { … })()`,
//!   a body reached through a call — records no span, so its uses have no
//!   owner rather than the wrong one.
//! * A binding redefined mid-body. The span says which body a use is in; it
//!   does not say that the `useState` inside it is React's.
//!
//! All three fail towards [`None`], which downstream reads as "uf cannot say"
//! — the state this crate already reports rather than guesses at.

use compact_str::CompactString;

use super::lexer::{Token, TokenKind, matching_close, matching_open};

/// One module-level declaration body, as a token range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerSpan {
    /// Name of the declaration the body belongs to.
    pub(crate) name: CompactString,
    /// Token index of the `{` opening the body.
    pub(crate) open: usize,
    /// Token index of the `}` closing it.
    pub(crate) close: usize,
}

/// Where the head of a function whose body opens at a `{` sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FunctionHead {
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
///
/// # A return type is stepped over, not walked through
///
/// A `]` or a `>` jumps to its opener rather than being stepped past one token
/// at a time, because a Flow return type can contain the very tokens this walk
/// stops at. `hook useTheme(): [Theme, (next: Theme) => void] {` is not an
/// arrow function, and a walk that meets that `=>` before the `)` of the
/// parameter list says it is — which is how the hook the issue was filed about
/// ended up unnameable. An unmatched closer steps back one token, which is what
/// it did before.
///
/// A `}` is not jumped, and an object-type return annotation is the price. The
/// group before a `{` is far more often the body of the *previous* declaration
/// — `function setup() {}` and then a bare block — and walking behind it would
/// hand that block the previous function's parameter list and its name. A
/// wrong owner propagates; a missing one is the answer this crate already
/// gives.
pub(crate) fn function_head(source: &str, tokens: &[Token], brace: usize) -> Option<FunctionHead> {
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
            // A group of the annotation: `[…]` or `<…>`. Behind it in one step,
            // so nothing inside it can be mistaken for the head.
            TokenKind::Punct(b']') => {
                at = matching_open(tokens, at, b'[', b']').unwrap_or(at);
                at = at.checked_sub(1)?;
            }
            TokenKind::Punct(b'>') => {
                at = matching_open(tokens, at, b'<', b'>').unwrap_or(at);
                at = at.checked_sub(1)?;
            }
            // Tokens a Flow return-type annotation is made of.
            TokenKind::Ident
            | TokenKind::String
            | TokenKind::Number
            | TokenKind::Punct(b':' | b'<' | b'|' | b'&' | b'?' | b'.' | b'[' | b'+') => {
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

/// Best-effort name of the function whose head is at `head`.
///
/// [`None`] for a genuine anonymous closure — a callback, an IIFE — which the
/// two callers then say differently: a `"use server"` directive numbers it,
/// and an owner span declines to exist.
pub(crate) fn function_name(
    source: &str,
    tokens: &[Token],
    head: FunctionHead,
) -> Option<CompactString> {
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
    binding_name(source, tokens, params_start)
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

/// The module-level declaration bodies of one module, in source order.
///
/// Linear in the token count: every token is visited once, and a body is
/// skipped whole rather than descended into, so a deeply nested module costs
/// no more than a flat one.
pub(crate) fn owner_spans(source: &str, tokens: &[Token]) -> Vec<OwnerSpan> {
    let mut spans: Vec<OwnerSpan> = Vec::new();
    // Nesting of every bracket kind, not just braces: a `{` inside a call
    // argument or an array is not a module-level declaration's body, and
    // `foo({ bar() { … } })` would otherwise look like one.
    let mut depth = 0usize;
    let mut at = 0usize;

    while at < tokens.len() {
        let token = &tokens[at];
        if token.is_punct(b'{') {
            if depth == 0
                && let Some(head) = function_head(source, tokens, at)
                && let Some(name) = function_name(source, tokens, head)
                && let Some(close) = matching_close(tokens, at, b'{', b'}')
            {
                spans.push(OwnerSpan {
                    name,
                    open: at,
                    close,
                });
                at = close + 1;
                continue;
            }
            depth += 1;
        } else if token.is_punct(b'(') || token.is_punct(b'[') {
            depth += 1;
        } else if token.is_punct(b'}') || token.is_punct(b')') || token.is_punct(b']') {
            // Saturating rather than panicking: an unbalanced source is a
            // syntax error somebody else reports, and this walk still has to
            // finish over it.
            depth = depth.saturating_sub(1);
        }
        at += 1;
    }

    spans
}

/// The declaration whose body contains the token at `position`.
///
/// The spans are disjoint and in ascending order, so the first one that starts
/// after `position` ends the search: no later span can reach back over it.
pub(crate) fn owner_at(spans: &[OwnerSpan], position: usize) -> Option<CompactString> {
    for span in spans {
        if span.open > position {
            break;
        }
        if position < span.close {
            return Some(span.name.clone());
        }
    }
    None
}
