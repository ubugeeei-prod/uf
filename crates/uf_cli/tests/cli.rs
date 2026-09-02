//! End-to-end coverage for the commands that scaffold, build, and serve.

mod support;

use std::fs;

use support::{assert_plain, binary, create_app, uf};

#[test]
fn uf_prints_help() {
    let output = uf().arg("--help").output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Unified Toolchain for Flow (React)"));
    assert!(stdout.contains("--color"));
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

    let output = binary("ufr")
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
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "alias-ok",
        "a task owns stdout; uf must not render onto it"
    );
}

#[test]
fn ufx_alias_runs_uniflowed_create_package() {
    let dir = tempfile::tempdir().unwrap();

    let output = binary("ufx")
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
    assert!(stdout.contains("ufx  @uniflowed/create"));
    assert!(stdout.contains("UfNative"));
    assert!(stdout.contains("exec-cache"));
    assert!(stdout.contains("created 12 files"));
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

    let stdout = create_app(&app);

    assert!(app.join("app.js").exists());
    assert!(app.join("uf.config.js").exists());
    assert!(app.join("app/_uf.page.js").exists());
    assert!(app.join("app/_uf.page.native.js").exists());
    assert!(app.join("server/actions.js").exists());

    let package = fs::read_to_string(app.join("package.json")).unwrap();
    assert!(!package.contains(r#""scripts""#));

    // The generated files are shown as a tree, not as a flat count.
    assert!(stdout.contains("uf create"));
    assert!(stdout.contains("├─ app"));
    assert!(stdout.contains("│  ├─ client"));
    assert!(stdout.contains("└─ uf.config.js"));
    assert!(stdout.contains("next steps"));
    assert!(stdout.contains("1. cd app"));
    assert!(stdout.contains("2. uf install"));
    assert!(stdout.contains("3. uf dev"));
    assert!(stdout.contains("✓ created 12 files"));
}

#[test]
fn creating_a_library_suggests_running_its_tests() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("kit");

    let output = uf().args(["create", "lib"]).arg(&lib).output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("3. uf test"));
}

#[test]
fn creating_over_an_existing_project_reports_the_conflict_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf()
        .args(["create", "app", "react"])
        .arg(&app)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("--force"));
}

#[test]
fn build_writes_native_manifest_and_router_types() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf().arg("--cwd").arg(&app).arg("build").output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("uf build"));
    assert!(stdout.contains("uf-build-manifest.json"));
    assert!(stdout.contains("uf-rsc-manifest.json"));
    assert!(stdout.contains("router.js"));
    assert!(stdout.contains("✓ build succeeded in"));
    assert_plain(&stdout);
    assert!(
        String::from_utf8(output.stderr).unwrap().is_empty(),
        "a successful build must not write to stderr"
    );

    let manifest_path = app.join("dist/uf-build-manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["engine"], serde_json::json!("uf-native"));
    assert_eq!(
        manifest["pluginContract"],
        serde_json::json!("uf-plugin-v1")
    );
    assert_eq!(manifest["runtime"]["wintertc"], serde_json::json!(true));
    assert!(app.join("router.js").exists());
    assert!(
        fs::read_to_string(app.join("router.js"))
            .unwrap()
            .contains("export type RoutePath")
    );

    let rsc_manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(app.join("dist/uf-rsc-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(
        rsc_manifest["clientBundleRoots"],
        serde_json::json!(["app/client/Counter.js"])
    );
    assert_eq!(
        rsc_manifest["serverActions"][0]["module"],
        serde_json::json!("server/actions.js")
    );
    assert_eq!(rsc_manifest["diagnostics"], serde_json::json!([]));
}

#[test]
fn build_reports_each_phase_timing_and_a_summary() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf().arg("--cwd").arg(&app).arg("build").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    for phase in ["config", "routes", "rsc analysis", "manifest", "total"] {
        assert!(
            stdout.contains(phase),
            "missing phase {phase} in:\n{stdout}"
        );
    }
    assert!(stdout.contains("client components"));
    assert!(stdout.contains("server actions"));
    assert!(stdout.contains("output"));
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

    let output = uf()
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
    assert!(stdout.contains("uf dev"));
    assert!(stdout.contains("uf-native"));
    assert!(stdout.contains("http://127.0.0.1:"));
    assert!(stdout.contains("/__uf/health"));
    assert!(stdout.contains("✓ dev server ready"));

    let state: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join(".uf/dev-server.json")).unwrap())
            .unwrap();
    assert_eq!(state["engine"], serde_json::json!("uf-native"));
    assert!(state["port"].as_u64().unwrap() > 0);

    // The access-control posture is part of the reported state, and the default
    // is loopback with no allowlists. See `docs/security.md`.
    assert!(stdout.contains("exposure"), "{stdout}");
    assert!(stdout.contains("loopback"), "{stdout}");
    assert!(stdout.contains("0 hosts, 0 origins, 1 root"), "{stdout}");
    assert_eq!(state["exposure"], serde_json::json!("loopback"));
    assert_eq!(state["allowedHosts"], serde_json::json!([]));
    assert_eq!(state["allowedOrigins"], serde_json::json!([]));
    assert!(
        state["fsDeny"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(".env*"))
    );
}

#[test]
fn dev_host_without_an_allowed_hosts_list_refuses_to_start() {
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

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["dev", "--host", "0.0.0.0", "--once"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("allowedHosts"), "{stderr}");
}

#[test]
fn dev_host_with_an_allowed_hosts_list_starts_exposed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              dev: { port: 0, allowedHosts: ["dev.internal"] },
            });
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["dev", "--host", "0.0.0.0", "--once"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("exposed"), "{stdout}");
    assert!(stdout.contains("1 host, 0 origins, 1 root"), "{stdout}");
}

#[test]
fn lsp_initialize_returns_native_capabilities() {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let input = format!("Content-Length: {}\r\n\r\n{body}", body.len());

    let output = uf().arg("lsp").write_stdin(input).output().unwrap();

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
    assert_plain(&stdout);
}

#[test]
fn fmt_reports_an_already_formatted_project() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf().arg("--cwd").arg(&app).arg("fmt").output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("uf fmt"));
    assert!(stdout.contains("✓"));
}

/// `uf fmt` must not rewrite `package.json`.
///
/// Discovery returns it because the linter reads it, and the formatter used to
/// take the same list — so `uf fmt` inserted a statement terminator after
/// `"@uniflowed/core": "latest"` and left the manifest unparseable. `uf install`
/// then failed on a project whose only crime was running `uf fmt`.
///
/// A scaffolded project is a real one: this is what `uf create && uf fmt` did.
#[test]
fn fmt_leaves_the_package_manifest_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let manifest = app.join("package.json");
    let before = fs::read_to_string(&manifest).unwrap();
    serde_json::from_str::<serde_json::Value>(&before).expect("the scaffold writes valid JSON");

    let output = uf().arg("--cwd").arg(&app).arg("fmt").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&manifest).unwrap();
    serde_json::from_str::<serde_json::Value>(&after)
        .expect("uf fmt must leave package.json parseable");
    assert_eq!(before, after, "uf fmt rewrote package.json");
}

/// And `--check` must not claim it needs formatting either, which is how a
/// green CI job would start failing for a file the formatter must never touch.
#[test]
fn fmt_check_ignores_the_package_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf()
        .arg("--cwd")
        .arg(&app)
        .args(["fmt", "--check"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("package.json"),
        "uf fmt --check listed package.json:\n{stdout}"
    );
    assert!(
        output.status.success(),
        "a freshly scaffolded project must pass `uf fmt --check`:\n{stdout}"
    );
}

#[test]
fn env_use_records_the_active_environment() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "use", "staging"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("✓ active environment: staging")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".uniflowed/env")).unwrap(),
        "staging\n"
    );
}
