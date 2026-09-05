//! `stylex.createTheme`: replacing a token the compiler has already named.
//!
//! A preset nobody can replace is not a default, so this is the half of the
//! preset that makes the other half optional. Every test here is about the same
//! promise: the override writes the *same* custom property `defineVars`
//! declared, at build time, and merges the way every other compiled namespace
//! merges.

use super::compile;
use crate::error::StyleXError;
use crate::parse::parse_module;
use crate::props::props_of;
use crate::variable_name;

/// A module that declares three tokens and then does `body` with them.
fn themed(body: &str) -> String {
    format!(
        "// @flow\nimport {{ stylex }} from \"@uniflowed/stylex\";\n\
         export const tokens = stylex.defineVars({{ canvas: \"#fff\", ink: \"#000\", gap: 8 }});\n\
         {body}"
    )
}

#[test]
fn the_theme_call_is_replaced_by_a_marked_object() {
    let compiled = compile(&themed(
        "export const dark = stylex.createTheme(tokens, { canvas: \"#111\" });\n",
    ));
    assert!(!compiled.code.contains("stylex.createTheme"));
    assert!(
        compiled
            .code
            .contains("export const dark = {\"$$css\":true,")
    );
}

#[test]
fn an_override_writes_the_custom_property_the_token_declared() {
    // The point of the whole feature: the theme's rule and the `var(--…)` the
    // rules below it resolve have to be the same name, or a theme changes
    // nothing at all.
    let compiled = compile(&themed(
        "export const dark = stylex.createTheme(tokens, { canvas: \"#111\" });\n",
    ));
    let rule = compiled.sheet.rules().next().expect("the override rule");
    assert_eq!(rule.property, variable_name("tokens", "canvas"));
    assert_eq!(rule.value, "#111");
}

#[test]
fn a_theme_is_not_one_of_the_modules_styles() {
    let compiled = compile(&themed(
        "export const dark = stylex.createTheme(tokens, { canvas: \"#111\" });\n",
    ));
    assert_eq!(compiled.themes.len(), 1);
    assert!(
        compiled.styles.is_empty(),
        "a theme merged with a style would type-check and mean nothing"
    );
}

#[test]
fn a_number_in_a_theme_keeps_its_own_units() {
    // `defineVars` does not guess pixels for a token, because it cannot know
    // which property will use it. A theme that guessed differently would change
    // the meaning of every rule that read the token it replaced.
    let compiled = compile(&themed(
        "export const dense = stylex.createTheme(tokens, { gap: 4 });\n",
    ));
    let rule = compiled.sheet.rules().next().expect("the override rule");
    assert_eq!(rule.value, "4");
}

#[test]
fn a_conditional_override_is_emitted_inside_its_at_rule() {
    let compiled = compile(&themed(
        "export const auto = stylex.createTheme(tokens, { canvas: { \"@media (prefers-color-scheme: dark)\": \"#111\" } });\n",
    ));
    let css = compiled.sheet.to_css();
    assert!(
        css.contains("@media (prefers-color-scheme: dark){"),
        "expected a media block in {css}"
    );
}

#[test]
fn a_conditional_override_compiles_to_the_state_map_the_runtime_reads() {
    let compiled = compile(&themed(
        "export const auto = stylex.createTheme(tokens, { canvas: { \"@media (prefers-color-scheme: dark)\": \"#111\" } });\n",
    ));
    let class = &compiled.themes[0].properties[0].classes[0].class;
    assert!(
        compiled.code.contains(&format!(
            "\"@media (prefers-color-scheme: dark)\":\"{class}\""
        )),
        "expected a state map in {}",
        compiled.code
    );
}

#[test]
fn a_conditional_override_costs_nothing_in_the_state_it_does_not_name() {
    // An auto theme that only writes a dark value has no unconditional rule at
    // all, which is what makes it free in light mode.
    let compiled = compile(&themed(
        "export const auto = stylex.createTheme(tokens, { canvas: { \"@media (prefers-color-scheme: dark)\": \"#111\" } });\n",
    ));
    assert_eq!(compiled.sheet.len(), 1);
    assert!(
        compiled
            .sheet
            .rules()
            .all(|rule| rule.condition.at_rule().is_some())
    );
}

#[test]
fn two_themes_that_give_a_token_the_same_value_share_one_rule() {
    // The class is hashed over the token namespace, not the binding the theme
    // was assigned to, so repeating a colour costs one rule and not two.
    let compiled = compile(&themed(
        "export const a = stylex.createTheme(tokens, { canvas: \"#111\" });\n\
         export const b = stylex.createTheme(tokens, { canvas: \"#111\" });\n",
    ));
    assert_eq!(compiled.themes.len(), 2);
    assert_eq!(compiled.sheet.len(), 1);
}

#[test]
fn a_later_theme_replaces_an_earlier_one_token_by_token() {
    let compiled = compile(&themed(
        "export const a = stylex.createTheme(tokens, { canvas: \"#111\", ink: \"#eee\" });\n\
         export const b = stylex.createTheme(tokens, { canvas: \"#222\" });\n",
    ));
    let merged = props_of(&compiled.themes);
    assert_eq!(merged.len(), 2, "one canvas and one ink survive");

    let winner = &compiled.themes[1]
        .property(&variable_name("tokens", "canvas"))
        .expect("the canvas override")
        .classes[0]
        .class;
    assert!(merged.class_name().contains(winner.as_str()));

    let loser = &compiled.themes[0]
        .property(&variable_name("tokens", "canvas"))
        .expect("the canvas override")
        .classes[0]
        .class;
    assert!(!merged.class_name().contains(loser.as_str()));
}

#[test]
fn a_theme_may_point_one_token_at_another() {
    let compiled = compile(&themed(
        "export const flat = stylex.createTheme(tokens, { canvas: tokens.ink });\n",
    ));
    let rule = compiled.sheet.rules().next().expect("the override rule");
    assert_eq!(
        rule.value,
        format!("var({})", variable_name("tokens", "ink"))
    );
}

#[test]
fn a_theme_over_an_imported_variables_module_resolves() {
    let compiled = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\n\
         import { tokens } from \"./styles/tokens.stylex.js\";\n\
         export const dark = stylex.createTheme(tokens, { canvas: \"#111\" });\n",
    );
    let rule = compiled.sheet.rules().next().expect("the override rule");
    assert_eq!(rule.property, variable_name("tokens", "canvas"));
}

#[test]
fn a_renamed_variables_import_themes_the_same_custom_property() {
    let renamed = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\n\
         import { tokens as t } from \"./tokens.stylex.js\";\n\
         export const dark = stylex.createTheme(t, { canvas: \"#111\" });\n",
    );
    let plain = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\n\
         import { tokens } from \"./tokens.stylex.js\";\n\
         export const dark = stylex.createTheme(tokens, { canvas: \"#111\" });\n",
    );
    assert_eq!(renamed.sheet.to_css(), plain.sheet.to_css());
}

#[test]
fn a_theme_written_with_a_named_import_compiles() {
    let compiled = compile(
        "// @flow\nimport { createTheme, defineVars } from \"@uniflowed/stylex\";\n\
         export const tokens = defineVars({ canvas: \"#fff\" });\n\
         export const dark = createTheme(tokens, { canvas: \"#111\" });\n",
    );
    assert_eq!(compiled.themes.len(), 1);
    assert_eq!(compiled.sheet.len(), 1);
}

#[test]
fn a_shadowed_override_keeps_only_the_last_value() {
    let compiled = compile(&themed(
        "export const dark = stylex.createTheme(tokens, { canvas: \"#111\", canvas: \"#222\" });\n",
    ));
    assert_eq!(compiled.sheet.len(), 1, "the dead rule is never emitted");
    assert_eq!(
        compiled.sheet.rules().next().expect("one rule").value,
        "#222"
    );
}

#[test]
fn a_theme_over_a_binding_that_is_not_a_token_set_is_refused() {
    let source = themed("export const dark = stylex.createTheme(palette, { canvas: \"#111\" });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnknownVariableBinding { .. })
    ));
}

#[test]
fn a_theme_without_an_overrides_object_is_refused() {
    let source = themed("export const dark = stylex.createTheme(tokens);\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ExpectedObjectLiteral { .. })
    ));
}

#[test]
fn a_theme_whose_first_argument_is_not_a_binding_is_refused_for_its_shape() {
    // Refused for the shape, not for the name. uf has to resolve the first
    // argument to a variables namespace before it can say anything about which
    // custom properties the overrides write, and a string literal is not a
    // binding it could resolve — so the message is about the argument that is
    // wrong rather than about a namespace nobody named.
    let source =
        themed("export const dark = stylex.createTheme(\"tokens\", { canvas: \"#111\" });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ExpectedObjectLiteral { .. })
    ));
}

#[test]
fn a_theme_whose_first_argument_is_a_call_is_refused() {
    let source = themed("export const dark = stylex.createTheme(load(), { canvas: \"#111\" });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ExpectedObjectLiteral { .. })
    ));
}

#[test]
fn an_override_nested_one_level_too_deep_is_refused() {
    let source = themed(
        "export const dark = stylex.createTheme(tokens, { canvas: { \":hover\": { \":focus\": \"#111\" } } });\n",
    );
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::NestingTooDeep { .. })
    ));
}

#[test]
fn a_forbidden_key_in_a_theme_is_refused() {
    let source =
        themed("export const dark = stylex.createTheme(tokens, { __proto__: \"#111\" });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ForbiddenKey { .. })
    ));
}

#[test]
fn a_value_that_could_escape_its_rule_is_refused_in_a_theme_too() {
    let source = themed(
        "export const dark = stylex.createTheme(tokens, { canvas: \"#111}.evil{color:red\" });\n",
    );
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnsafeValue { .. })
    ));
}

#[test]
fn a_theme_and_the_styles_beside_it_are_both_rewritten() {
    let compiled = compile(&themed(
        "export const dark = stylex.createTheme(tokens, { canvas: \"#111\" });\n\
         const s = stylex.create({ page: { backgroundColor: tokens.canvas } });\n",
    ));
    assert!(!compiled.code.contains("stylex."));
    assert_eq!(compiled.themes.len(), 1);
    assert_eq!(compiled.styles.len(), 1);
    assert_eq!(compiled.sheet.len(), 2);
}
