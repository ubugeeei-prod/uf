use super::*;

#[test]
fn default_plan_is_self_hosted_and_script_free() {
    let plan = PackageManagerPlan::default();

    assert_eq!(plan.resolver, PackageResolver::UfNative);
    assert_eq!(plan.lockfile, "uf.lock");
    assert_eq!(plan.store.strategy, PackageStoreStrategy::ContentAddressed);
    assert!(plan.forbids_npm_scripts());
    assert!(plan.steps.contains(&PackageManagerStep::ResolveGraph));
    assert!(plan.steps.contains(&PackageManagerStep::VerifyIntegrity));
}

#[test]
fn infers_registry_store_and_script_policy_from_config() {
    let config = UniflowedConfig::default();
    let plan = PackageManagerPlan::infer_from_config(&config);

    assert_eq!(plan.registry, "https://registry.npmjs.org");
    assert_eq!(plan.store.directory, ".uf/store");
    assert_eq!(plan.scripts, PackageScriptPolicy::Forbid);
}

#[test]
fn records_workspace_packages_without_npm_scripts() {
    let plan =
        PackageManagerPlan::default().with_workspace_package("@uniflowed/core", "packages/core");

    assert_eq!(plan.workspace_packages[0].name, "@uniflowed/core");
    assert!(plan.forbids_npm_scripts());
}

#[test]
fn install_writes_lockfile_and_store_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{
  "name": "demo",
  "version": "1.2.3",
  "dependencies": {
"@uniflowed/core": "latest"
  }
}
"#,
    )
    .unwrap();

    let report = install_workspace(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(report.packages.len(), 1);
    assert_eq!(report.packages[0].name, "demo");
    assert!(report.packages[0].integrity.starts_with("uf-fnv1a64-"));
    assert_eq!(report.store_entries.len(), 1);
    assert!(report.lockfile.exists());
    assert!(report.store_manifest.exists());
    assert!(report.store_entries[0].exists());
    assert!(
        fs::read_to_string(root.join("uf.lock"))
            .unwrap()
            .contains("\"lockfileVersion\": 1")
    );
    assert!(
        fs::read_to_string(report.store_entries[0].as_std_path())
            .unwrap()
            .contains("\"integrity\"")
    );
}

#[test]
fn install_rejects_package_scripts_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{
  "name": "demo",
  "scripts": {
"test": "jest"
  }
}
"#,
    )
    .unwrap();

    let error = install_workspace(&root, &UniflowedConfig::default()).unwrap_err();

    assert!(matches!(
        error,
        PackageManagerError::ScriptsForbidden { .. }
    ));
}

#[test]
fn install_drops_prototype_pollution_dependency_keys() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{
  "name": "demo",
  "dependencies": {
"__proto__": "1.0.0",
"constructor": "1.0.0",
"prototype": "1.0.0",
"@uniflowed/core": "latest"
  }
}
"#,
    )
    .unwrap();

    let report = install_workspace(&root, &UniflowedConfig::default()).unwrap();

    let dependencies = &report.packages[0].dependencies;
    assert_eq!(dependencies.len(), 1);
    assert!(dependencies.contains_key("@uniflowed/core"));
    for key in POLLUTING_JSON_KEYS {
        assert!(!dependencies.contains_key(key), "{key} survived");
    }
}

#[test]
fn polluting_json_keys_are_recognised() {
    assert!(is_polluting_json_key("__proto__"));
    assert!(is_polluting_json_key("constructor"));
    assert!(is_polluting_json_key("prototype"));
    assert!(!is_polluting_json_key("dependencies"));
    assert!(!is_polluting_json_key("__proto__x"));
}

#[test]
fn install_still_detects_the_native_resolver_afterwards() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(root.join("package.json"), r#"{ "name": "demo" }"#).unwrap();

    install_workspace(&root, &UniflowedConfig::default()).unwrap();
    let detection =
        detect_package_manager_with(&root, &DetectionOptions::new().with_boundary(&root));

    assert_eq!(detection.package_manager, PackageManager::Uf);
    assert_eq!(
        detection.source,
        DetectionSource::Lockfile {
            lockfile: Lockfile::UfLock,
            path: root.join("uf.lock"),
        }
    );
}

/// A submodule is somebody else's repository, not one of this project's
/// packages.
///
/// This repository has had `upstream/flow` — Meta's Flow — since the
/// beginning, and its manifest was being locked as a workspace package. It
/// went unnoticed because that one happens to declare no scripts; a fixture
/// that does, such as Metro, turned it into `uf install` refusing to run at
/// all.
#[test]
fn a_submodule_is_not_one_of_the_project_s_packages() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{ "name": "demo", "version": "1.0.0" }"#,
    )
    .unwrap();
    fs::write(
        root.join(".gitmodules"),
        "[submodule \"vendored\"]\n\tpath = vendor/thing\n\turl = https://example.test/thing\n",
    )
    .unwrap();

    let vendored = root.join("vendor/thing");
    fs::create_dir_all(&vendored).unwrap();
    // A manifest that would be refused if it were ours.
    fs::write(
        vendored.join("package.json"),
        r#"{ "name": "thing", "version": "2.0.0", "scripts": { "build": "make" } }"#,
    )
    .unwrap();

    let report = install_workspace(&root, &UniflowedConfig::default()).unwrap();

    let names: Vec<&str> = report
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    assert_eq!(names, ["demo"], "the submodule was locked as a package");
}

#[test]
fn a_directory_that_merely_looks_like_a_submodule_is_still_ours() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{ "name": "demo", "version": "1.0.0" }"#,
    )
    .unwrap();
    // No `.gitmodules`, so nothing is skipped.
    let nested = root.join("packages/core");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("package.json"),
        r#"{ "name": "@demo/core", "version": "1.0.0" }"#,
    )
    .unwrap();

    let report = install_workspace(&root, &UniflowedConfig::default()).unwrap();

    let mut names: Vec<&str> = report
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["@demo/core", "demo"]);
}
