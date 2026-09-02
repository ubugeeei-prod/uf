//! Exported-binding collection over an already-lexed module.
//!
//! Answers what a module exports and, as far as a lexer can tell, the shape of
//! each binding — the async-vs-sync distinction the server-action contract
//! turns on. Local `function`/`class`/`const` declarations are indexed first so
//! that a bare `export { foo }` can be classified without a second pass.

use compact_str::CompactString;
use uf_infra::LineIndex;

use super::imports::find_from_clause;
use super::lexer::{Token, TokenKind, matching_close, starts_statement};
use super::{ExportKind, ExportList, ModuleExport, clamp_u32};

pub(crate) fn exports_from_tokens(source: &str, tokens: &[Token], index: &LineIndex) -> ExportList {
    let mut exports = ExportList::new();
    let locals = local_bindings(source, tokens);

    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident || token.text(source) != "export" {
            continue;
        }
        if !starts_statement(tokens, position) {
            continue;
        }
        let line = clamp_u32(index.line_col(token.start).line);
        collect_export(source, tokens, position, line, &locals, &mut exports);
    }

    exports
}

fn collect_export(
    source: &str,
    tokens: &[Token],
    position: usize,
    line: u32,
    locals: &[(CompactString, ExportKind)],
    exports: &mut ExportList,
) {
    let Some(next) = tokens.get(position + 1) else {
        return;
    };

    if next.is_punct(b'*') {
        return;
    }

    if next.is_punct(b'{') {
        let re_export = find_from_clause(source, tokens, position).is_some();
        let mut at = position + 2;
        let mut pending: Option<&Token> = None;
        while at < tokens.len() && !tokens[at].is_punct(b'}') {
            let token = &tokens[at];
            if token.kind == TokenKind::Ident {
                if token.text(source) == "as" {
                    pending = None;
                    at += 1;
                    continue;
                }
                pending = Some(token);
            }
            if token.is_punct(b',')
                && let Some(name) = pending.take()
            {
                push_named_export(source, name, line, re_export, locals, exports);
            }
            at += 1;
        }
        if let Some(name) = pending.take() {
            push_named_export(source, name, line, re_export, locals, exports);
        }
        return;
    }

    if next.kind != TokenKind::Ident {
        return;
    }

    match next.text(source) {
        "default" => {
            let kind = initializer_kind(source, tokens, position + 2);
            exports.push(ModuleExport {
                name: CompactString::const_new("default"),
                kind,
                line,
            });
        }
        "type" | "interface" | "opaque" | "declare" | "enum" => {}
        "async" => {
            if let Some(name) = declaration_name(source, tokens, position + 3) {
                exports.push(ModuleExport {
                    name,
                    kind: ExportKind::AsyncFunction,
                    line,
                });
            }
        }
        "function" | "hook" | "component" => {
            if let Some(name) = declaration_name(source, tokens, position + 2) {
                exports.push(ModuleExport {
                    name,
                    kind: ExportKind::SyncFunction,
                    line,
                });
            }
        }
        "class" => {
            if let Some(name) = declaration_name(source, tokens, position + 2) {
                exports.push(ModuleExport {
                    name,
                    kind: ExportKind::Class,
                    line,
                });
            }
        }
        "const" | "let" | "var" => {
            collect_declarators(source, tokens, position + 2, line, exports);
        }
        _ => {}
    }
}

fn push_named_export(
    source: &str,
    name: &Token,
    line: u32,
    re_export: bool,
    locals: &[(CompactString, ExportKind)],
    exports: &mut ExportList,
) {
    let text = name.text(source);
    let kind = if re_export {
        ExportKind::ReExport
    } else {
        locals
            .iter()
            .find(|(local, _)| local == text)
            .map_or(ExportKind::Value, |(_, kind)| *kind)
    };
    exports.push(ModuleExport {
        name: CompactString::from(text),
        kind,
        line,
    });
}

fn declaration_name(source: &str, tokens: &[Token], position: usize) -> Option<CompactString> {
    let token = tokens.get(position)?;
    (token.kind == TokenKind::Ident).then(|| CompactString::from(token.text(source)))
}

/// Walk the declarators of a `const a = 1, b = 2;` statement.
///
/// Destructuring patterns bind names the lexer cannot attribute to an
/// initializer, so they are skipped rather than guessed at.
fn collect_declarators(
    source: &str,
    tokens: &[Token],
    position: usize,
    line: u32,
    exports: &mut ExportList,
) {
    let mut at = position;
    loop {
        let Some(name_token) = tokens.get(at) else {
            return;
        };
        if name_token.kind != TokenKind::Ident {
            return;
        }
        let name = CompactString::from(name_token.text(source));

        let mut cursor = at + 1;
        let mut depth = 0usize;
        let mut kind = ExportKind::Value;
        let mut assigned = false;
        while cursor < tokens.len() {
            let token = &tokens[cursor];
            match token.kind {
                TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
                TokenKind::Punct(b')' | b']' | b'}') => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::Punct(b'=') if depth == 0 && !assigned => {
                    assigned = true;
                    kind = initializer_kind(source, tokens, cursor + 1);
                }
                TokenKind::Punct(b',' | b';') if depth == 0 => break,
                _ => {}
            }
            cursor += 1;
        }

        exports.push(ModuleExport { name, kind, line });

        match tokens.get(cursor) {
            Some(token) if token.is_punct(b',') => at = cursor + 1,
            _ => return,
        }
    }
}

/// Classify the initializer expression starting at `position`.
///
/// Server-action factories are transparent: `serverAction(async () => {})` is an
/// async function for the purposes of the React server-action contract, so a
/// call expression is classified by its first argument.
fn initializer_kind(source: &str, tokens: &[Token], position: usize) -> ExportKind {
    initializer_kind_inner(source, tokens, position, 0)
}

fn initializer_kind_inner(
    source: &str,
    tokens: &[Token],
    position: usize,
    depth: u32,
) -> ExportKind {
    if depth > 4 {
        return ExportKind::Value;
    }
    let Some(token) = tokens.get(position) else {
        return ExportKind::Value;
    };

    if token.kind == TokenKind::Ident {
        match token.text(source) {
            "async" => {
                return ExportKind::AsyncFunction;
            }
            "function" | "hook" | "component" => return ExportKind::SyncFunction,
            "class" => return ExportKind::Class,
            _ => {}
        }
        if tokens
            .get(position + 1)
            .is_some_and(|next| next.is_punct(b'('))
        {
            return initializer_kind_inner(source, tokens, position + 2, depth + 1);
        }
        if tokens
            .get(position + 1)
            .is_some_and(|next| next.kind == TokenKind::Arrow)
        {
            return ExportKind::SyncFunction;
        }
        return ExportKind::Value;
    }

    if token.is_punct(b'(')
        && let Some(close) = matching_close(tokens, position, b'(', b')')
    {
        // A Flow return type may sit between `)` and `=>`.
        let mut at = close + 1;
        let limit = (close + 64).min(tokens.len());
        while at < limit {
            if tokens[at].kind == TokenKind::Arrow {
                return ExportKind::SyncFunction;
            }
            if tokens[at].is_punct(b';') || tokens[at].is_punct(b',') {
                break;
            }
            at += 1;
        }
    }

    ExportKind::Value
}

/// Collect top-level `function`/`class`/`const` bindings so `export { foo }` can
/// be classified without a second pass over the source.
fn local_bindings(source: &str, tokens: &[Token]) -> Vec<(CompactString, ExportKind)> {
    let mut locals = Vec::new();
    for (position, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Ident || !starts_statement(tokens, position) {
            continue;
        }
        match token.text(source) {
            "async" => {
                if tokens
                    .get(position + 1)
                    .is_some_and(|next| next.kind == TokenKind::Ident)
                    && let Some(name) = declaration_name(source, tokens, position + 2)
                {
                    locals.push((name, ExportKind::AsyncFunction));
                }
            }
            "function" | "hook" | "component" => {
                if let Some(name) = declaration_name(source, tokens, position + 1) {
                    locals.push((name, ExportKind::SyncFunction));
                }
            }
            "class" => {
                if let Some(name) = declaration_name(source, tokens, position + 1) {
                    locals.push((name, ExportKind::Class));
                }
            }
            "const" | "let" | "var" => {
                let mut declarators = ExportList::new();
                collect_declarators(source, tokens, position + 1, 0, &mut declarators);
                locals.extend(
                    declarators
                        .into_iter()
                        .map(|export| (export.name, export.kind)),
                );
            }
            _ => {}
        }
    }
    locals
}
