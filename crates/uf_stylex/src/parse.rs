//! Finding the StyleX calls in a module, and what they turned out to declare.
//!
//! Everything here reads the token vector [`uf_rsc`] produces — uf's only lexer
//! for `.js` — so a `stylex.create` inside a string or a comment is never
//! mistaken for a call. This module owns finding the calls and resolving which
//! local names mean StyleX; [`extract`] owns reading one call's argument, and
//! is where the bound on nesting lives.

pub mod bindings;
pub mod extract;
pub mod object;

use compact_str::CompactString;
use uf_infra::LineIndex;
use uf_rsc::{TokenKind, matching_close, tokenize};

use crate::condition::StyleCondition;
use crate::error::{MAX_SOURCE_BYTES, SourcePosition, StyleXError};
use crate::value::StyleValue;

use bindings::{BindingKind, ModuleBindings};
use extract::{create_namespaces, variables};
use object::Cursor;

/// One resolved declaration inside one namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The key exactly as authored, which is the key of the compiled object.
    pub key: CompactString,
    /// The CSS property name the key denotes.
    pub property: CompactString,
    /// The state the declaration applies in.
    pub condition: StyleCondition,
    /// The resolved value.
    pub value: StyleValue,
    /// Where the declaration was written.
    pub at: SourcePosition,
}

/// One key of the object handed to `stylex.create`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    /// The namespace's name, as authored.
    pub name: CompactString,
    /// Its declarations, in source order.
    pub declarations: Vec<Declaration>,
}

/// One entry of the object handed to `stylex.defineVars`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// The key exactly as authored.
    pub key: CompactString,
    /// The generated custom-property name, dashes included.
    pub name: CompactString,
    /// The resolved value.
    pub value: StyleValue,
    /// Where the entry was written.
    pub at: SourcePosition,
}

/// A `stylex.create({...})` call and everything read out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCall {
    /// Byte offset of the first byte of the call expression.
    pub start: usize,
    /// Byte offset one past the closing parenthesis.
    pub end: usize,
    /// The namespaces the call declares, in source order.
    pub namespaces: Vec<Namespace>,
}

/// A `stylex.defineVars({...})` call and everything read out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineVarsCall {
    /// Byte offset of the first byte of the call expression.
    pub start: usize,
    /// Byte offset one past the closing parenthesis.
    pub end: usize,
    /// The binding the variables object is declared under.
    pub binding: CompactString,
    /// The variables the call declares, in source order.
    pub variables: Vec<Variable>,
}

/// Everything one module contributes to the stylesheet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedModule {
    /// `stylex.create` calls, in source order.
    pub creates: Vec<CreateCall>,
    /// `stylex.defineVars` calls, in source order.
    pub defines: Vec<DefineVarsCall>,
}

impl ParsedModule {
    /// Whether the module declares any styles at all.
    pub fn is_empty(&self) -> bool {
        self.creates.is_empty() && self.defines.is_empty()
    }
}

/// Which StyleX function one call site names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallKind {
    Create,
    DefineVars,
}

/// One call site, before its argument has been read.
#[derive(Debug, Clone)]
struct CallSite {
    kind: CallKind,
    /// Token index of the first token of the call expression.
    callee: usize,
    /// Token index of the `(`.
    open_paren: usize,
    /// The `const` binding the call result is assigned to, when there is one.
    binding: Option<CompactString>,
}

/// Read every StyleX declaration out of one module.
pub fn parse_module(source: &str) -> Result<ParsedModule, StyleXError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(StyleXError::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_SOURCE_BYTES,
        });
    }

    // Every specifier that can bind a StyleX name contains this substring —
    // `@uniflowed/stylex`, `@uniflowed/core/stylex`, and any `*.stylex.js` —
    // so a module without it cannot declare styles and is never tokenized.
    if !source.contains("stylex") {
        return Ok(ParsedModule::default());
    }

    let tokens = tokenize(source);
    let mut bindings = bindings::from_imports(source, &tokens);
    if bindings.is_empty() {
        return Ok(ParsedModule::default());
    }
    let lines = LineIndex::new(source);
    let cursor = Cursor {
        source,
        tokens: &tokens,
        lines: &lines,
    };

    let sites = call_sites(cursor, &bindings);
    // A `defineVars` result is a variables object for the rest of the module,
    // whichever order the declarations appear in.
    for site in &sites {
        if site.kind == CallKind::DefineVars
            && let Some(binding) = &site.binding
        {
            bindings.bind_variables(binding, binding);
        }
    }

    let mut parsed = ParsedModule::default();
    for site in &sites {
        let (open, close) = argument_object(cursor, site)?;
        let start = tokens[site.callee].start;
        let end = tokens[close].end;
        match site.kind {
            CallKind::Create => parsed.creates.push(CreateCall {
                start,
                end,
                namespaces: create_namespaces(cursor, &bindings, open)?,
            }),
            CallKind::DefineVars => {
                let Some(binding) = site.binding.clone() else {
                    return Err(StyleXError::MalformedEntry {
                        at: cursor.position(site.callee),
                    });
                };
                parsed.defines.push(DefineVarsCall {
                    start,
                    end,
                    variables: variables(cursor, &bindings, &binding, open)?,
                    binding,
                });
            }
        }
    }
    Ok(parsed)
}

/// Locate every `stylex.create` / `stylex.defineVars` call in the module.
fn call_sites(cursor: Cursor<'_>, bindings: &ModuleBindings) -> Vec<CallSite> {
    let tokens = cursor.tokens;
    let mut sites = Vec::new();
    for index in 0..tokens.len() {
        if tokens[index].kind != TokenKind::Ident {
            continue;
        }
        let name = cursor.text(index);
        let called = tokens
            .get(index + 1)
            .is_some_and(|next| next.is_punct(b'('));
        let site = match bindings.kind_of(name) {
            Some(BindingKind::Namespace) => member_call(cursor, index),
            Some(BindingKind::Create) if called => Some((CallKind::Create, index + 1)),
            Some(BindingKind::DefineVars) if called => Some((CallKind::DefineVars, index + 1)),
            _ => None,
        };
        if let Some((kind, open_paren)) = site {
            sites.push(CallSite {
                kind,
                callee: index,
                open_paren,
                binding: assigned_binding(cursor, index),
            });
        }
    }
    sites
}

/// Match `namespace . create (` and `namespace . defineVars (`.
fn member_call(cursor: Cursor<'_>, index: usize) -> Option<(CallKind, usize)> {
    let tokens = cursor.tokens;
    if !tokens.get(index + 1)?.is_punct(b'.') {
        return None;
    }
    let member = tokens.get(index + 2)?;
    if member.kind != TokenKind::Ident || !tokens.get(index + 3)?.is_punct(b'(') {
        return None;
    }
    let kind = match member.text(cursor.source) {
        "create" => CallKind::Create,
        "defineVars" => CallKind::DefineVars,
        _ => return None,
    };
    Some((kind, index + 3))
}

/// The `const NAME =` a call is assigned to, when it is assigned to one.
fn assigned_binding(cursor: Cursor<'_>, callee: usize) -> Option<CompactString> {
    let equals = cursor.tokens.get(callee.checked_sub(1)?)?;
    let name = cursor.tokens.get(callee.checked_sub(2)?)?;
    let keyword = cursor.tokens.get(callee.checked_sub(3)?)?;
    if !equals.is_punct(b'=') || name.kind != TokenKind::Ident || keyword.kind != TokenKind::Ident {
        return None;
    }
    matches!(keyword.text(cursor.source), "const" | "let" | "var")
        .then(|| CompactString::new(name.text(cursor.source)))
}

/// The object literal a call was handed, as `(open brace, close paren)`.
fn argument_object(cursor: Cursor<'_>, site: &CallSite) -> Result<(usize, usize), StyleXError> {
    let Some(close_paren) = matching_close(cursor.tokens, site.open_paren, b'(', b')') else {
        return Err(StyleXError::UnterminatedObject {
            at: cursor.position(site.open_paren),
        });
    };
    let open = site.open_paren + 1;
    if !cursor
        .tokens
        .get(open)
        .is_some_and(|token| token.is_punct(b'{'))
    {
        return Err(StyleXError::ExpectedObjectLiteral {
            at: cursor.position(open),
        });
    }
    Ok((open, close_paren))
}
