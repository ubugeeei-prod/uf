//! The commands that run tests, install packages, and write release plans.

mod support;

use std::fs;

use support::{assert_plain, uf};

fn write_test_file(dir: &std::path::Path, name: &str, body: &str) {
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join(name), body).unwrap();
}

#[test]
fn test_list_discovers_native_test_import_shape() {
    let dir = tempfile::tempdir().unwrap();
    write_test_file(
        dir.path(),
        "index.test.js",
        "// @flow\nimport { it } from '@uniflowed/test';\nit('runs', () => {});\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["test", "--list"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("src/index.test.js"));
    assert!(stdout.contains("discovered 1 runnable test"));
    assert!(stdout.contains("FasterThanBun"));
    assert!(stdout.contains("location"));
    assert_plain(&stdout);
}

// What `uf test` does when it *runs* a suite — assertions, failures, hooks,
// filters, retries, bail — is covered in `testing.rs`, against a real host.
// Those tests need the workspace's `node_modules`, so they build their
// projects inside this repository rather than in a system temp directory.

#[test]
fn prepare_prints_lint_staged_and_codegen_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("prepare")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("lint-staged compatible  yes"));
    assert!(stdout.contains("GenerateRouterTypes"));
    assert!(stdout.contains("GenerateValidatorTypes"));
    assert!(stdout.contains("✓ prepare plan written"));
    assert!(dir.path().join(".uf/prepare.json").exists());
}

#[test]
fn publish_and_release_report_trusted_publish_plan() {
    let dir = tempfile::tempdir().unwrap();

    let publish = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("publish")
        .output()
        .unwrap();

    assert!(
        publish.status.success(),
        "{}",
        String::from_utf8_lossy(&publish.stderr)
    );
    let stdout = String::from_utf8(publish.stdout).unwrap();
    assert!(stdout.contains("first publish     Local"));
    assert!(stdout.contains("local bootstrap   yes"));
    assert!(stdout.contains("trusted provider  GitHubActionsOidc"));
    assert!(stdout.contains("tokenless         yes"));
    assert!(stdout.contains("trigger           TagPush"));
    assert!(stdout.contains("publish.json"));
    assert!(dir.path().join(".uf/publish.json").exists());

    let release = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["release", "alpha"])
        .output()
        .unwrap();

    assert!(
        release.status.success(),
        "{}",
        String::from_utf8_lossy(&release.stderr)
    );
    let stdout = String::from_utf8(release.stdout).unwrap();
    // `uf release` bumps this crate's own version, so a literal tag here would
    // fail on the next version bump rather than when the plan is wrong. The
    // bump arithmetic itself is pinned by the unit tests next to
    // `bump_semver`; this only has to predict the same answer for whatever
    // version the workspace is on right now — including one that is already a
    // prerelease, which the previous `strip_suffix(".0")` got wrong the first
    // time a release moved off `0.0.0-alpha.0`.
    let current = env!("CARGO_PKG_VERSION");
    let expected = match current.split_once("-alpha.") {
        Some((core, count)) => {
            let count: u64 = count.parse().expect("the alpha count is numeric");
            format!("{core}-alpha.{}", count + 1)
        }
        None => format!("{current}-alpha.0"),
    };
    assert!(stdout.contains("bump             Alpha"));
    assert!(stdout.contains(&format!("tag              uf@{expected}")));
    assert!(stdout.contains("command          uf release alpha"));
    assert!(stdout.contains("publish          yes"));
    assert!(stdout.contains("release.json"));
    assert!(dir.path().join(".uf/release.json").exists());
}

#[test]
fn install_reports_native_package_manager_plan() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "name": "install-demo",
  "dependencies": {
    "@uniflowed/core": "latest"
  }
}
"#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("install")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("resolver       UfNative"));
    assert!(stdout.contains("scripts        Forbid"));
    assert!(stdout.contains("uf.lock"));
    assert!(stdout.contains(".uf/store/manifest.json"));
    assert!(stdout.contains("packages       1"));
    assert!(stdout.contains("store entries  1"));
    assert!(stdout.contains("✓ installed 1 package"));
    assert!(dir.path().join("uf.lock").exists());
    assert!(dir.path().join(".uf/store/manifest.json").exists());
    assert!(dir.path().join(".uf/store/packages").exists());
}

#[test]
fn install_rejects_npm_scripts() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "name": "scripted",
  "scripts": {
    "test": "jest"
  }
}
"#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("install")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("error: "));
    assert!(stderr.contains("declares scripts"));
    assert!(stderr.contains("uf tasks"));
}

#[test]
fn upgrade_reports_package_and_runtime_manager_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("upgrade")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("package resolver  UfNative"));
    assert!(stdout.contains("runtime engine    Node"));
    assert!(stdout.contains("acquisition       Auto"));
    assert!(stdout.contains("upgrade.json"));
    assert!(stdout.contains("✓ workspace upgraded"));
    assert!(dir.path().join("uf.lock").exists());
    assert!(dir.path().join(".uf/upgrade.json").exists());
}

#[test]
fn use_reports_xdg_runtime_switch_plan() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let config_home = dir.path().join("xdg-config");
    let data_home = dir.path().join("xdg-data");
    let cache_home = dir.path().join("xdg-cache");
    let state_home = dir.path().join("xdg-state");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["use", "uf@0.1.0"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("XDG_STATE_HOME", &state_home)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("uf use  uf@0.1.0"));
    assert!(stdout.contains("auto switch  enabled"));
    assert!(stdout.contains(".local/bin/uf"));
    assert!(stdout.contains("active-runtime.json"));
    assert!(stdout.contains("runtime.json"));
    assert!(stdout.contains("WriteShim"));
    assert!(stdout.contains("ActivateVersion"));
    assert!(stdout.contains("✓ now using uf@0.1.0"));
    assert!(
        data_home
            .join("uniflowed/runtimes/uf/0.1.0/bin/uf")
            .exists()
    );
    assert!(
        data_home
            .join("uniflowed/runtimes/uf/0.1.0/runtime.json")
            .exists()
    );
    assert!(state_home.join("uniflowed/active-runtime.json").exists());
    assert!(home.join(".local/bin/uf").exists());
}

#[test]
fn an_undefined_task_reports_an_error_on_stderr() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run", "nope"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("error: task \"nope\" is not defined"));
    assert_plain(&stderr);
}
