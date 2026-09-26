//! The `a11y/*` rules that read the ARIA table.
//!
//! Four questions about the same three words an author writes on an element —
//! a `role`, an `aria-*` name, an `aria-*` value — and none of them has a
//! run-time symptom. `a11y/aria-role` asks whether the role exists;
//! `a11y/aria-proptypes` whether a value is one its attribute takes;
//! `a11y/role-has-required-aria-props` whether a role was given the state it
//! cannot do without; and `a11y/role-supports-aria-props` whether the element's
//! role — written, or the one HTML gives it — takes the attributes it was
//! written with. A browser keeps every one of these in the DOM and an
//! assistive technology reads none of them, so the page looks finished and the
//! control is unusable.
//!
//! # What the rules do not answer
//!
//! An element that spreads props may be passing in the very `role` a rule is
//! reasoning about, and an attribute whose value is an expression is a value
//! the module does not hold. Both are silence, which is the direction every
//! `a11y/*` rule errs in: see [`super::aria::Implied`] for the roles HTML only
//! gives an element in a particular place, which are silence for the same
//! reason.

use uf_config::UniflowedConfig;
use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::aria::{self, Implied, Written};
use super::value::{Value, spread_may_set};
use super::{Tree, attribute};
use crate::{Severity, severity};

/// `a11y/aria-role`.
const ARIA_ROLE: &str = "a11y/aria-role";

/// `a11y/aria-proptypes`.
const ARIA_PROPTYPES: &str = "a11y/aria-proptypes";

/// `a11y/role-has-required-aria-props`.
const ROLE_REQUIRED_PROPS: &str = "a11y/role-has-required-aria-props";

/// `a11y/role-supports-aria-props`.
const ROLE_SUPPORTS_PROPS: &str = "a11y/role-supports-aria-props";

/// Configured severity for each rule in this module.
#[derive(Clone, Copy)]
pub(super) struct Levels {
    aria_role: Option<Severity>,
    aria_proptypes: Option<Severity>,
    role_required_props: Option<Severity>,
    role_supports_props: Option<Severity>,
}

impl Levels {
    pub(super) fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            aria_role: severity(config, ARIA_ROLE),
            aria_proptypes: severity(config, ARIA_PROPTYPES),
            role_required_props: severity(config, ROLE_REQUIRED_PROPS),
            role_supports_props: severity(config, ROLE_SUPPORTS_PROPS),
        }
    }

    /// Whether any rule in this module is on.
    pub(super) fn any(&self) -> bool {
        [
            self.aria_role,
            self.aria_proptypes,
            self.role_required_props,
            self.role_supports_props,
        ]
        .iter()
        .any(Option::is_some)
    }

    pub(super) fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            ARIA_ROLE => self.aria_role,
            ARIA_PROPTYPES => self.aria_proptypes,
            ROLE_REQUIRED_PROPS => self.role_required_props,
            ROLE_SUPPORTS_PROPS => self.role_supports_props,
            _ => None,
        }
    }
}

/// Run every rule in this module that is on against one element.
///
/// `host` is the tag when the element is an HTML element and [`None`] when it
/// is a component. The two rules about a name and a value run on components as
/// well: a component that takes an `aria-*` or `role` prop is taking it to put
/// on a DOM node, and a name that does not exist or a value of the wrong type
/// is as inert one level up as it is at the bottom. The two rules about *the
/// element's role* need to know what the element is, so they stop here.
pub(super) fn check(tree: &mut Tree<'_>, host: Option<&str>, opening: &jsx::Opening<Loc, Loc>) {
    let levels = tree.roles;
    if levels.aria_role.is_some() {
        aria_role(tree, opening);
    }
    if levels.aria_proptypes.is_some() {
        aria_proptypes(tree, opening);
    }
    let Some(host) = host else {
        return;
    };
    if levels.role_required_props.is_some() {
        role_has_required_aria_props(tree, host, opening);
    }
    if levels.role_supports_props.is_some() {
        role_supports_aria_props(tree, host, opening);
    }
}

// --- a11y/aria-role ---------------------------------------------------------

/// A `role` that ARIA does not define, or that no element may carry.
///
/// ARIA lets an author write a list of roles and the browser takes the first
/// one it knows, so a list with one real role in it is a working element and is
/// not reported. What is reported is a list with none: the element keeps
/// whatever role HTML gave it, which is usually nothing at all, and the
/// author's intent is silently gone.
///
/// The abstract roles — `widget`, `input`, `range`, `section` — are the other
/// half. They exist to hold the role hierarchy together and WAI-ARIA 1.2 says
/// outright that they must not be used on elements, so one written on an
/// element is ignored exactly like a misspelling.
fn aria_role(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    let Some(written) = attribute(opening, "role") else {
        return;
    };
    let value = tree.scope.value(written);
    let text = match value {
        // `role={null}` and `role={undefined}` render no attribute.
        Value::Nullish | Value::Unknown => return,
        Value::Text(text) => text,
        // `role` on its own is `role={true}`, which reaches the DOM as the word
        // "true"; a number reaches it as its digits. Neither is a role.
        Value::Bool(_) | Value::Number(_) => {
            tree.report(
                &written.loc,
                ARIA_ROLE,
                String::from(
                    "`role` needs the name of an ARIA role, and this is not one, so the element \
                     keeps whatever role HTML gave it and the one that was meant is silently gone \
                     (WCAG 4.1.2); write the role as a string, such as `role=\"button\"`",
                ),
            );
            return;
        }
    };

    let mut abstract_role = None;
    for token in text.split_ascii_whitespace() {
        match aria::role(token) {
            Some(found) if found.flags.has(aria::Flags::ABSTRACT) => {
                abstract_role.get_or_insert(found.name);
            }
            // A role ARIA defines: the element has a role, and this rule is
            // done.
            Some(_) => return,
            None => {}
        }
    }

    if let Some(name) = abstract_role {
        tree.report(
            &written.loc,
            ARIA_ROLE,
            uf_infra::cstr!(
                "`{name}` is an abstract role: it exists to hold ARIA's role hierarchy together \
                 and WAI-ARIA says it must not be put on an element, so nothing reads it and the \
                 element is left with no role at all (WCAG 4.1.2); name the concrete role the \
                 element plays, such as `role=\"slider\"` for a `range`"
            )
            .into_string(),
        );
        return;
    }

    let written_text = text.trim();
    let suggestion = aria::nearest_role(written_text)
        .map(|near| uf_infra::into_string(uf_infra::cstr!("; did you mean `{near}`?")))
        .unwrap_or_default();
    let subject = if written_text.is_empty() {
        String::from("an empty `role`")
    } else {
        uf_infra::into_string(uf_infra::cstr!("`{written_text}`"))
    };
    tree.report(
        &written.loc,
        ARIA_ROLE,
        uf_infra::cstr!(
            "{subject} is not an ARIA role, so nothing reads it: the browser keeps the attribute, \
             no assistive technology looks at it, and the element is announced as whatever HTML \
             made it (WCAG 4.1.2){suggestion}"
        )
        .into_string(),
    );
}

// --- a11y/aria-proptypes ----------------------------------------------------

/// An `aria-*` attribute whose value is not one it takes.
///
/// `aria-hidden="yes"` is the shape of it: ARIA defines `aria-hidden` as
/// `true`/`false`, a browser that is handed anything else treats the attribute
/// as absent, and the element the author meant to hide is read out. Every
/// attribute in the table is checked the same way, against the type WAI-ARIA
/// 1.2 gives it.
fn aria_proptypes(tree: &mut Tree<'_>, opening: &jsx::Opening<Loc, Loc>) {
    for attribute in &*opening.attributes {
        let jsx::OpeningAttribute::Attribute(attribute) = attribute else {
            continue;
        };
        let jsx::attribute::Name::Identifier(name) = &attribute.name else {
            continue;
        };
        let name = &*name.name;
        let Some(spec) = aria::spec(name) else {
            continue;
        };
        let value = tree.scope.value(attribute);
        if aria::value_fits(spec, value) {
            continue;
        }
        let written = match value {
            Value::Text(text) if text.trim().is_empty() => String::from("an empty value"),
            Value::Text(text) => uf_infra::into_string(uf_infra::cstr!("`{text}`")),
            Value::Bool(_) => uf_infra::into_string(uf_infra::cstr!(
                "`{name}` on its own, which renders the word `true`"
            )),
            Value::Number(number) => uf_infra::into_string(uf_infra::cstr!("`{number}`")),
            Value::Nullish | Value::Unknown => continue,
        };
        tree.report(
            &attribute.loc,
            ARIA_PROPTYPES,
            uf_infra::cstr!(
                "`{name}` takes {wanted}, and {written} is not one, so the attribute is dropped \
                 and the state it was written for is never announced (WCAG 4.1.2)",
                wanted = aria::wanted(spec),
            )
            .into_string(),
        );
    }
}

// --- a11y/role-has-required-aria-props --------------------------------------

/// A role written without the state it cannot do without.
///
/// `role="checkbox"` says "this is a checkbox" and `aria-checked` is the only
/// thing that says whether it is checked; without it a screen reader announces
/// a checkbox whose state it cannot read, which is worse than the `<div>` it
/// was before the role was added.
///
/// **Deliberately silent** where HTML already provides the state. `<input
/// type="checkbox" role="switch">` is the pattern for a switch: the role
/// renames the control and the native checkbox keeps answering for
/// `aria-checked`, so nothing is missing.
fn role_has_required_aria_props(tree: &mut Tree<'_>, host: &str, opening: &jsx::Opening<Loc, Loc>) {
    let Some((written, Written::Role(role))) = aria::written_role(tree.scope, opening) else {
        return;
    };
    let missing: Vec<&'static str> = role
        .required()
        .filter(|name| {
            attribute(opening, name).is_none()
                && !spread_may_set(opening, name)
                && !natively_provides(host, opening, tree.scope, name)
        })
        .collect();
    if missing.is_empty() {
        return;
    }

    let names = missing
        .iter()
        .map(|name| uf_infra::into_string(uf_infra::cstr!("`{name}`")))
        .collect::<Vec<_>>()
        .join(" and ");
    let is = if missing.len() == 1 { "is" } else { "are" };
    tree.report(
        &written.loc,
        ROLE_REQUIRED_PROPS,
        uf_infra::cstr!(
            "`role=\"{role}\"` tells a screen reader this is a {role}, and {names} {is} the state \
             a {role} is announced by: without it the control is read out with nothing to say \
             whether it is on, off or anywhere in between (WCAG 4.1.2); add it, or drop the role \
             and use the HTML element that carries the state itself",
            role = role.name,
        )
        .into_string(),
    );
}

/// Whether the element already answers for `wanted` without being told.
///
/// The HTML Accessibility API Mappings give a handful of elements a state of
/// their own — a checkbox is checked, a range has a value, an option is
/// selected — and an ARIA role on top of one of those does not have to repeat
/// it. This is the same exemption `eslint-plugin-jsx-a11y` makes through
/// `axobject-query`, written out.
fn natively_provides(
    host: &str,
    opening: &jsx::Opening<Loc, Loc>,
    scope: super::value::Scope,
    wanted: &str,
) -> bool {
    let input_type = || {
        if spread_may_set(opening, "type") {
            return None;
        }
        match attribute(opening, "type").map(|written| scope.value(written)) {
            Some(Value::Text(text)) => Some(text.trim().to_ascii_lowercase()),
            // An `<input>` with no `type` is a text field.
            None => Some(String::from("text")),
            _ => None,
        }
    };
    // A `type` the module does not hold is not a claim that this is a
    // checkbox, and not a claim that it is not — so the rule stops rather than
    // reporting a state the element may already have.
    let input_is = |kinds: &[&str]| input_type().is_none_or(|kind| kinds.contains(&kind.as_str()));
    match (host, wanted) {
        ("input", "aria-checked") => input_is(&["checkbox", "radio"]),
        ("input", "aria-valuenow") => input_is(&["range", "number"]),
        ("progress" | "meter", "aria-valuenow") => true,
        ("option", "aria-selected") => true,
        ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", "aria-level") => true,
        // A `<select>`, or a text field wired to a `<datalist>`, is a combobox
        // the browser opens and closes on its own.
        ("select", "aria-expanded" | "aria-controls") => true,
        ("input", "aria-expanded" | "aria-controls") => {
            attribute(opening, "list").is_some() || spread_may_set(opening, "list")
        }
        _ => false,
    }
}

// --- a11y/role-supports-aria-props ------------------------------------------

/// An `aria-*` attribute the element's role does not take.
///
/// Most ARIA states and properties belong to particular roles: `aria-checked`
/// to the things that can be checked, `aria-required` to the things that can be
/// filled in. One written anywhere else is inert — the DOM keeps it and the
/// accessibility tree has nowhere to put it — so `<li role="radio"
/// aria-required>` reads as a radio button with nothing said about whether an
/// answer is needed.
///
/// The role is the one the author wrote, and otherwise the one HTML gives the
/// element: `<a href>` is a `link` and takes no `aria-checked`.
///
/// **Where uf differs from `eslint-plugin-jsx-a11y`.** WAI-ARIA 1.2 makes
/// `generic` — the role of a plain `<div>` or `<span>` — *name-prohibited*:
/// `aria-label` and `aria-labelledby` on one are discarded, which is why
/// `<div aria-label="Close">` names nothing. The plugin does not report it,
/// because its table of implicit roles has no entry for `div` at all. uf
/// reports it, because it is the same defect the rest of the rule is about and
/// it is one of the commonest ways an element ends up unnamed.
fn role_supports_aria_props(tree: &mut Tree<'_>, host: &str, opening: &jsx::Opening<Loc, Loc>) {
    let role = match aria::written_role(tree.scope, opening) {
        Some((_, Written::Role(role))) => role,
        // A role that is not a role is `a11y/aria-role`'s to report, and until
        // it is fixed there is no role to check attributes against.
        Some((_, Written::Abstract | Written::None | Written::Unsettled)) => return,
        None => {
            if spread_may_set(opening, "role") {
                return;
            }
            match aria::implicit_role(tree.scope, host, opening) {
                Implied::Certain(role) => role,
                Implied::Unsettled | Implied::None => return,
            }
        }
    };

    let implicit = attribute(opening, "role").is_none();
    for attribute in &*opening.attributes {
        let jsx::OpeningAttribute::Attribute(attribute) = attribute else {
            continue;
        };
        let jsx::attribute::Name::Identifier(name) = &attribute.name else {
            continue;
        };
        let name = &*name.name;
        let Some(spec) = aria::spec(name) else {
            continue;
        };
        // Nothing renders, so nothing is wrong.
        if tree.scope.value(attribute) == Value::Nullish {
            continue;
        }
        let name = spec.name;
        if role.prohibits(name) {
            tree.report(
                &attribute.loc,
                ROLE_SUPPORTS_PROPS,
                prohibited_message(host, role.name, name, implicit),
            );
            continue;
        }
        if role.supports(name) {
            continue;
        }
        let of = if implicit {
            uf_infra::into_string(uf_infra::cstr!(
                "a `<{host}>` is a `{role}`",
                role = role.name
            ))
        } else {
            uf_infra::into_string(uf_infra::cstr!("`role=\"{role}\"`", role = role.name))
        };
        tree.report(
            &attribute.loc,
            ROLE_SUPPORTS_PROPS,
            uf_infra::cstr!(
                "{of}, and ARIA gives `{role}` no `{name}`, so the attribute sits in the DOM with \
                 nothing to read it and the state it describes is never announced (WCAG 4.1.2); \
                 remove it, or give the element the role this state belongs to",
                role = role.name,
            )
            .into_string(),
        );
    }
}

/// The message for an attribute ARIA forbids on the element's role.
fn prohibited_message(host: &str, role: &str, name: &str, implicit: bool) -> String {
    if role == "generic" {
        return uf_infra::cstr!(
            "a `<{host}>` has no role of its own, and WAI-ARIA forbids naming one: `{name}` on it \
             is discarded, so this element has no accessible name at all (WAI-ARIA 1.2, \
             \"prohibited attributes\"); put the name where it can be read — `<section \
             {name}=…>`, which is a landmark, a `role` that takes a name, or text inside the \
             element"
        )
        .into_string();
    }
    let of = if implicit {
        uf_infra::into_string(uf_infra::cstr!("a `<{host}>` is a `{role}`"))
    } else {
        uf_infra::into_string(uf_infra::cstr!("`role=\"{role}\"`"))
    };
    uf_infra::cstr!(
        "{of}, and WAI-ARIA forbids `{name}` on a `{role}`: it is discarded rather than announced \
         (WAI-ARIA 1.2, \"prohibited attributes\"); name a role that takes a name, or put the \
         words in the element"
    )
    .into_string()
}
