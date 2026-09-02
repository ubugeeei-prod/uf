//! Flow lints over type annotations: unclear, deprecated and internal types, and
//! object types that never said whether they are exact.

use super::*;

#[test]
fn unclear_type_rejects_any_object_and_function() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype A = any;\ntype B = Object;\ntype C = Function;\n",
    );

    assert_eq!(diagnostics.len(), 3);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 10));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 10));
    assert_eq!((diagnostics[2].line, diagnostics[2].column), (4, 10));
}

#[test]
fn unclear_type_accepts_precise_annotations() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype A = mixed;\ntype B = { +id: string, ... };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unclear_type_ignores_value_positions() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst keys = Object.keys(props);\nconst ok = list.any;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn unclear_type_ignores_identifiers_that_merely_contain_any() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\nconst company = 1;\n");

    assert!(diagnostics.is_empty());
}

#[test]
fn unclear_type_ignores_comments() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\n// TODO: replace any here\n");

    assert!(diagnostics.is_empty());
}

#[test]
fn deprecated_type_rejects_the_bool_alias() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\ntype A = bool;\n");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 10));
}

#[test]
fn deprecated_type_accepts_boolean() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype A = boolean;\nconst o = { bool: true };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn internal_type_rejects_flow_internals() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\ntype N = React$Node;\ntype T = $TEMPORARY$object;\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].line, 2);
    assert_eq!(diagnostics[1].line, 3);
}

#[test]
fn internal_type_accepts_the_public_equivalents() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nimport type { Node } from '@uniflowed/react';\ntype N = Node;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn ambiguous_object_type_rejects_unmarked_object_types() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\ntype Props = { id: string };\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 14));
}

#[test]
fn ambiguous_object_type_accepts_exact_and_explicitly_inexact_types() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\ntype A = {| id: string |};\ntype B = { id: string, ... };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn ambiguous_object_type_reaches_nested_object_types() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\ntype Props = {\n  id: string,\n  meta: { title: string },\n  ...\n};\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
}

#[test]
fn ambiguous_object_type_ignores_object_literals() {
    let diagnostics = lint_js(
        "flow/ambiguous-object-type",
        "// @flow\nconst defaults = { id: 'x' };\n",
    );

    assert!(diagnostics.is_empty());
}
