//! Which declaration a line of a declaration file belongs to.
//!
//! A translation keeps every statement and member on the line it was written
//! on, so an error Flow reports at `v4/core/schemas.d.cts.flow:792` is about
//! line 792 of `v4/core/schemas.d.cts`. A line number sends a reader to open
//! the file. `uf check --explain-any` names the declaration instead, the way a
//! [`crate::Hole`] names its own: outermost name first, joined with `.`, down
//! to the member the line is in — `util.Shape.parse`, `ZodType.register`.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, ExportDefaultDeclarationKind, PropertyKey, Statement,
    TSInterfaceDeclaration, TSNamespaceDeclaration, TSNamespaceDeclarationBody, TSSignature,
    TSType,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};

use crate::emit::declaration_name;

/// The declaration that line `line` (one-based) of the declaration file
/// `source` is in, or [`None`] when the line is in none — a blank line, a
/// comment between declarations, an import — or the file does not parse.
///
/// The innermost thing with a name wins: a namespace, then the declaration
/// inside it, then the member of an interface, class or object type the line
/// is in. A line holding two declarations is named for the first.
#[must_use]
pub fn declaration_at(source: &str, line: u32) -> Option<String> {
    let bounds = line_bounds(source, line)?;
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
    if parsed.panicked {
        return None;
    }
    let mut path = Vec::new();
    in_statements(&parsed.program.body, bounds, &mut path);
    (!path.is_empty()).then(|| path.join("."))
}

/// The byte range line `line` (one-based) covers, newline included.
fn line_bounds(source: &str, line: u32) -> Option<(u32, u32)> {
    let index = usize::try_from(line.checked_sub(1)?).ok()?;
    let mut start = 0usize;
    for (number, text) in source.split_inclusive('\n').enumerate() {
        let end = start + text.len();
        if number == index {
            return Some((u32::try_from(start).ok()?, u32::try_from(end).ok()?));
        }
        start = end;
    }
    None
}

/// Whether `span` shares a byte with the line.
fn overlaps(span: Span, (start, end): (u32, u32)) -> bool {
    span.start < end && span.end > start
}

fn in_statements(statements: &[Statement<'_>], bounds: (u32, u32), path: &mut Vec<String>) {
    let Some(statement) = statements
        .iter()
        .find(|statement| overlaps(statement.span(), bounds))
    else {
        return;
    };
    match statement {
        Statement::ExportDeclaration(export) => in_declaration(&export.declaration, bounds, path),
        Statement::ExportDefaultDeclaration(default) => match &default.declaration {
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                in_interface(interface, bounds, path);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                in_class(class, bounds, path);
            }
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => path.push(
                function
                    .id
                    .as_ref()
                    .map_or_else(|| "default".to_owned(), |id| id.name.as_str().to_owned()),
            ),
            _ => path.push("default".to_owned()),
        },
        other => {
            if let Some(declaration) = other.as_declaration() {
                in_declaration(declaration, bounds, path);
            }
        }
    }
}

fn in_declaration(declaration: &Declaration<'_>, bounds: (u32, u32), path: &mut Vec<String>) {
    match declaration {
        Declaration::TSNamespaceDeclaration(namespace) => in_namespace(namespace, bounds, path),
        Declaration::TSInterfaceDeclaration(interface) => in_interface(interface, bounds, path),
        Declaration::ClassDeclaration(class) => in_class(class, bounds, path),
        Declaration::TSTypeAliasDeclaration(alias) => {
            path.push(alias.id.name.as_str().to_owned());
            if let TSType::TSTypeLiteral(literal) = &alias.type_annotation {
                in_signatures(&literal.members, bounds, path);
            }
        }
        Declaration::TSGlobalDeclaration(global) => {
            path.push("global".to_owned());
            in_statements(&global.body.body, bounds, path);
        }
        Declaration::TSExternalModuleDeclaration(module) => {
            path.push(format!("module \"{}\"", module.id.value));
        }
        other => {
            if let Some(name) = declaration_name(other) {
                path.push(name.to_owned());
            }
        }
    }
}

fn in_namespace(
    namespace: &TSNamespaceDeclaration<'_>,
    bounds: (u32, u32),
    path: &mut Vec<String>,
) {
    path.push(namespace.id.name.as_str().to_owned());
    match &namespace.body {
        TSNamespaceDeclarationBody::TSModuleBlock(block) => {
            in_statements(&block.body, bounds, path)
        }
        TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
            if overlaps(inner.span, bounds) {
                in_namespace(inner, bounds, path);
            }
        }
    }
}

fn in_interface(
    interface: &TSInterfaceDeclaration<'_>,
    bounds: (u32, u32),
    path: &mut Vec<String>,
) {
    path.push(interface.id.name.as_str().to_owned());
    in_signatures(&interface.body.body, bounds, path);
}

fn in_class(class: &Class<'_>, bounds: (u32, u32), path: &mut Vec<String>) {
    path.push(
        class
            .id
            .as_ref()
            .map_or_else(|| "default".to_owned(), |id| id.name.as_str().to_owned()),
    );
    let member = class
        .body
        .body
        .iter()
        .find(|element| overlaps(element.span(), bounds))
        .and_then(|element| match element {
            ClassElement::MethodDefinition(method) => key_name(&method.key),
            ClassElement::PropertyDefinition(property) => key_name(&property.key),
            ClassElement::AccessorProperty(accessor) => key_name(&accessor.key),
            _ => None,
        });
    path.extend(member);
}

fn in_signatures(members: &[TSSignature<'_>], bounds: (u32, u32), path: &mut Vec<String>) {
    let member = members
        .iter()
        .find(|member| overlaps(member.span(), bounds))
        .and_then(|member| match member {
            TSSignature::TSPropertySignature(property) => key_name(&property.key),
            TSSignature::TSMethodSignature(method) => key_name(&method.key),
            _ => None,
        });
    path.extend(member);
}

/// A member key a reader would write after a `.`: an identifier, or the
/// contents of a string key.
fn key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Some(identifier.name.as_str().to_owned()),
        PropertyKey::StringLiteral(literal) => Some(literal.value.as_str().to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "\
export declare namespace util {
  type Identity<T> = T;
  interface Shape {
    name: string;
    parse(input: unknown): string;
  }
}
export interface ZodType<T> {
  register(registry: unknown): this;
}
export type Options = {
  strict: boolean;
};
export declare class Registry {
  add(item: unknown): void;
}
export declare const version: string;
";

    #[test]
    fn a_line_inside_a_namespace_names_the_namespace_first() {
        assert_eq!(declaration_at(SOURCE, 2).as_deref(), Some("util.Identity"));
        assert_eq!(
            declaration_at(SOURCE, 4).as_deref(),
            Some("util.Shape.name")
        );
        assert_eq!(
            declaration_at(SOURCE, 5).as_deref(),
            Some("util.Shape.parse")
        );
    }

    #[test]
    fn a_line_inside_a_member_names_the_member() {
        assert_eq!(
            declaration_at(SOURCE, 9).as_deref(),
            Some("ZodType.register")
        );
        assert_eq!(
            declaration_at(SOURCE, 12).as_deref(),
            Some("Options.strict")
        );
        assert_eq!(declaration_at(SOURCE, 15).as_deref(), Some("Registry.add"));
    }

    #[test]
    fn a_line_that_opens_a_declaration_names_the_declaration() {
        assert_eq!(declaration_at(SOURCE, 8).as_deref(), Some("ZodType"));
        assert_eq!(declaration_at(SOURCE, 17).as_deref(), Some("version"));
    }

    #[test]
    fn a_line_in_no_declaration_names_nothing() {
        assert_eq!(declaration_at(SOURCE, 0), None);
        assert_eq!(declaration_at(SOURCE, 400), None);
        assert_eq!(declaration_at("\n\nexport declare const x: 1;\n", 1), None);
    }
}
