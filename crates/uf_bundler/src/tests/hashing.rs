//! Content hashing, and the ambiguity the length prefix removes.

use crate::hash::{ContentHasher, HASH_HEX_LEN, hash_bytes};

#[test]
fn a_hash_is_eight_hex_characters() {
    let digest = hash_bytes(b"uf");

    assert_eq!(digest.len(), HASH_HEX_LEN);
    assert!(
        digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    );
}

#[test]
fn the_same_bytes_hash_the_same_way() {
    assert_eq!(hash_bytes(b"chunk"), hash_bytes(b"chunk"));
}

#[test]
fn different_bytes_hash_differently() {
    assert_ne!(hash_bytes(b"chunk"), hash_bytes(b"chunkk"));
}

#[test]
fn empty_input_hashes() {
    assert_eq!(hash_bytes(b"").len(), HASH_HEX_LEN);
}

#[test]
fn field_boundaries_are_unambiguous() {
    let left = ContentHasher::new().with(b"ab").with(b"c").finish();
    let right = ContentHasher::new().with(b"a").with(b"bc").finish();

    assert_ne!(left, right);
}

#[test]
fn field_order_matters() {
    let left = ContentHasher::new().with(b"a").with(b"b").finish();
    let right = ContentHasher::new().with(b"b").with(b"a").finish();

    assert_ne!(left, right);
}

#[test]
fn a_field_count_changes_the_digest() {
    let left = ContentHasher::new().with(b"a").finish();
    let right = ContentHasher::new().with(b"a").with(b"").finish();

    assert_ne!(left, right);
}

#[test]
fn a_large_field_hashes_without_allocating_a_copy() {
    let payload = vec![b'x'; 1 << 20];

    assert_eq!(hash_bytes(&payload).len(), HASH_HEX_LEN);
}

#[test]
fn non_ascii_bytes_hash() {
    assert_eq!(hash_bytes("caffè ☕".as_bytes()).len(), HASH_HEX_LEN);
}

#[test]
fn a_module_symbol_is_a_valid_identifier() {
    let symbol = crate::emit::module_symbol(camino::Utf8Path::new("app/client/Counter.js"));

    assert!(crate::emit::is_identifier(&symbol), "{symbol}");
    assert!(symbol.starts_with("uf_"));
}

#[test]
fn two_module_paths_get_different_symbols() {
    let left = crate::emit::module_symbol(camino::Utf8Path::new("a.js"));
    let right = crate::emit::module_symbol(camino::Utf8Path::new("b.js"));

    assert_ne!(left, right);
}

#[test]
fn quoting_escapes_everything_a_specifier_could_carry() {
    let quoted = crate::emit::quote("a\"b\\c\nd\u{2028}e\u{1}f");

    assert_eq!(quoted, "\"a\\\"b\\\\c\\nd\\u2028e\\u0001f\"");
}

#[test]
fn an_identifier_check_rejects_what_cannot_be_written_bare() {
    assert!(crate::emit::is_identifier("valid"));
    assert!(crate::emit::is_identifier("_$valid0"));
    assert!(!crate::emit::is_identifier(""));
    assert!(!crate::emit::is_identifier("0start"));
    assert!(!crate::emit::is_identifier("has-dash"));
    assert!(!crate::emit::is_identifier("has space"));
}
