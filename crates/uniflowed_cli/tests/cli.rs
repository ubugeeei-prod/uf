use std::fs;

use assert_cmd::Command;

#[test]
fn uf_prints_help() {
    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Unified Toolchain for Flow (React)"));
}

#[test]
fn creates_react_app_from_cli() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");

    let output = Command::cargo_bin("uf")
        .unwrap()
        .args(["create", "app", "react"])
        .arg(&app)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(app.join("app.flow").exists());
    assert!(app.join("app/_uf.page.flow").exists());
    assert!(app.join("app/_uf.page.native.flow").exists());
    assert!(app.join("server/actions.flow").exists());
    assert!(!app.join("uniflowed.config.flow").exists());
}

#[test]
fn inspect_reports_zero_config_defaults() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
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
    assert_eq!(value["config"]["config_path"], serde_json::Value::Null);
}

#[test]
fn test_list_discovers_native_test_import_shape() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.test.js"),
        "// @flow\nimport { it } from '@uniflowed/testing';\nit('runs', () => {});\n",
    )
    .unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
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
}
