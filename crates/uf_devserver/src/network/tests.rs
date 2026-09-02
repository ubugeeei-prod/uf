use super::*;

fn loopback() -> NetworkPolicy {
    NetworkPolicy::loopback()
}

fn exposed(hosts: &[&str], origins: &[&str]) -> NetworkPolicy {
    NetworkPolicy::new(
        Exposure::Exposed,
        hosts.iter().copied(),
        origins.iter().copied(),
    )
    .unwrap()
}

// --- host checks ------------------------------------------------------------

#[test]
fn a_loopback_server_accepts_the_loopback_names() {
    let policy = loopback();
    for host in [
        "localhost",
        "localhost:5173",
        "127.0.0.1",
        "127.0.0.1:5173",
        "[::1]",
        "[::1]:5173",
        "LOCALHOST",
    ] {
        assert_eq!(policy.check_host(Some(host)), Ok(()), "{host} was refused");
    }
}

#[test]
fn a_loopback_server_refuses_a_rebinding_host() {
    // DNS rebinding: the socket says 127.0.0.1, the Host header says otherwise,
    // and the Host header is the one that describes what the browser thinks it
    // is talking to.
    let policy = loopback();
    for host in ["evil.test", "evil.test:5173", "attacker.example.com"] {
        assert!(
            matches!(
                policy.check_host(Some(host)).unwrap_err(),
                NetworkDenial::HostNotAllowed { .. }
            ),
            "{host} was accepted"
        );
    }
}

#[test]
fn a_missing_host_header_is_refused() {
    assert_eq!(
        loopback().check_host(None).unwrap_err(),
        NetworkDenial::MissingHost
    );
}

#[test]
fn a_malformed_host_header_is_refused_not_truncated() {
    let policy = loopback();
    for host in [
        "",
        "localhost:notaport",
        "localhost:80:80",
        "user@localhost",
        "localhost/../",
        "local host",
        "[::1",
        "[]",
        "[zz::1]",
    ] {
        assert_eq!(
            policy.check_host(Some(host)).unwrap_err(),
            NetworkDenial::MalformedHost,
            "{host:?} was not refused as malformed"
        );
    }
}

#[test]
fn a_host_over_the_byte_ceiling_is_refused() {
    let host = "a".repeat(MAX_AUTHORITY_BYTES + 1);
    assert_eq!(
        loopback().check_host(Some(&host)).unwrap_err(),
        NetworkDenial::MalformedHost
    );
}

#[test]
fn an_exposed_server_accepts_only_listed_hosts() {
    let policy = exposed(&["dev.internal"], &[]);
    assert_eq!(policy.check_host(Some("dev.internal:5173")), Ok(()));
    assert!(matches!(
        policy.check_host(Some("evil.test")).unwrap_err(),
        NetworkDenial::HostNotAllowed { .. }
    ));
}

#[test]
fn an_exposed_server_does_not_implicitly_allow_loopback() {
    // The implicit loopback names exist because a loopback socket is only
    // reachable from the machine itself. An exposed socket has no such excuse.
    let policy = exposed(&["dev.internal"], &[]);
    assert!(matches!(
        policy.check_host(Some("localhost")).unwrap_err(),
        NetworkDenial::HostNotAllowed { .. }
    ));
}

#[test]
fn a_loopback_server_also_honours_configured_hosts() {
    let policy =
        NetworkPolicy::new(Exposure::Loopback, ["dev.internal"], Vec::<&str>::new()).unwrap();
    assert_eq!(policy.check_host(Some("dev.internal")), Ok(()));
    assert_eq!(policy.check_host(Some("localhost")), Ok(()));
}

// --- origin checks ----------------------------------------------------------

#[test]
fn a_simple_get_needs_no_origin() {
    let policy = loopback();
    assert_eq!(policy.check_origin(Method::Get, None), Ok(()));
    assert_eq!(policy.check_origin(Method::Head, None), Ok(()));
    assert_eq!(
        policy.check_origin(Method::Get, Some("http://evil.test")),
        Ok(())
    );
}

#[test]
fn a_preflight_without_an_origin_is_refused() {
    assert_eq!(
        loopback().check_origin(Method::Options, None).unwrap_err(),
        NetworkDenial::MissingOrigin
    );
}

#[test]
fn a_preflight_from_an_unlisted_origin_is_refused() {
    let policy = loopback();
    assert!(matches!(
        policy
            .check_origin(Method::Options, Some("http://evil.test"))
            .unwrap_err(),
        NetworkDenial::OriginNotAllowed { .. }
    ));
}

#[test]
fn the_default_origin_allowlist_is_empty() {
    // No `*`, and no implicit entry either: by default nothing non-simple is
    // accepted from anywhere.
    assert_eq!(loopback().allowed_origins().count(), 0);
    assert!(
        loopback()
            .check_origin(Method::Options, Some("http://localhost:5173"))
            .is_err()
    );
}

#[test]
fn a_preflight_from_a_listed_origin_passes_the_origin_check() {
    let policy = exposed(&["dev.internal"], &["http://dev.internal:5173"]);
    assert_eq!(
        policy.check_origin(Method::Options, Some("http://dev.internal:5173")),
        Ok(())
    );
    assert_eq!(
        policy.check_origin(Method::Options, Some("http://dev.internal:5173/")),
        Ok(())
    );
}

#[test]
fn origin_matching_ignores_ascii_case_but_not_the_host() {
    let policy = exposed(&["dev.internal"], &["http://dev.internal:5173"]);
    assert_eq!(
        policy.check_origin(Method::Options, Some("HTTP://DEV.INTERNAL:5173")),
        Ok(())
    );
    assert!(
        policy
            .check_origin(Method::Options, Some("http://dev.internal.evil.test:5173"))
            .is_err()
    );
}

#[test]
fn a_null_origin_is_refused() {
    let policy = exposed(&["dev.internal"], &["http://dev.internal:5173"]);
    assert!(matches!(
        policy
            .check_origin(Method::Options, Some("null"))
            .unwrap_err(),
        NetworkDenial::OriginNotAllowed { .. }
    ));
}

// --- construction -----------------------------------------------------------

#[test]
fn exposing_without_an_allowed_hosts_list_is_a_startup_error() {
    assert_eq!(
        NetworkPolicy::new(Exposure::Exposed, Vec::<&str>::new(), Vec::<&str>::new()).unwrap_err(),
        NetworkPolicyError::ExposedWithoutAllowedHosts
    );
}

#[test]
fn a_wildcard_host_is_refused() {
    assert_eq!(
        NetworkPolicy::new(Exposure::Exposed, ["*"], Vec::<&str>::new()).unwrap_err(),
        NetworkPolicyError::Wildcard {
            list: "dev.allowedHosts"
        }
    );
}

#[test]
fn a_wildcard_origin_is_refused() {
    assert_eq!(
        NetworkPolicy::new(Exposure::Loopback, Vec::<&str>::new(), ["*"]).unwrap_err(),
        NetworkPolicyError::Wildcard {
            list: "dev.allowedOrigins"
        }
    );
}

#[test]
fn an_origin_without_a_scheme_is_refused_at_startup() {
    for entry in ["dev.internal", "null", "http://", "http://a/b", "://x"] {
        assert!(
            NetworkPolicy::new(Exposure::Loopback, Vec::<&str>::new(), [entry]).is_err(),
            "{entry} was accepted as an origin"
        );
    }
}

#[test]
fn a_malformed_host_entry_is_refused_at_startup() {
    for entry in ["", "  ", "host name", "http://dev.internal"] {
        assert!(
            NetworkPolicy::new(Exposure::Exposed, [entry], Vec::<&str>::new()).is_err(),
            "{entry:?} was accepted as a host"
        );
    }
}

#[test]
fn duplicate_entries_are_stored_once() {
    let policy = exposed(&["dev.internal", "dev.internal:5173"], &[]);
    assert_eq!(
        policy.allowed_hosts().collect::<Vec<_>>(),
        vec!["dev.internal"]
    );
}

#[test]
fn rejects_more_entries_than_the_ceiling() {
    let hosts: Vec<String> = (0..=MAX_ALLOWLIST_ENTRIES)
        .map(|n| format!("h{n}.test"))
        .collect();
    assert!(matches!(
        NetworkPolicy::new(Exposure::Exposed, hosts, Vec::<&str>::new()).unwrap_err(),
        NetworkPolicyError::TooManyEntries { .. }
    ));
}

#[test]
fn exposure_serializes_in_kebab_case() {
    assert_eq!(
        serde_json::to_string(&Exposure::Loopback).unwrap(),
        "\"loopback\""
    );
    assert_eq!(
        serde_json::to_string(&Exposure::Exposed).unwrap(),
        "\"exposed\""
    );
}
