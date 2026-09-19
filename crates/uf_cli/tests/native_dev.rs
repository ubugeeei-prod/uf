//! `uf dev --target native`: the device loop, run by the project's own CLI.
//!
//! The servers here are stand-ins — a shell script named `expo` or
//! `react-native` that writes down how it was started and exits — because what
//! these tests ask is uf's half of the loop: which server it chose, what it
//! handed that server, and what it refused to start. Metro, Expo CLI and React
//! Native CLI themselves are exercised against real projects outside this
//! repository (ubugeeei-prod/uf#990); a copy of either in here would be several
//! hundred megabytes proving somebody else's code.
//!
//! The Metro config check is not a stand-in. `node` loads each fixture's own
//! `metro.config.js` through a `metro-config` that answers the way Metro's does,
//! so a config that does not compose uf's transformer is refused by the same
//! code path a real project goes through.

#![cfg(unix)]

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use support::{uf, uf_path};

const REACT_NATIVE_CONFIG: &str = "// @flow\nexport default defineConfig({ app: { framework: \"react-native\", targets: [\"react-native\"] } });\n";

/// Metro's config loader, as far as `uf dev` asks it anything.
const METRO_CONFIG_STANDIN: &str = r#""use strict";
const fs = require("node:fs");
const path = require("node:path");

function find(cwd) {
  const file = path.join(cwd, "metro.config.js");
  return fs.existsSync(file) ? file : null;
}

exports.resolveConfig = async (filePath, cwd) => {
  const file = filePath ?? find(cwd);
  return file == null
    ? { filepath: "", isEmpty: true, config: {} }
    : { filepath: file, isEmpty: false, config: require(file) };
};

exports.loadConfig = async ({ cwd }) => {
  const file = find(cwd);
  const config = file == null ? {} : require(file);
  return {
    ...config,
    server: { port: 8081, ...(config.server ?? {}) },
    transformer: {
      babelTransformerPath: "/metro/src/metro-babel-transformer.js",
      ...(config.transformer ?? {}),
    },
  };
};
"#;

/// A Metro config composed the way `withUniflowedMetro()` composes one.
const COMPOSED_METRO_CONFIG: &str = r#"process.env.UF_METRO_CONFIGURED_UPSTREAM =
  require("node:path").join(__dirname, "node_modules/@react-native/metro-babel-transformer/src/index.js");
module.exports = {
  transformer: {
    babelTransformerPath: require.resolve("@uniflowed/react-native/metro-transformer.cjs"),
  },
};
"#;

/// A server that writes down its arguments and the `uf` it was handed.
const SERVER_STANDIN: &str = "#!/bin/sh\nroot=\"$(dirname \"$0\")/../..\"\nprintf '%s\\n' \"$@\" > \"$root/started.txt\"\nprintf '%s' \"$UF_BINARY\" > \"$root/uf-binary.txt\"\n";

#[test]
fn native_build_uses_the_project_cli_and_records_every_asset_without_vite() {
    if !node_ready() {
        return;
    }
    for target in ["ios", "android", "native"] {
        let project = native_project(REACT_NATIVE_CONFIG);
        let root = project.path();
        with_react_native_cli(root);
        write(root, "metro.config.js", COMPOSED_METRO_CONFIG);
        write(
            root,
            "app/$page.native.js",
            "export default component Page() { return null; }\n",
        );
        executable(
            root,
            "node_modules/.bin/react-native",
            r#"#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');
const args = process.argv.slice(2);
if (args[0] !== 'bundle' || !process.env.UF_BINARY) process.exit(9);
const argument = (name) => args[args.indexOf(name) + 1];
fs.writeFileSync(argument('--bundle-output'), 'native bundle');
fs.writeFileSync(argument('--sourcemap-output'), '{}');
const assets = argument('--assets-dest');
for (const name of ['icon.png', 'icon@2x.png', 'icon@3x.png', 'font.ttf']) {
  fs.writeFileSync(path.join(assets, name), 'asset');
}
"#,
        );
        let output = uf()
            .arg("--cwd")
            .arg(root)
            .args(["build", "--target", target])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join(".uf/build/meta/uf-build-manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["target"], target);
        assert_eq!(manifest["command"], "bundle");
        assert_eq!(manifest["targetContract"]["transform"]["platform"], target);
        assert_eq!(
            manifest["outputs"].as_array().unwrap().len(),
            if target == "native" { 12 } else { 6 }
        );
        assert!(root.join("router.ios.js").is_file());
        assert!(manifest.to_string().contains("icon@3x.png"));
        let explanation = uf()
            .arg("--cwd")
            .arg(root)
            .args(["explain", "build", "--target", target, "--json"])
            .output()
            .unwrap();
        assert!(explanation.status.success());
        let explained = String::from_utf8_lossy(&explanation.stdout);
        assert!(explained.contains("Metro"), "{explained}");
        assert!(!explained.contains("Vite"), "{explained}");
    }
}

#[test]
fn a_native_build_without_a_cli_refuses_before_running_the_web_builder() {
    let project = native_project(REACT_NATIVE_CONFIG);
    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["build", "--target", "ios"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("Expo or React Native CLI"), "{error}");
    assert!(!project.path().join("dist").exists());
}

/// Whether `node` can run here, which the Metro config check needs.
///
/// The policy of `support::bun_ready`, for the same reason: a check that skips
/// reads exactly like a check that passed. CI has Node and never skips.
fn node_ready() -> bool {
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "this test needs `node` on PATH and there is none, so it would prove nothing"
    );
    eprintln!("skipping: `node` is not on PATH");
    false
}

fn write(root: &Path, name: &str, contents: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn executable(root: &Path, name: &str, contents: &str) {
    write(root, name, contents);
    let path = root.join(name);
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

/// A React Native project whose dependencies are stand-ins, with `config` as
/// its `uf.config.js`.
fn native_project(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "uf.config.js", config);
    write(
        root,
        "node_modules/metro-config/package.json",
        r#"{ "name": "metro-config", "main": "index.js" }"#,
    );
    write(
        root,
        "node_modules/metro-config/index.js",
        METRO_CONFIG_STANDIN,
    );
    write(
        root,
        "node_modules/@uniflowed/react-native/package.json",
        r#"{ "name": "@uniflowed/react-native", "exports": { "./metro-transformer.cjs": "./metro-transformer.cjs" } }"#,
    );
    write(
        root,
        "node_modules/@uniflowed/react-native/metro-transformer.cjs",
        "module.exports.transform = async () => ({});\n",
    );
    dir
}

fn with_react_native_cli(root: &Path) {
    executable(root, "node_modules/.bin/react-native", SERVER_STANDIN);
    write(
        root,
        "node_modules/@react-native-community/cli/package.json",
        r#"{ "name": "@react-native-community/cli", "version": "20.2.0" }"#,
    );
    write(
        root,
        "node_modules/react-native/package.json",
        r#"{ "name": "react-native", "version": "0.87.1" }"#,
    );
}

fn with_expo(root: &Path) {
    executable(root, "node_modules/.bin/expo", SERVER_STANDIN);
    write(
        root,
        "node_modules/expo/package.json",
        r#"{ "name": "expo", "version": "57.0.22" }"#,
    );
}

fn started(root: &Path) -> Option<Vec<String>> {
    fs::read_to_string(root.join("started.txt"))
        .ok()
        .map(|text| text.lines().map(str::to_owned).collect())
}

#[test]
fn a_react_native_cli_project_starts_its_own_server_with_this_uf_and_the_passthrough() {
    if !node_ready() {
        return;
    }
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();
    with_react_native_cli(root);
    write(root, "metro.config.js", COMPOSED_METRO_CONFIG);

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args([
            "dev",
            "--target",
            "native",
            "--port",
            "8099",
            "--",
            "--reset-cache",
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert_eq!(
        started(root).expect("the server was started"),
        ["start", "--port", "8099", "--reset-cache"]
    );
    assert_eq!(
        fs::read_to_string(root.join("uf-binary.txt")).unwrap(),
        uf_path(),
        "the server compiles with the uf that started it"
    );
    assert!(
        stdout.contains(
            "react-native start (@react-native-community/cli 20.2.0, react-native 0.87.1)"
        ),
        "{stdout}"
    );
    assert!(stdout.contains(":8099"), "{stdout}");
    assert!(stdout.contains("metro.config.js"), "{stdout}");
    assert!(
        stdout.contains("node_modules/@react-native/metro-babel-transformer/src/index.js"),
        "the banner names the transformer that runs after uf's:\n{stdout}"
    );
}

#[test]
fn an_expo_project_starts_expo_and_prints_the_url_expo_go_opens() {
    if !node_ready() {
        return;
    }
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();
    with_expo(root);
    write(root, "metro.config.js", COMPOSED_METRO_CONFIG);

    let output = uf().arg("--cwd").arg(root).arg("dev").output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    // No `--target`: a `react-native` framework project develops the native
    // target, the way `uf build` builds it.
    assert_eq!(started(root).expect("the server was started"), ["start"]);
    assert!(stdout.contains("expo start (expo 57.0.22)"), "{stdout}");
    assert!(stdout.contains("exp://"), "{stdout}");
    assert!(stdout.contains(":8081"), "{stdout}");
}

#[test]
fn a_metro_config_that_does_not_compose_ufs_transformer_is_refused_before_the_server_starts() {
    if !node_ready() {
        return;
    }
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();
    with_react_native_cli(root);
    write(
        root,
        "metro.config.js",
        "module.exports = { transformer: { babelTransformerPath: \"/app/custom-transformer.js\" } };\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["dev", "--target", "ios"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("metro.config.js does not route modules through uf's transformer"),
        "{stderr}"
    );
    assert!(stderr.contains("/app/custom-transformer.js"), "{stderr}");
    assert!(
        stderr.contains("withUniflowedMetro(mergeConfig(getDefaultConfig(__dirname), {}))"),
        "{stderr}"
    );
    assert_eq!(started(root), None, "no server starts behind a refusal");
}

#[test]
fn a_project_with_no_native_dev_server_is_told_which_ones_uf_runs() {
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["dev", "--target", "android"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("`expo` (Expo CLI)"), "{stderr}");
    assert!(
        stderr.contains("`@react-native-community/cli` (React Native CLI)"),
        "{stderr}"
    );
}

#[test]
fn a_native_target_the_project_does_not_declare_is_refused() {
    let project =
        native_project("// @flow\nexport default defineConfig({ app: { targets: [\"web\"] } });\n");
    let root = project.path();
    with_react_native_cli(root);

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["dev", "--target", "native"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("`uf dev --target native` needs `app.targets` to include `react-native`"),
        "{stderr}"
    );
    assert_eq!(started(root), None);
}

#[test]
fn host_is_refused_for_a_native_target_with_the_servers_own_flags_named() {
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();
    with_react_native_cli(root);

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["dev", "--target", "native", "--host", "0.0.0.0"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("-- --lan"), "{stderr}");
    assert!(stderr.contains("-- --host 0.0.0.0"), "{stderr}");
    assert_eq!(started(root), None);
}

#[test]
fn arguments_after_the_separator_are_refused_for_the_web_target() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "uf.config.js",
        "// @flow\nexport default defineConfig({});\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["dev", "--", "--tunnel"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("-- --tunnel"), "{stderr}");
    assert!(stderr.contains("--target native"), "{stderr}");
}

#[test]
fn explain_names_the_native_server_and_the_metro_config_it_will_use() {
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();
    with_react_native_cli(root);
    write(root, "metro.config.js", COMPOSED_METRO_CONFIG);

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["explain", "dev", "--target", "native", "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(plan["command"], "uf dev --target native");
    let providers = plan["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|stage| stage["provider"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(
        providers.contains(
            &"react-native start (@react-native-community/cli 20.2.0, react-native 0.87.1)"
                .to_string()
        ),
        "{providers:?}"
    );
    assert!(
        providers.contains(&"metro.config.js".to_string()),
        "{providers:?}"
    );
    assert!(
        !providers.iter().any(|provider| provider.contains("vite")),
        "a native dev plan names no web builder: {providers:?}"
    );
}

#[test]
fn explain_refuses_a_target_for_a_command_it_does_not_describe_per_target() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "uf.config.js",
        "// @flow\nexport default defineConfig({});\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["explain", "fmt", "--target", "native"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("uf explain fmt --target native"),
        "{stderr}"
    );
}

#[test]
fn the_route_table_for_each_platform_is_written_before_the_server_starts() {
    if !node_ready() {
        return;
    }
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();
    with_react_native_cli(root);
    write(root, "metro.config.js", COMPOSED_METRO_CONFIG);
    write(root, "app/$page.native.js", "// @flow\n");
    write(root, "app/users/[id]/$page.js", "// @flow\n");
    write(root, "app/users/[id]/$page.ios.js", "// @flow\n");

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["dev", "--target", "native"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ios = fs::read_to_string(root.join("router.ios.js")).unwrap();
    let android = fs::read_to_string(root.join("router.android.js")).unwrap();
    assert!(ios.contains("\"./app/users/[id]/$page.ios.js\""), "{ios}");
    assert!(
        android.contains("\"./app/users/[id]/$page.js\""),
        "{android}"
    );
    assert!(!android.contains("$page.ios.js"), "{android}");
    assert!(root.join("router.native.js").is_file());
    assert!(
        !root.join("router.js").exists(),
        "a native target leaves the web router's types alone"
    );
    assert!(
        stdout.contains("2 routes → router.ios.js, router.android.js, router.native.js"),
        "{stdout}"
    );
}

#[test]
fn a_route_file_added_while_the_server_runs_is_in_the_table_without_a_restart() {
    if !node_ready() {
        return;
    }
    let project = native_project(REACT_NATIVE_CONFIG);
    let root = project.path();
    with_react_native_cli(root);
    // A server that stays up until the test says so, the way Metro stays up
    // until Ctrl-C.
    executable(
        root,
        "node_modules/.bin/react-native",
        "#!/bin/sh\nroot=\"$(dirname \"$0\")/../..\"\n: > \"$root/started.txt\"\ni=0\nwhile [ ! -f \"$root/stop\" ] && [ \"$i\" -lt 600 ]; do sleep 0.05; i=$((i + 1)); done\n",
    );
    write(root, "metro.config.js", COMPOSED_METRO_CONFIG);
    write(root, "app/$page.native.js", "// @flow\n");

    let child = std::process::Command::new(uf_path())
        .arg("--cwd")
        .arg(root)
        .args(["dev", "--target", "native"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let wait_for = |what: &str, done: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !done() {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    };
    wait_for("the server to start", &|| {
        root.join("started.txt").is_file()
    });
    assert!(
        !fs::read_to_string(root.join("router.ios.js"))
            .unwrap()
            .contains("\"/about\"")
    );

    write(root, "app/about/$page.native.js", "// @flow\n");
    wait_for("the route table to be rewritten", &|| {
        fs::read_to_string(root.join("router.ios.js"))
            .is_ok_and(|module| module.contains("\"/about\""))
    });

    write(root, "stop", "");
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("the routes changed; rewrote router.ios.js"),
        "{stderr}"
    );
}
