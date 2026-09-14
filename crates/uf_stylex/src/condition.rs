//! The state a declaration applies in, and where that puts it in the sheet.
//!
//! A `:hover` rule and a base rule are both a single class selector, so they
//! have identical specificity and the later one in the sheet wins. Ordering by
//! source position would therefore make hover work or not work depending on
//! which module the bundler reached first. [`StyleCondition::weight`] replaces
//! that with a fixed order: base, then pseudo-classes in the order the cascade
//! expects them (`:link`, `:visited`, `:focus-within`, `:hover`, `:focus`,
//! `:active`), then at-rules, then pseudo-elements.

use compact_str::CompactString;
use serde::Serialize;

/// Weight added by an at-rule such as `@media` or `@supports`.
const AT_RULE_WEIGHT: u32 = 200;
/// Weight added by a pseudo-element, which must outrank every pseudo-class.
const PSEUDO_ELEMENT_WEIGHT: u32 = 5_000;
/// Weight given to a pseudo-class uf does not have an opinion about.
const UNKNOWN_PSEUDO_WEIGHT: u32 = 100;

/// The state one declaration applies in.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "selector")]
pub enum StyleCondition {
    /// The declaration always applies.
    Base,
    /// A pseudo-class such as `:hover`, written with its leading colon.
    PseudoClass(CompactString),
    /// A pseudo-element such as `::before`, written with its leading colons.
    PseudoElement(CompactString),
    /// An at-rule such as `@media (min-width: 600px)`.
    AtRule(CompactString),
}

impl StyleCondition {
    /// Read a condition out of an authored object key.
    ///
    /// `default` is the key StyleX uses for the unconditional value inside a
    /// conditional object, so it maps to [`StyleCondition::Base`].
    pub fn parse(key: &str) -> Option<Self> {
        if key == "default" {
            return Some(Self::Base);
        }
        if let Some(rest) = key.strip_prefix("::") {
            return is_selector_body(rest).then(|| Self::PseudoElement(CompactString::new(key)));
        }
        if let Some(rest) = key.strip_prefix(':') {
            return is_selector_body(rest).then(|| Self::PseudoClass(CompactString::new(key)));
        }
        if key.starts_with('@') {
            return is_at_rule_body(key).then(|| Self::AtRule(CompactString::new(key)));
        }
        None
    }

    /// Whether an authored key looks like a condition rather than a property.
    pub fn is_condition_key(key: &str) -> bool {
        key == "default" || key.starts_with(':') || key.starts_with('@')
    }

    /// The condition's text, as it goes into the sheet and into the class hash.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Base => "",
            Self::PseudoClass(text) | Self::PseudoElement(text) | Self::AtRule(text) => {
                text.as_str()
            }
        }
    }

    /// Ordering weight; smaller sorts earlier.
    pub fn weight(&self) -> u32 {
        match self {
            Self::Base => 0,
            Self::PseudoClass(name) => pseudo_class_weight(name.as_str()),
            Self::PseudoElement(_) => PSEUDO_ELEMENT_WEIGHT,
            Self::AtRule(_) => AT_RULE_WEIGHT,
        }
    }

    /// The selector suffix appended to `.class` in the emitted rule.
    pub fn selector_suffix(&self) -> &str {
        match self {
            Self::Base | Self::AtRule(_) => "",
            Self::PseudoClass(text) | Self::PseudoElement(text) => text.as_str(),
        }
    }

    /// The at-rule the emitted rule has to be wrapped in, if any.
    pub fn at_rule(&self) -> Option<&str> {
        match self {
            Self::AtRule(text) => Some(text.as_str()),
            _ => None,
        }
    }
}

/// Ordering weight of one pseudo-class.
///
/// The numbers follow the order the cascade needs rather than any spec: a
/// structural selector first, then link states in `:link`, `:visited`,
/// `:focus-within`, `:hover`, `:focus`, `:active` order, then form states. A
/// pseudo-class uf has no entry for lands on [`UNKNOWN_PSEUDO_WEIGHT`], where
/// it still sorts deterministically because the class name breaks the tie.
///
/// A key that selects an attribute is none of those, and is weighed by
/// [`announced_state_weight`] instead.
fn pseudo_class_weight(name: &str) -> u32 {
    if name.contains('[') {
        return announced_state_weight(name);
    }
    match PSEUDO_CLASSES.binary_search_by_key(&name, |entry| entry.0) {
        Ok(index) => PSEUDO_CLASSES[index].1,
        Err(_) => UNKNOWN_PSEUDO_WEIGHT,
    }
}

/// Weight of a key that selects an attribute: the state a headless part
/// announces, such as `:is([aria-selected=true])`.
///
/// After every pseudo-class in [`PSEUDO_CLASSES`], and that order is the whole
/// decision. A state a part announces is a fact about the element and a pointer
/// resting on it is not, so a pressed toggle stays pressed under the pointer
/// and an item that says `aria-disabled` does not light up when it is hovered.
/// Sorting them the other way — which is where these keys landed when they
/// were an unknown pseudo-class — makes every `:hover` quietly repaint the
/// state a reader was just shown, and the only fix a style author had was to
/// repeat the state inside every interaction key. It is also the order a
/// Tailwind user already has in their head, where `aria-*` and `data-*`
/// variants sort after `hover` and `disabled`.
///
/// Before the at-rules, which still wrap everything, `prefers-reduced-motion`
/// included.
const ANNOUNCED_STATE_WEIGHT: u32 = 195;

/// Where a key that selects an attribute sorts.
///
/// [`ANNOUNCED_STATE_WEIGHT`], plus one for every pseudo-class the key adds
/// beside the one holding the attribute, so `:is([aria-pressed=true]):hover`
/// sorts after `:is([aria-pressed=true])` and a pressed toggle can be given a
/// hover colour of its own. Without that step the two would share a weight and
/// the class name would decide between them, which is deterministic and says
/// nothing about what the author meant. Capped below the at-rules.
fn announced_state_weight(name: &str) -> u32 {
    let mut depth = 0usize;
    let mut outermost = 0u32;
    for byte in name.bytes() {
        match byte {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b':' if depth == 0 => outermost += 1,
            _ => {}
        }
    }
    (ANNOUNCED_STATE_WEIGHT + outermost.saturating_sub(1)).min(AT_RULE_WEIGHT - 1)
}

/// Whether the body of a pseudo selector is made only of safe characters.
///
/// The text is emitted straight into a selector, so anything that could close
/// the rule, open a declaration block, or terminate an inline `<style>` element
/// is refused here rather than escaped later.
///
/// # The state a part announces
///
/// An attribute selector is accepted in exactly one shape: inside the argument
/// of a functional pseudo-class, as a name or as a `name=value` whose value is
/// unquoted — `:is([aria-selected=true])`, `:not([aria-disabled=true])`,
/// `:is([data-state=open])`.
///
/// `:is()` and `:not()` are the two to write it in, because both take the
/// specificity of their argument: `.x:is([open])` weighs what `.x:hover` weighs,
/// so the sheet's order is what decides between them, and
/// [`ANNOUNCED_STATE_WEIGHT`] is that order. `:where()` is accepted, as it was
/// before this, and weighs nothing — so a state written in it loses to `:hover`
/// wherever it sorts, which is the one reason not to.
///
/// That shape is what styling a headless component takes, and nothing more.
/// `@uniflowed/ui` ships no styles and announces its state instead: a selected
/// tab says `aria-selected="true"`, and the option under a select's keyboard
/// cursor says `data-active="true"` while real focus stays on the trigger. The
/// cursor is the case that makes this necessary rather than convenient. No
/// pseudo-class matches an element named by `aria-activedescendant`, the part
/// renders that option itself, and without a drawn highlight a sighted keyboard
/// user has no focus indicator at all, which is WCAG 2.4.7. Before this, the
/// highlight could only be drawn by a component that tracked the cursor a
/// second time, beside the part that already owns it.
///
/// `[`, `]` and `=` cannot close a rule, open a block or end a `<style>`
/// element, so the argument at the top of this comment is unchanged. The shape
/// is narrower than CSS on purpose:
///
/// * **No quotes.** Every value an ARIA or `data-` state takes is an
///   identifier, and a quote is one more character to reason about in text
///   that is inlined into JavaScript as well as into a stylesheet.
/// * **No bracket outside an argument.** `.x:hover[open]` is a compound
///   selector a StyleX key has no business spelling; `:is([open]):hover` says
///   the same thing in the shape this accepts.
/// * **Nothing that was a condition stops being one.** The new bytes are read
///   only between a `[` and its `]`, and a `[` only after an unclosed `(`, so
///   every key accepted before attribute selectors existed is accepted
///   byte for byte the same way.
fn is_selector_body(body: &str) -> bool {
    if body.is_empty() || body.len() > 128 {
        return false;
    }
    // Counted only to decide where a `[` may start. The parentheses themselves
    // are held to nothing stricter than they were before.
    let mut open_arguments = 0usize;
    let mut reading = Reading::Selector;
    for byte in body.bytes() {
        reading = match (reading, byte) {
            (Reading::Selector, b'(') => {
                open_arguments += 1;
                Reading::Selector
            }
            (Reading::Selector, b')') => {
                open_arguments = open_arguments.saturating_sub(1);
                Reading::Selector
            }
            (Reading::Selector, b'[') if open_arguments > 0 => Reading::Name { empty: true },
            (Reading::Selector, b'+' | b':' | b'-' | b'_') => Reading::Selector,
            (Reading::Selector, _) if byte.is_ascii_alphanumeric() => Reading::Selector,
            (Reading::Name { empty: false }, b'=') => Reading::Value { empty: true },
            (Reading::Name { empty: false } | Reading::Value { empty: false }, b']') => {
                Reading::Selector
            }
            (Reading::Name { .. }, _) if is_identifier_byte(byte) => Reading::Name { empty: false },
            (Reading::Value { .. }, _) if is_identifier_byte(byte) => {
                Reading::Value { empty: false }
            }
            _ => return false,
        };
    }
    reading == Reading::Selector
}

/// Where [`is_selector_body`] is, relative to an attribute selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// Outside any `[…]`.
    Selector,
    /// After a `[`, reading the attribute's name; `empty` until it has a byte.
    Name { empty: bool },
    /// After the `=`, reading the value; `empty` until it has a byte.
    Value { empty: bool },
}

/// A byte an attribute's name, or its unquoted value, may hold.
fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

/// Whether an at-rule is made only of safe characters.
fn is_at_rule_body(body: &str) -> bool {
    body.len() <= 256
        && body.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'-' | b'_'
                        | b'('
                        | b')'
                        | b':'
                        | b' '
                        | b'.'
                        | b','
                        | b'@'
                        | b'/'
                        | b'*'
                        | b'='
                        | b'%'
                )
        })
        && !body.contains("/*")
        && !body.contains("*/")
}

/// Known pseudo-classes and their weights. Sorted for binary search.
const PSEUDO_CLASSES: &[(&str, u32)] = &[
    (":active", 170),
    (":any-link", 111),
    (":autofill", 190),
    (":checked", 182),
    (":default", 184),
    (":dir", 50),
    (":disabled", 181),
    (":empty", 70),
    (":enabled", 180),
    (":first-child", 52),
    (":first-of-type", 53),
    (":focus", 150),
    (":focus-visible", 155),
    (":focus-within", 130),
    (":has", 45),
    (":hover", 140),
    (":in-range", 191),
    (":indeterminate", 183),
    (":invalid", 187),
    (":is", 40),
    (":lang", 51),
    (":last-child", 54),
    (":last-of-type", 55),
    (":link", 110),
    (":not", 40),
    (":nth-child", 60),
    (":nth-last-child", 61),
    (":nth-last-of-type", 62),
    (":nth-of-type", 63),
    (":only-child", 56),
    (":only-of-type", 57),
    (":optional", 192),
    (":out-of-range", 193),
    (":placeholder-shown", 188),
    (":read-only", 189),
    (":read-write", 194),
    (":required", 185),
    (":root", 30),
    (":target", 121),
    (":valid", 186),
    (":visited", 120),
    (":where", 40),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudo_class_table_is_sorted_for_binary_search() {
        assert!(PSEUDO_CLASSES.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn hover_sorts_after_the_base_state() {
        let hover = StyleCondition::parse(":hover").expect("a pseudo-class");
        assert!(hover.weight() > StyleCondition::Base.weight());
    }

    #[test]
    fn active_sorts_after_hover_and_focus() {
        let hover = StyleCondition::parse(":hover").expect("a pseudo-class");
        let focus = StyleCondition::parse(":focus").expect("a pseudo-class");
        let active = StyleCondition::parse(":active").expect("a pseudo-class");
        assert!(hover.weight() < focus.weight());
        assert!(focus.weight() < active.weight());
    }

    #[test]
    fn visited_sorts_after_link_and_before_hover() {
        let link = StyleCondition::parse(":link").expect("a pseudo-class");
        let visited = StyleCondition::parse(":visited").expect("a pseudo-class");
        let hover = StyleCondition::parse(":hover").expect("a pseudo-class");
        assert!(link.weight() < visited.weight());
        assert!(visited.weight() < hover.weight());
    }

    #[test]
    fn default_is_the_base_state() {
        assert_eq!(StyleCondition::parse("default"), Some(StyleCondition::Base));
    }

    #[test]
    fn a_selector_that_could_close_the_rule_is_refused() {
        assert_eq!(StyleCondition::parse(":hover}.evil{color:red"), None);
        assert_eq!(StyleCondition::parse("::after</style"), None);
    }

    #[test]
    fn an_at_rule_carrying_a_comment_is_refused() {
        assert_eq!(StyleCondition::parse("@media /* */ screen"), None);
    }

    #[test]
    fn an_unknown_pseudo_class_still_has_a_weight() {
        let unknown = StyleCondition::parse(":unheard-of").expect("a pseudo-class");
        assert_eq!(unknown.weight(), UNKNOWN_PSEUDO_WEIGHT);
    }

    /// The shape a headless part's state is styled in: an attribute selector
    /// inside a functional pseudo-class.
    #[test]
    fn the_state_a_part_announces_is_a_condition() {
        for key in [
            ":is([aria-selected=true])",
            ":not([aria-disabled=true])",
            ":where([data-state=open])",
            ":is([data-active])",
            ":is([aria-pressed=true]):hover",
            ":hover:not([aria-disabled=true])",
        ] {
            assert_eq!(
                StyleCondition::parse(key),
                Some(StyleCondition::PseudoClass(CompactString::new(key))),
                "{key}"
            );
        }
    }

    #[test]
    fn an_attribute_selector_outside_an_argument_is_refused() {
        assert_eq!(StyleCondition::parse(":hover[open]"), None);
        assert_eq!(StyleCondition::parse(":[open]"), None);
        assert_eq!(StyleCondition::parse("::before[open]"), None);
    }

    #[test]
    fn an_attribute_selector_that_is_not_well_formed_is_refused() {
        for key in [
            ":is([open)",
            ":is([open",
            ":is([])",
            ":is([=true])",
            ":is([state=])",
            ":is([a=b=c])",
            ":is([a[b]])",
        ] {
            assert_eq!(StyleCondition::parse(key), None, "{key}");
        }
    }

    /// Every ARIA and `data-` state is an identifier, so a quote is a character
    /// the sheet never needs and one more to reason about in inlined text.
    #[test]
    fn a_quoted_attribute_value_is_refused() {
        assert_eq!(StyleCondition::parse(":is([data-state=\"open\"])"), None);
        assert_eq!(StyleCondition::parse(":is([data-state='open'])"), None);
    }

    #[test]
    fn an_attribute_selector_cannot_carry_a_rule_out_of_its_class() {
        for key in [
            ":is([open]){}.victim",
            ":is([open=x}])",
            ":is([open=</style>])",
            ":is([open], .victim)",
            ":is([open=a;color:red])",
        ] {
            assert_eq!(StyleCondition::parse(key), None, "{key}");
        }
    }

    /// Accepting attribute selectors widened what a key may hold only between a
    /// `[` and its `]`, so nothing that parsed before parses differently now.
    #[test]
    fn a_key_that_was_a_condition_before_still_is_one() {
        for key in [
            ":nth-child(2n+1)",
            ":not(:hover)",
            ":focus-visible",
            ":lang(en)",
            "::before",
            ":is(:hover)",
        ] {
            assert!(StyleCondition::parse(key).is_some(), "{key}");
        }
    }

    /// An announced state outranks every interaction and form state, so a
    /// pressed toggle stays pressed under the pointer.
    #[test]
    fn an_announced_state_sorts_after_the_pointer_and_form_states() {
        let state = StyleCondition::parse(":is([aria-pressed=true])").expect("a state");
        for key in [
            ":hover",
            ":focus-visible",
            ":active",
            ":disabled",
            ":read-write",
        ] {
            let other = StyleCondition::parse(key).expect("a pseudo-class");
            assert!(state.weight() > other.weight(), "{key}");
        }
        let media =
            StyleCondition::parse("@media (prefers-reduced-motion: reduce)").expect("media");
        assert!(state.weight() < media.weight());
    }

    /// And the same state under the pointer sorts after the state alone, so it
    /// can be given a colour of its own rather than a coin toss by class name.
    #[test]
    fn an_announced_state_with_an_interaction_sorts_after_the_state_alone() {
        let state = StyleCondition::parse(":is([aria-pressed=true])").expect("a state");
        let hovered = StyleCondition::parse(":is([aria-pressed=true]):hover").expect("a state");
        assert!(hovered.weight() > state.weight());

        let crowded =
            StyleCondition::parse(":is([a]):hover:focus:active:checked:enabled").expect("a state");
        assert!(
            crowded.weight() < AT_RULE_WEIGHT,
            "however many pseudo-classes a key stacks, the at-rules still wrap it"
        );
    }
}
