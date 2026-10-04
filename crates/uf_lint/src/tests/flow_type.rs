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
fn unclear_type_ignores_a_word_inside_a_string() {
    // A test name that says what a matcher does is prose, not an annotation:
    // `it("treats Object as any non-null object", …)` names no type.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nit(\"treats Object as any non-null object\", () => {});\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_still_reads_an_annotation_after_a_string() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst label = \"any\";\ntype A = any;\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 3);
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
fn unclear_type_still_reads_a_type_parameter_default_on_the_next_line() {
    // `T` starts the line, which is also what `ctor = Function` looks like.
    // The `<` that makes it a type closed the line above.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Box<\n  T = any\n> = T;\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 7));
}

#[test]
fn unclear_type_ignores_a_value_on_the_right_of_an_assignment() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst value = any;\nlet ctor = Object;\nctor = Function;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn internal_type_ignores_a_name_inside_a_string() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst label = \"React$Node\";\nconst other = 'React$Element';\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_ignores_an_imported_value() {
    // The names are the module's bindings. Rewriting `any` to `mixed` here
    // would rename the import.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nimport { any, Object, Function } from \"./matchers.js\";\nimport {\n  any as value,\n} from \"./more.js\";\nexport { Function } from \"./matchers.js\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_still_reads_an_imported_type() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nimport type { any, Object } from \"./types.js\";\nimport { type Function } from \"./types.js\";\nexport type { any } from \"./types.js\";\n",
    );

    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
}

#[test]
fn deprecated_type_ignores_an_imported_bool() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nimport { bool } from \"./postgresql.js\";\nimport {\n  bool as flag,\n} from \"./postgresql.js\";\nexport { bool } from \"./postgresql.js\";\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn deprecated_type_still_reads_an_imported_bool_type() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nimport type { bool } from \"./types.js\";\nimport { type bool as Flag } from \"./types.js\";\ntype Alias = bool;\n",
    );

    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
}

#[test]
fn deprecated_type_ignores_a_value_named_bool() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfunction enabled(bool) { return bool; }\nconst flag = bool;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_ignores_a_parameter_name_and_a_returned_value() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfunction test(any: string) { return any; }\nconst any = 1;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_ignores_a_case_label() {
    // `case Object:` matches against the global constructor. Flow has no syntax
    // that puts a type after `case`, so this is only ever a value.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nswitch (ctor) {\n  case Function:\n    return 1;\n  case Object:\n    return 2;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_ignores_an_equality_operand() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst plain = obj.constructor === Object || obj.constructor !== Function;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_still_reads_an_alias_after_a_single_equals() {
    // The one `=` this rule exists for. A generic, a default type argument, and
    // an opaque type's implementation are the same alias with a different left.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Handler = Function;\ntype Box<T> = any;\ntype BoxDefault<T = any> = string;\nopaque type Hidden: Super = any;\n",
    );

    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 16));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 15));
    assert_eq!((diagnostics[2].line, diagnostics[2].column), (4, 21));
    assert_eq!((diagnostics[3].line, diagnostics[3].column), (5, 29));
}

#[test]
fn unclear_type_ignores_a_property_named_any() {
    // `expect.any` is the matcher's name in Jest, Vitest and `@uniflowed/test`,
    // and the object that carries it has to spell it out.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nexport const expect = {\n  any: asymmetric.any,\n  not: { Object: 1 },\n};\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_still_reads_the_value_type_of_a_property() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Row = {\n  any: string,\n  value: any,\n};\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (4, 10));
}

#[test]
fn unclear_type_ignores_a_constructor_passed_as_an_argument() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nexpect(fn).toEqual(expect.any(Function));\nexpect({}).toEqual(expect.any(Object));\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_still_reads_a_bare_type_in_a_function_type() {
    // The same shape with a different opener: the `(` of `(Object) => void`
    // follows an `=`, not a name, so nothing is being called and `Object` is
    // the parameter's type.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Sink = (Object) => void;\ntype Sunk = Foo<(Function) => void>;\n",
    );

    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 14));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 18));
}

#[test]
fn unclear_type_still_reads_a_type_argument_inside_a_call() {
    // `any` here is inside `<>`, not an argument of the call around it.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nregister(new Map<string, any>(), (node: any) => node);\n",
    );

    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 26));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (2, 41));
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
fn unclear_type_ignores_a_comparison_an_arithmetic_use_and_jsx_text() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nif (any < limit) {}\nif (count > any) {}\nif (count >= any) {}\nif (any > limit) {}\nif (any > (limit)) {}\nif (any >= limit) {}\nif (any === limit) {}\nif (any !== limit) {}\nfoo(any + 1);\nfoo(1 + any);\nfoo(any >> 1);\nconst view = <p>any</p>;\nconst sentence = <p>hello any there</p>;\nconst named = <p>hello Object there</p>;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Box = any;\ntype Items = Array<any>;\ntype Nested = Array<Array<any>>;\ntype Either = any | number;\nconst label = \">\"; type Quoted = any; const view = <p>x</p>;\n",
    );
    assert_eq!(still.len(), 5, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_name_on_the_line_after_a_tag() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst view = (\n  <pre>\n    any\n  </pre>\n);\nconst objectName = (\n  <pre>\n    Object\n  </pre>\n);\nconst functionName = (\n  <pre>\n    Function\n  </pre>\n);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nconst expr = <p>{any}</p>;\ntype Box = any;\ntype Items = Array<Object>;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
    assert_eq!(
        still
            .iter()
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 5]
    );
}

#[test]
fn unclear_type_ignores_a_xor_operand() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfoo(any ^ mask);\nfoo(mask ^ Object);\nfoo(Function ^ mask);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Items = Array<Object>;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_prefix_keyword_operand() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfoo(typeof any);\nfoo(void Object);\nfoo(await Function);\nfoo(yield any);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype U = void | any;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn unclear_type_ignores_an_in_or_instanceof_operand() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfoo(any in items);\nfoo(key in Object);\nfoo(value instanceof Function);\nfoo(Object instanceof Ctor);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Items = Array<Object>;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn unclear_type_ignores_nullish_and_optional_operands() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfoo(any ?? ready);\nfoo(ready ?? any);\nfoo(any?.prop);\nfoo(Object?.());\nfoo(Function?.name);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype C = T extends any ? number : empty;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!(still[0].line, 2);
}

#[test]
fn unclear_type_ignores_a_logical_operand() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfoo(ready && any);\nfoo(any && ready);\nfoo(ready || Object);\nfoo(Function || ready);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype U = any | Other;\ntype I = any & Other;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_less_than_comparison() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nif (count < any) {}\nif (count <= any) {}\nif (count < Object) {}\nif (count < Function) {}\nif (count < any && ready) {}\nif (count < any ? 1 : 0) {}\nfoo(count << any);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Items = Array<any>;\ntype Pair = Foo<any, number>;\ntype Union = Foo<any | number>;\ntype Both = Foo<any & number>;\ntype Default = Foo<any = number>;\ntype Box<T = any> = T;\nconst made = new Map<string, any>();\n",
    );
    assert_eq!(still.len(), 7, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_comparison() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nif (bool < limit) {}\nif (bool > limit) {}\nif (bool === ready) {}\nconst view = <p>hello bool there</p>;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_xor_operand() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfoo(bool ^ mask);\nfoo(mask ^ bool);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_prefix_keyword_operand() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfoo(typeof bool);\nfoo(void bool);\nfoo(await bool);\nfoo(yield bool);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_an_in_or_instanceof_operand() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfoo(key in bool);\nfoo(value instanceof bool);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_nullish_and_optional_operands() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfoo(bool ?? ready);\nfoo(ready ?? bool);\nfoo(bool?.value);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_logical_operand() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfoo(ready && bool);\nfoo(bool || ready);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_less_than_comparison() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nif (count < bool) {}\nif (count <= bool) {}\nif (count < bool && ready) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype Items = Array<bool>;\ntype Default = Foo<bool = number>;\ntype Flag = bool;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn internal_type_ignores_a_name_in_jsx_text() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst view = <p>React$Node</p>;\nconst sentence = <p>hello React$Node there</p>;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 20));
}

#[test]
fn internal_type_ignores_a_name_on_the_line_after_a_tag() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst view = (\n  <pre>\n    React$Node\n  </pre>\n);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nconst expr = <p>{React$Node}</p>;\nexport type Slot = React$Node;\ntype Items = Array<React$Node>;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
    assert_eq!(
        still
            .iter()
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
}

#[test]
fn internal_type_ignores_a_xor_operand() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfoo(React$Node ^ mask);\nfoo(mask ^ React$Node);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_prefix_keyword_operand() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfoo(typeof React$Node);\nfoo(void React$Node);\nfoo(await React$Node);\nfoo(yield React$Node);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_an_in_or_instanceof_operand() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfoo(key in React$Node);\nfoo(value instanceof React$Node);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_nullish_and_optional_operands() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfoo(React$Node ?? ready);\nfoo(ready ?? React$Node);\nfoo(React$Node?.type);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_logical_operand() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfoo(ready && React$Node);\nfoo(React$Node || ready);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_less_than_comparison() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nif (count < React$Node) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = Array<React$Node>;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_bool_on_the_line_after_a_tag() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst view = (\n  <pre>\n    bool\n  </pre>\n);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst expr = <p>{bool}</p>;\ntype Flag = bool;\ntype Items = Array<bool>;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
    assert_eq!(
        still
            .iter()
            .map(|diagnostic| diagnostic.line)
            .collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
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

/// The rule is off by default, and the reason is a fact about Flow rather than
/// a matter of taste.
///
/// It exists for a world where `exact_by_default=false` was a `.flowconfig`
/// option and `{ a: b }` meant different things in different projects. Flow has
/// defaulted to exact since 2023 and now rejects that option as deprecated, so
/// there is nothing left to disambiguate — and `{| |}`, which the rule asks
/// for, is the legacy spelling of what plain braces already mean.
#[test]
fn ambiguous_object_type_is_off_by_default() {
    let config = UniflowedConfig::default();

    assert_eq!(
        config.lint.rules.get("flow/ambiguous-object-type").copied(),
        Some(RuleLevel::Off)
    );

    let report =
        lint_source(&source("// @flow\ntype Props = { id: string };\n"), &config).expect("lint");

    assert!(
        report.diagnostics.is_empty(),
        "modern Flow's exact object type must not be a default error: {:?}",
        report.diagnostics
    );
}

/// Off by default is not gone: a codebase migrating from an older Flow may
/// want every object type marked while both spellings are in the tree.
#[test]
fn ambiguous_object_type_can_still_be_switched_on() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.insert(
        CompactString::const_new("flow/ambiguous-object-type"),
        RuleLevel::Error,
    );

    let report =
        lint_source(&source("// @flow\ntype Props = { id: string };\n"), &config).expect("lint");

    assert_eq!(report.diagnostics.len(), 1);
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

#[test]
fn unclear_type_ignores_a_bare_argument_written_over_several_lines() {
    // The scan reads one line at a time, so until the enclosing delimiter was
    // carried across lines the continuation line held a lone `Function,` with
    // no opener in front of it — and the rule, finding no argument list, read
    // it as a type. `expect.any(…)` is written this way whenever the formatter
    // decides the call is too long for one line, and `check:lib` is an
    // error-level gate on CI, so this was a red build for correct code.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nexpect(handler).toHaveBeenCalledWith(\n  expect.any(\n    Function,\n  ),\n  expect.any(Object),\n);\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unclear_type_still_reads_a_type_in_a_list_opened_on_an_earlier_line() {
    // And the half that proves the carry is a *question* rather than a licence:
    // a function type's parameter list is opened on an earlier line just the
    // same, and a bare `Object` inside one is still an annotation.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Sink = (\n  Object,\n) => void;\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn unclear_type_still_reads_an_annotation_on_a_continued_parameter_list() {
    // A call and a declaration are told apart by what stands before the `(`,
    // and a declaration's parameters carry annotations however they are laid
    // out.
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfunction handle(\n  node: any,\n  rest: Object,\n) {\n  return node;\n}\n",
    );

    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
}

/// ubugeeei-prod/uf#571: a property named `any` is a name, not a type — and
/// the three spellings of the same key were three different answers.
///
/// `@uniflowed/test`'s matcher interface lists Jest's asymmetric matchers, and
/// `readonly any: (expected: mixed) => …` was reported as an unclear type. The
/// only way past it was a suppression, for a rule that was simply wrong.
#[test]
fn a_property_key_is_a_key_however_its_variance_is_written() {
    for source in [
        "// @flow\nexport type M = { any: (expected: mixed) => boolean };\n",
        "// @flow\nexport type M = { readonly any: (expected: mixed) => boolean };\n",
        "// @flow\nexport type M = { +any: (expected: mixed) => boolean };\n",
        "// @flow\nexport type M = { -any: (expected: mixed) => boolean };\n",
        "// @flow\nexport type M = { any?: (expected: mixed) => boolean };\n",
        "// @flow\nexport type M = { readonly any?: (expected: mixed) => boolean };\n",
    ] {
        let diagnostics = lint_js("flow/unclear-type", source);
        assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
    }
}

/// And an `any` that really is a type is still reported, whatever stands
/// before it — otherwise the fix above would be a hole rather than a fix.
#[test]
fn an_annotation_is_still_unclear_beside_a_key_that_is_not() {
    for source in [
        "// @flow\nexport type M = { readonly value: any };\n",
        "// @flow\nexport type M = { +value: any };\n",
        "// @flow\nexport type M = { value?: any };\n",
    ] {
        let diagnostics = lint_js("flow/unclear-type", source);
        assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:?}");
    }
}
