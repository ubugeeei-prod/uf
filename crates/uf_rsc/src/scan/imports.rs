//! Import-specifier collection over an already-lexed module.
//!
//! Recognizes the four ways a module names another one — static `import`,
//! `export ... from`, dynamic `import()` and CommonJS `require()` — and
//! records each specifier exactly as written, so the resolver rather than the
//! scanner decides what a specifier points at.
//!
//! # Type-only imports are not edges
//!
//! `import type { T } from "./m.js"` and `import typeof * as M from "./m.js"`
//! are erased before anything runs, so they are not recorded. That is not a
//! tidiness question: the graph decides which server actions are callable
//! endpoints from who reaches whom, and a Client Component that imports a
//! *type* from a server module used to make every action that module reaches
//! dialable from the browser. A type annotation must not open an endpoint.
//!
//! The same rule one level down: `import { type T, useRoute } from "./m.js"` is
//! an edge, and only `useRoute` is a binding of it.
//!
//! # The clause, and not only the specifier
//!
//! Each specifier carries the `{ imported, local }` pairs its clause
//! introduced. A specifier alone says this module depends on `"react"`; the
//! pairs say `useS` here is `useState` there, which is what connects a call
//! site to a name a package's table can be looked up by. Keeping only one half
//! loses the aliased import in one direction or the other — see
//! [`ImportBinding`](super::ImportBinding).

use compact_str::CompactString;
use uf_infra::LineIndex;

use super::lexer::{Token, TokenKind, matching_close, starts_statement};
use super::{
    ImportBinding, ImportBindingList, ImportKind, ImportList, ImportSpecifier, ImportedName,
    clamp_u32,
};

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
                if is_type_only_import(source, tokens, position) {
                    continue;
                }
                if let Some(next) = tokens.get(position + 1) {
                    if next.kind == TokenKind::String {
                        // `import "m"`: a side effect and no bindings.
                        push_specifier(
                            &mut imports,
                            source,
                            next,
                            ImportKind::Static,
                            index,
                            ImportBindingList::new(),
                        );
                        continue;
                    }
                    if next.is_punct(b'(') {
                        if let Some(literal) = tokens.get(position + 2)
                            && literal.kind == TokenKind::String
                        {
                            // What `import("m")` binds is decided by the
                            // expression around it, not by a clause.
                            push_specifier(
                                &mut imports,
                                source,
                                literal,
                                ImportKind::Dynamic,
                                index,
                                ImportBindingList::new(),
                            );
                        }
                        continue;
                    }
                }
                if let Some(from) = find_from_clause(source, tokens, position) {
                    let bindings = clause_bindings(source, tokens, position + 1, from);
                    push_specifier(
                        &mut imports,
                        source,
                        &tokens[from + 1],
                        ImportKind::Static,
                        index,
                        bindings,
                    );
                }
            }
            "export" => {
                if !starts_statement(tokens, position) {
                    continue;
                }
                // `export type { T } from "./m.js"` is erased with the rest of
                // the types; only `export { value } from` is an edge.
                if tokens.get(position + 1).is_some_and(|next| {
                    next.kind == TokenKind::Ident && next.text(source) == "type"
                }) {
                    continue;
                }
                if let Some(from) = find_from_clause(source, tokens, position) {
                    let bindings = clause_bindings(source, tokens, position + 1, from);
                    push_specifier(
                        &mut imports,
                        source,
                        &tokens[from + 1],
                        ImportKind::ReExport,
                        index,
                        bindings,
                    );
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
                    push_specifier(
                        &mut imports,
                        source,
                        literal,
                        ImportKind::Require,
                        index,
                        ImportBindingList::new(),
                    );
                }
            }
            _ => {}
        }
    }

    imports
}

/// Whether the `import` at `position` imports types rather than values.
///
/// The keyword alone does not settle it. `import type from "./m.js"` is a
/// *value* import of the default export into a binding called `type`, which is
/// legal and rare and would be silently dropped by a rule that stopped at the
/// keyword. What distinguishes the two is the token after it: a type import is
/// followed by the thing being imported — `{`, `*`, or the name of a default
/// binding — and the value import is followed by `from`.
fn is_type_only_import(source: &str, tokens: &[Token], position: usize) -> bool {
    let Some(keyword) = tokens.get(position + 1) else {
        return false;
    };
    if keyword.kind != TokenKind::Ident || !matches!(keyword.text(source), "type" | "typeof") {
        return false;
    }
    let Some(after) = tokens.get(position + 2) else {
        return false;
    };
    after.is_punct(b'{')
        || after.is_punct(b'*')
        || (after.kind == TokenKind::Ident && after.text(source) != "from")
}

fn push_specifier(
    imports: &mut ImportList,
    source: &str,
    literal: &Token,
    kind: ImportKind,
    index: &LineIndex,
    bindings: ImportBindingList,
) {
    let position = index.line_col(literal.start);
    imports.push(ImportSpecifier {
        specifier: CompactString::from(literal.quoted_content(source)),
        kind,
        line: clamp_u32(position.line),
        bindings,
    });
}

/// The `{ imported, local }` pairs of the clause in `start..end`.
///
/// `end` is the index of the `from` keyword, so the range is exactly what was
/// written between the `import` (or `export`) and the specifier: a default
/// binding, a namespace binding, a braced list, or any comma-separated
/// combination of them.
fn clause_bindings(source: &str, tokens: &[Token], start: usize, end: usize) -> ImportBindingList {
    let mut bindings = ImportBindingList::new();
    let mut at = start;

    while at < end {
        let token = &tokens[at];

        if token.is_punct(b'*') {
            // `* as ns`, the one form whose local name binds no single export.
            if let Some(local) = aliased_name(source, tokens, at, end) {
                bindings.push(ImportBinding {
                    imported: ImportedName::Namespace,
                    local,
                });
                at += 3;
                continue;
            }
            at += 1;
            continue;
        }

        if token.is_punct(b'{') {
            let close = matching_close(tokens, at, b'{', b'}')
                .unwrap_or(end)
                .min(end);
            named_bindings(source, tokens, at + 1, close, &mut bindings);
            at = close + 1;
            continue;
        }

        if token.kind == TokenKind::Ident {
            // A bare name in a clause is the default binding: `import d from`.
            bindings.push(ImportBinding {
                imported: ImportedName::Default,
                local: CompactString::from(token.text(source)),
            });
            at += 1;
            continue;
        }

        at += 1;
    }

    bindings
}

/// The `local` of an `as local` following the token at `at`, if there is one.
fn aliased_name(source: &str, tokens: &[Token], at: usize, end: usize) -> Option<CompactString> {
    if at + 2 >= end {
        return None;
    }
    let keyword = tokens.get(at + 1)?;
    if keyword.kind != TokenKind::Ident || keyword.text(source) != "as" {
        return None;
    }
    let local = tokens.get(at + 2)?;
    (local.kind == TokenKind::Ident).then(|| CompactString::from(local.text(source)))
}

/// Split a braced specifier list into its pairs.
///
/// Each comma-separated entry is one of `a`, `a as b`, `type T`, `type T as U`
/// or `"a-b" as c` — the last being ES2022's arbitrary module namespace names,
/// which are legal export names a lexer would otherwise drop.
fn named_bindings(
    source: &str,
    tokens: &[Token],
    start: usize,
    end: usize,
    bindings: &mut ImportBindingList,
) {
    let mut at = start;
    while at < end {
        let mut next = at;
        while next < end && !tokens[next].is_punct(b',') {
            next += 1;
        }
        push_named_binding(source, &tokens[at..next], bindings);
        at = next + 1;
    }
}

/// Read one entry of a braced specifier list.
fn push_named_binding(source: &str, entry: &[Token], bindings: &mut ImportBindingList) {
    let Some(first) = entry.first() else {
        return;
    };

    // `{ type T }` and `{ typeof x }` are erased, and `{ type }` and
    // `{ type as t }` are not: what separates them is whether a *name* follows
    // the keyword, which is exactly the test the statement-level
    // `is_type_only_import` makes one level up.
    if first.kind == TokenKind::Ident
        && matches!(first.text(source), "type" | "typeof")
        && entry
            .get(1)
            .is_some_and(|next| next.kind == TokenKind::Ident && next.text(source) != "as")
    {
        return;
    }

    let Some(imported) = specifier_name(source, first) else {
        return;
    };
    let local = match (entry.get(1), entry.get(2)) {
        (Some(keyword), Some(local))
            if keyword.kind == TokenKind::Ident
                && keyword.text(source) == "as"
                && local.kind == TokenKind::Ident =>
        {
            CompactString::from(local.text(source))
        }
        // Unaliased, so the two names are the same one. A string entry with no
        // alias is not legal — `import { "a-b" } from "m"` binds nothing
        // nameable — and falls out here as the imported name repeated, which
        // no lookup can be wrong about.
        _ => imported.clone(),
    };

    bindings.push(ImportBinding {
        imported: if imported == "default" {
            // `import { default as x }` and `import x` name the same export.
            ImportedName::Default
        } else {
            ImportedName::Named(imported)
        },
        local,
    });
}

/// The exported name an entry starts with, identifier or string literal.
fn specifier_name(source: &str, token: &Token) -> Option<CompactString> {
    match token.kind {
        TokenKind::Ident => Some(CompactString::from(token.text(source))),
        TokenKind::String => Some(CompactString::from(token.quoted_content(source))),
        _ => None,
    }
}

/// Find the `from` keyword of a `from "..."` clause started at `position`.
///
/// Returns the index of the keyword, so a caller that wants the specifier
/// reads `tokens[index + 1]` — checked to be a string literal before this
/// returns — and a caller that wants the clause has its end.
pub(crate) fn find_from_clause(source: &str, tokens: &[Token], position: usize) -> Option<usize> {
    let mut at = position + 1;
    let limit = (position + 512).min(tokens.len());
    while at < limit {
        let token = &tokens[at];
        if token.is_punct(b';') {
            return None;
        }
        if token.kind == TokenKind::Ident && token.text(source) == "from" {
            let literal = tokens.get(at + 1)?;
            return (literal.kind == TokenKind::String).then_some(at);
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
