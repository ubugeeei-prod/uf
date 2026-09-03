//! `uf inspect`, in both its machine and its human shape.

mod support;

use std::fs;

use support::{assert_plain, uf};

#[test]
fn inspect_reports_zero_config_defaults() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["inspect", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], serde_json::json!("uf"));
    assert_eq!(
        value["engines"]["reactCompiler"],
        serde_json::json!({"enabled": true, "mode": "syntax"})
    );
    assert_eq!(
        value["engines"]["runtimeContract"]["standard"],
        serde_json::json!("winter-tc")
    );
    assert_eq!(
        value["engines"]["runtimeContract"]["javascriptEngine"],
        serde_json::json!("capability-js-host")
    );
    assert_eq!(
        value["engines"]["runtimeContract"]["hosts"],
        serde_json::json!(["node", "deno", "bun"])
    );
    assert_eq!(
        value["engines"]["runtime"],
        serde_json::json!("capability-js-host-contract")
    );
    assert_eq!(
        value["engines"]["testRunner"]["runtime"],
        serde_json::json!("capability-js-host")
    );
    assert_eq!(
        value["engines"]["testRunner"]["hosts"],
        serde_json::json!(["node", "deno", "bun"])
    );
    assert_eq!(
        value["engines"]["testRunner"]["performanceTarget"],
        serde_json::json!("faster-than-bun")
    );
    assert_eq!(
        value["engines"]["packageManager"]["resolver"],
        serde_json::json!("uf-native")
    );
    assert_eq!(value["engines"]["build"], serde_json::json!("vite"));
    assert_eq!(value["engines"]["devServer"], serde_json::json!("vite"));
    assert_eq!(
        value["engines"]["runtimeManager"]["acquisition"],
        serde_json::json!("auto")
    );
    assert_eq!(value["tui"]["standard"], serde_json::json!("open-tui"));
    assert_eq!(
        value["tui"]["renderer"],
        serde_json::json!("cell-diff-native")
    );
    assert_eq!(
        value["tui"]["reactInkTarget"]["performanceTarget"],
        serde_json::json!("faster-than-react-ink")
    );
    assert!(
        value["tui"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|component| component["name"] == "EmbeddedTerminal")
    );
    assert!(
        value["stdModules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["specifier"] == "@uniflowed/std/import-meta")
    );
    assert!(
        value["stdModules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["specifier"] == "@uniflowed/std/tui")
    );
    assert_eq!(value["config"]["config_path"], serde_json::Value::Null);
    assert_eq!(
        value["config"]["config"]["pm"]["packageManager"],
        serde_json::json!("auto")
    );
    assert!(value["engines"]["packageManagerDetection"]["packageManager"].is_string());
}

/// Write a project uf's config loader will treat as its own root.
fn write_detection_project(dir: &std::path::Path, manifest: &str) {
    fs::write(dir.join("package.json"), manifest).unwrap();
}

fn inspect_json(dir: &std::path::Path) -> serde_json::Value {
    let output = uf()
        .arg("--cwd")
        .arg(dir)
        .args(["inspect", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn inspect_detects_pnpm_from_a_lockfile() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(dir.path(), r#"{ "name": "demo" }"#);
    fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();

    let value = inspect_json(dir.path());
    let detection = &value["engines"]["packageManagerDetection"];

    assert_eq!(detection["packageManager"], serde_json::json!("pnpm"));
    assert_eq!(detection["source"]["kind"], serde_json::json!("lockfile"));
    assert_eq!(
        detection["source"]["lockfile"],
        serde_json::json!("pnpm-lock")
    );
    assert_eq!(
        detection["outcome"]["kind"],
        serde_json::json!("unambiguous")
    );
}

#[test]
fn inspect_detects_yarn_berry_from_the_package_manager_field() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(
        dir.path(),
        r#"{ "name": "demo", "packageManager": "yarn@4.1.0+sha224.abc" }"#,
    );
    fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();

    let value = inspect_json(dir.path());
    let detection = &value["engines"]["packageManagerDetection"];

    assert_eq!(detection["packageManager"], serde_json::json!("yarn-berry"));
    assert_eq!(
        detection["source"]["kind"],
        serde_json::json!("package-manager-field")
    );
    assert_eq!(
        detection["source"]["spec"]["version"]["major"],
        serde_json::json!(4)
    );
    assert_eq!(
        detection["source"]["spec"]["integrity"],
        serde_json::json!("sha224.abc")
    );
    assert_eq!(
        detection["alternatives"][0]["packageManager"],
        serde_json::json!("pnpm")
    );
}

#[test]
fn inspect_reports_side_by_side_lockfiles_as_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(dir.path(), r#"{ "name": "demo" }"#);
    fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();
    fs::write(dir.path().join("package-lock.json"), "{}\n").unwrap();

    let value = inspect_json(dir.path());
    let detection = &value["engines"]["packageManagerDetection"];

    assert_eq!(detection["outcome"]["kind"], serde_json::json!("ambiguous"));
    assert_eq!(
        detection["outcome"]["lockfiles"],
        serde_json::json!(["pnpm-lock", "package-lock"])
    );
}

#[test]
fn inspect_rejects_a_hostile_package_manager_field() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(
        dir.path(),
        r#"{ "name": "demo", "packageManager": "pnpm@9.0.0; rm -rf /" }"#,
    );
    fs::write(dir.path().join("bun.lock"), "{}\n").unwrap();

    let value = inspect_json(dir.path());
    let detection = &value["engines"]["packageManagerDetection"];

    assert_eq!(detection["packageManager"], serde_json::json!("bun"));
    assert_eq!(
        detection["issues"][0]["kind"],
        serde_json::json!("invalid-package-manager-field")
    );
    assert_eq!(
        detection["issues"][0]["error"]["kind"],
        serde_json::json!("forbidden-character")
    );
}

#[test]
fn inspect_honours_the_config_package_manager_override() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(dir.path(), r#"{ "name": "demo" }"#);
    fs::write(
        dir.path().join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"export default defineConfig({ pm: { packageManager: "bun" } });"#,
    )
    .unwrap();

    let value = inspect_json(dir.path());
    let detection = &value["engines"]["packageManagerDetection"];

    assert_eq!(detection["packageManager"], serde_json::json!("bun"));
    assert_eq!(
        detection["source"]["kind"],
        serde_json::json!("config-override")
    );
}

#[test]
fn inspect_text_output_reports_the_detected_package_manager() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(dir.path(), r#"{ "name": "demo" }"#);
    fs::write(dir.path().join("yarn.lock"), "# yarn lockfile v1\n").unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("inspect")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("detected       yarn-classic"));
    assert!(stdout.contains("detected from  lockfile"));
    assert!(stdout.contains("ambiguous      no"));
    assert_plain(&stdout);
}

#[test]
fn inspect_text_output_is_sectioned() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("inspect")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    for section in ["project", "app", "runners", "package manager", "catalogue"] {
        assert!(
            stdout.contains(&format!("  {section}\n")),
            "missing section {section} in:\n{stdout}"
        );
    }
    assert!(stdout.contains("zero-config defaults"));
    assert!(stdout.contains("lint rules"));
}
