//! Detection of client-only React APIs and browser globals in a module.
//!
//! A Server Component that calls `useState` or touches `window` fails at render
//! time, so the graph needs the call sites to report them before the app runs.
//! Declaration sites are skipped: a module that defines its own `useState` is
//! not reaching for React's, and that includes the method form — `useState() {}`
//! in a class body or an object literal writes a function rather than calling
//! one.
//!
//! The match is by name, and the name is the whole of what it knows. That is
//! sound in the direction it claims — the two lists are React's own APIs and
//! the browser's own globals, and nothing else is reported as one — and it is
//! silent in the other: `useRoute` is built on `useContext`, and a call to
//! `useRoute` looks like a call to nothing at all. [`hook_calls_from_tokens`]
//! is the second half of that sentence, said out loud.
//!
//! Both collectors record, beside the line and the column, the module-level
//! declaration the use sits in — `super::owner` has the rule and what it
//! declines to answer. That is what makes "this module reaches `useState`" into
//! "`useTheme` does", which is the difference between a fact about a file and
//! an edge an export graph can propagate along.

use compact_str::CompactString;
use uf_infra::LineIndex;

use super::lexer::{Token, TokenKind, matching_close};
use super::owner::{OwnerSpan, owner_at};
use super::{
    CLIENT_ONLY_APIS, CLIENT_ONLY_GLOBALS, ClientApiUse, ClientApiUseList, HookCall, HookCallList,
    clamp_u32,
};

pub(crate) fn client_api_uses_from_tokens(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
    owners: &[OwnerSpan],
) -> ClientApiUseList {
    let mut uses = ClientApiUseList::new();
    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let text = token.text(source);
        if is_declaration_site(source, tokens, position) {
            continue;
        }

        let matched = if tokens
            .get(position + 1)
            .is_some_and(|next| next.is_punct(b'('))
        {
            CLIENT_ONLY_APIS
                .binary_search(&text)
                .ok()
                .map(|found| CLIENT_ONLY_APIS[found])
        } else {
            None
        };

        let matched = matched.or_else(|| {
            if position
                .checked_sub(1)
                .is_some_and(|previous| tokens[previous].is_punct(b'.'))
            {
                return None;
            }
            CLIENT_ONLY_GLOBALS
                .binary_search(&text)
                .ok()
                .map(|found| CLIENT_ONLY_GLOBALS[found])
        });

        if let Some(api) = matched {
            let at = index.line_col(token.start);
            uses.push(ClientApiUse {
                api,
                line: clamp_u32(at.line),
                column: clamp_u32(at.column),
                owner: owner_at(owners, position),
            });
        }
    }
    uses
}

/// Whether the identifier at `position` is being declared rather than used.
fn is_declaration_site(source: &str, tokens: &[Token], position: usize) -> bool {
    let Some(previous) = position.checked_sub(1) else {
        return false;
    };
    let token = &tokens[previous];
    if token.kind == TokenKind::Ident
        && matches!(
            token.text(source),
            "function" | "hook" | "component" | "class" | "const" | "let" | "var" | "import"
        )
    {
        return true;
    }
    is_method_definition(source, tokens, position)
}

/// Whether the identifier at `position` names a method being defined.
///
/// `useRoute() {}` in an object literal, a class body or a Flow object type
/// declares a function; it does not run one. Every check in this module claims
/// to have found a *use* — a hook that runs where the server runs, a browser
/// global touched during a render — and a warning on the line that writes the
/// hook is a warning about a line that does nothing. That is worse than no
/// warning: a rule which fires where the reader can see it is wrong is a rule
/// they learn to skip, including the times it is right.
///
/// Recognised by two facts, because neither is enough on its own:
///
/// * The name sits where a member goes — after the brace opening the body,
///   after the comma or the closing brace of the previous member, after
///   `static`, `async`, `get` or `set`, or after a generator's `*`. Alone this
///   catches calls, since `{`, `}`, `,` and `;` precede ordinary expressions
///   too: `f(a, useRoute())` and `{ useRoute(); }` are both calls.
/// * The parameter list is followed by the body — `{`, or `:` and a Flow
///   return type first. Alone this catches nothing useful, but together the
///   pair is tight: `if (useRoute())` and `f(a, useRoute())` close into a `)`,
///   and the `:` of `cond ? useRoute() : x` is reached from a `?`, which is
///   not a member position.
fn is_method_definition(source: &str, tokens: &[Token], position: usize) -> bool {
    if !tokens
        .get(position + 1)
        .is_some_and(|next| next.is_punct(b'('))
    {
        return false;
    }
    let Some(previous) = position.checked_sub(1) else {
        return false;
    };
    if !is_member_position(source, tokens, previous) {
        return false;
    }
    let Some(close) = matching_close(tokens, position + 1, b'(', b')') else {
        return false;
    };
    tokens
        .get(close + 1)
        .is_some_and(|next| next.is_punct(b'{') || next.is_punct(b':'))
}

/// Whether a member name can follow the token at `at`.
fn is_member_position(source: &str, tokens: &[Token], at: usize) -> bool {
    let token = &tokens[at];
    if token.is_punct(b'|') {
        // The `|` of Flow's `{| … |}` belongs to the brace that opens the
        // object type, so a member follows it. A `|` anywhere else is an
        // operator and what follows it is an expression — `a | useRoute()`.
        return at
            .checked_sub(1)
            .is_some_and(|before| tokens[before].is_punct(b'{'));
    }
    if token.kind == TokenKind::Ident {
        return matches!(token.text(source), "static" | "async" | "get" | "set");
    }
    token.is_punct(b'{')
        || token.is_punct(b'}')
        || token.is_punct(b',')
        || token.is_punct(b';')
        || token.is_punct(b'*')
}

/// Calls to hooks the name lists do not know.
///
/// React's naming rule — `use` followed by a capital — is the only thing a
/// token stream can go on, and it is what React itself goes on: a function
/// named that way is a hook, and one not named that way is not, enforced by
/// the compiler and by every lint rule in the ecosystem. So a called
/// `use[A-Z]…` that is not on [`CLIENT_ONLY_APIS`] is exactly the set of
/// places where "does this run on the server" is a question this scanner
/// asked and could not answer.
///
/// It deliberately does not guess. A hook that wraps `useContext` is
/// client-only; a hook that wraps nothing but a `fetch` is not; and the
/// difference is in another module's body, which is a graph question
/// (ubugeeei-prod/uf#388). What is recorded here is the call and the name, so
/// something downstream can say "uf does not know" rather than say nothing.
pub(crate) fn hook_calls_from_tokens(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
    owners: &[OwnerSpan],
) -> HookCallList {
    let mut calls = HookCallList::new();
    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let text = token.text(source);
        if !is_hook_name(text) || CLIENT_ONLY_APIS.binary_search(&text).is_ok() {
            continue;
        }
        // The same exclusions the API match makes, for the same reasons: a
        // declaration is not a use — including `useRoute() {}`, which defines
        // the hook rather than running it — `theme.useRoute` is somebody's
        // method, and an identifier that is not called is not a hook being run
        // here.
        if is_declaration_site(source, tokens, position) {
            continue;
        }
        if position
            .checked_sub(1)
            .is_some_and(|previous| tokens[previous].is_punct(b'.'))
        {
            continue;
        }
        if !tokens
            .get(position + 1)
            .is_some_and(|next| next.is_punct(b'('))
        {
            continue;
        }
        let at = index.line_col(token.start);
        calls.push(HookCall {
            name: CompactString::new(text),
            line: clamp_u32(at.line),
            column: clamp_u32(at.column),
            owner: owner_at(owners, position),
        });
    }
    calls
}

/// React's rule for what a hook is called: `use` and then a capital.
fn is_hook_name(text: &str) -> bool {
    text.strip_prefix("use")
        .and_then(|rest| rest.chars().next())
        .is_some_and(char::is_uppercase)
}
