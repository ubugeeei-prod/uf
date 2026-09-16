//! The ARIA table, and the questions the `a11y/*` rules ask of it.
//!
//! A misspelled ARIA attribute is the quietest bug on this list. `aria-lable`
//! is not rejected by the browser, not reported by React, and not read by any
//! assistive technology: the DOM keeps the attribute and nothing ever looks at
//! it, so the control is unlabelled and the page looks finished. There is no
//! run-time symptom to notice, which is exactly why a linter has to be the one
//! to notice. The same is true of every other answer this table gives: a role
//! that does not exist, a value of the wrong type, a state the role does not
//! take. None of them fails, and none of them works.
//!
//! # Where the table comes from
//!
//! [`table`] is generated from [`aria-query`][aria-query] 5.3.2 — the encoding
//! of [WAI-ARIA 1.2][aria-1.2] that `eslint-plugin-jsx-a11y` itself reads, so
//! that a rule uf's guide sends to a plugin rule answers the same question from
//! the same data — plus the two ARIA 1.3 index-text attributes
//! (`aria-colindextext`, `aria-rowindextext`) that `a11y/aria-props` already
//! accepts, which `aria-query` has not caught up with. The generator is
//! `tools/aria/gen-table.cjs`, and its header says how to run it.
//!
//! **Not axe-core.** axe-core 4.13 is the other table one could import, and it
//! is deliberately more permissive than the specification in places: it allows
//! `aria-required` on `radio`, which ARIA 1.2 does not give `radio` and which
//! the plugin's own documentation lists as a failure. Where the two disagree
//! the specification decides, and each rule's documentation says so.
//!
//! [aria-query]: https://www.npmjs.com/package/aria-query
//! [aria-1.2]: https://www.w3.org/TR/wai-aria-1.2/
//!
//! # What "the element's role" means
//!
//! A `role` attribute is only half the answer. HTML elements carry roles of
//! their own — `<a href>` is a `link`, `<input type="checkbox">` a `checkbox`,
//! `<nav>` a `navigation` — and [`Implied`] is how the rules ask for one.
//! Several of those roles depend on where the element sits (`<header>` is a
//! `banner` only directly inside `<body>`; `<li>` is a `listitem` only inside a
//! list) or on a value the source does not hold (`<input type={kind}>`), and
//! both come back as [`Implied::Unsettled`], which is where every rule stops.

mod table;

use uf_flow::Loc;
use uf_flow::ast::jsx;

use super::value::{Scope, Value};
use super::{attribute, has_spread};

/// How an ARIA attribute's value is spelled. WAI-ARIA 1.2, "Values".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// `true` or `false`.
    Boolean,
    /// `true`, `false` or `mixed`.
    Tristate,
    /// A whole number.
    Integer,
    /// Any number.
    Number,
    /// Any text.
    Text,
    /// The id of an element.
    Id,
    /// A space-separated list of ids.
    IdList,
    /// One of these words.
    Token(&'static [&'static str]),
    /// A space-separated list of these words.
    TokenList(&'static [&'static str]),
}

/// One ARIA attribute.
pub(super) struct Spec {
    pub(super) name: &'static str,
    pub(super) kind: Kind,
    /// Whether the token list also takes `true` and `false` — `aria-current`,
    /// `aria-haspopup` and `aria-invalid` each do.
    pub(super) boolean_spelling: bool,
    /// Whether `undefined` is one of the tokens, which `aria-orientation`
    /// spells out as a real value rather than an absence.
    pub(super) undefined_token: bool,
}

/// What kind of role a role is. WAI-ARIA 1.2, "Categorization of Roles".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Flags(u8);

impl Flags {
    pub(super) const NONE: Self = Self(0);
    /// A role in the class hierarchy that is never put on an element.
    pub(super) const ABSTRACT: Self = Self(1 << 0);
    /// A widget that manages focus among the elements it owns: `listbox`,
    /// `menu`, `radiogroup`, `tablist`, `tree`.
    pub(super) const COMPOSITE: Self = Self(1 << 1);
    /// A user-interface widget rather than a piece of document structure.
    pub(super) const WIDGET: Self = Self(1 << 2);
    /// A role that only means anything inside another one — `listitem` in a
    /// `list`, `cell` in a `row`.
    pub(super) const CONTEXTUAL: Self = Self(1 << 3);

    pub(super) const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(super) const fn has(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

/// One ARIA role, with the attributes it takes as bit masks over [`ATTRIBUTES`].
///
/// A mask rather than a list of names because every rule's question is
/// membership — does this role take this attribute — and 139 roles with a
/// list each is a table nobody wants to read or pay for.
///
/// [`ATTRIBUTES`]: table::ATTRIBUTES
pub(super) struct Role {
    pub(super) name: &'static str,
    pub(super) flags: Flags,
    /// The attributes the role takes, its inherited and global ones included.
    supported: u64,
    /// The attributes without which the role is incomplete.
    required: u64,
    /// The attributes the role is forbidden — `generic` and the other
    /// name-prohibited roles take no `aria-label`.
    prohibited: u64,
}

impl Role {
    /// Whether this role takes `attribute`, which must be an ARIA name.
    pub(super) fn supports(&self, attribute: &str) -> bool {
        bit(attribute).is_some_and(|bit| self.supported & bit != 0)
    }

    /// Whether ARIA forbids `attribute` on this role.
    pub(super) fn prohibits(&self, attribute: &str) -> bool {
        bit(attribute).is_some_and(|bit| self.prohibited & bit != 0)
    }

    /// The attributes this role cannot do without.
    pub(super) fn required(&self) -> impl Iterator<Item = &'static str> {
        names(self.required)
    }

    /// Whether this role is part of a widget somebody has to make work: it is
    /// one, it manages one, or it only exists inside one.
    ///
    /// This is the line `a11y/prefer-tag-over-role` draws. A landmark or a
    /// piece of document structure is a tag away; a widget is a component
    /// away, and telling somebody to write `<select>` instead of the combobox
    /// they have built is not advice they can take.
    pub(super) fn is_widget(&self) -> bool {
        self.flags.has(
            Flags::WIDGET
                .union(Flags::COMPOSITE)
                .union(Flags::CONTEXTUAL),
        )
    }
}

/// An attribute an element must carry to have the role beside it.
pub(super) struct Required {
    pub(super) name: &'static str,
    /// The value it must have, when the mapping names one.
    pub(super) value: Option<&'static str>,
    /// Whether it is enough for the attribute to be there at all.
    pub(super) set: bool,
    /// Whether the attribute must be *absent*.
    pub(super) unset: bool,
}

/// The role an HTML element carries without being told. HTML-AAM, as
/// `aria-query` encodes it.
pub(super) struct Implicit {
    pub(super) element: &'static str,
    pub(super) attributes: &'static [Required],
    /// Whether the mapping also depends on where the element sits — "scoped to
    /// the body element", "direct descendant of `ul`". uf does not answer
    /// those.
    pub(super) placed: bool,
    pub(super) role: &'static str,
}

/// An attribute in an HTML element's description.
pub(super) struct Attr {
    pub(super) name: &'static str,
    pub(super) value: Option<&'static str>,
}

/// An HTML element that already is some role.
pub(super) struct Tag {
    pub(super) name: &'static str,
    pub(super) attributes: &'static [Attr],
}

impl Tag {
    /// How the element is written, for a diagnostic: `<nav>`, `<input
    /// type="checkbox">`.
    pub(super) fn spelled(&self) -> String {
        let mut spelled = format!("<{}", self.name);
        for attribute in self.attributes {
            match attribute.value {
                Some(value) => spelled.push_str(&format!(" {}=\"{value}\"", attribute.name)),
                None => spelled.push_str(&format!(" {}", attribute.name)),
            }
        }
        spelled.push('>');
        spelled
    }
}

/// The role an element has, as far as the source settles it.
pub(super) enum Implied {
    /// This role, wherever the element is put.
    Certain(&'static Role),
    /// A role that depends on where the element sits, or on a value the module
    /// does not hold.
    Unsettled,
    /// HTML gives this element no role of its own.
    None,
}

/// Whether `name` is an ARIA attribute.
///
/// The comparison is case-insensitive because HTML attribute names are, and
/// React lowercases an `aria-*` prop on its way to the DOM: `aria-Label`
/// reaches the document as `aria-label` and works, so a rule that reported it
/// would be reporting working code.
pub(super) fn is_aria_attribute(name: &str) -> bool {
    spec(name).is_some()
}

/// The ARIA attribute called `name`, however it is cased.
pub(super) fn spec(name: &str) -> Option<&'static Spec> {
    if let Some(found) = exact_spec(name) {
        return Some(found);
    }
    name.chars()
        .any(char::is_uppercase)
        .then(|| exact_spec(&name.to_lowercase()))
        .flatten()
}

fn exact_spec(name: &str) -> Option<&'static Spec> {
    table::ATTRIBUTES
        .binary_search_by(|candidate| candidate.name.cmp(name))
        .ok()
        .map(|at| &table::ATTRIBUTES[at])
}

/// The bit that stands for `name` in a role's masks.
fn bit(name: &str) -> Option<u64> {
    let at = table::ATTRIBUTES
        .binary_search_by(|candidate| candidate.name.cmp(name))
        .ok()?;
    Some(1 << at)
}

/// The names a mask holds, in the table's order.
fn names(mask: u64) -> impl Iterator<Item = &'static str> {
    (0..table::ATTRIBUTES.len())
        .filter(move |at| mask >> at & 1 == 1)
        .map(|at| table::ATTRIBUTES[at].name)
}

/// The ARIA role called `name`, however it is cased.
///
/// Role tokens are matched ASCII case-insensitively, the way HTML matches
/// them, so `role="BUTTON"` is the `button` role and is not reported.
pub(super) fn role(name: &str) -> Option<&'static Role> {
    if let Some(found) = exact_role(name) {
        return Some(found);
    }
    name.chars()
        .any(|character| character.is_ascii_uppercase())
        .then(|| exact_role(&name.to_ascii_lowercase()))
        .flatten()
}

fn exact_role(name: &str) -> Option<&'static Role> {
    table::ROLES
        .binary_search_by(|candidate| candidate.name.cmp(name))
        .ok()
        .map(|at| &table::ROLES[at])
}

/// The HTML elements that already are `role`, or [`None`] when none does.
pub(super) fn elements_for(role: &str) -> Option<&'static [Tag]> {
    table::ROLE_ELEMENTS
        .iter()
        .find(|(name, _)| *name == role)
        .map(|(_, tags)| *tags)
}

/// Whether ARIA reserves `element`, in which case nothing it is given is read.
pub(super) fn is_reserved(element: &str) -> bool {
    table::RESERVED_ELEMENTS.contains(&element)
}

/// The role the `role` attribute names, when it names one.
///
/// ARIA lets an author write a list — `role="doc-subtitle heading"` — and the
/// browser takes the first role it knows, so that is the one uf reads.
pub(super) fn written_role(
    scope: Scope,
    opening: &jsx::Opening<Loc, Loc>,
) -> Option<(&jsx::Attribute<Loc, Loc>, Written)> {
    let written = attribute(opening, "role")?;
    let value = match scope.value(written) {
        Value::Text(text) => text,
        Value::Unknown => return Some((written, Written::Unsettled)),
        // `role={null}` renders no attribute, and `role` on its own renders
        // `role="true"`, which is not a role but is `a11y/aria-role`'s to
        // report rather than this function's to interpret.
        _ => return Some((written, Written::None)),
    };
    let mut abstract_only = false;
    for token in value.split_ascii_whitespace() {
        if let Some(found) = role(token) {
            if found.flags.has(Flags::ABSTRACT) {
                abstract_only = true;
                continue;
            }
            return Some((written, Written::Role(found)));
        }
    }
    Some((
        written,
        if abstract_only {
            Written::Abstract
        } else {
            Written::None
        },
    ))
}

/// What an element's `role` attribute says.
pub(super) enum Written {
    /// The first role in the list that ARIA defines.
    Role(&'static Role),
    /// Every name in the list is an abstract role, which no element may carry.
    Abstract,
    /// A value the module does not hold.
    Unsettled,
    /// No name in the list is a role.
    None,
}

/// The role `element` carries on its own, given the attributes it was written
/// with.
///
/// Answers [`Implied::Unsettled`] rather than guessing whenever the mapping
/// asks something the source does not settle: a value that is an expression, a
/// `{...spread}` that may be carrying the very attribute the mapping turns on,
/// or a mapping that depends on the element's ancestors.
pub(super) fn implicit_role(
    scope: Scope,
    element: &str,
    opening: &jsx::Opening<Loc, Loc>,
) -> Implied {
    for entry in table::IMPLICIT_ROLES
        .iter()
        .filter(|entry| entry.element == element)
    {
        match matches(scope, entry, opening) {
            Some(true) if entry.placed => return Implied::Unsettled,
            Some(true) => {
                return role(entry.role).map_or(Implied::Unsettled, Implied::Certain);
            }
            Some(false) => {}
            None => return Implied::Unsettled,
        }
    }
    // Either HTML gives this element no role, or every mapping it has wanted an
    // attribute the element was not written with.
    Implied::None
}

/// Whether the element was written the way this mapping asks, or [`None`] when
/// the source does not say.
fn matches(scope: Scope, entry: &Implicit, opening: &jsx::Opening<Loc, Loc>) -> Option<bool> {
    for required in entry.attributes {
        let written = attribute(opening, required.name);
        let Some(written) = written else {
            // A spread may be supplying it.
            if spread_carries(opening, required.name) {
                return None;
            }
            if required.unset {
                continue;
            }
            return Some(false);
        };
        let value = scope.value(written);
        if value == Value::Unknown {
            return None;
        }
        let present = value != Value::Nullish;
        if required.unset {
            if present {
                return Some(false);
            }
            continue;
        }
        if !present {
            return Some(false);
        }
        if let Some(wanted) = required.value {
            match value {
                Value::Text(text) if text.eq_ignore_ascii_case(wanted) => {}
                _ => return Some(false),
            }
        } else if !required.set {
            return Some(false);
        }
    }
    Some(true)
}

/// Whether a `{...spread}` may be deciding `name`.
fn spread_carries(opening: &jsx::Opening<Loc, Loc>, name: &str) -> bool {
    has_spread(opening) && super::value::spread_may_set(opening, name)
}

/// Whether `value` is one this attribute takes.
///
/// [`Value::Unknown`] and [`Value::Nullish`] are never wrong: the first is a
/// value the module does not hold, and the second renders no attribute at all.
pub(super) fn value_fits(spec: &Spec, value: Value<'_>) -> bool {
    let text = match value {
        Value::Unknown | Value::Nullish => return true,
        Value::Bool(written) => {
            return match spec.kind {
                Kind::Boolean | Kind::Tristate => true,
                Kind::Token(_) | Kind::TokenList(_) => spec.boolean_spelling,
                // `aria-label` on its own is `aria-label={true}`, which reaches
                // the DOM as the word "true".
                _ => {
                    let _ = written;
                    false
                }
            };
        }
        Value::Number(number) => {
            return match spec.kind {
                Kind::Integer => number.fract() == 0.0,
                Kind::Number => true,
                // A number is not a name, an id or a word from a list.
                _ => false,
            };
        }
        Value::Text(text) => text,
    };
    match spec.kind {
        Kind::Boolean => matches!(text.trim(), "true" | "false"),
        Kind::Tristate => matches!(text.trim(), "true" | "false" | "mixed"),
        Kind::Integer => text.trim().parse::<i64>().is_ok(),
        Kind::Number => text.trim().parse::<f64>().is_ok(),
        Kind::Text | Kind::Id | Kind::IdList => true,
        Kind::Token(tokens) => is_token(spec, tokens, text.trim()),
        Kind::TokenList(tokens) => {
            let mut any = false;
            for token in text.split_ascii_whitespace() {
                any = true;
                if !is_token(spec, tokens, token) {
                    return false;
                }
            }
            any
        }
    }
}

fn is_token(spec: &Spec, tokens: &[&str], token: &str) -> bool {
    if tokens
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(token))
    {
        return true;
    }
    if spec.boolean_spelling && matches!(token, "true" | "false") {
        return true;
    }
    spec.undefined_token && token == "undefined"
}

/// The words an attribute takes, for a diagnostic.
pub(super) fn permitted(spec: &Spec) -> String {
    let tokens = match spec.kind {
        Kind::Token(tokens) | Kind::TokenList(tokens) => tokens,
        _ => return String::new(),
    };
    let mut words: Vec<String> = tokens.iter().map(|token| format!("`{token}`")).collect();
    if spec.boolean_spelling {
        words.push(String::from("`true`"));
        words.push(String::from("`false`"));
    }
    words.join(", ")
}

/// How the kind reads in a diagnostic.
pub(super) fn wanted(spec: &Spec) -> String {
    match spec.kind {
        Kind::Boolean => String::from("`true` or `false`"),
        Kind::Tristate => String::from("`true`, `false` or `mixed`"),
        Kind::Integer => String::from("a whole number"),
        Kind::Number => String::from("a number"),
        Kind::Text => String::from("text"),
        Kind::Id => String::from("the id of an element in the page"),
        Kind::IdList => String::from("the ids of elements in the page, separated by spaces"),
        Kind::Token(_) => format!("one of {}", permitted(spec)),
        Kind::TokenList(_) => format!("one or more of {}, separated by spaces", permitted(spec)),
    }
}

/// The ARIA attribute `name` was probably meant to be, if one is close enough.
///
/// "Close enough" is a Levenshtein distance of at most two, which covers the
/// mistakes that actually happen — a transposition (`aria-lable`), a dropped
/// letter (`aria-labeledby`), a doubled one, a plural (`aria-controls` written
/// as `aria-control`) — and stops well short of turning an attribute somebody
/// invented into a suggestion to use an unrelated one. [`None`] when nothing
/// is near, and the diagnostic then just says the name is not an ARIA
/// attribute, which is still the whole of what is wrong with it.
pub(super) fn nearest_aria_attribute(name: &str) -> Option<&'static str> {
    nearest(
        &name.to_lowercase(),
        table::ATTRIBUTES.iter().map(|spec| spec.name),
    )
}

/// The role `name` was probably meant to be, by the same measure.
pub(super) fn nearest_role(name: &str) -> Option<&'static str> {
    nearest(
        &name.to_ascii_lowercase(),
        table::ROLES
            .iter()
            .filter(|role| !role.flags.has(Flags::ABSTRACT))
            .map(|role| role.name),
    )
}

fn nearest(name: &str, candidates: impl Iterator<Item = &'static str>) -> Option<&'static str> {
    candidates
        .filter_map(|candidate| {
            let distance = edit_distance(name, candidate, 2)?;
            Some((distance, candidate))
        })
        // Ties go to the alphabetically first name, so the suggestion does not
        // depend on the iteration order of the table.
        .min()
        .map(|(_, candidate)| candidate)
}

/// Levenshtein distance between `left` and `right`, or [`None`] over `cap`.
///
/// The cap is what keeps this cheap: the length difference alone answers most
/// pairs, and the remaining ones are two short ASCII strings. Rows rather than
/// a matrix, because only the previous row is ever read.
fn edit_distance(left: &str, right: &str, cap: usize) -> Option<usize> {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len().abs_diff(right.len()) > cap {
        return None;
    }

    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (i, &l) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, &r) in right.iter().enumerate() {
            let substitution = previous[j] + usize::from(l != r);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        // Every distance in the final row is at least the smallest in this
        // one, so a row that is already over the cap cannot come back under it.
        if current.iter().min().copied().unwrap_or(0) > cap {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let distance = previous[right.len()];
    (distance <= cap).then_some(distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_aria_attributes() {
        assert!(is_aria_attribute("aria-label"));
        assert!(is_aria_attribute("aria-labelledby"));
        assert!(is_aria_attribute("aria-braillelabel"));
        assert!(!is_aria_attribute("aria-lable"));
        assert!(!is_aria_attribute("aria-"));
    }

    /// The two names ARIA 1.3 adds that `aria-query` has not caught up with:
    /// `a11y/aria-props` accepts them, so the table has to know them too.
    #[test]
    fn knows_the_index_text_attributes() {
        assert!(is_aria_attribute("aria-colindextext"));
        assert!(is_aria_attribute("aria-rowindextext"));
    }

    #[test]
    fn an_attribute_name_is_case_insensitive() {
        // React lowercases it on the way to the DOM, so this one works.
        assert!(is_aria_attribute("aria-Label"));
        assert!(!is_aria_attribute("aria-Lable"));
    }

    #[test]
    fn the_attribute_table_is_sorted_for_binary_search() {
        assert!(
            table::ATTRIBUTES
                .windows(2)
                .all(|pair| pair[0].name < pair[1].name)
        );
        assert!(
            table::ROLES
                .windows(2)
                .all(|pair| pair[0].name < pair[1].name)
        );
    }

    #[test]
    fn suggests_the_name_that_was_meant() {
        assert_eq!(nearest_aria_attribute("aria-lable"), Some("aria-label"));
        assert_eq!(
            nearest_aria_attribute("aria-labeledby"),
            Some("aria-labelledby")
        );
        assert_eq!(nearest_aria_attribute("aria-hiden"), Some("aria-hidden"));
    }

    #[test]
    fn suggests_nothing_when_nothing_is_near() {
        assert_eq!(nearest_aria_attribute("aria-nonsense-attribute"), None);
        assert_eq!(nearest_aria_attribute("aria-"), None);
    }

    #[test]
    fn edit_distance_gives_up_past_the_cap() {
        assert_eq!(edit_distance("abc", "abc", 2), Some(0));
        assert_eq!(edit_distance("abc", "abd", 2), Some(1));
        assert_eq!(edit_distance("abc", "xyz", 2), None);
        assert_eq!(edit_distance("abc", "abcdef", 2), None);
    }

    /// The masks are indexed by position in `ATTRIBUTES`, so a role's
    /// membership question has to come back with the names the specification
    /// gives it.
    #[test]
    fn a_roles_attributes_come_back_by_name() {
        let checkbox = role("checkbox").expect("checkbox is a role");
        assert!(checkbox.supports("aria-checked"));
        assert!(checkbox.supports("aria-label"));
        assert!(!checkbox.supports("aria-level"));
        assert_eq!(checkbox.required().collect::<Vec<_>>(), ["aria-checked"]);

        // axe-core allows `aria-required` on `radio`; ARIA 1.2 does not, and
        // the plugin's own documentation lists it as a failure.
        let radio = role("radio").expect("radio is a role");
        assert!(!radio.supports("aria-required"));

        let generic = role("generic").expect("generic is a role");
        assert!(generic.prohibits("aria-label"));
        assert!(generic.prohibits("aria-labelledby"));
    }

    /// WAI-ARIA 1.2 gives every role the global states and properties, and
    /// `aria-query` does not spell them out on all of them: `none` carries no
    /// properties and no superclass, and `doc-pullquote` inherits only from
    /// `none`. A table built from each role's own properties left those two
    /// supporting nothing, so `a11y/role-supports-aria-props` reported
    /// `aria-hidden` on `<div role="none">` — working markup, which is the one
    /// thing these rules must never do.
    #[test]
    fn every_role_takes_the_global_attributes() {
        for role in table::ROLES {
            if role.flags.has(Flags::ABSTRACT) {
                continue;
            }
            assert!(
                role.supports("aria-hidden"),
                "`{}` takes no `aria-hidden`",
                role.name
            );
            assert!(
                role.supports("aria-busy"),
                "`{}` takes no `aria-busy`",
                role.name
            );
        }
    }

    /// ARIA 1.3's index-text attributes go wherever the 1.2 index they are the
    /// text form of goes: a role that takes `aria-colindex` takes
    /// `aria-colindextext` too. `aria-query` stops at 1.2 and carries neither,
    /// so a mask built from it alone left all five of these roles without
    /// them, and `a11y/role-supports-aria-props` — an `error` — reported
    /// `<td role="cell" aria-colindextext="Q1">`, which is valid markup.
    ///
    /// The generator merges them in and refuses to emit a table that does not.
    /// This is the half that fails in CI, where the generator does not run.
    #[test]
    fn the_index_text_attributes_go_where_their_index_goes() {
        for name in ["cell", "columnheader", "gridcell", "row", "rowheader"] {
            let found = role(name).expect("a role in the table");
            assert!(
                found.supports("aria-colindex"),
                "`{name}` takes no `aria-colindex`"
            );
            assert!(
                found.supports("aria-colindextext"),
                "`{name}` takes no `aria-colindextext`"
            );
            assert!(
                found.supports("aria-rowindex"),
                "`{name}` takes no `aria-rowindex`"
            );
            assert!(
                found.supports("aria-rowindextext"),
                "`{name}` takes no `aria-rowindextext`"
            );
        }
    }

    #[test]
    fn a_role_name_is_case_insensitive() {
        assert!(role("BUTTON").is_some());
        assert!(role("Checkbox").is_some());
        assert!(role("datepicker").is_none());
    }

    #[test]
    fn an_element_that_is_already_a_role_is_named() {
        let tags = elements_for("navigation").expect("navigation has an element");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].spelled(), "<nav>");

        let tags = elements_for("checkbox").expect("checkbox has an element");
        assert_eq!(tags[0].spelled(), "<input type=\"checkbox\">");

        assert!(elements_for("tooltip").is_none());
    }

    #[test]
    fn the_reserved_elements_are_the_ones_nothing_announces() {
        assert!(is_reserved("meta"));
        assert!(is_reserved("script"));
        assert!(is_reserved("title"));
        assert!(!is_reserved("div"));
        assert!(!is_reserved("span"));
    }

    #[test]
    fn a_widget_role_is_told_from_a_structural_one() {
        assert!(role("checkbox").expect("checkbox").is_widget());
        assert!(role("listbox").expect("listbox").is_widget());
        assert!(role("listitem").expect("listitem").is_widget());
        assert!(!role("navigation").expect("navigation").is_widget());
        assert!(!role("heading").expect("heading").is_widget());
    }

    #[test]
    fn abstract_roles_are_marked() {
        assert!(role("range").expect("range").flags.has(Flags::ABSTRACT));
        assert!(!role("button").expect("button").flags.has(Flags::ABSTRACT));
        assert!(
            role("listbox")
                .expect("listbox")
                .flags
                .has(Flags::COMPOSITE)
        );
        assert!(
            role("listitem")
                .expect("listitem")
                .flags
                .has(Flags::CONTEXTUAL)
        );
    }

    #[test]
    fn values_are_checked_against_the_kind() {
        let hidden = spec("aria-hidden").expect("aria-hidden");
        assert!(value_fits(hidden, Value::Text("true")));
        assert!(value_fits(hidden, Value::Bool(true)));
        assert!(!value_fits(hidden, Value::Text("yes")));

        let checked = spec("aria-checked").expect("aria-checked");
        assert!(value_fits(checked, Value::Text("mixed")));
        assert!(!value_fits(checked, Value::Text("maybe")));

        let level = spec("aria-level").expect("aria-level");
        assert!(value_fits(level, Value::Number(2.0)));
        assert!(value_fits(level, Value::Text("2")));
        assert!(!value_fits(level, Value::Text("two")));
        assert!(!value_fits(level, Value::Bool(true)));

        let current = spec("aria-current").expect("aria-current");
        assert!(value_fits(current, Value::Text("page")));
        assert!(value_fits(current, Value::Text("true")));
        assert!(!value_fits(current, Value::Text("pages")));

        let relevant = spec("aria-relevant").expect("aria-relevant");
        assert!(value_fits(relevant, Value::Text("additions text")));
        assert!(!value_fits(relevant, Value::Text("additions nonsense")));

        // Nothing the module does not hold is ever wrong.
        assert!(value_fits(hidden, Value::Unknown));
        assert!(value_fits(hidden, Value::Nullish));
    }
}
