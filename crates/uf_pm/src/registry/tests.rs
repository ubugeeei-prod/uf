//! The two halves that do not need a network: the URL, and the answer.

use super::*;

#[test]
fn a_scoped_name_is_one_path_segment() {
    assert_eq!(
        url_for("https://registry.npmjs.org", "@uniflowed/router").unwrap(),
        "https://registry.npmjs.org/@uniflowed%2frouter"
    );
    // A trailing slash on the configured registry is not two slashes in the URL.
    assert_eq!(
        url_for("https://registry.npmjs.org/", "react").unwrap(),
        "https://registry.npmjs.org/react"
    );
}

/// Each of these would request a URL nobody wrote.
#[test]
fn a_name_that_would_change_the_url_is_refused() {
    for name in [
        "../../etc/passwd",
        "react?x=1",
        "react#frag",
        "react%2e%2e",
        "@scope",
        "@/name",
        "@scope/",
        "React",
        ".hidden",
        "_private",
        "a b",
        "",
    ] {
        assert!(
            matches!(
                url_for("https://registry.npmjs.org", name),
                Err(RegistryError::UnsafeName { .. })
            ),
            "{name} reached curl"
        );
    }
}

#[test]
fn a_registry_that_is_not_http_is_refused() {
    for registry in ["file:///etc", "ftp://example.com", "registry.npmjs.org", ""] {
        assert!(
            matches!(
                url_for(registry, "react"),
                Err(RegistryError::UnsafeRegistry { .. })
            ),
            "{registry} reached curl"
        );
    }
}

#[test]
fn the_names_npm_actually_publishes_are_accepted() {
    for name in [
        "react",
        "left-pad",
        "@uniflowed/router",
        "@types/node",
        "lodash.merge",
        "vue3-sfc-loader",
    ] {
        assert!(url_for("https://registry.npmjs.org", name).is_ok(), "{name}");
    }
}

#[test]
fn a_packument_is_its_version_keys_and_its_latest_tag() {
    let body = br#"{
        "name": "react",
        "dist-tags": { "latest": "18.3.1", "next": "19.0.0-rc.1" },
        "versions": {
            "18.2.0": { "name": "react" },
            "18.3.1": { "name": "react" },
            "19.0.0-rc.1": { "name": "react" }
        }
    }"#;

    let packument = parse(body).expect("a packument");
    assert_eq!(packument.latest, Version::parse("18.3.1"));
    assert_eq!(packument.versions.len(), 3);
    assert!(packument.versions.contains(&Version::parse("18.2.0").unwrap()));
}

/// A registry that carries a version string from before semver was settled
/// still has a readable packument.
#[test]
fn an_unparseable_version_key_is_skipped_rather_than_fatal() {
    let body = br#"{"dist-tags":{"latest":"1.0.0"},"versions":{"0.1":{},"1.0.0":{}}}"#;

    let packument = parse(body).expect("a packument");
    assert_eq!(packument.versions, vec![Version::parse("1.0.0").unwrap()]);
}

#[test]
fn an_answer_that_is_not_a_packument_is_none() {
    for body in [
        &b"{}"[..],
        b"not json",
        b"[]",
        br#"{"error":"Not found"}"#,
        // An HTML error page a proxy returned with a 200.
        b"<!doctype html><title>502</title>",
    ] {
        assert!(parse(body).is_none(), "{}", String::from_utf8_lossy(body));
    }
}

/// A `latest` the registry never published is not a version uf offers.
#[test]
fn a_missing_latest_tag_is_none_rather_than_a_guess() {
    let body = br#"{"versions":{"1.0.0":{}}}"#;
    assert_eq!(parse(body).expect("a packument").latest, None);
}

#[test]
fn untrusted_text_in_an_error_is_bounded() {
    let long = "a".repeat(500);
    let Err(RegistryError::UnsafeRegistry { registry }) = url_for(&long, "react") else {
        panic!("a 500-byte registry was accepted");
    };
    assert!(registry.len() <= 64, "{} bytes", registry.len());
}
