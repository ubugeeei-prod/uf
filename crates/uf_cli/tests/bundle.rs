//! `uf build`'s shipped-size report and the budgets it enforces.

mod support;

use std::fs;

use support::uf;

/// A scaffolded app, optionally with extra `uf.config.js` keys.
///
/// `uf build` bundles the project into `dist/assets/`, so a scaffolded app
/// already has real weight to measure. Tests that need a specific size on top
/// of that write their own files into `dist/`, which is exactly what the size
/// reporter walks.
fn built_app(config_extra: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();

    let created = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["create", "app", "react"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    if !config_extra.is_empty() {
        let config_path = dir.path().join("uf.config.js");
        let config = fs::read_to_string(&config_path).unwrap();
        let injected = config.replacen(
            "defineConfig({",
            &format!("defineConfig({{{config_extra}"),
            1,
        );
        assert_ne!(injected, config, "config template changed shape");
        fs::write(&config_path, injected).unwrap();
    }

    dir
}

/// Write a shipped asset into `dist/` so there is real weight to measure.
fn emit_asset(dir: &tempfile::TempDir, relative: &str, contents: &str) {
    let path = dir.path().join("dist").join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn build_writes_a_bundle_size_report() {
    let dir = built_app("");
    emit_asset(&dir, "assets/app.js", &"export const a = 1;\n".repeat(50));

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("shipped"), "{stdout}");
    assert!(stdout.contains("gzip"), "{stdout}");
    assert!(stdout.contains("brotli"), "{stdout}");
    assert!(stdout.contains("uf-bundle-report.json"), "{stdout}");

    let report = fs::read_to_string(dir.path().join("dist/uf-bundle-report.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["gzipLevel"], 9);
    assert_eq!(parsed["brotliQuality"], 11);
    assert!(!parsed["assets"].as_array().unwrap().is_empty());
    // The report must never measure itself or the other build manifests.
    let paths = parsed["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|asset| asset["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("uf-bundle-report.json")),
        "{paths:?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("uf-build-manifest.json")),
        "{paths:?}"
    );
}

#[test]
fn build_fails_when_a_size_budget_is_exceeded() {
    let dir = built_app("\n  build: { budgets: { total: { max: \"1b\" } } },");
    emit_asset(&dir, "assets/app.js", &"export const a = 1;\n".repeat(200));

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("total budget exceeded"), "{stderr}");
    assert!(stderr.contains("gzip"), "{stderr}");
    assert!(stderr.contains("error: bundle size exceeded"), "{stderr}");
}

#[test]
fn build_passes_a_budget_that_is_large_enough() {
    let dir = built_app(
        "\n  build: { budgets: { total: { max: \"10mb\" }, perAsset: { max: \"10mb\" } } },",
    );
    emit_asset(&dir, "assets/app.js", &"export const a = 1;\n".repeat(200));

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn build_size_report_flag_lists_the_largest_assets() {
    let dir = built_app("");
    emit_asset(
        &dir,
        "assets/large.js",
        &"export const a = 1;\n".repeat(400),
    );
    emit_asset(&dir, "assets/small.js", "export const b = 2;\n");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["build", "--size-report"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let large = stdout
        .find("assets/large.js")
        .expect("the large asset is listed");
    let small = stdout
        .find("assets/small.js")
        .expect("the small asset is listed");

    // Largest first, and the table header names every column.
    assert!(large < small, "{stdout}");
    assert!(stdout.contains("asset"), "{stdout}");
    assert!(stdout.contains("kind"), "{stdout}");
}

#[test]
fn build_without_the_flag_does_not_list_individual_assets() {
    let dir = built_app("");
    emit_asset(&dir, "assets/app.js", "export const a = 1;\n");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("assets/app.js"), "{stdout}");
}

/// A project `uf create` generates must pass `uf lint` without router errors.
///
/// The scaffold emits `_uf.page.native.js` and `_uf.page.test.js`, and
/// `router/reserved-files` used to reject both — so a freshly created project
/// failed its own linter on the first command a user runs after `create`.
#[test]
fn a_scaffolded_app_has_no_reserved_router_file_errors() {
    let dir = tempfile::tempdir().unwrap();

    let created = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["create", "app", "react"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    assert!(dir.path().join("app/_uf.page.native.js").exists());
    assert!(dir.path().join("app/_uf.page.test.js").exists());

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("lint")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let reserved = stdout
        .lines()
        .filter(|line| line.contains("router/reserved-files"))
        .collect::<Vec<_>>();
    assert!(reserved.is_empty(), "{reserved:?}");
}
