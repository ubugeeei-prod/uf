//! `uf check` with Flow's own type inference compiled in.
//!
//! Only built with the `upstream-typecheck` feature; without it `uf check` is
//! the linter alone and `tests/output.rs` already covers that shape.

#![cfg(feature = "upstream-typecheck")]

mod support;

use std::fs;
use std::path::Path;

use serde_json::Value;
use support::{assert_plain, uf};

/// A project whose one file has a type error and nothing for the linter to say.
fn typed_project(dir: &Path) {
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("total.js"),
        "// @flow\nexport const total: number = \"twelve\";\n",
    )
    .unwrap();
}

/// A project that both halves of `uf check` are happy with.
fn clean_project(dir: &Path) {
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("total.js"),
        "// @flow\nexport function add(a: number, b: number): number {\n  return a + b;\n}\n",
    )
    .unwrap();
}

fn check_json(dir: &Path) -> Value {
    let output = uf()
        .arg("--cwd")
        .arg(dir)
        .args(["check", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    serde_json::from_str(&stdout).expect("--json must parse")
}

#[test]
fn check_reports_the_upstream_checker_as_its_backend() {
    let dir = tempfile::tempdir().unwrap();
    clean_project(dir.path());

    let value = check_json(dir.path());

    assert_eq!(value["command"], serde_json::json!("uf check"));
    assert_eq!(
        value["typeCheck"]["backend"],
        serde_json::json!("upstream-flow-rust-port")
    );
    assert_eq!(value["typeCheck"]["status"], serde_json::json!("checked"));
    assert!(value["typeCheck"]["filesChecked"].as_u64().unwrap() >= 1);
    assert!(value["typeCheck"]["builtinsMs"].as_f64().unwrap() > 0.0);
}

#[test]
fn a_clean_project_passes_both_halves_of_the_check() {
    let dir = tempfile::tempdir().unwrap();
    clean_project(dir.path());

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["check", "--color", "never"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("types checked"), "{stdout}");
}

#[test]
fn a_type_error_fails_the_run_and_carries_a_flow_error_code() {
    let dir = tempfile::tempdir().unwrap();
    typed_project(dir.path());

    let value = check_json(dir.path());

    assert_eq!(value["errors"], serde_json::json!(1));
    let diagnostics = value["typeCheck"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0]["code"],
        serde_json::json!("incompatible-type")
    );
    assert_eq!(diagnostics[0]["severity"], serde_json::json!("error"));
    assert_eq!(diagnostics[0]["kind"], serde_json::json!("infer"));
    assert_eq!(
        diagnostics[0]["primary"]["start"]["line"],
        serde_json::json!(2)
    );
    assert_eq!(
        diagnostics[0]["primary"]["path"],
        serde_json::json!("src/total.js")
    );
}

#[test]
fn a_type_error_is_rendered_as_a_code_frame() {
    let dir = tempfile::tempdir().unwrap();
    typed_project(dir.path());

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["check", "--color", "never"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!output.status.success());
    assert!(stdout.contains("error[incompatible-type]"), "{stdout}");
    assert!(stdout.contains("src/total.js:2:"), "{stdout}");
    // The offending line, and a caret under it.
    assert!(stdout.contains("\"twelve\""), "{stdout}");
    assert!(stdout.contains('^'), "{stdout}");
}

#[test]
fn two_runs_of_the_same_project_report_identical_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    typed_project(dir.path());

    let first = check_json(dir.path());
    let second = check_json(dir.path());

    assert_eq!(
        first["typeCheck"]["diagnostics"],
        second["typeCheck"]["diagnostics"]
    );
    assert_eq!(first["diagnostics"], second["diagnostics"]);
    assert_eq!(first["errors"], second["errors"]);
}

#[test]
fn imports_that_uf_cannot_type_yet_are_named_rather_than_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("app.js"),
        "// @flow\nimport { thing } from \"./other.js\";\nexport const used: mixed = thing;\n",
    )
    .unwrap();
    fs::write(src.join("other.js"), "// @flow\nexport const thing = 1;\n").unwrap();

    let value = check_json(dir.path());

    let untyped = value["typeCheck"]["untypedModules"].as_array().unwrap();
    assert!(
        untyped.iter().any(|name| name == "./other.js"),
        "{untyped:?}"
    );
}
