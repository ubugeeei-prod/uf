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
fn ufr_alias_runs_config_task() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.flow"),
        r#"
            export default defineConfig({
              tasks: {
                hello: { command: "printf alias-ok" },
              },
            });
        "#,
    )
    .unwrap();

    let output = Command::cargo_bin("ufr")
        .unwrap()
        .arg("--cwd")
        .arg(dir.path())
        .arg("hello")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "alias-ok");
}

#[test]
fn ufx_alias_reports_temporary_exec_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("ufx")
        .unwrap()
        .arg("--cwd")
        .arg(dir.path())
        .args(["@uniflowed/create", "app"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ufx: package=@uniflowed/create"));
    assert!(stdout.contains("resolver=UfNative"));
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
    assert!(app.join("uf.config.flow").exists());
    assert!(app.join("app/_uf.page.flow").exists());
    assert!(app.join("app/_uf.page.native.flow").exists());
    assert!(app.join("server/actions.flow").exists());

    let package = fs::read_to_string(app.join("package.json")).unwrap();
    assert!(!package.contains(r#""scripts""#));
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
    assert_eq!(
        value["engines"]["runtimeContract"]["standard"],
        serde_json::json!("winter-tc")
    );
    assert_eq!(
        value["engines"]["runtimeContract"]["javascriptEngine"],
        serde_json::json!("hermes")
    );
    assert_eq!(
        value["engines"]["testRunner"]["performanceTarget"],
        serde_json::json!("faster-than-bun")
    );
    assert_eq!(
        value["engines"]["packageManager"]["resolver"],
        serde_json::json!("uf-native")
    );
    assert_eq!(
        value["engines"]["runtimeManager"]["acquisition"],
        serde_json::json!("auto")
    );
    assert!(
        value["stdModules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["specifier"] == "@uniflowed/std/import-meta")
    );
    assert_eq!(
        value["orm"]["parameterizedQueriesOnly"],
        serde_json::json!(true)
    );
    assert_eq!(value["motion"]["engine"], serde_json::json!("uf-native"));
    assert_eq!(value["vrt"]["baselines"], serde_json::json!("__uf_vrt__"));
    assert_eq!(value["config"]["config_path"], serde_json::Value::Null);
}

#[test]
fn test_list_discovers_native_test_import_shape() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("index.test.js"),
        "// @flow\nimport { it } from '@uniflowed/test';\nit('runs', () => {});\n",
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
    assert!(stdout.contains("FasterThanBun"));
}

#[test]
fn prepare_prints_lint_staged_and_codegen_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
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
    assert!(stdout.contains("lintStagedCompatible=true"));
    assert!(stdout.contains("GenerateRouterTypes"));
    assert!(stdout.contains("GenerateValidatorTypes"));
}

#[test]
fn publish_and_release_report_trusted_publish_plan() {
    let dir = tempfile::tempdir().unwrap();

    let publish = Command::cargo_bin("uf")
        .unwrap()
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
    assert!(stdout.contains("firstPublish=Local"));
    assert!(stdout.contains("localBootstrap=true"));
    assert!(stdout.contains("trustedProvider=GitHubActionsOidc"));
    assert!(stdout.contains("tokenless=true"));
    assert!(stdout.contains("trigger=TagPush"));

    let release = Command::cargo_bin("uf")
        .unwrap()
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
    assert!(stdout.contains("bump=Minor"));
    assert!(stdout.contains("tagPrefix=uf@"));
    assert!(stdout.contains("command=uf release minor"));
    assert!(stdout.contains("publish=true"));
}

#[test]
fn install_reports_native_package_manager_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
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
    assert!(stdout.contains("resolver=UfNative"));
    assert!(stdout.contains("lockfile=uf.lock"));
    assert!(stdout.contains("scripts=Forbid"));
}

#[test]
fn upgrade_reports_package_and_runtime_manager_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
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
    assert!(stdout.contains("packageResolver=UfNative"));
    assert!(stdout.contains("runtimeEngine=Uf"));
    assert!(stdout.contains("acquisition=Auto"));
}

#[test]
fn use_reports_xdg_runtime_switch_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("--cwd")
        .arg(dir.path())
        .args(["use", "uf@0.1.0"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("runtime=uf@0.1.0"));
    assert!(stdout.contains("autoSwitch=true"));
    assert!(stdout.contains(".local/bin/uf"));
    assert!(stdout.contains("WriteShim"));
    assert!(stdout.contains("ActivateVersion"));
}
