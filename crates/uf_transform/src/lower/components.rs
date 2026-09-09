//! `component` and `hook` declarations become functions.
//!
//! A port of `hermes-parser`'s `TransformComponentSyntax.js`, for React 19:
//! `ref` is an ordinary prop there, so no `forwardRef` wrapper is ever built.
//! The lowered function carries `__componentDeclaration` or
//! `__hookDeclaration`, which is how the official React Compiler's `syntax`
//! mode knows which functions to compile.
//!
//! ```text
//! component Foo(a: string, b?: number = 1, ...rest: Rest) { … }
//! function Foo({ a, b = 1, ...rest }) { … }
//!
//! component Bar(...props: Props) { … }
//! function Bar(props) { … }
//!
//! hook useX(v: T): number { … }
//! function useX(v) { … }
//! ```

use serde_json::Value;
use uf_profiler::profile_span;

use super::builders::{property, rest_element};
use super::{Edit, bool_field, node_type, refuse, str_field, take, transform_post};
use crate::TransformError;

/// Lower every component and hook declaration in `program`.
///
/// Returns whether any was found.
pub fn lower(program: &mut Value) -> Result<bool, TransformError> {
    profile_span!("lower::components");
    let mut found = false;
    transform_post(program, &mut |node| {
        Ok(match node_type(node) {
            Some("ComponentDeclaration") => {
                found = true;
                Edit::Replace(component_to_function(node)?)
            }
            Some("HookDeclaration") => {
                found = true;
                Edit::Replace(hook_to_function(node))
            }
            _ => Edit::Keep,
        })
    })?;
    Ok(found)
}

fn component_to_function(node: &mut Value) -> Result<Value, TransformError> {
    profile_span!("components::to_function");
    let params = component_parameters(node)?;
    let mut function = node! {
        "type": "FunctionDeclaration",
        "id": take(node, "id"),
        "params": params,
        "body": take(node, "body"),
        "async": bool_field(node, "async"),
        "generator": false,
        "__componentDeclaration": true,
    };
    copy_position(&mut function, node);
    Ok(function)
}

fn hook_to_function(node: &mut Value) -> Value {
    profile_span!("components::hook_to_function");
    let mut function = node! {
        "type": "FunctionDeclaration",
        "id": take(node, "id"),
        "params": take(node, "params"),
        "body": take(node, "body"),
        "async": bool_field(node, "async"),
        "generator": false,
        "__hookDeclaration": true,
    };
    copy_position(&mut function, node);
    function
}

fn copy_position(target: &mut Value, source: &Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for key in ["loc", "range"] {
            if let Some(value) = source.get(key) {
                target.insert(key.to_owned(), value.clone());
            }
        }
    }
}

/// The parameter list of the lowered function.
///
/// Mirrors `mapComponentParameters` with `reactRuntimeTarget: "19"`: no
/// parameters stay no parameters; a lone `...props: Props` becomes the single
/// parameter `props`; anything else becomes one destructuring pattern.
fn component_parameters(node: &mut Value) -> Result<Vec<Value>, TransformError> {
    profile_span!("components::parameters");
    // Taken, not borrowed and cloned. `take` has already moved the list out of
    // the node, so the parameters here are this function's own — and every one
    // of them is either moved into the pattern below or dropped. Cloning them
    // first copied each parameter's `typeAnnotation` subtree in full, only for
    // `strip_pattern` to remove it a few lines later: 5,374 allocations per
    // `component` declaration, which was 13% of what `uf lint` spent on
    // `packages/router/internal/runtime.js`. See ubugeeei-prod/uf#668.
    let Value::Array(params) = take(node, "params") else {
        return Ok(Vec::new());
    };
    if params.is_empty() {
        return Ok(Vec::new());
    }

    if params.len() == 1
        && node_type(&params[0]) == Some("RestElement")
        && node_type(&params[0]["argument"]) == Some("Identifier")
    {
        let mut only = params;
        return Ok(vec![strip_pattern(take(&mut only[0], "argument"))]);
    }

    let mut properties = Vec::with_capacity(params.len());
    for mut param in params {
        match node_type(&param) {
            Some("RestElement") => {
                let mut argument = take(&mut param, "argument");
                match node_type(&argument) {
                    Some("Identifier") => {
                        properties.push(rest_element(strip_pattern(argument)));
                    }
                    Some("ObjectPattern") => {
                        if let Value::Array(entries) = take(&mut argument, "properties") {
                            for entry in entries {
                                properties.push(strip_pattern(entry));
                            }
                        }
                    }
                    other => {
                        return Err(refuse(
                            &param,
                            format!(
                                "unhandled {} encountered in component rest parameter",
                                other.unwrap_or("node")
                            ),
                        ));
                    }
                }
            }
            Some("ComponentParameter") => {
                let name = take(&mut param, "name");
                let value = strip_pattern(take(&mut param, "local"));
                let shorthand = node_type(&name) == Some("Identifier")
                    && bool_field(&param, "shorthand")
                    && matches!(node_type(&value), Some("Identifier" | "AssignmentPattern"));
                let mut entry = property(name, value, false, shorthand);
                copy_position(&mut entry, &param);
                properties.push(entry);
            }
            other => {
                return Err(refuse(
                    &param,
                    format!(
                        "unknown component parameter type {:?}",
                        other.unwrap_or("node")
                    ),
                ));
            }
        }
    }

    // Read out of the first and last property rather than cloning them whole:
    // all that is wanted is four numbers, and a property carries the whole
    // subtree the parameter had.
    let range = match (
        properties.first().and_then(|first| first.get("range")),
        properties.last().and_then(|last| last.get("range")),
    ) {
        (Some(first), Some(last)) => Some(Value::Array(vec![first[0].clone(), last[1].clone()])),
        _ => None,
    };
    let loc = match (
        properties.first().and_then(|first| first.get("loc")),
        properties.last().and_then(|last| last.get("loc")),
    ) {
        (Some(first), Some(last)) => {
            Some(node! { "start": first["start"].clone(), "end": last["end"].clone() })
        }
        _ => None,
    };

    let mut pattern = node! { "type": "ObjectPattern", "properties": properties };
    if let Some(range) = range {
        pattern["range"] = range;
    }
    if let Some(loc) = loc {
        pattern["loc"] = loc;
    }
    Ok(vec![pattern])
}

/// A binding pattern with its annotation and optional marker removed.
///
/// An `AssignmentPattern` keeps its default; only its left side is stripped.
fn strip_pattern(mut pattern: Value) -> Value {
    if let Some(object) = pattern.as_object_mut() {
        object.remove("typeAnnotation");
        object.remove("optional");
        if object.get("type").and_then(Value::as_str) == Some("AssignmentPattern")
            && let Some(left) = object.get_mut("left")
        {
            let stripped = strip_pattern(left.take());
            *left = stripped;
        }
        if str_field(&pattern, "type") == Some("RestElement")
            && let Some(argument) = pattern.get_mut("argument")
        {
            let stripped = strip_pattern(argument.take());
            *argument = stripped;
        }
    }
    pattern
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;

    fn lowered(source: &str) -> Value {
        let mut program = parse(source).unwrap();
        lower(&mut program).unwrap();
        program
    }

    #[test]
    fn a_component_becomes_a_flagged_function_with_destructured_props() {
        let program =
            lowered("component Foo(a: string, b?: number = 1, ...rest: R) { return a; }\n");
        let function = &program["body"][0];
        assert_eq!(function["type"], "FunctionDeclaration");
        assert_eq!(function["__componentDeclaration"], true);
        let pattern = &function["params"][0];
        assert_eq!(pattern["type"], "ObjectPattern");
        let properties = pattern["properties"].as_array().unwrap();
        assert_eq!(properties.len(), 3);
        assert_eq!(properties[0]["shorthand"], true);
        assert_eq!(properties[1]["value"]["type"], "AssignmentPattern");
        assert!(
            properties[1]["value"]["left"]
                .get("typeAnnotation")
                .is_none()
        );
        assert_eq!(properties[2]["type"], "RestElement");
        assert_eq!(properties[2]["argument"]["name"], "rest");
    }

    #[test]
    fn a_lone_rest_parameter_is_the_props_object() {
        let program = lowered("export component Foo(...props: Props) { return null; }\n");
        let function = &program["body"][0]["declaration"];
        assert_eq!(function["params"][0]["type"], "Identifier");
        assert_eq!(function["params"][0]["name"], "props");
    }

    #[test]
    fn a_renamed_and_string_keyed_parameter_keeps_its_key() {
        let program =
            lowered("component Foo('data-x' as dataX: string, y as z: number) { return z; }\n");
        let properties = program["body"][0]["params"][0]["properties"]
            .as_array()
            .unwrap();
        assert_eq!(properties[0]["key"]["type"], "Literal");
        assert_eq!(properties[0]["value"]["name"], "dataX");
        assert_eq!(properties[0]["shorthand"], false);
        assert_eq!(properties[1]["key"]["name"], "y");
        assert_eq!(properties[1]["value"]["name"], "z");
    }

    #[test]
    fn a_hook_becomes_a_flagged_function() {
        let program = lowered("hook useX(v: T): number { return v; }\n");
        let function = &program["body"][0];
        assert_eq!(function["type"], "FunctionDeclaration");
        assert_eq!(function["__hookDeclaration"], true);
        assert_eq!(function["params"][0]["name"], "v");
    }

    #[test]
    fn a_component_with_no_parameters_has_none() {
        let program = lowered("component Empty() { return null; }\n");
        assert_eq!(program["body"][0]["params"].as_array().unwrap().len(), 0);
    }
}
