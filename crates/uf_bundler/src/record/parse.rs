//! Reading one import or export statement out of the token stream.
//!
//! Each reader is handed the index of the keyword that starts a statement and
//! returns where the scan should resume, or [`None`] when the tokens are not
//! the shape it recognizes — in which case the walk moves on by one token and
//! the statement is simply not recorded. Nothing here rewrites bytes; it only
//! records spans, and the emitter applies them.

use compact_str::CompactString;
use smallvec::SmallVec;
use uf_flow::scan::{Token, TokenKind, matching_close};

use super::{
    ExportRecord, ExportSource, ImportBinding, ImportForm, ImportRecord, ModuleRecord, Patch,
    PatchText,
};

/// The local name a default export is rewritten to.
pub const DEFAULT_LOCAL: &str = "__uf_default";

/// The text that replaces `export default` before an expression.
const DEFAULT_PATCH: &str = "const __uf_default =";

/// `import "…"`, `import d, { a as b } from "…"`, `import * as ns from "…"`.
pub(super) fn read_import(
    source: &str,
    tokens: &[Token],
    at: usize,
    record: &mut ModuleRecord,
) -> Option<usize> {
    let mut bindings: SmallVec<[ImportBinding; 4]> = SmallVec::new();
    let mut cursor = at + 1;

    if tokens.get(cursor)?.is_punct(b'(') {
        return read_dynamic(source, tokens, at, ImportForm::Dynamic, record);
    }
    if tokens.get(cursor)?.kind == TokenKind::String {
        return finish_import(
            source,
            tokens,
            at,
            cursor,
            bindings,
            ImportForm::Static,
            record,
        );
    }

    loop {
        let token = tokens.get(cursor)?;
        if token.is_punct(b'*') {
            let local = tokens.get(cursor + 2)?;
            if !tokens.get(cursor + 1)?.is_ident(source, "as") || local.kind != TokenKind::Ident {
                return None;
            }
            bindings.push(ImportBinding::Namespace {
                local: CompactString::new(local.text(source)),
            });
            cursor += 3;
        } else if token.is_punct(b'{') {
            let close = matching_close(tokens, cursor, b'{', b'}')?;
            read_named_specifiers(source, tokens, cursor, close, &mut bindings);
            cursor = close + 1;
        } else if token.kind == TokenKind::Ident && !token.is_ident(source, "from") {
            bindings.push(ImportBinding::Default {
                local: CompactString::new(token.text(source)),
            });
            cursor += 1;
        } else {
            break;
        }

        if tokens.get(cursor)?.is_punct(b',') {
            cursor += 1;
            continue;
        }
        break;
    }

    if !tokens.get(cursor)?.is_ident(source, "from") {
        return None;
    }
    let specifier = tokens.get(cursor + 1)?;
    if specifier.kind != TokenKind::String {
        return None;
    }
    finish_import(
        source,
        tokens,
        at,
        cursor + 1,
        bindings,
        ImportForm::Static,
        record,
    )
}

/// Record the import and blank the statement that declared it.
fn finish_import(
    source: &str,
    tokens: &[Token],
    at: usize,
    specifier_index: usize,
    bindings: SmallVec<[ImportBinding; 4]>,
    form: ImportForm,
    record: &mut ModuleRecord,
) -> Option<usize> {
    let last = with_semicolon(tokens, specifier_index);
    record.imports.push(ImportRecord {
        specifier: CompactString::new(quoted(source, &tokens[specifier_index])),
        form,
        bindings,
    });
    record.patches.push(Patch {
        start: tokens[at].start,
        end: tokens[last].end,
        text: PatchText::Blank,
    });
    Some(last + 1)
}

/// `import("…")` and `require("…")`: recorded as edges, never rewritten.
pub(super) fn read_dynamic(
    source: &str,
    tokens: &[Token],
    at: usize,
    form: ImportForm,
    record: &mut ModuleRecord,
) -> Option<usize> {
    if !tokens.get(at + 1)?.is_punct(b'(') {
        return None;
    }
    let literal = tokens.get(at + 2)?;
    if literal.kind != TokenKind::String {
        return None;
    }
    record.imports.push(ImportRecord {
        specifier: CompactString::new(quoted(source, literal)),
        form,
        bindings: SmallVec::new(),
    });
    Some(at + 3)
}

/// Every `export` form.
pub(super) fn read_export(
    source: &str,
    tokens: &[Token],
    at: usize,
    record: &mut ModuleRecord,
) -> Option<usize> {
    let next = tokens.get(at + 1)?;

    if next.is_punct(b'{') {
        return read_export_clause(source, tokens, at, record);
    }
    if next.is_punct(b'*') {
        return read_export_star(source, tokens, at, record);
    }
    if next.is_ident(source, "default") {
        return read_export_default(source, tokens, at, record);
    }
    read_export_declaration(source, tokens, at, record)
}

/// `export { a, b as c };` and `export { a } from "…";`.
fn read_export_clause(
    source: &str,
    tokens: &[Token],
    at: usize,
    record: &mut ModuleRecord,
) -> Option<usize> {
    let close = matching_close(tokens, at + 1, b'{', b'}')?;
    let mut bindings: SmallVec<[ImportBinding; 4]> = SmallVec::new();
    read_named_specifiers(source, tokens, at + 1, close, &mut bindings);

    let from = tokens
        .get(close + 1)
        .filter(|token| token.is_ident(source, "from"));
    let last = if from.is_some() {
        let specifier = tokens.get(close + 2)?;
        if specifier.kind != TokenKind::String {
            return None;
        }
        let import = record.imports.len();
        record.imports.push(ImportRecord {
            specifier: CompactString::new(quoted(source, specifier)),
            form: ImportForm::ReExport,
            bindings: SmallVec::new(),
        });
        for binding in &bindings {
            if let ImportBinding::Named { imported, local } = binding {
                record.exports.push(ExportRecord {
                    exported: local.clone(),
                    source: ExportSource::Reexport {
                        import,
                        imported: imported.clone(),
                    },
                });
            }
        }
        with_semicolon(tokens, close + 2)
    } else {
        for binding in &bindings {
            if let ImportBinding::Named { imported, local } = binding {
                record.exports.push(ExportRecord {
                    exported: local.clone(),
                    source: ExportSource::Local {
                        local: imported.clone(),
                    },
                });
            }
        }
        with_semicolon(tokens, close)
    };

    record.patches.push(Patch {
        start: tokens[at].start,
        end: tokens[last].end,
        text: PatchText::Blank,
    });
    Some(last + 1)
}

/// `export * from "…";` and `export * as ns from "…";`.
fn read_export_star(
    source: &str,
    tokens: &[Token],
    at: usize,
    record: &mut ModuleRecord,
) -> Option<usize> {
    let named = tokens.get(at + 2)?.is_ident(source, "as");
    let from_index = if named { at + 4 } else { at + 2 };
    if !tokens.get(from_index)?.is_ident(source, "from") {
        return None;
    }
    let specifier = tokens.get(from_index + 1)?;
    if specifier.kind != TokenKind::String {
        return None;
    }

    let import = record.imports.len();
    record.imports.push(ImportRecord {
        specifier: CompactString::new(quoted(source, specifier)),
        form: ImportForm::ReExport,
        bindings: SmallVec::new(),
    });
    if named {
        let alias = tokens.get(at + 3)?;
        record.exports.push(ExportRecord {
            exported: CompactString::new(alias.text(source)),
            source: ExportSource::Reexport {
                import,
                imported: CompactString::const_new("*"),
            },
        });
    } else {
        record.star_reexports.push(import);
    }

    let last = with_semicolon(tokens, from_index + 1);
    record.patches.push(Patch {
        start: tokens[at].start,
        end: tokens[last].end,
        text: PatchText::Blank,
    });
    Some(last + 1)
}

/// `export default …` in all four of its shapes.
fn read_export_default(
    source: &str,
    tokens: &[Token],
    at: usize,
    record: &mut ModuleRecord,
) -> Option<usize> {
    let named = named_declaration(source, tokens, at + 2);
    let local = match &named {
        // `export default function f() {}` keeps its declaration and its name,
        // so the binding stays hoisted exactly as the author wrote it.
        Some(name) => name.clone(),
        None => CompactString::const_new(DEFAULT_LOCAL),
    };
    record.exports.push(ExportRecord {
        exported: CompactString::const_new("default"),
        source: ExportSource::Local { local },
    });
    record.patches.push(Patch {
        start: tokens[at].start,
        end: tokens[at + 1].end,
        text: if named.is_some() {
            PatchText::Blank
        } else {
            PatchText::Text(DEFAULT_PATCH)
        },
    });
    Some(at + 2)
}

/// The name of `function f`, `async function f` or `class C` at `at`.
fn named_declaration(source: &str, tokens: &[Token], at: usize) -> Option<CompactString> {
    let keyword = tokens.get(at)?;
    if keyword.kind != TokenKind::Ident {
        return None;
    }
    let offset = match keyword.text(source) {
        "function" | "class" => 1,
        "async" if tokens.get(at + 1)?.is_ident(source, "function") => 2,
        _ => return None,
    };
    let name = tokens.get(at + offset)?;
    (name.kind == TokenKind::Ident).then(|| CompactString::new(name.text(source)))
}

/// `export const a = 1;`, `export function f() {}`, `export class C {}`.
fn read_export_declaration(
    source: &str,
    tokens: &[Token],
    at: usize,
    record: &mut ModuleRecord,
) -> Option<usize> {
    let keyword = tokens.get(at + 1)?;
    if keyword.kind != TokenKind::Ident {
        return None;
    }

    match keyword.text(source) {
        "function" | "class" | "async" => {
            let name = named_declaration(source, tokens, at + 1)?;
            record.exports.push(ExportRecord {
                exported: name.clone(),
                source: ExportSource::Local { local: name },
            });
        }
        "const" | "let" | "var" => {
            for name in declarator_names(source, tokens, at + 2) {
                record.exports.push(ExportRecord {
                    exported: name.clone(),
                    source: ExportSource::Local { local: name },
                });
            }
        }
        _ => return None,
    }

    record.patches.push(Patch {
        start: tokens[at].start,
        end: tokens[at].end,
        text: PatchText::Blank,
    });
    Some(at + 1)
}

/// Names bound by the declarator list starting at `at`.
///
/// Handles `a`, `a = 1`, `a, b`, and the identifiers of an object or array
/// pattern. A pattern's property *keys* are skipped, so `{ a: b }` binds `b`.
fn declarator_names(source: &str, tokens: &[Token], at: usize) -> SmallVec<[CompactString; 4]> {
    let mut names: SmallVec<[CompactString; 4]> = SmallVec::new();
    let mut depth: usize = 0;
    let mut expect_name = true;
    let mut initializer = false;
    let limit = (at + MAX_DECLARATOR_TOKENS).min(tokens.len());

    for index in at..limit {
        let token = tokens[index];
        match token.kind {
            TokenKind::Punct(b'{' | b'[' | b'(') => {
                depth += 1;
                expect_name = !initializer;
            }
            TokenKind::Punct(b'}' | b']' | b')') => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                expect_name = false;
            }
            TokenKind::Punct(b',') => {
                if depth == 0 {
                    initializer = false;
                }
                expect_name = !initializer;
            }
            TokenKind::Punct(b';') if depth == 0 => break,
            TokenKind::Punct(b'=') if depth == 0 => initializer = true,
            TokenKind::Ident if expect_name && !initializer => {
                // In `{ key: local }` the key is followed by `:`; the binding is
                // whatever comes after it.
                if tokens
                    .get(index + 1)
                    .is_some_and(|next| next.is_punct(b':'))
                {
                    continue;
                }
                names.push(CompactString::new(token.text(source)));
                expect_name = false;
            }
            _ => {}
        }
    }

    names
}

/// Longest declarator list the record will read.
const MAX_DECLARATOR_TOKENS: usize = 4096;

/// Parse `{ a, b as c }` between `open` and `close` into named bindings.
fn read_named_specifiers(
    source: &str,
    tokens: &[Token],
    open: usize,
    close: usize,
    bindings: &mut SmallVec<[ImportBinding; 4]>,
) {
    let mut at = open + 1;
    while at < close {
        let name = tokens[at];
        if name.kind != TokenKind::Ident && name.kind != TokenKind::String {
            at += 1;
            continue;
        }
        let imported = CompactString::new(text_of(source, &name));
        let (local, next) = match tokens.get(at + 1) {
            Some(token) if token.is_ident(source, "as") => match tokens.get(at + 2) {
                Some(alias) => (CompactString::new(text_of(source, alias)), at + 3),
                None => break,
            },
            _ => (imported.clone(), at + 1),
        };
        bindings.push(ImportBinding::Named { imported, local });
        at = next;
    }
}

/// The token's text, unquoted when it is a string literal.
fn text_of<'a>(source: &'a str, token: &Token) -> &'a str {
    if token.kind == TokenKind::String {
        quoted(source, token)
    } else {
        token.text(source)
    }
}

/// A string literal's content, without its quotes.
pub(super) fn quoted<'a>(source: &'a str, token: &Token) -> &'a str {
    if token.end < token.start + 2 {
        return "";
    }
    source
        .get(token.start + 1..token.end - 1)
        .unwrap_or_default()
}

/// Extend a span over a trailing `;`.
fn with_semicolon(tokens: &[Token], at: usize) -> usize {
    match tokens.get(at + 1) {
        Some(token) if token.is_punct(b';') => at + 1,
        _ => at,
    }
}
