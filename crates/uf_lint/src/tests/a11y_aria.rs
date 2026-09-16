//! The `a11y/*` rules that read the ARIA table.
//!
//! Each rule is tested first with the examples eslint-plugin-jsx-a11y
//! documents for it, adapted to a Flow component, and then with the shapes uf
//! answers differently — which are marked, with the reason, where they appear.

use super::a11y_content::{accepts, reports};
use super::tree::component;
use super::*;

// --- a11y/aria-role ---------------------------------------------------------

#[test]
fn aria_role_reports_the_documented_failures() {
    reports(
        "a11y/aria-role",
        &[
            // Not a role at all.
            r#"<div role="datepicker" />"#,
            // An abstract role, which no element may carry.
            r#"<div role="range" />"#,
            // An empty one.
            r#"<div role="" />"#,
        ],
    );
}

#[test]
fn aria_role_accepts_the_documented_passes() {
    accepts(
        "a11y/aria-role",
        &[
            r#"<div role="button" />"#,
            // A value the module does not hold: it cannot be judged, so it is
            // not judged. The plugin's `ignoreNonDOM` option decides whether a
            // component is checked; uf checks the element it can read and says
            // nothing about the one it cannot, whatever kind of element it is.
            "<div role={role} />",
            "<div />",
            "<Foo role={role} />",
        ],
    );
}

#[test]
fn aria_role_reads_a_list_and_the_casing_a_browser_reads() {
    accepts(
        "a11y/aria-role",
        &[
            // ARIA lets an author write a fallback list and the browser takes
            // the first role it knows.
            r#"<div role="doc-subtitle heading" />"#,
            // Role tokens are matched case-insensitively, so this element is a
            // button and reporting it would be reporting working code.
            r#"<div role="BUTTON" />"#,
        ],
    );
}

#[test]
fn aria_role_suggests_the_role_that_was_meant() {
    let diagnostics = lint_js("a11y/aria-role", &component(r#"    <div role="buton" />"#));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("did you mean `button`?"),
        "{diagnostics:?}"
    );
}

#[test]
fn aria_role_says_an_abstract_role_is_abstract() {
    let diagnostics = lint_js("a11y/aria-role", &component(r#"    <div role="range" />"#));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("abstract role"),
        "{diagnostics:?}"
    );
}

// --- a11y/aria-proptypes ----------------------------------------------------

#[test]
fn aria_proptypes_reports_the_documented_failure() {
    reports(
        "a11y/aria-proptypes",
        &[r#"<span aria-hidden="yes">foo</span>"#],
    );
}

#[test]
fn aria_proptypes_accepts_the_documented_pass() {
    accepts(
        "a11y/aria-proptypes",
        &[r#"<span aria-hidden="true">foo</span>"#],
    );
}

#[test]
fn aria_proptypes_checks_each_kind_of_value() {
    reports(
        "a11y/aria-proptypes",
        &[
            // A tristate takes `mixed`, not any word.
            r#"<div role="checkbox" aria-checked="maybe" />"#,
            // An integer.
            r#"<div role="heading" aria-level="two" />"#,
            // A word from the attribute's own list.
            r#"<div aria-live="loud" />"#,
            // Every word in a list attribute.
            r#"<div aria-relevant="additions nonsense" />"#,
            // A name, written bare, reaches the DOM as the word "true".
            "<div aria-label />",
        ],
    );
    accepts(
        "a11y/aria-proptypes",
        &[
            r#"<div role="checkbox" aria-checked="mixed" />"#,
            "<div role=\"heading\" aria-level={2} />",
            r#"<div aria-live="polite" />"#,
            r#"<div aria-relevant="additions text" />"#,
            // `aria-current` and `aria-haspopup` take booleans as well as words.
            r#"<div aria-current="page" />"#,
            r#"<div aria-current="true" />"#,
            "<div aria-haspopup />",
            // Nothing the module holds, and nothing that renders.
            "<div aria-checked={checked} />",
            "<div aria-hidden={undefined} />",
        ],
    );
}

#[test]
fn aria_proptypes_reads_a_value_on_a_component() {
    // A component that takes an `aria-*` prop is taking it to put on a DOM
    // node, and a value of the wrong type is as inert one level up.
    reports(
        "a11y/aria-proptypes",
        &[r#"<Toggle aria-checked="maybe" />"#],
    );
}

// --- a11y/role-has-required-aria-props --------------------------------------

#[test]
fn role_has_required_aria_props_reports_the_documented_failure() {
    reports(
        "a11y/role-has-required-aria-props",
        &[r#"<span role="checkbox" aria-labelledby="foo" tabIndex="0" />"#],
    );
}

#[test]
fn role_has_required_aria_props_accepts_the_documented_pass() {
    accepts(
        "a11y/role-has-required-aria-props",
        &[r#"<span role="checkbox" aria-checked="false" aria-labelledby="foo" tabIndex="0" />"#],
    );
}

#[test]
fn role_has_required_aria_props_leaves_a_native_state_alone() {
    accepts(
        "a11y/role-has-required-aria-props",
        &[
            // The pattern for a switch: the role renames the control and the
            // native checkbox keeps answering for `aria-checked`.
            r#"<input type="checkbox" role="switch" />"#,
            // A heading element already has a level.
            r#"<h2 role="heading">Settings</h2>"#,
            // A `<select>` opens and closes itself.
            r#"<select role="combobox" />"#,
            // A type the module does not hold is not a claim that it is a
            // checkbox, and not a claim that it is not.
            "<input type={kind} role=\"switch\" />",
            // A spread may be carrying the state.
            "<span role=\"checkbox\" {...rest} />",
        ],
    );
}

#[test]
fn role_has_required_aria_props_names_the_state() {
    let diagnostics = lint_js(
        "a11y/role-has-required-aria-props",
        &component(r#"    <span role="checkbox" />"#),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("`aria-checked`"),
        "{diagnostics:?}"
    );
}

// --- a11y/role-supports-aria-props ------------------------------------------

#[test]
fn role_supports_aria_props_reports_the_documented_failure() {
    reports(
        "a11y/role-supports-aria-props",
        // The `radio` role does not take `aria-required`; the `radiogroup`
        // around it does.
        &[r#"<li aria-required tabIndex="-1" role="radio" aria-checked="false" />"#],
    );
}

#[test]
fn role_supports_aria_props_accepts_the_documented_pass() {
    accepts(
        "a11y/role-supports-aria-props",
        &[r#"<ul role="radiogroup" aria-required aria-labelledby="foo" />"#],
    );
}

#[test]
fn role_supports_aria_props_reads_the_role_html_already_gave_the_element() {
    reports(
        "a11y/role-supports-aria-props",
        &[
            // `<a href>` is a link, and a link is not checkable.
            r#"<a href="/" aria-checked="true">Home</a>"#,
            // `<input type="checkbox">` is a checkbox, which has no level.
            r#"<input type="checkbox" aria-level="2" />"#,
            // An `<a>` with no `href` is not a link but a `generic`, and a
            // `generic` is not checkable either.
            "<a aria-checked={checked} />",
        ],
    );
    accepts(
        "a11y/role-supports-aria-props",
        &[
            r#"<a href="/" aria-current="page">Home</a>"#,
            r#"<input type="checkbox" aria-checked="true" />"#,
            // A `<li>` is only a `listitem` inside a list, which is a question
            // about the document rather than the element.
            r#"<li aria-required="true" />"#,
            // A spread may be carrying the role this would be judged against.
            "<div {...rest} aria-checked=\"true\" />",
            // A role that is not a role is `a11y/aria-role`'s to report, and
            // until it is fixed there is no role to check against.
            r#"<div role="datepicker" aria-checked="true" />"#,
        ],
    );
}

/// WAI-ARIA 1.2 makes `generic` — the role of a plain `<div>` or `<span>` —
/// name-prohibited, so `aria-label` on one is discarded and the element ends
/// up with no name. The plugin does not report this, because its own table of
/// implicit roles has no entry for `div`; uf does, and the guide says so.
#[test]
fn role_supports_aria_props_reports_a_name_on_an_element_that_cannot_have_one() {
    let diagnostics = lint_js(
        "a11y/role-supports-aria-props",
        &component(r#"    <div aria-label="Close" />"#),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("no accessible name"),
        "{diagnostics:?}"
    );

    accepts(
        "a11y/role-supports-aria-props",
        &[
            // A `<section>` with a name is a region, which is named.
            r#"<section aria-label="Timeline posts" />"#,
            // So is any element given a role that takes a name.
            r#"<div role="group" aria-label="Filters" />"#,
            r#"<div role="status" aria-label="Loading" />"#,
            // A description is not a name, and `generic` takes one.
            r#"<div aria-describedby="hint" />"#,
        ],
    );
}

#[test]
fn role_supports_aria_props_stays_off_components() {
    // What a component does with an `aria-*` prop is the component's business:
    // it may be putting it on any element at all.
    accepts(
        "a11y/role-supports-aria-props",
        &[
            r#"<Toggle aria-checked="true" />"#,
            r#"<Card aria-label="Card" />"#,
        ],
    );
}
