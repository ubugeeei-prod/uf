//! Reading every declaration, and changing one without restyling the file.

use super::*;

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
    for (path, contents) in files {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        fs::write(&file, contents).expect("a file");
    }
    dir
}

fn root_of(dir: &tempfile::TempDir) -> &Utf8Path {
    Utf8Path::from_path(dir.path()).expect("a UTF-8 path")
}

fn change(field: &'static str, name: &str, range: &str) -> Changes {
    let mut changes = Changes::new();
    changes.insert((field, name.to_compact_string()), range.to_compact_string());
    changes
}

const MANIFEST: &str = r#"{
  "name": "app",
  "dependencies": {
    "react": "^18.2.0",
    "left-pad": "1.3.0"
  },
  "devDependencies": {
    "vitest": "~1.0.0"
  }
}
"#;

#[test]
fn every_field_of_every_manifest_is_read() {
    let dir = project(&[
        ("package.json", MANIFEST),
        (
            "packages/ui/package.json",
            r#"{"name":"ui","peerDependencies":{"react":">=18.0.0"},"optionalDependencies":{"fsevents":"^2.3.0"}}"#,
        ),
    ]);

    let found = declarations(root_of(&dir)).expect("declarations");

    let mut seen: Vec<_> = found
        .iter()
        .map(|d| (d.field, d.name.as_str(), d.range.as_str()))
        .collect();
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![
            ("dependencies", "left-pad", "1.3.0"),
            ("dependencies", "react", "^18.2.0"),
            ("devDependencies", "vitest", "~1.0.0"),
            ("optionalDependencies", "fsevents", "^2.3.0"),
            ("peerDependencies", "react", ">=18.0.0"),
        ]
    );
}

/// A monorepo that declares one package six times has six places to change.
#[test]
fn one_package_declared_twice_is_two_declarations() {
    let dir = project(&[
        ("package.json", r#"{"dependencies":{"react":"^18.0.0"}}"#),
        (
            "packages/a/package.json",
            r#"{"dependencies":{"react":"^17.0.0"}}"#,
        ),
    ]);

    let found = declarations(root_of(&dir)).expect("declarations");

    assert_eq!(found.len(), 2, "{found:?}");
    assert_ne!(found[0].manifest, found[1].manifest);
}

#[test]
fn node_modules_is_not_the_projects_own_manifests() {
    let dir = project(&[
        ("package.json", r#"{"dependencies":{"react":"^18.0.0"}}"#),
        (
            "node_modules/react/package.json",
            r#"{"dependencies":{"loose-envify":"^1.1.0"}}"#,
        ),
    ]);

    let found = declarations(root_of(&dir)).expect("declarations");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "react");
}

#[test]
fn a_prototype_key_is_not_a_dependency() {
    let dir = project(&[(
        "package.json",
        r#"{"dependencies":{"__proto__":"^1.0.0","react":"^18.0.0"}}"#,
    )]);

    let found = declarations(root_of(&dir)).expect("declarations");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "react");
}

/// The whole point of the splice: one character changed, one line in the diff.
#[test]
fn a_rewrite_touches_only_the_range_it_changed() {
    let dir = project(&[("package.json", MANIFEST)]);
    let manifest = root_of(&dir).join("package.json");

    let written = apply(&manifest, &change("dependencies", "react", "^19.0.0")).expect("applied");

    assert_eq!(written, 1);
    assert_eq!(
        fs::read_to_string(&manifest).expect("the manifest"),
        MANIFEST.replace("^18.2.0", "^19.0.0"),
        "something other than the range moved"
    );
}

#[test]
fn indentation_and_the_trailing_newline_survive() {
    for (name, source) in [
        (
            "four spaces",
            "{\n    \"dependencies\": {\n        \"react\": \"^18.2.0\"\n    }\n}\n",
        ),
        (
            "tabs",
            "{\n\t\"dependencies\": {\n\t\t\"react\": \"^18.2.0\"\n\t}\n}\n",
        ),
        (
            "no trailing newline",
            "{\n  \"dependencies\": {\n    \"react\": \"^18.2.0\"\n  }\n}",
        ),
    ] {
        let dir = project(&[("package.json", source)]);
        let manifest = root_of(&dir).join("package.json");

        apply(&manifest, &change("dependencies", "react", "^19.0.0")).expect("applied");

        assert_eq!(
            fs::read_to_string(&manifest).expect("the manifest"),
            source.replace("^18.2.0", "^19.0.0"),
            "{name}"
        );
    }
}

/// The same name and the same range in two fields cannot be spliced without
/// guessing which one was meant, so the whole file is written instead — and
/// only the field that was asked for changes.
#[test]
fn an_ambiguous_pair_falls_back_to_rewriting_the_file() {
    let source = r#"{
  "dependencies": { "react": "^18.2.0" },
  "peerDependencies": { "react": "^18.2.0" }
}
"#;
    let dir = project(&[("package.json", source)]);
    let manifest = root_of(&dir).join("package.json");

    apply(&manifest, &change("dependencies", "react", "^19.0.0")).expect("applied");

    let after: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("the manifest")).expect("json");
    assert_eq!(after["dependencies"]["react"], "^19.0.0");
    assert_eq!(after["peerDependencies"]["react"], "^18.2.0", "both moved");
}

/// A manifest on one line has no `"name": "range"` pair with the spacing the
/// splice looks for in only one place, and still comes back correct.
#[test]
fn a_single_line_manifest_is_rewritten_correctly() {
    let dir = project(&[(
        "package.json",
        r#"{"name":"app","dependencies":{"react":"^18.2.0","left-pad":"1.3.0"}}"#,
    )]);
    let manifest = root_of(&dir).join("package.json");

    apply(&manifest, &change("dependencies", "react", "^19.0.0")).expect("applied");

    let after: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("the manifest")).expect("json");
    assert_eq!(after["dependencies"]["react"], "^19.0.0");
    assert_eq!(after["dependencies"]["left-pad"], "1.3.0");
    assert_eq!(after["name"], "app");
}

#[test]
fn a_range_that_is_already_what_it_would_be_set_to_writes_nothing() {
    let dir = project(&[("package.json", MANIFEST)]);
    let manifest = root_of(&dir).join("package.json");

    let written = apply(&manifest, &change("dependencies", "react", "^18.2.0")).expect("applied");

    assert_eq!(written, 0);
    assert_eq!(
        fs::read_to_string(&manifest).expect("the manifest"),
        MANIFEST
    );
}

#[test]
fn a_package_the_manifest_does_not_declare_is_not_added() {
    let dir = project(&[("package.json", MANIFEST)]);
    let manifest = root_of(&dir).join("package.json");

    let written = apply(&manifest, &change("dependencies", "vue", "^3.0.0")).expect("applied");

    assert_eq!(written, 0);
    assert_eq!(
        fs::read_to_string(&manifest).expect("the manifest"),
        MANIFEST
    );
}

/// Several at once, which is what `uf update --latest` does.
#[test]
fn every_change_lands_in_one_write() {
    let dir = project(&[("package.json", MANIFEST)]);
    let manifest = root_of(&dir).join("package.json");
    let mut changes = Changes::new();
    changes.insert(
        ("dependencies", "react".to_compact_string()),
        "^19.0.0".to_compact_string(),
    );
    changes.insert(
        ("dependencies", "left-pad".to_compact_string()),
        "1.3.1".to_compact_string(),
    );
    changes.insert(
        ("devDependencies", "vitest".to_compact_string()),
        "~2.0.0".to_compact_string(),
    );

    let written = apply(&manifest, &changes).expect("applied");

    assert_eq!(written, 3);
    assert_eq!(
        fs::read_to_string(&manifest).expect("the manifest"),
        MANIFEST
            .replace("^18.2.0", "^19.0.0")
            .replace("\"1.3.0\"", "\"1.3.1\"")
            .replace("~1.0.0", "~2.0.0")
    );
}

/// The failure ubugeeei-prod/uf#541's review found: a workspace half rewritten,
/// followed by an install of it.
#[test]
fn a_workspace_rewrite_that_fails_part_way_leaves_every_manifest_alone() {
    let first = "{\n  \"dependencies\": { \"react\": \"^18.2.0\" }\n}\n";
    let dir = project(&[
        ("package.json", first),
        // A directory where the second manifest should be, so writing it fails
        // after the first has already been written.
        ("packages/ui/package.json/keep", "not a manifest\n"),
    ]);
    let root = root_of(&dir);
    let mut all = BTreeMap::new();
    all.insert(
        root.join("package.json"),
        change("dependencies", "react", "^19.0.0"),
    );
    all.insert(
        root.join("packages/ui/package.json"),
        change("dependencies", "react", "^19.0.0"),
    );

    let error = apply_all(&all).expect_err("the second manifest cannot be read");

    assert!(
        matches!(error, PackageManagerError::Read { .. }),
        "{error:?}"
    );
    assert_eq!(
        fs::read_to_string(root.join("package.json")).expect("the first manifest"),
        first,
        "the first manifest kept a range the second could not be given"
    );
}

#[test]
fn a_workspace_rewrite_that_succeeds_counts_every_range() {
    let dir = project(&[
        ("package.json", r#"{"dependencies":{"react":"^18.2.0"}}"#),
        (
            "packages/ui/package.json",
            r#"{"dependencies":{"react":"^17.0.0"}}"#,
        ),
    ]);
    let root = root_of(&dir);
    let mut all = BTreeMap::new();
    for manifest in ["package.json", "packages/ui/package.json"] {
        all.insert(
            root.join(manifest),
            change("dependencies", "react", "^19.0.0"),
        );
    }

    assert_eq!(apply_all(&all).expect("applied"), 2);
    for manifest in ["package.json", "packages/ui/package.json"] {
        let after: Value =
            serde_json::from_str(&fs::read_to_string(root.join(manifest)).expect("read"))
                .expect("json");
        assert_eq!(after["dependencies"]["react"], "^19.0.0", "{manifest}");
    }
}
