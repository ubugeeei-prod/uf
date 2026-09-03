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

use serde_json::Value;

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

/// Erase every type in `program`.
pub fn lower(program: &mut Value) -> Result<(), TransformError> {
    transform_post(program, &mut |node| Ok(strip_node(node)))?;
    Ok(())
}

fn strip_node(node: &mut Value) -> Edit {
    let Some(kind) = node_type(node).map(str::to_owned) else {
        return Edit::Keep;
    };
    if TYPE_ONLY_STATEMENTS.contains(&kind.as_str()) {
        return Edit::Remove;
    }

    match kind.as_str() {
        "AsExpression" | "AsConstExpression" | "TypeCastExpression" | "SatisfiesExpression" => {
            return Edit::Replace(take(node, "expression"));
        }
        "ImportDeclaration" => {
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
        "ExportAllDeclaration" => {
            if str_field(node, "exportKind") == Some("type") {
                return Edit::Remove;
            }
            remove_key(node, "exportKind");
        }
        "ExportNamedDeclaration" => {
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
        "FunctionDeclaration" | "FunctionExpression" | "ArrowFunctionExpression" => {
            strip_function(node);
        }
        "ClassDeclaration" | "ClassExpression" => {
            for key in [
                "typeParameters",
                "superTypeArguments",
                "superTypeParameters",
                "implements",
            ] {
                remove_key(node, key);
            }
        }
        "PropertyDefinition" => {
            if node.get("declare").and_then(Value::as_bool) == Some(true) {
                return Edit::Remove;
            }
            for key in ["typeAnnotation", "variance", "optional", "declare"] {
                remove_key(node, key);
            }
        }
        "Identifier" | "ObjectPattern" | "ArrayPattern" | "RestElement" | "AssignmentPattern" => {
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

    fn stripped(source: &str) -> Value {
        let mut program = parse(source).unwrap();
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
