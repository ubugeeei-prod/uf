//! ESTree → Babel's AST shape.
//!
//! The official React Compiler consumes Babel's AST, whose node vocabulary
//! differs from ESTree's in a handful of well-known places: literals are
//! typed (`StringLiteral`, not `Literal`), object members are `ObjectProperty`
//! and `ObjectMethod`, class members are `ClassMethod` and `ClassProperty`,
//! optional chains are `OptionalMemberExpression` and `OptionalCallExpression`
//! rather than a `ChainExpression` wrapper, `import()` is a call to an
//! `Import` callee, and directives are lifted out of statement lists. This is
//! a port of `hermes-parser`'s `TransformESTreeToBabel.js` covering exactly
//! those differences.
//!
//! Every node also gets a `_nodeId`, unique within the file, which is how the
//! compiler's scope information refers to nodes; and `start`/`end` offsets,
//! which it uses for positional queries.

use serde_json::{Map, Value, json};

use crate::TransformError;
use crate::lower::{Edit, bool_field, node_type, str_field, take, transform_post};

/// Convert a lowered ESTree `Program` into a Babel `File`.
///
/// # Errors
///
/// [`TransformError::Internal`] when a node is not the shape the parser
/// produces — a `Property` whose method value is not a function, say.
pub fn to_babel(mut program: Value, source: &str) -> Result<Value, TransformError> {
    transform_post(&mut program, &mut |node| convert(node))?;
    let mut file = wrap_file(program);
    let lines = LineTable::new(source);
    let mut next_id = 0u32;
    finalize(&mut file, &mut next_id, &lines);
    Ok(file)
}

/// Where each line starts, in UTF-16 code units.
///
/// The parser's `loc` columns count code points; source maps and editors
/// count UTF-16 code units, and so do the `range` offsets. This table turns
/// an offset back into the (line, column) pair everything downstream uses.
struct LineTable {
    starts: Vec<u32>,
}

impl LineTable {
    fn new(source: &str) -> Self {
        let mut starts = vec![0u32];
        let mut offset = 0u32;
        for ch in source.chars() {
            offset += u32::try_from(ch.len_utf16()).unwrap_or(1);
            if ch == '\n' {
                starts.push(offset);
            }
        }
        Self { starts }
    }

    /// 1-based line and 0-based column of an offset.
    fn position(&self, offset: u32) -> (u32, u32) {
        let line = self.starts.partition_point(|start| *start <= offset).max(1);
        let column = offset - self.starts[line - 1];
        (u32::try_from(line).unwrap_or(u32::MAX), column)
    }
}

fn convert(node: &mut Value) -> Result<Edit, TransformError> {
    let Some(kind) = node_type(node).map(str::to_owned) else {
        return Ok(Edit::Keep);
    };
    // The parser attaches comments to nodes in its own shape; the compiler
    // reads them from the file's list, so the attached copies only get in
    // the way of deserialization.
    for key in ["leadingComments", "trailingComments", "innerComments"] {
        remove_key(node, key);
    }
    Ok(match kind.as_str() {
        "Literal" => Edit::Replace(literal(node)),
        "Property" => Edit::Replace(property(node)?),
        "MethodDefinition" => Edit::Replace(method_definition(node)),
        "PropertyDefinition" => Edit::Replace(property_definition(node)),
        "PrivateIdentifier" => Edit::Replace(private_name(node)),
        "ImportExpression" => Edit::Replace(import_expression(node)),
        "ExportAllDeclaration" if !node["exported"].is_null() => {
            Edit::Replace(export_namespace(node))
        }
        "ChainExpression" => Edit::Replace(chain(take(node, "expression"))),
        "Program" | "BlockStatement" => {
            lift_directives(node);
            Edit::Keep
        }
        "JSXText" => {
            let value = node["value"].clone();
            let raw = take(node, "raw");
            node["extra"] = json!({ "rawValue": value, "raw": raw });
            Edit::Keep
        }
        "ArrayExpression" => {
            remove_key(node, "trailingComma");
            Edit::Keep
        }
        "VariableDeclaration" => {
            remove_key(node, "__ufEnum");
            Edit::Keep
        }
        "ClassDeclaration" | "ClassExpression" => {
            if node
                .get("decorators")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                remove_key(node, "decorators");
            }
            Edit::Keep
        }
        "ImportDeclaration" => {
            for key in ["attributes", "assertions"] {
                if node
                    .get(key)
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
                {
                    remove_key(node, key);
                }
            }
            Edit::Keep
        }
        "ArrowFunctionExpression" | "FunctionExpression" | "FunctionDeclaration" => {
            remove_key(node, "expression");
            Edit::Keep
        }
        _ => Edit::Keep,
    })
}

fn remove_key(node: &mut Value, key: &str) {
    if let Some(object) = node.as_object_mut() {
        object.remove(key);
    }
}

fn base(node: &Value, kind: &str) -> Map<String, Value> {
    let mut map = Map::new();
    map.insert("type".to_owned(), Value::String(kind.to_owned()));
    for key in ["loc", "range"] {
        if let Some(value) = node.get(key) {
            map.insert(key.to_owned(), value.clone());
        }
    }
    map
}

fn literal(node: &mut Value) -> Value {
    let raw = node.get("raw").cloned().unwrap_or(Value::Null);
    let value = take(node, "value");
    if let Some(regex) = node.get("regex") {
        let mut out = base(node, "RegExpLiteral");
        out.insert("pattern".to_owned(), regex["pattern"].clone());
        out.insert("flags".to_owned(), regex["flags"].clone());
        out.insert("extra".to_owned(), json!({ "raw": raw }));
        return Value::Object(out);
    }
    if let Some(bigint) = node.get("bigint") {
        let mut out = base(node, "BigIntLiteral");
        out.insert("value".to_owned(), bigint.clone());
        out.insert(
            "extra".to_owned(),
            json!({ "rawValue": bigint, "raw": raw }),
        );
        return Value::Object(out);
    }
    match value {
        Value::String(text) => {
            let mut out = base(node, "StringLiteral");
            out.insert("value".to_owned(), Value::String(text.clone()));
            out.insert("extra".to_owned(), json!({ "rawValue": text, "raw": raw }));
            Value::Object(out)
        }
        Value::Number(number) => {
            let mut out = base(node, "NumericLiteral");
            out.insert("value".to_owned(), Value::Number(number.clone()));
            out.insert(
                "extra".to_owned(),
                json!({ "rawValue": number, "raw": raw }),
            );
            Value::Object(out)
        }
        Value::Bool(flag) => {
            let mut out = base(node, "BooleanLiteral");
            out.insert("value".to_owned(), Value::Bool(flag));
            Value::Object(out)
        }
        _ => Value::Object(base(node, "NullLiteral")),
    }
}

fn property(node: &mut Value) -> Result<Value, TransformError> {
    let kind = str_field(node, "kind").unwrap_or("init").to_owned();
    let is_method = bool_field(node, "method");
    let computed = bool_field(node, "computed");
    let key = take(node, "key");
    let mut value = take(node, "value");

    if is_method || kind != "init" {
        if node_type(&value) != Some("FunctionExpression") {
            return Err(TransformError::Internal(format!(
                "a method property must hold a FunctionExpression, found {}",
                node_type(&value).unwrap_or("nothing")
            )));
        }
        let mut out = base(node, "ObjectMethod");
        out.insert(
            "kind".to_owned(),
            Value::String(if kind == "init" {
                "method".to_owned()
            } else {
                kind.clone()
            }),
        );
        out.insert("method".to_owned(), Value::Bool(kind == "init"));
        out.insert("computed".to_owned(), Value::Bool(computed));
        out.insert("key".to_owned(), key);
        out.insert("id".to_owned(), Value::Null);
        out.insert("params".to_owned(), take(&mut value, "params"));
        out.insert("body".to_owned(), take(&mut value, "body"));
        out.insert("async".to_owned(), Value::Bool(bool_field(&value, "async")));
        out.insert(
            "generator".to_owned(),
            Value::Bool(bool_field(&value, "generator")),
        );
        return Ok(Value::Object(out));
    }

    let mut out = base(node, "ObjectProperty");
    out.insert("computed".to_owned(), Value::Bool(computed));
    out.insert("key".to_owned(), key);
    out.insert("value".to_owned(), value);
    out.insert("method".to_owned(), Value::Bool(false));
    out.insert(
        "shorthand".to_owned(),
        Value::Bool(bool_field(node, "shorthand")),
    );
    Ok(Value::Object(out))
}

fn method_definition(node: &mut Value) -> Value {
    let key = take(node, "key");
    let mut value = take(node, "value");
    let private = node_type(&key) == Some("PrivateName");
    let mut out = base(
        node,
        if private {
            "ClassPrivateMethod"
        } else {
            "ClassMethod"
        },
    );
    out.insert(
        "kind".to_owned(),
        node.get("kind")
            .cloned()
            .unwrap_or_else(|| Value::String("method".to_owned())),
    );
    out.insert(
        "computed".to_owned(),
        Value::Bool(bool_field(node, "computed")),
    );
    out.insert("static".to_owned(), Value::Bool(bool_field(node, "static")));
    out.insert("key".to_owned(), key);
    out.insert("id".to_owned(), Value::Null);
    out.insert("params".to_owned(), take(&mut value, "params"));
    out.insert("body".to_owned(), take(&mut value, "body"));
    out.insert("async".to_owned(), Value::Bool(bool_field(&value, "async")));
    out.insert(
        "generator".to_owned(),
        Value::Bool(bool_field(&value, "generator")),
    );
    Value::Object(out)
}

fn property_definition(node: &mut Value) -> Value {
    let key = take(node, "key");
    let private = node_type(&key) == Some("PrivateName");
    let mut out = base(
        node,
        if private {
            "ClassPrivateProperty"
        } else {
            "ClassProperty"
        },
    );
    out.insert("key".to_owned(), key);
    out.insert("value".to_owned(), take(node, "value"));
    out.insert("static".to_owned(), Value::Bool(bool_field(node, "static")));
    if !private {
        out.insert(
            "computed".to_owned(),
            Value::Bool(bool_field(node, "computed")),
        );
    }
    Value::Object(out)
}

fn private_name(node: &mut Value) -> Value {
    let name = take(node, "name");
    let mut id = base(node, "Identifier");
    id.insert("name".to_owned(), name);
    let mut out = base(node, "PrivateName");
    out.insert("id".to_owned(), Value::Object(id));
    Value::Object(out)
}

fn import_expression(node: &mut Value) -> Value {
    let source = take(node, "source");
    let options = take(node, "options");
    let mut arguments = vec![source];
    if !options.is_null() {
        arguments.push(options);
    }
    let mut out = base(node, "CallExpression");
    out.insert("callee".to_owned(), json!({ "type": "Import" }));
    out.insert("arguments".to_owned(), Value::Array(arguments));
    Value::Object(out)
}

fn export_namespace(node: &mut Value) -> Value {
    let exported = take(node, "exported");
    let mut specifier = base(&exported, "ExportNamespaceSpecifier");
    specifier.insert("exported".to_owned(), exported);
    let mut out = base(node, "ExportNamedDeclaration");
    out.insert("declaration".to_owned(), Value::Null);
    out.insert(
        "specifiers".to_owned(),
        Value::Array(vec![Value::Object(specifier)]),
    );
    out.insert("source".to_owned(), take(node, "source"));
    out.insert("exportKind".to_owned(), Value::String("value".to_owned()));
    Value::Object(out)
}

/// Rewrite the members of an optional chain into Babel's `Optional*` nodes.
///
/// A member or call inside the chain that is itself not optional but sits on
/// an optional one still becomes `Optional*` with `optional: false`, which is
/// how Babel marks "part of the chain" — short-circuiting has to cover it.
fn chain(node: Value) -> Value {
    match node_type(&node) {
        Some("MemberExpression") => {
            let mut node = node;
            let object = chain(take(&mut node, "object"));
            let optional = bool_field(&node, "optional");
            let inner_optional = matches!(
                node_type(&object),
                Some("OptionalMemberExpression" | "OptionalCallExpression")
            );
            if !optional && !inner_optional {
                node["object"] = object;
                return node;
            }
            let mut out = base(&node, "OptionalMemberExpression");
            out.insert("object".to_owned(), object);
            out.insert("property".to_owned(), take(&mut node, "property"));
            out.insert(
                "computed".to_owned(),
                Value::Bool(bool_field(&node, "computed")),
            );
            out.insert("optional".to_owned(), Value::Bool(optional));
            Value::Object(out)
        }
        Some("CallExpression") => {
            let mut node = node;
            let callee = chain(take(&mut node, "callee"));
            let optional = bool_field(&node, "optional");
            let inner_optional = matches!(
                node_type(&callee),
                Some("OptionalMemberExpression" | "OptionalCallExpression")
            );
            if !optional && !inner_optional {
                node["callee"] = callee;
                return node;
            }
            let mut out = base(&node, "OptionalCallExpression");
            out.insert("callee".to_owned(), callee);
            out.insert("optional".to_owned(), Value::Bool(optional));
            out.insert("arguments".to_owned(), take(&mut node, "arguments"));
            Value::Object(out)
        }
        _ => node,
    }
}

/// Move a statement list's leading directive prologue into `directives`.
fn lift_directives(node: &mut Value) {
    if node.get("directives").is_some() {
        return;
    }
    let mut directives = Vec::new();
    let mut leading = 0usize;
    if let Some(body) = node.get("body").and_then(Value::as_array) {
        for statement in body {
            let Some(text) = str_field(statement, "directive") else {
                break;
            };
            let raw = statement["expression"]
                .get("extra")
                .and_then(|extra| extra.get("raw"))
                .cloned()
                .or_else(|| statement["expression"].get("raw").cloned())
                .unwrap_or_else(|| Value::String(format!("\"{text}\"")));
            let mut literal = base(&statement["expression"], "DirectiveLiteral");
            literal.insert("value".to_owned(), Value::String(text.to_owned()));
            literal.insert("extra".to_owned(), json!({ "rawValue": text, "raw": raw }));
            let mut directive = base(statement, "Directive");
            directive.insert("value".to_owned(), Value::Object(literal));
            directives.push(Value::Object(directive));
            leading += 1;
        }
    }
    if let Some(body) = node.get_mut("body").and_then(Value::as_array_mut) {
        body.drain(..leading);
    }
    node["directives"] = Value::Array(directives);
}

fn wrap_file(mut program: Value) -> Value {
    let comments: Vec<Value> = take(&mut program, "comments")
        .as_array()
        .map(|list| list.iter().map(comment).collect())
        .unwrap_or_default();
    if program.get("sourceType").is_none() {
        program["sourceType"] = Value::String("module".to_owned());
    }
    let loc = program.get("loc").cloned().unwrap_or(Value::Null);
    let range = program.get("range").cloned().unwrap_or(Value::Null);
    json!({
        "type": "File",
        "program": program,
        "comments": comments,
        "loc": loc,
        "range": range,
    })
}

fn comment(node: &Value) -> Value {
    let kind = if node_type(node) == Some("Line") {
        "CommentLine"
    } else {
        "CommentBlock"
    };
    let mut out = base(node, kind);
    out.insert(
        "value".to_owned(),
        node.get("value").cloned().unwrap_or(Value::Null),
    );
    Value::Object(out)
}

/// The keys `finalize` never descends into.
const SKIPPED: [&str; 4] = ["type", "loc", "range", "extra"];

/// Assign node ids and Babel's `start`/`end`, and tidy the fields Babel
/// omits, throughout the file.
fn finalize(node: &mut Value, next_id: &mut u32, lines: &LineTable) {
    let Some(object) = node.as_object_mut() else {
        return;
    };
    let Some(kind) = object
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return;
    };

    object.insert("_nodeId".to_owned(), Value::from(*next_id));
    *next_id += 1;

    let mut offsets = None;
    if let Some(range) = object.remove("range")
        && let Some(pair) = range.as_array()
        && pair.len() == 2
    {
        object.insert("start".to_owned(), pair[0].clone());
        object.insert("end".to_owned(), pair[1].clone());
        offsets = pair[0].as_u64().zip(pair[1].as_u64());
    }
    if let Some((start, end)) = offsets {
        let (start_line, start_column) = lines.position(u32::try_from(start).unwrap_or(u32::MAX));
        let (end_line, end_column) = lines.position(u32::try_from(end).unwrap_or(u32::MAX));
        object.insert(
            "loc".to_owned(),
            json!({
                "start": { "line": start_line, "column": start_column },
                "end": { "line": end_line, "column": end_column },
            }),
        );
    } else if let Some(loc) = object.get_mut("loc").and_then(Value::as_object_mut) {
        loc.remove("source");
    }

    match kind.as_str() {
        "ExpressionStatement" => {
            object.remove("directive");
        }
        "MemberExpression" => {
            object.remove("optional");
        }
        "CallExpression" | "NewExpression" => {
            if object.get("optional") == Some(&Value::Bool(false)) {
                object.remove("optional");
            }
        }
        "Identifier" => {
            let name = object.get("name").cloned();
            if let (Some(loc), Some(name)) =
                (object.get_mut("loc").and_then(Value::as_object_mut), name)
            {
                loc.insert("identifierName".to_owned(), name);
            }
        }
        _ => {}
    }

    let keys: Vec<String> = object
        .keys()
        .filter(|key| !SKIPPED.contains(&key.as_str()))
        .cloned()
        .collect();
    for key in keys {
        match object.get_mut(&key) {
            Some(Value::Array(items)) => {
                for item in items {
                    finalize(item, next_id, lines);
                }
            }
            Some(child @ Value::Object(_)) => finalize(child, next_id, lines),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;
    use crate::lower;

    fn babel(source: &str) -> Value {
        let mut program = parse(source).unwrap();
        lower::lower(&mut program, source).unwrap();
        to_babel(program, source).unwrap()
    }

    #[test]
    fn literals_are_typed_and_carry_their_raw_text() {
        let file = babel("const a = 'x', b = 0x10, c = true, d = null, e = /r/g, f = 10n;\n");
        let declarations = file["program"]["body"][0]["declarations"]
            .as_array()
            .unwrap();
        assert_eq!(declarations[0]["init"]["type"], "StringLiteral");
        assert_eq!(declarations[0]["init"]["extra"]["raw"], "'x'");
        assert_eq!(declarations[1]["init"]["type"], "NumericLiteral");
        assert_eq!(declarations[1]["init"]["value"], 16);
        assert_eq!(declarations[2]["init"]["type"], "BooleanLiteral");
        assert_eq!(declarations[3]["init"]["type"], "NullLiteral");
        assert_eq!(declarations[4]["init"]["type"], "RegExpLiteral");
        assert_eq!(declarations[4]["init"]["flags"], "g");
        assert_eq!(declarations[5]["init"]["type"], "BigIntLiteral");
    }

    #[test]
    fn object_and_class_members_take_babel_s_names() {
        let file = babel(
            "const o = { a: 1, b() {}, get c() { return 1; } };\nclass K { m() {} #p = 1; static q = 2; #r() {} }\n",
        );
        let props = file["program"]["body"][0]["declarations"][0]["init"]["properties"]
            .as_array()
            .unwrap();
        assert_eq!(props[0]["type"], "ObjectProperty");
        assert_eq!(props[1]["type"], "ObjectMethod");
        assert_eq!(props[1]["kind"], "method");
        assert_eq!(props[2]["kind"], "get");
        let members = file["program"]["body"][1]["body"]["body"]
            .as_array()
            .unwrap();
        assert_eq!(members[0]["type"], "ClassMethod");
        assert_eq!(members[1]["type"], "ClassPrivateProperty");
        assert_eq!(members[1]["key"]["type"], "PrivateName");
        assert_eq!(members[2]["type"], "ClassProperty");
        assert_eq!(members[3]["type"], "ClassPrivateMethod");
    }

    #[test]
    fn optional_chains_become_optional_nodes() {
        let file = babel("a?.b.c?.();\n");
        let expression = &file["program"]["body"][0]["expression"];
        assert_eq!(expression["type"], "OptionalCallExpression");
        assert_eq!(expression["callee"]["type"], "OptionalMemberExpression");
        assert_eq!(expression["callee"]["optional"], false);
        assert_eq!(expression["callee"]["object"]["optional"], true);
    }

    #[test]
    fn directives_are_lifted_and_dynamic_import_is_a_call() {
        let file =
            babel("'use client';\nfunction f() { 'use strict'; return import('./x.js'); }\n");
        assert_eq!(
            file["program"]["directives"][0]["value"]["value"],
            "use client"
        );
        assert_eq!(file["program"]["body"].as_array().unwrap().len(), 1);
        let function = &file["program"]["body"][0];
        assert_eq!(
            function["body"]["directives"][0]["value"]["value"],
            "use strict"
        );
        assert_eq!(
            function["body"]["body"][0]["argument"]["callee"]["type"],
            "Import"
        );
    }

    #[test]
    fn every_node_has_a_unique_id_and_offsets() {
        let file = babel("const a = 1;\n");
        assert_eq!(file["_nodeId"], 0);
        assert_eq!(file["program"]["_nodeId"], 1);
        assert_eq!(file["program"]["body"][0]["start"], 0);
        assert_eq!(file["program"]["body"][0]["end"], 12);
        assert_eq!(
            file["program"]["body"][0]["declarations"][0]["id"]["loc"]["identifierName"],
            "a"
        );
    }

    #[test]
    fn columns_count_utf16_code_units() {
        let file = babel("const s = \"😀\";\nconst t = \"日本\"; const u = 1;\n");
        let u = &file["program"]["body"][2]["declarations"][0];
        assert_eq!(u["loc"]["start"]["line"], 2);
        // `const t = "日本"; ` is 16 code units; the declarator starts after `const `.
        assert_eq!(u["loc"]["start"]["column"], 22);
        // The first line, `const s = "😀";` plus its newline, is 16 code units too.
        assert_eq!(u["start"], 16 + 22);
    }

    #[test]
    fn export_star_as_becomes_a_namespace_specifier() {
        let file = babel("export * as ns from './x.js';\n");
        let export = &file["program"]["body"][0];
        assert_eq!(export["type"], "ExportNamedDeclaration");
        assert_eq!(export["specifiers"][0]["type"], "ExportNamespaceSpecifier");
    }

    #[test]
    fn the_result_deserializes_into_the_compiler_s_ast() {
        let file = babel(
            "// @flow\nimport {useState} from 'react';\nexport component App(title: string) { const [n, setN] = useState(0); return <h1 onClick={() => setN(n + 1)}>{title}{n}</h1>; }\n",
        );
        let parsed: Result<react_compiler_ast::File, _> = serde_json::from_value(file);
        assert!(parsed.is_ok(), "{parsed:?}");
    }
}
