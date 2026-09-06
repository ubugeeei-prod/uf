//! The delta is checked against `package-lock.json` documents shaped the way
//! npm writes them: a `packages` map keyed by tree path, with the root at the
//! empty string.

use super::*;
use crate::detect::YarnEdition;

/// Write a lockfile holding `entries` as `(tree path, version)` pairs.
fn write_lock(dir: &Utf8Path, entries: &[(&str, &str)]) {
    let mut packages = serde_json::Map::new();
    packages.insert(
        String::new(),
        serde_json::json!({ "name": "demo", "version": "1.0.0" }),
    );
    for (path, version) in entries {
        packages.insert(
            (*path).to_owned(),
            serde_json::json!({ "version": version, "resolved": "https://registry.npmjs.org/x" }),
        );
    }
    let document = serde_json::json!({
        "name": "demo",
        "lockfileVersion": 3,
        "packages": packages,
    });
    fs::write(
        dir.join("package-lock.json"),
        serde_json::to_string_pretty(&document).unwrap(),
    )
    .unwrap();
}

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

fn changed(delta: &LockfileDelta, kind: ChangeKind) -> Vec<(String, String, String)> {
    delta
        .changes
        .iter()
        .filter(|change| change.kind == kind)
        .map(|change| {
            (
                change.name.to_string(),
                change.before.to_string(),
                change.after.to_string(),
            )
        })
        .collect()
}

#[test]
fn a_missing_lockfile_reads_as_absent_rather_than_as_empty() {
    let (_guard, root) = temp();
    let snapshot = snapshot(&root, PackageManager::Npm);

    assert!(!snapshot.present);
    assert!(!snapshot.detailed);
    assert_eq!(snapshot.package_count(), None);
    assert!(snapshot.path.ends_with("package-lock.json"));
}

#[test]
fn a_lockfile_is_read_as_a_tree_of_packages() {
    let (_guard, root) = temp();
    write_lock(
        &root,
        &[
            ("node_modules/react", "18.3.1"),
            ("node_modules/@babel/core", "7.24.0"),
            ("node_modules/a/node_modules/ms", "2.1.3"),
        ],
    );
    let snapshot = snapshot(&root, PackageManager::Npm);

    assert!(snapshot.detailed);
    assert_eq!(snapshot.package_count(), Some(3));
    assert_eq!(
        snapshot.entries["node_modules/@babel/core"].name.as_str(),
        "@babel/core"
    );
    // A nested installation is named by the package it installs, not by the
    // package it is nested inside.
    assert_eq!(
        snapshot.entries["node_modules/a/node_modules/ms"]
            .name
            .as_str(),
        "ms"
    );
}

#[test]
fn a_first_install_reports_every_package_as_added() {
    let (_guard, root) = temp();
    let before = snapshot(&root, PackageManager::Npm);
    write_lock(&root, &[("node_modules/react", "18.3.1")]);
    let after = snapshot(&root, PackageManager::Npm);
    let delta = diff(&before, &after);

    assert!(
        delta.detailed,
        "a lockfile that did not exist is an empty tree, not an unreadable one"
    );
    assert_eq!(
        changed(&delta, ChangeKind::Added),
        [("react".to_owned(), String::new(), "18.3.1".to_owned())]
    );
    assert_eq!(delta.packages_before, None);
    assert_eq!(delta.packages_after, Some(1));
    assert!(!delta.is_unchanged());
}

#[test]
fn the_four_kinds_of_change_are_told_apart() {
    let (_guard, root) = temp();
    write_lock(
        &root,
        &[
            ("node_modules/keep", "1.0.0"),
            ("node_modules/gone", "2.0.0"),
            ("node_modules/bumped", "4.17.20"),
            ("node_modules/host/node_modules/hoisted", "3.0.0"),
        ],
    );
    let before = snapshot(&root, PackageManager::Npm);
    write_lock(
        &root,
        &[
            ("node_modules/keep", "1.0.0"),
            ("node_modules/fresh", "0.1.0"),
            ("node_modules/bumped", "4.17.21"),
            ("node_modules/hoisted", "3.0.0"),
        ],
    );
    let after = snapshot(&root, PackageManager::Npm);
    let delta = diff(&before, &after);

    assert_eq!(
        changed(&delta, ChangeKind::Added),
        [("fresh".to_owned(), String::new(), "0.1.0".to_owned())]
    );
    assert_eq!(
        changed(&delta, ChangeKind::Removed),
        [("gone".to_owned(), "2.0.0".to_owned(), String::new())]
    );
    assert_eq!(
        changed(&delta, ChangeKind::Updated),
        [(
            "bumped".to_owned(),
            "4.17.20".to_owned(),
            "4.17.21".to_owned()
        )]
    );
    // Same package, same version, a different place in `node_modules`. Nothing
    // was downloaded, and the tree still resolves differently.
    assert_eq!(
        changed(&delta, ChangeKind::Moved),
        [("hoisted".to_owned(), "3.0.0".to_owned(), "3.0.0".to_owned())]
    );
    assert_eq!(delta.count(ChangeKind::Added), 1);
    assert!(!delta.changes.iter().any(|change| change.name == "keep"));
}

#[test]
fn an_install_that_changed_nothing_says_so() {
    let (_guard, root) = temp();
    write_lock(&root, &[("node_modules/react", "18.3.1")]);
    let before = snapshot(&root, PackageManager::Npm);
    let after = snapshot(&root, PackageManager::Npm);
    let delta = diff(&before, &after);

    assert!(delta.changes.is_empty());
    assert!(delta.is_unchanged());
    assert_eq!(delta.packages_after, Some(1));
}

#[test]
fn two_nested_versions_of_one_package_are_both_reported() {
    let (_guard, root) = temp();
    write_lock(&root, &[("node_modules/ms", "2.1.3")]);
    let before = snapshot(&root, PackageManager::Npm);
    write_lock(
        &root,
        &[
            ("node_modules/ms", "2.1.3"),
            ("node_modules/debug/node_modules/ms", "2.0.0"),
        ],
    );
    let after = snapshot(&root, PackageManager::Npm);
    let delta = diff(&before, &after);

    assert_eq!(
        changed(&delta, ChangeKind::Updated),
        [(
            "ms".to_owned(),
            "2.1.3".to_owned(),
            "2.0.0, 2.1.3".to_owned()
        )]
    );
}

#[test]
fn a_lockfile_version_1_document_is_reported_by_size_and_not_guessed_at() {
    let (_guard, root) = temp();
    fs::write(
        root.join("package-lock.json"),
        r#"{"lockfileVersion":1,"dependencies":{"react":{"version":"16.0.0"}}}"#,
    )
    .unwrap();
    let snapshot = snapshot(&root, PackageManager::Npm);

    assert!(snapshot.present);
    assert!(!snapshot.detailed, "npm 6's shape is not npm 7's");
    assert!(snapshot.bytes > 0);
}

#[test]
fn a_manager_whose_lockfile_uf_does_not_parse_still_gets_a_size() {
    let (_guard, root) = temp();
    fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n").unwrap();
    let before = snapshot(&root, PackageManager::Pnpm);
    fs::write(
        root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\npackages:\n  react@18.3.1: {}\n",
    )
    .unwrap();
    let after = snapshot(&root, PackageManager::Pnpm);
    let delta = diff(&before, &after);

    assert!(before.present && !before.detailed);
    assert!(!delta.detailed);
    assert!(delta.changes.is_empty());
    assert!(
        !delta.is_unchanged(),
        "a lockfile of a different size is proof something changed"
    );
    assert!(delta.bytes_after > delta.bytes_before);
}

#[test]
fn an_untouched_unparsed_lockfile_reads_as_unchanged() {
    let (_guard, root) = temp();
    fs::write(root.join("yarn.lock"), "# yarn lockfile v1\n").unwrap();
    let manager = PackageManager::Yarn(YarnEdition::Classic);
    let delta = diff(&snapshot(&root, manager), &snapshot(&root, manager));

    assert!(delta.is_unchanged());
}

#[test]
fn a_shrinkwrap_wins_over_a_package_lock_because_npm_writes_that_one() {
    let (_guard, root) = temp();
    write_lock(&root, &[("node_modules/react", "18.3.1")]);
    fs::write(
        root.join("npm-shrinkwrap.json"),
        r#"{"lockfileVersion":3,"packages":{"":{},"node_modules/vue":{"version":"3.4.0"}}}"#,
    )
    .unwrap();
    let snapshot = snapshot(&root, PackageManager::Npm);

    assert!(snapshot.path.ends_with("npm-shrinkwrap.json"));
    assert_eq!(
        snapshot.entries["node_modules/vue"].version.as_str(),
        "3.4.0"
    );
}

#[test]
fn a_prototype_pollution_key_is_not_a_package() {
    let (_guard, root) = temp();
    fs::write(
        root.join("package-lock.json"),
        r#"{"lockfileVersion":3,"packages":{"":{},"__proto__":{"version":"9.9.9"},
            "node_modules/react":{"version":"18.3.1"}}}"#,
    )
    .unwrap();
    let snapshot = snapshot(&root, PackageManager::Npm);

    assert_eq!(snapshot.package_count(), Some(1));
    assert!(!snapshot.entries.contains_key("__proto__"));
}

#[test]
fn a_workspace_link_with_no_version_is_not_counted() {
    let (_guard, root) = temp();
    fs::write(
        root.join("package-lock.json"),
        r#"{"lockfileVersion":3,"packages":{"":{},
            "node_modules/@demo/ui":{"resolved":"packages/ui","link":true},
            "node_modules/react":{"version":"18.3.1"}}}"#,
    )
    .unwrap();
    let snapshot = snapshot(&root, PackageManager::Npm);

    assert_eq!(snapshot.package_count(), Some(1));
}

#[test]
fn the_change_marks_are_one_column_each() {
    for kind in ChangeKind::ALL {
        assert_eq!(kind.mark().chars().count(), 1, "{kind:?}");
        assert!(kind.mark().is_ascii());
        assert!(!kind.as_str().is_empty());
    }
}

#[test]
fn bun_is_found_under_either_of_the_two_names_it_writes() {
    let (_guard, root) = temp();
    // Nothing written yet: the name Bun uses today.
    assert!(
        snapshot(&root, PackageManager::Bun)
            .path
            .ends_with("bun.lock")
    );

    // A machine on an older Bun has the binary one, and uf must watch that
    // file rather than report the textual one as never written.
    fs::write(root.join("bun.lockb"), [0u8, 1, 2, 3]).unwrap();
    let found = snapshot(&root, PackageManager::Bun);
    assert!(found.path.ends_with("bun.lockb"));
    assert!(found.present);
    assert!(!found.detailed, "a binary lockfile is not read as a tree");
}
