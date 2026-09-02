//! Deterministic names for the classes and variables uf emits.
//!
//! # Why not a `Hasher`
//!
//! `std::collections::hash_map::DefaultHasher` documents that its output is not
//! guaranteed stable across Rust releases. A class name that changes when the
//! compiler is upgraded changes every emitted file, which breaks reproducible
//! builds and invalidates every CDN cache entry for a project that did not
//! change. So the construction below is SHA-256, pinned here, and versioned.
//!
//! # Construction
//!
//! ```text
//! frame(s) = u32::to_le_bytes(s.len()) ‖ s
//! digest   = SHA-256( frame(domain) ‖ frame(namespace) ‖ frame(property)
//!                                   ‖ frame(condition) ‖ frame(value) )
//! name     = prefix ‖ base36(u64::from_be_bytes(digest[0..8]), 13 chars)
//! ```
//!
//! * The domain string carries a version (`uf-stylex/class/v1`), so any future
//!   change to what goes into the hash is an explicit, greppable bump rather
//!   than a silent change of every class name in every project.
//! * Every component is length-framed instead of separator-joined. A separator
//!   would make `("a", "bc")` and `("ab", "c")` hash alike the moment a value
//!   contained the separator byte, and a `.stylex.js` from `node_modules` is
//!   untrusted input that can contain any byte at all.
//! * 64 bits of the digest are kept. Over a million distinct declarations —
//!   far more than a real project emits — the birthday probability of any
//!   collision is under one in thirty thousand, and a collision is detectable
//!   rather than silent because both rules stay in the sheet.
//! * Base-36 rather than hexadecimal: 13 characters carry the same 64 bits that
//!   hexadecimal needs 16 for, and shipped bytes are a product requirement.

use compact_str::CompactString;
use sha2::{Digest, Sha256};

use crate::condition::StyleCondition;

/// Domain string for a class name.
const CLASS_DOMAIN: &str = "uf-stylex/class/v1";
/// Domain string for a CSS variable name.
const VARIABLE_DOMAIN: &str = "uf-stylex/var/v1";
/// Prefix every generated class name carries.
pub const CLASS_PREFIX: &str = "x";
/// Prefix every generated CSS variable carries, custom-property dashes included.
pub const VARIABLE_PREFIX: &str = "--x";
/// How many base-36 characters a generated name carries.
pub const NAME_DIGITS: usize = 13;

/// Number of bits of the digest a generated name carries.
pub const NAME_BITS: u32 = 64;

/// The class name for one atomic declaration.
///
/// The namespace takes part in the hash, so a class name identifies exactly the
/// declaration it came from rather than being shared by any declaration that
/// happens to set the same value.
pub fn class_name(
    namespace: &str,
    property: &str,
    condition: &StyleCondition,
    value: &str,
) -> CompactString {
    let mut hasher = Sha256::new();
    for component in [CLASS_DOMAIN, namespace, property, condition.as_str(), value] {
        frame(&mut hasher, component);
    }
    encode(CLASS_PREFIX, hasher)
}

/// The custom-property name for one `stylex.defineVars` entry.
///
/// Derived from the binding the variables object is declared under and the key
/// inside it, and from nothing else — a consuming module can therefore resolve
/// `tokens.canvas` to the same variable without reading the declaring module.
pub fn variable_name(namespace: &str, key: &str) -> CompactString {
    let mut hasher = Sha256::new();
    for component in [VARIABLE_DOMAIN, namespace, key] {
        frame(&mut hasher, component);
    }
    encode(VARIABLE_PREFIX, hasher)
}

/// Feed one length-framed component into the digest.
fn frame(hasher: &mut Sha256, component: &str) {
    let length = u32::try_from(component.len()).unwrap_or(u32::MAX);
    hasher.update(length.to_le_bytes());
    hasher.update(component.as_bytes());
}

/// Finish the digest and render it as `prefix` plus [`NAME_DIGITS`] base-36 digits.
fn encode(prefix: &str, hasher: Sha256) -> CompactString {
    let digest = hasher.finalize();
    let mut leading = [0u8; 8];
    leading.copy_from_slice(&digest[..8]);
    let mut value = u64::from_be_bytes(leading);

    const ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut digits = [b'0'; NAME_DIGITS];
    let mut at = NAME_DIGITS;
    while value > 0 && at > 0 {
        at -= 1;
        digits[at] = ALPHABET[(value % 36) as usize];
        value /= 36;
    }

    let mut name = CompactString::const_new("");
    name.push_str(prefix);
    // Every byte in `digits` is ASCII, so this is UTF-8 by construction.
    name.push_str(std::str::from_utf8(&digits).unwrap_or("0000000000000"));
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_class_name_is_stable_for_the_same_declaration() {
        let first = class_name("shell", "color", &StyleCondition::Base, "red");
        let second = class_name("shell", "color", &StyleCondition::Base, "red");
        assert_eq!(first, second);
    }

    #[test]
    fn a_class_name_is_the_prefix_plus_thirteen_digits() {
        let name = class_name("shell", "color", &StyleCondition::Base, "red");
        assert_eq!(name.len(), CLASS_PREFIX.len() + NAME_DIGITS);
        assert!(name.starts_with(CLASS_PREFIX));
        assert!(
            name[CLASS_PREFIX.len()..]
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        );
    }

    #[test]
    fn the_namespace_takes_part_in_the_class_name() {
        let a = class_name("a", "color", &StyleCondition::Base, "red");
        let b = class_name("b", "color", &StyleCondition::Base, "red");
        assert_ne!(a, b);
    }

    #[test]
    fn the_condition_takes_part_in_the_class_name() {
        let base = class_name("a", "color", &StyleCondition::Base, "red");
        let hover = class_name(
            "a",
            "color",
            &StyleCondition::parse(":hover").expect("a pseudo-class"),
            "red",
        );
        assert_ne!(base, hover);
    }

    #[test]
    fn length_framing_keeps_a_shifted_boundary_from_colliding() {
        // Without length framing these two would feed the same bytes to the
        // digest, which is how a hostile `.stylex.js` would forge a class name.
        let left = class_name("ab", "c", &StyleCondition::Base, "red");
        let right = class_name("a", "bc", &StyleCondition::Base, "red");
        assert_ne!(left, right);
    }

    #[test]
    fn a_variable_name_is_a_custom_property() {
        let name = variable_name("tokens", "canvas");
        assert!(name.starts_with("--x"));
        assert_eq!(name.len(), VARIABLE_PREFIX.len() + NAME_DIGITS);
    }

    #[test]
    fn a_variable_and_a_class_over_the_same_words_differ() {
        let class = class_name("tokens", "canvas", &StyleCondition::Base, "");
        let variable = variable_name("tokens", "canvas");
        assert_ne!(&class[1..], &variable[3..]);
    }

    #[test]
    fn a_name_never_carries_a_non_ascii_byte_from_the_input() {
        let name = class_name("ns", "content", &StyleCondition::Base, "\"日本語\"");
        assert!(name.is_ascii());
    }
}
