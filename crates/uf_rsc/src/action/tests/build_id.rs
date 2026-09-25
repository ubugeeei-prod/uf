//! Build-id validation, generation and redaction.

use super::*;

#[test]
fn a_short_build_id_is_rejected() {
    assert_eq!(
        BuildId::new("short"),
        Err(BuildIdError::TooShort { len: 5 })
    );
}

#[test]
fn an_empty_build_id_is_rejected() {
    assert_eq!(BuildId::new(""), Err(BuildIdError::TooShort { len: 0 }));
}

#[test]
fn an_oversized_build_id_is_rejected() {
    let value = "x".repeat(MAX_BUILD_ID_BYTES + 1);
    assert_eq!(
        BuildId::new(value),
        Err(BuildIdError::TooLong {
            len: MAX_BUILD_ID_BYTES + 1
        })
    );
}

#[test]
fn a_generated_build_id_is_long_and_unique() {
    let first = BuildId::generate();
    let second = BuildId::generate();
    assert!(first.value.len() >= MIN_BUILD_ID_BYTES);
    assert_ne!(first, second);
}

#[test]
fn an_unset_or_out_of_range_build_id_is_generated() {
    let supplied = BuildId::from_supplied_or_generate(Some("valid-build-id".to_owned()));
    assert_eq!(supplied, BuildId::new("valid-build-id").unwrap());

    for candidate in [
        None,
        Some("short".to_owned()),
        Some("x".repeat(MAX_BUILD_ID_BYTES + 1)),
    ] {
        let first = BuildId::from_supplied_or_generate(candidate.clone());
        let second = BuildId::from_supplied_or_generate(candidate);
        assert_eq!(first.value.len(), 64);
        assert_ne!(first, second);
    }
}

#[test]
fn a_build_id_never_prints_itself() {
    let formatted = format!("{:?}", build_id());
    assert_eq!(formatted, "BuildId(<redacted>)");
    assert!(!formatted.contains("build-id-for-tests"));
}
