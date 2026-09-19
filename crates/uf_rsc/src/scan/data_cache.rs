//! Request reads reachable from a `cacheFunction` callback in this module.
//!
//! Uses the existing token stream and declaration owners. Local helper calls
//! are followed with a visited set; imported helpers and dynamic property
//! access remain protected by the runtime scope. This is not a purity proof.

use compact_str::CompactString;
use uf_infra::LineIndex;

use super::owner::OwnerSpan;
use super::{ImportSpecifier, ImportedName, Token, TokenKind, matching_close};

/// A request read inside a named function cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedFunctionRead {
    /// Application-supplied cache identity.
    pub function: CompactString,
    /// The request API being called.
    pub api: CompactString,
    /// 1-based source line of the call.
    pub line: u32,
}

pub(crate) fn cached_function_reads(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
    imports: &[ImportSpecifier],
    owners: &[OwnerSpan],
) -> Vec<CachedFunctionRead> {
    let mut found = Vec::new();
    for at in 0..tokens.len() {
        if imported_call(source, tokens, at, imports, "@uniflowed/server/cache")
            != Some("cacheFunction")
        {
            continue;
        }
        let Some(open) = call_open(tokens, at) else {
            continue;
        };
        let Some(close) = matching_close(tokens, open, b'(', b')') else {
            continue;
        };
        let Some(name) = tokens.get(open + 1).filter(|t| t.kind == TokenKind::String) else {
            continue;
        };
        let mut work = vec![(open + 2, close)];
        let mut visited = vec![false; owners.len()];
        while let Some((start, end)) = work.pop() {
            for cursor in start..end {
                if let Some(api) =
                    imported_call(source, tokens, cursor, imports, "@uniflowed/server")
                    && matches!(
                        api,
                        "cookies" | "headers" | "draftMode" | "nonce" | "requestId"
                    )
                {
                    let read = CachedFunctionRead {
                        function: name.quoted_content(source).into(),
                        api: api.into(),
                        line: u32::try_from(index.line_col(tokens[cursor].start).line)
                            .unwrap_or(u32::MAX),
                    };
                    if !found.contains(&read) {
                        found.push(read);
                    }
                }
                if tokens[cursor].kind != TokenKind::Ident {
                    continue;
                }
                // A named callback or a helper referenced inside its body.
                for (position, owner) in owners.iter().enumerate() {
                    if !visited[position] && tokens[cursor].text(source) == owner.name {
                        visited[position] = true;
                        work.push((owner.open + 1, owner.close));
                    }
                }
            }
        }
    }
    found
}

fn call_open(tokens: &[Token], at: usize) -> Option<usize> {
    let next = at + 1;
    let open = if tokens.get(next)?.is_punct(b'<') {
        matching_close(tokens, next, b'<', b'>')? + 1
    } else {
        next
    };
    tokens.get(open)?.is_punct(b'(').then_some(open)
}

fn imported_call<'a>(
    source: &str,
    tokens: &[Token],
    at: usize,
    imports: &'a [ImportSpecifier],
    package: &str,
) -> Option<&'a str> {
    call_open(tokens, at)?;
    for import in imports.iter().filter(|import| import.specifier == package) {
        for binding in &import.bindings {
            match &binding.imported {
                ImportedName::Named(name)
                    if tokens[at].text(source) == binding.local
                        && (at == 0 || !tokens[at - 1].is_punct(b'.')) =>
                {
                    return Some(name);
                }
                ImportedName::Namespace
                    if at >= 2
                        && tokens[at - 1].is_punct(b'.')
                        && tokens[at - 2].text(source) == binding.local =>
                {
                    // Static names only; bracket/dynamic access is a runtime guard.
                    return [
                        "cacheFunction",
                        "cookies",
                        "headers",
                        "draftMode",
                        "nonce",
                        "requestId",
                    ]
                    .into_iter()
                    .find(|name| *name == tokens[at].text(source));
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::{EntryKind, RscGraphBuilder};

    fn errors(source: &str) -> Vec<String> {
        let mut graph = RscGraphBuilder::new();
        graph
            .add_source("app/$page.js", source)
            .add_entry("app/$page.js", EntryKind::Server);
        graph
            .build()
            .diagnostics()
            .iter()
            .filter(|error| error.rule() == "rsc/request-state-in-cached-function")
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn names_inline_callbacks_and_aliases() {
        let errors = errors(
            r#"
            import { cacheFunction as cached } from '@uniflowed/server/cache';
            import { cookies as jar } from '@uniflowed/server';
            export const account = cached('account', async () => jar().get('session'), {});
        "#,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("`account`"));
        assert!(errors[0].contains("`cookies()`"));
    }

    #[test]
    fn checks_explicit_type_arguments() {
        let errors = errors(
            r#"
            import { cacheFunction } from '@uniflowed/server/cache';
            import { cookies } from '@uniflowed/server';
            export const account = cacheFunction<[], string>('account', async () => cookies().get('session'), {});
        "#,
        );
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn follows_local_helpers_and_namespace_reads_without_looping() {
        let errors = errors(
            r#"
            import * as cache from '@uniflowed/server/cache';
            import * as server from '@uniflowed/server';
            async function load() { return helper(); }
            function helper() { if (false) load(); return server.headers(); }
            export const account = cache.cacheFunction('account', load, {});
        "#,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("`headers()`"));
    }

    #[test]
    fn ignores_reads_outside_the_cached_callback() {
        assert!(
            errors(
                r#"
            import { cacheFunction } from '@uniflowed/server/cache';
            import { cookies } from '@uniflowed/server';
            function session() { return cookies(); }
            export const publicData = cacheFunction('public', async id => id, {});
        "#
            )
            .is_empty()
        );
    }
}
