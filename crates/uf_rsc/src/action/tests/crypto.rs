//! HMAC and constant-time comparison against published vectors.

use super::*;

#[test]
fn hmac_matches_rfc_4231_test_case_1() {
    let mac = hmac_sha256(&[0x0b; 20], b"Hi There");
    assert_eq!(
        hex(&mac),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn hmac_matches_rfc_4231_test_case_2() {
    let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(
        hex(&mac),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}

#[test]
fn hmac_matches_rfc_4231_test_case_6_with_an_oversized_key() {
    let mac = hmac_sha256(
        &[0xaa; 131],
        b"Test Using Larger Than Block-Size Key - Hash Key First",
    );
    assert_eq!(
        hex(&mac),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
}

#[test]
fn constant_time_comparison_agrees_with_equality() {
    let left = [7u8; 32];
    let mut right = [7u8; 32];
    assert_eq!(constant_time_eq(&left, &right), 1);
    right[31] = 8;
    assert_eq!(constant_time_eq(&left, &right), 0);
    right[31] = 7;
    right[0] = 8;
    assert_eq!(constant_time_eq(&left, &right), 0);
}
