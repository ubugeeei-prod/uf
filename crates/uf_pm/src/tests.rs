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
fn install_refuses_the_install_time_hooks_a_manifest_declares() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{
  "name": "demo",
  "version": "1.0.0",
  "scripts": {
    "start": "expo start",
    "postinstall": "node scripts/fetch.js"
  }
}
"#,
    )
    .unwrap();

    let error = install_workspace(&root, &UniflowedConfig::default()).unwrap_err();

    match &error {
        PackageManagerError::ScriptsForbidden { scripts, .. } => {
            assert_eq!(scripts, &["postinstall"]);
        }
        other => panic!("expected the lifecycle refusal, got {other}"),
    }
    let message = error.to_string();
    assert!(
        message.contains("declares install-time lifecycle scripts (postinstall)"),
        "{message}"
    );
    assert!(!message.contains("start"), "{message}");
    assert!(message.contains("uf tasks"), "{message}");
}

/// Scripts no install runs are the project's business, and uf leaves them.
///
/// `create-expo-app` and `@react-native-community/cli init` both write `start`,
/// `android` and `ios`. Refusing those refused every project either tool
/// generates while protecting against nothing, because no package manager runs
/// a named script during an install. See ubugeeei-prod/uf#992.
#[test]
fn install_accepts_scripts_that_no_install_runs() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{
  "name": "demo",
  "version": "1.0.0",
  "scripts": {
    "start": "expo start",
    "android": "expo start --android",
    "ios": "expo start --ios",
    "web": "expo start --web",
    "prestart": "echo before",
    "test": "jest"
  }
}
"#,
    )
    .unwrap();

    check_workspace_manifests(&root, &UniflowedConfig::default()).unwrap();
    let unrun = scripts_uf_does_not_run(&root).unwrap();

    assert_eq!(unrun.len(), 1, "{unrun:?}");
    assert_eq!(unrun[0].0, root.join("package.json"));
    let mut names = unrun[0].1.clone();
    names.sort();
    assert_eq!(
        names,
        ["android", "ios", "prestart", "start", "test", "web"]
    );
}

/// Every hook on the list is refused on its own, and is named in the refusal.
#[test]
fn every_install_time_hook_is_refused_by_name() {
    for hook in INSTALL_LIFECYCLE_SCRIPTS {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::write(
            root.join("package.json"),
            format!(
                "{{ \"name\": \"demo\", \"version\": \"1.0.0\", \"scripts\": {{ \"{hook}\": \"echo hook\" }} }}\n"
            ),
        )
        .unwrap();

        let error = check_workspace_manifests(&root, &UniflowedConfig::default()).unwrap_err();

        assert!(
            matches!(
                &error,
                PackageManagerError::ScriptsForbidden { scripts, .. }
                    if scripts.len() == 1 && scripts[0] == *hook
            ),
            "{hook}: {error}"
        );
    }
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

/// `uf install` rewrites `uf.lock` from the manifests and keeps the toolchain
/// record `uf_env` put there.
///
/// ubugeeei-prod/uf#940: which release `node@26` resolved to is not the
/// manifests' to decide, and an install that dropped it would have every prefix
/// re-resolved on the next command.
#[test]
fn install_keeps_the_toolchain_record_in_uf_lock() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(root.join("package.json"), r#"{ "name": "demo" }"#).unwrap();
    fs::write(
        root.join("uf.lock"),
        "{\n  \"toolchain\": {\n    \"node@26\": \"26.8.2\"\n  }\n}\n",
    )
    .unwrap();

    install_workspace(&root, &UniflowedConfig::default()).unwrap();

    let lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("uf.lock")).unwrap()).unwrap();
    assert_eq!(
        lock["toolchain"],
        serde_json::json!({ "node@26": "26.8.2" })
    );
    assert_eq!(lock["packages"][0]["name"], "demo");

    // A project with no record gets the file it always got.
    fs::remove_file(root.join("uf.lock")).unwrap();
    install_workspace(&root, &UniflowedConfig::default()).unwrap();
    assert!(
        !fs::read_to_string(root.join("uf.lock"))
            .unwrap()
            .contains("toolchain")
    );
}

/// An install waits for a command holding `uf.lock`, and keeps what that
/// command locked while it waited.
///
/// Without the guard the install reads the record before the other command
/// writes it and renames over it afterwards, and the entry is gone.
#[test]
fn install_waits_for_a_command_holding_the_lock() {
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(root.join("package.json"), r#"{ "name": "demo" }"#).unwrap();
    let held = uf_env::lock::guard(&root.join("uf.lock")).unwrap();

    let (done, finished) = mpsc::channel();
    let worker_root = root.clone();
    let worker = std::thread::spawn(move || {
        let installed = install_workspace(&worker_root, &UniflowedConfig::default()).map(|_| ());
        done.send(()).unwrap();
        installed
    });

    assert!(
        finished.recv_timeout(Duration::from_millis(300)).is_err(),
        "installed while another command held the lock"
    );
    fs::write(
        root.join("uf.lock"),
        "{\n  \"toolchain\": {\n    \"node@26\": \"26.8.2\"\n  }\n}\n",
    )
    .unwrap();
    drop(held);

    finished
        .recv_timeout(Duration::from_secs(10))
        .expect("the install finished once the lock was free");
    worker.join().unwrap().unwrap();
    let lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join("uf.lock")).unwrap()).unwrap();
    assert_eq!(
        lock["toolchain"],
        serde_json::json!({ "node@26": "26.8.2" })
    );
}

/// The manager uf installed is the one that runs: the directory it is linked
/// into goes in front of the child's `PATH`, and the manager's name — which
/// is still a fixed program name, never a path — is looked up there.
/// ubugeeei-prod/uf#940.
#[cfg(unix)]
#[test]
fn a_path_prefix_decides_which_manager_runs() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(root.join("package.json"), r#"{ "name": "demo" }"#).unwrap();
    let bin = root.join("store-bin");
    fs::create_dir_all(&bin).unwrap();
    let marks = root.join("ran");
    let pnpm = bin.join("pnpm");
    fs::write(&pnpm, format!("#!/bin/sh\necho \"$@\" > '{marks}'\n")).unwrap();
    fs::set_permissions(&pnpm, fs::Permissions::from_mode(0o755)).unwrap();
    let detection = detect_package_manager_with(
        &root,
        &DetectionOptions::new()
            .with_boundary(&root)
            .with_config_override(PackageManager::Pnpm),
    );

    run_operation_with_detection(&root, &detection, Operation::List, &[], false, &[bin])
        .expect("the pnpm in the prefix ran");

    assert!(
        marks.is_file(),
        "a pnpm other than the one in the prefix ran, or none did"
    );
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

/// A checkout that is not a submodule is still not ours.
///
/// `.gitmodules` names the repositories git tracks a gitlink for, and that is
/// only some of the repositories inside this one. `tools/corpus/sync.sh`
/// fetches fifteen third-party trees into `tests/fixtures/git`, each with a
/// `.git` file pointing at a git directory outside the checkout, and eleven of
/// them were never in `.gitmodules` at all — so Parcel's twenty-eight scripts
/// were being read as this project breaking its own no-scripts rule, and
/// `uf install` refused to run. ubugeeei-prod/uf#137 took the other four out
/// of `.gitmodules`, which would have made that every one of them.
///
/// A `.git` *file* rather than a directory, because that is the shape all of
/// them have: `git init --separate-git-dir` writes one, and so does a
/// submodule.
#[test]
fn a_nested_checkout_is_not_one_of_the_project_s_packages() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{ "name": "demo", "version": "1.0.0" }"#,
    )
    .unwrap();
    // No `.gitmodules`: this is a pinned fetch, not a submodule.
    let fetched = root.join("tests/fixtures/git/parcel");
    fs::create_dir_all(&fetched).unwrap();
    fs::write(
        fetched.join(".git"),
        "gitdir: ../../../../.git/corpus/parcel\n",
    )
    .unwrap();
    fs::write(
        fetched.join("package.json"),
        r#"{ "name": "@parcel/monorepo", "version": "2.0.0", "scripts": { "build": "gulp" } }"#,
    )
    .unwrap();
    // And its own workspace packages, which are equally not ours.
    let nested = fetched.join("packages/core");
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("package.json"),
        r#"{ "name": "@parcel/core", "version": "2.0.0" }"#,
    )
    .unwrap();

    let report = install_workspace(&root, &UniflowedConfig::default()).unwrap();

    let names: Vec<&str> = report
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    assert_eq!(names, ["demo"], "a nested checkout was locked as a package");
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

/// ubugeeei-prod/uf#540: the plan resolves against `pm.registry`, and only
/// falls back to `publish.registry` for a project that never set the new one.
#[test]
fn the_plan_resolves_against_the_registry_uf_reads_from() {
    let mut config = UniflowedConfig::default();
    config.publish.registry = CompactString::const_new("https://npm.company.example");
    assert_eq!(
        PackageManagerPlan::infer_from_config(&config).registry,
        "https://npm.company.example"
    );

    config.pm.registry = Some(CompactString::const_new("https://mirror.company.example"));
    let plan = PackageManagerPlan::infer_from_config(&config);
    assert_eq!(plan.registry, "https://mirror.company.example");
    // And the publish target is not what a resolver reads.
    assert_ne!(plan.registry, config.publish.registry);
}
