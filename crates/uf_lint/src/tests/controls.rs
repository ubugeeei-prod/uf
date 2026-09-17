//! The rules about a control missing the prop that makes it behave.
//!
//! Each rule is tested first with the examples eslint-plugin-react documents
//! for it, adapted to a Flow component, and then with the shapes uf answers
//! differently — marked, with the reason, where they appear.

use super::a11y_content::{accepts, reports};

// --- react/button-has-type --------------------------------------------------

#[test]
fn button_has_type_reports_the_documented_failures() {
    reports(
        "react/button-has-type",
        &[
            "<button>Save</button>",
            "<button onClick={save}>Save</button>",
            // Not a button type, and HTML treats anything else as `submit`.
            r#"<button type="huge">Save</button>"#,
        ],
    );
}

#[test]
fn button_has_type_accepts_the_documented_passes() {
    accepts(
        "react/button-has-type",
        &[
            r#"<button type="button">Save</button>"#,
            r#"<button type="submit">Save</button>"#,
            r#"<button type="reset">Reset</button>"#,
            // A value the module does not hold is never wrong.
            "<button type={kind}>Save</button>",
            // A spread may be carrying the `type` in.
            "<button {...rest}>Save</button>",
            // Not a button, and a component decides its own markup.
            "<div>Save</div>",
            "<Button>Save</Button>",
        ],
    );
}

// --- react/checked-requires-onchange-or-readonly ----------------------------

#[test]
fn checked_requires_reports_the_documented_failures() {
    reports(
        "react/checked-requires-onchange-or-readonly",
        &[
            r#"<input type="checkbox" checked />"#,
            "<input type=\"checkbox\" checked={done} />",
            // The prop written and turned off answers no click.
            "<input type=\"checkbox\" checked readOnly={false} />",
            "<input type=\"checkbox\" checked onChange={null} />",
        ],
    );
}

#[test]
fn checked_requires_accepts_the_documented_passes() {
    accepts(
        "react/checked-requires-onchange-or-readonly",
        &[
            "<input type=\"checkbox\" checked onChange={toggle} />",
            "<input type=\"checkbox\" checked readOnly />",
            // The uncontrolled form: the DOM owns the value and a click moves it.
            "<input type=\"checkbox\" defaultChecked />",
            // Renders no attribute at all, so the input is uncontrolled.
            "<input type=\"checkbox\" checked={null} />",
            "<input type=\"checkbox\" />",
            // A spread may be carrying the handler in.
            "<input type=\"checkbox\" checked {...rest} />",
            // A component decides its own markup.
            "<Checkbox checked />",
        ],
    );
}

// --- the boundaries with the shipped rules ----------------------------------

/// The companion written with its HTML spelling belongs to
/// `react/no-unknown-property`, and this rule stays silent on it.
///
/// `<input checked readonly />` has the companion written; what it lacks is the
/// prop, because React spells it `readOnly`. Reporting both would put two
/// diagnostics on one line for one mistake, so exactly one rule answers — and a
/// change that lets both speak fails here.
#[test]
fn checked_requires_leaves_the_html_spelling_to_no_unknown_property() {
    let misspelled = "<input type=\"checkbox\" checked readonly />";
    reports("react/no-unknown-property", &[misspelled]);
    accepts("react/checked-requires-onchange-or-readonly", &[misspelled]);

    let missing = r#"<input type="checkbox" checked />"#;
    reports("react/checked-requires-onchange-or-readonly", &[missing]);
    accepts("react/no-unknown-property", &[missing]);
}

/// `react/button-has-type` and the shipped `a11y/control-has-associated-label`
/// read the same element and ask different questions of it, so neither is a
/// second opinion about the other.
#[test]
fn button_has_type_and_the_label_rule_ask_different_questions() {
    let unlabelled = r#"<button type="button" />"#;
    reports("a11y/control-has-associated-label", &[unlabelled]);
    accepts("react/button-has-type", &[unlabelled]);

    let untyped = "<button>Save</button>";
    reports("react/button-has-type", &[untyped]);
    accepts("a11y/control-has-associated-label", &[untyped]);
}
