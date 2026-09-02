//! CSS property names, and how broadly each one writes.
//!
//! Atomic CSS gives every rule the same specificity — one class selector — so
//! the cascade is decided by nothing but the order the rules appear in the
//! sheet. That makes [`PropertyRank`] load-bearing rather than cosmetic: if
//! `margin-top` is emitted before `margin`, then `margin` silently wins and a
//! component's layout changes depending on which file the bundler happened to
//! scan first. The rank is what pins the order down, and [`RANK_STEP`] is the
//! distance the sheet keeps between two ranks.

use compact_str::CompactString;

/// Weight given to `all`, the property that writes every other property.
const ALL_WEIGHT: u32 = 1_000;
/// Base weight for shorthands, before the width adjustment.
const SHORTHAND_WEIGHT: u32 = 2_000;
/// Weight given to a property that writes exactly one thing.
const LONGHAND_WEIGHT: u32 = 3_000;
/// Widest shorthand the ranking distinguishes.
const MAX_LONGHANDS: u32 = 32;
/// Distance between two adjacent shorthand widths.
pub const RANK_STEP: u32 = 10;

/// How broadly a property writes.
///
/// Narrower always beats broader, which is the rule the cascade would give for
/// free if these were not all one-class selectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyRank {
    /// `all`, which writes every other property.
    All,
    /// A shorthand, with how many longhands it can write.
    Shorthand {
        /// Number of longhands this shorthand expands into.
        longhands: u8,
    },
    /// A property that writes exactly one thing.
    Longhand,
}

impl PropertyRank {
    /// Ordering weight; smaller sorts earlier, so broader rules come first.
    pub const fn weight(self) -> u32 {
        match self {
            Self::All => ALL_WEIGHT,
            Self::Shorthand { longhands } => {
                let width = if longhands as u32 > MAX_LONGHANDS {
                    MAX_LONGHANDS
                } else {
                    longhands as u32
                };
                SHORTHAND_WEIGHT + (MAX_LONGHANDS - width) * RANK_STEP
            }
            Self::Longhand => LONGHAND_WEIGHT,
        }
    }

    /// The rank of a kebab-case CSS property name.
    pub fn of(property: &str) -> Self {
        if property == "all" {
            return Self::All;
        }
        match SHORTHANDS.binary_search_by_key(&property, |entry| entry.0) {
            Ok(index) => Self::Shorthand {
                longhands: SHORTHANDS[index].1,
            },
            Err(_) => Self::Longhand,
        }
    }
}

/// Whether a bare number written for this property means "pixels".
///
/// The list is the unitless set every CSS engine agrees on; anything absent
/// gets `px` appended, which is what StyleX does and what makes `padding: 8`
/// mean what an author expects.
pub fn is_unitless(property: &str) -> bool {
    UNITLESS.binary_search(&property).is_ok()
}

/// Turn an authored object key into the CSS property name it denotes.
///
/// `minHeight` becomes `min-height` and `WebkitLineClamp` becomes
/// `-webkit-line-clamp`; a key that already looks like a custom property
/// (`--brand-ink`) or a kebab-case name is passed through unchanged.
pub fn css_property_name(key: &str) -> CompactString {
    if key.starts_with("--") || !key.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return CompactString::new(key);
    }
    let mut out = String::with_capacity(key.len() + 4);
    for byte in key.bytes() {
        if byte.is_ascii_uppercase() {
            // A capital always becomes `-` plus the lowercase letter, including
            // at index 0, where it is a vendor prefix: `WebkitBoxOrient`.
            out.push('-');
            out.push(byte.to_ascii_lowercase() as char);
        } else {
            out.push(byte as char);
        }
    }
    CompactString::from(out)
}

/// Whether an authored key is a usable CSS property or namespace name.
///
/// Deliberately strict. The key ends up both in a CSS selector's declaration
/// block and in a JavaScript object literal uf generates, so anything outside
/// this set is refused rather than escaped.
pub fn is_valid_key(key: &str) -> bool {
    if key.is_empty() || key.len() > 128 {
        return false;
    }
    let bytes = key.as_bytes();
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_' || bytes[0] == b'-') {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'$'))
}

/// Object keys that would poison a prototype in the JavaScript uf generates.
pub const FORBIDDEN_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

/// Whether a key is one of [`FORBIDDEN_KEYS`].
pub fn is_forbidden_key(key: &str) -> bool {
    FORBIDDEN_KEYS.contains(&key)
}

/// Shorthands, with how many longhands each writes. Sorted for binary search.
///
/// The counts do not have to be exact CSS-spec expansions; they only have to
/// order broader shorthands before narrower ones, which is what the cascade
/// needs. Where a shorthand's width is arguable the wider reading is used, so
/// the more specific property still wins.
const SHORTHANDS: &[(&str, u8)] = &[
    ("animation", 8),
    ("animation-range", 2),
    ("background", 8),
    ("background-position", 2),
    ("border", 12),
    ("border-block", 6),
    ("border-block-color", 2),
    ("border-block-end", 3),
    ("border-block-start", 3),
    ("border-block-style", 2),
    ("border-block-width", 2),
    ("border-bottom", 3),
    ("border-color", 4),
    ("border-image", 5),
    ("border-inline", 6),
    ("border-inline-color", 2),
    ("border-inline-end", 3),
    ("border-inline-start", 3),
    ("border-inline-style", 2),
    ("border-inline-width", 2),
    ("border-left", 3),
    ("border-radius", 4),
    ("border-right", 3),
    ("border-style", 4),
    ("border-top", 3),
    ("border-width", 4),
    ("column-rule", 3),
    ("columns", 2),
    ("contain-intrinsic-size", 2),
    ("container", 2),
    ("flex", 3),
    ("flex-flow", 2),
    ("font", 7),
    ("font-synthesis", 3),
    ("font-variant", 5),
    ("gap", 2),
    ("grid", 6),
    ("grid-area", 4),
    ("grid-column", 2),
    ("grid-gap", 2),
    ("grid-row", 2),
    ("grid-template", 3),
    ("inset", 4),
    ("inset-block", 2),
    ("inset-inline", 2),
    ("list-style", 3),
    ("margin", 4),
    ("margin-block", 2),
    ("margin-inline", 2),
    ("mask", 7),
    ("mask-border", 5),
    ("offset", 5),
    ("outline", 3),
    ("overflow", 2),
    ("overscroll-behavior", 2),
    ("padding", 4),
    ("padding-block", 2),
    ("padding-inline", 2),
    ("place-content", 2),
    ("place-items", 2),
    ("place-self", 2),
    ("scroll-margin", 4),
    ("scroll-margin-block", 2),
    ("scroll-margin-inline", 2),
    ("scroll-padding", 4),
    ("scroll-padding-block", 2),
    ("scroll-padding-inline", 2),
    ("scroll-timeline", 2),
    ("text-decoration", 4),
    ("text-emphasis", 2),
    ("transition", 4),
    ("view-timeline", 2),
];

/// Properties whose bare numbers are not lengths. Sorted for binary search.
const UNITLESS: &[&str] = &[
    "animation-iteration-count",
    "aspect-ratio",
    "border-image-outset",
    "border-image-slice",
    "border-image-width",
    "box-flex",
    "box-flex-group",
    "box-ordinal-group",
    "column-count",
    "columns",
    "flex",
    "flex-grow",
    "flex-negative",
    "flex-order",
    "flex-positive",
    "flex-shrink",
    "font-weight",
    "grid-area",
    "grid-column",
    "grid-column-end",
    "grid-column-start",
    "grid-row",
    "grid-row-end",
    "grid-row-start",
    "line-clamp",
    "line-height",
    "mask-border-outset",
    "mask-border-slice",
    "mask-border-width",
    "opacity",
    "order",
    "orphans",
    "scale",
    "shape-image-threshold",
    "tab-size",
    "widows",
    "z-index",
    "zoom",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_table_is_sorted_for_binary_search() {
        assert!(SHORTHANDS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn unitless_table_is_sorted_for_binary_search() {
        assert!(UNITLESS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_longhand_outranks_the_shorthand_that_writes_it() {
        assert!(PropertyRank::of("margin-top").weight() > PropertyRank::of("margin").weight());
    }

    #[test]
    fn a_narrow_shorthand_outranks_a_wide_one() {
        assert!(
            PropertyRank::of("margin-inline").weight() > PropertyRank::of("margin").weight(),
            "margin-inline writes two longhands, margin writes four"
        );
    }

    #[test]
    fn all_sorts_before_every_other_property() {
        assert!(PropertyRank::of("all").weight() < PropertyRank::of("border").weight());
        assert!(PropertyRank::of("all").weight() < PropertyRank::of("color").weight());
    }
}
