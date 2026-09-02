//! `component` and `hook` declarations becoming plain functions.

use super::super::*;
use super::stripped_text;

#[test]
fn components_become_functions_with_destructured_props() {
    let source = "// @flow\ncomponent Greeting(name: string) {\n  return name;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("function Greeting({name}) {"), "{out}");
}

#[test]
fn a_component_without_props_keeps_an_empty_parameter_list() {
    let out = stripped_text("// @flow\ncomponent Page() {\n  return null;\n}\n");

    assert!(out.contains("function Page() {"), "{out}");
}

#[test]
fn a_renders_clause_is_erased() {
    let source = "// @flow\ncomponent Page() renders React.Node {\n  return null;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("function Page() {"), "{out}");
    assert!(!out.contains("renders"), "{out}");
}

#[test]
fn a_component_with_optional_props_destructures_them() {
    let source = "// @flow\ncomponent Box(a?: number, b?: string) {\n  return a;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("function Box({a, b}) {"), "{out}");
}

#[test]
fn a_component_with_a_renamed_prop_keeps_its_parameter_list() {
    let source = "// @flow\ncomponent Box('data-id' as id: string) {\n  return id;\n}\n";

    let stripped = strip_types(source).expect("strips");

    assert!(stripped.code.contains("function "), "{}", stripped.code);
    assert!(!stripped.code.contains("({"), "{}", stripped.code);
}

#[test]
fn hooks_become_functions() {
    let source =
        "// @flow\nexport hook useCount(initial: number): number {\n  return initial;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("export function useCount(initial) {"), "{out}");
}

#[test]
fn component_rewriting_keeps_the_line_count_of_a_multi_line_signature() {
    let source = "// @flow\ncomponent Card(\n  title: string,\n  body: string,\n) renders Node {\n  return title;\n}\n";

    let stripped = strip_types(source).expect("strips");

    assert_eq!(stripped.code.lines().count(), source.lines().count());
    let outcome = crate::validate_source(&stripped.code).expect("parser ran");
    assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
}

#[test]
fn the_scaffolded_client_hook_strips_to_javascript() {
    let source = "\"use client\";\n// @flow\nimport { useState } from \"@uniflowed/react\";\n\nexport hook useCounter(initial: number): [number, () => void] {\n  const [count, setCount] = useState(initial);\n  return [count, () => setCount(count + 1)];\n}\n";

    let out = stripped_text(source);

    assert!(out.starts_with("\"use client\";"), "{out}");
    assert!(
        out.contains("export function useCounter(initial) {"),
        "{out}"
    );
}
