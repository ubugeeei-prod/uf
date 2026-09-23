//! The rules that ask whether a name or a value is a thing at all.
//!
//! Each rule is tested first with the examples eslint-plugin-react documents
//! for its own version, adapted to a Flow component, and then with the shapes
//! uf answers differently — which are marked, with the reason, where they
//! appear. The boundary with every shipped rule that reads the same markup is
//! pinned at the bottom.

use super::a11y_content::{accepts, reports};

// --- react/no-unknown-property ----------------------------------------------

#[test]
fn no_unknown_property_reports_the_documented_failures() {
    reports(
        "react/no-unknown-property",
        &[
            r#"<div class="x">y</div>"#,
            r#"<label for="email">Email</label>"#,
            r#"<td colspan="2">x</td>"#,
            "<input readonly />",
            r#"<img src="/a.png" alt="" srcset="/a2.png 2x" />"#,
            r#"<meta http-equiv="refresh" content="0" />"#,
        ],
    );
}

#[test]
fn no_unknown_property_reports_a_lowercase_event_handler() {
    reports(
        "react/no-unknown-property",
        &["<div onclick={go}>x</div>", "<form onsubmit={save} />"],
    );
}

#[test]
fn no_unknown_property_names_the_react_spelling() {
    let found = reports("react/no-unknown-property", &[r#"<div class="x">y</div>"#]);

    assert!(found[0].message.contains("`className`"), "{found:?}");
}

#[test]
fn no_unknown_property_accepts_the_documented_passes() {
    accepts(
        "react/no-unknown-property",
        &[
            r#"<div className="x">y</div>"#,
            r#"<label htmlFor="email">Email</label>"#,
            "<td colSpan={2}>x</td>",
            "<input readOnly />",
            "<div onClick={go}>x</div>",
            // Not a rename of anything: a `data-*` is passed through as
            // written, which is what it is for.
            r#"<div data-class="x">y</div>"#,
            // An attribute uf has never heard of is left alone, because the
            // platform keeps growing and a stale table would report markup that
            // had become correct. This is where uf answers less than the plugin.
            r#"<div nonsenseattribute="x">y</div>"#,
            // A component decides its own props; `class` may well be one.
            r#"<Card class="x" />"#,
        ],
    );
}

// --- react/style-prop-object ------------------------------------------------

#[test]
fn style_prop_object_reports_the_documented_failures() {
    reports(
        "react/style-prop-object",
        &[
            r#"<div style="color: red">x</div>"#,
            r#"<div style={"color: red"}>x</div>"#,
            "<div style={1}>x</div>",
        ],
    );
}

#[test]
fn style_prop_object_accepts_the_documented_passes() {
    accepts(
        "react/style-prop-object",
        &[
            r#"<div style={{ color: "red" }}>x</div>"#,
            // A value the module does not hold is never wrong.
            "<div style={styles}>x</div>",
            // React takes `null` as no style at all.
            "<div style={null}>x</div>",
            "<div>x</div>",
            // A component decides what its `style` prop means.
            r#"<Card style="color: red" />"#,
        ],
    );
}

// --- react/no-namespace -----------------------------------------------------

#[test]
fn no_namespace_reports_the_documented_failures() {
    reports(
        "react/no-namespace",
        &["<svg:rect />", "<xhtml:div>x</xhtml:div>"],
    );
}

#[test]
fn no_namespace_accepts_the_documented_passes() {
    accepts(
        "react/no-namespace",
        &[
            "<rect />",
            "<div>x</div>",
            // A member expression is a value in scope, not a namespace.
            "<Icons.Chevron />",
            "<Svg />",
        ],
    );
}

// --- markup/no-invalid-rel --------------------------------------------------

#[test]
fn no_invalid_rel_reports_a_keyword_that_belongs_to_another_element() {
    reports(
        "markup/no-invalid-rel",
        &[
            // `stylesheet` and `icon` are `<link>`'s alone.
            r#"<a href="/x" rel="stylesheet">y</a>"#,
            r#"<a href="/x" rel="icon">y</a>"#,
            // `<form>` takes a smaller set than `<a>` does: no `bookmark`.
            r#"<form action="/x" rel="bookmark" />"#,
        ],
    );
}

#[test]
fn no_invalid_rel_reports_the_non_conforming_spellings() {
    reports(
        "markup/no-invalid-rel",
        &[
            r#"<a href="/x" rel="previous">y</a>"#,
            r#"<a href="/x" rel="copyright">y</a>"#,
            // The one people still write in front of `icon`.
            r#"<link rel="shortcut icon" href="/f.ico" />"#,
        ],
    );
}

#[test]
fn no_invalid_rel_accepts_the_documented_passes() {
    accepts(
        "markup/no-invalid-rel",
        &[
            r#"<a href="/x" rel="noreferrer">y</a>"#,
            r#"<a href="/x" rel="nofollow noreferrer">y</a>"#,
            // Keywords are matched the way HTML matches them.
            r#"<a href="/x" rel="NoFollow">y</a>"#,
            r#"<link rel="stylesheet" href="/a.css" />"#,
            r#"<link rel="preconnect" href="https://example.com" />"#,
            r#"<form action="/x" rel="noreferrer" />"#,
            // Real, widely used, and in none of the closed lists: the
            // registries `rel` draws on are open, so an unknown keyword is not
            // a wrong one. This is where uf deliberately says nothing.
            r#"<link rel="apple-touch-icon" href="/a.png" />"#,
            // A value the module does not hold is never wrong.
            "<a href=\"/x\" rel={rel}>y</a>",
            // Not an element that takes `rel` at all.
            r#"<div rel="stylesheet">x</div>"#,
        ],
    );
}

// --- the boundaries with the shipped rules ----------------------------------

/// `react/no-unknown-property` and the shipped `a11y/aria-props` ask the same
/// kind of question — is this attribute name real — of different sets, and
/// exactly one answers any given attribute.
///
/// `a11y/aria-props` owns every `aria-*`, asked of the generated ARIA table. No
/// `aria-` name is in this rule's rename table, so the two cannot both speak.
#[test]
fn no_unknown_property_leaves_the_aria_attributes_to_aria_props() {
    let misspelled_aria = r#"<div aria-lable="Close">y</div>"#;
    reports("a11y/aria-props", &[misspelled_aria]);
    accepts("react/no-unknown-property", &[misspelled_aria]);

    let html_spelling = r#"<div class="x">y</div>"#;
    reports("react/no-unknown-property", &[html_spelling]);
    accepts("a11y/aria-props", &[html_spelling]);
}

/// `autocomplete` is left out of the rename table on purpose.
///
/// The shipped `a11y/autocomplete-valid` reads that attribute under this
/// lowercase spelling as well as React's `autoComplete`, so adding it here
/// would put two rules on one
/// attribute for one line of markup. A change that starts reporting it twice
/// fails here rather than in somebody's editor.
#[test]
fn no_unknown_property_leaves_autocomplete_to_the_a11y_rule() {
    let lowercase = r#"<input autocomplete="birthday" />"#;
    reports("a11y/autocomplete-valid", &[lowercase]);
    accepts("react/no-unknown-property", &[lowercase]);
}

/// A namespaced element is `react/no-namespace`'s and nobody else's.
///
/// `super::super::runner::tree::host_name` returns `None` for a namespaced
/// name, so the host-element rules — this module's rename check included —
/// never run on one. The `class` here is reported by neither rule, and that is
/// by construction rather than by agreement.
#[test]
fn a_namespaced_element_is_answered_only_by_no_namespace() {
    let namespaced = r#"<svg:rect class="x" />"#;
    reports("react/no-namespace", &[namespaced]);
    accepts("react/no-unknown-property", &[namespaced]);
}

/// `markup/no-invalid-rel` and the shipped `security/no-target-blank` read one
/// attribute and ask different questions of it: whether the keyword belongs on
/// this element, and whether the referrer is stopped.
///
/// Neither is a second opinion about the other, and the markup each reports is
/// silent from the other.
#[test]
fn the_two_rel_rules_ask_different_questions_of_one_attribute() {
    let wrong_keyword = r#"<a href="/x" rel="stylesheet">y</a>"#;
    reports("markup/no-invalid-rel", &[wrong_keyword]);
    accepts("security/no-target-blank", &[wrong_keyword]);

    let unstopped = r#"<a href="https://example.com" target="_blank">y</a>"#;
    reports("security/no-target-blank", &[unstopped]);
    accepts("markup/no-invalid-rel", &[unstopped]);

    let correct = r#"<a href="https://example.com" target="_blank" rel="noreferrer">y</a>"#;
    accepts("markup/no-invalid-rel", &[correct]);
    accepts("security/no-target-blank", &[correct]);
}
