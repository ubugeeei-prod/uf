//! Flow enums become runtime objects.
//!
//! A port of `hermes-parser`'s `TransformEnumSyntax.js`, with one difference:
//! upstream emits `require("flow-enums-runtime")`, which is a CommonJS call
//! that a browser or an ES module cannot make. uf prepends the equivalent of
//! that runtime to the module instead, once, only when the module declares an
//! enum. The helper reproduces `flow-enums-runtime`'s contract — `cast`,
//! `isValid`, `members`, `getName` as non-enumerable methods, members frozen
//! and enumerable — so an enum behaves exactly as Flow documents.
//!
//! ```text
//! enum Status { Active, Off }            → const Status = $$ufEnumMirrored(["Active", "Off"]);
//! enum Code of number { A = 1, B = 2 }   → const Code = $$ufEnum({ A: 1, B: 2 });
//! enum Sym of symbol { X, Y }            → const Sym = $$ufEnum({ X: Symbol("X"), Y: Symbol("Y") });
//! ```

use serde_json::{Value, json};

use super::builders::{call, ident, string_literal, variable_declaration};
use super::{Edit, bool_field, list_field, node_type, str_field, take, transform_post};
use crate::TransformError;

/// The runtime prepended to a module that declares an enum.
///
/// Kept as source rather than hand-built nodes so it reads as the JavaScript
/// it is; it is parsed with the same parser as everything else.
pub const RUNTIME_SOURCE: &str = r#"
function $$ufEnum(members) {
  const byValue = new Map();
  for (const name of Object.keys(members)) byValue.set(members[name], name);
  const value = Object.assign(Object.create(null), members);
  Object.defineProperties(value, {
    isValid: { value: (candidate) => byValue.has(candidate) },
    cast: { value: (candidate) => (byValue.has(candidate) ? candidate : undefined) },
    members: { value: () => Object.values(members)[Symbol.iterator]() },
    getName: { value: (candidate) => byValue.get(candidate) },
  });
  return Object.freeze(value);
}
function $$ufEnumMirrored(names) {
  const members = {};
  for (const name of names) members[name] = name;
  return $$ufEnum(members);
}
"#;

/// Lower every enum declaration in `program`, prepending the runtime when
/// at least one was found.
pub fn lower(program: &mut Value, _source: &str) -> Result<(), TransformError> {
    let mut found = false;
    transform_post(program, &mut |node| {
        Ok(match node_type(node) {
            Some("EnumDeclaration") => {
                found = true;
                Edit::Replace(enum_to_declaration(node))
            }
            // Children are lowered first, so by the time the export is
            // visited its enum is already a `const` — one that carries a
            // marker saying so.
            Some("ExportDefaultDeclaration") if bool_field(&node["declaration"], "__ufEnum") => {
                let mut lowered = take(node, "declaration");
                if let Some(object) = lowered.as_object_mut() {
                    object.remove("__ufEnum");
                }
                let name = str_field(&lowered["declarations"][0]["id"], "name")
                    .unwrap_or("")
                    .to_owned();
                let mut export = node.clone();
                export["declaration"] = ident(&name);
                Edit::Splice(vec![lowered, export])
            }
            _ => Edit::Keep,
        })
    })?;

    if found {
        let mut runtime = crate::estree::parse(RUNTIME_SOURCE)?;
        // Parsed from `RUNTIME_SOURCE`, so every node in it carries a position
        // in *that* text — which the printer would otherwise record against the
        // author's module. See `builders::forget_positions`.
        super::builders::forget_positions(&mut runtime);
        let helpers = runtime["body"].as_array().cloned().unwrap_or_default();
        if let Some(body) = program.get_mut("body").and_then(Value::as_array_mut) {
            let rest = std::mem::take(body);
            body.extend(helpers);
            body.extend(rest);
        }
    }
    Ok(())
}

fn enum_to_declaration(node: &mut Value) -> Value {
    let id = take(node, "id");
    let body = take(node, "body");
    let members = list_field(&body, "members");
    let explicit = str_field(&body, "explicitType");
    let mirrored = matches!(explicit, None | Some("string"))
        && members
            .first()
            .is_none_or(|member| node_type(member) == Some("EnumDefaultedMember"));

    let init = if mirrored {
        let names: Vec<Value> = members
            .iter()
            .map(|member| string_literal(str_field(&member["id"], "name").unwrap_or("")))
            .collect();
        call(
            ident("$$ufEnumMirrored"),
            vec![json!({ "type": "ArrayExpression", "elements": names })],
        )
    } else {
        let properties: Vec<Value> = members
            .iter()
            .map(|member| {
                let name = str_field(&member["id"], "name").unwrap_or("");
                let value = if node_type(member) == Some("EnumDefaultedMember") {
                    // Only a symbol enum defaults a member without mirroring.
                    call(ident("Symbol"), vec![string_literal(name)])
                } else {
                    member["init"].clone()
                };
                super::builders::property(ident(name), value, false, false)
            })
            .collect();
        call(
            ident("$$ufEnum"),
            vec![json!({ "type": "ObjectExpression", "properties": properties })],
        )
    };
    let mut declaration = super::with_position_of(variable_declaration("const", id, init), node);
    declaration["__ufEnum"] = Value::Bool(true);
    declaration
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;

    fn lowered(source: &str) -> Value {
        let mut program = parse(source).unwrap();
        lower(&mut program, source).unwrap();
        program
    }

    #[test]
    fn a_string_enum_without_initialisers_mirrors_its_names() {
        let program = lowered("enum Status { Active, Off }\n");
        let body = program["body"].as_array().unwrap();
        // Two helper functions, then the declaration.
        assert_eq!(body.len(), 3);
        let init = &body[2]["declarations"][0]["init"];
        assert_eq!(init["callee"]["name"], "$$ufEnumMirrored");
        assert_eq!(init["arguments"][0]["elements"][1]["value"], "Off");
    }

    #[test]
    fn an_initialised_enum_keeps_its_values() {
        let program = lowered("export enum Code of number { A = 1, B = 2 }\n");
        let init = &program["body"][2]["declaration"]["declarations"][0]["init"];
        assert_eq!(init["callee"]["name"], "$$ufEnum");
        assert_eq!(init["arguments"][0]["properties"][1]["value"]["value"], 2);
    }

    #[test]
    fn a_default_export_is_split_into_a_declaration_and_an_export() {
        let program = lowered("export default enum E { A }\n");
        let body = program["body"].as_array().unwrap();
        assert_eq!(body[2]["type"], "VariableDeclaration");
        assert_eq!(body[3]["type"], "ExportDefaultDeclaration");
        assert_eq!(body[3]["declaration"]["name"], "E");
    }

    /// The enum runtime is parsed from `RUNTIME_SOURCE`, so every node in it
    /// arrives carrying a position — in a text that is not the author's file.
    /// Recording those against the module put a page of source map on lines
    /// the author never wrote, at columns their file does not have: a debugger
    /// stepping into `$$ufEnum` landed somewhere in the user's code, and a
    /// coverage report counted the runtime's four `value:` arrows as four
    /// uncovered functions of theirs.
    #[test]
    fn the_enum_runtime_is_mapped_to_nothing_at_all() {
        let source = "// @flow\nexport enum Size { Small, Large }\nexport const x: number = 1;\n";
        let transformed = crate::transform(source, &crate::TransformOptions::new("/x/badge.js"))
            .expect("the module transforms");
        let map: Value =
            serde_json::from_str(transformed.map.as_ref().expect("a map was asked for"))
                .expect("the map is JSON");
        let mappings = map["mappings"].as_str().expect("mappings are a string");

        // One group per generated line, empty when that line maps to nothing.
        let mapped: Vec<bool> = mappings.split(';').map(|group| !group.is_empty()).collect();
        for (line, text) in transformed.code.lines().enumerate() {
            let is_runtime = text.contains("$$ufEnum") && !text.contains("export const");
            if is_runtime || text.trim().is_empty() {
                continue;
            }
            // Everything the author wrote still maps.
            if text.contains("export const") {
                assert!(
                    mapped.get(line).copied().unwrap_or(false),
                    "line {line}: {text}"
                );
            }
        }
        // Nothing before the first line of the author's own code maps at all.
        let first_authored = transformed
            .code
            .lines()
            .position(|line| line.contains("export const Size"))
            .expect("the lowered declaration");
        assert!(
            mapped[..first_authored].iter().all(|mapped| !mapped),
            "the runtime maps to the author's file: {mappings}"
        );
    }

    #[test]
    fn a_module_without_enums_gets_no_runtime() {
        let program = lowered("const a = 1;\n");
        assert_eq!(program["body"].as_array().unwrap().len(), 1);
    }
}
