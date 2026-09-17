//! The rules about a control that is missing the prop which makes it behave.
//!
//! [`super::shape`] asks whether a name or a value is a thing. These two ask
//! something narrower: the attribute is spelled correctly and the value is
//! fine, and the control still does the wrong thing because a *companion* prop
//! was never written. A `<button>` with no `type` submits the form around it; an
//! `<input checked>` with nothing to change it is a control the reader cannot
//! move. Neither looks wrong in the markup, which is why a linter has to say it.
//!
//! # A spread is gated per prop, not per element
//!
//! Both rules look for an attribute's **absence**, and a `{...spread}` may be
//! supplying it — but only where the spread could still win. JSX applies props
//! in written order, so `<button {...props} type="huge">` settles `type` right
//! there and no spread can reach it, while `<button type="button" {...props}>`
//! leaves it open. Each rule therefore asks [`spread_may_set`] about its own
//! decisive props rather than skipping every element that carries a spread:
//! "what cannot be seen is not reported" is the principle, and a blanket gate
//! over-applies it, staying silent where the answer is written down.
//!
//! For `react/checked-requires-onchange-or-readonly` the two gates come to the
//! same thing, which is worth writing down rather than leaving to be
//! rediscovered: that rule fires precisely when `onChange` and `readOnly` are
//! *absent*, and an absent prop plus any spread means the spread may be
//! supplying it. The per-prop gate is still what is written, because it says
//! which props the silence is about.
//!
//! # The companion written with its HTML spelling
//!
//! `<input checked readonly />` has the companion written; what it does not
//! have is the *prop*, because React spells it `readOnly`. That is
//! `react/no-unknown-property`'s finding, and
//! `react/checked-requires-onchange-or-readonly` stays silent on such an
//! element so that one line of markup draws one diagnostic rather than two.
//! Pinned by a test.

use uf_config::UniflowedConfig;
use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::value::{Value, spread_may_set};
use super::{Tree, attribute};
use crate::{Severity, severity};

/// `react/button-has-type`.
const BUTTON_HAS_TYPE: &str = "react/button-has-type";

/// `react/checked-requires-onchange-or-readonly`.
const CHECKED_REQUIRES: &str = "react/checked-requires-onchange-or-readonly";

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    button_has_type: Option<Severity>,
    checked_requires: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            button_has_type: severity(config, BUTTON_HAS_TYPE),
            checked_requires: severity(config, CHECKED_REQUIRES),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        self.button_has_type.is_some() || self.checked_requires.is_some()
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            BUTTON_HAS_TYPE => self.button_has_type,
            CHECKED_REQUIRES => self.checked_requires,
            _ => None,
        }
    }
}

/// Run every rule in this module that is on against one host element.
pub(super) fn check(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    // Each rule gates on its own decisive props; see the module documentation
    // for why this is not one gate over the whole element.
    let levels = tree.controls;
    if levels.button_has_type.is_some() {
        button_has_type(tree, name, opening);
    }
    if levels.checked_requires.is_some() {
        checked_requires(tree, name, opening);
    }
}

// --- react/button-has-type --------------------------------------------------

/// The three values HTML defines for a `<button>`.
const BUTTON_TYPES: [&str; 3] = ["submit", "reset", "button"];

/// A `<button>` with no `type`, or one HTML does not define.
///
/// HTML defaults a button to `type="submit"`, so a button written for an
/// `onClick` inside a form submits that form and navigates away — the handler
/// runs and its effect is thrown away by the navigation, which reads as "the
/// button does nothing sometimes". The default is the surprise, so the fix is
/// to say which of the three it is.
///
/// `warn` rather than `error`: a button outside a form has nothing to submit,
/// and this module cannot see the form a component is rendered into, so this is
/// guidance rather than a defect wherever it appears.
fn button_has_type(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if name != "button" {
        return;
    }
    // Only a spread that could still win hides the answer: a `type` written
    // after one is settled where it stands.
    if spread_may_set(opening, "type") {
        return;
    }
    let Some(written) = attribute(opening, "type") else {
        tree.report(
            &opening.loc,
            BUTTON_HAS_TYPE,
            String::from(
                "this `<button>` has no `type`, and HTML defaults one to `type=\"submit\"`: inside \
                 a form it submits and navigates away, so whatever its `onClick` did is thrown \
                 away by the navigation. Write `type=\"button\"` for a button that runs a handler, \
                 or `type=\"submit\"` to say the submission is meant",
            ),
        );
        return;
    };
    // A value the module does not hold is not a wrong one.
    let Value::Text(text) = tree.scope.value(written) else {
        return;
    };
    if BUTTON_TYPES.contains(&text) {
        return;
    }
    tree.report(
        &written.loc,
        BUTTON_HAS_TYPE,
        format!(
            "`type=\"{text}\"` is not a button type — HTML defines `submit`, `reset` and `button`, \
             and treats anything else as `submit`, which is the one behaviour nobody writes a \
             custom value to get"
        ),
    );
}

// --- react/checked-requires-onchange-or-readonly ----------------------------

/// An `<input checked>` with nothing that can change it.
///
/// `checked` makes the input **controlled**: React renders whatever the prop
/// says and puts it back after every click, so without an `onChange` to move
/// the state the reader clicks and nothing happens. React says so in
/// development, and the fix is either the handler or `readOnly` to declare that
/// not moving is the point.
///
/// `defaultChecked` is the uncontrolled form and is not this rule's subject:
/// the DOM owns that value and a click moves it.
fn checked_requires(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if name != "input" {
        return;
    }
    // The three props this rule decides on. A spread that could still set any
    // of them takes the answer away: one could turn the input uncontrolled, and
    // either of the others could be the companion the rule is looking for.
    if spread_may_set(opening, "checked")
        || spread_may_set(opening, "onChange")
        || spread_may_set(opening, "readOnly")
    {
        return;
    }
    let scope = tree.scope;
    let Some(written) = attribute(opening, "checked") else {
        return;
    };
    // `checked={null}` and `checked={undefined}` render no attribute at all, so
    // the input is uncontrolled and there is nothing to report.
    if scope.value(written) == Value::Nullish {
        return;
    }
    // `onChange={null}` installs no listener, and `readOnly={false}` is the
    // prop written and turned off, so neither answers the click.
    let handled = attribute(opening, "onChange")
        .is_some_and(|written| scope.value(written) != Value::Nullish)
        || attribute(opening, "readOnly").is_some_and(|written| {
            !matches!(scope.value(written), Value::Nullish | Value::Bool(false))
        });
    if handled {
        return;
    }
    // The companion written with its HTML spelling is
    // `react/no-unknown-property`'s to report. See the module documentation.
    if attribute(opening, "onchange").is_some() || attribute(opening, "readonly").is_some() {
        return;
    }
    tree.report(
        &written.loc,
        CHECKED_REQUIRES,
        String::from(
            "`checked` makes this input controlled, so React puts the prop's value back after \
             every click and the reader cannot move it; add an `onChange` that stores the new \
             value, or `readOnly` to say that not moving is what was meant. `defaultChecked` is \
             the uncontrolled form, which a click does move",
        ),
    );
}
