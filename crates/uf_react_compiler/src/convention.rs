//! The `useX` naming convention, and when a module makes it mean React.
//!
//! A `component` and a `hook` declaration say what they are in the syntax. A
//! function named `useThing` does not: the convention is a convention, and
//! `useFakeTimers`, `useRealTimers` and `useTemporaryDirectory` are all names
//! that ordinary modules give to ordinary functions. Reading every one of them
//! as a hook attached the render rules to code with no React in it —
//! `@uniflowed/test`'s own fake timers replace the scheduling globals and write
//! module state on purpose, and `react/no-render-side-effects` reported eight
//! writes in a file that imports no React and will never rename
//! `useFakeTimers`, because Jest, Vitest and Sinon all call it that.
//!
//! So the convention is read as React only inside a module that has something
//! to do with React, which [`react_signal`] decides from the module's own text.
//! One of its four signals is true of every module that really holds a hook: a
//! hook that imports nothing from React is a hook *because it calls one*, and a
//! hook that calls none sits beside the component it was extracted from. None
//! of them is true of a module that merely borrows the name.
//!
//! # Which way the doubt runs
//!
//! The two mistakes are not symmetrical. Deciding wrongly that a module is
//! React costs a false positive, which a reader can see and argue with.
//! Deciding wrongly that it is not silently turns `react/hooks-rules` and
//! `react/no-render-side-effects` off for the whole file, and nothing says so.
//!
//! Every signal below is therefore written to be generous, and each says where
//! it is generous and why. A module this cannot classify is a React module.
//!
//! # What it still cannot see
//!
//! A hook that calls no hook, imports nothing from React, and shares its module
//! with neither JSX nor a `component` is indistinguishable from a plain helper
//! whose name begins with `use` — because from the source text alone it *is*
//! one. `export function useConstant<T>(value: T): T { return value; }` is the
//! shape, and it is no longer checked. That is the price of the fix, and it is
//! the same boundary the rest of this crate holds: what cannot be decided from
//! source text is not guessed at.

use uf_rsc::{Token, TokenKind};

use crate::scope::ident_at;
use crate::syntax::{names_a_declaration, previous_word};

/// What made a module a React module.
///
/// Reported rather than collapsed to a `bool` so that a test can say *which*
/// signal it means to exercise, and so a future diagnostic can explain why a
/// `useX` function was — or was not — held to the rules of hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReactSignal {
    /// A module specifier that names React: `react`, `@uniflowed/react`,
    /// `react-dom/client`, `@testing-library/react`.
    Import,
    /// A `component` or `hook` declaration.
    Declaration,
    /// A JSX element.
    Jsx,
    /// A call to a `useX` function.
    HookCall,
}

/// The first sign that this module has something to do with React, if any.
///
/// A second linear pass over the token vector, and it has to be: the walk
/// decides what a `useX` function's body *is* when the declaration opens, and
/// the evidence that the module is React may stand anywhere — a hook called
/// fifty lines below, an import written at the bottom. The pass stops at the
/// first signal, which in a real React module is almost always its first
/// import.
pub fn react_signal(source: &str, tokens: &[Token]) -> Option<ReactSignal> {
    for index in 0..tokens.len() {
        let token = &tokens[index];
        match token.kind {
            TokenKind::String if names_react(token.quoted_content(source)) => {
                return Some(ReactSignal::Import);
            }
            TokenKind::Punct(b'<' | b'/') if closes_a_jsx_element(source, tokens, index) => {
                return Some(ReactSignal::Jsx);
            }
            TokenKind::Ident => {
                let word = token.text(source);
                if matches!(word, "component" | "hook")
                    && names_a_declaration(source, tokens, index)
                {
                    return Some(ReactSignal::Declaration);
                }
                if is_hook_call(source, tokens, index) {
                    return Some(ReactSignal::HookCall);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether an identifier follows the `useSomething` hook naming convention.
pub fn is_hook_name(name: &str) -> bool {
    name.len() > 3 && name.starts_with("use") && name.as_bytes()[3].is_ascii_uppercase()
}

/// Whether the identifier at `index` is a call to a `useX` function.
///
/// The walk asks this too, at the point where it reports a misplaced hook, and
/// both ask it here so that the two can never disagree: a module in which the
/// walk finds a hook call is a module this pass has already called React, so
/// `react/hooks-rules` can never be switched off by the very call it was about
/// to report.
///
/// The declaration is not a call, which is the whole of the difficulty:
/// `function useFakeTimers()` and `useFakeTimers()` differ only in the word in
/// front. A member call is not one either — `jest.useFakeTimers()` and
/// `api.useThing()` are as common as they are unrelated to React — and leaving
/// them out agrees with the walk, which has never treated a property read as a
/// hook.
pub fn is_hook_call(source: &str, tokens: &[Token], index: usize) -> bool {
    let Some(name) = ident_at(source, tokens, index) else {
        return false;
    };
    if !is_hook_name(name)
        || !tokens
            .get(index + 1)
            .is_some_and(|next| next.is_punct(b'('))
    {
        return false;
    }
    if index
        .checked_sub(1)
        .is_some_and(|before| tokens[before].is_punct(b'.'))
    {
        return false;
    }
    !matches!(
        previous_word(source, tokens, index),
        Some("function" | "const" | "let" | "var" | "component" | "hook" | "class")
    )
}

/// Whether a module specifier names React.
///
/// Deliberately loose in two directions, both of them towards checking. Any
/// path segment that is `react` or begins with `react-` counts, so `react`,
/// `react-dom/client`, `react-native`, `@uniflowed/react/server` and
/// `@testing-library/react` all do; a module that reaches for any of them is a
/// module where a `useX` name is no coincidence.
///
/// And the string is not required to stand in import position. `require("react")`,
/// `import("react")`, `jest.mock("react")` and `export * from "react"` are four
/// syntaxes for the same fact, and a fifth would have been missed. The cost is
/// that a module holding the bare word in some unrelated string is treated as
/// React, which only means it keeps being checked.
fn names_react(specifier: &str) -> bool {
    specifier
        .split('/')
        .any(|segment| segment == "react" || segment.starts_with("react-"))
}

/// Whether the token at `index` opens one of the two shapes that close JSX.
///
/// A uf module may contain JSX with no React import at all — that is what the
/// automatic runtime is for — so this signal carries the modules the import
/// check cannot see, and it has to find every one of them.
///
/// It does, because JSX leaves the same two marks in every module that holds
/// any: an element with children closes with `</name>`, one without closes with
/// `/>`, and JSX allows no space inside either pair. Neither pair is
/// punctuation JavaScript produces on its own — `a </ b` and `a /> b` are not
/// expressions — and the three places the bytes do occur innocently, strings,
/// comments and regular expressions, the lexer has already folded away.
///
/// The second token of `</` is not always the punctuation `/`, and reading it
/// as such was the wrong answer that this now avoids. `<div><span></span></div>`
/// gives the lexer a `/` where a regular expression is allowed to start — the
/// token before it is `<` — and a second `/` on the same line to end it, so
/// `/span></div` lexes as one `Regex` token and the naive check found no JSX in
/// a module that is nothing but JSX. What is constant is the byte: whatever the
/// lexer made of it, the token after the `<` begins with `/`.
fn closes_a_jsx_element(source: &str, tokens: &[Token], index: usize) -> bool {
    let (Some(first), Some(second)) = (tokens.get(index), tokens.get(index + 1)) else {
        return false;
    };
    if first.end != second.start {
        return false;
    }
    if first.is_punct(b'<') {
        return second.text(source).starts_with('/');
    }
    first.is_punct(b'/') && second.is_punct(b'>')
}
