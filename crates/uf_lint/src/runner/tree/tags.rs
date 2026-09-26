//! The `a11y/*` rules that weigh a role against the element it was put on.
//!
//! `a11y/aria-unsupported-elements` asks whether the element is one ARIA
//! reserves, where nothing it is given is ever read;
//! `a11y/no-redundant-roles` whether the role restates what HTML already said;
//! and `a11y/prefer-tag-over-role` whether an element exists that *is* the role
//! and would not have to be told.
//!
//! # What `a11y/prefer-tag-over-role` leaves alone
//!
//! `eslint-plugin-jsx-a11y` reports every role an HTML element can carry, which
//! includes the roles of scripted widgets: it will tell you to write `<select>`
//! instead of `role="combobox"` and `<input type="checkbox">` instead of
//! `role="checkbox"`. That is not advice anybody can take — a combobox with a
//! custom popup is not a `<select>`, and the whole of `@uniflowed/ui` is built
//! out of exactly those roles — so uf reports a role only where the tag really
//! is a drop-in:
//!
//! * not a widget role, nor one that manages or belongs to a widget
//!   ([`aria::Role::is_widget`]);
//! * not an element that only means anything inside a particular parent
//!   ([`PARENT_BOUND`]) — a `<li>` outside a list is not a `listitem`;
//! * not an element that is form-associated ([`FORM_ASSOCIATED`]), which
//!   carries value semantics and behaviour of its own;
//! * not an element that takes focus or answers events — `<div
//!   role="separator" tabIndex={0}>` is a split pane's draggable divider, and
//!   `<hr>` is a void element nothing can focus;
//! * and not a mapping that is a quirk of HTML-AAM rather than advice
//!   ([`NOT_ADVICE`]).
//!
//! What is left is 27 roles — the landmarks and the document structure, where
//! the advice is exactly right: `role="navigation"` is a `<nav>`,
//! `role="heading"` is an `<h2>`, `role="list"` is a `<ul>`. The interactive
//! half is not silence in uf either; it is `a11y/no-static-element-interactions`
//! and the rules about focus and keys, which ask whether the widget *works*
//! rather than what it is spelled as.

use uf_config::UniflowedConfig;
use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::aria::{self, Implied, Written};
use super::value::Value;
use super::{Tree, has_handler, has_key_handler, has_spread, is_interactive};
use crate::{Severity, severity};

/// `a11y/aria-unsupported-elements`.
const ARIA_UNSUPPORTED: &str = "a11y/aria-unsupported-elements";

/// `a11y/no-interactive-element-to-noninteractive-role`.
const INTERACTIVE_TO_NONINTERACTIVE: &str = "a11y/no-interactive-element-to-noninteractive-role";

/// `a11y/no-noninteractive-element-interactions`.
const NONINTERACTIVE_INTERACTIONS: &str = "a11y/no-noninteractive-element-interactions";

/// `a11y/no-noninteractive-element-to-interactive-role`.
const NONINTERACTIVE_TO_INTERACTIVE: &str = "a11y/no-noninteractive-element-to-interactive-role";

/// `a11y/no-redundant-roles`.
const NO_REDUNDANT_ROLES: &str = "a11y/no-redundant-roles";

/// `a11y/prefer-tag-over-role`.
const PREFER_TAG_OVER_ROLE: &str = "a11y/prefer-tag-over-role";

/// Elements that only mean anything inside a particular parent.
///
/// Swapping a `<div>` for one of these does not give you the role unless you
/// also build the parent it belongs to, so the suggestion would be wrong.
static PARENT_BOUND: phf::Set<&'static str> = phf::phf_set! {
    "area", "caption", "col", "colgroup", "datalist", "dd", "dt", "figcaption", "legend", "li",
    "optgroup", "option", "source", "summary", "tbody", "td", "tfoot", "th", "thead", "tr", "track",
};

/// Form-associated elements, which bring value semantics, validation and
/// default behaviour that a plain element with a role does not have.
static FORM_ASSOCIATED: phf::Set<&'static str> = phf::phf_set! {
    "input", "meter", "output", "progress", "select", "textarea",
};

/// Roles whose element mapping is a quirk of HTML-AAM rather than advice.
///
/// `generic` is the role of every unremarkable element, `presentation` maps to
/// `<img alt="">` because that is how HTML spells a decorative image, and
/// `document` maps to `<html>`. "Write `<html>` instead" is not a thing to say
/// about a `<div>`.
static NOT_ADVICE: phf::Set<&'static str> = phf::phf_set! {
    "document", "generic", "none", "presentation",
};

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    aria_unsupported: Option<Severity>,
    interactive_to_noninteractive: Option<Severity>,
    noninteractive_interactions: Option<Severity>,
    noninteractive_to_interactive: Option<Severity>,
    no_redundant_roles: Option<Severity>,
    prefer_tag_over_role: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            aria_unsupported: severity(config, ARIA_UNSUPPORTED),
            interactive_to_noninteractive: severity(config, INTERACTIVE_TO_NONINTERACTIVE),
            noninteractive_interactions: severity(config, NONINTERACTIVE_INTERACTIONS),
            noninteractive_to_interactive: severity(config, NONINTERACTIVE_TO_INTERACTIVE),
            no_redundant_roles: severity(config, NO_REDUNDANT_ROLES),
            prefer_tag_over_role: severity(config, PREFER_TAG_OVER_ROLE),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        [
            self.aria_unsupported,
            self.interactive_to_noninteractive,
            self.noninteractive_interactions,
            self.noninteractive_to_interactive,
            self.no_redundant_roles,
            self.prefer_tag_over_role,
        ]
        .iter()
        .any(Option::is_some)
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            ARIA_UNSUPPORTED => self.aria_unsupported,
            INTERACTIVE_TO_NONINTERACTIVE => self.interactive_to_noninteractive,
            NONINTERACTIVE_INTERACTIONS => self.noninteractive_interactions,
            NONINTERACTIVE_TO_INTERACTIVE => self.noninteractive_to_interactive,
            NO_REDUNDANT_ROLES => self.no_redundant_roles,
            PREFER_TAG_OVER_ROLE => self.prefer_tag_over_role,
            _ => None,
        }
    }
}

/// Run every rule in this module that is on against one host element.
///
/// All three ask what the element *is*, so a component — whose rendered element
/// this module cannot see — is nobody's business here.
pub(super) fn check(tree: &mut Tree<'_>, host: &str, opening: &jsx::Opening<Loc, Loc>) {
    let levels = tree.tags;
    if levels.aria_unsupported.is_some() {
        aria_unsupported_elements(tree, host, opening);
    }
    if levels.no_redundant_roles.is_some() {
        no_redundant_roles(tree, host, opening);
    }
    if levels.prefer_tag_over_role.is_some() {
        prefer_tag_over_role(tree, host, opening);
    }
    if levels.interactive_to_noninteractive.is_some() {
        interactive_to_noninteractive(tree, host, opening);
    }
    if levels.noninteractive_to_interactive.is_some() {
        noninteractive_to_interactive(tree, host, opening);
    }
    if levels.noninteractive_interactions.is_some() {
        noninteractive_element_interactions(tree, host, opening);
    }
}

/// The role this element carries, as far as the source settles it: the one
/// written on it, or the one HTML gives it.
///
/// [`Implied::Unsettled`] is a role that depends on where the element sits —
/// `<li>` is a `listitem` only inside a list, `<td>` a `cell` only inside a
/// table — and the answer is [`None`] rather than a guess. That is a real
/// narrowing against the plugin, which reports those on the tag alone.
fn settled_role(
    tree: &Tree<'_>,
    host: &str,
    opening: &jsx::Opening<Loc, Loc>,
) -> Option<&'static aria::Role> {
    if let Some((_, Written::Role(role))) = aria::written_role(tree.scope, opening) {
        return Some(role);
    }
    match aria::implicit_role(tree.scope, host, opening) {
        Implied::Certain(role) => Some(role),
        Implied::Unsettled | Implied::None => None,
    }
}

// --- a11y/aria-unsupported-elements -----------------------------------------

/// ARIA on an element ARIA reserves.
///
/// `<meta>`, `<script>`, `<title>` and the rest are never rendered, so they are
/// not in the accessibility tree at all: a `role` or an `aria-*` on one is not
/// overridden or ignored so much as unread, and whatever it was meant to say is
/// said nowhere.
fn aria_unsupported_elements(tree: &mut Tree<'_>, host: &str, opening: &jsx::Opening<Loc, Loc>) {
    if !aria::is_reserved(host) {
        return;
    }
    for written in &*opening.attributes {
        let jsx::OpeningAttribute::Attribute(written) = written else {
            continue;
        };
        let jsx::attribute::Name::Identifier(name) = &written.name else {
            continue;
        };
        let name = &*name.name;
        let named = if name.eq_ignore_ascii_case("role") {
            "role"
        } else {
            match aria::spec(name) {
                Some(spec) => spec.name,
                None => continue,
            }
        };
        // Nothing renders, so nothing is wrong.
        if tree.scope.value(written) == Value::Nullish {
            continue;
        }
        tree.report(
            &written.loc,
            ARIA_UNSUPPORTED,
            uf_infra::into_string(uf_infra::cstr!(
                "`<{host}>` is one of the elements ARIA reserves: it is never rendered, so it is \
                 not in the accessibility tree and `{named}` on it is read by nothing (WCAG \
                 4.1.2); put it on the element a reader actually reaches"
            )),
        );
    }
}

// --- a11y/no-redundant-roles ------------------------------------------------

/// A role the element already had.
///
/// ARIA's own first rule is to use the HTML element that means what you mean,
/// and a role that restates one is at best noise: it is a second place to keep
/// the same fact correct, and it stops being correct the moment the element
/// changes.
///
/// **The `<nav role="navigation">` exception**, which the plugin also ships by
/// default: w3's guidance recommends writing it for assistive technology that
/// predates the HTML5 elements, so it is a decision rather than a mistake.
fn no_redundant_roles(tree: &mut Tree<'_>, host: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some((written, Written::Role(role))) = aria::written_role(tree.scope, opening) else {
        return;
    };
    let Implied::Certain(implicit) = aria::implicit_role(tree.scope, host, opening) else {
        return;
    };
    if implicit.name != role.name {
        return;
    }
    if host == "nav" && role.name == "navigation" {
        return;
    }
    tree.report(
        &written.loc,
        NO_REDUNDANT_ROLES,
        uf_infra::into_string(uf_infra::cstr!(
            "a `<{host}>` is already a `{role}`, so `role=\"{role}\"` tells the browser what it \
             told the browser: ARIA's first rule is to use the element and not to repeat it, and \
             the copy is one more thing to keep true when the markup changes; drop the attribute",
            role = role.name,
        )),
    );
}

// --- a11y/prefer-tag-over-role ----------------------------------------------

/// A role an HTML element already is.
///
/// `<div role="navigation">` works, and `<nav>` works without being told: it is
/// shorter, it survives somebody dropping the attribute, and it carries the
/// rest of the element's behaviour with it. See the module documentation for
/// the roles this deliberately says nothing about — every widget role among
/// them, because a tag is not what makes a widget work.
fn prefer_tag_over_role(tree: &mut Tree<'_>, host: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some((written, Written::Role(role))) = aria::written_role(tree.scope, opening) else {
        return;
    };
    if role.is_widget() || NOT_ADVICE.contains(role.name) {
        return;
    }
    let Some(tags) = aria::elements_for(role.name) else {
        return;
    };
    // The element already is one of them, which is `a11y/no-redundant-roles`'
    // question rather than this one's.
    if tags.iter().any(|tag| tag.name == host) {
        return;
    }
    if tags.iter().any(|tag| PARENT_BOUND.contains(tag.name)) {
        return;
    }
    if tags.iter().all(|tag| FORM_ASSOCIATED.contains(tag.name)) {
        return;
    }
    // An element that takes focus or answers events is a widget somebody has
    // built, whatever its role is spelled as. `<div role="separator"
    // tabIndex={0}>` is the draggable divider of a split pane, and `<hr>` — a
    // void element nothing can focus — is not what it should be rewritten as.
    if takes_focus_or_events(opening) {
        return;
    }

    let mut spellings: Vec<String> = Vec::new();
    for tag in tags {
        let spelled = tag.spelled();
        if !spellings.contains(&spelled) {
            spellings.push(spelled);
        }
    }
    let suggestion = spelled_list(&spellings);
    tree.report(
        &written.loc,
        PREFER_TAG_OVER_ROLE,
        uf_infra::into_string(uf_infra::cstr!(
            "`role=\"{role}\"` names what {suggestion} already is: the element carries the role \
             without being told, keeps it when somebody moves or copies the markup, and brings \
             the rest of its behaviour with it; write {suggestion} instead of a `<{host}>` with a \
             role",
            role = role.name,
        )),
    );
}

/// Whether the element is one somebody has wired up: it takes focus, or it
/// answers an event.
///
/// The tag is only half of what such an element is, so swapping it is not the
/// improvement this rule offers — see [`prefer_tag_over_role`].
fn takes_focus_or_events(opening: &jsx::Opening<Loc, Loc>) -> bool {
    opening.attributes.iter().any(|attribute| {
        let jsx::OpeningAttribute::Attribute(attribute) = attribute else {
            return false;
        };
        let jsx::attribute::Name::Identifier(name) = &attribute.name else {
            return false;
        };
        let name = &*name.name;
        name == "tabIndex"
            || name
                .strip_prefix("on")
                .is_some_and(|rest| rest.starts_with(|first: char| first.is_ascii_uppercase()))
    })
}

// --- a11y/no-interactive-element-to-noninteractive-role ---------------------

/// A control told it is not one.
///
/// `<button role="presentation">` still focuses, still fires its handler and
/// still sits in the tab order; the only thing the role changes is that a
/// screen reader stops calling it a button. The element becomes a control
/// nobody is told about, which is worse than either half on its own.
fn interactive_to_noninteractive(
    tree: &mut Tree<'_>,
    host: &str,
    opening: &jsx::Opening<Loc, Loc>,
) {
    // A `{...spread}` may be carrying the very attribute that decides whether
    // this is a control at all — an `<a>` is a link only with an `href`. This
    // is the one rule here that *reports* when the answer is "interactive", so
    // an answer the source does not settle must not be read as a yes.
    if has_spread(opening) || !is_interactive(tree.scope, host, opening) {
        return;
    }
    let Some((written, Written::Role(role))) = aria::written_role(tree.scope, opening) else {
        return;
    };
    if role.is_interactive() {
        return;
    }
    tree.report(
        &written.loc,
        INTERACTIVE_TO_NONINTERACTIVE,
        uf_infra::into_string(uf_infra::cstr!(
            "`<{host}>` is a control a keyboard reaches and `role=\"{role}\"` is not, so it keeps \
             the behaviour and loses the announcement: focus lands on something a screen reader \
             calls {role}; drop the role, or use an element that is one",
            role = role.name,
        )),
    );
}

// --- a11y/no-noninteractive-element-to-interactive-role ---------------------

/// Semantics of one kind given a role of the other.
///
/// `<ul role="button">` keeps a list's structure — a screen reader still walks
/// it as items — while claiming to be a control that Enter works. A `<div>` is
/// not reported: it has no role of its own, and giving one a widget role is
/// how every custom control is built.
fn noninteractive_to_interactive(
    tree: &mut Tree<'_>,
    host: &str,
    opening: &jsx::Opening<Loc, Loc>,
) {
    if is_interactive(tree.scope, host, opening) {
        return;
    }
    let Some((written, Written::Role(role))) = aria::written_role(tree.scope, opening) else {
        return;
    };
    if !role.is_interactive() {
        return;
    }
    let Implied::Certain(implicit) = aria::implicit_role(tree.scope, host, opening) else {
        return;
    };
    if implicit.is_interactive() || NOT_ADVICE.contains(implicit.name) {
        return;
    }
    // A role the element's own role is an ancestor of says something sharper
    // about it rather than something else: `grid` is a kind of `table`, so
    // `<table role="grid">` is how a keyboard-navigable grid is built and not
    // a contradiction to report. Asked of ARIA's taxonomy in the generated
    // table, because the widget flag cannot tell the two apart — `button` and
    // `grid` are both widgets, and only one of them disagrees with its host.
    if role.inherits_from(implicit.name) {
        return;
    }
    tree.report(
        &written.loc,
        NONINTERACTIVE_TO_INTERACTIVE,
        uf_infra::into_string(uf_infra::cstr!(
            "`<{host}>` already has the role `{implicit}` and `role=\"{role}\"` says it is a \
             control, so a screen reader is told one thing and the markup does another; build the \
             control out of an element that has no semantics of its own, or use the element whose \
             own role is `{role}`",
            implicit = implicit.name,
            role = role.name,
        )),
    );
}

// --- a11y/no-noninteractive-element-interactions ----------------------------

/// Handlers on something that is not a control at all.
///
/// The third and last share of a question uf splits three ways, and the line
/// is drawn so exactly one rule answers any markup:
///
/// * no `role` at all — `a11y/no-static-element-interactions`, where nobody
///   has said what the element is;
/// * a `role` and no key handler — `a11y/click-events-have-key-events`, where
///   the missing handler is the whole of the advice;
/// * a non-interactive role **and** a key handler — this one. Somebody has
///   wired up the keyboard and the element still is not a control, so the
///   advice is not another handler but that these semantics are wrong.
fn noninteractive_element_interactions(
    tree: &mut Tree<'_>,
    host: &str,
    opening: &jsx::Opening<Loc, Loc>,
) {
    if is_interactive(tree.scope, host, opening)
        || has_spread(opening)
        || !has_handler(tree.scope, opening)
    {
        return;
    }
    // An element a keyboard can reach is not the defect this rule names. The
    // complaint is that handlers sit where nothing can get to them, and a
    // `tabIndex` of zero or more answers it: a named, focusable region that
    // handles arrow keys is the documented way to build a scrollable or
    // navigable container, and reporting it would be reporting working
    // markup. Whether such an element is *announced* as a control is
    // `a11y/no-noninteractive-element-to-interactive-role`'s question.
    //
    // `focusable` is `interaction`'s, shared rather than restated, so that
    // "can a keyboard reach this" has one answer across the crate.
    if super::interaction::focusable(tree, host, opening) {
        return;
    }
    // Without a key handler this is one of the other two rules' markup. See
    // the doc comment: the split exists so that nothing is reported twice.
    if !has_key_handler(tree.scope, opening) {
        return;
    }
    let Some(role) = settled_role(tree, host, opening) else {
        return;
    };
    if role.is_widget() || NOT_ADVICE.contains(role.name) {
        return;
    }
    tree.report(
        &opening.loc,
        NONINTERACTIVE_INTERACTIONS,
        uf_infra::into_string(uf_infra::cstr!(
            "`<{host}>` has the role `{role}` and is not a control, so handlers on it are \
             reachable but never announced as anything to act on; move them to a `<button>`, or \
             give the element a role that says what it does",
            role = role.name,
        )),
    );
}

/// `<nav>`, or `<ul>`, `<ol> or `<menu>`.
fn spelled_list(spellings: &[String]) -> String {
    match spellings {
        [] => String::new(),
        [only] => uf_infra::into_string(uf_infra::cstr!("`{only}`")),
        [rest @ .., last] => uf_infra::into_string(uf_infra::cstr!(
            "{} or `{last}`",
            rest.iter()
                .map(|spelled| uf_infra::into_string(uf_infra::cstr!("`{spelled}`")))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}
