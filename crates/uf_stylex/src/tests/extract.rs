//! What the extractor finds, and what it refuses to guess at.

use super::{compile, module};
use crate::error::StyleXError;
use crate::parse::parse_module;
use crate::{StyleCondition, StyleValue};

#[test]
fn a_module_without_stylex_produces_nothing() {
    let compiled = compile("// @flow\nexport const answer = 42;\n");
    assert!(!compiled.changed);
    assert!(compiled.sheet.is_empty());
    assert!(compiled.styles.is_empty());
}

#[test]
fn a_module_that_imports_stylex_without_calling_it_produces_nothing() {
    let compiled = compile(&module("export const nothing = 1;\n"));
    assert!(!compiled.changed);
    assert!(compiled.sheet.is_empty());
}

#[test]
fn one_namespace_with_one_declaration_is_extracted() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    assert_eq!(compiled.styles.len(), 1);
    assert_eq!(compiled.styles[0].name, "a");
    assert_eq!(compiled.styles[0].properties.len(), 1);
    assert_eq!(compiled.styles[0].properties[0].key, "color");
    assert_eq!(compiled.sheet.len(), 1);
}

#[test]
fn several_namespaces_keep_their_source_order() {
    let compiled = compile(&module(
        "const s = stylex.create({ b: { color: \"red\" }, a: { color: \"blue\" } });\n",
    ));
    let names: Vec<&str> = compiled
        .styles
        .iter()
        .map(|style| style.name.as_str())
        .collect();
    assert_eq!(names, ["b", "a"]);
}

#[test]
fn a_camel_case_key_becomes_a_kebab_case_property() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { minHeight: \"1px\" } });\n",
    ));
    let rule = compiled.sheet.rules().next().expect("one rule");
    assert_eq!(rule.property, "min-height");
}

#[test]
fn a_vendor_prefixed_key_keeps_its_leading_dash() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { WebkitLineClamp: 2 } });\n",
    ));
    let rule = compiled.sheet.rules().next().expect("one rule");
    assert_eq!(rule.property, "-webkit-line-clamp");
}

#[test]
fn a_quoted_key_is_read_the_same_as_a_bare_one() {
    let quoted = compile(&module(
        "const s = stylex.create({ \"a\": { \"color\": \"red\" } });\n",
    ));
    let bare = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    assert_eq!(quoted.sheet.to_css(), bare.sheet.to_css());
}

#[test]
fn a_property_level_conditional_object_is_extracted() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: { default: \"black\", \":hover\": \"red\" } } });\n",
    ));
    assert_eq!(compiled.sheet.len(), 2);
    let property = compiled.styles[0].property("color").expect("a colour");
    assert_eq!(property.classes.len(), 2);
}

#[test]
fn a_condition_level_object_is_extracted() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { \":hover\": { color: \"red\", opacity: 1 } } });\n",
    ));
    assert_eq!(compiled.sheet.len(), 2);
    assert!(
        compiled
            .sheet
            .rules()
            .all(|rule| rule.condition == StyleCondition::parse(":hover").expect("hover"))
    );
}

#[test]
fn an_at_rule_condition_is_extracted() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: { default: \"red\", \"@media (min-width: 600px)\": \"blue\" } } });\n",
    ));
    let css = compiled.sheet.to_css();
    assert!(css.contains("@media (min-width: 600px){"), "got {css}");
}

#[test]
fn a_pseudo_element_condition_is_extracted() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { \"::before\": { content: \"x\" } } });\n",
    ));
    assert!(compiled.sheet.to_css().contains("::before{content:x}"));
}

#[test]
fn a_bare_create_import_is_recognized() {
    let compiled = compile(
        "// @flow\nimport { create } from \"@uniflowed/stylex\";\nconst s = create({ a: { color: \"red\" } });\n",
    );
    assert_eq!(compiled.sheet.len(), 1);
}

#[test]
fn a_star_import_is_recognized() {
    let compiled = compile(
        "// @flow\nimport * as sx from \"@uniflowed/stylex\";\nconst s = sx.create({ a: { color: \"red\" } });\n",
    );
    assert_eq!(compiled.sheet.len(), 1);
}

#[test]
fn a_create_call_from_an_unrelated_object_is_ignored() {
    let compiled = compile(&module(
        "const s = other.create({ a: { color: \"red\" } });\n",
    ));
    assert!(!compiled.changed);
}

#[test]
fn the_text_stylex_create_inside_a_string_is_ignored() {
    let compiled = compile(&module(
        "export const help = \"call stylex.create({ a: { color: 'red' } })\";\n",
    ));
    assert!(!compiled.changed);
}

#[test]
fn the_text_stylex_create_inside_a_comment_is_ignored() {
    let compiled = compile(&module(
        "// const s = stylex.create({ a: { color: \"red\" } });\nexport const x = 1;\n",
    ));
    assert!(!compiled.changed);
}

#[test]
fn a_negative_number_is_a_value() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { marginTop: -4 } });\n",
    ));
    let rule = compiled.sheet.rules().next().expect("one rule");
    assert_eq!(rule.value, "-4px");
}

#[test]
fn a_template_without_a_substitution_is_a_value() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: `red` } });\n",
    ));
    let rule = compiled.sheet.rules().next().expect("one rule");
    assert_eq!(rule.value, "red");
}

#[test]
fn a_template_with_a_substitution_is_refused() {
    let source = module("const s = stylex.create({ a: { color: `${tone}` } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnsupportedValue { .. })
    ));
}

#[test]
fn a_call_expression_value_is_refused() {
    let source = module("const s = stylex.create({ a: { color: pick() } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnsupportedValue { .. })
    ));
}

#[test]
fn a_bare_identifier_value_is_refused() {
    let source = module("const s = stylex.create({ a: { color: brand } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnsupportedValue { .. })
    ));
}

#[test]
fn a_spread_entry_is_refused() {
    let source = module("const s = stylex.create({ a: { ...base, color: \"red\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::MalformedEntry { .. })
    ));
}

#[test]
fn a_computed_key_is_refused() {
    let source = module("const s = stylex.create({ a: { [key]: \"red\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::MalformedEntry { .. })
    ));
}

#[test]
fn a_non_object_argument_is_refused() {
    let source = module("const s = stylex.create(base);\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ExpectedObjectLiteral { .. })
    ));
}

#[test]
fn a_scalar_namespace_is_refused() {
    let source = module("const s = stylex.create({ a: \"red\" });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ExpectedObjectLiteral { .. })
    ));
}

#[test]
fn an_error_carries_the_line_and_column_it_was_found_at() {
    let source = module("const s = stylex.create({\n  a: {\n    color: pick(),\n  },\n});\n");
    let error = parse_module(&source).expect_err("a value uf cannot resolve");
    let at = error.position().expect("a position");
    assert_eq!((at.line, at.column), (5, 12));
}

#[test]
fn a_declaration_records_the_value_it_resolved_to() {
    let source = module("const s = stylex.create({ a: { color: \"red\", opacity: 1 } });\n");
    let parsed = parse_module(&source).expect("a module that parses");
    let declarations = &parsed.creates[0].namespaces[0].declarations;
    assert_eq!(declarations[0].value, StyleValue::Text("red".into()));
    assert_eq!(declarations[1].value, StyleValue::Number("1".into()));
}
