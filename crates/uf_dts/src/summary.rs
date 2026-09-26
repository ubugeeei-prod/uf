//! What a declaration file binds, imports and exports — without its types.
//!
//! Translating one file needs facts about others, and they are all the same
//! fact: *is this name a type, a value, or both?* TypeScript's
//! `import { ZodType } from "./schemas.cjs"` imports whatever `ZodType` is;
//! Flow has to be told, because `import { I }` of an interface is an error and
//! `import type { C }` of a class cannot be extended. query-core's index
//! re-exports a hundred names from a bundler chunk with no `type` modifier on
//! any of them, and which of those are interfaces is written only in the
//! chunk. So every file the translation reaches is summarized first — a
//! lifetime-free record of its bindings — and the emitter asks the summaries.

use std::borrow::Cow;

use compact_str::{CompactString, ToCompactString};
use oxc_ast::ast::{
    Declaration, ExportDefaultDeclarationKind, Expression, ImportDeclarationSpecifier,
    ModuleExportName, Program, Statement, TSModuleReference, TSNamespaceDeclaration,
    TSNamespaceDeclarationBody, TSTypeName, TSTypeQuery, TSTypeQueryExprName,
};
use oxc_ast_visit::{Visit, walk};
use uf_infra::{FxHashMap, FxHashSet};

/// Which halves of a binding a name has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Kinds {
    /// Usable as a type.
    pub(crate) type_: bool,
    /// Usable as a value.
    pub(crate) value: bool,
    /// The type and the value come from different declarations —
    /// `interface ZodType` beside `const ZodType` — so Flow, which binds a
    /// name once, needs two names for them.
    pub(crate) split: bool,
    /// An interface merged into a namespace of the same name, which the
    /// translation declares as the namespace's `$Self`.
    pub(crate) self_interface: bool,
}

impl Kinds {
    pub(crate) const TYPE: Self = Self {
        type_: true,
        value: false,
        split: false,
        self_interface: false,
    };
    pub(crate) const VALUE: Self = Self {
        type_: false,
        value: true,
        split: false,
        self_interface: false,
    };
    pub(crate) const BOTH: Self = Self {
        type_: true,
        value: true,
        split: false,
        self_interface: false,
    };
}

/// Every declaration of one name in a file's top-level scope, counted by kind.
///
/// Counted rather than flagged because TypeScript merges declarations and
/// Flow does not, and what a merge becomes depends on which kinds met.
#[derive(Debug, Clone, Default)]
pub(crate) struct Local {
    pub(crate) interfaces: u32,
    pub(crate) aliases: u32,
    pub(crate) classes: u32,
    pub(crate) enums: u32,
    pub(crate) functions: u32,
    pub(crate) variables: u32,
    pub(crate) namespaces: u32,
    /// Whether any namespace of this name declares a value.
    pub(crate) instantiated: bool,
}

impl Local {
    pub(crate) fn kinds(&self) -> Kinds {
        // A class or an enum is one declaration that is both.
        let carrier = self.classes > 0 || self.enums > 0;
        // A namespace with no values in it is only something to qualify a
        // type name with.
        let type_declaration =
            self.interfaces > 0 || self.aliases > 0 || (self.namespaces > 0 && !self.instantiated);
        let value_declaration = self.functions > 0 || self.variables > 0;
        Kinds {
            type_: carrier || type_declaration || self.namespaces > 0,
            value: carrier || value_declaration || self.instantiated,
            split: type_declaration && value_declaration && !carrier,
            self_interface: self.interfaces > 0 && self.namespaces > 0 && self.classes == 0,
        }
    }
}

/// What a module specifier resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// A declaration file in this package, by its package path.
    Module(CompactString),
    /// Another package, by the specifier as written.
    Package(CompactString),
    /// A relative specifier naming no declaration file the package ships.
    Missing(CompactString),
}

/// Which export of the target an import or re-export names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Imported {
    Named(CompactString),
    Namespace,
}

/// One imported binding.
#[derive(Debug, Clone)]
pub(crate) struct Import {
    pub(crate) target: Target,
    pub(crate) imported: Imported,
    /// Written `import type`, or with a `type` modifier on the specifier.
    pub(crate) type_only: bool,
}

/// What one export name is bound to.
#[derive(Debug, Clone)]
pub(crate) enum Export {
    /// A name in this file's top-level scope — declared or imported.
    Local(CompactString),
    /// A name re-exported from another module.
    From {
        target: Target,
        imported: Imported,
        type_only: bool,
    },
    /// A default export with no name: `export default function (): void`.
    Anonymous(Kinds),
}

/// One declaration file, as other files need to see it.
#[derive(Debug, Default)]
pub(crate) struct Summary {
    /// Every module specifier the file writes, by how it resolved.
    pub(crate) targets: FxHashMap<CompactString, Target>,
    pub(crate) locals: FxHashMap<CompactString, Local>,
    pub(crate) imports: FxHashMap<CompactString, Import>,
    pub(crate) exports: FxHashMap<CompactString, Export>,
    /// `export * from` targets, in source order.
    pub(crate) stars: Vec<Target>,
    /// The name `export =` assigns, when there is one.
    pub(crate) equals: Option<CompactString>,
    /// Declarations exported under their own name — with an `export` modifier,
    /// or implicitly.
    pub(crate) self_exported: FxHashSet<CompactString>,
    /// Names used where a value is needed: `typeof x`, `class extends x`,
    /// `export default x`, `export = x`, `export { x }`.
    pub(crate) value_uses: FxHashSet<CompactString>,
    /// `/// <reference path>` directives, as the specifier they resolve by and
    /// the offset of the comment.
    pub(crate) references: Vec<(CompactString, u32)>,
}

/// What a first pass over a file finds before anything can be resolved.
pub(crate) struct Scan<'a> {
    /// Every module specifier the file writes, including `import("x")` types
    /// and `/// <reference path>` directives.
    ///
    /// As written, except a reference path, which is made relative: see
    /// [`reference_specifier`].
    pub(crate) specifiers: Vec<Cow<'a, str>>,
    value_uses: FxHashSet<CompactString>,
    references: Vec<(CompactString, u32)>,
}

pub(crate) fn scan<'a>(program: &Program<'a>) -> Scan<'a> {
    let mut found = Found::default();
    found.visit_program(program);
    let mut specifiers: Vec<Cow<'a, str>> =
        found.import_types.into_iter().map(Cow::Borrowed).collect();
    for statement in &program.body {
        let written = match statement {
            Statement::ImportDeclaration(import) => import.source.value.as_str(),
            Statement::ExportFromDeclaration(from) => from.source.value.as_str(),
            Statement::ExportAllDeclaration(all) => all.source.value.as_str(),
            Statement::TSImportEqualsDeclaration(import) => match &import.module_reference {
                TSModuleReference::ExternalModuleReference(reference) => {
                    reference.expression.value.as_str()
                }
                TSModuleReference::IdentifierReference(_) | TSModuleReference::QualifiedName(_) => {
                    continue;
                }
            },
            _ => continue,
        };
        specifiers.push(Cow::Borrowed(written));
    }
    let mut references = Vec::new();
    for comment in &program.comments {
        if !comment.is_line() {
            continue;
        }
        let span = comment.content_span();
        let text = &program.source_text[span.start as usize..span.end as usize];
        if let Some(path) = reference_path(text) {
            let specifier = reference_specifier(path);
            references.push((specifier.to_compact_string(), comment.span.start));
            specifiers.push(specifier);
        }
    }
    Scan {
        specifiers,
        value_uses: found.value_uses,
        references,
    }
}

/// The path a `/// <reference path="…" />` directive names.
///
/// `text` is the comment's content after `//`, so a directive starts with the
/// third slash.
fn reference_path(text: &str) -> Option<&str> {
    let directive = text.strip_prefix('/')?.trim_start();
    let attributes = directive.strip_prefix("<reference")?;
    let start = attributes.find("path=")? + "path=".len();
    let quoted = &attributes[start..];
    let quote = quoted.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &quoted[1..];
    let end = rest.find(quote)?;
    Some(&rest[..end])
}

/// Normalize a `/// <reference path>` into a relative specifier.
///
/// TypeScript resolves `path="globals.d.ts"` against the file that wrote it,
/// with no `./`, which would otherwise read as a package name.
fn reference_specifier(path: &str) -> Cow<'_, str> {
    if path.starts_with("./") || path.starts_with("../") {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(uf_infra::into_string(uf_infra::cstr!("./{path}")))
    }
}

#[derive(Default)]
struct Found<'a> {
    import_types: Vec<&'a str>,
    value_uses: FxHashSet<CompactString>,
}

impl<'a> Visit<'a> for Found<'a> {
    fn visit_ts_type_query(&mut self, it: &TSTypeQuery<'a>) {
        match &it.expr_name {
            TSTypeQueryExprName::IdentifierReference(identifier) => {
                self.value_uses.insert(identifier.name.to_compact_string());
            }
            TSTypeQueryExprName::QualifiedName(qualified) => {
                if let Some(root) = type_name_root(&qualified.left) {
                    self.value_uses.insert(root.to_compact_string());
                }
            }
            _ => {}
        }
        walk::walk_ts_type_query(self, it);
    }

    fn visit_ts_import_type(&mut self, it: &oxc_ast::ast::TSImportType<'a>) {
        self.import_types.push(it.source.value.as_str());
        walk::walk_ts_import_type(self, it);
    }

    fn visit_class(&mut self, it: &oxc_ast::ast::Class<'a>) {
        if let Some(heritage) = &it.heritage
            && let Some(root) = expression_root(&heritage.expression)
        {
            self.value_uses.insert(root.to_compact_string());
        }
        walk::walk_class(self, it);
    }

    // A computed key reads a value: `[constructFromSymbol]: …` in date-fns
    // needs `constructFromSymbol` imported as one, even where the member it
    // keys has no Flow spelling and is left out.
    fn visit_ts_property_signature(&mut self, it: &oxc_ast::ast::TSPropertySignature<'a>) {
        self.computed_key(it.computed, &it.key);
        walk::walk_ts_property_signature(self, it);
    }

    fn visit_ts_method_signature(&mut self, it: &oxc_ast::ast::TSMethodSignature<'a>) {
        self.computed_key(it.computed, &it.key);
        walk::walk_ts_method_signature(self, it);
    }

    fn visit_property_definition(&mut self, it: &oxc_ast::ast::PropertyDefinition<'a>) {
        self.computed_key(it.computed, &it.key);
        walk::walk_property_definition(self, it);
    }

    fn visit_method_definition(&mut self, it: &oxc_ast::ast::MethodDefinition<'a>) {
        self.computed_key(it.computed, &it.key);
        walk::walk_method_definition(self, it);
    }
}

impl Found<'_> {
    fn computed_key(&mut self, computed: bool, key: &oxc_ast::ast::PropertyKey<'_>) {
        if computed
            && let Some(expression) = key.as_expression()
            && let Some(root) = expression_root(expression)
        {
            self.value_uses.insert(root.to_compact_string());
        }
    }
}

/// The leftmost identifier of a qualified type name.
pub(crate) fn type_name_root<'a>(name: &TSTypeName<'a>) -> Option<&'a str> {
    match name {
        TSTypeName::IdentifierReference(identifier) => Some(identifier.name.as_str()),
        TSTypeName::QualifiedName(qualified) => type_name_root(&qualified.left),
        TSTypeName::ThisExpression(_) => None,
    }
}

/// The leftmost identifier of `a` or `a.b.c`.
pub(crate) fn expression_root<'a>(expression: &Expression<'a>) -> Option<&'a str> {
    match expression {
        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        Expression::StaticMemberExpression(member) => expression_root(&member.object),
        _ => None,
    }
}

/// The name an import or export specifier spells.
pub(crate) fn export_name(name: &ModuleExportName<'_>) -> CompactString {
    match name {
        ModuleExportName::IdentifierName(identifier) => identifier.name.to_compact_string(),
        ModuleExportName::IdentifierReference(identifier) => identifier.name.to_compact_string(),
        ModuleExportName::StringLiteral(literal) => literal.value.to_compact_string(),
    }
}

/// Whether a namespace declares anything that exists at runtime.
pub(crate) fn instantiated(namespace: &TSNamespaceDeclaration<'_>) -> bool {
    match &namespace.body {
        TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => instantiated(inner),
        TSNamespaceDeclarationBody::TSModuleBlock(block) => block.body.iter().any(|statement| {
            let declaration = match statement {
                Statement::ExportDeclaration(export) => &export.declaration,
                other => match other.as_declaration() {
                    Some(declaration) => declaration,
                    None => return false,
                },
            };
            match declaration {
                Declaration::VariableDeclaration(_)
                | Declaration::FunctionDeclaration(_)
                | Declaration::ClassDeclaration(_)
                | Declaration::TSEnumDeclaration(_)
                | Declaration::TSImportEqualsDeclaration(_) => true,
                Declaration::TSNamespaceDeclaration(inner) => instantiated(inner),
                Declaration::TSTypeAliasDeclaration(_)
                | Declaration::TSInterfaceDeclaration(_)
                | Declaration::TSGlobalDeclaration(_)
                | Declaration::TSExternalModuleDeclaration(_) => false,
            }
        }),
    }
}

/// Summarize a file whose specifiers have been resolved.
pub(crate) fn summarize(
    program: &Program<'_>,
    scan: Scan<'_>,
    targets: FxHashMap<CompactString, Target>,
) -> Summary {
    let mut summary = Summary {
        targets,
        value_uses: scan.value_uses,
        references: scan.references,
        ..Summary::default()
    };
    // TypeScript's rule for an ambient module: with no `export { … }`,
    // `export * from`, `export =` or `export default <expression>` anywhere in
    // it, every declaration is exported whether or not it says so. That is why
    // `tsc` ends every declaration file it writes with `export {};`.
    let mut explicit = false;
    let mut declared: Vec<CompactString> = Vec::new();

    for statement in &program.body {
        match statement {
            Statement::ImportDeclaration(import) => {
                let target = summary.target(import.source.value.as_str());
                let type_only = import.import_kind.is_type();
                for specifier in import.specifiers.iter().flatten() {
                    let (local, imported, specifier_type_only) = match specifier {
                        ImportDeclarationSpecifier::ImportSpecifier(named) => (
                            named.local.name.to_compact_string(),
                            Imported::Named(export_name(&named.imported)),
                            named.import_kind.is_type(),
                        ),
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => (
                            default.local.name.to_compact_string(),
                            Imported::Named(CompactString::const_new("default")),
                            false,
                        ),
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => (
                            namespace.local.name.to_compact_string(),
                            Imported::Namespace,
                            false,
                        ),
                    };
                    summary.imports.insert(
                        local,
                        Import {
                            target: target.clone(),
                            imported,
                            type_only: type_only || specifier_type_only,
                        },
                    );
                }
            }
            Statement::TSImportEqualsDeclaration(import) => {
                summary.import_equals(import, &mut declared);
            }
            Statement::ExportNamedDeclaration(list) => {
                explicit = true;
                for specifier in &list.specifiers {
                    let local = export_name(&specifier.local);
                    summary.value_uses.insert(local.clone());
                    summary
                        .exports
                        .insert(export_name(&specifier.exported), Export::Local(local));
                }
            }
            Statement::ExportFromDeclaration(from) => {
                explicit = true;
                let target = summary.target(from.source.value.as_str());
                let type_only = from.export_kind.is_type();
                for specifier in &from.specifiers {
                    summary.exports.insert(
                        export_name(&specifier.exported),
                        Export::From {
                            target: target.clone(),
                            imported: Imported::Named(export_name(&specifier.local)),
                            type_only: type_only || specifier.export_kind.is_type(),
                        },
                    );
                }
            }
            Statement::ExportAllDeclaration(all) => {
                explicit = true;
                let target = summary.target(all.source.value.as_str());
                match &all.exported {
                    Some(name) => {
                        summary.exports.insert(
                            export_name(name),
                            Export::From {
                                target,
                                imported: Imported::Namespace,
                                type_only: all.export_kind.is_type(),
                            },
                        );
                    }
                    None => summary.stars.push(target),
                }
            }
            Statement::ExportDeclaration(export) => {
                for name in summary.declare(&export.declaration) {
                    summary.self_exported.insert(name.clone());
                    summary.exports.insert(name.clone(), Export::Local(name));
                }
            }
            Statement::ExportDefaultDeclaration(default) => {
                let default_name = CompactString::const_new("default");
                match &default.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        let export = match &function.id {
                            Some(id) => {
                                let name = id.name.to_compact_string();
                                summary.locals.entry(name.clone()).or_default().functions += 1;
                                Export::Local(name)
                            }
                            None => Export::Anonymous(Kinds::VALUE),
                        };
                        summary.exports.insert(default_name, export);
                    }
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        let export = match &class.id {
                            Some(id) => {
                                let name = id.name.to_compact_string();
                                summary.locals.entry(name.clone()).or_default().classes += 1;
                                Export::Local(name)
                            }
                            None => Export::Anonymous(Kinds::BOTH),
                        };
                        summary.exports.insert(default_name, export);
                    }
                    ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                        let name = interface.id.name.to_compact_string();
                        summary.locals.entry(name.clone()).or_default().interfaces += 1;
                        summary.exports.insert(default_name, Export::Local(name));
                    }
                    expression => {
                        explicit = true;
                        if let Some(Expression::Identifier(identifier)) = expression.as_expression()
                        {
                            let name = identifier.name.to_compact_string();
                            summary.value_uses.insert(name.clone());
                            summary.exports.insert(default_name, Export::Local(name));
                        }
                    }
                }
            }
            Statement::TSExportAssignment(assignment) => {
                explicit = true;
                if let Expression::Identifier(identifier) = &assignment.expression {
                    let name = identifier.name.to_compact_string();
                    summary.value_uses.insert(name.clone());
                    summary.equals = Some(name);
                }
            }
            other => {
                if let Some(declaration) = other.as_declaration() {
                    declared.extend(summary.declare(declaration));
                }
            }
        }
    }

    if !explicit {
        for name in declared {
            summary.self_exported.insert(name.clone());
            summary
                .exports
                .entry(name.clone())
                .or_insert(Export::Local(name));
        }
    }
    summary
}

impl Summary {
    fn target(&self, specifier: &str) -> Target {
        self.targets
            .get(specifier)
            .cloned()
            .unwrap_or_else(|| Target::Package(specifier.to_compact_string()))
    }

    fn import_equals(
        &mut self,
        import: &oxc_ast::ast::TSImportEqualsDeclaration<'_>,
        declared: &mut Vec<CompactString>,
    ) {
        let local = import.id.name.to_compact_string();
        match &import.module_reference {
            TSModuleReference::ExternalModuleReference(reference) => {
                let target = self.target(reference.expression.value.as_str());
                self.imports.insert(
                    local,
                    Import {
                        target,
                        imported: Imported::Namespace,
                        type_only: import.import_kind.is_type(),
                    },
                );
            }
            // An alias of a namespace member is not something Flow can
            // import; the emitter reports it. Recorded as a declaration so the
            // name is not mistaken for a global.
            TSModuleReference::IdentifierReference(_) | TSModuleReference::QualifiedName(_) => {
                let entry = self.locals.entry(local.clone()).or_default();
                entry.variables += 1;
                entry.aliases += 1;
                declared.push(local);
            }
        }
    }

    /// Record a declaration's names, and return them.
    fn declare(&mut self, declaration: &Declaration<'_>) -> Vec<CompactString> {
        let mut names = Vec::new();
        let mut add = |summary: &mut Self, name: &str, record: fn(&mut Local)| {
            let name = name.to_compact_string();
            record(summary.locals.entry(name.clone()).or_default());
            names.push(name);
        };
        match declaration {
            Declaration::VariableDeclaration(variables) => {
                for declarator in &variables.declarations {
                    if let Some(identifier) = declarator.id.get_binding_identifier() {
                        add(self, identifier.name.as_str(), |local| local.variables += 1);
                    }
                }
            }
            Declaration::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    add(self, id.name.as_str(), |local| local.functions += 1);
                }
            }
            Declaration::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    add(self, id.name.as_str(), |local| local.classes += 1);
                }
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                add(self, alias.id.name.as_str(), |local| local.aliases += 1);
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                add(self, interface.id.name.as_str(), |local| {
                    local.interfaces += 1
                });
            }
            Declaration::TSEnumDeclaration(declaration) => {
                add(self, declaration.id.name.as_str(), |local| local.enums += 1);
            }
            Declaration::TSNamespaceDeclaration(namespace) => {
                let values = instantiated(namespace);
                let name = namespace.id.name.to_compact_string();
                let local = self.locals.entry(name.clone()).or_default();
                local.namespaces += 1;
                local.instantiated |= values;
                names.push(name);
            }
            Declaration::TSImportEqualsDeclaration(import) => {
                let mut declared = Vec::new();
                self.import_equals(import, &mut declared);
                names.push(import.id.name.to_compact_string());
            }
            Declaration::TSGlobalDeclaration(_) | Declaration::TSExternalModuleDeclaration(_) => {}
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_directive_names_its_path() {
        assert_eq!(
            reference_path("/ <reference path=\"globals.d.ts\" />"),
            Some("globals.d.ts")
        );
        assert_eq!(
            reference_path("/ <reference path='./a.d.ts'/>"),
            Some("./a.d.ts")
        );
        assert_eq!(reference_path("/ <reference types=\"node\" />"), None);
        assert_eq!(reference_path(" an ordinary comment"), None);
    }

    #[test]
    fn a_reference_path_without_a_dot_is_still_relative() {
        assert_eq!(reference_specifier("globals.d.ts"), "./globals.d.ts");
        assert_eq!(reference_specifier("../a.d.ts"), "../a.d.ts");
    }

    #[test]
    fn an_interface_beside_a_const_is_split() {
        let local = Local {
            interfaces: 1,
            variables: 1,
            ..Local::default()
        };
        assert_eq!(
            local.kinds(),
            Kinds {
                type_: true,
                value: true,
                split: true,
                self_interface: false,
            }
        );
    }

    #[test]
    fn a_class_is_one_binding_that_is_both() {
        let local = Local {
            classes: 1,
            interfaces: 1,
            ..Local::default()
        };
        assert_eq!(local.kinds(), Kinds::BOTH);
    }

    #[test]
    fn a_namespace_of_types_beside_a_function_is_split() {
        let local = Local {
            namespaces: 1,
            functions: 1,
            ..Local::default()
        };
        assert!(local.kinds().split);
    }
}
