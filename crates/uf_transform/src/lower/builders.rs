//! Constructors for the ESTree nodes the lowering passes synthesise.
//!
//! A port of `hermes-parser`'s `utils/Builders.js`. Nodes built here carry no
//! position: the printer falls back to the nearest positioned ancestor for the
//! source map, which is the right answer for code the author never wrote.

use serde_json::Value;

/// Take every position off `node` and everything under it.
///
/// The builders above synthesise nodes without one, but a lowering that reaches
/// for a *parsed* runtime — `enums.rs` prepends one, because a page of helper
/// JavaScript reads better as JavaScript than as node constructors — gets nodes
/// whose `loc` is a position in **that** text. The printer cannot tell the two
/// apart, so those positions were recorded against the author's file: the enum
/// runtime's fourth line became the author's line 4, and one of its long lines
/// became column 157 of a line the author's file does not have.
///
/// That is a source map pointing at the wrong place rather than at no place,
/// which is the one thing `docs/architecture.md` promises never happens — a
/// debugger stepping into `$$ufEnum` landed on an unrelated line of the user's
/// module, and a coverage report counted the runtime's helpers as the author's
/// uncovered functions. Forgetting the positions restores the invariant the
/// rest of this module already keeps.
pub fn forget_positions(node: &mut Value) {
    match node {
        Value::Object(fields) => {
            fields.remove("loc");
            fields.remove("range");
            fields.remove("start");
            fields.remove("end");
            for value in fields.values_mut() {
                forget_positions(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                forget_positions(item);
            }
        }
        _ => {}
    }
}

/// `name`
#[must_use]
pub fn ident(name: &str) -> Value {
    node! { "type": "Identifier", "name": name }
}

/// A string literal.
#[must_use]
pub fn string_literal(value: &str) -> Value {
    node! { "type": "Literal", "value": value, "raw": serde_json::to_string(value).unwrap_or_default() }
}

/// A number literal.
#[must_use]
pub fn number_literal(value: usize) -> Value {
    node! { "type": "Literal", "value": value, "raw": value.to_string() }
}

/// `null`
#[must_use]
pub fn null_literal() -> Value {
    node! { "type": "Literal", "value": ::serde_json::Value::Null, "raw": "null" }
}

/// `object.property` or `object[property]`.
#[must_use]
pub fn member(object: Value, property: Value, computed: bool) -> Value {
    node! {
        "type": "MemberExpression",
        "object": object,
        "property": property,
        "computed": computed,
        "optional": false,
    }
}

/// `callee(arguments)`
#[must_use]
pub fn call(callee: Value, arguments: Vec<Value>) -> Value {
    node! {
        "type": "CallExpression",
        "callee": callee,
        "arguments": arguments,
        "optional": false,
    }
}

/// `left <operator> right`
#[must_use]
pub fn binary(operator: &str, left: Value, right: Value) -> Value {
    node! { "type": "BinaryExpression", "operator": operator, "left": left, "right": right }
}

/// `left <operator> right` for `&&`, `||`, `??`.
#[must_use]
pub fn logical(operator: &str, left: Value, right: Value) -> Value {
    node! { "type": "LogicalExpression", "operator": operator, "left": left, "right": right }
}

/// `<operator> argument`
#[must_use]
pub fn unary(operator: &str, argument: Value) -> Value {
    node! { "type": "UnaryExpression", "operator": operator, "prefix": true, "argument": argument }
}

/// `typeof argument === "kind"`
#[must_use]
pub fn typeof_is(argument: Value, kind: &str) -> Value {
    binary("===", unary("typeof", argument), string_literal(kind))
}

/// `a && b && c`, or `a` alone, or `true` for nothing.
#[must_use]
pub fn conjunction(mut tests: Vec<Value>) -> Value {
    if tests.is_empty() {
        return node! { "type": "Literal", "value": true, "raw": "true" };
    }
    let mut result = tests.remove(0);
    for test in tests {
        result = logical("&&", result, test);
    }
    result
}

/// `a || b || c`, or `a` alone.
#[must_use]
pub fn disjunction(mut tests: Vec<Value>) -> Value {
    if tests.is_empty() {
        return node! { "type": "Literal", "value": false, "raw": "false" };
    }
    let mut result = tests.remove(0);
    for test in tests {
        result = logical("||", result, test);
    }
    result
}

/// `<kind> id = init;`
#[must_use]
pub fn variable_declaration(kind: &str, id: Value, init: Value) -> Value {
    node! {
        "type": "VariableDeclaration",
        "kind": kind,
        "declarations": vec![node!{ "type": "VariableDeclarator", "id": id, "init": init }],
    }
}

/// `{ body }`
#[must_use]
pub fn block(body: Vec<Value>) -> Value {
    node! { "type": "BlockStatement", "body": body }
}

/// `return argument;`
#[must_use]
pub fn return_statement(argument: Value) -> Value {
    node! { "type": "ReturnStatement", "argument": argument }
}

/// `throw argument;`
#[must_use]
pub fn throw_statement(argument: Value) -> Value {
    node! { "type": "ThrowStatement", "argument": argument }
}

/// `if (test) consequent [else alternate]`
#[must_use]
pub fn if_statement(test: Value, consequent: Value, alternate: Option<Value>) -> Value {
    node! {
        "type": "IfStatement",
        "test": test,
        "consequent": consequent,
        "alternate": alternate.unwrap_or(Value::Null),
    }
}

/// `expression;`
#[must_use]
pub fn expression_statement(expression: Value) -> Value {
    node! { "type": "ExpressionStatement", "expression": expression }
}

/// `((params) => { statements })(arguments)`
#[must_use]
pub fn iife(statements: Vec<Value>, params: Vec<Value>, arguments: Vec<Value>) -> Value {
    call(
        node! {
            "type": "ArrowFunctionExpression",
            "id": ::serde_json::Value::Null,
            "params": params,
            "body": block(statements),
            "async": false,
            "expression": false,
            "generator": false,
        },
        arguments,
    )
}

/// `test ? consequent : alternate`
#[must_use]
pub fn conditional(test: Value, consequent: Value, alternate: Value) -> Value {
    node! {
        "type": "ConditionalExpression",
        "test": test,
        "consequent": consequent,
        "alternate": alternate,
    }
}

/// A `Property` in an object pattern or expression.
#[must_use]
pub fn property(key: Value, value: Value, computed: bool, shorthand: bool) -> Value {
    node! {
        "type": "Property",
        "key": key,
        "value": value,
        "kind": "init",
        "method": false,
        "shorthand": shorthand,
        "computed": computed,
    }
}

/// `...argument`
#[must_use]
pub fn rest_element(argument: Value) -> Value {
    node! { "type": "RestElement", "argument": argument }
}
