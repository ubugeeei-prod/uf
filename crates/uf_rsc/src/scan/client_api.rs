//! Detection of client-only React APIs and browser globals in a module.
//!
//! A Server Component that calls `useState` or touches `window` fails at render
//! time, so the graph needs the call sites to report them before the app runs.
//! Declaration sites are skipped: a module that defines its own `useState` is
//! not reaching for React's.

use uf_infra::LineIndex;

use super::lexer::{Token, TokenKind};
use super::{CLIENT_ONLY_APIS, CLIENT_ONLY_GLOBALS, ClientApiUse, ClientApiUseList, clamp_u32};

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
