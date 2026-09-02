//! `stylex.props(...)`: last argument wins, per property.

use super::{compile, module};
use crate::compile::CompiledStyle;
use crate::props::{props, props_of};

/// Compile a `create` call and hand back its namespaces.
fn styles(body: &str) -> Vec<CompiledStyle> {
    compile(&module(body)).styles
}

#[test]
fn merging_nothing_produces_nothing() {
    let merged = props_of(&[] as &[CompiledStyle]);
    assert!(merged.is_empty());
    assert_eq!(merged.class_name(), "");
}

#[test]
fn one_namespace_contributes_all_of_its_classes() {
    let styles = styles("const s = stylex.create({ a: { color: \"red\", opacity: 1 } });\n");
    let merged = props_of(&styles);
    assert_eq!(merged.len(), 2);
}

#[test]
fn a_later_namespace_wins_a_conflicting_property() {
    let styles =
        styles("const s = stylex.create({ a: { color: \"red\" }, b: { color: \"blue\" } });\n");
    let merged = props_of(&styles);
    assert_eq!(merged.len(), 1, "only one colour survives");

    let winner = &styles[1].property("color").expect("a colour").classes[0].class;
    assert_eq!(merged.classes(), std::slice::from_ref(winner));
}

#[test]
fn argument_order_decides_the_winner() {
    let styles =
        styles("const s = stylex.create({ a: { color: \"red\" }, b: { color: \"blue\" } });\n");
    let forwards = props_of(&styles);
    let backwards = props_of(styles.iter().rev());
    assert_ne!(forwards.classes(), backwards.classes());
}

#[test]
fn properties_that_do_not_conflict_all_survive() {
    let styles = styles("const s = stylex.create({ a: { color: \"red\" }, b: { opacity: 1 } });\n");
    assert_eq!(props_of(&styles).len(), 2);
}

#[test]
fn a_later_plain_value_replaces_an_earlier_conditional_one() {
    // The property is the unit of merging, so `b` does not leave `a`'s hover
    // state behind — the same thing a later `color:` in one CSS rule does.
    let styles = styles(
        "const s = stylex.create({ a: { color: { default: \"red\", \":hover\": \"green\" } }, b: { color: \"blue\" } });\n",
    );
    let merged = props_of(&styles);
    assert_eq!(merged.len(), 1);
    assert!(
        !merged.class_name().contains(
            styles[0].property("color").expect("a colour").classes[1]
                .class
                .as_str()
        )
    );
}

#[test]
fn an_earlier_plain_value_is_replaced_by_a_later_conditional_one() {
    let styles = styles(
        "const s = stylex.create({ a: { color: \"blue\" }, b: { color: { default: \"red\", \":hover\": \"green\" } } });\n",
    );
    assert_eq!(
        props_of(&styles).len(),
        2,
        "the default and the hover class"
    );
}

#[test]
fn a_falsy_argument_contributes_nothing_but_keeps_its_place() {
    let styles =
        styles("const s = stylex.create({ a: { color: \"red\" }, b: { color: \"blue\" } });\n");
    let with_gap = props([Some(&styles[0]), None, Some(&styles[1])]);
    assert_eq!(with_gap.classes(), props_of(&styles).classes());
}

#[test]
fn the_same_namespace_twice_contributes_one_set_of_classes() {
    let styles = styles("const s = stylex.create({ a: { color: \"red\" } });\n");
    assert_eq!(props([Some(&styles[0]), Some(&styles[0])]).len(), 1);
}

#[test]
fn the_class_attribute_is_in_sheet_order() {
    let styles = styles(
        "const s = stylex.create({ a: { marginTop: 8, margin: 0, color: { \":hover\": \"red\" } } });\n",
    );
    let merged = props_of(&styles);
    let classes: Vec<&str> = merged.classes().iter().map(|c| c.as_str()).collect();

    let compiled = compile(&module(
        "const s = stylex.create({ a: { marginTop: 8, margin: 0, color: { \":hover\": \"red\" } } });\n",
    ));
    let sheet_order: Vec<&str> = compiled
        .sheet
        .rules()
        .map(|rule| rule.class.as_str())
        .collect();
    assert_eq!(classes, sheet_order);
}

#[test]
fn conflicting_properties_across_three_namespaces_keep_only_the_last() {
    let styles = styles(
        "const s = stylex.create({ a: { color: \"red\" }, b: { color: \"blue\" }, c: { color: \"green\" } });\n",
    );
    let merged = props_of(&styles);
    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged.classes(),
        std::slice::from_ref(&styles[2].property("color").expect("a colour").classes[0].class)
    );
}

#[test]
fn a_shorthand_and_a_longhand_are_different_properties_and_both_survive() {
    // `props` merges on the authored key, so `margin` does not swallow
    // `marginTop`; keeping both is exactly why the sheet order has to be right.
    let styles = styles("const s = stylex.create({ a: { margin: 0 }, b: { marginTop: 8 } });\n");
    assert_eq!(props_of(&styles).len(), 2);
}

#[test]
fn the_class_attribute_is_space_separated() {
    let styles = styles("const s = stylex.create({ a: { color: \"red\", opacity: 1 } });\n");
    let merged = props_of(&styles);
    assert_eq!(merged.class_name().split(' ').count(), 2);
}
