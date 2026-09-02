//! `stylex.defineVars`, and the `tokens.x` references that resolve to it.

use super::compile;
use crate::error::StyleXError;
use crate::parse::parse_module;
use crate::{VARIABLE_PREFIX, variable_name};

/// The scaffolded `app/styles/tokens.stylex.js`, as `uf create app react` writes it.
const TOKENS_MODULE: &str = "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\n\nexport const tokens = stylex.defineVars({\n  canvas: \"#f7f7f2\",\n  ink: \"#151b1f\",\n});\n";

#[test]
fn the_scaffolded_tokens_module_compiles() {
    let compiled = compile(TOKENS_MODULE);
    assert!(compiled.changed);
    let variables: Vec<_> = compiled.sheet.variables().collect();
    assert_eq!(variables.len(), 2);
}

#[test]
fn each_entry_becomes_a_custom_property() {
    let compiled = compile(TOKENS_MODULE);
    let css = compiled.sheet.to_css();
    assert!(css.starts_with(":root{"));
    assert!(css.contains(&format!("{}:#f7f7f2;", variable_name("tokens", "canvas"))));
    assert!(css.contains(&format!("{}:#151b1f;", variable_name("tokens", "ink"))));
}

#[test]
fn the_define_vars_call_is_replaced_by_var_references() {
    let compiled = compile(TOKENS_MODULE);
    assert!(!compiled.code.contains("stylex.defineVars"));
    assert!(compiled.code.contains(&format!(
        "\"canvas\":\"var({})\"",
        variable_name("tokens", "canvas")
    )));
}

#[test]
fn a_variable_name_is_a_custom_property() {
    assert!(variable_name("tokens", "canvas").starts_with(VARIABLE_PREFIX));
}

#[test]
fn a_reference_in_the_declaring_module_resolves() {
    let compiled = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nexport const tokens = stylex.defineVars({ ink: \"#000\" });\nconst s = stylex.create({ a: { color: tokens.ink } });\n",
    );
    let rule = compiled.sheet.rules().next().expect("the colour rule");
    assert_eq!(
        rule.value,
        format!("var({})", variable_name("tokens", "ink"))
    );
}

#[test]
fn a_reference_declared_after_the_use_still_resolves() {
    let compiled = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nconst s = stylex.create({ a: { color: tokens.ink } });\nexport const tokens = stylex.defineVars({ ink: \"#000\" });\n",
    );
    assert_eq!(compiled.sheet.len(), 1);
}

#[test]
fn a_reference_to_an_imported_variables_module_resolves() {
    let compiled = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nimport { tokens } from \"./styles/tokens.stylex.js\";\nconst s = stylex.create({ a: { color: tokens.ink } });\n",
    );
    let rule = compiled.sheet.rules().next().expect("the colour rule");
    assert_eq!(
        rule.value,
        format!("var({})", variable_name("tokens", "ink"))
    );
}

#[test]
fn a_renamed_import_resolves_to_the_exported_name() {
    let renamed = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nimport { tokens as t } from \"./styles/tokens.stylex.js\";\nconst s = stylex.create({ a: { color: t.ink } });\n",
    );
    let plain = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nimport { tokens } from \"./styles/tokens.stylex.js\";\nconst s = stylex.create({ a: { color: tokens.ink } });\n",
    );
    assert_eq!(renamed.sheet.to_css(), plain.sheet.to_css());
}

#[test]
fn a_reference_to_an_unknown_binding_is_refused() {
    let source = "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nconst s = stylex.create({ a: { color: theme.ink } });\n";
    assert!(matches!(
        parse_module(source),
        Err(StyleXError::UnknownVariableBinding { .. })
    ));
}

#[test]
fn a_define_vars_call_with_no_binding_is_refused() {
    let source = "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nstylex.defineVars({ ink: \"#000\" });\n";
    assert!(matches!(
        parse_module(source),
        Err(StyleXError::MalformedEntry { .. })
    ));
}

#[test]
fn a_number_in_define_vars_keeps_its_own_units() {
    // uf cannot know which property a custom property will end up in, so it
    // does not guess pixels the way it does for a declaration.
    let compiled = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nexport const tokens = stylex.defineVars({ weight: 700 });\n",
    );
    let variable = compiled.sheet.variables().next().expect("one variable");
    assert_eq!(variable.value, "700");
}

#[test]
fn two_variables_objects_with_the_same_name_and_value_do_not_conflict() {
    let first = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nexport const tokens = stylex.defineVars({ ink: \"#000\" });\n",
    );
    let second = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nexport const tokens = stylex.defineVars({ ink: \"#000\" });\n",
    );
    let mut sheet = first.sheet.clone();
    sheet.extend(&second.sheet);
    assert!(sheet.variable_conflicts().is_empty());
}

#[test]
fn two_variables_objects_with_the_same_name_and_different_values_conflict() {
    let first = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nexport const tokens = stylex.defineVars({ ink: \"#000\" });\n",
    );
    let second = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nexport const tokens = stylex.defineVars({ ink: \"#fff\" });\n",
    );
    let mut sheet = first.sheet.clone();
    sheet.extend(&second.sheet);

    let conflicts = sheet.variable_conflicts();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].name, variable_name("tokens", "ink"));
    assert_eq!(conflicts[0].values, ["#000", "#fff"]);
}

#[test]
fn a_variables_module_that_declares_no_styles_emits_no_rules() {
    let compiled = compile(TOKENS_MODULE);
    assert_eq!(compiled.sheet.len(), 0);
    assert!(!compiled.sheet.is_empty(), "it still declares variables");
}
