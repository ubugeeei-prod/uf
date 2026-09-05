//! Imports, exports and `declare` forms.

use uf_flow::Loc;
use uf_flow::ast::statement;

use super::Printer;
use super::statement::EmptyBlock;
use crate::doc::{Doc, HARDLINE, LINE, SOFTLINE};
use crate::flow::comments::{Marker, Placement};
use crate::flow::node::{NodeKey, NodeRef};

impl<'a> Printer<'a> {
    /// `import type { a as b }, * as ns, def from "m" with { type: "json" };`
    pub fn print_import(
        &mut self,
        import: &'a statement::ImportDeclaration<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let _ = key;
        let mut parts = vec![self.s("import")];
        parts.push(match import.import_kind {
            statement::ImportKind::ImportType => self.s(" type"),
            statement::ImportKind::ImportTypeof => self.s(" typeof"),
            statement::ImportKind::ImportValue => self.s(""),
        });

        let mut standalone: Vec<Doc<'a>> = Vec::new();
        let mut grouped: Vec<Doc<'a>> = Vec::new();
        let mut any_comment = false;
        if let Some(default) = &import.default {
            any_comment |= self.has_comment(NodeRef::ImportDefault(default).key());
            standalone.push(self.print_node(NodeRef::ImportDefault(default), |p| {
                p.print_identifier(&default.identifier)
            }));
        }
        match &import.specifiers {
            Some(statement::import_declaration::Specifier::ImportNamespaceSpecifier(ns)) => {
                any_comment |= self.has_comment(NodeRef::ImportNamespace(ns).key());
                standalone.push(self.print_node(NodeRef::ImportNamespace(ns), |p| {
                    let name = p.print_identifier(&ns.1);
                    p.concat([p.s("* as "), name])
                }));
            }
            Some(statement::import_declaration::Specifier::ImportNamedSpecifiers(named)) => {
                for specifier in named {
                    any_comment |= self.has_comment(NodeRef::ImportSpecifier(specifier).key());
                    grouped.push(self.print_import_specifier(specifier));
                }
            }
            None => {}
        }

        let has_braces_in_source = matches!(
            &import.specifiers,
            Some(statement::import_declaration::Specifier::ImportNamedSpecifiers(_))
        );
        let print_specifiers = !standalone.is_empty()
            || !grouped.is_empty()
            || has_braces_in_source
            || matches!(import.import_kind, statement::ImportKind::ImportType)
            || self.import_has_empty_braces(import);
        if print_specifiers {
            parts.push(self.s(" "));
            let separator = self.s(", ");
            parts.push(self.join(separator, standalone.clone()));
            if !grouped.is_empty() {
                if !standalone.is_empty() {
                    parts.push(self.s(", "));
                }
                parts.push(
                    self.print_braced_specifiers(grouped, !standalone.is_empty() || any_comment),
                );
            } else if standalone.is_empty() {
                parts.push(self.s("{}"));
            }
            parts.push(self.s(" from"));
        }
        parts.push(self.s(" "));
        parts.push(self.print_string_node(&import.source.0, &import.source.1));
        if let Some((_, attributes)) = &import.attributes
            && !attributes.is_empty()
        {
            let printed: Vec<Doc<'a>> = attributes
                .iter()
                .map(|attribute| {
                    self.print_node(NodeRef::ImportAttribute(attribute), |p| {
                        let key = match &attribute.key {
                            statement::import_declaration::ImportAttributeKey::Identifier(id) => {
                                p.print_identifier(id)
                            }
                            statement::import_declaration::ImportAttributeKey::StringLiteral(
                                loc,
                                literal,
                            ) => p.print_string_node(loc, literal),
                        };
                        let value = p.print_string_node(&attribute.value.0, &attribute.value.1);
                        p.concat([key, p.s(": "), value])
                    })
                })
                .collect();
            let separator = self.s(", ");
            parts.push(self.s(" with { "));
            parts.push(self.join(separator, printed));
            parts.push(self.s(" }"));
        }
        parts.push(self.semi());
        self.docs.concat_vec(parts)
    }

    /// Whether `import {} from "m"` was written with braces, which are
    /// kept, as opposed to `import "m"`.
    fn import_has_empty_braces(&self, import: &'a statement::ImportDeclaration<Loc, Loc>) -> bool {
        if import.default.is_some() || import.specifiers.is_some() {
            return false;
        }
        let Some(NodeRef::Statement(statement)) = self.current() else {
            return false;
        };
        let start = self.text.span(statement.loc()).start;
        let source_start = self.text.span(&import.source.0).start;
        let between = self.text.slice(crate::flow::text::Span {
            start,
            end: source_start,
        });
        let compact: String = between.chars().filter(|ch| !ch.is_whitespace()).collect();
        compact.contains("{}")
    }

    /// `{ a, b as c }`, breakable when there is more than one.
    fn print_braced_specifiers(&mut self, grouped: Vec<Doc<'a>>, force_breakable: bool) -> Doc<'a> {
        let can_break = grouped.len() > 1 || force_breakable;
        if can_break {
            let separator = self.concat([self.s(","), &LINE]);
            return self.group(self.concat([
                self.s("{"),
                self.indent(self.concat([&LINE, self.join(separator, grouped)])),
                self.if_break(self.s(","), self.s("")),
                &LINE,
                self.s("}"),
            ]));
        }
        let inner = self.docs.concat_vec(grouped);
        self.concat([self.s("{ "), inner, self.s(" }")])
    }

    fn print_import_specifier(
        &mut self,
        specifier: &'a statement::import_declaration::NamedSpecifier<Loc, Loc>,
    ) -> Doc<'a> {
        self.print_node(NodeRef::ImportSpecifier(specifier), |p| {
            let kind = match specifier.kind {
                Some(statement::ImportKind::ImportType) => p.s("type "),
                Some(statement::ImportKind::ImportTypeof) => p.s("typeof "),
                _ => p.s(""),
            };
            let remote = p.print_identifier(&specifier.remote);
            match &specifier.local {
                Some(local) if local.name != specifier.remote.name => {
                    let local = p.print_identifier(local);
                    p.concat([kind, remote, p.s(" as "), local])
                }
                Some(local) => {
                    // `a as a` is printed as written, but the local
                    // identifier's comments must not be lost.
                    let local_key = NodeRef::Identifier(local).key();
                    if p.has_comment(local_key) {
                        let local = p.print_identifier(local);
                        p.concat([kind, remote, p.s(" as "), local])
                    } else {
                        p.concat([kind, remote])
                    }
                }
                None => p.concat([kind, remote]),
            }
        })
    }

    /// `export { a as b } from "m";`, `export * as ns from "m";`,
    /// `export type T = …;`, `export const x = 1;`.
    pub fn print_export_named(
        &mut self,
        export: &'a statement::ExportNamedDeclaration<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let mut parts = vec![self.s("export")];
        if let Some(dangling) = self.print_dangling_comments(key, Marker::None, false) {
            parts.push(self.s(" "));
            parts.push(dangling);
            if self.has_line_comment(key, Some(Placement::Dangling)) {
                parts.push(&HARDLINE);
            }
        }
        if let Some(declaration) = &export.declaration {
            let printed = self.print_statement(declaration);
            parts.push(self.s(" "));
            parts.push(printed);
            return self.docs.concat_vec(parts);
        }
        if matches!(export.export_kind, statement::ExportKind::ExportType) {
            parts.push(self.s(" type"));
        }
        match &export.specifiers {
            Some(statement::export_named_declaration::Specifier::ExportBatchSpecifier(batch)) => {
                let printed =
                    self.print_node(NodeRef::ExportBatch(batch), |p| match &batch.specifier {
                        Some(name) => {
                            let name = p.print_identifier(name);
                            p.concat([p.s(" * as "), name])
                        }
                        None => p.s(" *"),
                    });
                parts.push(printed);
            }
            Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)) => {
                let any_comment = specifiers
                    .iter()
                    .any(|specifier| self.has_comment(NodeRef::ExportSpecifier(specifier).key()));
                let printed: Vec<Doc<'a>> = specifiers
                    .iter()
                    .map(|specifier| self.print_export_specifier(specifier))
                    .collect();
                parts.push(self.s(" "));
                if printed.is_empty() {
                    parts.push(self.s("{}"));
                } else {
                    parts.push(self.print_braced_specifiers(printed, any_comment));
                }
            }
            None => {
                parts.push(self.s(" {}"));
            }
        }
        if let Some((loc, source)) = &export.source {
            let printed = self.print_string_node(loc, source);
            parts.push(self.s(" from "));
            parts.push(printed);
        }
        parts.push(self.semi());
        self.docs.concat_vec(parts)
    }

    fn print_export_specifier(
        &mut self,
        specifier: &'a statement::export_named_declaration::ExportSpecifier<Loc, Loc>,
    ) -> Doc<'a> {
        self.print_node(NodeRef::ExportSpecifier(specifier), |p| {
            let kind = match specifier.export_kind {
                statement::ExportKind::ExportType => p.s("type "),
                statement::ExportKind::ExportValue => p.s(""),
            };
            let local = p.print_identifier(&specifier.local);
            match &specifier.exported {
                Some(exported)
                    if exported.name != specifier.local.name
                        || p.has_comment(NodeRef::Identifier(exported).key()) =>
                {
                    let exported = p.print_identifier(exported);
                    p.concat([kind, local, p.s(" as "), exported])
                }
                _ => p.concat([kind, local]),
            }
        })
    }

    /// `export default …`.
    pub fn print_export_default(
        &mut self,
        export: &'a statement::ExportDefaultDeclaration<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let mut parts = vec![self.s("export default")];
        if let Some(dangling) = self.print_dangling_comments(key, Marker::None, false) {
            parts.push(self.s(" "));
            parts.push(dangling);
            if self.has_line_comment(key, Some(Placement::Dangling)) {
                parts.push(&HARDLINE);
            }
        }
        match &export.declaration {
            statement::export_default_declaration::Declaration::Declaration(declaration) => {
                let printed = self.print_statement(declaration);
                parts.push(self.s(" "));
                parts.push(printed);
                let needs_semi = !matches!(
                    &**declaration,
                    statement::StatementInner::ClassDeclaration { .. }
                        | statement::StatementInner::ComponentDeclaration { .. }
                        | statement::StatementInner::FunctionDeclaration { .. }
                        | statement::StatementInner::DeclareClass { .. }
                        | statement::StatementInner::DeclareComponent { .. }
                        | statement::StatementInner::DeclareFunction { .. }
                        | statement::StatementInner::EnumDeclaration { .. }
                        | statement::StatementInner::InterfaceDeclaration { .. }
                        | statement::StatementInner::TypeAlias { .. }
                        | statement::StatementInner::OpaqueType { .. }
                        | statement::StatementInner::VariableDeclaration { .. }
                        | statement::StatementInner::Expression { .. }
                );
                if needs_semi {
                    parts.push(self.semi());
                }
            }
            statement::export_default_declaration::Declaration::Expression(expression) => {
                let printed = self.print_expression(expression);
                parts.push(self.s(" "));
                parts.push(printed);
                parts.push(self.semi());
            }
        }
        self.docs.concat_vec(parts)
    }

    /// `declare export …`.
    pub fn print_declare_export(
        &mut self,
        export: &'a statement::DeclareExportDeclaration<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        use statement::declare_export_declaration::Declaration;
        let mut parts = vec![self.s("declare export")];
        if export.default.is_some() {
            parts.push(self.s(" default"));
        }
        if let Some(dangling) = self.print_dangling_comments(key, Marker::None, false) {
            parts.push(self.s(" "));
            parts.push(dangling);
        }
        if let Some(declaration) = &export.declaration {
            let node = NodeRef::DeclareExportInner(declaration);
            let printed = self.print_node(node, |p| match declaration {
                Declaration::Variable { declaration, .. } => {
                    p.print_declare_variable_bare(declaration)
                }
                Declaration::Function { declaration, .. } => {
                    p.print_declare_function_inner(declaration, node.key(), false)
                }
                Declaration::Class { declaration, .. } => {
                    p.print_declare_class_bare(declaration, node.key())
                }
                Declaration::Component { declaration, .. } => {
                    p.print_declare_component_bare(declaration)
                }
                Declaration::DefaultType { type_ } => {
                    let printed = p.print_type(type_);
                    p.concat([printed, p.semi()])
                }
                Declaration::NamedType { declaration, .. } => {
                    p.print_type_alias(declaration, node.key())
                }
                Declaration::NamedOpaqueType { declaration, .. } => {
                    p.print_opaque_type(declaration, node.key())
                }
                Declaration::Interface { declaration, .. } => {
                    p.print_interface(declaration, node.key())
                }
                Declaration::Enum { declaration, .. } => p.print_enum(declaration, node.key()),
                Declaration::Namespace { declaration, .. } => {
                    p.print_declare_namespace_bare(declaration)
                }
            });
            parts.push(self.s(" "));
            parts.push(printed);
            return self.docs.concat_vec(parts);
        }
        match &export.specifiers {
            Some(statement::export_named_declaration::Specifier::ExportBatchSpecifier(batch)) => {
                let printed =
                    self.print_node(NodeRef::ExportBatch(batch), |p| match &batch.specifier {
                        Some(name) => {
                            let name = p.print_identifier(name);
                            p.concat([p.s(" * as "), name])
                        }
                        None => p.s(" *"),
                    });
                parts.push(printed);
            }
            Some(statement::export_named_declaration::Specifier::ExportSpecifiers(specifiers)) => {
                let any_comment = specifiers
                    .iter()
                    .any(|specifier| self.has_comment(NodeRef::ExportSpecifier(specifier).key()));
                let printed: Vec<Doc<'a>> = specifiers
                    .iter()
                    .map(|specifier| self.print_export_specifier(specifier))
                    .collect();
                parts.push(self.s(" "));
                if printed.is_empty() {
                    parts.push(self.s("{}"));
                } else {
                    parts.push(self.print_braced_specifiers(printed, any_comment));
                }
            }
            None => parts.push(self.s(" {}")),
        }
        if let Some((loc, source)) = &export.source {
            let printed = self.print_string_node(loc, source);
            parts.push(self.s(" from "));
            parts.push(printed);
        }
        parts.push(self.semi());
        self.docs.concat_vec(parts)
    }

    /// `declare var x: T;` as a statement.
    pub fn print_declare_variable(
        &mut self,
        declaration: &'a statement::DeclareVariable<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let _ = key;
        let printed = self.print_declare_variable_bare(declaration);
        self.concat([self.s("declare "), printed])
    }

    /// `var x: T;` without the `declare`.
    fn print_declare_variable_bare(
        &mut self,
        declaration: &'a statement::DeclareVariable<Loc, Loc>,
    ) -> Doc<'a> {
        let printed: Vec<Doc<'a>> = declaration
            .declarations
            .iter()
            .map(|declarator| {
                self.print_node(NodeRef::Declarator(declarator), |p| {
                    let id = p.print_pattern(&declarator.id);
                    match &declarator.init {
                        Some(init) => {
                            let init = p.print_expression(init);
                            p.concat([id, p.s(" = "), init])
                        }
                        None => id,
                    }
                })
            })
            .collect();
        let separator = self.s(", ");
        self.concat([
            self.s(declaration.kind.as_str()),
            self.s(" "),
            self.join(separator, printed),
            self.semi(),
        ])
    }

    /// `declare class` inside `declare export`, without the `declare`.
    fn print_declare_class_bare(
        &mut self,
        class: &'a statement::DeclareClass<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let printed = self.print_declare_class(class, key);
        self.strip_declare_prefix(printed)
    }

    fn print_declare_component_bare(
        &mut self,
        component: &'a statement::DeclareComponent<Loc, Loc>,
    ) -> Doc<'a> {
        let printed = self.print_declare_component(component);
        self.strip_declare_prefix(printed)
    }

    fn print_declare_namespace_bare(
        &mut self,
        namespace: &'a statement::DeclareNamespace<Loc, Loc>,
    ) -> Doc<'a> {
        let printed = self.print_declare_namespace(namespace);
        self.strip_declare_prefix(printed)
    }

    /// Remove a leading `declare ` text from a printed declaration, for
    /// the `declare export` forms that spell it once.
    fn strip_declare_prefix(&self, doc: Doc<'a>) -> Doc<'a> {
        match doc.kind {
            crate::doc::DocKind::Concat(parts) => match parts.first().map(|first| first.kind) {
                Some(crate::doc::DocKind::Text("declare ")) => {
                    self.docs.concat(parts[1..].iter().copied())
                }
                Some(crate::doc::DocKind::Text("declare class ")) => {
                    let mut rest: Vec<Doc<'a>> = vec![self.s("class ")];
                    rest.extend(parts[1..].iter().copied());
                    self.docs.concat_vec(rest)
                }
                Some(crate::doc::DocKind::Text("declare component ")) => {
                    let mut rest: Vec<Doc<'a>> = vec![self.s("component ")];
                    rest.extend(parts[1..].iter().copied());
                    self.docs.concat_vec(rest)
                }
                _ => doc,
            },
            _ => doc,
        }
    }

    /// `declare module "m" { … }` / `declare module M { … }`.
    pub fn print_declare_module(
        &mut self,
        module: &'a statement::DeclareModule<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let _ = key;
        let id = match &module.id {
            statement::declare_module::Id::Identifier(id) => self.print_identifier(id),
            statement::declare_module::Id::Literal((loc, literal)) => {
                self.print_string_node(loc, literal)
            }
        };
        let body = self.print_block(&module.body.0, &module.body.1, EmptyBlock::Open);
        self.concat([self.s("declare module "), id, self.s(" "), body])
    }

    /// `declare namespace N { … }`.
    pub fn print_declare_namespace(
        &mut self,
        namespace: &'a statement::DeclareNamespace<Loc, Loc>,
    ) -> Doc<'a> {
        let keyword = match namespace.keyword {
            statement::declare_namespace::Keyword::Namespace => "namespace ",
            statement::declare_namespace::Keyword::Module => "module ",
        };
        let id = match &namespace.id {
            statement::declare_namespace::Id::Global(id)
            | statement::declare_namespace::Id::Local(id) => self.print_identifier(id),
        };
        let body = self.print_block(&namespace.body.0, &namespace.body.1, EmptyBlock::Open);
        let declare = if namespace.implicit_declare {
            self.s("")
        } else {
            self.s("declare ")
        };
        self.concat([declare, self.s(keyword), id, self.s(" "), body])
    }
}

/// Unused import guard.
#[allow(dead_code)]
fn softline_guard() -> Doc<'static> {
    &SOFTLINE
}
