//! The order rules are emitted in.
//!
//! Every rule uf emits is one class selector, so all of them have the same
//! specificity and the last one in the sheet wins. Each test here is written so
//! that emitting in source order — the obvious implementation — makes it fail.

use super::{compile, module};
use crate::sheet::StyleSheet;

/// The `(property, selector suffix)` of every rule, in sheet order.
fn emitted(sheet: &StyleSheet) -> Vec<String> {
    sheet
        .rules()
        .map(|rule| format!("{}{}", rule.property, rule.condition.selector_suffix()))
        .collect()
}

#[test]
fn a_shorthand_is_emitted_before_the_longhand_that_narrows_it() {
    // Written longhand-first on purpose: in source order the shorthand would
    // land second and silently overwrite `margin-top` on every element that
    // uses both.
    let compiled = compile(&module(
        "const s = stylex.create({ a: { marginTop: 8, margin: 0 } });\n",
    ));
    assert_eq!(emitted(&compiled.sheet), ["margin", "margin-top"]);
}

#[test]
fn a_wide_shorthand_is_emitted_before_a_narrow_one() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { marginInline: 4, margin: 0 } });\n",
    ));
    assert_eq!(emitted(&compiled.sheet), ["margin", "margin-inline"]);
}

#[test]
fn all_is_emitted_before_every_other_property() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\", border: \"none\", all: \"unset\" } });\n",
    ));
    assert_eq!(emitted(&compiled.sheet), ["all", "border", "color"]);
}

#[test]
fn a_hover_rule_is_emitted_after_the_base_rule_it_overrides() {
    // The conditional value is written before the default one, so source order
    // would put `:hover` first and hovering would do nothing.
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: { \":hover\": \"red\", default: \"black\" } } });\n",
    ));
    assert_eq!(emitted(&compiled.sheet), ["color", "color:hover"]);
}

#[test]
fn link_states_are_emitted_in_cascade_order() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: {\n  \":active\": \"a\",\n  \":hover\": \"h\",\n  \":visited\": \"v\",\n  \":focus\": \"f\",\n  \":link\": \"l\",\n  default: \"d\",\n} } });\n",
    ));
    assert_eq!(
        emitted(&compiled.sheet),
        [
            "color",
            "color:link",
            "color:visited",
            "color:hover",
            "color:focus",
            "color:active",
        ]
    );
}

#[test]
fn a_media_query_is_emitted_after_the_base_rule() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: { \"@media (min-width: 600px)\": \"blue\", default: \"red\" } } });\n",
    ));
    let css = compiled.sheet.to_css();
    let base = css.find("color:red").expect("the base rule");
    let wide = css.find("@media").expect("the media rule");
    assert!(base < wide, "the base rule must come first in {css}");
}

#[test]
fn a_pseudo_element_is_emitted_after_every_pseudo_class() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { \"::before\": { color: \"a\" }, \":hover\": { color: \"b\" } } });\n",
    ));
    assert_eq!(emitted(&compiled.sheet), ["color:hover", "color::before"]);
}

#[test]
fn a_longhand_outranks_a_shorthand_even_in_a_hover_state() {
    // `margin` on hover must not beat a base `margin-top`: narrower always
    // wins, whatever state the broader rule applies in.
    let compiled = compile(&module(
        "const s = stylex.create({ a: { marginTop: 8, margin: { \":hover\": 0 } } });\n",
    ));
    assert_eq!(emitted(&compiled.sheet), ["margin:hover", "margin-top"]);
}

#[test]
fn two_modules_fold_into_the_same_sheet_whichever_order_they_arrive_in() {
    let first = compile(&module(
        "const s = stylex.create({ a: { color: \"red\", marginTop: 8 } });\n",
    ));
    let second = compile(&module(
        "const s = stylex.create({ b: { margin: 0, color: { \":hover\": \"blue\" } } });\n",
    ));

    let mut forwards = StyleSheet::new();
    forwards.extend(&first.sheet);
    forwards.extend(&second.sheet);

    let mut backwards = StyleSheet::new();
    backwards.extend(&second.sheet);
    backwards.extend(&first.sheet);

    similar_asserts::assert_eq!(forwards.to_css(), backwards.to_css());

    let order = emitted(&forwards);
    assert_eq!(order.first().map(String::as_str), Some("margin"));
    assert_eq!(order.last().map(String::as_str), Some("color:hover"));
}

#[test]
fn folding_the_same_module_twice_changes_nothing() {
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    let mut sheet = StyleSheet::new();
    sheet.extend(&compiled.sheet);
    let once = sheet.to_css();
    sheet.extend(&compiled.sheet);
    assert_eq!(sheet.to_css(), once);
}

#[test]
fn two_namespaces_setting_the_same_value_each_keep_their_own_rule() {
    // The namespace takes part in the class name, so the two rules are distinct
    // and both have to be in the sheet for `props` to resolve either of them.
    let compiled = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" }, b: { color: \"red\" } });\n",
    ));
    assert_eq!(compiled.sheet.len(), 2);
    assert_eq!(emitted(&compiled.sheet), ["color", "color"]);
}

#[test]
fn variables_are_emitted_before_the_rules_that_use_them() {
    let compiled = compile(
        "// @flow\nimport { stylex } from \"@uniflowed/stylex\";\nexport const tokens = stylex.defineVars({ ink: \"#000\" });\nconst s = stylex.create({ a: { color: tokens.ink } });\n",
    );
    let css = compiled.sheet.to_css();
    let root = css.find(":root{").expect("the variable block");
    let rule = css.find(".x").expect("a rule");
    assert!(root < rule, "variables must come first in {css}");
}
