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
fn a_registry_that_is_not_https_is_refused() {
    for registry in [
        "file:///etc",
        "ftp://example.com",
        "registry.npmjs.org",
        "",
        // Plain http, which curl would send userinfo over.
        "http://registry.npmjs.org",
        "http://localhost:4873",
        // And a URL carrying credentials, even over TLS: they would sit in the
        // process table for the life of the request.
        "https://user:token@registry.example.com",
        "https://token@registry.example.com/path",
    ] {
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
        assert!(
            url_for("https://registry.npmjs.org", name).is_ok(),
            "{name}"
        );
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
    assert!(
        packument
            .versions
            .contains(&Version::parse("18.2.0").unwrap())
    );
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

/// A `@` in a path is not an authority, and must not be read as one.
#[test]
fn an_at_sign_after_the_host_is_a_scoped_package_rather_than_credentials() {
    assert!(url_for("https://registry.example.com/@scope", "react").is_ok());
}

#[test]
fn untrusted_text_in_an_error_is_bounded() {
    let long = "a".repeat(500);
    let Err(RegistryError::UnsafeRegistry { registry }) = url_for(&long, "react") else {
        panic!("a 500-byte registry was accepted");
    };
    assert!(registry.len() <= 64, "{} bytes", registry.len());
}

#[test]
fn a_bound_scope_routes_to_its_own_registry_and_everything_else_to_the_default() {
    let routing = RegistryRouting::new("https://registry.npmjs.org")
        .bind("@company", "https://npm.company.example");

    let bound = routing.route("@company/internal-thing");
    assert_eq!(bound.registry, "https://npm.company.example");
    assert_eq!(bound.scope, Some("@company"));
    assert!(bound.is_bound());

    for name in ["react", "@types/node", "@companyish/thing"] {
        let route = routing.route(name);
        assert_eq!(route.registry, "https://registry.npmjs.org", "{name}");
        assert_eq!(route.scope, None, "{name}");
        assert!(!route.is_bound(), "{name}");
    }
}

/// A binding written without its `@` is the same binding. A security setting
/// that silently did not apply because of a missing sigil would be worse than
/// one that was never written.
#[test]
fn a_scope_binds_with_or_without_its_at_sign() {
    let with = RegistryRouting::new("https://registry.npmjs.org")
        .bind("@company", "https://npm.company.example");
    let without = RegistryRouting::new("https://registry.npmjs.org")
        .bind("company", "https://npm.company.example");

    assert_eq!(with, without);
    assert!(without.route("@company/thing").is_bound());
}

#[test]
fn the_scope_of_a_name_is_the_part_before_the_slash_or_nothing() {
    assert_eq!(scope_of("@company/thing"), Some("@company"));
    assert_eq!(scope_of("@company/nested/thing"), Some("@company"));
    assert_eq!(scope_of("react"), None);
    // A `@` with no `/` is not a scope; npm has no such name.
    assert_eq!(scope_of("@company"), None);
    assert_eq!(scope_of(""), None);
}

#[test]
fn a_routing_reads_the_default_and_every_binding_out_of_the_config() {
    let mut config = UniflowedConfig::default();
    config.pm.registry = Some(CompactString::const_new("https://mirror.company.example"));
    config.pm.scopes.insert(
        CompactString::const_new("@company"),
        CompactString::const_new("https://npm.company.example"),
    );

    let routing = RegistryRouting::from_config(&config);
    assert_eq!(routing.default_registry(), "https://mirror.company.example");
    assert!(routing.has_bindings());
    assert_eq!(
        routing.route("@company/thing").registry,
        "https://npm.company.example"
    );
    assert_eq!(
        routing.route("react").registry,
        "https://mirror.company.example"
    );
    // And a project that binds nothing has no bindings to check.
    assert!(!RegistryRouting::from_config(&UniflowedConfig::default()).has_bindings());
}

/// The refusal that *is* the dependency-confusion defence on the read path: a
/// bound registry that answered, and answered that it has never heard of this
/// name, ends the search. There is no second request, and the error says so
/// rather than reading as a network problem somebody might retry around.
#[test]
fn a_bound_registry_that_does_not_have_the_name_is_the_answer_not_the_first_half() {
    let bound = Route {
        registry: "https://npm.company.example",
        scope: Some("@company"),
    };
    let error = unread(
        &HttpFailure::Answered("curl: (22) 404".to_owned()),
        bound,
        "@company/internal-thing",
    );

    assert!(
        matches!(error, RegistryError::NotOnBoundRegistry { .. }),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("@company/internal-thing"), "{message}");
    assert!(message.contains("@company"), "{message}");
    assert!(message.contains("https://npm.company.example"), "{message}");
    // And it says why there is no fallback, because a reader whose install has
    // just stopped will otherwise go looking for the switch that turns it on.
    assert!(message.contains("dependency-confusion"), "{message}");
}

/// A registry that never answered is not evidence that a name is not on it.
/// Turning "the company registry is down" into "that package does not exist
/// here" is how a security message stops being believed.
#[test]
fn a_bound_registry_that_could_not_be_reached_is_a_different_sentence() {
    let bound = Route {
        registry: "https://npm.company.example",
        scope: Some("@company"),
    };
    let error = unread(
        &HttpFailure::Unreachable("curl: (6) could not resolve host".to_owned()),
        bound,
        "@company/internal-thing",
    );

    assert!(matches!(error, RegistryError::Fetch { .. }), "{error:?}");
}

/// The same 404 from the default registry is an ordinary unreadable package:
/// nothing was bound, so nothing was promised about where it lives.
#[test]
fn an_unbound_name_that_is_not_published_is_just_unreadable() {
    let unbound = Route {
        registry: "https://registry.npmjs.org",
        scope: None,
    };
    let error = unread(
        &HttpFailure::Answered("curl: (22) 404".to_owned()),
        unbound,
        "no-such-package",
    );

    assert!(matches!(error, RegistryError::Fetch { .. }), "{error:?}");
}
