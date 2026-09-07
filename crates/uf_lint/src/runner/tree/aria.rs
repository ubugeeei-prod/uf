//! The `aria-*` attribute names that exist.
//!
//! A misspelled ARIA attribute is the quietest bug on this list. `aria-lable`
//! is not rejected by the browser, not reported by React, and not read by any
//! assistive technology: the DOM keeps the attribute and nothing ever looks at
//! it, so the control is unlabelled and the page looks finished. There is no
//! run-time symptom to notice, which is exactly why a linter has to be the one
//! to notice.
//!
//! # Where the list comes from
//!
//! [WAI-ARIA 1.2, "Definitions of States and Properties"][aria-1.2], in full,
//! plus the five names ARIA 1.3 adds that browsers already ship
//! (`aria-braillelabel`, `aria-brailleroledescription`, `aria-colindextext`,
//! `aria-description`, `aria-rowindextext`). Nothing is omitted for being
//! deprecated: `aria-dropeffect` and `aria-grabbed` do nothing in any current
//! browser, but they *are* ARIA attributes, and a rule about spelling has no
//! business also being a rule about which attributes are worth using.
//!
//! [aria-1.2]: https://www.w3.org/TR/wai-aria-1.2/#state_prop_def

/// Every `aria-*` attribute name, lowercase.
static ARIA_ATTRIBUTES: phf::Set<&'static str> = phf::phf_set! {
    "aria-activedescendant",
    "aria-atomic",
    "aria-autocomplete",
    "aria-braillelabel",
    "aria-brailleroledescription",
    "aria-busy",
    "aria-checked",
    "aria-colcount",
    "aria-colindex",
    "aria-colindextext",
    "aria-colspan",
    "aria-controls",
    "aria-current",
    "aria-describedby",
    "aria-description",
    "aria-details",
    "aria-disabled",
    "aria-dropeffect",
    "aria-errormessage",
    "aria-expanded",
    "aria-flowto",
    "aria-grabbed",
    "aria-haspopup",
    "aria-hidden",
    "aria-invalid",
    "aria-keyshortcuts",
    "aria-label",
    "aria-labelledby",
    "aria-level",
    "aria-live",
    "aria-modal",
    "aria-multiline",
    "aria-multiselectable",
    "aria-orientation",
    "aria-owns",
    "aria-placeholder",
    "aria-posinset",
    "aria-pressed",
    "aria-readonly",
    "aria-relevant",
    "aria-required",
    "aria-roledescription",
    "aria-rowcount",
    "aria-rowindex",
    "aria-rowindextext",
    "aria-rowspan",
    "aria-selected",
    "aria-setsize",
    "aria-sort",
    "aria-valuemax",
    "aria-valuemin",
    "aria-valuenow",
    "aria-valuetext",
};

/// Whether `name` is an ARIA attribute.
///
/// The comparison is case-insensitive because HTML attribute names are, and
/// React lowercases an `aria-*` prop on its way to the DOM: `aria-Label`
/// reaches the document as `aria-label` and works, so a rule that reported it
/// would be reporting working code.
pub(super) fn is_aria_attribute(name: &str) -> bool {
    if ARIA_ATTRIBUTES.contains(name) {
        return true;
    }
    name.chars().any(char::is_uppercase) && ARIA_ATTRIBUTES.contains(name.to_lowercase().as_str())
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
    let name = name.to_lowercase();
    ARIA_ATTRIBUTES
        .iter()
        .filter_map(|candidate| {
            let distance = edit_distance(&name, candidate, 2)?;
            Some((distance, *candidate))
        })
        // Ties go to the alphabetically first name, so the suggestion does not
        // depend on the iteration order of a hash set.
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

    #[test]
    fn an_attribute_name_is_case_insensitive() {
        // React lowercases it on the way to the DOM, so this one works.
        assert!(is_aria_attribute("aria-Label"));
        assert!(!is_aria_attribute("aria-Lable"));
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
}
