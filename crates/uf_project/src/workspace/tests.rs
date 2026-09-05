use std::fs;

use camino::Utf8PathBuf;

use super::*;

/// A project tree with a `uf.config.js` at each of `members`.
fn tree(members: &[&str]) -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 path");
    fs::write(root.join("uf.config.js"), "export default {};\n").expect("write the root config");

    for member in members {
        let path = root.join(member);
        fs::create_dir_all(&path).expect("create the member");
        fs::write(path.join("uf.config.js"), "export default {};\n").expect("write its config");
    }
    (dir, root)
}

fn names(workspaces: &[Workspace]) -> Vec<&str> {
    workspaces
        .iter()
        .map(|workspace| workspace.name.as_str())
        .collect()
}

#[test]
fn a_directory_with_its_own_config_is_a_member() {
    let (_dir, root) = tree(&["docs", "site"]);

    let found = discover_workspaces(&root, &UniflowedConfig::default());

    assert_eq!(names(&found), vec!["docs", "site"]);
}

/// `uf dev` already means the root, so the root is not one of its own members.
#[test]
fn the_root_is_not_a_member_of_itself() {
    let (_dir, root) = tree(&[]);

    assert!(discover_workspaces(&root, &UniflowedConfig::default()).is_empty());
}

#[test]
fn a_member_may_be_nested() {
    let (_dir, root) = tree(&["tests/library"]);

    let found = discover_workspaces(&root, &UniflowedConfig::default());

    assert_eq!(names(&found), vec!["library"]);
    assert_eq!(found[0].path, "tests/library");
}

/// A project inside a member is that member's business. Descending into it
/// would offer `uf dev#example` for something the root cannot meaningfully run.
#[test]
fn a_member_inside_a_member_is_not_discovered() {
    let (_dir, root) = tree(&["docs", "docs/example"]);

    assert_eq!(
        names(&discover_workspaces(&root, &UniflowedConfig::default())),
        vec!["docs"]
    );
}

#[test]
fn ignored_directories_hold_no_members() {
    let (_dir, root) = tree(&["node_modules/pkg", "docs"]);

    assert_eq!(
        names(&discover_workspaces(&root, &UniflowedConfig::default())),
        vec!["docs"],
        "a config inside node_modules is somebody else's"
    );
}

/// A checkout inside a checkout is another repository, and its projects are
/// not this one's — the same rule `scan_source_files` applies.
#[test]
fn another_repository_holds_no_members() {
    let (_dir, root) = tree(&["vendor"]);
    fs::create_dir_all(root.join("vendor/.git")).expect("make it a repository");

    assert!(discover_workspaces(&root, &UniflowedConfig::default()).is_empty());
}

#[test]
fn discovery_is_bounded_by_depth() {
    let (_dir, root) = tree(&["a/b/c/d/deep"]);

    assert!(
        discover_workspaces(&root, &UniflowedConfig::default()).is_empty(),
        "a project five levels down is not discoverable by a person either"
    );
}

#[test]
fn members_come_back_in_a_stable_order() {
    let (_dir, root) = tree(&["zebra", "alpha", "middle"]);

    let first = discover_workspaces(&root, &UniflowedConfig::default());
    for _ in 0..8 {
        assert_eq!(
            discover_workspaces(&root, &UniflowedConfig::default()),
            first
        );
    }
    assert_eq!(names(&first), vec!["alpha", "middle", "zebra"]);
}

// --- selecting one ------------------------------------------------------

fn members() -> Vec<Workspace> {
    vec![
        Workspace {
            name: "docs".into(),
            path: "docs".into(),
        },
        Workspace {
            name: "library".into(),
            path: "tests/library".into(),
        },
    ]
}

#[test]
fn a_member_is_selected_by_its_name() {
    let members = members();
    let found = resolve_workspace(&members, "docs").expect("docs is a member");

    assert_eq!(found.path, "docs");
}

/// Both spellings are things someone reasonably types, and the path is the one
/// that settles an ambiguity between two directories with the same name.
#[test]
fn a_member_is_also_selected_by_its_path() {
    let members = members();
    let found = resolve_workspace(&members, "tests/library").expect("the path selects it");

    assert_eq!(found.name, "library");
}

#[test]
fn selecting_nothing_reports_what_there_was() {
    let members = members();
    let available = resolve_workspace(&members, "dcos").expect_err("no such member");

    assert_eq!(available, vec!["docs", "library"]);
}

#[test]
fn selecting_in_a_project_with_no_members_reports_an_empty_list() {
    assert_eq!(
        resolve_workspace(&[], "docs").expect_err("nothing to select"),
        Vec::<CompactString>::new()
    );
}
