#![allow(clippy::disallowed_macros)]

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
    let runner_hosts = value["engines"]["testRunner"]["hostSupport"]
        .as_array()
        .expect("test runner hostSupport");
    assert_eq!(
        runner_hosts
            .iter()
            .map(|host| host["host"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["node", "deno", "bun"]
    );
    let runner_deno = runner_hosts
        .iter()
        .find(|host| host["host"] == "deno")
        .expect("the test runner names Deno");
    assert_eq!(runner_deno["level"], "implemented");
    assert!(
        runner_deno["missing"]
            .as_str()
            .unwrap_or_default()
            .contains("coverage"),
        "{runner_deno}"
    );
    // No tracking issue: what the row still names is coverage, the version
    // floor and an upstream Deno limitation, not work #246 is holding open.
    assert!(runner_deno["trackingIssue"].is_null(), "{runner_deno}");
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
    assert_eq!(value["tui"]["renderer"], serde_json::json!("cell-diff"));
    // Not `faster-than-react-ink`: that was never measured against React Ink,
    // and `uf inspect` is where somebody would read it and believe it. See
    // ubugeeei-prod/uf#247.
    assert_eq!(
        value["tui"]["reactInkTarget"]["performanceTarget"],
        serde_json::json!("writes-only-changed-cells")
    );
    let components: Vec<&str> = value["tui"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|component| component["name"].as_str().unwrap())
        .collect();
    // Seven, and the list is the package's: `uf inspect` is where somebody
    // reads which components exist before importing one.
    assert_eq!(
        components,
        [
            "Box",
            "Text",
            "Input",
            "ScrollBox",
            "Select",
            "TabSelect",
            "Textarea"
        ]
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
    let native_modules = value["nativeModules"].as_array().unwrap();
    let segment_of = |specifier: &str| {
        native_modules
            .iter()
            .find(|module| module["specifier"] == specifier)
            .unwrap_or_else(|| panic!("nativeModules names {specifier}"))["segment"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(segment_of("@uniflowed/core"), "core");
    assert_eq!(segment_of("@uniflowed/test"), "toolchain");
    assert_eq!(segment_of("@uniflowed/router"), "framework");
    assert_eq!(value["config"]["config_path"], serde_json::Value::Null);
    assert_eq!(
        value["config"]["config"]["pm"]["packageManager"],
        serde_json::json!("auto")
    );
    assert!(value["engines"]["packageManagerDetection"]["packageManager"].is_string());

    // The contract's `hosts` is a list of targets and was being read as a list
    // of hosts that work, which is how "uf runs on Deno" got written down. The
    // report beside it is what answers that question, and `uf inspect --json`
    // is what a person pastes into an issue — so the row that says whether the
    // host they are on is one uf runs on has to be in it.
    let hosts = value["engines"]["hostSupport"]
        .as_array()
        .expect("hostSupport");
    let deno = hosts
        .iter()
        .find(|host| host["host"] == "deno")
        .expect("every host has a row");
    assert_eq!(deno["level"], "implemented");
    // A module now, like Node's and Bun's: Deno 2.8 has `registerHooks`, and
    // the preload is what installs the transform in it.
    assert_eq!(deno["flowLoader"], "@uniflowed/host/deno-preload", "{deno}");
    // And what it still lacks is said rather than dropped with the grade.
    assert!(
        deno["missing"]
            .as_str()
            .unwrap_or_default()
            .contains("coverage"),
        "{deno}"
    );
    let node = hosts
        .iter()
        .find(|host| host["host"] == "node")
        .expect("every host has a row");
    assert_eq!(node["level"], "implemented");
    assert_eq!(
        node["enforcesPermissions"],
        serde_json::json!(["read", "write"])
    );
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

/// Each tool a command runs, and the key that declared it.
///
/// ubugeeei-prod/uf#940. `uf inspect` is where a person checks what their
/// config says, and a test runtime that no key names — because the runner
/// implies it — is exactly the answer that has to be read rather than worked
/// out.
#[test]
fn inspect_reports_each_tool_and_the_key_it_came_from() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(dir.path(), r#"{ "name": "demo" }"#);
    fs::write(
        dir.path().join("uf.config.js"),
        r#"export default defineConfig({
  runtime: "node@26",
  packageManager: "pnpm@12.0.0",
  build: { builder: "vite" },
  test: { runner: "bun@1.4" },
});
"#,
    )
    .unwrap();

    let value = inspect_json(dir.path());
    let tools = value["tools"].as_array().expect("a tools array");
    let row = |role: &str| {
        tools
            .iter()
            .find(|tool| tool["role"] == role)
            .unwrap_or_else(|| panic!("no {role} row in {tools:#?}"))
    };
    assert_eq!(row("runtime")["spec"], "node@26");
    assert_eq!(row("runtime")["key"], "runtime");
    assert_eq!(row("buildRuntime")["spec"], "node@26");
    assert_eq!(row("buildRuntime")["key"], "runtime");
    assert_eq!(row("testRuntime")["spec"], "bun@1.4");
    assert_eq!(row("testRuntime")["key"], "test.runner");
    assert_eq!(row("testRuntime")["via"], "implied");
    assert_eq!(row("testRunner")["spec"], "bun@1.4");
    assert_eq!(row("packageManager")["spec"], "pnpm@12.0.0");
    assert_eq!(row("packageManager")["key"], "packageManager");
    assert_eq!(row("builder")["spec"], "vite");
    assert_eq!(row("builder")["key"], "build.builder");
    // The config beside it is what the project wrote, not a rendering of it.
    assert_eq!(value["config"]["config"]["test"]["runner"], "bun@1.4");
    assert_eq!(value["toolDeprecations"], serde_json::json!([]));

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
    assert!(stdout.contains("  tools\n"), "{stdout}");
    assert!(
        stdout.contains("bun@1.4 (implied by test.runner)"),
        "{stdout}"
    );
    assert!(stdout.contains("pnpm@12.0.0 (packageManager)"), "{stdout}");
    assert_plain(&stdout);
}

/// A prefix is shown beside the release `uf.lock` locks it to, read without
/// fetching or installing anything.
#[test]
fn inspect_shows_the_release_a_prefix_is_locked_to() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(dir.path(), r#"{ "name": "demo" }"#);
    fs::write(
        dir.path().join("uf.config.js"),
        r#"export default defineConfig({ runtime: "node@26", test: { runtime: "bun@1.4" } });"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("uf.lock"),
        "{\n  \"toolchain\": {\n    \"bun@1.4\": \"1.4.2\"\n  }\n}\n",
    )
    .unwrap();

    let value = inspect_json(dir.path());
    let row = |role: &str| {
        value["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["role"] == role)
            .cloned()
            .unwrap_or_else(|| panic!("no {role} row"))
    };
    assert_eq!(row("testRuntime")["locked"], "1.4.2");
    assert_eq!(
        row("runtime")["locked"],
        serde_json::Value::Null,
        "node@26 is not locked yet"
    );
    assert_eq!(row("builder")["locked"], serde_json::Value::Null);
    // And a `uf.lock` holding only the record is no vote for uf's resolver.
    assert_eq!(
        value["engines"]["packageManagerDetection"]["source"]["kind"],
        "default"
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("inspect")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("bun@1.4 (test.runtime) · locked at 1.4.2"),
        "{stdout}"
    );
    assert!(stdout.contains("node@26 (runtime)\n"), "{stdout}");
}

/// A project still writing a tool key #940 replaced is told which key
/// replaced it, in the report and in the JSON.
#[test]
fn inspect_names_the_key_that_replaced_a_deprecated_tool_key() {
    let dir = tempfile::tempdir().unwrap();
    write_detection_project(dir.path(), r#"{ "name": "demo" }"#);
    fs::write(
        dir.path().join("uf.config.js"),
        r#"export default defineConfig({
  builder: { module: "@uniflowed/vite" },
  env: { toolchain: { node: "24.14.0" } },
});
"#,
    )
    .unwrap();

    let value = inspect_json(dir.path());
    let deprecations: Vec<&str> = value["toolDeprecations"]
        .as_array()
        .expect("a toolDeprecations array")
        .iter()
        .map(|sentence| sentence.as_str().unwrap())
        .collect();
    assert_eq!(deprecations.len(), 2, "{deprecations:#?}");
    assert!(
        deprecations[0].contains(r#"`runtime: "node@24.14.0"`"#),
        "{deprecations:#?}"
    );
    assert!(
        deprecations[1].contains(r#"builder: "vite""#),
        "{deprecations:#?}"
    );
    let builder = value["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["role"] == "builder")
        .expect("a builder row");
    assert_eq!(builder["key"], "builder.module");
    assert_eq!(builder["via"], "deprecated");
}
