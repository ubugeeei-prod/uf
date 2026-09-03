//! Constructors for the ESTree nodes the lowering passes synthesise.
//!
//! A port of `hermes-parser`'s `utils/Builders.js`. Nodes built here carry no
//! position: the printer falls back to the nearest positioned ancestor for the
//! source map, which is the right answer for code the author never wrote.

use serde_json::{Value, json};

/// `name`
#[must_use]
pub fn ident(name: &str) -> Value {
    json!({ "type": "Identifier", "name": name })
}

/// A string literal.
#[must_use]
pub fn string_literal(value: &str) -> Value {
    json!({ "type": "Literal", "value": value, "raw": serde_json::to_string(value).unwrap_or_default() })
}

/// A number literal.
#[must_use]
pub fn number_literal(value: usize) -> Value {
    json!({ "type": "Literal", "value": value, "raw": value.to_string() })
}

/// `null`
#[must_use]
pub fn null_literal() -> Value {
    json!({ "type": "Literal", "value": null, "raw": "null" })
}

/// `object.property` or `object[property]`.
#[must_use]
pub fn member(object: Value, property: Value, computed: bool) -> Value {
    json!({
        "type": "MemberExpression",
        "object": object,
        "property": property,
        "computed": computed,
        "optional": false,
    })
}

/// `callee(arguments)`
#[must_use]
pub fn call(callee: Value, arguments: Vec<Value>) -> Value {
    json!({
        "type": "CallExpression",
        "callee": callee,
        "arguments": arguments,
        "optional": false,
    })
}

/// `left <operator> right`
#[must_use]
pub fn binary(operator: &str, left: Value, right: Value) -> Value {
    json!({ "type": "BinaryExpression", "operator": operator, "left": left, "right": right })
}

/// `left <operator> right` for `&&`, `||`, `??`.
#[must_use]
pub fn logical(operator: &str, left: Value, right: Value) -> Value {
    json!({ "type": "LogicalExpression", "operator": operator, "left": left, "right": right })
}

/// `<operator> argument`
#[must_use]
pub fn unary(operator: &str, argument: Value) -> Value {
    json!({ "type": "UnaryExpression", "operator": operator, "prefix": true, "argument": argument })
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
        return json!({ "type": "Literal", "value": true, "raw": "true" });
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
        return json!({ "type": "Literal", "value": false, "raw": "false" });
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
    json!({
        "type": "VariableDeclaration",
        "kind": kind,
        "declarations": [{ "type": "VariableDeclarator", "id": id, "init": init }],
    })
}

/// `{ body }`
#[must_use]
pub fn block(body: Vec<Value>) -> Value {
    json!({ "type": "BlockStatement", "body": body })
}

/// `return argument;`
#[must_use]
pub fn return_statement(argument: Value) -> Value {
    json!({ "type": "ReturnStatement", "argument": argument })
}

/// `throw argument;`
#[must_use]
pub fn throw_statement(argument: Value) -> Value {
    json!({ "type": "ThrowStatement", "argument": argument })
}

/// `if (test) consequent [else alternate]`
#[must_use]
pub fn if_statement(test: Value, consequent: Value, alternate: Option<Value>) -> Value {
    json!({
        "type": "IfStatement",
        "test": test,
        "consequent": consequent,
        "alternate": alternate.unwrap_or(Value::Null),
    })
}

/// `expression;`
#[must_use]
pub fn expression_statement(expression: Value) -> Value {
    json!({ "type": "ExpressionStatement", "expression": expression })
}

/// `((params) => { statements })(arguments)`
#[must_use]
pub fn iife(statements: Vec<Value>, params: Vec<Value>, arguments: Vec<Value>) -> Value {
    call(
        json!({
            "type": "ArrowFunctionExpression",
            "id": null,
            "params": params,
            "body": block(statements),
            "async": false,
            "expression": false,
            "generator": false,
        }),
        arguments,
    )
}

/// `test ? consequent : alternate`
#[must_use]
pub fn conditional(test: Value, consequent: Value, alternate: Value) -> Value {
    json!({
        "type": "ConditionalExpression",
        "test": test,
        "consequent": consequent,
        "alternate": alternate,
    })
}

/// A `Property` in an object pattern or expression.
#[must_use]
pub fn property(key: Value, value: Value, computed: bool, shorthand: bool) -> Value {
    json!({
        "type": "Property",
        "key": key,
        "value": value,
        "kind": "init",
        "method": false,
        "shorthand": shorthand,
        "computed": computed,
    })
}

/// `...argument`
#[must_use]
pub fn rest_element(argument: Value) -> Value {
    json!({ "type": "RestElement", "argument": argument })
}
