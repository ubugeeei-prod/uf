//! What the rewritten module looks like.

use super::{compile, module};
use crate::COMPILED_MARKER;

#[test]
fn the_create_call_is_replaced_by_an_object_literal() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    assert!(!compiled.code.contains("stylex.create"));
    assert!(compiled.code.contains("const s = {\"a\":{"));
}

#[test]
fn the_compiled_object_carries_the_marker() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    assert!(compiled.code.contains(COMPILED_MARKER));
}

#[test]
fn a_base_only_property_compiles_to_a_bare_class_name() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    let class = &compiled.styles[0].properties[0].classes[0].class;
    assert!(compiled.code.contains(&format!("\"color\":\"{class}\"")));
}

#[test]
fn a_conditional_property_compiles_to_a_state_map() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: { default: \"black\", \":hover\": \"red\" } } });\n",
    ));
    assert!(compiled.code.contains("\"color\":{\"default\":\""));
    assert!(compiled.code.contains("\":hover\":\""));
}

#[test]
fn everything_outside_the_call_is_left_alone() {
    let source = module(
        "export const before = 1;\nconst s = stylex.create({ a: { color: \"red\" } });\nexport const after = 2;\n",
    );
    let compiled = compile(&source);
    assert!(compiled.code.starts_with("// @flow\nimport { stylex }"));
    assert!(compiled.code.contains("export const before = 1;"));
    assert!(compiled.code.ends_with("export const after = 2;\n"));
}

#[test]
fn two_calls_in_one_module_are_both_rewritten() {
    let compiled = compile(&module(
        "const a = stylex.create({ x: { color: \"red\" } });\nconst b = stylex.create({ y: { color: \"blue\" } });\n",
    ));
    assert!(!compiled.code.contains("stylex.create"));
    assert_eq!(compiled.styles.len(), 2);
}

#[test]
fn a_call_written_across_several_lines_is_replaced_whole() {
    let compiled = compile(&module(
        "const s = stylex.create({\n  a: {\n    color: \"red\",\n  },\n});\n",
    ));
    assert!(!compiled.code.contains("stylex.create"));
    assert!(compiled.code.ends_with("};\n"));
}

#[test]
fn a_shadowed_declaration_keeps_only_the_last_value() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\", color: \"blue\" } });\n",
    ));
    assert_eq!(compiled.sheet.len(), 1, "the dead rule is never emitted");
    let rule = compiled.sheet.rules().next().expect("one rule");
    assert_eq!(rule.value, "blue");
}

#[test]
fn a_shadowed_state_keeps_only_the_last_value() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: { \":hover\": \"red\", \":hover\": \"blue\" } } });\n",
    ));
    assert_eq!(compiled.sheet.len(), 1);
    assert_eq!(
        compiled.sheet.rules().next().expect("one rule").value,
        "blue"
    );
}

#[test]
fn the_rewrite_reports_that_it_changed_the_module() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    assert!(compiled.changed);
}

#[test]
fn a_module_the_pass_does_not_touch_reports_no_change() {
    let compiled = compile("// @flow\nexport const x = 1;\n");
    assert!(!compiled.changed);
    assert_eq!(compiled.code, "// @flow\nexport const x = 1;\n");
}

#[test]
fn a_crlf_module_keeps_its_line_endings_outside_the_call() {
    let source = "// @flow\r\nimport { stylex } from \"@uniflowed/stylex\";\r\nconst s = stylex.create({ a: { color: \"red\" } });\r\n";
    let compiled = compile(source);
    assert!(compiled.code.starts_with("// @flow\r\n"));
    assert!(compiled.code.ends_with("};\r\n"));
}

#[test]
fn a_module_with_a_byte_order_mark_keeps_it() {
    let source = format!(
        "\u{feff}{}",
        module("const s = stylex.create({ a: { color: \"red\" } });\n")
    );
    let compiled = compile(&source);
    assert!(compiled.code.starts_with('\u{feff}'));
    assert_eq!(compiled.sheet.len(), 1);
}

#[test]
fn a_module_with_non_ascii_source_keeps_its_byte_offsets() {
    let source = module(
        "const 見出し = 1;\nconst s = stylex.create({ a: { color: \"red\" } });\nconst 本文 = 2;\n",
    );
    let compiled = compile(&source);
    assert!(compiled.code.contains("const 見出し = 1;"));
    assert!(compiled.code.contains("const 本文 = 2;"));
    assert!(!compiled.code.contains("stylex.create"));
}
