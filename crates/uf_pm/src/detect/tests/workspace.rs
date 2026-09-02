//! The ancestor walk: what marks a root, and the bounds it must respect.

use super::*;

#[test]
fn ancestor_lockfile_is_inherited_by_a_workspace_member() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    let package = root.join("packages/app");
    write(&package.join("package.json"), r#"{ "name": "app" }"#);

    let detection = detect_within(&root, &package);

    assert_eq!(detection.package_manager, PackageManager::Pnpm);
    assert_eq!(
        detection.source,
        DetectionSource::WorkspaceRoot {
            root: root.clone(),
            marker: WorkspaceMarker::Lockfile(Lockfile::PnpmLock),
        }
    );
}

#[test]
fn pnpm_workspace_yaml_marks_the_workspace_root() {
    let (_guard, root) = temp_root();
    write(
        &root.join("pnpm-workspace.yaml"),
        "packages:\n  - packages/*\n",
    );
    let package = root.join("packages/app");
    write(&package.join("package.json"), r#"{ "name": "app" }"#);

    let detection = detect_within(&root, &package);

    assert_eq!(detection.package_manager, PackageManager::Pnpm);
    assert_eq!(
        detection.source,
        DetectionSource::WorkspaceRoot {
            root: root.clone(),
            marker: WorkspaceMarker::PnpmWorkspaceYaml,
        }
    );
}

#[test]
fn package_json_workspaces_marks_the_workspace_root() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        r#"{ "workspaces": ["packages/*"], "packageManager": "npm@10.5.0" }"#,
    );
    let package = root.join("packages/app");
    write(&package.join("package.json"), r#"{ "name": "app" }"#);

    let detection = detect_within(&root, &package);

    assert_eq!(detection.package_manager, PackageManager::Npm);
    assert_eq!(
        detection.source,
        DetectionSource::WorkspaceRoot {
            root: root.clone(),
            marker: WorkspaceMarker::PackageJsonWorkspaces,
        }
    );
}

#[test]
fn yarn_object_workspaces_mark_the_workspace_root() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        r#"{ "workspaces": { "packages": ["packages/*"] }, "packageManager": "yarn@1.22.19" }"#,
    );
    let package = root.join("packages/app");
    fs::create_dir_all(&package).unwrap();

    let detection = detect_within(&root, &package);

    assert_eq!(
        detection.package_manager,
        PackageManager::Yarn(YarnEdition::Classic)
    );
}

#[test]
fn empty_workspaces_array_is_not_a_workspace_root() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        r#"{ "workspaces": [], "packageManager": "npm@10.5.0" }"#,
    );
    let package = root.join("packages/app");
    fs::create_dir_all(&package).unwrap();

    assert_eq!(
        detect_within(&root, &package).package_manager,
        PackageManager::Uf
    );
}

#[test]
fn a_workspace_root_without_manager_evidence_falls_back_to_uf() {
    let (_guard, root) = temp_root();
    write(&root.join("package.json"), r#"{ "workspaces": ["pkg/*"] }"#);
    let package = root.join("pkg/app");
    fs::create_dir_all(&package).unwrap();

    let detection = detect_within(&root, &package);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert_eq!(detection.source, DetectionSource::Default);
}

#[test]
fn the_ancestor_walk_stops_at_a_git_directory() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    let repo = root.join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    let package = repo.join("app");
    fs::create_dir_all(&package).unwrap();

    let detection = detect_within(&root, &package);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert_eq!(detection.source, DetectionSource::Default);
}

#[test]
fn the_ancestor_walk_is_depth_bounded() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    let deep = root.join("a/b/c/d");
    fs::create_dir_all(&deep).unwrap();

    let options = DetectionOptions::new()
        .with_boundary(&root)
        .with_max_ancestors(2);
    let detection = detect_package_manager_with(&deep, &options);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert!(
        detection
            .issues
            .contains(&DetectionIssue::AncestorLimitReached { limit: 2 })
    );
}

#[test]
fn the_ancestor_walk_never_reads_outside_the_boundary() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    let inner = root.join("inner");
    let package = inner.join("app");
    fs::create_dir_all(&package).unwrap();

    let bounded =
        detect_package_manager_with(&package, &DetectionOptions::new().with_boundary(&inner));
    assert_eq!(bounded.package_manager, PackageManager::Uf);

    let unbounded =
        detect_package_manager_with(&package, &DetectionOptions::new().with_boundary(&root));
    assert_eq!(unbounded.package_manager, PackageManager::Pnpm);
}

#[test]
fn a_start_path_that_escapes_the_boundary_reads_nothing() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    let inner = root.join("inner");
    fs::create_dir_all(&inner).unwrap();

    let escape = inner.join("..");
    let detection =
        detect_package_manager_with(&escape, &DetectionOptions::new().with_boundary(&inner));

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert_eq!(detection.source, DetectionSource::Default);
    assert!(matches!(
        detection.issues.first(),
        Some(DetectionIssue::OutsideBoundary { .. })
    ));
}

#[test]
fn lexical_normalization_never_escapes_a_root() {
    assert_eq!(lexically_normalized(Utf8Path::new("/a/./b/../c")), "/a/c");
    assert_eq!(lexically_normalized(Utf8Path::new("/../..")), "/");
    assert_eq!(lexically_normalized(Utf8Path::new("a/b/../../..")), "..");
    assert_eq!(lexically_normalized(Utf8Path::new("./a/")), "a");
}
