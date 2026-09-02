//! Import-specifier collection over an already-lexed module.
//!
//! Recognizes the four ways a module names another one — static `import`,
//! `export ... from`, dynamic `import()` and CommonJS `require()` — and
//! records each specifier exactly as written, so the resolver rather than the
//! scanner decides what a specifier points at.

use compact_str::CompactString;
use uf_infra::LineIndex;

use super::lexer::{Token, TokenKind, starts_statement};
use super::{ImportKind, ImportList, ImportSpecifier, clamp_u32};

pub(crate) fn imports_from_tokens(source: &str, tokens: &[Token], index: &LineIndex) -> ImportList {
    let mut imports = ImportList::new();

    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident {
            continue;
        }
        match token.text(source) {
            "import" => {
                if !starts_statement(tokens, position) && !is_dynamic_import(tokens, position) {
                    continue;
                }
                if let Some(next) = tokens.get(position + 1) {
                    if next.kind == TokenKind::String {
                        push_specifier(&mut imports, source, next, ImportKind::Static, index);
                        continue;
                    }
                    if next.is_punct(b'(') {
                        if let Some(literal) = tokens.get(position + 2)
                            && literal.kind == TokenKind::String
                        {
                            push_specifier(
                                &mut imports,
                                source,
                                literal,
                                ImportKind::Dynamic,
                                index,
                            );
                        }
                        continue;
                    }
                }
                if let Some(literal) = find_from_clause(source, tokens, position) {
                    push_specifier(&mut imports, source, literal, ImportKind::Static, index);
                }
            }
            "export" => {
                if !starts_statement(tokens, position) {
                    continue;
                }
                if let Some(literal) = find_from_clause(source, tokens, position) {
                    push_specifier(&mut imports, source, literal, ImportKind::ReExport, index);
                }
            }
            "require" => {
                let Some(open) = tokens.get(position + 1) else {
                    continue;
                };
                if !open.is_punct(b'(') {
                    continue;
                }
                if let Some(literal) = tokens.get(position + 2)
                    && literal.kind == TokenKind::String
                {
                    push_specifier(&mut imports, source, literal, ImportKind::Require, index);
                }
            }
            _ => {}
        }
    }

    imports
}

fn push_specifier(
    imports: &mut ImportList,
    source: &str,
    literal: &Token,
    kind: ImportKind,
    index: &LineIndex,
) {
    let position = index.line_col(literal.start);
    imports.push(ImportSpecifier {
        specifier: CompactString::from(literal.quoted_content(source)),
        kind,
        line: clamp_u32(position.line),
    });
}

/// Find the string literal of a `from "..."` clause started at `position`.
pub(crate) fn find_from_clause<'a>(
    source: &str,
    tokens: &'a [Token],
    position: usize,
) -> Option<&'a Token> {
    let mut at = position + 1;
    let limit = (position + 512).min(tokens.len());
    while at < limit {
        let token = &tokens[at];
        if token.is_punct(b';') {
            return None;
        }
        if token.kind == TokenKind::Ident && token.text(source) == "from" {
            let literal = tokens.get(at + 1)?;
            return (literal.kind == TokenKind::String).then_some(literal);
        }
        at += 1;
    }
    None
}

fn is_dynamic_import(tokens: &[Token], position: usize) -> bool {
    tokens
        .get(position + 1)
        .is_some_and(|token| token.is_punct(b'('))
}
