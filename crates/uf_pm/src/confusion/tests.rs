use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use compact_str::CompactString;

use super::*;
use crate::delta::LockedEntry;

const COMPANY: &str = "https://npm.company.example";
const PUBLIC: &str = "https://registry.npmjs.org";

fn routing() -> RegistryRouting {
    RegistryRouting::new(PUBLIC).bind("@company", COMPANY)
}

fn lockfile(entries: &[(&str, &str, Option<&str>)]) -> LockfileSnapshot {
    let mut map: BTreeMap<CompactString, LockedEntry> = BTreeMap::new();
    for (name, version, resolved) in entries {
        map.insert(
            CompactString::from(format!("node_modules/{name}")),
            LockedEntry {
                name: CompactString::from(*name),
                version: CompactString::from(*version),
                resolved: resolved.map(CompactString::from),
                integrity: None,
                link: false,
            },
        );
    }
    LockfileSnapshot {
        path: Utf8PathBuf::from("/project/package-lock.json"),
        present: true,
        bytes: 1,
        entries: map,
        detailed: true,
    }
}

/// The attack, in the shape it has actually taken: a private name that the
/// public registry answered for.
#[test]
fn a_bound_scope_answered_by_the_public_registry_is_refused_and_names_both() {
    let found = check(
        &routing(),
        &lockfile(&[(
            "@company/internal-thing",
            "9.9.9",
            Some("https://registry.npmjs.org/@company/internal-thing/-/internal-thing-9.9.9.tgz"),
        )]),
    );

    assert_eq!(found.len(), 1, "{found:?}");
    let message = found[0].to_string();
    // The message has to carry all three: which package, where it should have
    // come from, and who actually answered. Two of the three is a message that
    // still needs a search.
    assert!(message.contains("@company/internal-thing"), "{message}");
    assert!(message.contains(COMPANY), "{message}");
    assert!(message.contains("https://registry.npmjs.org"), "{message}");
    assert!(message.contains("package-lock.json"), "{message}");
}

#[test]
fn the_same_name_from_its_own_registry_is_not_a_finding() {
    let found = check(
        &routing(),
        &lockfile(&[(
            "@company/internal-thing",
            "1.0.0",
            Some("https://npm.company.example/@company/internal-thing/-/internal-thing-1.0.0.tgz"),
        )]),
    );

    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_unbound_scope_and_an_unscoped_package_are_nobodys_business() {
    let found = check(
        &routing(),
        &lockfile(&[
            (
                "react",
                "18.2.0",
                Some("https://registry.npmjs.org/react/-/react-18.2.0.tgz"),
            ),
            (
                "@types/node",
                "24.0.0",
                Some("https://registry.npmjs.org/@types/node/-/node-24.0.0.tgz"),
            ),
        ]),
    );

    assert!(found.is_empty(), "{found:?}");
}

/// Stripping `resolved` must not be a way through the check.
#[test]
fn a_bound_name_with_no_resolved_url_is_refused_rather_than_assumed_fine() {
    let found = check(
        &routing(),
        &lockfile(&[("@company/internal-thing", "9.9.9", None)]),
    );

    assert_eq!(found.len(), 1, "{found:?}");
    assert!(matches!(found[0], Confusion::Unrecorded { .. }));
    assert!(
        found[0].to_string().contains("does not record"),
        "{}",
        found[0]
    );
}

#[test]
fn a_workspace_link_came_from_this_repository_and_is_not_a_registry_answer() {
    let mut snapshot = lockfile(&[("@company/internal-thing", "1.0.0", None)]);
    for entry in snapshot.entries.values_mut() {
        entry.link = true;
    }

    assert!(check(&routing(), &snapshot).is_empty());
}

/// A registry with a path is a repository, and a sibling repository whose name
/// starts with the same characters is a different one.
#[test]
fn a_path_prefixed_registry_does_not_claim_its_neighbour() {
    let routing = RegistryRouting::new(PUBLIC).bind("@company", "https://host.example/npm-local");

    let same = check(
        &routing,
        &lockfile(&[(
            "@company/thing",
            "1.0.0",
            Some("https://host.example/npm-local/@company/thing/-/thing-1.0.0.tgz"),
        )]),
    );
    assert!(same.is_empty(), "{same:?}");

    let neighbour = check(
        &routing,
        &lockfile(&[(
            "@company/thing",
            "1.0.0",
            Some("https://host.example/npm-local-public/@company/thing/-/thing-1.0.0.tgz"),
        )]),
    );
    assert_eq!(neighbour.len(), 1, "{neighbour:?}");
}

#[test]
fn a_host_is_compared_without_regard_to_its_capitalisation() {
    let found = check(
        &routing(),
        &lockfile(&[(
            "@company/thing",
            "1.0.0",
            Some("https://NPM.Company.Example/@company/thing/-/thing-1.0.0.tgz"),
        )]),
    );

    assert!(found.is_empty(), "{found:?}");
}

/// A trailing slash on the configured registry is somebody's typing, not a
/// different registry.
#[test]
fn a_trailing_slash_on_the_binding_changes_nothing() {
    let routing = RegistryRouting::new(PUBLIC).bind("@company", "https://npm.company.example/");
    let found = check(
        &routing,
        &lockfile(&[(
            "@company/thing",
            "1.0.0",
            Some("https://npm.company.example/@company/thing/-/thing-1.0.0.tgz"),
        )]),
    );

    assert!(found.is_empty(), "{found:?}");
}

/// A lockfile format uf does not read as a tree says nothing, and saying
/// nothing must not read as saying it is fine.
#[test]
fn a_lockfile_uf_did_not_parse_yields_no_findings() {
    let mut snapshot = lockfile(&[(
        "@company/thing",
        "1.0.0",
        Some("https://registry.npmjs.org/@company/thing/-/thing-1.0.0.tgz"),
    )]);
    snapshot.detailed = false;

    assert!(check(&routing(), &snapshot).is_empty());
}

#[test]
fn a_project_that_binds_nothing_is_never_checked() {
    let found = check(
        &RegistryRouting::new(PUBLIC),
        &lockfile(&[(
            "@company/thing",
            "1.0.0",
            Some("https://registry.npmjs.org/@company/thing/-/thing-1.0.0.tgz"),
        )]),
    );

    assert!(found.is_empty(), "{found:?}");
}

/// Neither side of the comparison is trusted text, and a resolved value that is
/// not a URL at all must not be read as belonging to the bound registry.
#[test]
fn a_resolved_value_that_is_not_a_url_belongs_to_no_registry() {
    for resolved in [
        "not-a-url",
        "file:///etc/passwd",
        "://registry.npmjs.org/x",
        "https:///no-authority/x",
    ] {
        let found = check(
            &routing(),
            &lockfile(&[("@company/thing", "1.0.0", Some(resolved))]),
        );
        assert_eq!(found.len(), 1, "{resolved}: {found:?}");
    }
}

/// The prefix, walked back out of. Where two repositories share a host and
/// differ only by path, `..` is the one spelling that has the bound registry's
/// prefix and is served by the repository next door — so a plain prefix test
/// would call the attacker's tarball bound.
#[test]
fn a_resolved_url_that_climbs_out_of_the_bound_path_is_not_from_that_registry() {
    let routing = RegistryRouting::new(PUBLIC).bind("@company", "https://host.example/npm-local");

    for resolved in [
        "https://host.example/npm-local/../npm-public/@company/thing/-/thing-1.0.0.tgz",
        // The same walk, spelled the way a server that normalises the path
        // still reads as a walk.
        "https://host.example/npm-local/%2E%2E/npm-public/@company/thing/-/thing-1.0.0.tgz",
    ] {
        let found = check(
            &routing,
            &lockfile(&[("@company/thing", "1.0.0", Some(resolved))]),
        );
        assert_eq!(found.len(), 1, "{resolved}: {found:?}");
    }

    // And a path with two dots inside a segment is a filename, not a walk.
    let honest = check(
        &routing,
        &lockfile(&[(
            "@company/thing",
            "1.0.0",
            Some("https://host.example/npm-local/@company/thing/-/thing-1.0.0..tgz"),
        )]),
    );
    assert!(honest.is_empty(), "{honest:?}");
}
