//! Detection of client-only React APIs and browser globals in a module.
//!
//! A Server Component that calls `useState` or touches `window` fails at render
//! time, so the graph needs the call sites to report them before the app runs.
//! Declaration sites are skipped: a module that defines its own `useState` is
//! not reaching for React's.
//!
//! The match is by name, and the name is the whole of what it knows. That is
//! sound in the direction it claims — the two lists are React's own APIs and
//! the browser's own globals, and nothing else is reported as one — and it is
//! silent in the other: `useRoute` is built on `useContext`, and a call to
//! `useRoute` looks like a call to nothing at all. [`hook_calls_from_tokens`]
//! is the second half of that sentence, said out loud.

use compact_str::CompactString;
use uf_infra::LineIndex;

use super::lexer::{Token, TokenKind};
use super::{
    CLIENT_ONLY_APIS, CLIENT_ONLY_GLOBALS, ClientApiUse, ClientApiUseList, HookCall, HookCallList,
    clamp_u32,
};

pub(crate) fn client_api_uses_from_tokens(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
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
            let position = index.line_col(token.start);
            uses.push(ClientApiUse {
                api,
                line: clamp_u32(position.line),
                column: clamp_u32(position.column),
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
    token.kind == TokenKind::Ident
        && matches!(
            token.text(source),
            "function" | "hook" | "component" | "class" | "const" | "let" | "var" | "import"
        )
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
        // declaration is not a use, `theme.useRoute` is somebody's method, and
        // an identifier that is not called is not a hook being run here.
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
