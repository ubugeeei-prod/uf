//! The ranking between config override, manifest field, lockfile and ancestor.

use super::*;

#[test]
fn package_manager_field_outranks_a_lockfile() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    write(
        &root.join("package.json"),
        r#"{ "name": "demo", "packageManager": "yarn@4.1.0" }"#,
    );

    let detection = detect_within(&root, &root);

    assert_eq!(
        detection.package_manager,
        PackageManager::Yarn(YarnEdition::Berry)
    );
    assert_eq!(detection.source.kind(), "package-manager-field");
    assert_eq!(
        detection.alternatives[0].package_manager,
        PackageManager::Pnpm
    );
    assert!(!detection.is_ambiguous());
}

#[test]
fn package_manager_field_keeps_the_parsed_version_and_integrity() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        r#"{ "packageManager": "pnpm@9.1.0+sha512.abc123" }"#,
    );

    let detection = detect_within(&root, &root);

    let DetectionSource::PackageManagerField { spec, manifest } = &detection.source else {
        panic!("expected the packageManager field to decide detection");
    };
    assert_eq!(manifest, &root.join("package.json"));
    assert_eq!(spec.manager, PackageManager::Pnpm);
    assert_eq!(spec.version.to_string(), "9.1.0");
    assert_eq!(spec.integrity.as_deref(), Some("sha512.abc123"));
}

#[test]
fn config_override_outranks_the_package_manager_field() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    write(
        &root.join("package.json"),
        r#"{ "packageManager": "yarn@4.1.0" }"#,
    );

    let options = DetectionOptions::new()
        .with_boundary(&root)
        .with_config_override(PackageManager::Bun);
    let detection = detect_package_manager_with(&root, &options);

    assert_eq!(detection.package_manager, PackageManager::Bun);
    assert_eq!(detection.source, DetectionSource::ConfigOverride);
    assert_eq!(detection.alternatives.len(), 2);
    assert_eq!(
        detection.alternatives[0].package_manager,
        PackageManager::Yarn(YarnEdition::Berry)
    );
    assert_eq!(
        detection.alternatives[1].package_manager,
        PackageManager::Pnpm
    );
}

#[test]
fn local_lockfile_outranks_the_ancestor_workspace_root() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    let package = root.join("packages/app");
    write(&package.join("bun.lock"), "{}\n");

    let detection = detect_within(&root, &package);

    assert_eq!(detection.package_manager, PackageManager::Bun);
    assert_eq!(detection.source.kind(), "lockfile");
    assert_eq!(
        detection.alternatives[0].package_manager,
        PackageManager::Pnpm
    );
    assert_eq!(detection.alternatives[0].source.kind(), "workspace-root");
}
