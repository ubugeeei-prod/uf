//! Which local names in a module mean StyleX.
//!
//! The extractor cannot look for the literal text `stylex.create` and stop
//! there: `import * as s from "@uniflowed/stylex"` is legal and so is
//! `import { tokens as t } from "./tokens.stylex.js"`. This module resolves
//! those names first, in one pass over the same token vector everything else
//! reads, so the rest of the crate works with resolved bindings rather than
//! with a guess about what the author typed.

use compact_str::CompactString;
use uf_rsc::{Token, TokenKind, starts_statement};

/// The package a StyleX binding has to come from.
pub const STYLEX_PACKAGE: &str = "@uniflowed/stylex";
/// The alternative specifier for the same module inside `@uniflowed/core`.
pub const STYLEX_CORE_PACKAGE: &str = "@uniflowed/core/stylex";
/// The suffix that marks a module as declaring StyleX variables.
pub const VARIABLES_SUFFIX: &str = ".stylex.js";

/// How many bindings of one kind a module may declare before uf stops tracking
/// them. A module with more than this is not a StyleX module.
const MAX_BINDINGS: usize = 64;

/// What one local name in the module refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// The `stylex` namespace object, whose `.create` and `.defineVars` matter.
    Namespace,
    /// The `create` function, imported by name.
    Create,
    /// The `defineVars` function, imported by name.
    DefineVars,
}

/// Every StyleX-relevant local name in one module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleBindings {
    names: Vec<(CompactString, BindingKind)>,
    /// Local name to the name the variables object is declared under.
    variables: Vec<(CompactString, CompactString)>,
}

impl ModuleBindings {
    /// What `name` refers to, if anything.
    pub fn kind_of(&self, name: &str) -> Option<BindingKind> {
        self.names
            .iter()
            .find(|(bound, _)| bound == name)
            .map(|(_, kind)| *kind)
    }

    /// The variables namespace `name` refers to, if any.
    pub fn variables_namespace(&self, name: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|(local, _)| local == name)
            .map(|(_, namespace)| namespace.as_str())
    }

    /// Whether the module mentions StyleX at all.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.variables.is_empty()
    }

    fn bind(&mut self, name: &str, kind: BindingKind) {
        if self.names.len() < MAX_BINDINGS && self.kind_of(name).is_none() {
            self.names.push((CompactString::new(name), kind));
        }
    }

    /// Record that `local` names the variables object declared as `namespace`.
    pub fn bind_variables(&mut self, local: &str, namespace: &str) {
        if self.variables.len() < MAX_BINDINGS && self.variables_namespace(local).is_none() {
            self.variables
                .push((CompactString::new(local), CompactString::new(namespace)));
        }
    }
}

/// Resolve every StyleX name a module's imports bring in.
pub fn from_imports(source: &str, tokens: &[Token]) -> ModuleBindings {
    let mut bindings = ModuleBindings::default();
    for (position, token) in tokens.iter().enumerate() {
        // A newline also starts a statement: an import list written without
        // semicolons leaves the previous statement's specifier as the previous
        // token, which `starts_statement` alone would not accept.
        if token.kind != TokenKind::Ident
            || token.text(source) != "import"
            || !(starts_statement(tokens, position) || token.newline_before)
        {
            continue;
        }
        let Some(statement) = import_statement(source, tokens, position) else {
            continue;
        };
        apply(&mut bindings, source, tokens, &statement);
    }
    bindings
}

/// The token span of one import statement, and its specifier.
struct ImportStatement {
    /// First token after `import`.
    clause_start: usize,
    /// Token index of the `from` keyword, when the statement has one.
    clause_end: usize,
    /// The specifier text, without quotes.
    specifier: CompactString,
}

/// Find the clause and specifier of the `import` starting at `position`.
///
/// Bounded by the next `import` and by a small token budget, so a source
/// without a specifier cannot walk the rest of the file.
fn import_statement(source: &str, tokens: &[Token], position: usize) -> Option<ImportStatement> {
    /// Longest import clause uf will read, in tokens. A clause is a list of
    /// names; anything longer is not one.
    const MAX_CLAUSE_TOKENS: usize = 512;

    let clause_start = position + 1;
    let mut at = clause_start;
    let limit = (clause_start + MAX_CLAUSE_TOKENS).min(tokens.len());
    while at < limit {
        let token = &tokens[at];
        if token.kind == TokenKind::String {
            return Some(ImportStatement {
                clause_start,
                clause_end: at,
                specifier: CompactString::new(token.quoted_content(source)),
            });
        }
        if token.is_punct(b';') {
            return None;
        }
        at += 1;
    }
    None
}

/// Bind the names one import statement introduces.
fn apply(
    bindings: &mut ModuleBindings,
    source: &str,
    tokens: &[Token],
    statement: &ImportStatement,
) {
    let specifier = statement.specifier.as_str();
    let is_stylex = specifier == STYLEX_PACKAGE || specifier == STYLEX_CORE_PACKAGE;
    let is_variables = specifier.ends_with(VARIABLES_SUFFIX);
    if !is_stylex && !is_variables {
        return;
    }

    let clause = &tokens[statement.clause_start..statement.clause_end];
    let mut at = 0usize;
    while at < clause.len() {
        let token = &clause[at];
        match token.kind {
            // `import * as name from ...`
            TokenKind::Punct(b'*') => {
                if let Some(name) = named_after(source, clause, at + 1, "as")
                    && is_stylex
                {
                    bindings.bind(name, BindingKind::Namespace);
                }
                at += 1;
            }
            TokenKind::Ident => {
                let name = token.text(source);
                if name == "type" || name == "typeof" || name == "from" || name == "as" {
                    at += 1;
                    continue;
                }
                // Inside `{ ... }` an entry is `exported` or `exported as local`.
                let local = named_after(source, clause, at + 1, "as").unwrap_or(name);
                if is_stylex {
                    match name {
                        "stylex" => bindings.bind(local, BindingKind::Namespace),
                        "create" => bindings.bind(local, BindingKind::Create),
                        "defineVars" => bindings.bind(local, BindingKind::DefineVars),
                        _ => {}
                    }
                } else {
                    bindings.bind_variables(local, name);
                }
                at += if local == name { 1 } else { 3 };
            }
            _ => at += 1,
        }
    }
}

/// The identifier after `keyword` at `at`, when both are there.
fn named_after<'a>(source: &'a str, clause: &[Token], at: usize, keyword: &str) -> Option<&'a str> {
    let marker = clause.get(at)?;
    if marker.kind != TokenKind::Ident || marker.text(source) != keyword {
        return None;
    }
    let name = clause.get(at + 1)?;
    (name.kind == TokenKind::Ident).then(|| name.text(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uf_rsc::tokenize;

    fn bindings(source: &str) -> ModuleBindings {
        let tokens = tokenize(source);
        from_imports(source, &tokens)
    }

    #[test]
    fn a_named_stylex_import_binds_the_namespace() {
        let found = bindings("import { stylex } from \"@uniflowed/stylex\";\n");
        assert_eq!(found.kind_of("stylex"), Some(BindingKind::Namespace));
    }

    #[test]
    fn a_renamed_stylex_import_binds_the_local_name() {
        let found = bindings("import { stylex as sx } from \"@uniflowed/stylex\";\n");
        assert_eq!(found.kind_of("sx"), Some(BindingKind::Namespace));
        assert_eq!(found.kind_of("stylex"), None);
    }

    #[test]
    fn a_star_import_binds_the_namespace() {
        let found = bindings("import * as sx from \"@uniflowed/stylex\";\n");
        assert_eq!(found.kind_of("sx"), Some(BindingKind::Namespace));
    }

    #[test]
    fn a_named_define_vars_import_binds_the_function() {
        let found = bindings("import { defineVars } from \"@uniflowed/stylex\";\n");
        assert_eq!(found.kind_of("defineVars"), Some(BindingKind::DefineVars));
    }

    #[test]
    fn a_variables_module_import_binds_the_namespace_it_was_exported_under() {
        let found = bindings("import { tokens } from \"./styles/tokens.stylex.js\";\n");
        assert_eq!(found.variables_namespace("tokens"), Some("tokens"));
    }

    #[test]
    fn a_renamed_variables_import_keeps_the_exported_name() {
        let found = bindings("import { tokens as t } from \"./tokens.stylex.js\";\n");
        assert_eq!(found.variables_namespace("t"), Some("tokens"));
    }

    #[test]
    fn an_unrelated_import_binds_nothing() {
        let found = bindings("import { useState } from \"@uniflowed/react\";\n");
        assert!(found.is_empty());
    }

    #[test]
    fn the_word_import_inside_a_string_is_not_a_statement() {
        let found = bindings("const text = \"import { stylex } from '@uniflowed/stylex'\";\n");
        assert!(found.is_empty());
    }

    #[test]
    fn a_type_only_import_still_binds_the_value_names_beside_it() {
        let found = bindings("import { type StyleX, stylex } from \"@uniflowed/stylex\";\n");
        assert_eq!(found.kind_of("stylex"), Some(BindingKind::Namespace));
    }
}
