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

// --- a11y/no-interactive-element-to-noninteractive-role ---------------------

#[test]
fn interactive_to_noninteractive_reports_the_documented_failure() {
    reports(
        "a11y/no-interactive-element-to-noninteractive-role",
        &[
            r#"<button role="presentation">Save</button>"#,
            r#"<a href="/x" role="article">Home</a>"#,
        ],
    );
}

#[test]
fn interactive_to_noninteractive_accepts_the_documented_pass() {
    accepts(
        "a11y/no-interactive-element-to-noninteractive-role",
        &[
            // A widget role on a control is not this rule's question.
            r#"<button role="menuitem">Save</button>"#,
            // Not a control to begin with.
            r#"<div role="presentation" />"#,
            r#"<button>Save</button>"#,
        ],
    );
}

// --- a11y/no-noninteractive-element-to-interactive-role ---------------------

#[test]
fn noninteractive_to_interactive_reports_the_documented_failure() {
    reports(
        "a11y/no-noninteractive-element-to-interactive-role",
        &[
            r#"<ul role="button">Save</ul>"#,
            r#"<h1 role="textbox">Title</h1>"#,
        ],
    );
}

#[test]
fn noninteractive_to_interactive_accepts_the_documented_pass() {
    accepts(
        "a11y/no-noninteractive-element-to-interactive-role",
        &[
            // A `<div>` has no semantics of its own, and giving one a widget
            // role is how every custom control is built.
            r#"<div role="button" tabIndex={0} />"#,
            // The role is not a widget.
            r#"<ul role="list" />"#,
            // Already a control.
            r#"<button role="menuitem">Save</button>"#,
        ],
    );
}

/// A role the element is already a kind of says something sharper about it,
/// not something else.
///
/// `grid` is a kind of `table` in ARIA's taxonomy, so `<table role="grid">` is
/// how a keyboard-navigable grid is built — the pattern in uf's own date
/// picker — and reporting it would be reporting working markup. The widget
/// flag cannot draw this line: `button` and `grid` are both widgets, and only
/// one of them disagrees with the element under it.
#[test]
fn noninteractive_to_interactive_accepts_a_role_the_element_is_a_kind_of() {
    accepts(
        "a11y/no-noninteractive-element-to-interactive-role",
        &[r#"<table role="grid"><tbody><tr><td>1</td></tr></tbody></table>"#],
    );
    // The roles that are *not* a kind of the element they were put on are
    // still reported: `listbox`, `menu` and `tree` descend from `select` and
    // `group`, never from `list`.
    reports(
        "a11y/no-noninteractive-element-to-interactive-role",
        &[
            r#"<ul role="listbox">Save</ul>"#,
            r#"<ul role="menu">Save</ul>"#,
            r#"<ul role="button">Save</ul>"#,
        ],
    );
}

// --- a11y/no-noninteractive-element-interactions ----------------------------

#[test]
fn noninteractive_element_interactions_reports_the_documented_failure() {
    reports(
        "a11y/no-noninteractive-element-interactions",
        &[
            r#"<article onClick={open} onKeyDown={open}>Open</article>"#,
            r#"<div role="article" onClick={open} onKeyDown={open}>Open</div>"#,
        ],
    );
}

#[test]
fn noninteractive_element_interactions_accepts_the_documented_pass() {
    accepts(
        "a11y/no-noninteractive-element-interactions",
        &[
            // No key handler: that markup belongs to one of the other two
            // rules. See `the_interaction_rules_divide_the_markup_three_ways`.
            r#"<article onClick={open}>Open</article>"#,
            // A `<div>` has no role of its own to contradict.
            r#"<div onClick={open} onKeyDown={open}>Open</div>"#,
            // Already a control.
            r#"<button onClick={open} onKeyDown={open}>Open</button>"#,
            // A spread may carry a role in.
            r#"<article onClick={open} onKeyDown={open} {...rest}>Open</article>"#,
        ],
    );
}

/// An element a keyboard can reach is not what this rule is about.
///
/// A named, focusable container that answers the arrow keys is the documented
/// way to build a scrollable or navigable region — it is the same pattern that
/// makes `a11y/no-noninteractive-tabindex` a `warn` — and the complaint here,
/// that the handlers sit where nothing can get to them, is simply untrue of
/// it. Whether such an element is *announced* as a control belongs to
/// `a11y/no-noninteractive-element-to-interactive-role`.
#[test]
fn noninteractive_element_interactions_leaves_a_focusable_container_alone() {
    accepts(
        "a11y/no-noninteractive-element-interactions",
        &[
            r#"<div role="region" tabIndex={0} aria-label="Clips" onKeyDown={move}>x</div>"#,
            r#"<article tabIndex={0} onClick={open} onKeyDown={open}>Open</article>"#,
        ],
    );
    // Without the tab stop the same markup is unreachable, and reported.
    reports(
        "a11y/no-noninteractive-element-interactions",
        &[
            r#"<div role="region" aria-label="Clips" onKeyDown={move}>x</div>"#,
            r#"<article onClick={open} onKeyDown={open}>Open</article>"#,
        ],
    );
    // A negative `tabIndex` is script-only focus, never a tab stop, so it does
    // not make the handlers reachable either.
    reports(
        "a11y/no-noninteractive-element-interactions",
        &[r#"<article tabIndex={-1} onClick={open} onKeyDown={open}>Open</article>"#],
    );
}

// --- the three-way division of one defect -----------------------------------

/// Three rules split one question, and exactly one answers any given markup.
///
/// * no `role` at all — `a11y/no-static-element-interactions`;
/// * a `role` and no key handler — `a11y/click-events-have-key-events`;
/// * a non-interactive role with the keyboard already wired up — this batch's
///   `a11y/no-noninteractive-element-interactions`.
///
/// A change that makes two of them fire on one markup, or none of them fire on
/// any of these three, fails here rather than in somebody's editor.
#[test]
fn the_interaction_rules_divide_the_markup_three_ways() {
    // The case that actually exercises the boundary: an element whose own
    // role is settled and non-interactive, so all three rules are in scope
    // and only the shipped one may speak. A `<div>` tests far less here — it
    // is `generic`, which this batch's rule excludes for its own reasons, so
    // it would stay silent even if the division broke.
    let semantic_without_keys = r#"<article onClick={open}>Open</article>"#;
    reports(
        "a11y/no-static-element-interactions",
        &[semantic_without_keys],
    );
    accepts(
        "a11y/click-events-have-key-events",
        &[semantic_without_keys],
    );
    accepts(
        "a11y/no-noninteractive-element-interactions",
        &[semantic_without_keys],
    );

    let no_role = r#"<div onClick={open}>Open</div>"#;
    reports("a11y/no-static-element-interactions", &[no_role]);
    accepts("a11y/click-events-have-key-events", &[no_role]);
    accepts("a11y/no-noninteractive-element-interactions", &[no_role]);

    let role_without_keys = r#"<div role="button" tabIndex={0} onClick={open}>Open</div>"#;
    reports("a11y/click-events-have-key-events", &[role_without_keys]);
    accepts("a11y/no-static-element-interactions", &[role_without_keys]);
    accepts(
        "a11y/no-noninteractive-element-interactions",
        &[role_without_keys],
    );

    let semantic_with_keys = r#"<article onClick={open} onKeyDown={open}>Open</article>"#;
    reports(
        "a11y/no-noninteractive-element-interactions",
        &[semantic_with_keys],
    );
    accepts("a11y/no-static-element-interactions", &[semantic_with_keys]);
    accepts("a11y/click-events-have-key-events", &[semantic_with_keys]);
}

/// A role that depends on where the element sits is not guessed at.
///
/// `<li>` is a `listitem` only inside a list and `<td>` a `cell` only inside a
/// table, so `implicit_role` answers `Unsettled` and these rules say nothing.
/// That is a deliberate narrowing against the plugin, which reports on the tag
/// alone — and it is why `a11y/no-static-element-interactions` was left to go
/// on covering the element with no role, rather than being narrowed to the
/// ones whose role is settled.
#[test]
fn a_role_that_depends_on_placement_is_not_guessed() {
    accepts(
        "a11y/no-noninteractive-element-to-interactive-role",
        &[r#"<li role="button">Save</li>"#],
    );
    accepts(
        "a11y/no-noninteractive-element-interactions",
        &[r#"<li onClick={open} onKeyDown={open}>Open</li>"#],
    );
}
