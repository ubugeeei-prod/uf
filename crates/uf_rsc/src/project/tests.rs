use super::*;
use std::fs;

fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 root");
    for (path, contents) in files {
        let target = root.join(path);
        fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
        fs::write(&target, contents).expect("write");
    }
    (dir, root)
}

fn build_id() -> BuildId {
    BuildId::new("project-test-build-id").expect("valid build id")
}

#[test]
fn a_missing_root_analyses_to_nothing() {
    let analysis = analyze_project(
        Utf8Path::new("/nonexistent/uf/project"),
        &build_id(),
        &ProjectScanOptions::default(),
    )
    .unwrap();
    assert!(analysis.graph.modules().is_empty());
}

#[test]
fn a_scaffold_shaped_project_resolves_its_boundary_and_action() {
    let (_dir, root) = project(&[
        (
            "app/$page.js",
            "// @flow\nimport Counter from \"./client/Counter.js\";\nimport { refreshGreeting } from \"../server/actions.js\";\n",
        ),
        (
            "app/client/Counter.js",
            "\"use client\";\n// @flow\nimport { useCounter } from \"./useCounter.js\";\n",
        ),
        (
            "app/client/useCounter.js",
            "\"use client\";\n// @flow\nimport { useState } from \"@uniflowed/react\";\n",
        ),
        (
            "server/actions.js",
            "\"use server\";\n// @flow\nexport const refreshGreeting = serverAction(async () => {});\n",
        ),
    ]);

    let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();

    assert_eq!(analysis.graph.modules().len(), 4);
    assert_eq!(analysis.graph.client_boundaries().len(), 1);
    assert_eq!(analysis.client_bundle_root_count(), 1);
    assert_eq!(analysis.callable_action_count(), 1);
    assert!(analysis.graph.diagnostics().is_empty());
}

#[test]
fn ignored_directories_are_not_scanned() {
    let (_dir, root) = project(&[
        ("app/$page.js", "// @flow\n"),
        ("node_modules/pkg/index.js", "\"use client\";\n"),
        ("dist/bundle.js", "\"use client\";\n"),
    ]);
    let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();
    assert_eq!(analysis.graph.modules().len(), 1);
}

/// A checkout inside the project is not part of the project.
///
/// A submodule, a vendored clone, a git worktree: all of them are directories
/// with a `.git` in them, and none of them holds this project's modules. This
/// repository keeps a gigabyte of third-party Flow under `tests/fixtures/git`
/// and `uf prepare` analysed all of it, found `"use server"` in React's
/// sources, and wrote a server action table about them.
#[test]
fn a_nested_repository_is_not_part_of_the_project() {
    let (_dir, root) = project(&[
        ("app/$page.js", "// @flow\n"),
        (
            "vendor/upstream/actions.js",
            "\"use server\";\nexport async function leak() {}\n",
        ),
        (
            "vendor/upstream/.git",
            "gitdir: ../../.git/modules/upstream\n",
        ),
    ]);

    let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();

    assert_eq!(analysis.graph.modules().len(), 1);
    assert!(analysis.registry.is_empty());
}

#[test]
fn test_files_are_not_part_of_the_app_graph() {
    let (_dir, root) = project(&[
        ("app/$page.js", "// @flow\n"),
        ("app/$page.test.js", "// @flow\n"),
    ]);
    let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();
    assert_eq!(analysis.graph.modules().len(), 1);
}

#[test]
fn oversized_modules_are_skipped_rather_than_read() {
    let (_dir, root) = project(&[("app/$page.js", "// @flow\nconst a = 1;\n")]);
    let options = ProjectScanOptions {
        max_file_bytes: 4,
        ..ProjectScanOptions::default()
    };
    let analysis = analyze_project(&root, &build_id(), &options).unwrap();
    assert!(analysis.graph.modules().is_empty());
}

#[test]
fn router_files_become_server_entries() {
    let (_dir, root) = project(&[
        ("app/$page.js", "import \"./data.js\";\n"),
        ("app/data.js", "// @flow\n"),
    ]);
    let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();
    assert!(
        analysis
            .graph
            .module("app/data.js")
            .unwrap()
            .reachability
            .is_reachable()
    );
}

#[test]
fn extra_entries_are_honoured() {
    let (_dir, root) = project(&[("app/entry.js", "\"use client\";\n")]);
    let options = ProjectScanOptions {
        extra_entries: vec![(Utf8PathBuf::from("app/entry.js"), EntryKind::Client)],
        ..ProjectScanOptions::default()
    };
    let analysis = analyze_project(&root, &build_id(), &options).unwrap();
    assert_eq!(analysis.client_bundle_root_count(), 1);
}

#[test]
fn analysing_the_same_project_twice_gives_the_same_manifest() {
    let (_dir, root) = project(&[
        ("app/$page.js", "import \"./client/Counter.js\";\n"),
        ("app/client/Counter.js", "\"use client\";\n"),
    ]);
    let first = analyze_project(&root, &build_id(), &ProjectScanOptions::default())
        .unwrap()
        .manifest()
        .to_json()
        .unwrap();
    let second = analyze_project(&root, &build_id(), &ProjectScanOptions::default())
        .unwrap()
        .manifest()
        .to_json()
        .unwrap();
    assert_eq!(first, second);
}

#[test]
fn a_non_utf8_module_is_reported_not_ignored() {
    let (_dir, root) = project(&[("app/$page.js", "// @flow\n")]);
    fs::write(root.join("app/broken.js"), [0xff, 0xfe, 0xfd]).unwrap();
    let error = analyze_project(&root, &build_id(), &ProjectScanOptions::default())
        .expect_err("invalid utf-8 must be reported");
    assert!(matches!(error, RscError::NonUtf8Source { .. }));
}
