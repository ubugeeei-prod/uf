//! Type annotations, and the `:` positions that are not annotations.

use super::stripped_text;

#[test]
fn parameter_annotations_are_erased() {
    let out = stripped_text("// @flow\nfunction f(a: number, b: string) {\n  return a;\n}\n");

    assert!(out.contains("function f(a, b) {"), "{out}");
}

#[test]
fn optional_parameter_annotations_lose_the_question_mark() {
    let out = stripped_text("// @flow\nfunction f(a?: number) {\n  return a;\n}\n");

    assert!(out.contains("function f(a) {"), "{out}");
}

#[test]
fn default_parameter_values_survive_their_annotation() {
    let out = stripped_text("// @flow\nfunction f(a: number = 1) {\n  return a;\n}\n");

    assert!(out.contains("function f(a = 1) {"), "{out}");
}

#[test]
fn rest_parameter_annotations_are_erased() {
    let out = stripped_text("// @flow\nfunction f(...rest: Array<number>) {\n  return rest;\n}\n");

    assert!(out.contains("function f(...rest) {"), "{out}");
}

#[test]
fn return_type_annotations_are_erased() {
    let out = stripped_text("// @flow\nfunction f(): number {\n  return 1;\n}\n");

    assert!(out.contains("function f() {"), "{out}");
}

#[test]
fn a_function_type_return_annotation_is_erased_whole() {
    let out =
        stripped_text("// @flow\nfunction f(): (a: number) => void {\n  return () => {};\n}\n");

    assert!(out.contains("function f() {"), "{out}");
}

#[test]
fn an_object_type_return_annotation_is_erased_whole() {
    let out = stripped_text("// @flow\nfunction f(): { a: number } {\n  return { a: 1 };\n}\n");

    assert!(out.contains("function f() {"), "{out}");
    assert!(out.contains("return { a: 1};"), "{out}");
}

#[test]
fn an_arrow_return_annotation_stops_before_the_arrow() {
    let out = stripped_text("// @flow\nconst f = (a: number): string => String(a);\n");

    assert!(out.contains("const f = (a) => String(a);"), "{out}");
}

#[test]
fn variable_annotations_are_erased() {
    let out =
        stripped_text("// @flow\nconst a: number = 1;\nlet b: string = \"b\";\nvar c: X = 0;\n");

    assert!(out.contains("const a = 1;"), "{out}");
    assert!(out.contains("let b = \"b\";"), "{out}");
    assert!(out.contains("var c = 0;"), "{out}");
}

#[test]
fn a_destructured_parameter_annotation_is_erased() {
    let out = stripped_text("// @flow\nfunction f({ a, b }: Props) {\n  return a + b;\n}\n");

    assert!(out.contains("function f({ a, b}) {"), "{out}");
}

#[test]
fn class_member_annotations_are_erased() {
    let source = "// @flow\nclass Counter {\n  count: number = 0;\n  step(by: number): number {\n    return by;\n  }\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("count = 0;"), "{out}");
    assert!(out.contains("step(by) {"), "{out}");
}

#[test]
fn generic_call_type_arguments_are_erased() {
    let out = stripped_text("// @flow\nconst q = createQuery<string>({ key: 1 });\n");

    assert!(!out.contains('<'), "{out}");
    assert!(out.contains("createQuery"), "{out}");
}

#[test]
fn generic_call_type_arguments_holding_an_object_type_are_erased() {
    let out = stripped_text("// @flow\nconst l = createLoader<{ +name: string }>(\"viewer\");\n");

    assert!(!out.contains("name"), "{out}");
    assert!(out.contains("createLoader"), "{out}");
}

#[test]
fn a_less_than_comparison_is_not_mistaken_for_type_arguments() {
    let source = "// @flow\nconst ok = a < b;\nif (count < limit) {\n  run();\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("const ok = a < b;"), "{out}");
    assert!(out.contains("if (count < limit) {"), "{out}");
}

#[test]
fn object_literal_keys_are_not_mistaken_for_annotations() {
    let source = "// @flow\nconst o = { a: 1, b: \"two\", c: { d: 3 } };\nconst n = o.a;\n";

    let out = stripped_text(source);

    assert!(out.contains("{ a: 1, b: \"two\", c: { d: 3}}"), "{out}");
}

#[test]
fn a_ternary_inside_a_call_is_not_mistaken_for_an_annotation() {
    let out = stripped_text("// @flow\nconst v = pick(a, b ? c : d);\n");

    assert!(out.contains("pick(a, b ? c : d)"), "{out}");
}

#[test]
fn switch_case_labels_are_not_mistaken_for_annotations() {
    let source =
        "// @flow\nswitch (x) {\n  case 1:\n    run();\n    break;\n  default:\n    stop();\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("case 1:"), "{out}");
    assert!(out.contains("default:"), "{out}");
}
