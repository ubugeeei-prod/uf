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

#[test]
fn unclear_type_ignores_a_new_operand() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfoo(new any);\nfoo(new Object);\nfoo(new Function);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype C = new (x: any) => void;\ntype Items = Array<Object>;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_new_operand() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nfoo(new bool);\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_thrown_value() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nthrow any;\nthrow Object;\nthrow Function;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Box = any;\ntype Items = Array<Object>;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_thrown_value() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nthrow bool;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_thrown_value() {
    let diagnostics = lint_js("flow/internal-type", "// @flow\nthrow React$Node;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_new_operand() {
    let diagnostics = lint_js("flow/internal-type", "// @flow\nfoo(new React$Node);\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_an_export_default() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nexport default any;\nexport default Object;\nexport default Function;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nexport type Box = any;\ntype Items = Array<Object>;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_an_export_default() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nexport default bool;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_an_export_default() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nexport default React$Node;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_class_superclass() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nclass Box extends Object {}\nclass Fun extends Function {}\nclass Slot<T> extends Object {}\nexport default class Anon extends Function {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ninterface Box extends Object {}\ninterface Fun extends Function {}\ninterface Slot<T> extends Object {}\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_class_superclass() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nclass Flag extends bool {}\nclass Wrapped<T> extends bool {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ninterface Flag extends bool {}\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_reads_a_type_arrow_return() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Handler = () => any;\ntype Items = () => Object;\ntype Ctor = new () => Function;\ntype Param = (value: string) => Object;\ntype Box<T = () => any> = T;\n",
    );
    assert_eq!(diagnostics.len(), 5, "{diagnostics:?}");

    let quiet = lint_js(
        "flow/unclear-type",
        "// @flow\nconst body = () => any;\nconst typed = (value: string) => Object;\nconst generic = <A>(value: A) => Function;\nif (count > any) {}\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn deprecated_type_reads_a_type_arrow_return() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype Flag = () => bool;\ntype Ctor = new () => bool;\n",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");

    let quiet = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst body = () => bool;\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn internal_type_reads_a_type_arrow_return() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = () => React$Node;\ntype Ctor = new () => React$Node;\n",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");

    let quiet = lint_js(
        "flow/internal-type",
        "// @flow\nconst body = () => React$Node;\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn unclear_type_ignores_a_typed_value_arrow() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst callback: Handler = () => any;\nlet ctor: Handler = () => Object;\nvar make: Handler = () => Function;\nexport const exported: Handler = () => any;\nfunction take(callback: Handler = () => any) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nopaque type Box: Super = () => any;\nopaque type Wrapped<T>: Super = () => Object;\ntype Bound<T: Super = () => Function> = T;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_typed_value_arrow() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst flag: Handler = () => bool;\nlet other: Handler = () => bool;\nvar make: Handler = () => bool;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype Flag = () => bool;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_typed_value_arrow() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst node: Handler = () => React$Node;\nlet other: Handler = () => React$Node;\nvar make: Handler = () => React$Node;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = () => React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_class_superclass() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nclass Box extends React$Node {}\nclass Slot<T> extends React$Node {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ninterface Box extends React$Node {}\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_class_superclass_after_an_arrow_default() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nclass Box<T = () => any> extends Object {}\nclass Fun<T = () => Function> extends Object {}\nclass Nested<T = Foo<any>> extends Object {}\nclass Deep<T = () => Array<any>> extends Object {}\n",
    );
    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 21));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 21));
    assert_eq!((diagnostics[2].line, diagnostics[2].column), (4, 22));
    assert_eq!((diagnostics[3].line, diagnostics[3].column), (5, 28));

    let nested = lint_js(
        "flow/unclear-type",
        "// @flow\nclass Box<Foo<T>> extends Object {}\n",
    );
    assert!(nested.is_empty(), "{nested:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ninterface Box<T = () => any> extends Object {}\nopaque type Box<T = () => any>: Super = () => Object;\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 25));
    assert_eq!((still[1].line, still[1].column), (2, 38));
    assert_eq!((still[2].line, still[2].column), (3, 27));
    assert_eq!((still[3].line, still[3].column), (3, 47));
}

#[test]
fn deprecated_type_ignores_a_class_superclass_after_an_arrow_default() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nclass Slot<T = () => bool> extends bool {}\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 22));

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ninterface Slot<T = () => bool> extends bool {}\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn internal_type_ignores_a_class_superclass_after_an_arrow_default() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nclass Slot<T = () => React$Node> extends React$Node {}\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 22));
}

#[test]
fn unclear_type_reads_an_annotation_before_an_initializer() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst value: any = 1;\nlet made: Object = {};\nvar ctor: Function = foo;\nexport const exported: any = 1;\nfunction take(value: any = 1) {}\nfunction take2(first: string, value: Object = {}) {}\nfunction typed(first: { a: string }, value: Object = {}) {}\nclass Box { value: Function = foo; }\nclass Sub extends Base { value: any = 1; }\nexport default class { value: Object = {}; }\nclass Stat { static value: any = 1; }\nclass Gen<T> { value: any = 1; next: Object = {}; }\n",
    );
    assert_eq!(diagnostics.len(), 13, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 14));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 11));
    assert_eq!((diagnostics[2].line, diagnostics[2].column), (4, 11));
    assert_eq!((diagnostics[3].line, diagnostics[3].column), (5, 24));
    assert_eq!((diagnostics[4].line, diagnostics[4].column), (6, 22));
    assert_eq!((diagnostics[5].line, diagnostics[5].column), (7, 38));
    assert_eq!((diagnostics[6].line, diagnostics[6].column), (8, 45));
    assert_eq!((diagnostics[7].line, diagnostics[7].column), (9, 20));
    assert_eq!((diagnostics[8].line, diagnostics[8].column), (10, 33));
    assert_eq!((diagnostics[9].line, diagnostics[9].column), (11, 31));
    assert_eq!((diagnostics[10].line, diagnostics[10].column), (12, 28));
    assert_eq!((diagnostics[11].line, diagnostics[11].column), (13, 23));
    assert_eq!((diagnostics[12].line, diagnostics[12].column), (13, 38));

    let quiet = lint_js(
        "flow/unclear-type",
        "// @flow\nlabel: any = 1;\nfunction f() { label: any = 1; }\nclass Box { m() { label: any = 1; } }\nconst { any = 1 } = obj;\nconst { a, b: any = 1 } = obj;\nfunction g({ a, b: Object = {} }) {}\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Box<T = any> = T;\ntype Handler = Function;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn deprecated_type_reads_an_annotation_before_an_initializer() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst flag: bool = true;\nfunction take(flag: bool = true) {}\nclass Box { flag: bool = true; }\n",
    );
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_reads_an_annotation_before_an_initializer() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst node: React$Node = null;\nclass Box { node: React$Node = null; }\n",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");

    let still = lint_js("flow/internal-type", "// @flow\ntype Slot = React$Node;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_default_import() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nimport any from \"./mod\";\nimport Object from \"./mod\";\nimport * as Function from \"./mod\";\nimport any, { extra } from \"./mod\";\nexport * as any from \"./mod\";\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nimport type any from \"./mod\";\nimport type { any } from \"./mod\";\nimport { any } from \"./mod\";\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 13));
    assert_eq!((still[1].line, still[1].column), (3, 15));
}

#[test]
fn deprecated_type_ignores_a_default_import() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nimport bool from \"./postgresql.js\";\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\nimport type bool from \"./types.js\";\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_default_import() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nimport React$Node from \"./mod\";\nimport * as React$Node from \"./mod\";\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nimport type { React$Node } from \"./mod\";\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_reads_a_binding_arrow_annotation() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nlet callback: () => any;\nconst made: () => Object = () => ({});\nvar ctor: () => Function;\nexport const exported: () => any = () => null;\nfunction take(callback: () => any) {}\nfunction take2(first: string, callback: () => Object) {}\n",
    );
    assert_eq!(diagnostics.len(), 6, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 21));
    assert_eq!((diagnostics[1].line, diagnostics[1].column), (3, 19));
    assert_eq!((diagnostics[2].line, diagnostics[2].column), (4, 17));
    assert_eq!((diagnostics[3].line, diagnostics[3].column), (5, 30));
    assert_eq!((diagnostics[4].line, diagnostics[4].column), (6, 31));
    assert_eq!((diagnostics[5].line, diagnostics[5].column), (7, 47));

    let quiet = lint_js(
        "flow/unclear-type",
        "// @flow\nconst body = () => any;\ncond ? 1 : () => any;\ncond ? (1) : () => any;\nconst obj = { callback: () => any };\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");

    let still = lint_js("flow/unclear-type", "// @flow\ntype Handler = () => any;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_reads_a_binding_arrow_annotation() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nlet flag: () => bool;\nfunction take(flag: () => bool) {}\n",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype Flag = () => bool;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_reads_a_binding_arrow_annotation() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nlet node: () => React$Node;\nconst made: () => React$Node = () => null;\n",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = () => React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_reads_a_function_return_arrow() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfunction make(): () => any { return () => null; }\nclass Box { make(): () => Object { return () => ({}); } }\nexport default function(): () => Function { return () => null; }\n",
    );
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");

    let quiet = lint_js(
        "flow/unclear-type",
        "// @flow\ncond ? (1) : () => any;\nconst body = () => any;\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");

    let still = lint_js("flow/unclear-type", "// @flow\ntype Handler = () => any;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_reads_a_function_return_arrow() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfunction make(): () => bool { return () => true; }\n",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype Flag = () => bool;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_reads_a_function_return_arrow() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfunction make(): () => React$Node { return () => null; }\nclass Box { make(): () => React$Node { return () => null; } }\n",
    );
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = () => React$Node;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_deleted_value() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ndelete any;\ndelete Object;\ndelete Function;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/unclear-type", "// @flow\ntype T = any;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_deleted_value() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\ndelete bool;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_deleted_value() {
    let diagnostics = lint_js("flow/internal-type", "// @flow\ndelete React$Node;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/internal-type", "// @flow\ntype Slot = React$Node;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_an_optional_parameter() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfunction take(any?) {}\nfunction take2(any?, other) {}\nfunction take3(first, any?) {}\nfunction take4(Object?) {}\nfunction take5(Function?) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let annotated = lint_js(
        "flow/unclear-type",
        "// @flow\nfunction take(any?: string) {}\n",
    );
    assert!(annotated.is_empty(), "{annotated:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype T = any ? U : V;\ntype Wrapped = (any ? U : V);\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 10));
    assert_eq!((still[1].line, still[1].column), (3, 17));
}

#[test]
fn deprecated_type_ignores_an_optional_parameter() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfunction take(bool?) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_an_optional_parameter() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfunction take(React$Node?) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/internal-type", "// @flow\ntype Slot = React$Node;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_an_introduced_value() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfunction give() { return React$Node; }\nlet React$Node;\nvar React$Node;\nconst React$Node;\nexport let React$Node;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = React$Node;\nfunction f(): React$Node { return null; }\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 13));
    assert_eq!((still[1].line, still[1].column), (3, 15));
}

#[test]
fn internal_type_ignores_an_assigned_value() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst made = React$Node;\nmade = React$Node;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = React$Node;\ntype Box<T = React$Node> = T;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 13));
    assert_eq!((still[1].line, still[1].column), (3, 14));
}

#[test]
fn internal_type_ignores_a_call_argument() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfoo(React$Node);\nfoo(a, React$Node);\nfunction h(React$Node) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype F = (React$Node) => void;\ntype G = (x: React$Node) => void;\ntype A = Array<React$Node>;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 11));
    assert_eq!((still[1].line, still[1].column), (3, 14));
    assert_eq!((still[2].line, still[2].column), (4, 16));
}

#[test]
fn internal_type_ignores_a_parameter_name() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfunction h(React$Node: string) {}\nfunction i(React$Node?: string) {}\nfunction j(first: string, React$Node: number) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype F = (x: React$Node) => void;\ntype Slot = React$Node;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14));
    assert_eq!((still[1].line, still[1].column), (3, 13));
}

#[test]
fn internal_type_ignores_a_call_or_a_member() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nobj.React$Node;\nReact$Node.foo;\nobj?.React$Node;\nReact$Node();\nclass C { React$Node() {} }\nclass D { get React$Node() { return 1; } }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ndeclare function f(): React$Node;\ntype Slot = React$Node;\ntype A = Array<React$Node>;\ntype F = (x: React$Node) => void;\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 23));
    assert_eq!((still[1].line, still[1].column), (3, 13));
    assert_eq!((still[2].line, still[2].column), (4, 16));
    assert_eq!((still[3].line, still[3].column), (5, 14));
}

#[test]
fn internal_type_ignores_a_case_label() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nswitch (x) {\n  case React$Node:\n    break;\n}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/internal-type", "// @flow\ntype Slot = React$Node;\n");
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 13));
}

#[test]
fn internal_type_ignores_a_property_key() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst o = { React$Node: 1 };\nclass C { React$Node: string; }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nclass C { x: React$Node; }\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14));
}

#[test]
fn internal_type_ignores_an_equality_operand() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nx == React$Node;\nx === React$Node;\nx != React$Node;\nx !== React$Node;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/internal-type", "// @flow\ntype Slot = React$Node;\n");
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 13));
}

#[test]
fn internal_type_ignores_a_named_import_or_export() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nimport { React$Node } from \"./m\";\nimport { foo as React$Node } from \"./m\";\nexport { React$Node };\nexport { local as React$Node };\nexport { React$Node } from \"./m\";\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nimport type { React$Node } from \"./m\";\nimport { type React$Node } from \"./m\";\nexport type { React$Node };\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 15));
    assert_eq!((still[1].line, still[1].column), (3, 15));
    assert_eq!((still[2].line, still[2].column), (4, 15));
}

#[test]
fn unclear_type_ignores_a_break_or_continue_label() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nbreak any;\ncontinue Object;\ncontinue Function;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/unclear-type", "// @flow\ntype T = any;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_break_or_continue_label() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\ncontinue bool;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_break_or_continue_label() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nbreak React$Node;\ncontinue React$Node;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/internal-type", "// @flow\ntype Slot = React$Node;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_declared_name() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nclass any {}\nclass Object {}\nexport class Function {}\ndeclare class any {}\ninterface any {}\nenum any {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn deprecated_type_ignores_a_declared_name() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nclass bool {}\ninterface bool {}\nenum bool {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_declared_name() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nclass React$Node {}\nexport class React$Node {}\ndeclare class React$Node {}\ninterface React$Node {}\nenum React$Node {}\nclass Box extends React$Node {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nclass C implements React$Node {}\ninterface I extends React$Node {}\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 20));
    assert_eq!((still[1].line, still[1].column), (3, 21));
}

#[test]
fn unclear_type_ignores_an_optional_call_argument() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfoo?.(any);\nfoo?.(Object);\nfoo?.(Function);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/unclear-type", "// @flow\ntype F = (any) => void;\n");
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 11));
}

#[test]
fn deprecated_type_ignores_an_optional_call_argument() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nfoo?.(bool);\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn internal_type_ignores_an_optional_call_argument() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfoo?.(React$Node);\nfoo?.(\n  React$Node,\n);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype F = (React$Node) => void;\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 11));
}

#[test]
fn unclear_type_ignores_a_template_tag() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nany`x`;\nObject`x`;\nFunction`x`;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/unclear-type", "// @flow\ntype T = any;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_template_tag() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nbool`x`;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_template_tag() {
    let diagnostics = lint_js("flow/internal-type", "// @flow\nReact$Node`x`;\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/internal-type", "// @flow\ntype Slot = React$Node;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_bare_name_statement() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nany;\nObject;\nFunction;\nfunction f() { any; }\nclass C { any; }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype T = any;\nclass C { x: any; }\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_bare_name_statement() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nbool;\nfunction f() { bool; }\nclass C { bool; }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype Flag = bool;\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_bare_name_statement() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nReact$Node;\nfunction f() { React$Node; }\nclass C { React$Node; }\nclass D { React$Node }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot = React$Node;\nclass C { x: React$Node; }\ndeclare function f(): React$Node;\nexport type { React$Node };\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
}

#[test]
fn unclear_type_still_reads_a_continued_alias() {
    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype T =\n  any;\ntype Items =\n  Object;\ntype Handler =\n  Function;\nexport type Extra =\n\n  any;\ntype Box<T> =\n  any;\n",
    );
    assert_eq!(
        still.iter().map(|item| item.line).collect::<Vec<_>>(),
        vec![3, 5, 7, 10, 12],
        "{still:?}"
    );

    let quiet = lint_js(
        "flow/unclear-type",
        "// @flow\nconst value =\n  any;\nlet ctor =\n  Object;\nctor =\n  Function;\nclass C {\n  x =\n    any;\n}\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn deprecated_type_still_reads_a_continued_alias() {
    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype Flag =\n  bool;\nopaque type Hidden: Super =\n  bool;\n",
    );
    assert_eq!(
        still.iter().map(|item| item.line).collect::<Vec<_>>(),
        vec![3, 5],
        "{still:?}"
    );

    let quiet = lint_js("flow/deprecated-type", "// @flow\nconst flag =\n  bool;\n");
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn internal_type_still_reads_a_continued_alias() {
    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Slot =\n  React$Node;\ntype Box<T> =\n  React$Node;\nopaque type Hidden: Super =\n  React$Node;\n",
    );
    assert_eq!(
        still.iter().map(|item| item.line).collect::<Vec<_>>(),
        vec![3, 5, 7],
        "{still:?}"
    );

    let quiet = lint_js(
        "flow/internal-type",
        "// @flow\nconst slot =\n  React$Node;\n",
    );
    assert!(quiet.is_empty(), "{quiet:?}");
}

#[test]
fn unclear_type_ignores_a_shorthand_binding() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst { any } = obj;\nconst x = { any };\nfunction f({ any }) {}\ntry {} catch ({ any }) {}\n({ any } = obj);\nconst { any, Object } = obj;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nexport type { any };\ntype T = { x: any };\nconst expr = <p>{any}</p>;\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_shorthand_binding() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst { bool } = obj;\nfunction f({ bool }) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\nexport type { bool };\nconst expr = <p>{bool}</p>;\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
}

#[test]
fn internal_type_ignores_a_shorthand_binding() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst { React$Node } = obj;\nconst x = { React$Node };\nfunction f({ React$Node }) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nexport type { React$Node };\nimport type { React$Node };\ntype T = { x: React$Node };\nconst expr = <p>{React$Node}</p>;\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
}

#[test]
fn unclear_type_ignores_a_renamed_binding() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst { a: any } = obj;\nfunction f({ a: any }) {}\n({ a: any } = obj);\nconst { a: { b: any } } = obj;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype T = { a: any };\nfunction f(): { a: any } { return { a: 1 }; }\nfunction g(props: { a: any }) {}\nclass C { x: any; }\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_renamed_binding() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst { a: bool } = obj;\nfunction f({ a: bool }) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype T = { a: bool };\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_a_renamed_binding() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst { a: React$Node } = obj;\nfunction f({ a: React$Node }) {}\n({ a: React$Node } = obj);\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype T = { a: React$Node };\nfunction f(): { a: React$Node } { return { a: 1 }; }\nclass C { x: React$Node; }\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn unclear_type_ignores_an_object_value() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst x = { a: any };\nlet y = { a: { b: Object } };\nvar z = { a: Function };\nx = { a: any };\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype T = { a: any };\nfunction f(): { a: any } { return { a: 1 }; }\nfunction g(props: { a: any }) {}\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_an_object_value() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nconst x = { a: bool };\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype T = { a: bool };\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_an_object_value() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst x = { a: React$Node };\nconst y = { a: { b: React$Node } };\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype T = { a: React$Node };\nfunction f(): { a: React$Node } { return { a: 1 }; }\nfunction g(props: { a: React$Node }) {}\nclass C { x: React$Node; }\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
}

#[test]
fn unclear_type_ignores_an_array_binding() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst [any] = xs;\nfunction f([any]) {}\nconst [any, Object] = xs;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype T = [any];\nfunction f(): [any] { return []; }\nfunction g([value]: [any]) {}\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn deprecated_type_ignores_an_array_binding() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nconst [bool] = xs;\nfunction f([bool]) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\ntype T = [bool];\n");
    assert_eq!(still.len(), 1, "{still:?}");
}

#[test]
fn internal_type_ignores_an_array_binding() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nconst [React$Node] = xs;\nfunction f([React$Node]) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype T = [React$Node];\nfunction f(): [React$Node] { return []; }\nfunction g([value]: [React$Node]) {}\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
}

#[test]
fn unclear_type_ignores_an_enum_member() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nenum E { any }\nenum F { any, Object }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn deprecated_type_ignores_an_enum_member() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nenum E { bool, }\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn internal_type_ignores_an_enum_member() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nenum E { React$Node }\nenum F of string { React$Node }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nenum E of React$Node { A }\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 11), "{still:?}");
}

#[test]
fn unclear_type_ignores_a_for_of_binding() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfor (any of items) {}\nasync function f() { for await (any of items) {} }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn deprecated_type_ignores_a_for_of_binding() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nfor (bool of items) {}\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn internal_type_ignores_a_for_of_binding() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfor (React$Node of items) {}\nasync function f() { for await (React$Node of items) {} }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nfor (const item of React$Node) {}\nfor await (const item of React$Node) {}\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 20), "{still:?}");
}

#[test]
fn unclear_type_ignores_a_parameter_default() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfunction f(any = Object) {}\nconst g = (any = Object) => any;\nfunction h(value = Object) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nfunction f<T = any>() {}\nfunction g<T: any>() {}\nfunction h<T = any>(value = Object) {}\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 16), "{still:?}");
    assert_eq!((still[1].line, still[1].column), (3, 15), "{still:?}");
    assert_eq!((still[2].line, still[2].column), (4, 16), "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_parameter_default() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfunction f(value = bool) {}\nconst g = (value = bool) => value;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\nfunction f<T: bool>() {}\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 15), "{still:?}");
}

#[test]
fn internal_type_ignores_a_parameter_default() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfunction f(value = React$Node) {}\nfunction g(value: string = React$Node) {}\nconst h = (value = React$Node) => value;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype Box<T = React$Node> = T;\nfunction f<T = React$Node>() {}\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14), "{still:?}");
    assert_eq!((still[1].line, still[1].column), (3, 16), "{still:?}");
}

#[test]
fn unclear_type_ignores_a_type_parameter() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\ntype Box<any> = number;\nfunction f<any>(value: string) {}\ntype F = <any>(value: string) => void;\ntype Pair<any, Object> = number;\nclass C<any> {}\ninterface I<any> {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\ntype T = Box<any>;\nconst x = f<any>(1);\nfunction g<T: any>() {}\ntype Box<T = any> = T;\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14), "{still:?}");
    assert_eq!((still[1].line, still[1].column), (3, 13), "{still:?}");
    assert_eq!((still[2].line, still[2].column), (4, 15), "{still:?}");
    assert_eq!((still[3].line, still[3].column), (5, 14), "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_type_parameter() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype Box<bool> = number;\nfunction f<bool>(value: string) {}\ntype F = <bool>(value: string) => void;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\ntype T = Box<bool>;\nfunction f<T: bool>() {}\n",
    );
    assert_eq!(still.len(), 2, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14), "{still:?}");
    assert_eq!((still[1].line, still[1].column), (3, 15), "{still:?}");
}

#[test]
fn internal_type_ignores_a_type_parameter() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\ntype Box<React$Node> = number;\nfunction f<React$Node>(value: string) {}\nopaque type Hidden<React$Node> = number;\ndeclare function g<React$Node>(): void;\ntype F = <React$Node>(value: string) => void;\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\ntype T = Box<React$Node>;\nconst x = f<React$Node>(1);\nconst y = new Foo<React$Node>();\ntype Box<T = React$Node> = T;\n",
    );
    assert_eq!(still.len(), 4, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14), "{still:?}");
    assert_eq!((still[1].line, still[1].column), (3, 13), "{still:?}");
    assert_eq!((still[2].line, still[2].column), (4, 19), "{still:?}");
    assert_eq!((still[3].line, still[3].column), (5, 14), "{still:?}");
}

#[test]
fn unclear_type_ignores_a_private_name() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nclass C { #any; }\nclass D { #Object; }\nclass E { #Function; }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/unclear-type", "// @flow\nclass C { #x: any; }\n");
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 15), "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_private_name() {
    let diagnostics = lint_js("flow/deprecated-type", "// @flow\nclass C { #bool; }\n");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\nclass C { #x: bool; }\n");
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 15), "{still:?}");
}

#[test]
fn internal_type_ignores_a_private_name() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nclass C { #React$Node; }\nclass D { #React$Node: string; }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nclass C { #x: React$Node; }\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 15), "{still:?}");
}

#[test]
fn unclear_type_ignores_an_unannotated_class_field() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nclass C { x: string; any }\nclass D { x: string; Object }\nclass E { x: string; Function }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nclass C { x: any }\ndeclare function f(): any;\nexport type { any }\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14), "{still:?}");
    assert_eq!((still[1].line, still[1].column), (3, 23), "{still:?}");
    assert_eq!((still[2].line, still[2].column), (4, 15), "{still:?}");
}

#[test]
fn deprecated_type_ignores_an_unannotated_class_field() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nclass C { x: string; bool }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/deprecated-type", "// @flow\nclass C { x: bool }\n");
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14), "{still:?}");
}

#[test]
fn internal_type_ignores_an_unannotated_class_field() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nclass C { x: string; React$Node }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nclass C { x: React$Node }\ndeclare function f(): React$Node;\nexport type { React$Node }\n",
    );
    assert_eq!(still.len(), 3, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 14), "{still:?}");
    assert_eq!((still[1].line, still[1].column), (3, 23), "{still:?}");
    assert_eq!((still[2].line, still[2].column), (4, 15), "{still:?}");
}

#[test]
fn unclear_type_ignores_a_decorator() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nclass C { @any method() {} }\nclass D { @Object field = 1 }\n@any()\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js("flow/unclear-type", "// @flow\nclass C { @dec x: any }\n");
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 19), "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_decorator() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nclass C { @bool method() {} }\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\nclass C { @dec x: bool }\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 19), "{still:?}");
}

#[test]
fn internal_type_ignores_a_decorator() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nclass C { @React$Node method() {} }\nclass D { @React$Node field = 1 }\n@React$Node.foo\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nclass C { @dec x: React$Node }\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 19), "{still:?}");
}

#[test]
fn unclear_type_ignores_a_for_initializer() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nfor (any; i < n; i++) {}\nfor (Object; i < n; i++) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/unclear-type",
        "// @flow\nfor (const item of any) {}\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 20), "{still:?}");
}

#[test]
fn deprecated_type_ignores_a_for_initializer() {
    let diagnostics = lint_js(
        "flow/deprecated-type",
        "// @flow\nfor (bool; i < n; i++) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/deprecated-type",
        "// @flow\nfor (const item of bool) {}\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 20), "{still:?}");
}

#[test]
fn internal_type_ignores_a_for_initializer() {
    let diagnostics = lint_js(
        "flow/internal-type",
        "// @flow\nfor (React$Node; i < n; i++) {}\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let still = lint_js(
        "flow/internal-type",
        "// @flow\nfor (const item of React$Node) {}\n",
    );
    assert_eq!(still.len(), 1, "{still:?}");
    assert_eq!((still[0].line, still[0].column), (2, 20), "{still:?}");
}
