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

use serde_json::{Map, Value};
use uf_profiler::profile_span;

use crate::TransformError;
use crate::lower::{Edit, bool_field, node_type, str_field, take, transform_post};

/// Convert a lowered ESTree `Program` into a Babel `File`.
///
/// # Errors
///
/// [`TransformError::Internal`] when a node is not the shape the parser
/// produces — a `Property` whose method value is not a function, say.
pub fn to_babel(mut program: Value, source: &str) -> Result<Value, TransformError> {
    profile_span!("babel::to_babel");
    transform_post(&mut program, &mut |node| convert(node))?;
    let mut file = wrap_file(program);
    let lines = LineTable::new(source);
    let mut next_id = 0u32;
    finalize(&mut file, &mut next_id, &lines, !declares_flow(source));
    Ok(file)
}

/// Whether the module's first line declares Flow, which is how the React
/// Compiler's own fixtures decide the same thing (`parseLanguage` in
/// `snap/src/compiler.ts` reads the first line and nothing else).
///
/// It stands for which parser would have read the module. A Flow module is
/// `hermes-parser`'s — the parser `babel.rs` is a port of the Babel conversion
/// of — and everything else is `@babel/parser`'s. The two differ in what they
/// put on a `loc`, which [`finalize`] and
/// [`compiler::stamp_source_filename`](crate::compiler) both have to respect.
fn declares_flow(source: &str) -> bool {
    source
        .find('\n')
        .map_or(source, |end| &source[..end])
        .contains("@flow")
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
    // Classified rather than owned, for the reason [`Final::of`] gives: the
    // arms below need the node mutably, and paying a `String` per node to hold
    // a name across that borrow is the cost this avoids.
    let Some(kind) = node_type(node).map(Convert::of) else {
        return Ok(Edit::Keep);
    };
    // The parser attaches comments to nodes in its own shape; the compiler
    // reads them from the file's list, so the attached copies only get in
    // the way of deserialization.
    for key in ["leadingComments", "trailingComments", "innerComments"] {
        remove_key(node, key);
    }
    Ok(match kind {
        Convert::Literal => Edit::Replace(literal(node)),
        Convert::Property => Edit::Replace(property(node)?),
        Convert::MethodDefinition => Edit::Replace(method_definition(node)),
        Convert::PropertyDefinition => Edit::Replace(property_definition(node)),
        Convert::PrivateIdentifier => Edit::Replace(private_name(node)),
        Convert::ImportExpression => Edit::Replace(import_expression(node)),
        Convert::ExportAllDeclaration if !node["exported"].is_null() => {
            Edit::Replace(export_namespace(node))
        }
        Convert::ChainExpression => Edit::Replace(chain(take(node, "expression"))),
        Convert::Block => {
            lift_directives(node);
            Edit::Keep
        }
        Convert::JsxText => {
            let value = node["value"].clone();
            let raw = take(node, "raw");
            node["extra"] = node! { "rawValue": value, "raw": raw };
            Edit::Keep
        }
        Convert::ArrayExpression => {
            remove_key(node, "trailingComma");
            Edit::Keep
        }
        Convert::VariableDeclaration => {
            remove_key(node, "__ufEnum");
            Edit::Keep
        }
        Convert::Class => {
            if node
                .get("decorators")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                remove_key(node, "decorators");
            }
            Edit::Keep
        }
        Convert::ImportDeclaration => {
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
        Convert::Function => {
            remove_key(node, "expression");
            Edit::Keep
        }
        _ => Edit::Keep,
    })
}

/// The node types [`convert`] treats specially, as a value rather than a name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Convert {
    ArrayExpression,
    Block,
    ChainExpression,
    Class,
    ExportAllDeclaration,
    Function,
    ImportDeclaration,
    ImportExpression,
    JsxText,
    Literal,
    MethodDefinition,
    PrivateIdentifier,
    Property,
    PropertyDefinition,
    VariableDeclaration,
    Other,
}

impl Convert {
    fn of(kind: &str) -> Self {
        match kind {
            "ArrayExpression" => Self::ArrayExpression,
            "Program" | "BlockStatement" => Self::Block,
            "ChainExpression" => Self::ChainExpression,
            "ClassDeclaration" | "ClassExpression" => Self::Class,
            "ExportAllDeclaration" => Self::ExportAllDeclaration,
            "ArrowFunctionExpression" | "FunctionExpression" | "FunctionDeclaration" => {
                Self::Function
            }
            "ImportDeclaration" => Self::ImportDeclaration,
            "ImportExpression" => Self::ImportExpression,
            "JSXText" => Self::JsxText,
            "Literal" => Self::Literal,
            "MethodDefinition" => Self::MethodDefinition,
            "PrivateIdentifier" => Self::PrivateIdentifier,
            "Property" => Self::Property,
            "PropertyDefinition" => Self::PropertyDefinition,
            "VariableDeclaration" => Self::VariableDeclaration,
            _ => Self::Other,
        }
    }
}

fn remove_key(node: &mut Value, key: &str) {
    if let Some(object) = node.as_object_mut() {
        object.remove(key);
    }
}

fn base(node: &Value, kind: &str) -> Map<String, Value> {
    let position_fields =
        usize::from(node.get("loc").is_some()) + usize::from(node.get("range").is_some());
    let mut map = Map::with_capacity(1 + position_fields);
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
        out.insert("extra".to_owned(), node! { "raw": raw });
        return Value::Object(out);
    }
    if let Some(bigint) = node.get("bigint") {
        let mut out = base(node, "BigIntLiteral");
        out.insert("value".to_owned(), bigint.clone());
        out.insert(
            "extra".to_owned(),
            node! { "rawValue": bigint.clone(), "raw": raw },
        );
        return Value::Object(out);
    }
    match value {
        Value::String(text) => {
            let mut out = base(node, "StringLiteral");
            out.insert("value".to_owned(), Value::String(text.clone()));
            out.insert("extra".to_owned(), node! { "rawValue": text, "raw": raw });
            Value::Object(out)
        }
        Value::Number(number) => {
            let mut out = base(node, "NumericLiteral");
            out.insert("value".to_owned(), Value::Number(number.clone()));
            out.insert("extra".to_owned(), node! { "rawValue": number, "raw": raw });
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
            return Err(TransformError::Internal(
                uf_infra::cstr!(
                    "a method property must hold a FunctionExpression, found {}",
                    node_type(&value).unwrap_or("nothing")
                )
                .into_string(),
            ));
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
    out.insert("callee".to_owned(), node! { "type": "Import" });
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
///
/// The chain is the spine of `object` and `callee` links. Everything hanging
/// off that spine — a computed property, an argument — is a separate
/// expression, and a chain written there is a chain of its own, which the
/// parser leaves bare rather than wrapping in a `ChainExpression` of its own.
/// [`nested`] converts those.
fn chain(node: Value) -> Value {
    match node_type(&node) {
        Some("MemberExpression") => {
            let mut node = node;
            let object = chain(take(&mut node, "object"));
            let computed = bool_field(&node, "computed");
            // A property that is not computed is the name after the dot, which
            // holds no expression and so no chain.
            let property = if computed {
                nested(take(&mut node, "property"))
            } else {
                take(&mut node, "property")
            };
            let optional = bool_field(&node, "optional");
            let inner_optional = matches!(
                node_type(&object),
                Some("OptionalMemberExpression" | "OptionalCallExpression")
            );
            if !optional && !inner_optional {
                node["object"] = object;
                node["property"] = property;
                return node;
            }
            let mut out = base(&node, "OptionalMemberExpression");
            out.insert("object".to_owned(), object);
            out.insert("property".to_owned(), property);
            out.insert("computed".to_owned(), Value::Bool(computed));
            out.insert("optional".to_owned(), Value::Bool(optional));
            Value::Object(out)
        }
        Some("CallExpression") => {
            let mut node = node;
            let callee = chain(take(&mut node, "callee"));
            let arguments = nested(take(&mut node, "arguments"));
            let optional = bool_field(&node, "optional");
            let inner_optional = matches!(
                node_type(&callee),
                Some("OptionalMemberExpression" | "OptionalCallExpression")
            );
            if !optional && !inner_optional {
                node["callee"] = callee;
                node["arguments"] = arguments;
                return node;
            }
            let mut out = base(&node, "OptionalCallExpression");
            out.insert("callee".to_owned(), callee);
            out.insert("optional".to_owned(), Value::Bool(optional));
            out.insert("arguments".to_owned(), arguments);
            Value::Object(out)
        }
        _ => node,
    }
}

/// Convert the chains written inside an expression that is off a chain's spine.
///
/// The parser wraps a chain in a `ChainExpression` only at its outermost node,
/// and a chain inside another chain's computed property or arguments is not
/// that node, so no wrapper marks it: `a?.[b?.c]` is one `ChainExpression`
/// around a member whose property is a bare `MemberExpression` carrying
/// `optional: true`. Without this the inner `?.` would reach the compiler as a
/// plain member — [`finalize`] drops `optional` from those — and the compiler
/// would read `a?.[b.c]`, which is a different program.
fn nested(value: Value) -> Value {
    if value.is_object() && starts_a_chain(&value) {
        // `chain` converts this one's spine and comes back here for whatever
        // hangs off it, so there is nothing left to walk below.
        return chain(value);
    }
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(nested).collect()),
        Value::Object(mut map) => {
            for (key, child) in &mut map {
                if SKIPPED.contains(&key.as_str()) {
                    continue;
                }
                *child = nested(std::mem::take(child));
            }
            Value::Object(map)
        }
        other => other,
    }
}

/// Whether this node is the outermost node of an optional chain: one `?.`
/// anywhere along its spine of `object` and `callee` links makes the whole
/// spine one chain, and short-circuiting covers all of it.
fn starts_a_chain(node: &Value) -> bool {
    let mut current = node;
    loop {
        let link = match node_type(current) {
            Some("MemberExpression") => "object",
            Some("CallExpression") => "callee",
            // Anything else ends the spine — including the `Optional*` nodes a
            // chain already converted, which are nobody's to convert twice.
            _ => return false,
        };
        if bool_field(current, "optional") {
            return true;
        }
        current = &current[link];
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
                .unwrap_or_else(|| {
                    Value::String(uf_infra::into_string(uf_infra::cstr!("\"{text}\"")))
                });
            let mut literal = base(&statement["expression"], "DirectiveLiteral");
            literal.insert("value".to_owned(), Value::String(text.to_owned()));
            literal.insert("extra".to_owned(), node! { "rawValue": text, "raw": raw });
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
    node! {
        "type": "File",
        "program": program,
        "comments": comments,
        "loc": loc,
        "range": range,
    }
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

/// The keys [`finalize`] and [`nested`] never descend into.
const SKIPPED: [&str; 4] = ["type", "loc", "range", "extra"];

/// Overwrite an existing `loc` with a recomputed start and end.
///
/// `false` when there is nothing of the right shape to write into and the
/// caller has to build one — a node the translator gave no `loc`, or one whose
/// `loc` is not the two-point object every other node's is.
///
/// The keys are all present already, so every write is an assignment into a
/// slot rather than an insert: no key is allocated and no map is grown.
fn overwrite_position(
    object: &mut Map<String, Value>,
    start: (u32, u32, u32),
    end: (u32, u32, u32),
    with_index: bool,
) -> bool {
    let Some(loc) = object.get_mut("loc").and_then(Value::as_object_mut) else {
        return false;
    };
    // Babel's `loc` has no `source`, and the translator's does.
    loc.remove("source");
    let mut written = 0;
    for (key, (line, column, index)) in [("start", start), ("end", end)] {
        let Some(point) = loc.get_mut(key).and_then(Value::as_object_mut) else {
            continue;
        };
        // One at a time: two `get_mut` on the same map cannot be live at once,
        // and both keys are checked before either is written so a `loc`
        // missing one of them is left alone rather than half-updated.
        if !(point.contains_key("line") && point.contains_key("column")) {
            continue;
        }
        if let Some(slot) = point.get_mut("line") {
            *slot = Value::from(line);
        }
        if let Some(slot) = point.get_mut("column") {
            *slot = Value::from(column);
        }
        // Babel's own third field: the offset the point sits at. The compiler
        // reads `loc.start.index` and `loc.end.index` for the locations it
        // logs and for the fixes it offers — a suggestion is a range, and it
        // has none without these. `hermes-parser` writes no `index`, so a Flow
        // module gets none either, or the compiler offers a fix on a module
        // whose own snapshots have none. The translator writes no `index` in
        // either case, so this is an insert where the other two are
        // assignments.
        if with_index {
            point.insert("index".to_owned(), Value::from(index));
        }
        written += 1;
    }
    written == 2
}

/// The node types [`finalize`] treats specially, as a value rather than a name.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Final {
    Call,
    ExpressionStatement,
    Identifier,
    MemberExpression,
    Other,
}

impl Final {
    fn of(kind: &str) -> Self {
        match kind {
            "CallExpression" | "NewExpression" => Self::Call,
            "ExpressionStatement" => Self::ExpressionStatement,
            "Identifier" => Self::Identifier,
            "MemberExpression" => Self::MemberExpression,
            _ => Self::Other,
        }
    }
}

/// Assign node ids and Babel's `start`/`end`, and tidy the fields Babel
/// omits, throughout the file.
fn finalize(node: &mut Value, next_id: &mut u32, lines: &LineTable, with_index: bool) {
    let Some(object) = node.as_object_mut() else {
        return;
    };
    // Classified rather than owned. Reading the type borrows the node and
    // every arm below needs it mutably, which is what `str::to_owned` was
    // paying for: one `String` per node, for a name compared four times and
    // dropped. See ubugeeei-prod/uf#668.
    let Some(kind) = object.get("type").and_then(Value::as_str).map(Final::of) else {
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
        let start_index = u32::try_from(start).unwrap_or(u32::MAX);
        let end_index = u32::try_from(end).unwrap_or(u32::MAX);
        let (start_line, start_column) = lines.position(start_index);
        let (end_line, end_column) = lines.position(end_index);
        // Written into the `loc` the translator already attached, when there
        // is one. Its columns count code points where everything downstream
        // counts UTF-16 units, so the numbers are wrong — but the shape is
        // exactly right, and overwriting four of them costs nothing where
        // building a replacement cost three maps and six keys, on every node
        // of every module. `estree::parse` asks for `include_locs`, so there
        // is one to write into almost always. See ubugeeei-prod/uf#668.
        if !overwrite_position(
            object,
            (start_line, start_column, start_index),
            (end_line, end_column, end_index),
            with_index,
        ) {
            let mut start = node! { "line": start_line, "column": start_column };
            let mut end = node! { "line": end_line, "column": end_column };
            if with_index {
                start["index"] = Value::from(start_index);
                end["index"] = Value::from(end_index);
            }
            object.insert("loc".to_owned(), node! { "start": start, "end": end });
        }
    } else if let Some(loc) = object.get_mut("loc").and_then(Value::as_object_mut) {
        loc.remove("source");
    }

    match kind {
        Final::ExpressionStatement => {
            object.remove("directive");
        }
        Final::MemberExpression => {
            object.remove("optional");
        }
        Final::Call => {
            if object.get("optional") == Some(&Value::Bool(false)) {
                object.remove("optional");
            }
        }
        Final::Identifier => {
            let name = object.get("name").cloned();
            if let (Some(loc), Some(name)) =
                (object.get_mut("loc").and_then(Value::as_object_mut), name)
            {
                loc.insert("identifierName".to_owned(), name);
            }
        }
        Final::Other => {}
    }

    // Walked through the map itself. This used to collect the keys into a
    // `Vec<String>` first — a vector and an owned copy of every key, on every
    // node — because `get_mut` cannot run while `keys` is borrowed. `iter_mut`
    // hands out the key and the child together and needs neither, and it is
    // the same order, so `_nodeId` still counts the tree the way it did.
    // On `packages/router/internal/runtime.js` this was the largest single
    // line in ubugeeei-prod/uf#668.
    for (key, child) in object.iter_mut() {
        if SKIPPED.contains(&key.as_str()) {
            continue;
        }
        match child {
            Value::Array(items) => {
                for item in items {
                    finalize(item, next_id, lines, with_index);
                }
            }
            Value::Object(_) => finalize(child, next_id, lines, with_index),
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
    fn a_chain_in_a_computed_property_is_a_chain_of_its_own() {
        // The parser wraps only the outermost member in a `ChainExpression`,
        // so `b?.c` arrives bare and nothing but this marks it optional.
        let file = babel("a?.[b?.c];\n");
        let expression = &file["program"]["body"][0]["expression"];
        assert_eq!(expression["type"], "OptionalMemberExpression");
        assert_eq!(expression["optional"], true);
        assert_eq!(expression["property"]["type"], "OptionalMemberExpression");
        assert_eq!(expression["property"]["optional"], true);
    }

    #[test]
    fn a_chain_in_an_argument_is_a_chain_of_its_own() {
        let file = babel("a?.b(c?.d, {e: f?.g});\n");
        let expression = &file["program"]["body"][0]["expression"];
        assert_eq!(expression["type"], "OptionalCallExpression");
        let arguments = expression["arguments"].as_array().unwrap();
        assert_eq!(arguments[0]["type"], "OptionalMemberExpression");
        assert_eq!(arguments[0]["optional"], true);
        // Reached through an object literal, which is not a chain itself.
        let value = &arguments[1]["properties"][0]["value"];
        assert_eq!(value["type"], "OptionalMemberExpression");
        assert_eq!(value["optional"], true);
    }

    #[test]
    fn chains_nest_to_any_depth_in_a_property() {
        let file = babel("x.y?.[p.a?.[p.b?.[p.c]]];\n");
        let outer = &file["program"]["body"][0]["expression"];
        let middle = &outer["property"];
        let inner = &middle["property"];
        assert_eq!(outer["type"], "OptionalMemberExpression");
        assert_eq!(middle["type"], "OptionalMemberExpression");
        assert_eq!(inner["type"], "OptionalMemberExpression");
        for node in [outer, middle, inner] {
            assert_eq!(node["optional"], true);
            // The spine below each `?.` is not optional and stays plain.
            assert_eq!(node["object"]["type"], "MemberExpression");
        }
    }

    #[test]
    fn a_member_off_the_spine_that_is_not_optional_stays_plain() {
        let file = babel("a?.b(c.d);\n");
        let expression = &file["program"]["body"][0]["expression"];
        assert_eq!(expression["type"], "OptionalCallExpression");
        assert_eq!(expression["arguments"][0]["type"], "MemberExpression");
        // Babel keeps no `optional` on a plain member; `finalize` removes it.
        assert!(expression["arguments"][0].get("optional").is_none());
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
    fn every_position_carries_the_offset_it_is_at() {
        // Babel's `loc` points carry an `index` beside the line and column,
        // and the compiler reads those two for the code frame it prints under
        // a diagnostic. They are the same offsets `start` and `end` hold.
        let file = babel("const a = 1;\n");
        let declaration = &file["program"]["body"][0];
        assert_eq!(declaration["loc"]["start"]["index"], 0);
        assert_eq!(declaration["loc"]["end"]["index"], 12);
        assert_eq!(declaration["start"], declaration["loc"]["start"]["index"]);
        assert_eq!(declaration["end"], declaration["loc"]["end"]["index"]);
    }

    #[test]
    fn a_flow_module_carries_no_offsets() {
        // `hermes-parser` reads a Flow module and writes no `index`, and the
        // compiler offers a fix only where it has a range to offer it over.
        // Giving one here made it suggest dependencies on a module whose own
        // snapshot has no such suggestion.
        let file = babel("// @flow\nconst a = 1;\n");
        let declaration = &file["program"]["body"][0];
        assert_eq!(declaration["loc"]["start"]["line"], 2);
        assert!(declaration["loc"]["start"].get("index").is_none());
        assert!(declaration["loc"]["end"].get("index").is_none());
        // The offsets themselves are still there, where Babel keeps them too.
        assert_eq!(declaration["start"], 9);
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
