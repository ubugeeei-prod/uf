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
        dir.path().join("uf.config.js"),
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
fn ufx_alias_runs_uniflowed_create_package() {
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
    assert!(stdout.contains("exec-cache"));
    assert!(stdout.contains("created 12 file"));
    assert!(dir.path().join("app.js").exists());
    assert!(
        dir.path()
            .join(".uf/exec-cache/_uniflowed_create.json")
            .exists()
    );
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
    assert!(app.join("app.js").exists());
    assert!(app.join("uf.config.js").exists());
    assert!(app.join("app/_uf.page.js").exists());
    assert!(app.join("app/_uf.page.native.js").exists());
    assert!(app.join("server/actions.js").exists());

    let package = fs::read_to_string(app.join("package.json")).unwrap();
    assert!(!package.contains(r#""scripts""#));
}

#[test]
fn build_writes_native_manifest_and_router_types() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");

    let create = Command::cargo_bin("uf")
        .unwrap()
        .args(["create", "app", "react"])
        .arg(&app)
        .output()
        .unwrap();

    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );

    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("--cwd")
        .arg(&app)
        .arg("build")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("backend=uf-native/vite-compatible/rolldown-compatible"));
    assert!(stdout.contains("uf-build-manifest.json"));

    let manifest_path = app.join("dist/uf-build-manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["engine"], serde_json::json!("uf-native"));
    assert_eq!(
        manifest["bundlerCompatibility"],
        serde_json::json!(["vite", "rolldown"])
    );
    assert_eq!(manifest["runtime"]["wintertc"], serde_json::json!(true));
    assert!(app.join("router.js").exists());
    assert!(
        fs::read_to_string(app.join("router.js"))
            .unwrap()
            .contains("export type RoutePath")
    );
}

#[test]
fn dev_once_writes_native_server_state() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              dev: { port: 0 },
            });
        "#,
    )
    .unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("--cwd")
        .arg(dir.path())
        .args(["dev", "--once"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("backend=uf-native/vite-compatible-dev-server"));

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join(".uf/dev-server.json")).unwrap())
            .unwrap();
    assert_eq!(state["engine"], serde_json::json!("uf-native"));
    assert!(state["port"].as_u64().unwrap() > 0);
}

#[test]
fn lsp_initialize_returns_native_capabilities() {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let input = format!("Content-Length: {}\r\n\r\n{body}", body.len());

    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("lsp")
        .write_stdin(input)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("Content-Length: "));
    assert!(stdout.contains(r#""name":"uf-lsp""#));
    assert!(stdout.contains(r#""documentFormattingProvider":true"#));
    assert!(stdout.contains(r#""workspaceDiagnostics":true"#));
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
fn test_runs_native_assertion_subset() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("math.test.js"),
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("adds", () => {
  expect(1 + 1).toBe(2);
});
"#,
    )
    .unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
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
    assert!(stdout.contains("passed=1"));
    assert!(stdout.contains("failed=0"));
    assert!(stdout.contains("FasterThanBun"));
}

#[test]
fn test_reports_native_assertion_failures() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("math.test.js"),
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("fails", () => {
  expect("flow").toBe("typescript");
});
"#,
    )
    .unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("--cwd")
        .arg(dir.path())
        .arg("test")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("failed=1"));
    assert!(stdout.contains("toBe assertion failed"));
}

#[test]
fn test_rejects_unsupported_native_assertions() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("array.test.js"),
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("contains", () => {
  expect([1, 2, 3]).toContain(2);
});
"#,
    )
    .unwrap();

    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("--cwd")
        .arg(dir.path())
        .arg("test")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("unsupportedAssertions=1"));
    assert!(stderr.contains("unsupported assertion"));
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
    assert!(dir.path().join(".uf/prepare.json").exists());
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
    assert!(stdout.contains("publish.json"));
    assert!(dir.path().join(".uf/publish.json").exists());

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
    assert!(stdout.contains("tag=uf@0.2.0"));
    assert!(stdout.contains("command=uf release minor"));
    assert!(stdout.contains("publish=true"));
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
    assert!(stdout.contains("uf.lock"));
    assert!(stdout.contains(".uf/store/manifest.json"));
    assert!(stdout.contains("scripts=Forbid"));
    assert!(stdout.contains("packages=1"));
    assert!(stdout.contains("storeEntries=1"));
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

    let output = Command::cargo_bin("uf")
        .unwrap()
        .arg("--cwd")
        .arg(dir.path())
        .arg("install")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("declares scripts"));
    assert!(stderr.contains("uf tasks"));
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
    assert!(stdout.contains("upgrade.json"));
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

    let output = Command::cargo_bin("uf")
        .unwrap()
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
    assert!(stdout.contains("runtime=uf@0.1.0"));
    assert!(stdout.contains("autoSwitch=true"));
    assert!(stdout.contains(".local/bin/uf"));
    assert!(stdout.contains("active-runtime.json"));
    assert!(stdout.contains("runtime.json"));
    assert!(stdout.contains("WriteShim"));
    assert!(stdout.contains("ActivateVersion"));
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
