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
fn pseudo_class_weight(name: &str) -> u32 {
    match PSEUDO_CLASSES.binary_search_by_key(&name, |entry| entry.0) {
        Ok(index) => PSEUDO_CLASSES[index].1,
        Err(_) => UNKNOWN_PSEUDO_WEIGHT,
    }
}

/// Whether the body of a pseudo selector is made only of safe characters.
///
/// The text is emitted straight into a selector, so anything that could close
/// the rule, open a declaration block, or terminate an inline `<style>` element
/// is refused here rather than escaped later.
fn is_selector_body(body: &str) -> bool {
    !body.is_empty()
        && body.len() <= 128
        && body.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'(' | b')' | b'+' | b':')
        })
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
}
