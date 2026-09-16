//! The `a11y/*` rules that weigh a role against the element it was put on.
//!
//! Each rule is tested first with the examples eslint-plugin-jsx-a11y
//! documents for it, adapted to a Flow component, and then with the shapes uf
//! answers differently — which are marked, with the reason, where they appear.

use super::a11y_content::{accepts, reports};

// --- a11y/aria-unsupported-elements -----------------------------------------

#[test]
fn aria_unsupported_elements_reports_the_documented_failure() {
    reports(
        "a11y/aria-unsupported-elements",
        &[r#"<meta charSet="UTF-8" aria-hidden="false" />"#],
    );
}

#[test]
fn aria_unsupported_elements_accepts_the_documented_pass() {
    accepts(
        "a11y/aria-unsupported-elements",
        &[r#"<meta charSet="UTF-8" />"#],
    );
}

#[test]
fn aria_unsupported_elements_covers_a_role_and_the_other_reserved_elements() {
    reports(
        "a11y/aria-unsupported-elements",
        &[
            r#"<script role="presentation" />"#,
            r#"<style aria-label="theme" />"#,
            r#"<title aria-hidden="true">uf</title>"#,
        ],
    );
    accepts(
        "a11y/aria-unsupported-elements",
        &[
            // Not reserved: these are elements a reader reaches.
            r#"<div role="button" tabIndex={0} />"#,
            r#"<span aria-hidden="true">·</span>"#,
            // Nothing renders, so nothing is wrong.
            "<meta charSet=\"UTF-8\" aria-hidden={undefined} />",
        ],
    );
}

// --- a11y/no-redundant-roles ------------------------------------------------

#[test]
fn no_redundant_roles_reports_the_documented_failures() {
    reports(
        "a11y/no-redundant-roles",
        &[
            r#"<button role="button" />"#,
            r#"<img role="img" src="foo.jpg" alt="foo" />"#,
        ],
    );
}

#[test]
fn no_redundant_roles_accepts_the_documented_passes() {
    accepts(
        "a11y/no-redundant-roles",
        &[
            "<div />",
            r#"<button role="presentation" />"#,
            r#"<MyComponent role="main" />"#,
        ],
    );
}

#[test]
fn no_redundant_roles_keeps_the_navigation_exception() {
    // w3's own guidance recommends `<nav role="navigation">` for assistive
    // technology that predates the HTML5 elements, and the plugin ships the
    // same exception by default.
    accepts("a11y/no-redundant-roles", &[r#"<nav role="navigation" />"#]);
}

#[test]
fn no_redundant_roles_says_nothing_it_cannot_settle() {
    accepts(
        "a11y/no-redundant-roles",
        &[
            // An `<a>` is a link only with an `href`, and this one's is a value
            // the module does not hold.
            "<a href={to} role=\"link\" />",
            // A `<li>` is a `listitem` only inside a list, which is a question
            // about the document rather than the element.
            r#"<li role="listitem" />"#,
            // A role the module does not hold.
            "<button role={role} />",
        ],
    );
}

// --- a11y/prefer-tag-over-role ----------------------------------------------

#[test]
fn prefer_tag_over_role_reports_a_role_an_element_already_is() {
    let found = reports(
        "a11y/prefer-tag-over-role",
        &[
            // The plugin's own documented failure.
            r#"<div role="img" />"#,
            r#"<div role="navigation" />"#,
            r#"<div role="heading" aria-level={2} />"#,
            r#"<span role="list" />"#,
        ],
    );
    assert!(found[1].message.contains("`<nav>`"), "{found:?}");
    assert!(found[3].message.contains("or"), "{found:?}");
}

#[test]
fn prefer_tag_over_role_accepts_the_documented_passes() {
    accepts(
        "a11y/prefer-tag-over-role",
        &[
            "<div>x</div>",
            "<header>x</header>",
            r#"<img alt="" src="image.jpg" />"#,
            // The element already is the role: that is
            // `a11y/no-redundant-roles`' question, not this one's.
            r#"<nav role="navigation" />"#,
        ],
    );
}

/// The plugin reports every role an HTML element can carry, which includes the
/// roles of scripted widgets: it asks for `<select>` in place of
/// `role="combobox"` and `<input type="checkbox">` in place of
/// `role="checkbox"`. A tag is not what makes a widget work, and `@uniflowed/ui`
/// is built out of exactly those roles, so uf says nothing about them.
#[test]
fn prefer_tag_over_role_leaves_a_widget_role_alone() {
    accepts(
        "a11y/prefer-tag-over-role",
        &[
            r#"<div role="checkbox" aria-checked="false" />"#,
            r#"<div role="combobox" aria-controls="list" aria-expanded={false} />"#,
            r#"<div role="listbox" />"#,
            r#"<div role="option" aria-selected={false} />"#,
            r#"<span role="link" />"#,
            // Only meaningful inside a parent: a `<li>` outside a list is not a
            // `listitem`, and a `<td>` outside a table is not a cell.
            r#"<div role="listitem" />"#,
            r#"<div role="cell" />"#,
            // Form-associated elements bring behaviour a plain element does
            // not: `<output>` is not a drop-in for a live region.
            r#"<div role="status" aria-live="polite" />"#,
            r#"<div role="meter" aria-valuenow={4} />"#,
            // The mapping is a quirk of HTML-AAM rather than advice.
            r#"<div role="presentation" />"#,
            r#"<div role="generic" />"#,
        ],
    );
}

/// An element that takes focus or answers events is a widget somebody has
/// built, whatever its role is spelled as — and `@uniflowed/ui` is full of
/// them. `<hr>` is a void element nothing can focus, so telling a split pane's
/// draggable divider to become one is advice that cannot be taken.
#[test]
fn prefer_tag_over_role_leaves_an_element_that_is_wired_up_alone() {
    accepts(
        "a11y/prefer-tag-over-role",
        &[
            r#"<div role="separator" tabIndex={0} onKeyDown={resize} />"#,
            r#"<div role="region" aria-label="Clips" tabIndex={0} />"#,
            r#"<div role="navigation" onClick={track} />"#,
        ],
    );
    // Without the wiring, the same roles are a tag away.
    reports(
        "a11y/prefer-tag-over-role",
        &[
            r#"<div role="separator" />"#,
            r#"<div role="region" aria-label="Clips" />"#,
        ],
    );
}
