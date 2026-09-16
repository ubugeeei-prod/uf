//! The `a11y/*` rules that ask whether a pointer is the only way in.
//!
//! A mouse can reach anything on the page. A keyboard reaches what takes
//! focus, in the order the document gives it, and answers what listens for a
//! key. These rules ask the two halves of that: can this element be reached,
//! and once reached, does anything happen.
//!
//! # Focus is read from the element, not guessed
//!
//! [`focusable`] is the one answer every rule here shares, so that "can a
//! keyboard reach this" means the same thing seven times. It is `true` for an
//! element HTML already focuses and for a `tabIndex` that is zero or more, and
//! that is all: an element made focusable by something the file does not show
//! is not something to guess about.
//!
//! # Where this stops, and `a11y/no-static-element-interactions` starts
//!
//! The plugin splits one defect across two rules, and so does uf — but along a
//! different line, chosen so exactly one of them answers any given markup:
//!
//! * `a11y/no-static-element-interactions` takes the element with **no `role`
//!   at all**. Nobody has said what it is, so the advice is to use a `<button>`
//!   or to give it a role and a handler.
//! * `a11y/click-events-have-key-events` takes the element that **has a
//!   `role`**. Somebody has said what it is and a keyboard still cannot work
//!   it, so the advice is the missing handler alone.
//!
//! The shipped rule stays silent on an element with a `role`, and this one
//! stays silent on an element without one. `a11y_interaction.rs` pins that:
//! the markup either rule reports draws exactly one finding, never two.
//!
//! # What cannot be seen is not reported
//!
//! A `{...spread}` may carry in the very attribute each of these rules looks
//! for — a `tabIndex`, an `onKeyDown`, an `onFocus` — so an element that has
//! one is a question this module does not answer. Components are left alone
//! for the same reason: `<Card onClick={open} />` renders markup this file
//! does not show.

use uf_config::UniflowedConfig;
use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::value::Value;
use super::{INTERACTIVE_ELEMENTS, KEY_HANDLERS, Tree, aria, attribute, has_spread};
use crate::{Severity, severity};

/// `a11y/aria-activedescendant-has-tabindex`.
const ACTIVEDESCENDANT_TABINDEX: &str = "a11y/aria-activedescendant-has-tabindex";

/// `a11y/click-events-have-key-events`.
const CLICK_EVENTS_HAVE_KEY_EVENTS: &str = "a11y/click-events-have-key-events";

/// `a11y/interactive-supports-focus`.
const INTERACTIVE_SUPPORTS_FOCUS: &str = "a11y/interactive-supports-focus";

/// `a11y/mouse-events-have-key-events`.
const MOUSE_EVENTS_HAVE_KEY_EVENTS: &str = "a11y/mouse-events-have-key-events";

/// `a11y/no-aria-hidden-on-focusable`.
const NO_ARIA_HIDDEN_ON_FOCUSABLE: &str = "a11y/no-aria-hidden-on-focusable";

/// `a11y/no-noninteractive-tabindex`.
const NO_NONINTERACTIVE_TABINDEX: &str = "a11y/no-noninteractive-tabindex";

/// `a11y/tabindex-no-positive`.
const TABINDEX_NO_POSITIVE: &str = "a11y/tabindex-no-positive";

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    activedescendant_tabindex: Option<Severity>,
    click_events_have_key_events: Option<Severity>,
    interactive_supports_focus: Option<Severity>,
    mouse_events_have_key_events: Option<Severity>,
    no_aria_hidden_on_focusable: Option<Severity>,
    no_noninteractive_tabindex: Option<Severity>,
    tabindex_no_positive: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            activedescendant_tabindex: severity(config, ACTIVEDESCENDANT_TABINDEX),
            click_events_have_key_events: severity(config, CLICK_EVENTS_HAVE_KEY_EVENTS),
            interactive_supports_focus: severity(config, INTERACTIVE_SUPPORTS_FOCUS),
            mouse_events_have_key_events: severity(config, MOUSE_EVENTS_HAVE_KEY_EVENTS),
            no_aria_hidden_on_focusable: severity(config, NO_ARIA_HIDDEN_ON_FOCUSABLE),
            no_noninteractive_tabindex: severity(config, NO_NONINTERACTIVE_TABINDEX),
            tabindex_no_positive: severity(config, TABINDEX_NO_POSITIVE),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        [
            self.activedescendant_tabindex,
            self.click_events_have_key_events,
            self.interactive_supports_focus,
            self.mouse_events_have_key_events,
            self.no_aria_hidden_on_focusable,
            self.no_noninteractive_tabindex,
            self.tabindex_no_positive,
        ]
        .iter()
        .any(Option::is_some)
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            ACTIVEDESCENDANT_TABINDEX => self.activedescendant_tabindex,
            CLICK_EVENTS_HAVE_KEY_EVENTS => self.click_events_have_key_events,
            INTERACTIVE_SUPPORTS_FOCUS => self.interactive_supports_focus,
            MOUSE_EVENTS_HAVE_KEY_EVENTS => self.mouse_events_have_key_events,
            NO_ARIA_HIDDEN_ON_FOCUSABLE => self.no_aria_hidden_on_focusable,
            NO_NONINTERACTIVE_TABINDEX => self.no_noninteractive_tabindex,
            TABINDEX_NO_POSITIVE => self.tabindex_no_positive,
            _ => None,
        }
    }
}

/// Run every rule in this module that is on against one host element.
pub(super) fn check(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    // See the module documentation: a spread may be carrying in the attribute
    // every rule here is looking for, so none of them answers such an element.
    if has_spread(opening) {
        return;
    }
    let levels = tree.interaction;
    if levels.tabindex_no_positive.is_some() {
        tabindex_no_positive(tree, opening);
    }
    if levels.no_aria_hidden_on_focusable.is_some() {
        no_aria_hidden_on_focusable(tree, name, opening);
    }
    if levels.activedescendant_tabindex.is_some() {
        activedescendant_has_tabindex(tree, name, opening);
    }
    if levels.mouse_events_have_key_events.is_some() {
        mouse_events_have_key_events(tree, opening);
    }
    if levels.click_events_have_key_events.is_some() {
        click_events_have_key_events(tree, name, opening);
    }
    if levels.interactive_supports_focus.is_some() {
        interactive_supports_focus(tree, name, opening);
    }
    if levels.no_noninteractive_tabindex.is_some() {
        no_noninteractive_tabindex(tree, name, opening);
    }
}

/// The `tabIndex` written on this element, when the source settles it.
pub(super) fn tab_index(tree: &Tree<'_>, opening: &jsx::Opening<Loc, Loc>) -> Option<f64> {
    let written = attribute(opening, "tabIndex")?;
    match tree.scope.value(written) {
        Value::Number(index) => Some(index),
        _ => None,
    }
}

/// Whether a keyboard can reach this element.
///
/// The elements HTML focuses on its own, and anything given a `tabIndex` of
/// zero or more. A negative `tabIndex` is script-only focus — reachable by
/// `.focus()`, never by <kbd>Tab</kbd> — which is why it does not count here.
pub(super) fn focusable(tree: &Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) -> bool {
    match tab_index(tree, opening) {
        Some(index) => index >= 0.0,
        None => INTERACTIVE_ELEMENTS.contains(name),
    }
}

/// Whether the element answers a key press.
fn has_key_handler(opening: &jsx::Opening<Loc, Loc>) -> bool {
    KEY_HANDLERS
        .iter()
        .any(|handler| attribute(opening, handler).is_some())
}

/// The role written on this element, when one is.
fn written_role_name(tree: &Tree<'_>, opening: &jsx::Opening<Loc, Loc>) -> Option<&'static str> {
    match aria::written_role(tree.scope, opening) {
        Some((_, aria::Written::Role(role))) => Some(role.name),
        _ => None,
    }
}

// --- a11y/tabindex-no-positive ----------------------------------------------

/// A `tabIndex` above zero.
///
/// A positive `tabIndex` does not move an element one place forward; it moves
/// it ahead of **everything** that has none, and every other positive value on
/// the page joins the same queue. The reading order and the tab order stop
/// agreeing, and the disagreement grows with each one added.
fn tabindex_no_positive(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = attribute(opening, "tabIndex") else {
        return;
    };
    let Some(index) = tab_index(tree, opening) else {
        return;
    };
    if index <= 0.0 {
        return;
    }
    tree.report(
        &written.loc,
        TABINDEX_NO_POSITIVE,
        format!(
            "a `tabIndex` of {index} puts this element ahead of everything the document orders \
             itself, so the tab order stops matching the reading order; give it `tabIndex={{0}}` \
             and let its position in the markup decide"
        ),
    );
}

// --- a11y/no-aria-hidden-on-focusable ---------------------------------------

/// `aria-hidden` on something a keyboard still lands on.
///
/// `aria-hidden="true"` takes the element out of the accessibility tree while
/// leaving it in the tab order, which is the worst of both: focus arrives on
/// something a screen reader has nothing to say about, and the reader is told
/// nothing at all about where they are.
fn no_aria_hidden_on_focusable(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = attribute(opening, "aria-hidden") else {
        return;
    };
    let hidden = match tree.scope.value(written) {
        Value::Bool(value) => value,
        Value::Text(text) => text == "true",
        _ => return,
    };
    if !hidden || !focusable(tree, name, opening) {
        return;
    }
    tree.report(
        &written.loc,
        NO_ARIA_HIDDEN_ON_FOCUSABLE,
        format!(
            "`<{name}>` is hidden from assistive technology but still takes focus, so a keyboard \
             lands on an element a screen reader cannot announce; drop the `aria-hidden`, or take \
             it out of the tab order with `tabIndex={{-1}}`"
        ),
    );
}

// --- a11y/aria-activedescendant-has-tabindex --------------------------------

/// `aria-activedescendant` on something that never holds focus.
///
/// The attribute says "focus is on me, and the element it is standing in for
/// is this one" — a combobox holding its highlighted option. An element that
/// cannot take focus in the first place never gets to say it.
fn activedescendant_has_tabindex(
    tree: &mut Tree<'_>,
    name: &str,
    opening: &jsx::Opening<Loc, Loc>,
) {
    let Some(written) = attribute(opening, "aria-activedescendant") else {
        return;
    };
    if focusable(tree, name, opening) {
        return;
    }
    tree.report(
        &written.loc,
        ACTIVEDESCENDANT_TABINDEX,
        format!(
            "`aria-activedescendant` says which element focus is standing in for, and `<{name}>` \
             cannot take focus, so nothing ever reads it; give it `tabIndex={{0}}`"
        ),
    );
}

// --- a11y/mouse-events-have-key-events --------------------------------------

/// A hover handler with no focus handler beside it.
///
/// `onMouseOver` and `onMouseOut` fire for a pointer only. The keyboard
/// equivalents are `onFocus` and `onBlur`, and a tooltip or a menu built on
/// the mouse pair alone simply never opens for somebody tabbing through.
fn mouse_events_have_key_events(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    for (mouse, key) in [("onMouseOver", "onFocus"), ("onMouseOut", "onBlur")] {
        let Some(written) = attribute(opening, mouse) else {
            continue;
        };
        if attribute(opening, key).is_some() {
            continue;
        }
        tree.report(
            &written.loc,
            MOUSE_EVENTS_HAVE_KEY_EVENTS,
            format!(
                "`{mouse}` fires for a pointer and nothing else, so whatever it does never happens \
                 for a keyboard; add `{key}`, which is the same moment for somebody tabbing \
                 through"
            ),
        );
    }
}

// --- a11y/click-events-have-key-events --------------------------------------

/// A click handler on a role a keyboard cannot work.
///
/// The half of the plugin's split that takes the element which **has** a
/// `role`: somebody has said what this is, and nothing answers a key press, so
/// the missing handler is the whole of the advice. The element with no role is
/// `a11y/no-static-element-interactions`, which is where the other half of
/// this question lives — see the module documentation.
fn click_events_have_key_events(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = attribute(opening, "onClick") else {
        return;
    };
    // The element HTML already works with a keyboard: a `<button>` answers
    // Enter and Space without being told, and its `onClick` fires for both.
    if INTERACTIVE_ELEMENTS.contains(name) || has_key_handler(opening) {
        return;
    }
    let Some(role) = written_role_name(tree, opening) else {
        return;
    };
    tree.report(
        &written.loc,
        CLICK_EVENTS_HAVE_KEY_EVENTS,
        format!(
            "`<{name} role=\"{role}\">` answers a click and no key press, so a keyboard can reach \
             it and still not use it; add an `onKeyDown` that runs the same handler"
        ),
    );
}

// --- a11y/interactive-supports-focus ----------------------------------------

/// A widget role a keyboard cannot reach.
///
/// A role from ARIA's widget set is a promise that this element behaves like
/// the control it names, and the first thing every one of them needs is to be
/// reachable. Read from the generated ARIA table rather than from a list kept
/// here, so that "is this a widget" has one answer across the crate.
fn interactive_supports_focus(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    if attribute(opening, "onClick").is_none() && !has_key_handler(opening) {
        return;
    }
    let Some((written, aria::Written::Role(role))) = aria::written_role(tree.scope, opening) else {
        return;
    };
    if !role.is_widget() || focusable(tree, name, opening) {
        return;
    }
    tree.report(
        &written.loc,
        INTERACTIVE_SUPPORTS_FOCUS,
        format!(
            "`role=\"{role}\"` is a widget and `<{name}>` cannot take focus, so a keyboard never \
             reaches the handler on it; give it `tabIndex={{0}}`, or use the element that is \
             already this role",
            role = role.name,
        ),
    );
}

// --- a11y/no-noninteractive-tabindex ----------------------------------------

/// A `tabIndex` on something there is nothing to do with.
///
/// Tabbing is how a keyboard reaches the controls, and every stop that is not
/// one makes the rest further away. A heading or an article put in the tab
/// order gives no reason for being there and no way to act on it.
///
/// A negative `tabIndex` is not reported: it is script-only focus, which is
/// how a dialog or a live region is focused after it opens, and that is a
/// reason to be focusable without being a tab stop.
fn no_noninteractive_tabindex(tree: &mut Tree<'_>, name: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = attribute(opening, "tabIndex") else {
        return;
    };
    let Some(index) = tab_index(tree, opening) else {
        return;
    };
    if index < 0.0 || INTERACTIVE_ELEMENTS.contains(name) {
        return;
    }
    // A written role decides it: a widget role is a control somebody is
    // building, whatever the element under it is.
    if let Some(role) = written_role_name(tree, opening) {
        if aria::role(role).is_some_and(aria::Role::is_widget) {
            return;
        }
        tree.report(
            &written.loc,
            NO_NONINTERACTIVE_TABINDEX,
            format!(
                "`role=\"{role}\"` is not a control, so this `tabIndex` adds a stop with nothing \
                 to do at it; drop it, or use `tabIndex={{-1}}` if something focuses this \
                 element itself"
            ),
        );
        return;
    }
    tree.report(
        &written.loc,
        NO_NONINTERACTIVE_TABINDEX,
        format!(
            "`<{name}>` is not a control, so this `tabIndex` adds a stop with nothing to do at \
             it; drop it, or use `tabIndex={{-1}}` if something focuses this element itself"
        ),
    );
}
