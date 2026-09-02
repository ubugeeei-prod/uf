//! Reading `package.json` when the repository is assumed hostile.

use super::*;

#[cfg(unix)]
#[test]
fn a_symlinked_lockfile_never_votes() {
    let (_guard, root) = temp_root();
    let outside = root.join("outside");
    let project = root.join("project");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(&project).unwrap();
    write(&outside.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    std::os::unix::fs::symlink(
        outside.join("pnpm-lock.yaml").as_std_path(),
        project.join("pnpm-lock.yaml").as_std_path(),
    )
    .unwrap();

    let detection = detect_within(&root, &project);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert!(scan_lockfiles(&project).is_empty());
}

#[cfg(unix)]
#[test]
fn a_symlinked_manifest_is_refused_instead_of_followed() {
    let (_guard, root) = temp_root();
    let outside = root.join("outside");
    let project = root.join("project");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(&project).unwrap();
    write(
        &outside.join("package.json"),
        r#"{ "packageManager": "bun@1.1.0" }"#,
    );
    std::os::unix::fs::symlink(
        outside.join("package.json").as_std_path(),
        project.join("package.json").as_std_path(),
    )
    .unwrap();

    let detection = detect_within(&root, &project);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert!(
        detection
            .issues
            .contains(&DetectionIssue::ManifestUnusable {
                manifest: project.join("package.json"),
                fault: ManifestFault::NotARegularFile,
            })
    );
}

#[test]
fn a_hostile_package_manager_field_is_rejected_and_never_executed() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        r#"{ "packageManager": "pnpm@9.0.0; rm -rf /" }"#,
    );

    let detection = detect_within(&root, &root);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert_eq!(detection.source, DetectionSource::Default);
    assert!(matches!(
        detection.issues.first(),
        Some(DetectionIssue::InvalidPackageManagerField {
            error: PackageManagerFieldError::ForbiddenCharacter { character: ';', .. },
            ..
        })
    ));
}

#[test]
fn an_oversized_manifest_is_refused_with_a_typed_issue() {
    let (_guard, root) = temp_root();
    let mut manifest = String::from(r#"{ "packageManager": "pnpm@9.0.0", "pad": ""#);
    manifest.push_str(&"a".repeat(4096));
    manifest.push_str("\" }");
    write(&root.join("package.json"), &manifest);

    let options = DetectionOptions::new()
        .with_boundary(&root)
        .with_max_manifest_bytes(64);
    let detection = detect_package_manager_with(&root, &options);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert!(matches!(
        detection.issues.first(),
        Some(DetectionIssue::ManifestTooLarge { limit: 64, .. })
    ));
}

#[test]
fn an_invalid_manifest_is_refused_with_a_typed_issue() {
    let (_guard, root) = temp_root();
    write(&root.join("package.json"), "{ not json");

    let detection = detect_within(&root, &root);

    assert!(
        detection
            .issues
            .contains(&DetectionIssue::ManifestUnusable {
                manifest: root.join("package.json"),
                fault: ManifestFault::InvalidJson,
            })
    );
}

#[test]
fn a_non_object_manifest_is_refused() {
    let (_guard, root) = temp_root();
    write(&root.join("package.json"), "[1, 2, 3]");

    let detection = detect_within(&root, &root);

    assert!(
        detection
            .issues
            .contains(&DetectionIssue::ManifestUnusable {
                manifest: root.join("package.json"),
                fault: ManifestFault::InvalidJson,
            })
    );
}

#[test]
fn prototype_pollution_keys_are_reported_and_ignored() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        r#"{ "__proto__": { "packageManager": "npm@10.0.0" }, "constructor": {}, "prototype": {} }"#,
    );

    let detection = detect_within(&root, &root);

    assert_eq!(detection.package_manager, PackageManager::Uf);
    for key in ["__proto__", "constructor", "prototype"] {
        assert!(
            detection
                .issues
                .contains(&DetectionIssue::PollutingManifestKey {
                    manifest: root.join("package.json"),
                    key: CompactString::const_new(key),
                })
        );
    }
}

#[test]
fn a_manifest_with_a_bom_and_crlf_still_parses() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        "\u{feff}{\r\n  \"packageManager\": \"bun@1.1.30\"\r\n}\r\n",
    );

    // serde_json rejects a BOM, so the manifest is refused rather than guessed at.
    let detection = detect_within(&root, &root);
    assert!(
        detection
            .issues
            .contains(&DetectionIssue::ManifestUnusable {
                manifest: root.join("package.json"),
                fault: ManifestFault::InvalidJson,
            })
    );

    write(
        &root.join("package.json"),
        "{\r\n  \"packageManager\": \"bun@1.1.30\"\r\n}\r\n",
    );
    assert_eq!(
        detect_within(&root, &root).package_manager,
        PackageManager::Bun
    );
}

#[test]
fn a_non_ascii_package_manager_field_is_rejected() {
    let (_guard, root) = temp_root();
    write(
        &root.join("package.json"),
        "{ \"packageManager\": \"pnpm@9.0.0\u{3002}1\" }",
    );

    let detection = detect_within(&root, &root);

    assert!(matches!(
        detection.issues.first(),
        Some(DetectionIssue::InvalidPackageManagerField {
            error: PackageManagerFieldError::ForbiddenCharacter {
                character: '\u{3002}',
                ..
            },
            ..
        })
    ));
}

#[test]
fn detection_round_trips_through_json() {
    let (_guard, root) = temp_root();
    write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    write(
        &root.join("package.json"),
        r#"{ "packageManager": "yarn@4.1.0" }"#,
    );

    let detection = detect_within(&root, &root);
    let json = serde_json::to_string(&detection).unwrap();
    let parsed = serde_json::from_str::<Detection>(&json).unwrap();

    assert_eq!(parsed, detection);
    assert!(json.contains(r#""packageManager":"yarn-berry""#));
    assert!(json.contains(r#""kind":"package-manager-field""#));
}

#[test]
fn detection_is_idempotent() {
    let (_guard, root) = temp_root();
    write(&root.join("bun.lock"), "{}\n");

    let first = detect_within(&root, &root);
    let second = detect_within(&root, &root);

    assert_eq!(first, second);
}
