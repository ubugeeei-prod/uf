//! What the four screens look like, and what the manifest reader reads.
//!
//! Rendered here rather than driven through npm, the way `install/tests.rs`
//! does it and for the same reason: a test that runs a package manager asserts
//! on whatever that manager resolved today. What is pinned below is the layout
//! and the arithmetic. `crates/uf_cli/tests/dependencies.rs` is the other half,
//! and it does run npm, over a fixture no registry is needed for.

use super::*;
use std::fs;
use uf_pm::delta::{ChangeKind, PackageChange};
use uf_term::Capabilities;

/// A renderer with nothing to draw with: no colour, ASCII glyphs, nobody
/// watching. The same capability a redirected stream gets.
fn plain() -> Renderer {
    Renderer::new(Capabilities::plain())
}

fn added(field: &'static str, name: &str, range: &str) -> ManifestChange {
    ManifestChange {
        field,
        name: name.to_owned(),
        range: range.to_owned(),
        kind: ManifestChangeKind::Added,
    }
}

fn unchanged_tree() -> LockfileDelta {
    LockfileDelta {
        detailed: true,
        ..LockfileDelta::default()
    }
}

fn changed_tree() -> LockfileDelta {
    LockfileDelta {
        changes: vec![PackageChange {
            name: "date-fns".into(),
            kind: ChangeKind::Added,
            before: "".into(),
            after: "4.1.0".into(),
        }],
        detailed: true,
        ..LockfileDelta::default()
    }
}

fn report(heading: &'static str, manifest: Vec<ManifestChange>, tree: LockfileDelta) -> DepsReport {
    DepsReport {
        heading,
        continued: false,
        manager: "npm".to_owned(),
        chosen_by: "package-lock.json".to_owned(),
        command: "npm install --ignore-scripts date-fns".to_owned(),
        lockfile: "package-lock.json · 17 packages · 12.40 kB".to_owned(),
        manifest,
        tree,
        elapsed: Duration::from_millis(2100),
    }
}

/// The first `uf add`: both files moved, and the screen says which fields.
#[test]
fn an_add_that_changed_something_names_the_field_and_the_tree() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report(
            "uf add",
            vec![added("dependencies", "date-fns", "^4.1.0")],
            changed_tree(),
        ),
    );

    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "  manager    npm",
            "  chosen by  package-lock.json",
            "  command    npm install --ignore-scripts date-fns",
            "  lockfile   package-lock.json · 17 packages · 12.40 kB",
            "",
            "  manifest",
            "",
            "       field         package   range",
            "    +  dependencies  date-fns  ^4.1.0",
            "",
            "  dependency tree",
            "    added  1",
            "",
            "       package   version",
            "    +  date-fns  4.1.0",
            "",
            "+ 1 package recorded in dependencies in 2.1s",
        ]
    );
    assert!(out.ends_with('\n'));
}

/// The second `uf add` of the same thing, which is the run that has to stay
/// cheap to read.
#[test]
fn an_add_that_changed_nothing_says_so_in_one_line() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report("uf add", Vec::new(), unchanged_tree()),
    );

    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "  manager    npm",
            "  chosen by  package-lock.json",
            "  command    npm install --ignore-scripts date-fns",
            "  lockfile   package-lock.json · 17 packages · 12.40 kB",
            "",
            "+ already up to date in 2.1s",
        ]
    );
    assert!(
        !out.contains("manifest") && !out.contains("dependency tree"),
        "a report about nothing happening is noise:\n{out}"
    );
}

/// A range that was already satisfied, resolved to something newer: the
/// manifest did not move and the tree did, and both halves are said.
#[test]
fn a_tree_that_moved_without_the_manifest_is_not_called_an_add() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report("uf add", Vec::new(), changed_tree()),
    );

    assert!(
        out.contains("+ the manifest already said so; 1 change in the tree"),
        "{out}"
    );
}

/// Each command's own verb, so the last line is not the same sentence four
/// times.
#[test]
fn each_command_reports_in_its_own_words() {
    for (heading, expected) in [
        ("uf add", "1 package recorded in dependencies"),
        ("uf remove", "1 package taken out of dependencies"),
        ("uf update", "1 package re-ranged in dependencies"),
    ] {
        let mut out = String::new();
        render_summary(
            &plain(),
            &mut out,
            &report(
                heading,
                vec![added("dependencies", "date-fns", "^4.1.0")],
                unchanged_tree(),
            ),
        );
        assert!(out.contains(expected), "{heading}:\n{out}");
    }
}

/// A lockfile uf does not parse gets no tree section rather than an empty one,
/// the way `uf install` does it.
#[test]
fn a_lockfile_uf_cannot_read_gets_no_empty_tree_section() {
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report(
            "uf add",
            vec![added("dependencies", "date-fns", "^4.1.0")],
            LockfileDelta::default(),
        ),
    );

    assert!(out.contains("manifest"), "{out}");
    assert!(!out.contains("dependency tree"), "{out}");
}

/// Nothing drawn here needs a terminal.
#[test]
fn nothing_the_summary_draws_needs_an_escape_sequence() {
    for (manifest, tree) in [
        (Vec::new(), unchanged_tree()),
        (
            vec![added("peerDependencies", "react", "^19")],
            changed_tree(),
        ),
    ] {
        let mut out = String::new();
        render_summary(&plain(), &mut out, &report("uf add", manifest, tree));
        assert!(!out.contains('\u{1b}'), "{out}");
    }
}

/// A long list is cut, and says how much it cut.
#[test]
fn a_long_manifest_list_is_cut_and_says_how_much_it_cut() {
    let manifest: Vec<ManifestChange> = (0..MANIFEST_CHANGES_SHOWN + 4)
        .map(|index| added("dependencies", &format!("package-{index}"), "^1.0.0"))
        .collect();
    let mut out = String::new();
    render_summary(
        &plain(),
        &mut out,
        &report("uf add", manifest, unchanged_tree()),
    );

    assert!(out.contains("and 4 more"), "{out}");
    assert!(out.contains("package-0"), "{out}");
    assert!(!out.contains("package-16"), "{out}");
}

// --- the manifest reader -------------------------------------------------

fn manifest(source: &str) -> BTreeMap<(&'static str, String), String> {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path =
        camino::Utf8PathBuf::from_path_buf(dir.path().join("package.json")).expect("a UTF-8 path");
    fs::write(&path, source).expect("a manifest");
    dependency_entries(&path)
}

/// All four fields are read, and nothing else is.
#[test]
fn every_dependency_field_is_read_and_only_those() {
    let entries = manifest(
        r#"{
          "name": "app",
          "dependencies": { "react": "^19.0.0" },
          "devDependencies": { "eslint": "^9" },
          "optionalDependencies": { "fsevents": "^2" },
          "peerDependencies": { "react-dom": "^19" },
          "bundledDependencies": ["nope"],
          "scripts": { "build": "vite build" }
        }"#,
    );

    assert_eq!(entries.len(), 4);
    assert_eq!(entries[&("dependencies", "react".to_owned())], "^19.0.0");
    assert_eq!(entries[&("devDependencies", "eslint".to_owned())], "^9");
    assert_eq!(
        entries[&("optionalDependencies", "fsevents".to_owned())],
        "^2"
    );
    assert_eq!(
        entries[&("peerDependencies", "react-dom".to_owned())],
        "^19"
    );
}

/// A manifest is repository content. It gets read before anything validates it,
/// so the pollution keys go the way they go everywhere else uf walks JSON.
#[test]
fn prototype_pollution_keys_are_never_dependency_names() {
    let entries = manifest(
        r#"{"dependencies": {"__proto__": "1", "constructor": "1", "prototype": "1", "ok": "^1"}}"#,
    );

    assert_eq!(entries.len(), 1);
    assert!(entries.contains_key(&("dependencies", "ok".to_owned())));
}

/// A manifest uf cannot read is no entries, not an error: the manager is what
/// gets to refuse a manifest, and it says so far better than this could.
#[test]
fn a_manifest_that_is_not_readable_json_is_no_entries() {
    assert!(manifest("this is not json").is_empty());
    assert!(manifest("[]").is_empty());
    assert!(manifest(r#"{"dependencies": ["react"]}"#).is_empty());
    assert!(manifest(r#"{"dependencies": {"react": 19}}"#).is_empty());
    assert!(dependency_entries(camino::Utf8Path::new("/nowhere/package.json")).is_empty());
}

/// The three ways an entry can differ, and the one way it cannot.
#[test]
fn a_manifest_diff_reports_arrivals_departures_and_new_ranges() {
    let before = manifest(
        r#"{"dependencies": {"react": "^18", "lodash": "^4"},
            "devDependencies": {"eslint": "^9"}}"#,
    );
    let after = manifest(
        r#"{"dependencies": {"react": "^19", "date-fns": "^4"},
            "devDependencies": {"eslint": "^9"}}"#,
    );

    let mut changes = manifest_changes(&before, &after);
    changes.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(
        changes,
        [
            added("dependencies", "date-fns", "^4"),
            ManifestChange {
                field: "dependencies",
                name: "lodash".to_owned(),
                range: "^4".to_owned(),
                kind: ManifestChangeKind::Removed,
            },
            ManifestChange {
                field: "dependencies",
                name: "react".to_owned(),
                range: "^19".to_owned(),
                kind: ManifestChangeKind::Reranged,
            },
        ],
        "eslint did not move, so it is not a change"
    );
}

/// The same package in two fields is two entries: `uf add --peer react` in a
/// project that already had `react` in `dependencies` really did add one.
#[test]
fn the_same_name_in_two_fields_is_two_entries() {
    let before = manifest(r#"{"dependencies": {"react": "^19"}}"#);
    let after =
        manifest(r#"{"dependencies": {"react": "^19"}, "peerDependencies": {"react": "^19"}}"#);

    assert_eq!(
        manifest_changes(&before, &after),
        [added("peerDependencies", "react", "^19")]
    );
}

/// The retry line is the command as it was typed, so it can be pasted back.
#[test]
fn the_retry_line_is_the_command_that_was_typed() {
    assert_eq!(
        retry_line("uf add", &["react".to_owned(), "react-dom@^19".to_owned()]),
        "uf add react react-dom@^19"
    );
    assert_eq!(retry_line("uf update", &[]), "uf update");
}

/// `uf update --latest` rewrites the manifests itself and says so; the install
/// that follows must not answer that with "the manifest already said so".
#[test]
fn a_continued_command_does_not_deny_the_line_above_it() {
    let mut report = report("uf update", Vec::new(), changed_tree());
    assert!(
        headline(&report).contains("the manifest already said so"),
        "{}",
        headline(&report)
    );

    report.continued = true;
    assert_eq!(headline(&report), "1 change in the tree");
}
