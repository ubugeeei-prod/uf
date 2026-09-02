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

#[test]
fn test_runs_native_assertion_subset() {
    let dir = tempfile::tempdir().unwrap();
    write_test_file(
        dir.path(),
        "math.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("adds", () => {
  expect(1 + 1).toBe(2);
});
"#,
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("test")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("✓ src/math.test.js  adds"));
    assert!(stdout.contains("passed"));
    assert!(stdout.contains("1 passed, 0 failed in"));
    assert!(stdout.contains("FasterThanBun"));
}

#[test]
fn test_reports_native_assertion_failures() {
    let dir = tempfile::tempdir().unwrap();
    write_test_file(
        dir.path(),
        "math.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("fails", () => {
  expect("flow").toBe("typescript");
});
"#,
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("test")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("✗ src/math.test.js  fails"));
    assert!(stdout.contains("toBe assertion failed"));
    assert!(stdout.contains("0 passed, 1 failed in"));

    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("error: "));
    assert!(stderr.contains("1 failure"));
}

#[test]
fn test_rejects_unsupported_native_assertions() {
    let dir = tempfile::tempdir().unwrap();
    write_test_file(
        dir.path(),
        "array.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("contains", () => {
  expect([1, 2, 3]).toContain(2);
});
"#,
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("test")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("unsupported assertions  1"));
    assert!(stdout.contains("1 unsupported in"));
    assert!(stderr.contains("unsupported assertion"));
}

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
        .args(["release", "minor"])
        .output()
        .unwrap();

    assert!(
        release.status.success(),
        "{}",
        String::from_utf8_lossy(&release.stderr)
    );
    let stdout = String::from_utf8(release.stdout).unwrap();
    // `uf release` bumps this crate's own version, so a literal tag here fails
    // on the next version bump rather than when the plan is wrong. The bump
    // arithmetic is pinned by the unit tests next to `bump_semver`.
    let mut version = env!("CARGO_PKG_VERSION").split('.');
    let major: u64 = version.next().unwrap().parse().unwrap();
    let minor: u64 = version.next().unwrap().parse().unwrap();
    assert!(stdout.contains("bump             Minor"));
    assert!(stdout.contains(&format!("tag              uf@{major}.{}.0", minor + 1)));
    assert!(stdout.contains("command          uf release minor"));
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
    assert!(stdout.contains("runtime engine    Uf"));
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
