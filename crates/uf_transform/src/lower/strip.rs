//! Every type is erased.
//!
//! A port of `hermes-parser`'s `StripFlowTypes.js`, extended to cover the
//! declaration forms that transform only strips for Babel (`declare enum`,
//! `declare namespace`, `declare component`, `declare hook`): after this pass
//! there is no type left in the tree for anything downstream to know about.
//!
//! One deliberate departure: upstream drops an `import "x";` with no
//! specifiers as if it had been all type imports. A bare import is a side
//! effect the author asked for, so it stays.
//!
//! And one addition upstream has no reason for: the `"use flow"` and
//! `"use js"` directives go too. They say how the type checker reads the
//! module — a source mode, the way `// @flow` used to — and nothing at
//! runtime reads them. Left in, they reach the bundler as directives it does
//! not know, and Rolldown warns once per module (`MODULE_LEVEL_DIRECTIVE`) and
//! still leaves the string at the top of the chunk. Every other directive in
//! the prologue stays where it was, in the order it was written: `"use client"`
//! and `"use server"` are what React's bundler integration reads, and
//! `"use strict"` changes what the module means.

use serde_json::Value;
use uf_profiler::profile_span;

use super::{Edit, list_field, node_type, str_field, take, transform_post};
use crate::TransformError;

/// Statement forms that exist only for the type checker.
const TYPE_ONLY_STATEMENTS: [&str; 19] = [
    "TypeAlias",
    "OpaqueType",
    "InterfaceDeclaration",
    "DeclareClass",
    "DeclareComponent",
    "DeclareEnum",
    "DeclareExportAllDeclaration",
    "DeclareExportDeclaration",
    "DeclareFunction",
    "DeclareHook",
    "DeclareInterface",
    "DeclareModule",
    "DeclareModuleExports",
    "DeclareNamespace",
    "DeclareOpaqueType",
    "DeclareTypeAlias",
    "DeclareVariable",
    "TypeParameterDeclaration",
    "TypeParameterInstantiation",
];

/// Fields that hold type syntax on any node that has them.
const TYPE_FIELDS: [&str; 9] = [
    "typeAnnotation",
    "typeArguments",
    "typeParameters",
    "returnType",
    "predicate",
    "variance",
    "rendersType",
    "superTypeArguments",
    "implements",
];

/// Directives that are only for the type checker: a module's source mode.
///
/// `"use js"` opts a module out of Flow and `"use flow"` opts it in; with
/// either spelling, single or double quotes, the program means the same thing
/// to the engine that runs it.
const SOURCE_MODE_DIRECTIVES: [&str; 2] = ["use flow", "use js"];

/// Erase every type in `program`.
///
/// The source-mode directives go in [`remove_source_mode_directives`], which
/// `super::lower` runs before any pass that adds statements to the top.
pub fn lower(program: &mut Value) -> Result<(), TransformError> {
    profile_span!("lower::strip");
    transform_post(program, &mut |node| Ok(strip_node(node)))?;
    Ok(())
}

/// Drop `"use flow"` and `"use js"` from the program's directive prologue.
///
/// Only the program's: a function body that opens with the same string is not
/// a source mode, and the checker does not read it as one. Only the prologue's
/// too, which is the statements the parser marked `directive`, read from the
/// top until the first that is not one. A later `"use flow";` is an ordinary
/// expression statement, and `const s = "use flow";` is a string.
///
/// The value compared is the directive's source text between its quotes, as
/// ECMAScript compares a directive: `"use\x20flow"` is not the directive.
pub fn remove_source_mode_directives(program: &mut Value) {
    let Some(body) = program.get_mut("body").and_then(Value::as_array_mut) else {
        return;
    };
    let prologue = body
        .iter()
        .take_while(|statement| str_field(statement, "directive").is_some())
        .count();
    let mut index = 0usize;
    body.retain(|statement| {
        let in_prologue = index < prologue;
        index += 1;
        !(in_prologue
            && str_field(statement, "directive")
                .is_some_and(|directive| SOURCE_MODE_DIRECTIVES.contains(&directive)))
    });
}

/// The node types [`strip_node`] treats specially, as a value rather than a name.
///
/// The classification happens while the type is still borrowed from the node,
/// so nothing is owned. Reading it as a `&str` and keeping it across the arms
/// below — which all need the node mutably — is what `str::to_owned` was
/// paying for: one `String` per node of every module, for a name that is
/// compared and dropped. See ubugeeei-prod/uf#668.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Strip {
    Cast,
    Class,
    Export,
    ExportAll,
    Function,
    Import,
    Pattern,
    PropertyDefinition,
    TypeOnly,
    Other,
}

impl Strip {
    fn of(kind: &str) -> Self {
        if TYPE_ONLY_STATEMENTS.contains(&kind) {
            return Self::TypeOnly;
        }
        match kind {
            "AsExpression" | "AsConstExpression" | "TypeCastExpression" | "SatisfiesExpression" => {
                Self::Cast
            }
            "ClassDeclaration" | "ClassExpression" => Self::Class,
            "ExportNamedDeclaration" => Self::Export,
            "ExportAllDeclaration" => Self::ExportAll,
            "FunctionDeclaration" | "FunctionExpression" | "ArrowFunctionExpression" => {
                Self::Function
            }
            "ImportDeclaration" => Self::Import,
            "Identifier" | "ObjectPattern" | "ArrayPattern" | "RestElement"
            | "AssignmentPattern" => Self::Pattern,
            "PropertyDefinition" => Self::PropertyDefinition,
            _ => Self::Other,
        }
    }
}

fn strip_node(node: &mut Value) -> Edit {
    let Some(kind) = node_type(node).map(Strip::of) else {
        return Edit::Keep;
    };
    if kind == Strip::TypeOnly {
        return Edit::Remove;
    }

    match kind {
        Strip::Cast => {
            return Edit::Replace(take(node, "expression"));
        }
        Strip::Import => {
            if matches!(str_field(node, "importKind"), Some("type" | "typeof")) {
                return Edit::Remove;
            }
            let had_specifiers = !list_field(node, "specifiers").is_empty();
            if let Some(specifiers) = node.get_mut("specifiers").and_then(Value::as_array_mut) {
                specifiers.retain(|specifier| {
                    !matches!(str_field(specifier, "importKind"), Some("type" | "typeof"))
                });
                for specifier in specifiers.iter_mut() {
                    remove_key(specifier, "importKind");
                }
                if had_specifiers && specifiers.is_empty() {
                    return Edit::Remove;
                }
            }
            remove_key(node, "importKind");
        }
        Strip::ExportAll => {
            if str_field(node, "exportKind") == Some("type") {
                return Edit::Remove;
            }
            remove_key(node, "exportKind");
        }
        Strip::Export => {
            if str_field(node, "exportKind") == Some("type") {
                return Edit::Remove;
            }
            let had_specifiers = !list_field(node, "specifiers").is_empty();
            if let Some(specifiers) = node.get_mut("specifiers").and_then(Value::as_array_mut) {
                specifiers.retain(|specifier| str_field(specifier, "exportKind") != Some("type"));
                for specifier in specifiers.iter_mut() {
                    remove_key(specifier, "exportKind");
                }
            }
            let declaration_gone = node.get("declaration").is_none_or(Value::is_null);
            let no_specifiers = list_field(node, "specifiers").is_empty();
            let no_source = node.get("source").is_none_or(Value::is_null);
            if declaration_gone && no_specifiers && (had_specifiers || no_source) {
                return Edit::Remove;
            }
            remove_key(node, "exportKind");
        }
        Strip::Function => {
            strip_function(node);
        }
        Strip::Class => {
            for key in [
                "typeParameters",
                "superTypeArguments",
                "superTypeParameters",
                "implements",
            ] {
                remove_key(node, key);
            }
        }
        Strip::PropertyDefinition => {
            if node.get("declare").and_then(Value::as_bool) == Some(true) {
                return Edit::Remove;
            }
            for key in ["typeAnnotation", "variance", "optional", "declare"] {
                remove_key(node, key);
            }
        }
        Strip::Pattern => {
            remove_key(node, "typeAnnotation");
            remove_key(node, "optional");
        }
        _ => {}
    }

    for key in TYPE_FIELDS {
        remove_key(node, key);
    }
    Edit::Keep
}

fn strip_function(node: &mut Value) {
    if let Some(params) = node.get_mut("params").and_then(Value::as_array_mut)
        && params.first().is_some_and(|first| {
            node_type(first) == Some("Identifier") && str_field(first, "name") == Some("this")
        })
    {
        params.remove(0);
    }
    for key in ["returnType", "typeParameters", "predicate"] {
        remove_key(node, key);
    }
}

fn remove_key(node: &mut Value, key: &str) {
    if let Some(object) = node.as_object_mut() {
        object.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;

    /// What `super::lower` does with these two: the directives first, then the types.
    fn stripped(source: &str) -> Value {
        let mut program = parse(source).unwrap();
        remove_source_mode_directives(&mut program);
        lower(&mut program).unwrap();
        program
    }

    fn types(program: &Value) -> Vec<&str> {
        program["body"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node_type(node).unwrap())
            .collect()
    }

    /// The prologue's directives, in order.
    fn directives(program: &Value) -> Vec<&str> {
        program["body"]
            .as_array()
            .unwrap()
            .iter()
            .map_while(|statement| str_field(statement, "directive"))
            .collect()
    }

    #[test]
    fn the_source_mode_directive_goes_and_use_client_stays_first() {
        let after = stripped("\"use client\";\n\"use flow\";\nexport const x: number = 1;\n");
        assert_eq!(directives(&after), ["use client"]);
        let before = stripped("\"use flow\";\n\"use client\";\nexport const x: number = 1;\n");
        assert_eq!(directives(&before), ["use client"]);
        assert_eq!(
            types(&before),
            ["ExpressionStatement", "ExportNamedDeclaration"]
        );
    }

    #[test]
    fn every_spelling_of_a_source_mode_goes_and_other_directives_keep_their_order() {
        let program = stripped(
            "'use strict';\n'use flow'\n\"use server\";\n\"use js\";\n'use js';\nexport async function save() {}\n",
        );
        assert_eq!(directives(&program), ["use strict", "use server"]);
        assert_eq!(
            types(&program),
            [
                "ExpressionStatement",
                "ExpressionStatement",
                "ExportNamedDeclaration"
            ]
        );
    }

    #[test]
    fn the_words_outside_the_program_s_prologue_are_left_alone() {
        let program = stripped(
            "\"use flow\";\nconst s = \"use flow\";\nf();\n\"use flow\";\nfunction g() {\n  \"use flow\";\n}\n",
        );
        assert!(directives(&program).is_empty());
        assert_eq!(
            types(&program),
            [
                "VariableDeclaration",
                "ExpressionStatement",
                "ExpressionStatement",
                "FunctionDeclaration"
            ]
        );
        assert_eq!(
            program["body"][0]["declarations"][0]["init"]["value"],
            "use flow"
        );
        assert_eq!(program["body"][2]["expression"]["value"], "use flow");
        assert_eq!(
            program["body"][3]["body"]["body"][0]["directive"],
            "use flow"
        );
    }

    #[test]
    fn an_escaped_spelling_is_not_the_directive() {
        let program = stripped("\"use\\x20flow\";\nexport const x = 1;\n");
        assert_eq!(directives(&program), ["use\\x20flow"]);
    }

    #[test]
    fn type_declarations_disappear() {
        let program = stripped(
            "type A = string;\nopaque type B = number;\ninterface C {}\ndeclare var d: number;\ndeclare function f(): void;\nexport type {A};\nconst x = 1;\n",
        );
        assert_eq!(types(&program), ["VariableDeclaration"]);
    }

    #[test]
    fn type_imports_disappear_and_value_imports_stay() {
        let program = stripped(
            "import type {A} from './a';\nimport typeof B from './b';\nimport {type C, d} from './c';\nimport {type E} from './e';\nimport './side-effect';\n",
        );
        let body = program["body"].as_array().unwrap();
        assert_eq!(body.len(), 2);
        assert_eq!(body[0]["specifiers"].as_array().unwrap().len(), 1);
        assert_eq!(body[0]["specifiers"][0]["local"]["name"], "d");
        assert_eq!(body[1]["specifiers"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn annotations_and_casts_are_erased() {
        let program = stripped(
            "function f(this: T, a: string, b?: number = 1): void {}\nconst y = (x: any);\nconst z = x as number;\nclass K<T> extends B<T> implements I { p: string = ''; declare q: number; }\ncall<T>(a);\n",
        );
        let function = &program["body"][0];
        assert_eq!(function["params"].as_array().unwrap().len(), 2);
        assert!(function.get("returnType").is_none());
        assert!(function["params"][0].get("typeAnnotation").is_none());
        assert_eq!(
            program["body"][1]["declarations"][0]["init"]["type"],
            "Identifier"
        );
        assert_eq!(
            program["body"][2]["declarations"][0]["init"]["type"],
            "Identifier"
        );
        let class = &program["body"][3];
        assert!(class.get("implements").is_none());
        assert!(class.get("typeParameters").is_none());
        assert_eq!(class["body"]["body"].as_array().unwrap().len(), 1);
        assert!(
            program["body"][4]["expression"]
                .get("typeArguments")
                .is_none()
        );
    }
}
