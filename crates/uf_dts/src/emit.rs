//! One declaration file, printed as Flow.
//!
//! The target is not the Flow of five years ago. The vendored checker runs
//! with `ts_syntax`, `tslib_syntax` and `ts_utility_syntax` on (see
//! `uf_check`'s `options`), and with those Flow parses and types `keyof`,
//! conditional types with `infer`, mapped types with `as` clauses and `-?`,
//! template literal types, `x is T` guards, `in`/`out`/`const` type
//! parameters, `extends` bounds, labelled and optional tuple elements,
//! `readonly` members, abstract classes and constructor types, and the
//! TypeScript utility types by their own names. So most of a declaration file
//! is carried over as it is written, and the work here is the part where the
//! two languages *mean* different things:
//!
//! * **Object types are inexact in TypeScript and exact in Flow**, so every
//!   type literal ends in `...`. An interface is inexact in both.
//! * **A name is one binding in Flow.** TypeScript lets `interface ZodType`
//!   and `const ZodType` share a name, and zod does that for every schema. The
//!   value half is declared as `ZodType$value` and exported under the name it
//!   had, which a Flow module can do because its type exports and its value
//!   exports are two tables. Interfaces merge into one interface, and into a
//!   class of the same name.
//! * **An import says what it imports.** `import { I }` of an interface is an
//!   error in Flow and `import type { C }` of a class cannot be extended, so
//!   each specifier is decided from what the target module really exports —
//!   see [`crate::summary`] — and from how the name is used here when the
//!   target is another package this translation cannot see.
//! * **`{}` means "not null or undefined"**, which Flow spells
//!   `$NonMaybeType<mixed>`; `object` is `interface {}`, `unknown` is `mixed`,
//!   `never` is `empty` and `undefined` is `void`.
//!
//! What has no Flow meaning at all is a hole: `any` in the output and a
//! [`Hole`] naming the declaration. See [`crate::hole::Construct`] for the
//! list.

use compact_str::{CompactString, ToCompactString, format_compact};
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, ExportAllDeclaration,
    ExportDefaultDeclaration, ExportDefaultDeclarationKind, ExportFromDeclaration,
    ExportNamedDeclaration, Expression, FormalParameters, Function, ImportDeclaration,
    ImportDeclarationSpecifier, MethodDefinitionKind, MethodDefinitionType, Program, PropertyKey,
    Statement, TSAccessibility, TSConditionalType, TSEnumDeclaration, TSEnumMemberName,
    TSExportAssignment, TSGlobalDeclaration, TSImportEqualsDeclaration, TSImportType,
    TSImportTypeQualifier, TSIndexSignature, TSInterfaceDeclaration, TSIntersectionType, TSLiteral,
    TSMappedType, TSMappedTypeModifierOperator, TSMethodSignatureKind, TSModuleReference,
    TSNamespaceDeclaration, TSNamespaceDeclarationBody, TSSignature, TSThisParameter,
    TSTupleElement, TSTupleType, TSType, TSTypeAnnotation, TSTypeLiteral, TSTypeName,
    TSTypeOperatorOperator, TSTypeParameterDeclaration, TSTypeParameterInstantiation,
    TSTypePredicateName, TSTypeQuery, TSTypeQueryExprName, TSTypeReference, UnaryOperator,
    VariableDeclaration, VariableDeclarationKind,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use smallvec::SmallVec;
use uf_infra::{FxHashMap, FxHashSet};

use crate::hole::{Construct, Hole};
use crate::printer::Printer;
use crate::resolve::{flow_path, relative_specifier};
use crate::summary::{Import, Imported, Kinds, Summary, Target, export_name};
use crate::unit::Oracle;

/// Print `program`, the declaration file at `path`.
pub(crate) fn module(
    oracle: &dyn Oracle,
    path: &str,
    program: &Program<'_>,
    summary: &Summary,
) -> (String, Vec<Hole>) {
    let mut emitter = Emitter::new(oracle, path, program.source_text, summary);
    emitter.references();
    let statements: SmallVec<[&Statement<'_>; 32]> = program.body.iter().collect();
    emitter.statements(&statements, Scope::Module);
    emitter.hoisted_imports();
    let Emitter {
        printer, mut holes, ..
    } = emitter;
    holes.sort_by_key(|hole| hole.line);
    (printer.finish(), holes)
}

/// How tightly a type binds, loosest first, for deciding where parentheses go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    /// A conditional type.
    Any,
    /// A function or constructor type.
    Function,
    Union,
    Intersection,
    /// `keyof T`, `infer U`.
    Prefix,
    /// `T[K]`, and everything that is one token or bracketed.
    Postfix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Module,
    Namespace,
}

/// How an import is written in Flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plan {
    /// `import * as ns`.
    Namespace,
    Value,
    Type,
    /// `import type { X }`, and `import { X as X$value }` when the value half
    /// is used.
    Split,
    /// `import typeof { X as X$typeof }`: a type-only import of a value that is
    /// used in `typeof`.
    Typeof,
    /// Both of the above, for a type-only import of a class or an enum that is
    /// used as a type and in `typeof`.
    TypeAndTypeof,
}

/// How a name's value half is spelled in this file.
#[derive(Debug, Clone)]
enum Value {
    /// `typeof X` is `typeof X$value`.
    Renamed(CompactString),
    /// `typeof X` is `X$typeof`, which already is the type.
    Typeof(CompactString),
}

/// What a `return` position prints.
enum Return<'r, 'a> {
    Annotation(Option<&'r TSTypeAnnotation<'a>>),
    Void,
}

/// Declarations in one scope that TypeScript merges and Flow must see once.
#[derive(Default)]
struct Merges<'s, 'a> {
    interfaces: FxHashMap<&'a str, SmallVec<[&'s TSInterfaceDeclaration<'a>; 2]>>,
    namespaces: FxHashMap<&'a str, SmallVec<[&'s TSNamespaceDeclaration<'a>; 2]>>,
    enums: FxHashMap<&'a str, SmallVec<[&'s TSEnumDeclaration<'a>; 2]>>,
    classes: FxHashSet<&'a str>,
    functions: FxHashSet<&'a str>,
    printed: FxHashSet<&'a str>,
    /// Namespaces already reported as merged into something Flow cannot
    /// merge them with, so a name declared as three namespaces is one hole.
    dropped_namespaces: FxHashSet<&'a str>,
}

impl<'s, 'a> Merges<'s, 'a> {
    fn collect(statements: &[&'s Statement<'a>]) -> Self {
        let mut merges = Self::default();
        for statement in statements {
            let declaration = match statement {
                Statement::ExportDeclaration(export) => &export.declaration,
                Statement::ExportDefaultDeclaration(default) => {
                    match &default.declaration {
                        ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                            merges
                                .interfaces
                                .entry(interface.id.name.as_str())
                                .or_default()
                                .push(interface);
                        }
                        ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                            if let Some(id) = &class.id {
                                merges.classes.insert(id.name.as_str());
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                other => match other.as_declaration() {
                    Some(declaration) => declaration,
                    None => continue,
                },
            };
            match declaration {
                Declaration::TSInterfaceDeclaration(interface) => merges
                    .interfaces
                    .entry(interface.id.name.as_str())
                    .or_default()
                    .push(interface),
                Declaration::TSNamespaceDeclaration(namespace) => merges
                    .namespaces
                    .entry(namespace.id.name.as_str())
                    .or_default()
                    .push(namespace),
                Declaration::TSEnumDeclaration(declaration) => merges
                    .enums
                    .entry(declaration.id.name.as_str())
                    .or_default()
                    .push(declaration),
                Declaration::ClassDeclaration(class) => {
                    if let Some(id) = &class.id {
                        merges.classes.insert(id.name.as_str());
                    }
                }
                Declaration::FunctionDeclaration(function) => {
                    if let Some(id) = &function.id {
                        merges.functions.insert(id.name.as_str());
                    }
                }
                _ => {}
            }
        }
        merges
    }
}

fn method_properties<'s, 'a>(
    class: &'s Class<'a>,
    merges: &Merges<'s, 'a>,
    merged: &[&'s TSInterfaceDeclaration<'a>],
) -> FxHashSet<CompactString> {
    let mut interface_properties = FxHashSet::default();
    for interface in merged {
        collect_function_properties(interface, &mut interface_properties);
    }
    for implemented in &class.implements {
        collect_function_properties_from_name(
            &implemented.expression,
            implemented.type_arguments.as_deref(),
            merges,
            &mut interface_properties,
        );
    }
    if interface_properties.is_empty() {
        return FxHashSet::default();
    }
    class
        .body
        .body
        .iter()
        .filter_map(class_method_key)
        .filter(|key| interface_properties.contains(key))
        .collect()
}

fn collect_function_properties(
    interface: &TSInterfaceDeclaration<'_>,
    out: &mut FxHashSet<CompactString>,
) {
    out.extend(interface.body.body.iter().filter_map(function_property_key));
}

fn collect_function_properties_from_name<'s, 'a>(
    name: &TSTypeName<'a>,
    arguments: Option<&TSTypeParameterInstantiation<'a>>,
    merges: &Merges<'s, 'a>,
    out: &mut FxHashSet<CompactString>,
) {
    let TSTypeName::IdentifierReference(identifier) = name else {
        return;
    };
    let written = identifier.name.as_str();
    if let Some(interfaces) = merges.interfaces.get(written) {
        for interface in interfaces {
            collect_function_properties(interface, out);
        }
        return;
    }
    let Some(arguments) = arguments else {
        return;
    };
    match written {
        "Partial" | "Required" | "Readonly" => {
            if let Some(target) = arguments.params.first() {
                collect_function_properties_from_type(target, merges, out);
            }
        }
        "Omit" => {
            let (Some(target), Some(omitted)) = (arguments.params.first(), arguments.params.get(1))
            else {
                return;
            };
            let Some(omitted) = literal_keys(omitted) else {
                return;
            };
            let mut properties = FxHashSet::default();
            collect_function_properties_from_type(target, merges, &mut properties);
            properties.retain(|key| !omitted.contains(key));
            out.extend(properties);
        }
        "Pick" => {
            let (Some(target), Some(picked)) = (arguments.params.first(), arguments.params.get(1))
            else {
                return;
            };
            let Some(picked) = literal_keys(picked) else {
                return;
            };
            let mut properties = FxHashSet::default();
            collect_function_properties_from_type(target, merges, &mut properties);
            properties.retain(|key| picked.contains(key));
            out.extend(properties);
        }
        _ => {}
    }
}

fn collect_function_properties_from_type<'s, 'a>(
    ty: &TSType<'a>,
    merges: &Merges<'s, 'a>,
    out: &mut FxHashSet<CompactString>,
) {
    match ty {
        TSType::TSParenthesizedType(parenthesized) => {
            collect_function_properties_from_type(&parenthesized.type_annotation, merges, out);
        }
        TSType::TSTypeReference(reference) => collect_function_properties_from_name(
            &reference.type_name,
            reference.type_arguments.as_deref(),
            merges,
            out,
        ),
        TSType::TSIntersectionType(intersection) => {
            for member in &intersection.types {
                collect_function_properties_from_type(member, merges, out);
            }
        }
        TSType::TSTypeLiteral(literal) => {
            out.extend(literal.members.iter().filter_map(function_property_key));
        }
        _ => {}
    }
}

fn literal_keys(ty: &TSType<'_>) -> Option<FxHashSet<CompactString>> {
    let mut keys = FxHashSet::default();
    collect_literal_keys(ty, &mut keys)?;
    Some(keys)
}

fn collect_literal_keys(ty: &TSType<'_>, out: &mut FxHashSet<CompactString>) -> Option<()> {
    match ty {
        TSType::TSParenthesizedType(parenthesized) => {
            collect_literal_keys(&parenthesized.type_annotation, out)
        }
        TSType::TSNeverKeyword(_) => Some(()),
        TSType::TSUnionType(union) => {
            for member in &union.types {
                collect_literal_keys(member, out)?;
            }
            Some(())
        }
        TSType::TSLiteralType(literal) => match &literal.literal {
            TSLiteral::StringLiteral(string) => {
                out.insert(string.value.to_compact_string());
                Some(())
            }
            _ => None,
        },
        _ => None,
    }
}

fn class_method_key(element: &ClassElement<'_>) -> Option<CompactString> {
    let ClassElement::MethodDefinition(method) = element else {
        return None;
    };
    if method.r#static
        || method.optional
        || method.kind != MethodDefinitionKind::Method
        || method.r#type == MethodDefinitionType::TSAbstractMethodDefinition
    {
        return None;
    }
    key_text(&method.key, method.computed)
}

fn function_property_key(member: &TSSignature<'_>) -> Option<CompactString> {
    let TSSignature::TSPropertySignature(property) = member else {
        return None;
    };
    let annotation = property.type_annotation.as_deref()?;
    if !matches!(annotation.type_annotation, TSType::TSFunctionType(_)) {
        return None;
    }
    key_text(&property.key, property.computed)
}

fn key_text(key: &PropertyKey<'_>, _computed: bool) -> Option<CompactString> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.to_compact_string()),
        PropertyKey::PrivateIdentifier(_) => None,
        PropertyKey::StringLiteral(literal) => Some(quoted(literal.value.as_str()).into()),
        PropertyKey::NumericLiteral(literal) => Some(
            literal
                .raw
                .as_ref()
                .filter(|raw| !raw.contains('_'))
                .map_or_else(|| number_text(literal.value), |raw| raw.to_compact_string()),
        ),
        PropertyKey::TemplateLiteral(template) if template.expressions.is_empty() => {
            let cooked = template
                .quasis
                .first()
                .and_then(|quasi| quasi.value.cooked.as_ref())
                .map_or("", |cooked| cooked.as_str());
            Some(quoted(cooked).into())
        }
        PropertyKey::StaticMemberExpression(member) if matches!(&member.object, Expression::Identifier(object) if object.name == "Symbol") => {
            match member.property.name.as_str() {
                "iterator" => Some(CompactString::const_new("@@iterator")),
                "asyncIterator" => Some(CompactString::const_new("@@asyncIterator")),
                _ => None,
            }
        }
        _ => None,
    }
}

struct Emitter<'e> {
    oracle: &'e dyn Oracle,
    path: &'e str,
    summary: &'e Summary,
    printer: Printer,
    holes: Vec<Hole>,
    plans: FxHashMap<CompactString, Plan>,
    values: FxHashMap<CompactString, Value>,
    /// Names bound by the type parameters of whatever is being printed.
    type_scope: Vec<CompactString>,
    /// The declaration being printed, outermost first, for naming a hole.
    context: Vec<CompactString>,
    /// Modules an `import("…")` type names, as `(specifier, local)`.
    hoisted: Vec<(CompactString, CompactString)>,
    /// Split values already given their `export { X$value as X }`.
    exported_values: FxHashSet<CompactString>,
    /// Missing relative specifiers already reported.
    missing: FxHashSet<CompactString>,
    /// Whether a `unique symbol` may be printed here: only a `const` may have
    /// one in Flow.
    unique_symbol: bool,
    /// The `out` type parameters of the interface or class being printed.
    ///
    /// TypeScript reads a property covariantly whether or not it is
    /// `readonly`, so `interface ZodType<out Output> { _output: Output }` is a
    /// declaration it accepts. Flow reads a writable property invariantly and
    /// rejects the same one. A property whose type mentions one of these is
    /// printed `readonly`, which is the variance TypeScript was already giving
    /// it — and without which `ZodString` would not be a `ZodType<unknown>`.
    covariant: Vec<CompactString>,
    /// The one index signature the member list being printed keeps, by its
    /// offset, and whether its key includes `string`.
    indexer: Option<(u32, bool)>,
    /// Top-level type names this file binds that Flow reserves, and the name
    /// each is bound under instead. See [`RESERVED_TYPE_NAMES`].
    type_renames: FxHashMap<CompactString, CompactString>,
    /// Interfaces merged into a namespace of the same name, whose bare
    /// references are to `Name.$Self`.
    self_interfaces: FxHashSet<CompactString>,
}

/// The names Flow refuses to let a type, an interface or a class be declared
/// under, because they name its own utility types.
///
/// `flow_typing_errors`' `IncorrectType::is_type_reserved`, spelled out: the
/// port does not export the list, and a package that declares
/// `type Values<T>` is ordinary TypeScript.
const RESERVED_TYPE_NAMES: [&str; 7] = [
    "$NonMaybeType",
    "NonNullable",
    "$ReadOnly",
    "Readonly",
    "$Keys",
    "Values",
    "$Values",
];

impl<'e> Emitter<'e> {
    fn new(oracle: &'e dyn Oracle, path: &'e str, source: &str, summary: &'e Summary) -> Self {
        let mut emitter = Self {
            oracle,
            path,
            summary,
            printer: Printer::new(source),
            holes: Vec::new(),
            plans: FxHashMap::default(),
            values: FxHashMap::default(),
            type_scope: Vec::new(),
            context: Vec::new(),
            hoisted: Vec::new(),
            exported_values: FxHashSet::default(),
            missing: FxHashSet::default(),
            unique_symbol: false,
            covariant: Vec::new(),
            indexer: None,
            type_renames: FxHashMap::default(),
            self_interfaces: FxHashSet::default(),
        };
        for name in summary.locals.keys().chain(summary.imports.keys()) {
            if RESERVED_TYPE_NAMES.contains(&name.as_str()) {
                emitter
                    .type_renames
                    .insert(name.clone(), format_compact!("{name}$type"));
            }
        }
        // Decided before anything is printed, because the first `typeof X` can
        // come before the import of `X` is reached in a hand-written file.
        for (local, import) in &summary.imports {
            // An interface merged into its namespace elsewhere in the package
            // is that namespace's `$Self` here too.
            if emitter
                .import_kinds(import)
                .is_some_and(|kinds| kinds.self_interface)
            {
                emitter.self_interfaces.insert(local.clone());
            }
            let plan = emitter.plan(local, import);
            match plan {
                Plan::Split => {
                    emitter.values.insert(
                        local.clone(),
                        Value::Renamed(format_compact!("{local}$value")),
                    );
                }
                Plan::Typeof | Plan::TypeAndTypeof => {
                    emitter.values.insert(
                        local.clone(),
                        Value::Typeof(format_compact!("{local}$typeof")),
                    );
                }
                Plan::Namespace | Plan::Value | Plan::Type => {}
            }
            emitter.plans.insert(local.clone(), plan);
        }
        for (name, local) in &summary.locals {
            if local.kinds().split {
                emitter.values.insert(
                    name.clone(),
                    Value::Renamed(format_compact!("{name}$value")),
                );
            }
        }
        emitter
    }

    // ----------------------------------------------------------------------
    // Statements
    // ----------------------------------------------------------------------

    /// What a `/// <reference path>` names, imported.
    ///
    /// The referenced file is a script: what it declares is global to
    /// TypeScript, and a Flow module has no global scope to add to. Its
    /// translation exports everything it declares — a script has no export
    /// declarations, so every declaration is implicitly exported — and the
    /// file that referenced it imports each name it does not bind itself.
    fn references(&mut self) {
        let references = self.summary.references.clone();
        for (specifier, offset) in references {
            let source = self.specifier(&specifier, offset);
            let Some(Target::Module(module)) =
                self.summary.targets.get(specifier.as_str()).cloned()
            else {
                continue;
            };
            let mut types: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
            let mut values: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
            for (name, kinds) in self.oracle.exports_of(&module) {
                if self.is_bound(&name) {
                    continue;
                }
                match kinds {
                    Some(kinds) if kinds.value && !kinds.split => {
                        values.push((name.clone(), name));
                    }
                    _ => types.push((name.clone(), name)),
                }
            }
            if types.is_empty() && values.is_empty() {
                continue;
            }
            self.printer.anchor(offset);
            self.import_group("import type", &types, &source);
            self.import_group("import", &values, &source);
        }
    }

    /// Whether a member's type mentions an `out` parameter of its owner.
    fn mentions_covariant(&self, annotation: Option<&TSTypeAnnotation<'_>>) -> bool {
        let Some(annotation) = annotation else {
            return false;
        };
        if self.covariant.is_empty() {
            return false;
        }
        let mut mentions = Mentions {
            names: &self.covariant,
            found: false,
        };
        mentions.visit_ts_type(&annotation.type_annotation);
        mentions.found
    }

    /// Pick the one index signature a member list keeps, returning the choice
    /// it replaces.
    ///
    /// TypeScript allows a `string` and a `number` index signature side by
    /// side, and requires the `number` one's type to be a subtype of the
    /// `string` one's — which makes the `string` one the whole story, since a
    /// number key is a string key to JavaScript. Flow allows one indexer per
    /// object type, so that is the one kept.
    fn enter_indexers(&mut self, indexers: &[&TSIndexSignature<'_>]) -> Option<(u32, bool)> {
        let chosen = indexers
            .iter()
            .find(|index| index_key_includes(index, KeyKind::String))
            .or_else(|| indexers.first())
            .map(|index| (index.span.start, index_key_includes(index, KeyKind::String)));
        std::mem::replace(&mut self.indexer, chosen)
    }

    /// Whether `index` is the indexer its member list keeps. When it is not, a
    /// hole says so — unless the kept one is keyed by `string` and this one by
    /// `number`, which it already covers.
    fn keeps_indexer(&mut self, index: &TSIndexSignature<'_>) -> bool {
        match self.indexer {
            Some((chosen, _)) if chosen == index.span.start => true,
            Some((_, string_kept)) => {
                if !(string_kept && index_key_includes(index, KeyKind::Number)) {
                    self.hole(
                        Construct::IndexSignatureKey,
                        "Flow holds one index signature per object type, so this one is left out",
                        index.span.start,
                    );
                }
                false
            }
            None => true,
        }
    }

    /// Make the `out` parameters of `declaration` the ones properties are
    /// checked against, returning the ones they replace.
    fn enter_variance(
        &mut self,
        declaration: Option<&TSTypeParameterDeclaration<'_>>,
    ) -> Vec<CompactString> {
        let covariant = declaration
            .map(|declaration| {
                declaration
                    .params
                    .iter()
                    .filter(|parameter| parameter.out && !parameter.r#in)
                    .map(|parameter| parameter.name.name.to_compact_string())
                    .collect()
            })
            .unwrap_or_default();
        std::mem::replace(&mut self.covariant, covariant)
    }

    fn statements<'s, 'a>(&mut self, statements: &[&'s Statement<'a>], scope: Scope) {
        let mut merges = Merges::collect(statements);
        if scope == Scope::Module {
            for name in merges.interfaces.keys() {
                if merges.namespaces.contains_key(name) && !merges.classes.contains(name) {
                    self.self_interfaces.insert(name.to_compact_string());
                }
            }
        }
        for statement in statements {
            self.statement(statement, scope, &mut merges);
        }
    }

    fn statement<'s, 'a>(
        &mut self,
        statement: &'s Statement<'a>,
        scope: Scope,
        merges: &mut Merges<'s, 'a>,
    ) {
        match statement {
            Statement::ImportDeclaration(import) => self.import(import),
            Statement::TSImportEqualsDeclaration(import) => {
                self.import_equals(import, false);
            }
            Statement::ExportNamedDeclaration(list) => self.export_list(list),
            Statement::ExportFromDeclaration(from) => self.export_from(from),
            Statement::ExportAllDeclaration(all) => self.export_all(all),
            Statement::ExportDeclaration(export) => {
                self.declaration(&export.declaration, scope, merges, export.span.start);
            }
            Statement::ExportDefaultDeclaration(default) => self.export_default(default, merges),
            Statement::TSExportAssignment(assignment) => {
                self.export_assignment(assignment, merges);
            }
            // `export as namespace X` declares a global for scripts. Nothing a
            // module import sees depends on it.
            Statement::TSNamespaceExportDeclaration(_) => {}
            other => {
                if let Some(declaration) = other.as_declaration() {
                    self.declaration(declaration, scope, merges, other.span().start);
                }
            }
        }
    }

    fn declaration<'s, 'a>(
        &mut self,
        declaration: &'s Declaration<'a>,
        scope: Scope,
        merges: &mut Merges<'s, 'a>,
        offset: u32,
    ) {
        match declaration {
            Declaration::VariableDeclaration(variables) => self.variables(variables, scope, offset),
            Declaration::FunctionDeclaration(function) => self.function(function, scope, offset),
            Declaration::ClassDeclaration(class) => {
                self.class(class, scope, merges, offset, ClassExport::AsDeclared);
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                let name = alias.id.name.to_compact_string();
                self.printer.anchor(offset);
                self.context.push(name.clone());
                let scope_len = self.type_scope.len();
                let binding = self.type_binding(&name);
                let exported = self.exported(&name, scope);
                self.printer.text(if exported && binding == name {
                    "export type "
                } else {
                    "type "
                });
                self.printer.text(&binding);
                let inferred =
                    alias
                        .type_parameters
                        .as_deref()
                        .map_or_else(Vec::new, |parameters| {
                            infer_variance(parameters, |uses| {
                                uses.ty(&alias.type_annotation, Polarity::Positive);
                            })
                        });
                self.type_parameters_with_variance(alias.type_parameters.as_deref(), &inferred);
                self.printer.text(" = ");
                self.ty(&alias.type_annotation, Prec::Any);
                self.printer.char(';');
                if exported {
                    self.export_renamed(&name, "export type");
                }
                self.type_scope.truncate(scope_len);
                self.context.pop();
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                self.interface(interface, scope, merges, offset, false);
            }
            Declaration::TSEnumDeclaration(declaration) => {
                self.enumeration(declaration, scope, merges, offset);
            }
            Declaration::TSNamespaceDeclaration(namespace) => {
                self.namespace(namespace, scope, merges, offset);
            }
            Declaration::TSGlobalDeclaration(global) => self.global(global),
            Declaration::TSExternalModuleDeclaration(module) => {
                let specifier = module.id.value.to_compact_string();
                self.hole(
                    Construct::ModuleAugmentation,
                    format!(
                        "`declare module \"{specifier}\"` adds to another module, and Flow reads \
                         a module's declarations from that module alone; the block is not \
                         translated"
                    ),
                    module.span.start,
                );
            }
            Declaration::TSImportEqualsDeclaration(import) => self.import_equals(import, true),
        }
    }

    /// Whether a declaration of `name` is exported under that name.
    fn exported(&self, name: &str, scope: Scope) -> bool {
        scope == Scope::Module && self.summary.self_exported.contains(name)
    }

    /// Whether `name`'s value half is declared under another name here.
    fn split_here(&self, name: &str, scope: Scope) -> bool {
        scope == Scope::Module
            && self
                .summary
                .locals
                .get(name)
                .is_some_and(|local| local.kinds().split)
    }

    /// The name a declaration of the type `name` binds in this file.
    fn type_binding(&self, name: &str) -> CompactString {
        self.type_renames
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_compact_string())
    }

    /// Export a declaration bound under a reserved name's replacement by the
    /// name it was written with.
    fn export_renamed(&mut self, name: &str, keyword: &str) {
        if let Some(renamed) = self.type_renames.get(name).cloned() {
            self.printer.space();
            self.printer
                .text(&format!("{keyword} {{ {renamed} as {name} }};"));
        }
    }

    fn export_value_once(&mut self, name: &str) {
        if self.exported_values.insert(name.to_compact_string()) {
            self.printer.space();
            self.printer
                .text(&format!("export {{ {name}$value as {name} }};"));
        }
    }

    fn variables(&mut self, variables: &VariableDeclaration<'_>, scope: Scope, offset: u32) {
        let keyword = match variables.kind {
            VariableDeclarationKind::Var => "var",
            VariableDeclarationKind::Let => "let",
            VariableDeclarationKind::Const
            | VariableDeclarationKind::Using
            | VariableDeclarationKind::AwaitUsing => "const",
        };
        for (index, declarator) in variables.declarations.iter().enumerate() {
            // A destructuring pattern is not a declaration a `.d.ts` can hold.
            let Some(identifier) = declarator.id.get_binding_identifier() else {
                continue;
            };
            let name = identifier.name.to_compact_string();
            self.printer.anchor(if index == 0 {
                offset
            } else {
                declarator.span.start
            });
            self.context.push(name.clone());
            let split = self.split_here(&name, scope);
            let exported = self.exported(&name, scope);
            self.printer.text(if exported && !split {
                "declare export "
            } else {
                "declare "
            });
            self.printer.text(keyword);
            self.printer.char(' ');
            self.printer.text(&name);
            if split {
                self.printer.text("$value");
            }
            self.printer.text(": ");
            match (&declarator.type_annotation, &declarator.init) {
                (Some(annotation), _) => {
                    self.unique_symbol = keyword == "const";
                    self.ty(&annotation.type_annotation, Prec::Any);
                    self.unique_symbol = false;
                }
                (None, Some(init)) => self.initializer_type(init),
                (None, None) => self.printer.text("any"),
            }
            self.printer.char(';');
            if split && exported {
                self.export_value_once(&name);
            }
            self.context.pop();
        }
    }

    /// The type a declaration file's `const x = "a"` gives `x`.
    fn initializer_type(&mut self, init: &Expression<'_>) {
        match init {
            Expression::StringLiteral(literal) => self.string(literal.value.as_str()),
            Expression::NumericLiteral(literal) => {
                self.number(literal.value, literal.raw.as_ref().map(|raw| raw.as_str()));
            }
            Expression::BooleanLiteral(literal) => {
                self.printer
                    .text(if literal.value { "true" } else { "false" });
            }
            Expression::BigIntLiteral(literal) => self.bigint(literal),
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation =>
            {
                match &unary.argument {
                    Expression::NumericLiteral(literal) => {
                        self.printer.char('-');
                        self.number(literal.value, literal.raw.as_ref().map(|raw| raw.as_str()));
                    }
                    Expression::BigIntLiteral(literal) => {
                        self.printer.char('-');
                        self.bigint(literal);
                    }
                    _ => self.printer.text("any"),
                }
            }
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
                let cooked = template
                    .quasis
                    .first()
                    .and_then(|quasi| quasi.value.cooked.as_ref())
                    .map_or("", |cooked| cooked.as_str());
                self.string(cooked);
            }
            _ => self.printer.text("any"),
        }
    }

    fn function(&mut self, function: &Function<'_>, scope: Scope, offset: u32) {
        let Some(id) = &function.id else {
            return;
        };
        let name = id.name.to_compact_string();
        let split = self.split_here(&name, scope);
        let exported = self.exported(&name, scope);
        self.printer.anchor(offset);
        self.context.push(name.clone());
        self.printer.text(if exported && !split {
            "declare export function "
        } else {
            "declare function "
        });
        self.printer.text(&name);
        if split {
            self.printer.text("$value");
        }
        self.signature(
            function.type_parameters.as_deref(),
            function.this_param.as_deref(),
            &function.params,
            Return::Annotation(function.return_type.as_deref()),
            false,
        );
        self.printer.char(';');
        if split && exported {
            self.export_value_once(&name);
        }
        self.context.pop();
    }

    fn merged_hole(&mut self, name: &str, kept: &str, dropped: &str, offset: u32) {
        self.hole(
            Construct::MergedDeclaration,
            format!(
                "`{name}` is declared as both a {kept} and a {dropped}, and Flow binds a name \
                 once; the {dropped} is left out"
            ),
            offset,
        );
    }

    fn class<'s, 'a>(
        &mut self,
        class: &'s Class<'a>,
        scope: Scope,
        merges: &mut Merges<'s, 'a>,
        offset: u32,
        export: ClassExport,
    ) {
        let name = match (&class.id, export) {
            (Some(id), _) => id.name.as_str(),
            (None, ClassExport::Default) => "$Default",
            (None, ClassExport::AsDeclared) => return,
        };
        if !merges.printed.insert(name) {
            return;
        }
        self.printer.anchor(offset);
        self.context.push(name.to_compact_string());
        let binding = self.type_binding(name);
        let exported = export == ClassExport::AsDeclared && self.exported(name, scope);
        let prefix = match export {
            ClassExport::Default => "declare export default ",
            ClassExport::AsDeclared if exported && binding == name => "declare export ",
            ClassExport::AsDeclared => "declare ",
        };
        self.printer.text(prefix);
        if class.r#abstract {
            self.printer.text("abstract ");
        }
        self.printer.text("class ");
        self.printer.text(&binding);
        let scope_len = self.type_scope.len();
        let interfaces_for_variance = merges.interfaces.get(name).cloned().unwrap_or_default();
        let inferred = class
            .type_parameters
            .as_deref()
            .map_or_else(Vec::new, |parameters| {
                infer_variance(parameters, |uses| {
                    uses.class_members(&class.body.body);
                    for interface in &interfaces_for_variance {
                        uses.members(&interface.body.body, Polarity::Positive);
                    }
                })
            });
        self.type_parameters_with_variance(class.type_parameters.as_deref(), &inferred);
        let outer_variance = self.enter_variance(class.type_parameters.as_deref());
        if let Some(heritage) = &class.heritage {
            let mut path = String::new();
            if self.value_path(&heritage.expression, &mut path) {
                self.printer.text(" extends ");
                self.printer.text(&path);
                self.type_arguments(heritage.type_arguments.as_deref());
            } else {
                self.hole(
                    Construct::ClassHeritage,
                    format!(
                        "`{name}` extends an expression that is not a name, which a Flow \
                         declaration cannot; the class extends nothing"
                    ),
                    heritage.expression.span().start,
                );
            }
        }
        if !class.implements.is_empty() {
            self.printer.text(" implements ");
            for (index, implemented) in class.implements.iter().enumerate() {
                if index > 0 {
                    self.printer.text(", ");
                }
                self.type_name(&implemented.expression);
                self.type_arguments(implemented.type_arguments.as_deref());
            }
        }
        self.printer.text(" {");
        self.printer.indent();
        let interfaces = merges.interfaces.get(name).cloned().unwrap_or_default();
        let method_properties = method_properties(class, merges, &interfaces);
        let mut indexers: SmallVec<[&TSIndexSignature<'_>; 2]> = class
            .body
            .body
            .iter()
            .filter_map(|element| match element {
                ClassElement::TSIndexSignature(index) if !index.r#static => Some(&**index),
                _ => None,
            })
            .collect();
        for interface in &interfaces {
            indexers.extend(interface_indexers(&interface.body.body));
        }
        let outer_indexer = self.enter_indexers(&indexers);
        for element in &class.body.body {
            self.class_member(element, &method_properties);
        }
        for interface in &interfaces {
            for member in &interface.body.body {
                if function_property_key(member).is_some_and(|key| method_properties.contains(&key))
                {
                    continue;
                }
                self.signature_member(member, ';');
            }
        }
        self.indexer = outer_indexer;
        self.printer.dedent();
        let end = interfaces
            .last()
            .map_or(class.body.span.end, |interface| interface.body.span.end);
        self.printer.anchor(end.saturating_sub(1));
        self.printer.char('}');
        if exported {
            self.export_renamed(name, "export");
        }
        self.covariant = outer_variance;
        self.type_scope.truncate(scope_len);
        self.context.pop();
    }

    /// The name path a class extends, spelled as a value; false when it is not
    /// one.
    fn value_path(&mut self, expression: &Expression<'_>, out: &mut String) -> bool {
        match expression {
            Expression::Identifier(identifier) => {
                match self.values.get(identifier.name.as_str()) {
                    Some(Value::Renamed(renamed)) => out.push_str(renamed),
                    Some(Value::Typeof(_)) => return false,
                    None => out.push_str(identifier.name.as_str()),
                }
                true
            }
            Expression::StaticMemberExpression(member) => {
                if !self.value_path(&member.object, out) {
                    return false;
                }
                out.push('.');
                out.push_str(member.property.name.as_str());
                true
            }
            _ => false,
        }
    }

    fn class_member(
        &mut self,
        element: &ClassElement<'_>,
        method_properties: &FxHashSet<CompactString>,
    ) {
        match element {
            ClassElement::MethodDefinition(method) => {
                // A private member is not part of what a consumer can touch.
                if method.accessibility == Some(TSAccessibility::Private)
                    || matches!(method.key, PropertyKey::PrivateIdentifier(_))
                {
                    return;
                }
                let function = &method.value;
                if method.kind == MethodDefinitionKind::Constructor {
                    self.printer.anchor(method.span.start);
                    self.printer.text("constructor");
                    self.signature(
                        function.type_parameters.as_deref(),
                        function.this_param.as_deref(),
                        &function.params,
                        Return::Void,
                        false,
                    );
                    self.printer.char(';');
                    return;
                }
                let Some(key) = self.key(&method.key, method.computed, method.span.start) else {
                    return;
                };
                self.printer.anchor(method.span.start);
                self.context.push(key.clone());
                if method.r#static {
                    self.printer.text("static ");
                }
                match method.kind {
                    MethodDefinitionKind::Get => {
                        self.printer.text("get ");
                        self.printer.text(&key);
                        self.printer.text("(): ");
                        self.return_type(function.return_type.as_deref(), false);
                    }
                    MethodDefinitionKind::Set => {
                        self.printer.text("set ");
                        self.printer.text(&key);
                        self.signature(None, None, &function.params, Return::Void, false);
                    }
                    MethodDefinitionKind::Method | MethodDefinitionKind::Constructor => {
                        if method.r#type == MethodDefinitionType::TSAbstractMethodDefinition {
                            self.printer.text("abstract ");
                        }
                        self.printer.text(&key);
                        if !method.r#static
                            && method.r#type != MethodDefinitionType::TSAbstractMethodDefinition
                            && method_properties.contains(&key)
                        {
                            self.printer.text(": ");
                            self.signature(
                                function.type_parameters.as_deref(),
                                function.this_param.as_deref(),
                                &function.params,
                                Return::Annotation(function.return_type.as_deref()),
                                true,
                            );
                        } else if method.optional {
                            self.printer.text("?: ");
                            self.signature(
                                function.type_parameters.as_deref(),
                                function.this_param.as_deref(),
                                &function.params,
                                Return::Annotation(function.return_type.as_deref()),
                                true,
                            );
                        } else {
                            self.signature(
                                function.type_parameters.as_deref(),
                                function.this_param.as_deref(),
                                &function.params,
                                Return::Annotation(function.return_type.as_deref()),
                                false,
                            );
                        }
                    }
                }
                self.printer.char(';');
                self.context.pop();
            }
            ClassElement::PropertyDefinition(property) => {
                if property.accessibility == Some(TSAccessibility::Private)
                    || matches!(property.key, PropertyKey::PrivateIdentifier(_))
                {
                    return;
                }
                let Some(key) = self.key(&property.key, property.computed, property.span.start)
                else {
                    return;
                };
                self.printer.anchor(property.span.start);
                self.context.push(key.clone());
                if property.r#static {
                    self.printer.text("static ");
                }
                if property.readonly
                    || (!property.r#static
                        && self.mentions_covariant(property.type_annotation.as_deref()))
                {
                    self.printer.text("readonly ");
                }
                self.printer.text(&key);
                if property.optional {
                    self.printer.char('?');
                }
                self.printer.text(": ");
                self.annotation(property.type_annotation.as_deref());
                self.printer.char(';');
                self.context.pop();
            }
            ClassElement::AccessorProperty(accessor) => {
                if accessor.accessibility == Some(TSAccessibility::Private)
                    || matches!(accessor.key, PropertyKey::PrivateIdentifier(_))
                {
                    return;
                }
                let Some(key) = self.key(&accessor.key, accessor.computed, accessor.span.start)
                else {
                    return;
                };
                self.printer.anchor(accessor.span.start);
                if accessor.r#static {
                    self.printer.text("static ");
                }
                self.printer.text(&key);
                self.printer.text(": ");
                self.annotation(accessor.type_annotation.as_deref());
                self.printer.char(';');
            }
            ClassElement::TSIndexSignature(index) => {
                if index.r#static {
                    self.hole(
                        Construct::StaticIndexSignature,
                        "a `static` index signature has no Flow spelling, so it is left out",
                        index.span.start,
                    );
                    return;
                }
                if self.keeps_indexer(index) && self.index_signature_keyable(index) {
                    self.printer.anchor(index.span.start);
                    self.index_signature(index);
                    self.printer.char(';');
                }
            }
            ClassElement::StaticBlock(_) => {}
        }
    }

    fn interface<'s, 'a>(
        &mut self,
        interface: &'s TSInterfaceDeclaration<'a>,
        scope: Scope,
        merges: &mut Merges<'s, 'a>,
        offset: u32,
        default: bool,
    ) {
        let name = interface.id.name.as_str();
        // Merged into the class of the same name, where it is printed — or
        // into the namespace of the same name, as its `$Self`.
        if merges.classes.contains(name) || merges.namespaces.contains_key(name) {
            return;
        }
        if !merges.printed.insert(name) {
            return;
        }
        let group = merges
            .interfaces
            .get(name)
            .cloned()
            .unwrap_or_else(|| SmallVec::from_slice(&[interface]));
        self.printer.anchor(offset);
        let exported = !default && self.exported(name, scope);
        let binding = self.type_binding(name);
        self.interface_group(name, &binding, &group, exported && binding == name);
        if exported {
            self.export_renamed(name, "export type");
        }
    }

    /// `interface Binding<…> extends … { … }`, with every declaration in
    /// `group` merged into it.
    fn interface_group(
        &mut self,
        name: &str,
        binding: &str,
        group: &[&TSInterfaceDeclaration<'_>],
        export: bool,
    ) {
        let Some(first) = group.first() else {
            return;
        };
        self.context.push(name.to_compact_string());
        self.printer.text(if export {
            "export interface "
        } else {
            "interface "
        });
        self.printer.text(binding);
        let scope_len = self.type_scope.len();
        let inferred = first
            .type_parameters
            .as_deref()
            .map_or_else(Vec::new, |parameters| {
                infer_variance(parameters, |uses| {
                    for declaration in group {
                        for heritage in &declaration.extends {
                            if let Some(arguments) = &heritage.type_arguments {
                                for argument in &arguments.params {
                                    uses.ty(argument, Polarity::Positive);
                                }
                            }
                        }
                        uses.members(&declaration.body.body, Polarity::Positive);
                    }
                })
            });
        self.type_parameters_with_variance(first.type_parameters.as_deref(), &inferred);
        let outer_variance = self.enter_variance(first.type_parameters.as_deref());
        let mut extended: SmallVec<[&oxc_ast::ast::TSInterfaceHeritage<'_>; 4]> = SmallVec::new();
        for declaration in group {
            extended.extend(declaration.extends.iter());
        }
        if !extended.is_empty() {
            self.printer.text(" extends ");
            for (index, heritage) in extended.iter().enumerate() {
                if index > 0 {
                    self.printer.text(", ");
                }
                self.type_name(&heritage.type_name);
                self.type_arguments(heritage.type_arguments.as_deref());
            }
        }
        self.printer.text(" {");
        self.printer.indent();
        let indexers: SmallVec<[&TSIndexSignature<'_>; 2]> = group
            .iter()
            .flat_map(|declaration| interface_indexers(&declaration.body.body))
            .collect();
        let outer_indexer = self.enter_indexers(&indexers);
        for declaration in group {
            for member in &declaration.body.body {
                self.signature_member(member, ';');
            }
        }
        self.indexer = outer_indexer;
        self.printer.dedent();
        let end = group.last().map_or(0, |last| last.body.span.end);
        self.printer.anchor(end.saturating_sub(1));
        self.printer.char('}');
        self.covariant = outer_variance;
        self.type_scope.truncate(scope_len);
        self.context.pop();
    }

    fn enumeration<'s, 'a>(
        &mut self,
        declaration: &'s TSEnumDeclaration<'a>,
        scope: Scope,
        merges: &mut Merges<'s, 'a>,
        offset: u32,
    ) {
        let name = declaration.id.name.as_str();
        if !merges.printed.insert(name) {
            return;
        }
        let group = merges
            .enums
            .get(name)
            .cloned()
            .unwrap_or_else(|| SmallVec::from_slice(&[declaration]));
        self.printer.anchor(offset);
        self.context.push(name.to_compact_string());
        let exported = self.exported(name, scope);
        match enum_members(&group) {
            Ok(members) => {
                self.printer.text(if exported {
                    "declare export enum "
                } else {
                    "declare enum "
                });
                self.printer.text(name);
                self.printer.text(" {");
                self.printer.indent();
                for member in &members {
                    self.printer.anchor(member.offset);
                    self.printer.text(&member.name);
                    self.printer.text(" = ");
                    match &member.value {
                        EnumValue::Number(number) => self.number(*number, None),
                        EnumValue::String(string) => self.string(string),
                    }
                    self.printer.char(',');
                }
                self.printer.dedent();
                let end = group
                    .last()
                    .map_or(declaration.span.end, |last| last.span.end);
                self.printer.anchor(end.saturating_sub(1));
                self.printer.char('}');
            }
            Err((reason, at)) => {
                self.hole(
                    Construct::EnumMember,
                    format!("{reason}; `{name}` is declared `any`"),
                    at,
                );
                self.printer.text(if exported {
                    "declare export var "
                } else {
                    "declare var "
                });
                self.printer.text(name);
                self.printer.text(": any;");
            }
        }
        self.context.pop();
    }

    fn namespace<'s, 'a>(
        &mut self,
        namespace: &'s TSNamespaceDeclaration<'a>,
        scope: Scope,
        merges: &mut Merges<'s, 'a>,
        offset: u32,
    ) {
        let name = namespace.id.name.as_str();
        // A namespace merged into a class or an enum carries statics Flow has
        // nowhere to put, and one merged into a function with values of its own
        // cannot be split into a type half and a value half. Asked before the
        // name counts as printed, because the class it merges with printed it.
        let split = self.split_here(name, scope);
        let kept = if merges.classes.contains(name) {
            Some("class")
        } else if merges.enums.contains_key(name) {
            Some("enum")
        } else if merges.functions.contains(name) && !split {
            Some("function")
        } else {
            None
        };
        if let Some(kept) = kept {
            if merges.dropped_namespaces.insert(name) {
                self.merged_hole(name, kept, "namespace", offset);
            }
            return;
        }
        if !merges.printed.insert(name) {
            return;
        }
        let group = merges
            .namespaces
            .get(name)
            .cloned()
            .unwrap_or_else(|| SmallVec::from_slice(&[namespace]));
        let interfaces = merges.interfaces.get(name).cloned().unwrap_or_default();
        if !interfaces.is_empty() {
            self.hole(
                Construct::MergedDeclaration,
                format!(
                    "`{name}` is declared as both an interface and a namespace, and Flow binds a \
                     name once; the namespace keeps the name and the interface is `{name}.$Self`, \
                     so a `{name}<…>` written outside this package names nothing"
                ),
                offset,
            );
        }
        self.printer.anchor(offset);
        self.namespace_body(name, &group, &interfaces, self.exported(name, scope));
    }

    fn namespace_body<'s, 'a>(
        &mut self,
        name: &str,
        group: &[&'s TSNamespaceDeclaration<'a>],
        interfaces: &[&'s TSInterfaceDeclaration<'a>],
        exported: bool,
    ) {
        self.context.push(name.to_compact_string());
        self.printer.text(if exported {
            "declare export namespace "
        } else {
            "declare namespace "
        });
        self.printer.text(name);
        self.printer.text(" {");
        self.printer.indent();
        // `interface StandardSchemaV1<I, O>` beside `namespace StandardSchemaV1`
        // is one name in TypeScript and two bindings in Flow. The namespace
        // keeps the name, because `StandardSchemaV1.Props` is how the package
        // reaches its members, and the interface moves inside it as `$Self`,
        // which is where a bare `StandardSchemaV1` in this file now points.
        if !interfaces.is_empty() {
            self.printer.char(' ');
            self.interface_group(name, "$Self", interfaces, false);
        }
        let mut statements: SmallVec<[&'s Statement<'a>; 32]> = SmallVec::new();
        let mut nested: SmallVec<[&'s TSNamespaceDeclaration<'a>; 2]> = SmallVec::new();
        for declaration in group {
            match &declaration.body {
                TSNamespaceDeclarationBody::TSModuleBlock(block) => {
                    statements.extend(block.body.iter());
                }
                TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => nested.push(inner),
            }
        }
        self.statements(&statements, Scope::Namespace);
        if let Some(first) = nested.first() {
            self.printer.anchor(first.span.start);
            let inner_name = first.id.name.as_str();
            self.namespace_body(inner_name, &nested, &[], false);
        }
        self.printer.dedent();
        let end = group.last().map_or(0, |last| last.span.end);
        self.printer.anchor(end.saturating_sub(1));
        self.printer.char('}');
        self.context.pop();
    }

    fn global(&mut self, global: &TSGlobalDeclaration<'_>) {
        for statement in &global.body.body {
            let declaration = match statement {
                Statement::ExportDeclaration(export) => &export.declaration,
                other => match other.as_declaration() {
                    Some(declaration) => declaration,
                    None => continue,
                },
            };
            let name = declaration_name(declaration).unwrap_or("(declaration)");
            self.context.push(name.to_compact_string());
            self.hole(
                Construct::GlobalAugmentation,
                format!(
                    "`declare global` adds `{name}` to every module's scope, and a Flow module \
                     declares only its own; it is not translated"
                ),
                statement.span().start,
            );
            self.context.pop();
        }
    }

    // ----------------------------------------------------------------------
    // Imports and exports
    // ----------------------------------------------------------------------

    fn plan(&self, local: &str, import: &Import) -> Plan {
        if import.imported == Imported::Namespace {
            return Plan::Namespace;
        }
        let used_as_value = self.summary.value_uses.contains(local);
        let kinds = self.import_kinds(import);
        if import.type_only {
            if !used_as_value {
                return Plan::Type;
            }
            // `import type { Class }` then `typeof Class`: TypeScript reads the
            // value's type through a type-only import, which Flow spells
            // `import typeof`.
            return match kinds {
                Some(kinds) if !kinds.value => Plan::Type,
                Some(kinds) if !kinds.type_ => Plan::Typeof,
                _ => Plan::TypeAndTypeof,
            };
        }
        match kinds {
            Some(kinds) if kinds.split => Plan::Split,
            Some(kinds) if kinds.value => Plan::Value,
            Some(_) => Plan::Type,
            None if used_as_value => Plan::Value,
            None => Plan::Type,
        }
    }

    fn import_kinds(&self, import: &Import) -> Option<Kinds> {
        let Target::Module(module) = &import.target else {
            return None;
        };
        match &import.imported {
            Imported::Namespace => Some(Kinds::BOTH),
            Imported::Named(name) => self.oracle.export_kinds(module, name),
        }
    }

    /// A specifier as the translation writes it: a relative one names the
    /// translated module.
    fn specifier(&mut self, written: &str, offset: u32) -> CompactString {
        match self.summary.targets.get(written) {
            Some(Target::Module(module)) => {
                relative_specifier(self.path, &flow_path(module)).into()
            }
            Some(Target::Missing(_)) => {
                if self.missing.insert(written.to_compact_string()) {
                    self.hole(
                        Construct::MissingFile,
                        format!(
                            "`{written}` names no declaration file this package ships, so \
                             everything imported from it is `any`"
                        ),
                        offset,
                    );
                }
                written.to_compact_string()
            }
            Some(Target::Package(_)) | None => written.to_compact_string(),
        }
    }

    fn import(&mut self, import: &ImportDeclaration<'_>) {
        let Some(specifiers) = &import.specifiers else {
            return;
        };
        if specifiers.is_empty() {
            return;
        }
        let written = import.source.value.as_str();
        let source = self.specifier(written, import.span.start);
        self.printer.anchor(import.span.start);
        if matches!(self.summary.targets.get(written), Some(Target::Missing(_))) {
            self.missing_import(specifiers);
            return;
        }
        let mut types: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
        let mut values: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
        let mut typeofs: SmallVec<[(CompactString, CompactString); 2]> = SmallVec::new();
        for specifier in specifiers {
            let (imported, local) = match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(named) => (
                    export_name(&named.imported),
                    named.local.name.to_compact_string(),
                ),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => (
                    CompactString::const_new("default"),
                    default.local.name.to_compact_string(),
                ),
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                    self.printer.text(&format!(
                        "import * as {} from {};",
                        namespace.local.name,
                        quoted(&source)
                    ));
                    continue;
                }
            };
            let plan = self.plans.get(&local).copied().unwrap_or(Plan::Type);
            let local = self.type_binding(&local);
            match plan {
                Plan::Value | Plan::Namespace => values.push((imported, local)),
                Plan::Type => types.push((imported, local)),
                Plan::Split => {
                    if self.summary.value_uses.contains(&local) {
                        values.push((imported.clone(), format_compact!("{local}$value")));
                    }
                    types.push((imported, local));
                }
                Plan::Typeof => typeofs.push((imported, format_compact!("{local}$typeof"))),
                Plan::TypeAndTypeof => {
                    typeofs.push((imported.clone(), format_compact!("{local}$typeof")));
                    types.push((imported, local));
                }
            }
        }
        self.import_group("import type", &types, &source);
        self.import_group("import", &values, &source);
        self.import_group("import typeof", &typeofs, &source);
    }

    /// What an import from a file the package does not ship binds: `any`,
    /// declared here, as the [`Construct::MissingFile`] hole already says.
    ///
    /// Declared rather than imported, because an import of a module that
    /// resolves to nothing binds an `any`-typed *value*, and every use of one as
    /// a type is an error of its own.
    fn missing_import(
        &mut self,
        specifiers: &oxc_allocator::Vec<'_, ImportDeclarationSpecifier<'_>>,
    ) {
        for specifier in specifiers {
            let local = specifier.local().name.as_str();
            let as_value =
                matches!(
                    specifier,
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(_)
                ) || matches!(self.plans.get(local), Some(Plan::Value | Plan::Namespace));
            self.printer.space();
            if as_value {
                self.printer.text(&format!("declare var {local}: any;"));
            } else {
                self.printer.text(&format!("type {local} = any;"));
            }
        }
    }

    fn import_group(
        &mut self,
        keyword: &str,
        names: &[(CompactString, CompactString)],
        source: &str,
    ) {
        if names.is_empty() {
            return;
        }
        self.printer.space();
        self.printer.text(keyword);
        self.printer.text(" { ");
        for (index, (imported, local)) in names.iter().enumerate() {
            if index > 0 {
                self.printer.text(", ");
            }
            self.printer.text(imported);
            if imported != local {
                self.printer.text(" as ");
                self.printer.text(local);
            }
        }
        self.printer.text(" } from ");
        self.printer.text(&quoted(source));
        self.printer.char(';');
    }

    fn import_equals(&mut self, import: &TSImportEqualsDeclaration<'_>, exported: bool) {
        let local = import.id.name.as_str();
        match &import.module_reference {
            TSModuleReference::ExternalModuleReference(reference) => {
                let written = reference.expression.value.as_str();
                let source = self.specifier(written, import.span.start);
                self.printer.anchor(import.span.start);
                if matches!(self.summary.targets.get(written), Some(Target::Missing(_))) {
                    self.printer.text(&format!("declare var {local}: any;"));
                    return;
                }
                // `import x = require("m")` binds what `m` assigns to
                // `module.exports`. Flow's default import of a CommonJS module
                // is exactly that; of an ES module it is the default export, so
                // an ES target is imported whole instead.
                let es_target = matches!(
                    self.summary.targets.get(written),
                    Some(Target::Module(module)) if !self.oracle.assigns_exports(module)
                );
                if es_target {
                    self.printer
                        .text(&format!("import * as {local} from {};", quoted(&source)));
                } else {
                    self.printer
                        .text(&format!("import {local} from {};", quoted(&source)));
                }
                if exported {
                    self.printer.text(&format!(" export {{ {local} }};"));
                }
            }
            TSModuleReference::IdentifierReference(_) | TSModuleReference::QualifiedName(_) => {
                self.context.push(local.to_compact_string());
                self.hole(
                    Construct::ImportAlias,
                    format!(
                        "`import {local} = …` aliases a namespace member, which a Flow module \
                         cannot import; every use of `{local}` is `any`"
                    ),
                    import.span.start,
                );
                self.context.pop();
            }
        }
    }

    /// The kinds of a name in this file's top-level scope, when they are known.
    fn local_kinds(&self, local: &str) -> Option<Kinds> {
        if let Some(declared) = self.summary.locals.get(local) {
            return Some(declared.kinds());
        }
        let import = self.summary.imports.get(local)?;
        match self.plans.get(local).copied()? {
            Plan::Namespace => Some(Kinds::BOTH),
            Plan::Split => Some(Kinds {
                type_: true,
                value: true,
                split: true,
                self_interface: false,
            }),
            Plan::Type | Plan::Typeof | Plan::TypeAndTypeof => Some(Kinds::TYPE),
            Plan::Value => Some(self.import_kinds(import).unwrap_or(Kinds::VALUE)),
        }
    }

    fn export_list(&mut self, list: &ExportNamedDeclaration<'_>) {
        if list.specifiers.is_empty() {
            return;
        }
        self.printer.anchor(list.span.start);
        let mut types: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
        let mut values: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
        for specifier in &list.specifiers {
            let local = export_name(&specifier.local);
            let exported = export_name(&specifier.exported);
            let binding = self.type_binding(&local);
            if list.export_kind.is_type() || specifier.export_kind.is_type() {
                types.push((binding, exported));
                continue;
            }
            match self.local_kinds(&local) {
                Some(kinds) if kinds.split => {
                    types.push((binding, exported.clone()));
                    values.push((format_compact!("{local}$value"), exported));
                }
                Some(kinds) if kinds.value => values.push((binding, exported)),
                Some(_) => types.push((binding, exported)),
                None => values.push((binding, exported)),
            }
        }
        self.export_group("export type", &types, None);
        self.export_group("export", &values, None);
    }

    fn export_from(&mut self, from: &ExportFromDeclaration<'_>) {
        let written = from.source.value.as_str();
        let source = self.specifier(written, from.span.start);
        let target = self.summary.targets.get(written).cloned();
        // The hole is reported; re-exporting from nothing would be an error
        // about a name that was never going to exist.
        if matches!(target, Some(Target::Missing(_))) {
            return;
        }
        self.printer.anchor(from.span.start);
        let mut types: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
        let mut values: SmallVec<[(CompactString, CompactString); 8]> = SmallVec::new();
        for specifier in &from.specifiers {
            let imported = export_name(&specifier.local);
            let exported = export_name(&specifier.exported);
            if from.export_kind.is_type() || specifier.export_kind.is_type() {
                types.push((imported, exported));
                continue;
            }
            let kinds = match &target {
                Some(Target::Module(module)) => self.oracle.export_kinds(module, &imported),
                _ => None,
            };
            match kinds {
                Some(kinds) if kinds.split => {
                    types.push((imported.clone(), exported.clone()));
                    values.push((imported, exported));
                }
                Some(kinds) if kinds.value => values.push((imported, exported)),
                Some(_) => types.push((imported, exported)),
                None => values.push((imported, exported)),
            }
        }
        self.export_group("export type", &types, Some(&source));
        self.export_group("export", &values, Some(&source));
    }

    fn export_group(
        &mut self,
        keyword: &str,
        names: &[(CompactString, CompactString)],
        source: Option<&str>,
    ) {
        if names.is_empty() {
            return;
        }
        self.printer.space();
        self.printer.text(keyword);
        self.printer.text(" { ");
        for (index, (local, exported)) in names.iter().enumerate() {
            if index > 0 {
                self.printer.text(", ");
            }
            self.printer.text(local);
            if local != exported {
                self.printer.text(" as ");
                self.printer.text(exported);
            }
        }
        self.printer.text(" }");
        if let Some(source) = source {
            self.printer.text(" from ");
            self.printer.text(&quoted(source));
        }
        self.printer.char(';');
    }

    fn export_all(&mut self, all: &ExportAllDeclaration<'_>) {
        let written = all.source.value.as_str();
        let source = self.specifier(written, all.span.start);
        if matches!(self.summary.targets.get(written), Some(Target::Missing(_))) {
            return;
        }
        self.printer.anchor(all.span.start);
        self.printer.text(if all.export_kind.is_type() {
            "export type * "
        } else {
            "export * "
        });
        if let Some(exported) = &all.exported {
            self.printer.text("as ");
            self.printer.text(&export_name(exported));
            self.printer.char(' ');
        }
        self.printer.text("from ");
        self.printer.text(&quoted(&source));
        self.printer.char(';');
    }

    fn export_default<'s, 'a>(
        &mut self,
        default: &'s ExportDefaultDeclaration<'a>,
        merges: &mut Merges<'s, 'a>,
    ) {
        let offset = default.span.start;
        match &default.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                self.printer.anchor(offset);
                self.context.push(CompactString::const_new("default"));
                match &function.id {
                    Some(id) => {
                        self.printer.text("declare export default function ");
                        self.printer.text(id.name.as_str());
                        self.signature(
                            function.type_parameters.as_deref(),
                            function.this_param.as_deref(),
                            &function.params,
                            Return::Annotation(function.return_type.as_deref()),
                            false,
                        );
                    }
                    None => {
                        // Flow's parser wants a name after `declare export
                        // default function`; a function type says the same.
                        self.printer.text("declare export default ");
                        self.signature(
                            function.type_parameters.as_deref(),
                            function.this_param.as_deref(),
                            &function.params,
                            Return::Annotation(function.return_type.as_deref()),
                            true,
                        );
                    }
                }
                self.printer.char(';');
                self.context.pop();
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                self.class(class, Scope::Module, merges, offset, ClassExport::Default);
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                self.interface(interface, Scope::Module, merges, offset, true);
                self.context.push(interface.id.name.to_compact_string());
                self.default_type_hole(offset);
                self.context.pop();
            }
            expression => {
                let Some(Expression::Identifier(identifier)) = expression.as_expression() else {
                    self.context.push(CompactString::const_new("default"));
                    self.hole(
                        Construct::ExportAssignment,
                        "`export default` of an expression that is not a name has no Flow \
                         spelling, so the default export is `any`",
                        offset,
                    );
                    self.context.pop();
                    return;
                };
                let name = identifier.name.as_str();
                match self.local_kinds(name) {
                    Some(kinds) if !kinds.value => {
                        self.context.push(name.to_compact_string());
                        self.default_type_hole(offset);
                        self.context.pop();
                    }
                    _ => {
                        self.printer.anchor(offset);
                        self.printer.text("export default ");
                        let spelled = match self.values.get(name) {
                            Some(Value::Renamed(renamed)) => renamed.clone(),
                            _ => name.to_compact_string(),
                        };
                        self.printer.text(&spelled);
                        self.printer.char(';');
                    }
                }
            }
        }
    }

    fn default_type_hole(&mut self, offset: u32) {
        self.hole(
            Construct::DefaultExportType,
            "a default export that is only a type has no Flow spelling, so `import type X from` \
             this module finds nothing",
            offset,
        );
    }

    fn export_assignment<'s, 'a>(
        &mut self,
        assignment: &'s TSExportAssignment<'a>,
        merges: &Merges<'s, 'a>,
    ) {
        let offset = assignment.span.start;
        self.printer.anchor(offset);
        let Expression::Identifier(identifier) = &assignment.expression else {
            self.context.push(CompactString::const_new("export ="));
            self.hole(
                Construct::ExportAssignment,
                "`export =` of an expression that is not a name has no Flow spelling, so \
                 `module.exports` is `any`",
                offset,
            );
            self.context.pop();
            self.printer.text("declare module.exports: any;");
            return;
        };
        let name = identifier.name.as_str();
        let spelled = match self.values.get(name) {
            Some(Value::Renamed(renamed)) => renamed.clone(),
            _ => name.to_compact_string(),
        };
        self.printer
            .text(&format!("declare module.exports: typeof {spelled};"));
        // TypeScript lets a consumer `import type { Options } from "m"` when
        // `m` assigns a namespace holding `Options`. A Flow CommonJS module
        // exports types beside `module.exports`, so each one is re-declared.
        let Some(namespaces) = merges.namespaces.get(name) else {
            return;
        };
        for namespace in namespaces {
            let TSNamespaceDeclarationBody::TSModuleBlock(block) = &namespace.body else {
                continue;
            };
            for statement in &block.body {
                let declaration = match statement {
                    Statement::ExportDeclaration(export) => &export.declaration,
                    other => match other.as_declaration() {
                        Some(declaration) => declaration,
                        None => continue,
                    },
                };
                let (member, parameters) = match declaration {
                    Declaration::TSInterfaceDeclaration(interface) => (
                        interface.id.name.as_str(),
                        interface.type_parameters.as_deref(),
                    ),
                    Declaration::TSTypeAliasDeclaration(alias) => {
                        (alias.id.name.as_str(), alias.type_parameters.as_deref())
                    }
                    Declaration::ClassDeclaration(class) => match &class.id {
                        Some(id) => (id.name.as_str(), class.type_parameters.as_deref()),
                        None => continue,
                    },
                    _ => continue,
                };
                self.printer.text(" export type ");
                self.printer.text(member);
                let scope_len = self.type_scope.len();
                self.type_parameters(parameters);
                self.printer.text(&format!(" = {name}.{member}"));
                if let Some(parameters) = parameters
                    && !parameters.params.is_empty()
                {
                    self.printer.char('<');
                    for (index, parameter) in parameters.params.iter().enumerate() {
                        if index > 0 {
                            self.printer.text(", ");
                        }
                        self.printer.text(parameter.name.name.as_str());
                    }
                    self.printer.char('>');
                }
                self.printer.char(';');
                self.type_scope.truncate(scope_len);
            }
        }
    }

    /// The modules `import("…")` types named, imported on the first line.
    ///
    /// On the first line rather than the last because `typeof import("m")` is
    /// a use of the value the import binds, and Flow reports a value used
    /// before the statement that declares it. The first line rather than a
    /// new one above it, so every other line stays where the source had it.
    fn hoisted_imports(&mut self) {
        let hoisted = std::mem::take(&mut self.hoisted);
        if hoisted.is_empty() {
            return;
        }
        let mut text = String::new();
        for (specifier, local) in hoisted {
            text.push_str(&format!(
                "import * as {local} from {}; ",
                quoted(&specifier)
            ));
        }
        self.printer.prepend(&text);
    }

    // ----------------------------------------------------------------------
    // Signatures and members
    // ----------------------------------------------------------------------

    fn signature(
        &mut self,
        type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
        this_param: Option<&TSThisParameter<'_>>,
        params: &FormalParameters<'_>,
        returns: Return<'_, '_>,
        arrow: bool,
    ) {
        let scope_len = self.type_scope.len();
        self.type_parameters(type_parameters);
        self.printer.char('(');
        let mut first = true;
        if let Some(this) = this_param {
            self.printer.text("this: ");
            self.annotation(this.type_annotation.as_deref());
            first = false;
        }
        for (index, param) in params.items.iter().enumerate() {
            if !first {
                self.printer.text(", ");
            }
            first = false;
            let name = binding_name(&param.pattern, index);
            self.printer.text(&name);
            if param.optional || param.initializer.is_some() {
                self.printer.char('?');
            }
            self.printer.text(": ");
            self.annotation(param.type_annotation.as_deref());
        }
        if let Some(rest) = &params.rest {
            if !first {
                self.printer.text(", ");
            }
            self.printer.text("...");
            let name = binding_name(&rest.rest.argument, params.items.len());
            self.printer.text(&name);
            self.printer.text(": ");
            self.annotation(rest.type_annotation.as_deref());
        }
        self.printer.char(')');
        self.printer.text(if arrow { " => " } else { ": " });
        match returns {
            Return::Void => self.printer.text("void"),
            Return::Annotation(annotation) => self.return_type(annotation, arrow),
        }
        self.type_scope.truncate(scope_len);
    }

    fn return_type(&mut self, annotation: Option<&TSTypeAnnotation<'_>>, arrow: bool) {
        let Some(annotation) = annotation else {
            self.printer.text("any");
            return;
        };
        if let TSType::TSTypePredicate(predicate) = &annotation.type_annotation {
            if predicate.asserts {
                self.hole(
                    Construct::AssertionSignature,
                    "Flow has no assertion signatures, so this returns `void` and narrows \
                     nothing after a call",
                    predicate.span.start,
                );
                self.printer.text("void");
                return;
            }
            let subject = match &predicate.parameter_name {
                TSTypePredicateName::Identifier(identifier) => identifier.name.as_str(),
                TSTypePredicateName::This(_) => "this",
            };
            match &predicate.type_annotation {
                Some(guarded) => {
                    self.printer.text(subject);
                    self.printer.text(" is ");
                    self.ty(&guarded.type_annotation, Prec::Prefix);
                }
                None => self.printer.text("boolean"),
            }
            return;
        }
        self.ty(
            &annotation.type_annotation,
            if arrow { Prec::Function } else { Prec::Any },
        );
    }

    fn annotation(&mut self, annotation: Option<&TSTypeAnnotation<'_>>) {
        match annotation {
            Some(annotation) => self.ty(&annotation.type_annotation, Prec::Any),
            None => self.printer.text("any"),
        }
    }

    fn type_parameters(&mut self, declaration: Option<&TSTypeParameterDeclaration<'_>>) {
        self.type_parameters_with_variance(declaration, &[]);
    }

    /// Type parameters, with the variance `inferred` gives a parameter that
    /// TypeScript did not annotate. See [`infer_variance`].
    fn type_parameters_with_variance(
        &mut self,
        declaration: Option<&TSTypeParameterDeclaration<'_>>,
        inferred: &[Option<&'static str>],
    ) {
        let Some(declaration) = declaration else {
            return;
        };
        if declaration.params.is_empty() {
            return;
        }
        for parameter in &declaration.params {
            self.type_scope
                .push(parameter.name.name.to_compact_string());
        }
        self.printer.char('<');
        for (index, parameter) in declaration.params.iter().enumerate() {
            if index > 0 {
                self.printer.text(", ");
            }
            if parameter.r#const {
                self.printer.text("const ");
            }
            if parameter.r#in {
                self.printer.text("in ");
            }
            if parameter.out {
                self.printer.text("out ");
            }
            if !parameter.r#in
                && !parameter.out
                && let Some(Some(variance)) = inferred.get(index)
            {
                self.printer.text(variance);
                self.printer.char(' ');
            }
            self.printer.text(parameter.name.name.as_str());
            if let Some(constraint) = &parameter.constraint {
                self.printer.text(" extends ");
                self.ty(constraint, Prec::Union);
            }
            if let Some(default) = &parameter.default {
                self.printer.text(" = ");
                self.ty(default, Prec::Union);
            }
        }
        self.printer.char('>');
    }

    fn type_arguments(&mut self, arguments: Option<&TSTypeParameterInstantiation<'_>>) {
        let Some(arguments) = arguments else {
            return;
        };
        self.printer.char('<');
        for (index, argument) in arguments.params.iter().enumerate() {
            if index > 0 {
                self.printer.text(", ");
            }
            self.ty(argument, Prec::Any);
        }
        self.printer.char('>');
    }

    /// A member key as Flow spells it, or [`None`] — with a hole — when Flow
    /// has no spelling for it.
    fn key(&mut self, key: &PropertyKey<'_>, computed: bool, offset: u32) -> Option<CompactString> {
        if matches!(key, PropertyKey::PrivateIdentifier(_)) {
            return None;
        }
        match key_text(key, computed) {
            Some(key) => Some(key),
            None => {
                self.computed_key_hole(offset);
                None
            }
        }
    }

    fn computed_key_hole(&mut self, offset: u32) {
        self.hole(
            Construct::ComputedKey,
            "a computed key other than `Symbol.iterator` or `Symbol.asyncIterator` has no Flow \
             spelling, so the member is left out",
            offset,
        );
    }

    fn signature_member(&mut self, member: &TSSignature<'_>, separator: char) {
        match member {
            TSSignature::TSPropertySignature(property) => {
                let Some(key) = self.key(&property.key, property.computed, property.span.start)
                else {
                    return;
                };
                self.printer.anchor(property.span.start);
                self.context.push(key.clone());
                // Every property of an interface or an object type is read
                // covariantly by TypeScript, `readonly` or not: a
                // `{ format: "date" }` is a `{ format: string }` there. Flow
                // reads a writable property invariantly, so without this a
                // subtype TypeScript accepts in every call is refused. What it
                // costs is a write through a type the package declared, which
                // TypeScript's own variance was already unsound about.
                self.printer.text("readonly ");
                self.printer.text(&key);
                if property.optional {
                    self.printer.char('?');
                }
                self.printer.text(": ");
                self.annotation(property.type_annotation.as_deref());
                self.printer.char(separator);
                self.context.pop();
            }
            TSSignature::TSMethodSignature(method) => {
                let Some(key) = self.key(&method.key, method.computed, method.span.start) else {
                    return;
                };
                self.printer.anchor(method.span.start);
                self.context.push(key.clone());
                match method.kind {
                    TSMethodSignatureKind::Get => {
                        self.printer.text("get ");
                        self.printer.text(&key);
                        self.printer.text("(): ");
                        self.return_type(method.return_type.as_deref(), false);
                    }
                    TSMethodSignatureKind::Set => {
                        self.printer.text("set ");
                        self.printer.text(&key);
                        self.signature(None, None, &method.params, Return::Void, false);
                    }
                    TSMethodSignatureKind::Method => {
                        self.printer.text(&key);
                        if method.optional {
                            self.printer.text("?: ");
                        }
                        self.signature(
                            method.type_parameters.as_deref(),
                            method.this_param.as_deref(),
                            &method.params,
                            Return::Annotation(method.return_type.as_deref()),
                            method.optional,
                        );
                    }
                }
                self.printer.char(separator);
                self.context.pop();
            }
            TSSignature::TSCallSignatureDeclaration(call) => {
                self.printer.anchor(call.span.start);
                self.signature(
                    call.type_parameters.as_deref(),
                    call.this_param.as_deref(),
                    &call.params,
                    Return::Annotation(call.return_type.as_deref()),
                    false,
                );
                self.printer.char(separator);
            }
            TSSignature::TSConstructSignatureDeclaration(construct) => {
                self.printer.anchor(construct.span.start);
                self.printer.text("new ");
                self.signature(
                    construct.type_parameters.as_deref(),
                    None,
                    &construct.params,
                    Return::Annotation(construct.return_type.as_deref()),
                    false,
                );
                self.printer.char(separator);
            }
            TSSignature::TSIndexSignature(index) => {
                if self.keeps_indexer(index) && self.index_signature_keyable(index) {
                    self.printer.anchor(index.span.start);
                    self.index_signature(index);
                    self.printer.char(separator);
                }
            }
        }
    }

    /// Whether an index signature's key is one Flow can key by; when it is
    /// not, the hole is recorded here and nothing should be printed.
    fn index_signature_keyable(&mut self, index: &TSIndexSignature<'_>) -> bool {
        let key = &index.parameter.type_annotation.type_annotation;
        let supported = match key {
            TSType::TSStringKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSSymbolKeyword(_) => true,
            TSType::TSUnionType(union) => union.types.iter().all(|member| {
                matches!(
                    member,
                    TSType::TSStringKeyword(_)
                        | TSType::TSNumberKeyword(_)
                        | TSType::TSSymbolKeyword(_)
                )
            }),
            _ => false,
        };
        if !supported {
            self.hole(
                Construct::IndexSignatureKey,
                "Flow keys an index signature by `string`, `number` or `symbol`, so this one is \
                 left out",
                index.span.start,
            );
        }
        supported
    }

    /// An index signature whose key [`Self::index_signature_keyable`] accepted.
    fn index_signature(&mut self, index: &TSIndexSignature<'_>) {
        let key = &index.parameter.type_annotation.type_annotation;
        if index.readonly {
            self.printer.text("readonly ");
        }
        self.printer.char('[');
        self.printer.text(index.parameter.name.as_str());
        self.printer.text(": ");
        self.ty(key, Prec::Any);
        self.printer.text("]: ");
        self.ty(&index.type_annotation.type_annotation, Prec::Any);
    }

    // ----------------------------------------------------------------------
    // Types
    // ----------------------------------------------------------------------

    fn ty(&mut self, ty: &TSType<'_>, context: Prec) {
        match ty {
            TSType::TSParenthesizedType(parenthesized) => {
                self.ty(&parenthesized.type_annotation, context);
            }
            TSType::TSAnyKeyword(_) => self.printer.text("any"),
            TSType::TSUnknownKeyword(_) => self.printer.text("mixed"),
            TSType::TSNeverKeyword(_) => self.printer.text("empty"),
            TSType::TSUndefinedKeyword(_) | TSType::TSVoidKeyword(_) => self.printer.text("void"),
            TSType::TSNullKeyword(_) => self.printer.text("null"),
            TSType::TSBooleanKeyword(_) => self.printer.text("boolean"),
            TSType::TSNumberKeyword(_) => self.printer.text("number"),
            TSType::TSStringKeyword(_) => self.printer.text("string"),
            TSType::TSBigIntKeyword(_) => self.printer.text("bigint"),
            TSType::TSSymbolKeyword(_) => self.printer.text("symbol"),
            TSType::TSObjectKeyword(_) => self.printer.text("interface {}"),
            TSType::TSThisType(_) => self.printer.text("this"),
            TSType::TSIntrinsicKeyword(keyword) => {
                self.hole(
                    Construct::IntrinsicType,
                    "`intrinsic` names a type only TypeScript's compiler implements, so it is \
                     `any`",
                    keyword.span.start,
                );
                self.printer.text("any");
            }
            TSType::JSDocNullableType(jsdoc) => self.jsdoc_hole(jsdoc.span.start),
            TSType::JSDocNonNullableType(jsdoc) => self.jsdoc_hole(jsdoc.span.start),
            TSType::JSDocUnknownType(jsdoc) => self.jsdoc_hole(jsdoc.span.start),
            TSType::TSUnionType(union) => {
                if union.types.len() == 1 {
                    self.ty(&union.types[0], context);
                    return;
                }
                let wrap = context > Prec::Union;
                self.open(wrap);
                for (index, member) in union.types.iter().enumerate() {
                    if index > 0 {
                        self.printer.text(" | ");
                    }
                    self.ty(member, Prec::Intersection);
                }
                self.close(wrap);
            }
            TSType::TSIntersectionType(intersection) => self.intersection(intersection, context),
            TSType::TSConditionalType(conditional) => self.conditional(conditional, context),
            TSType::TSFunctionType(function) => {
                let wrap = context > Prec::Function;
                self.open(wrap);
                self.signature(
                    function.type_parameters.as_deref(),
                    function.this_param.as_deref(),
                    &function.params,
                    Return::Annotation(Some(&function.return_type)),
                    true,
                );
                self.close(wrap);
            }
            TSType::TSConstructorType(constructor) => {
                let wrap = context > Prec::Function;
                self.open(wrap);
                if constructor.r#abstract {
                    self.printer.text("abstract ");
                }
                self.printer.text("new ");
                self.signature(
                    constructor.type_parameters.as_deref(),
                    None,
                    &constructor.params,
                    Return::Annotation(Some(&constructor.return_type)),
                    true,
                );
                self.close(wrap);
            }
            TSType::TSTypeOperatorType(operator) => match operator.operator {
                TSTypeOperatorOperator::Keyof => {
                    let wrap = context > Prec::Prefix;
                    self.open(wrap);
                    self.printer.text("keyof ");
                    self.ty(&operator.type_annotation, Prec::Prefix);
                    self.close(wrap);
                }
                TSTypeOperatorOperator::Unique => {
                    self.printer.text(if self.unique_symbol {
                        "unique symbol"
                    } else {
                        "symbol"
                    });
                }
                TSTypeOperatorOperator::Readonly => match &operator.type_annotation {
                    TSType::TSArrayType(array) => {
                        self.printer.text("$ReadOnlyArray<");
                        self.ty(&array.element_type, Prec::Any);
                        self.printer.char('>');
                    }
                    other => {
                        self.printer.text("$ReadOnly<");
                        self.ty(other, Prec::Any);
                        self.printer.char('>');
                    }
                },
            },
            TSType::TSArrayType(array) => {
                self.printer.text("Array<");
                self.ty(&array.element_type, Prec::Any);
                self.printer.char('>');
            }
            TSType::TSIndexedAccessType(indexed) => {
                self.ty(&indexed.object_type, Prec::Postfix);
                self.printer.char('[');
                self.ty(&indexed.index_type, Prec::Any);
                self.printer.char(']');
            }
            TSType::TSTupleType(tuple) => self.tuple(tuple),
            TSType::TSNamedTupleMember(member) => self.named_tuple_member(member, false),
            TSType::TSTypeLiteral(literal) => self.type_literal(literal),
            TSType::TSMappedType(mapped) => self.mapped(mapped),
            TSType::TSTypeReference(reference) => self.reference(reference, context),
            TSType::TSTypeQuery(query) => {
                // `(typeof x)["k"]` needs its parentheses in Flow as much as in
                // TypeScript: without them the index binds to `x`.
                let wrap = context > Prec::Prefix;
                self.open(wrap);
                self.type_query(query);
                self.close(wrap);
            }
            TSType::TSImportType(import) => self.import_type(import),
            TSType::TSInferType(infer) => {
                let wrap = context > Prec::Prefix;
                self.open(wrap);
                self.printer.text("infer ");
                self.printer.text(infer.type_parameter.name.name.as_str());
                if let Some(constraint) = &infer.type_parameter.constraint {
                    self.printer.text(" extends ");
                    self.ty(constraint, Prec::Union);
                }
                self.close(wrap);
            }
            TSType::TSLiteralType(literal) => self.literal(&literal.literal),
            TSType::TSTemplateLiteralType(template) => {
                self.printer.char('`');
                for (index, quasi) in template.quasis.iter().enumerate() {
                    self.printer.text(quasi.value.raw.as_str());
                    if let Some(interpolated) = template.types.get(index) {
                        self.printer.text("${");
                        self.ty(interpolated, Prec::Any);
                        self.printer.char('}');
                    }
                }
                self.printer.char('`');
            }
            // Only meaningful where a return type is printed, which handles it
            // before getting here.
            TSType::TSTypePredicate(_) => self.printer.text("boolean"),
        }
    }

    fn open(&mut self, wrap: bool) {
        if wrap {
            self.printer.char('(');
        }
    }

    fn close(&mut self, wrap: bool) {
        if wrap {
            self.printer.char(')');
        }
    }

    fn jsdoc_hole(&mut self, offset: u32) {
        self.hole(
            Construct::JsdocType,
            "a JSDoc type is not something a declaration file can hold, so it is `any`",
            offset,
        );
        self.printer.text("any");
    }

    fn intersection(&mut self, intersection: &TSIntersectionType<'_>, context: Prec) {
        let mut kept: SmallVec<[&TSType<'_>; 4]> = SmallVec::new();
        let mut non_nullish = false;
        for member in &intersection.types {
            match unparenthesized(member) {
                // `T & {}` is TypeScript's spelling of "T without null or
                // undefined", which is what `$NonMaybeType` is.
                TSType::TSTypeLiteral(literal) if literal.members.is_empty() => non_nullish = true,
                TSType::TSTypeReference(reference) if self.is_global(reference, "ThisType") => {}
                _ => kept.push(member),
            }
        }
        if non_nullish {
            self.printer.text("$NonMaybeType<");
            self.intersection_members(&kept, Prec::Any);
            self.printer.char('>');
        } else {
            self.intersection_members(&kept, context);
        }
    }

    fn intersection_members(&mut self, members: &[&TSType<'_>], context: Prec) {
        match members {
            [] => self.printer.text("mixed"),
            [only] => self.ty(only, context),
            members => {
                let wrap = context > Prec::Intersection;
                self.open(wrap);
                for (index, member) in members.iter().enumerate() {
                    if index > 0 {
                        self.printer.text(" & ");
                    }
                    self.ty(member, Prec::Prefix);
                }
                self.close(wrap);
            }
        }
    }

    fn conditional(&mut self, conditional: &TSConditionalType<'_>, context: Prec) {
        let wrap = context > Prec::Any;
        self.open(wrap);
        self.ty(&conditional.check_type, Prec::Union);
        self.printer.text(" extends ");
        let scope_len = self.type_scope.len();
        self.ty(&conditional.extends_type, Prec::Function);
        let mut inferred = InferNames::default();
        inferred.visit_ts_type(&conditional.extends_type);
        self.type_scope.extend(inferred.names);
        self.printer.text(" ? ");
        self.ty(&conditional.true_type, Prec::Any);
        self.type_scope.truncate(scope_len);
        self.printer.text(" : ");
        self.ty(&conditional.false_type, Prec::Any);
        self.close(wrap);
    }

    fn tuple(&mut self, tuple: &TSTupleType<'_>) {
        let array_rest = tuple.element_types.iter().find(|element| {
            let rest = match element {
                TSTupleElement::TSRestType(rest) => Some(rest_inner(&rest.type_annotation)),
                TSTupleElement::TSNamedTupleMember(named) => match &named.element_type {
                    TSTupleElement::TSRestType(rest) => Some(rest_inner(&rest.type_annotation)),
                    _ => None,
                },
                _ => None,
            };
            rest.is_some_and(|rest| self.is_array(rest))
        });
        if let Some(rest) = array_rest {
            self.hole(
                Construct::ArrayRestInTuple,
                "Flow spreads only a tuple into a tuple, so a tuple type with an array rest is \
                 `any`",
                rest.span().start,
            );
            self.printer.text("any");
            return;
        }
        self.printer.char('[');
        for (index, element) in tuple.element_types.iter().enumerate() {
            if index > 0 {
                self.printer.text(", ");
            }
            self.tuple_element(element);
        }
        self.printer.char(']');
    }

    fn is_array(&self, ty: &TSType<'_>) -> bool {
        match unparenthesized(ty) {
            TSType::TSArrayType(_) => true,
            TSType::TSTypeOperatorType(operator) => {
                operator.operator == TSTypeOperatorOperator::Readonly
                    && matches!(operator.type_annotation, TSType::TSArrayType(_))
            }
            TSType::TSTypeReference(reference) => {
                self.is_global(reference, "Array") || self.is_global(reference, "ReadonlyArray")
            }
            _ => false,
        }
    }

    fn tuple_element(&mut self, element: &TSTupleElement<'_>) {
        match element {
            TSTupleElement::TSOptionalType(optional) => {
                self.ty(&optional.type_annotation, Prec::Postfix);
                self.printer.char('?');
            }
            TSTupleElement::TSRestType(rest) => match unparenthesized(&rest.type_annotation) {
                // `...tail: T` is parsed as a rest of a labelled member.
                TSType::TSNamedTupleMember(named) => self.named_tuple_member(named, true),
                inner => {
                    self.printer.text("...");
                    self.ty(inner, Prec::Prefix);
                }
            },
            TSTupleElement::TSNamedTupleMember(named) => self.named_tuple_member(named, false),
            other => self.ty(other.to_ts_type(), Prec::Any),
        }
    }

    /// `label: T`, `label?: T` or `...label: T`.
    fn named_tuple_member(&mut self, named: &oxc_ast::ast::TSNamedTupleMember<'_>, rest: bool) {
        let (element, spread, optional) = match &named.element_type {
            TSTupleElement::TSRestType(inner) => (&inner.type_annotation, true, false),
            TSTupleElement::TSOptionalType(inner) => (&inner.type_annotation, false, true),
            other => (other.to_ts_type(), false, named.optional),
        };
        if rest || spread {
            self.printer.text("...");
        }
        self.printer.text(named.label.name.as_str());
        if optional {
            self.printer.char('?');
        }
        self.printer.text(": ");
        self.ty(element, Prec::Any);
    }

    /// A TypeScript object type, as a Flow inline interface.
    ///
    /// Not as a Flow object type, though that is the closer spelling. An
    /// object type in Flow accepts object literals and nothing else: neither a
    /// class instance nor a value typed by an interface is its subtype. So
    /// zod's `type SomeType = { _zod: … }`, the bound every schema function is
    /// declared against, would refuse every schema, all of which are
    /// interfaces — and a TypeScript object type accepts all three. An inline
    /// interface is structural over all three, is inexact as TypeScript's
    /// object types are, can be extended by an interface where an object type
    /// alias cannot, and is the only form Flow constructs through.
    fn type_literal(&mut self, literal: &TSTypeLiteral<'_>) {
        if literal.members.is_empty() {
            self.printer.text("$NonMaybeType<mixed>");
            return;
        }
        self.printer.text("interface {");
        self.printer.indent();
        let indexers = interface_indexers(&literal.members);
        let outer_indexer = self.enter_indexers(&indexers);
        for member in &literal.members {
            self.signature_member(member, ';');
        }
        self.indexer = outer_indexer;
        self.printer.dedent();
        self.printer.anchor(literal.span.end.saturating_sub(1));
        self.printer.char('}');
    }

    fn mapped(&mut self, mapped: &TSMappedType<'_>) {
        self.printer.text("{ ");
        match mapped.readonly {
            Some(TSMappedTypeModifierOperator::True) => self.printer.text("readonly "),
            Some(TSMappedTypeModifierOperator::Plus) => self.printer.text("+readonly "),
            Some(TSMappedTypeModifierOperator::Minus) => self.printer.text("-readonly "),
            None => {}
        }
        self.printer.char('[');
        let key = mapped.key.name.as_str();
        self.printer.text(key);
        self.printer.text(" in ");
        self.ty(&mapped.constraint, Prec::Any);
        let scope_len = self.type_scope.len();
        self.type_scope.push(key.to_compact_string());
        if let Some(name_type) = &mapped.name_type {
            self.printer.text(" as ");
            self.ty(name_type, Prec::Any);
        }
        self.printer.char(']');
        match mapped.optional {
            Some(TSMappedTypeModifierOperator::True) => self.printer.char('?'),
            Some(TSMappedTypeModifierOperator::Plus) => self.printer.text("+?"),
            Some(TSMappedTypeModifierOperator::Minus) => self.printer.text("-?"),
            None => {}
        }
        self.printer.text(": ");
        match &mapped.type_annotation {
            Some(value) => self.ty(value, Prec::Any),
            None => self.printer.text("any"),
        }
        self.type_scope.truncate(scope_len);
        self.printer.text(" }");
    }

    /// Whether `reference` names the global `name` rather than something this
    /// file binds.
    fn is_global(&self, reference: &TSTypeReference<'_>, name: &str) -> bool {
        matches!(&reference.type_name, TSTypeName::IdentifierReference(identifier)
            if identifier.name == name && !self.is_bound(identifier.name.as_str()))
    }

    /// Whether a type is a key *type* — `string`, `number`, `symbol`,
    /// `PropertyKey` or a union of them — rather than a set of key names.
    fn is_key_keyword(&self, ty: &TSType<'_>) -> bool {
        match unparenthesized(ty) {
            TSType::TSStringKeyword(_)
            | TSType::TSNumberKeyword(_)
            | TSType::TSSymbolKeyword(_) => true,
            TSType::TSTypeReference(reference) => self.is_global(reference, "PropertyKey"),
            TSType::TSUnionType(union) => {
                union.types.iter().all(|member| self.is_key_keyword(member))
            }
            _ => false,
        }
    }

    fn is_bound(&self, name: &str) -> bool {
        self.type_scope.iter().any(|bound| bound == name)
            || self.summary.locals.contains_key(name)
            || self.summary.imports.contains_key(name)
    }

    fn reference(&mut self, reference: &TSTypeReference<'_>, context: Prec) {
        // TypeScript's markers: `NoInfer<T>` is `T` to everything but
        // inference, and `ThisType<T>` types `this` in an object literal,
        // which a declaration never holds.
        if self.is_global(reference, "NoInfer")
            && let Some(arguments) = &reference.type_arguments
            && let [argument] = arguments.params.as_slice()
        {
            self.ty(argument, context);
            return;
        }
        if self.is_global(reference, "ThisType") {
            self.printer.text("mixed");
            return;
        }
        // Flow's `Record` is an object type, which no interface is a subtype
        // of — and `Record<string, unknown>` is how a package says "any object
        // with string keys" in a bound its interfaces must meet. Keyed by a
        // key type rather than a union of names, it is exactly an indexer.
        if self.is_global(reference, "Record")
            && let Some(arguments) = &reference.type_arguments
            && let [key, value] = arguments.params.as_slice()
            && self.is_key_keyword(key)
        {
            self.printer.text("interface { readonly [key: ");
            self.ty(key, Prec::Any);
            self.printer.text("]: ");
            self.ty(value, Prec::Any);
            self.printer.text(" }");
            return;
        }
        // TypeScript's iteration types default their return and next types to
        // `any`; Flow's generators have no defaults and its iterators default
        // them to `void`, which is narrower than what the package declared.
        if let TSTypeName::IdentifierReference(identifier) = &reference.type_name
            && matches!(
                identifier.name.as_str(),
                "Generator" | "AsyncGenerator" | "Iterator" | "AsyncIterator"
            )
            && !self.is_bound(identifier.name.as_str())
        {
            let given = reference
                .type_arguments
                .as_ref()
                .map_or(0, |arguments| arguments.params.len());
            if given < 3 {
                self.printer.text(identifier.name.as_str());
                self.printer.char('<');
                for index in 0..3 {
                    if index > 0 {
                        self.printer.text(", ");
                    }
                    match reference
                        .type_arguments
                        .as_ref()
                        .and_then(|arguments| arguments.params.get(index))
                    {
                        Some(argument) => self.ty(argument, Prec::Any),
                        None if index == 0 => self.printer.text("mixed"),
                        None => self.printer.text("any"),
                    }
                }
                self.printer.char('>');
                return;
            }
        }
        self.type_name(&reference.type_name);
        self.type_arguments(reference.type_arguments.as_deref());
    }

    fn type_name(&mut self, name: &TSTypeName<'_>) {
        self.type_name_in(name, false);
    }

    /// A type name; `qualifies` when it is the left side of a qualified name,
    /// where a namespace's own name means the namespace.
    fn type_name_in(&mut self, name: &TSTypeName<'_>, qualifies: bool) {
        match name {
            TSTypeName::IdentifierReference(identifier) => {
                let written = identifier.name.as_str();
                let in_type_scope = self.type_scope.iter().any(|bound| bound == written);
                if !qualifies && !in_type_scope && self.self_interfaces.contains(written) {
                    self.printer.text(written);
                    self.printer.text(".$Self");
                    return;
                }
                match self.type_renames.get(written) {
                    Some(renamed) if !in_type_scope => {
                        let renamed = renamed.clone();
                        self.printer.text(&renamed);
                    }
                    _ => self.printer.text(written),
                }
            }
            TSTypeName::QualifiedName(qualified) => {
                self.type_name_in(&qualified.left, true);
                self.printer.char('.');
                self.printer.text(qualified.right.name.as_str());
            }
            TSTypeName::ThisExpression(_) => self.printer.text("this"),
        }
    }

    fn type_query(&mut self, query: &TSTypeQuery<'_>) {
        if query.type_arguments.is_some() {
            self.hole(
                Construct::InstantiationQuery,
                "`typeof f<T>` instantiates a value's type, which Flow's `typeof` cannot, so it \
                 is `any`",
                query.span.start,
            );
            self.printer.text("any");
            return;
        }
        match &query.expr_name {
            TSTypeQueryExprName::TSImportType(import) => {
                let local = self.hoist(import.source.value.as_str(), import.span.start);
                self.printer.text("typeof ");
                self.printer.text(&local);
                if let Some(qualifier) = &import.qualifier {
                    self.printer.char('.');
                    self.import_qualifier(qualifier);
                }
            }
            TSTypeQueryExprName::IdentifierReference(identifier) => {
                let name = identifier.name.as_str();
                if name == "globalThis" && !self.is_bound(name) {
                    self.global_this_hole(query.span.start);
                    return;
                }
                match self.values.get(name).cloned() {
                    Some(Value::Renamed(renamed)) => {
                        self.printer.text("typeof ");
                        self.printer.text(&renamed);
                    }
                    Some(Value::Typeof(alias)) => self.printer.text(&alias),
                    None => {
                        self.printer.text("typeof ");
                        self.printer.text(name);
                    }
                }
            }
            TSTypeQueryExprName::QualifiedName(qualified) => {
                let mut segments: SmallVec<[&str; 4]> = SmallVec::new();
                let mut left = &qualified.left;
                segments.push(qualified.right.name.as_str());
                let root = loop {
                    match left {
                        TSTypeName::IdentifierReference(identifier) => {
                            break identifier.name.as_str();
                        }
                        TSTypeName::QualifiedName(inner) => {
                            segments.push(inner.right.name.as_str());
                            left = &inner.left;
                        }
                        TSTypeName::ThisExpression(_) => break "this",
                    }
                };
                segments.reverse();
                if root == "globalThis" && !self.is_bound(root) {
                    self.global_this_hole(query.span.start);
                    return;
                }
                match self.values.get(root).cloned() {
                    Some(Value::Typeof(alias)) => {
                        self.printer.text(&alias);
                        for segment in &segments {
                            self.printer.text("[");
                            self.printer.text(&quoted(segment));
                            self.printer.text("]");
                        }
                    }
                    renamed => {
                        self.printer.text("typeof ");
                        match renamed {
                            Some(Value::Renamed(renamed)) => self.printer.text(&renamed),
                            _ => self.printer.text(root),
                        }
                        for segment in &segments {
                            self.printer.char('.');
                            self.printer.text(segment);
                        }
                    }
                }
            }
            TSTypeQueryExprName::ThisExpression(_) => self.printer.text("this"),
        }
    }

    fn global_this_hole(&mut self, offset: u32) {
        self.hole(
            Construct::GlobalThis,
            "`globalThis` has no Flow type, so this is `any`",
            offset,
        );
        self.printer.text("any");
    }

    fn import_type(&mut self, import: &TSImportType<'_>) {
        let local = self.hoist(import.source.value.as_str(), import.span.start);
        match &import.qualifier {
            Some(qualifier) => {
                self.printer.text(&local);
                self.printer.char('.');
                self.import_qualifier(qualifier);
            }
            None => {
                self.printer.text("typeof ");
                self.printer.text(&local);
            }
        }
        self.type_arguments(import.type_arguments.as_deref());
    }

    fn import_qualifier(&mut self, qualifier: &TSImportTypeQualifier<'_>) {
        match qualifier {
            TSImportTypeQualifier::Identifier(identifier) => {
                self.printer.text(identifier.name.as_str());
            }
            TSImportTypeQualifier::QualifiedName(qualified) => {
                self.import_qualifier(&qualified.left);
                self.printer.char('.');
                self.printer.text(qualified.right.name.as_str());
            }
        }
    }

    /// The local name a module an `import("…")` type names is imported under.
    fn hoist(&mut self, written: &str, offset: u32) -> CompactString {
        let specifier = self.specifier(written, offset);
        if let Some((_, local)) = self.hoisted.iter().find(|(known, _)| *known == specifier) {
            return local.clone();
        }
        let local = format_compact!("$Import{}", self.hoisted.len());
        self.hoisted.push((specifier, local.clone()));
        local
    }

    fn literal(&mut self, literal: &TSLiteral<'_>) {
        match literal {
            TSLiteral::BooleanLiteral(boolean) => {
                self.printer
                    .text(if boolean.value { "true" } else { "false" });
            }
            TSLiteral::NumericLiteral(number) => {
                self.number(number.value, number.raw.as_ref().map(|raw| raw.as_str()));
            }
            TSLiteral::BigIntLiteral(bigint) => self.bigint(bigint),
            TSLiteral::StringLiteral(string) => self.string(string.value.as_str()),
            TSLiteral::TemplateLiteral(template) => {
                let cooked = template
                    .quasis
                    .first()
                    .and_then(|quasi| quasi.value.cooked.as_ref())
                    .map_or("", |cooked| cooked.as_str());
                self.string(cooked);
            }
            TSLiteral::UnaryExpression(unary) => match (&unary.operator, &unary.argument) {
                (UnaryOperator::UnaryNegation, Expression::NumericLiteral(number)) => {
                    self.printer.char('-');
                    self.number(number.value, number.raw.as_ref().map(|raw| raw.as_str()));
                }
                (UnaryOperator::UnaryNegation, Expression::BigIntLiteral(bigint)) => {
                    self.printer.char('-');
                    self.bigint(bigint);
                }
                _ => self.printer.text("number"),
            },
        }
    }

    fn number(&mut self, value: f64, raw: Option<&str>) {
        match raw.filter(|raw| !raw.contains('_')) {
            Some(raw) => self.printer.text(raw),
            None => self.printer.text(&number_text(value)),
        }
    }

    fn bigint(&mut self, bigint: &oxc_ast::ast::BigIntLiteral<'_>) {
        match bigint.raw.as_ref().filter(|raw| !raw.contains('_')) {
            Some(raw) => self.printer.text(raw.as_str()),
            None => {
                self.printer.text(bigint.value.as_str());
                self.printer.char('n');
            }
        }
    }

    fn string(&mut self, value: &str) {
        self.printer.text(&quoted(value));
    }

    fn hole(&mut self, construct: Construct, reason: impl Into<CompactString>, offset: u32) {
        let declaration = if self.context.is_empty() {
            CompactString::const_new("(module)")
        } else {
            self.context.join(".").into()
        };
        self.holes.push(Hole {
            declaration,
            construct,
            reason: reason.into(),
            line: self.printer.source_line(offset) + 1,
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClassExport {
    /// Exported under its own name when the declaration is.
    AsDeclared,
    /// `export default class`.
    Default,
}

/// Where a type parameter can occur, for [`infer_variance`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    Positive,
    Negative,
    Both,
}

impl Polarity {
    fn flip(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
            Self::Both => Self::Both,
        }
    }
}

/// The variance TypeScript would measure for each unannotated parameter of
/// `parameters`, as the keyword Flow spells it — `out`, `in`, or nothing for
/// invariant — given a walk of the declaration's body in `walk`.
///
/// TypeScript measures variance by looking at a declaration's structure; Flow
/// takes a parameter with no annotation as invariant. So without this a
/// `QueryObserverResult<string>` is not a `QueryObserverResult<unknown>` in
/// Flow when it is one in every TypeScript program written against it. The
/// measurement is TypeScript's, simplified: a property, a return type, an
/// index signature and a heritage argument read a parameter covariantly; a
/// parameter of a function *type* or of a call signature reads it
/// contravariantly; a *method*'s parameters count for neither, because
/// TypeScript compares methods bivariantly; and a conditional type's check, a
/// mapped type's keys and an indexed access read it both ways.
///
/// A parameter Flow finds used against its inferred variance is reported in
/// the translation and filtered out of what `uf check` shows. Flow still types
/// uses of the declaration by the variance it was given, which is the point.
fn infer_variance(
    parameters: &TSTypeParameterDeclaration<'_>,
    walk: impl FnOnce(&mut Uses<'_>),
) -> Vec<Option<&'static str>> {
    let names: SmallVec<[&str; 4]> = parameters
        .params
        .iter()
        .map(|parameter| parameter.name.name.as_str())
        .collect();
    let mut uses = Uses {
        names: &names,
        positive: SmallVec::from_elem(false, names.len()),
        negative: SmallVec::from_elem(false, names.len()),
    };
    walk(&mut uses);
    (0..names.len())
        .map(|index| match (uses.positive[index], uses.negative[index]) {
            (_, false) => Some("out"),
            (false, true) => Some("in"),
            (true, true) => None,
        })
        .collect()
}

/// Where a declaration's type parameters occur.
struct Uses<'n> {
    names: &'n [&'n str],
    positive: SmallVec<[bool; 4]>,
    negative: SmallVec<[bool; 4]>,
}

impl Uses<'_> {
    fn mark(&mut self, name: &str, polarity: Polarity) {
        let Some(index) = self.names.iter().position(|known| *known == name) else {
            return;
        };
        if polarity != Polarity::Negative {
            self.positive[index] = true;
        }
        if polarity != Polarity::Positive {
            self.negative[index] = true;
        }
    }

    fn annotation(&mut self, annotation: Option<&TSTypeAnnotation<'_>>, polarity: Polarity) {
        if let Some(annotation) = annotation {
            self.ty(&annotation.type_annotation, polarity);
        }
    }

    fn parameters(&mut self, params: &FormalParameters<'_>, polarity: Polarity) {
        for param in &params.items {
            self.annotation(param.type_annotation.as_deref(), polarity);
        }
        if let Some(rest) = &params.rest {
            self.annotation(rest.type_annotation.as_deref(), polarity);
        }
    }

    fn members(&mut self, members: &[TSSignature<'_>], polarity: Polarity) {
        for member in members {
            match member {
                TSSignature::TSPropertySignature(property) => {
                    self.annotation(property.type_annotation.as_deref(), polarity);
                }
                TSSignature::TSMethodSignature(method) => {
                    self.annotation(method.return_type.as_deref(), polarity);
                }
                TSSignature::TSCallSignatureDeclaration(call) => {
                    self.parameters(&call.params, polarity.flip());
                    self.annotation(call.return_type.as_deref(), polarity);
                }
                TSSignature::TSConstructSignatureDeclaration(construct) => {
                    self.parameters(&construct.params, polarity.flip());
                    self.annotation(construct.return_type.as_deref(), polarity);
                }
                TSSignature::TSIndexSignature(index) => {
                    self.ty(&index.type_annotation.type_annotation, polarity);
                }
            }
        }
    }

    fn class_members(&mut self, elements: &[ClassElement<'_>]) {
        for element in elements {
            match element {
                ClassElement::PropertyDefinition(property) if !property.r#static => {
                    self.annotation(property.type_annotation.as_deref(), Polarity::Positive);
                }
                ClassElement::AccessorProperty(accessor) if !accessor.r#static => {
                    self.annotation(accessor.type_annotation.as_deref(), Polarity::Positive);
                }
                ClassElement::MethodDefinition(method)
                    if !method.r#static && method.kind != MethodDefinitionKind::Constructor =>
                {
                    self.annotation(method.value.return_type.as_deref(), Polarity::Positive);
                }
                ClassElement::TSIndexSignature(index) if !index.r#static => {
                    self.ty(&index.type_annotation.type_annotation, Polarity::Positive);
                }
                _ => {}
            }
        }
    }

    fn ty(&mut self, ty: &TSType<'_>, polarity: Polarity) {
        match ty {
            TSType::TSTypeReference(reference) => {
                if let TSTypeName::IdentifierReference(identifier) = &reference.type_name {
                    self.mark(identifier.name.as_str(), polarity);
                }
                if let Some(arguments) = &reference.type_arguments {
                    for argument in &arguments.params {
                        self.ty(argument, polarity);
                    }
                }
            }
            TSType::TSFunctionType(function) => {
                self.parameters(&function.params, polarity.flip());
                self.ty(&function.return_type.type_annotation, polarity);
            }
            TSType::TSConstructorType(constructor) => {
                self.parameters(&constructor.params, polarity.flip());
                self.ty(&constructor.return_type.type_annotation, polarity);
            }
            TSType::TSConditionalType(conditional) => {
                self.ty(&conditional.check_type, Polarity::Both);
                self.ty(&conditional.extends_type, Polarity::Both);
                self.ty(&conditional.true_type, polarity);
                self.ty(&conditional.false_type, polarity);
            }
            TSType::TSMappedType(mapped) => {
                self.ty(&mapped.constraint, Polarity::Both);
                if let Some(name_type) = &mapped.name_type {
                    self.ty(name_type, Polarity::Both);
                }
                if let Some(value) = &mapped.type_annotation {
                    self.ty(value, polarity);
                }
            }
            TSType::TSTypeOperatorType(operator) => {
                // TypeScript reads `keyof T` contravariantly and Flow reads it
                // covariantly, so it is neither the one nor the other here.
                let inner = match operator.operator {
                    TSTypeOperatorOperator::Keyof => Polarity::Both,
                    TSTypeOperatorOperator::Unique | TSTypeOperatorOperator::Readonly => polarity,
                };
                self.ty(&operator.type_annotation, inner);
            }
            TSType::TSIndexedAccessType(indexed) => {
                self.ty(&indexed.object_type, Polarity::Both);
                self.ty(&indexed.index_type, Polarity::Both);
            }
            TSType::TSTypeLiteral(literal) => self.members(&literal.members, polarity),
            TSType::TSUnionType(union) => {
                for member in &union.types {
                    self.ty(member, polarity);
                }
            }
            TSType::TSIntersectionType(intersection) => {
                for member in &intersection.types {
                    self.ty(member, polarity);
                }
            }
            TSType::TSArrayType(array) => self.ty(&array.element_type, polarity),
            TSType::TSTupleType(tuple) => {
                for element in &tuple.element_types {
                    self.tuple_element(element, polarity);
                }
            }
            TSType::TSNamedTupleMember(member) => {
                self.tuple_element(&member.element_type, polarity)
            }
            TSType::TSParenthesizedType(parenthesized) => {
                self.ty(&parenthesized.type_annotation, polarity);
            }
            TSType::TSTemplateLiteralType(template) => {
                for interpolated in &template.types {
                    self.ty(interpolated, polarity);
                }
            }
            TSType::TSTypePredicate(predicate) => {
                if let Some(guarded) = &predicate.type_annotation {
                    self.ty(&guarded.type_annotation, Polarity::Both);
                }
            }
            TSType::TSImportType(import) => {
                if let Some(arguments) = &import.type_arguments {
                    for argument in &arguments.params {
                        self.ty(argument, polarity);
                    }
                }
            }
            _ => {}
        }
    }

    fn tuple_element(&mut self, element: &TSTupleElement<'_>, polarity: Polarity) {
        match element {
            TSTupleElement::TSOptionalType(optional) => {
                self.ty(&optional.type_annotation, polarity);
            }
            TSTupleElement::TSRestType(rest) => self.ty(&rest.type_annotation, polarity),
            other => self.ty(other.to_ts_type(), polarity),
        }
    }
}

/// Whether a type refers to any of `names`.
struct Mentions<'n> {
    names: &'n [CompactString],
    found: bool,
}

impl<'a> Visit<'a> for Mentions<'_> {
    fn visit_ts_type_reference(&mut self, it: &TSTypeReference<'a>) {
        if let TSTypeName::IdentifierReference(identifier) = &it.type_name
            && self
                .names
                .iter()
                .any(|name| name == identifier.name.as_str())
        {
            self.found = true;
            return;
        }
        walk::walk_ts_type_reference(self, it);
    }
}

/// The `infer` names a conditional's `extends` clause binds.
#[derive(Default)]
struct InferNames {
    names: Vec<CompactString>,
}

impl<'a> Visit<'a> for InferNames {
    fn visit_ts_infer_type(&mut self, it: &oxc_ast::ast::TSInferType<'a>) {
        self.names
            .push(it.type_parameter.name.name.to_compact_string());
        walk::walk_ts_infer_type(self, it);
    }
}

/// The index signatures in a list of members.
fn interface_indexers<'m, 'a>(
    members: &'m [TSSignature<'a>],
) -> SmallVec<[&'m TSIndexSignature<'a>; 2]> {
    members
        .iter()
        .filter_map(|member| match member {
            TSSignature::TSIndexSignature(index) => Some(&**index),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyKind {
    String,
    Number,
}

/// Whether an index signature's key type is, or is a union including, `kind`.
fn index_key_includes(index: &TSIndexSignature<'_>, kind: KeyKind) -> bool {
    let matches = |ty: &TSType<'_>| match kind {
        KeyKind::String => matches!(ty, TSType::TSStringKeyword(_)),
        KeyKind::Number => matches!(ty, TSType::TSNumberKeyword(_)),
    };
    match &index.parameter.type_annotation.type_annotation {
        TSType::TSUnionType(union) => union.types.iter().any(matches),
        other => matches(other),
    }
}

/// What a rest element spreads: the element type, or the type a labelled rest
/// member holds.
fn rest_inner<'r, 'a>(ty: &'r TSType<'a>) -> &'r TSType<'a> {
    match unparenthesized(ty) {
        TSType::TSNamedTupleMember(named) => match &named.element_type {
            TSTupleElement::TSRestType(rest) => unparenthesized(&rest.type_annotation),
            other => unparenthesized(other.to_ts_type()),
        },
        other => other,
    }
}

fn unparenthesized<'r, 'a>(ty: &'r TSType<'a>) -> &'r TSType<'a> {
    match ty {
        TSType::TSParenthesizedType(parenthesized) => {
            unparenthesized(&parenthesized.type_annotation)
        }
        other => other,
    }
}

/// The name a parameter binds, or a made-up one for a destructuring pattern.
///
/// Made up rather than left out because an optional destructured parameter
/// still needs somewhere to put its `?`.
fn binding_name(pattern: &BindingPattern<'_>, index: usize) -> CompactString {
    match pattern.get_binding_identifier() {
        Some(identifier) if matches!(pattern, BindingPattern::BindingIdentifier(_)) => {
            identifier.name.to_compact_string()
        }
        _ => format_compact!("arg{index}"),
    }
}

/// The name a declaration binds, for naming a hole — and, in
/// [`crate::locate`], for naming the declaration a finding is in.
pub(crate) fn declaration_name<'a>(declaration: &Declaration<'a>) -> Option<&'a str> {
    match declaration {
        Declaration::VariableDeclaration(variables) => variables
            .declarations
            .first()
            .and_then(|declarator| declarator.id.get_binding_identifier())
            .map(|identifier| identifier.name.as_str()),
        Declaration::FunctionDeclaration(function) => {
            function.id.as_ref().map(|id| id.name.as_str())
        }
        Declaration::ClassDeclaration(class) => class.id.as_ref().map(|id| id.name.as_str()),
        Declaration::TSTypeAliasDeclaration(alias) => Some(alias.id.name.as_str()),
        Declaration::TSInterfaceDeclaration(interface) => Some(interface.id.name.as_str()),
        Declaration::TSEnumDeclaration(declaration) => Some(declaration.id.name.as_str()),
        Declaration::TSNamespaceDeclaration(namespace) => Some(namespace.id.name.as_str()),
        Declaration::TSImportEqualsDeclaration(import) => Some(import.id.name.as_str()),
        Declaration::TSGlobalDeclaration(_) | Declaration::TSExternalModuleDeclaration(_) => None,
    }
}

/// A double-quoted string literal.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            control if control.is_control() => {
                out.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// A number as a literal: integers without a fraction, everything else as
/// Rust prints it.
fn number_text(value: f64) -> CompactString {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        format_compact!("{}", value as i64)
    } else {
        format_compact!("{value}")
    }
}

/// One enum member with its value worked out.
struct EnumMember {
    name: CompactString,
    value: EnumValue,
    offset: u32,
}

#[derive(Clone)]
enum EnumValue {
    Number(f64),
    String(CompactString),
}

/// Every member of the enum `group` declares, with its value, or why a Flow
/// enum cannot hold them.
///
/// Flow's enums are of one kind — all strings or all numbers — and every
/// member names an explicit value, so the translation computes what
/// TypeScript would: the auto-incremented numbers, the constant arithmetic,
/// the references to earlier members.
fn enum_members(group: &[&TSEnumDeclaration<'_>]) -> Result<Vec<EnumMember>, (String, u32)> {
    let mut members: Vec<EnumMember> = Vec::new();
    for declaration in group {
        let mut previous: Option<EnumValue> = None;
        for member in &declaration.body.members {
            let offset = member.span.start;
            let name = match &member.id {
                TSEnumMemberName::Identifier(identifier) => identifier.name.to_compact_string(),
                TSEnumMemberName::String(literal) | TSEnumMemberName::ComputedString(literal)
                    if is_identifier(literal.value.as_str()) =>
                {
                    literal.value.to_compact_string()
                }
                _ => {
                    return Err((
                        "a Flow enum member is named by an identifier, and this one is not"
                            .to_owned(),
                        offset,
                    ));
                }
            };
            let value = match &member.initializer {
                Some(initializer) => evaluate(initializer, &members).ok_or_else(|| {
                    (
                        format!("the value of `{name}` is not a constant uf can compute"),
                        offset,
                    )
                })?,
                None => match &previous {
                    None => EnumValue::Number(0.0),
                    Some(EnumValue::Number(number)) => EnumValue::Number(number + 1.0),
                    Some(EnumValue::String(_)) => {
                        return Err((
                            format!("`{name}` follows a string member and has no value"),
                            offset,
                        ));
                    }
                },
            };
            previous = Some(value.clone());
            members.push(EnumMember {
                name,
                value,
                offset,
            });
        }
    }
    let strings = members
        .iter()
        .filter(|member| matches!(member.value, EnumValue::String(_)))
        .count();
    if strings != 0 && strings != members.len() {
        let offset = group
            .first()
            .map_or(0, |declaration| declaration.span.start);
        return Err((
            "a Flow enum holds strings or numbers and this one mixes them".to_owned(),
            offset,
        ));
    }
    Ok(members)
}

/// A constant enum initializer, evaluated.
fn evaluate(expression: &Expression<'_>, members: &[EnumMember]) -> Option<EnumValue> {
    match expression {
        Expression::NumericLiteral(literal) => Some(EnumValue::Number(literal.value)),
        Expression::StringLiteral(literal) => {
            Some(EnumValue::String(literal.value.to_compact_string()))
        }
        Expression::TemplateLiteral(template) if template.expressions.is_empty() => {
            let cooked = template.quasis.first()?.value.cooked.as_ref()?;
            Some(EnumValue::String(cooked.to_compact_string()))
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            evaluate(&parenthesized.expression, members)
        }
        Expression::Identifier(identifier) => members
            .iter()
            .rev()
            .find(|member| member.name == identifier.name.as_str())
            .map(|member| member.value.clone()),
        Expression::StaticMemberExpression(member) => members
            .iter()
            .rev()
            .find(|known| known.name == member.property.name.as_str())
            .map(|known| known.value.clone()),
        Expression::UnaryExpression(unary) => {
            let EnumValue::Number(number) = evaluate(&unary.argument, members)? else {
                return None;
            };
            match unary.operator {
                UnaryOperator::UnaryNegation => Some(EnumValue::Number(-number)),
                UnaryOperator::UnaryPlus => Some(EnumValue::Number(number)),
                UnaryOperator::BitwiseNot => Some(EnumValue::Number(f64::from(!to_i32(number)))),
                _ => None,
            }
        }
        Expression::BinaryExpression(binary) => {
            use oxc_ast::ast::BinaryOperator;
            let left = evaluate(&binary.left, members)?;
            let right = evaluate(&binary.right, members)?;
            match (left, right) {
                (EnumValue::String(left), EnumValue::String(right))
                    if binary.operator == BinaryOperator::Addition =>
                {
                    Some(EnumValue::String(format_compact!("{left}{right}")))
                }
                (EnumValue::Number(left), EnumValue::Number(right)) => {
                    let value = match binary.operator {
                        BinaryOperator::Addition => left + right,
                        BinaryOperator::Subtraction => left - right,
                        BinaryOperator::Multiplication => left * right,
                        BinaryOperator::Division => left / right,
                        BinaryOperator::Remainder => left % right,
                        BinaryOperator::Exponential => left.powf(right),
                        BinaryOperator::BitwiseOR => f64::from(to_i32(left) | to_i32(right)),
                        BinaryOperator::BitwiseAnd => f64::from(to_i32(left) & to_i32(right)),
                        BinaryOperator::BitwiseXOR => f64::from(to_i32(left) ^ to_i32(right)),
                        BinaryOperator::ShiftLeft => {
                            f64::from(to_i32(left).wrapping_shl(to_u32(right) & 31))
                        }
                        BinaryOperator::ShiftRight => {
                            f64::from(to_i32(left).wrapping_shr(to_u32(right) & 31))
                        }
                        BinaryOperator::ShiftRightZeroFill => {
                            f64::from(to_u32(left).wrapping_shr(to_u32(right) & 31))
                        }
                        _ => return None,
                    };
                    value.is_finite().then_some(EnumValue::Number(value))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// ECMAScript's `ToInt32`, for the bitwise operators an enum may use.
fn to_i32(value: f64) -> i32 {
    to_u32(value) as i32
}

/// ECMAScript's `ToUint32`.
fn to_u32(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    let truncated = value.trunc().rem_euclid(4_294_967_296.0);
    truncated as u32
}

fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first == '$' || first.is_alphabetic())
        && characters.all(|rest| rest == '_' || rest == '$' || rest.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_is_quoted_so_flow_reads_the_same_characters() {
        assert_eq!(quoted("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        assert_eq!(quoted("~standard"), "\"~standard\"");
    }

    #[test]
    fn a_whole_number_prints_without_a_fraction() {
        assert_eq!(number_text(3.0), "3");
        assert_eq!(number_text(-1.0), "-1");
        assert_eq!(number_text(1.5), "1.5");
    }

    #[test]
    fn bitwise_enum_arithmetic_follows_ecmascript() {
        assert_eq!(to_i32(4_294_967_295.0), -1);
        assert_eq!(to_u32(-1.0), 4_294_967_295);
        assert_eq!(to_i32(f64::NAN), 0);
    }
}
