//! What a range means, and what uf refuses to guess at.

use super::*;

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} is a version"))
}

fn range(text: &str) -> Range {
    Range::parse(text).unwrap_or_else(|| panic!("{text} is a range uf reads"))
}

#[test]
fn the_four_shapes_a_manifest_actually_contains() {
    for (text, prefix, base) in [
        ("^1.2.3", Prefix::Caret, "1.2.3"),
        ("~1.2.3", Prefix::Tilde, "1.2.3"),
        ("1.2.3", Prefix::Exact, "1.2.3"),
        ("=1.2.3", Prefix::Exact, "1.2.3"),
        (">=1.2.3", Prefix::AtLeast, "1.2.3"),
        // Whitespace around the whole thing is a manifest that was hand-edited,
        // not a compound range.
        (" ^1.2.3 ", Prefix::Caret, "1.2.3"),
        ("^2.0.0-rc.1", Prefix::Caret, "2.0.0-rc.1"),
    ] {
        let parsed = range(text);
        assert_eq!(parsed.prefix, prefix, "{text}");
        assert_eq!(parsed.base, version(base), "{text}");
    }
}

/// Each of these says something uf cannot restate in one comparator, so uf
/// says so instead of rewriting it.
#[test]
fn everything_else_is_declined_rather_than_guessed_at() {
    for text in [
        "workspace:*",
        "workspace:^",
        "catalog:",
        "catalog:default",
        "npm:@scope/other@^1.0.0",
        "file:../local",
        "link:../local",
        "git+ssh://git@github.com/o/r.git#v1",
        "https://example.com/p.tgz",
        "*",
        "",
        "latest",
        "next",
        "1.x",
        "1.2.x",
        "^1 || ^2",
        ">=1.2.3 <2.0.0",
        // A tag that looks numeric but is not a version.
        "1",
        "1.2",
    ] {
        assert!(Range::parse(text).is_none(), "{text} was rewritten");
    }
}

#[test]
fn a_caret_holds_the_major() {
    let range = range("^1.2.3");
    assert!(range.allows(&version("1.2.3")));
    assert!(range.allows(&version("1.9.0")));
    assert!(!range.allows(&version("1.2.2")), "went backwards");
    assert!(!range.allows(&version("2.0.0")));
}

/// The rule people are surprised by, and the reason `uf update` on a `0.x`
/// project reports more than they expect.
#[test]
fn below_one_a_caret_holds_the_left_most_non_zero() {
    assert!(range("^0.2.3").allows(&version("0.2.9")));
    assert!(!range("^0.2.3").allows(&version("0.3.0")));
    assert!(range("^0.0.3").allows(&version("0.0.3")));
    assert!(!range("^0.0.3").allows(&version("0.0.4")));
}

#[test]
fn a_tilde_holds_the_minor_and_an_exact_holds_everything() {
    assert!(range("~1.2.3").allows(&version("1.2.9")));
    assert!(!range("~1.2.3").allows(&version("1.3.0")));
    assert!(range("1.2.3").allows(&version("1.2.3")));
    assert!(!range("1.2.3").allows(&version("1.2.4")));
    assert!(range(">=1.2.3").allows(&version("9.9.9")));
}

/// Without this rule every `^1` in the world matches the next major's first
/// release candidate.
#[test]
fn a_prerelease_does_not_satisfy_a_range_that_is_not_one() {
    assert!(!range("^1.2.3").allows(&version("1.3.0-rc.1")));
    assert!(!range(">=1.2.3").allows(&version("2.0.0-rc.1")));
    // Same `major.minor.patch` as the base, which is the one case npm allows.
    assert!(range("^2.0.0-rc.1").allows(&version("2.0.0-rc.2")));
    assert!(range("^2.0.0-rc.1").allows(&version("2.0.0")));
}

#[test]
fn a_rewrite_keeps_the_comparator_the_project_chose() {
    let to = version("2.0.0");
    assert_eq!(range("^1.2.3").rewritten_to(&to), "^2.0.0");
    assert_eq!(range("~1.2.3").rewritten_to(&to), "~2.0.0");
    assert_eq!(range("1.2.3").rewritten_to(&to), "2.0.0");
    assert_eq!(range(">=1.2.3").rewritten_to(&to), ">=2.0.0");
    // `=1.2.3` is the same statement as `1.2.3`, and comes back in the shorter
    // spelling npm writes.
    assert_eq!(range("=1.2.3").rewritten_to(&to), "2.0.0");
}

#[test]
fn the_step_is_the_component_that_changed() {
    assert_eq!(
        step(&version("1.2.3"), &version("1.2.4")),
        Some(Level::Patch)
    );
    assert_eq!(
        step(&version("1.2.3"), &version("1.3.0")),
        Some(Level::Minor)
    );
    assert_eq!(
        step(&version("1.2.3"), &version("2.0.0")),
        Some(Level::Major)
    );
    assert_eq!(step(&version("1.2.3"), &version("1.2.3")), None);
    assert_eq!(step(&version("1.2.3"), &version("1.2.2")), None);
    // A release is a step up from its own candidate.
    assert_eq!(
        step(&version("2.0.0-rc.1"), &version("2.0.0")),
        Some(Level::Patch)
    );
}

#[test]
fn best_is_capped_by_the_level_that_was_asked_for() {
    let published = ["1.2.3", "1.2.9", "1.4.0", "2.0.0", "3.1.0"]
        .map(version)
        .to_vec();
    let from = version("1.2.3");

    assert_eq!(
        best(&published, &from, Level::Patch),
        Some(&version("1.2.9"))
    );
    assert_eq!(
        best(&published, &from, Level::Minor),
        Some(&version("1.4.0"))
    );
    assert_eq!(
        best(&published, &from, Level::Major),
        Some(&version("3.1.0"))
    );
}

#[test]
fn nothing_newer_is_none_rather_than_the_version_it_is_already_on() {
    let published = ["1.0.0", "1.2.3"].map(version).to_vec();
    assert_eq!(best(&published, &version("1.2.3"), Level::Major), None);
}

/// `--latest` on a stable project must not propose a release candidate; on a
/// project already on one it must.
#[test]
fn a_prerelease_is_only_offered_to_a_project_already_on_one() {
    let published = ["1.2.3", "2.0.0-rc.1"].map(version).to_vec();
    assert_eq!(best(&published, &version("1.2.3"), Level::Major), None);

    let published = ["2.0.0-rc.1", "2.0.0-rc.2"].map(version).to_vec();
    assert_eq!(
        best(&published, &version("2.0.0-rc.1"), Level::Major),
        Some(&version("2.0.0-rc.2"))
    );
}

/// A plain string compare puts `alpha.10` before `alpha.9`, which would offer
/// an older prerelease as an upgrade.
#[test]
fn prerelease_identifiers_compare_numerically() {
    assert!(version("1.0.0-alpha.9") < version("1.0.0-alpha.10"));
    assert!(version("1.0.0-alpha") < version("1.0.0-alpha.1"));
    assert!(version("1.0.0-alpha.1") < version("1.0.0-beta"));
    assert!(version("1.0.0-rc.1") < version("1.0.0"));
    assert!(version("1.9.0") < version("1.10.0"));
}

/// Build metadata takes no part in precedence, so it is dropped on the way in
/// rather than making two spellings of one version.
#[test]
fn build_metadata_is_dropped() {
    assert_eq!(version("1.2.3+build.5"), version("1.2.3"));
}
