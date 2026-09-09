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
fn a_second_run_is_answered_from_the_cache_under_the_project_root() {
    let dir = tempfile::tempdir().unwrap();
    typed_project(dir.path());

    let first = check_json(dir.path());
    let second = check_json(dir.path());

    assert_eq!(first["typeCheck"]["filesFromCache"], serde_json::json!(0));
    assert_eq!(
        second["typeCheck"]["filesFromCache"], first["typeCheck"]["filesChecked"],
        "every file the first run checked is answered by the second"
    );
    // Everything a reader is told about the check, other than how long it took
    // and how much of it was avoided, has to be the same both times.
    for field in [
        "diagnostics",
        "filesChecked",
        "filesSkipped",
        "untypedModules",
    ] {
        assert_eq!(
            first["typeCheck"][field], second["typeCheck"][field],
            "typeCheck.{field} differs between the run that filled the cache and the run served from it"
        );
    }
    assert!(
        dir.path().join(".uf/cache/check").is_dir(),
        "the cache belongs under `.uf/`, which `.gitignore` already covers"
    );
}

#[test]
fn editing_a_dependency_rechecks_what_imports_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("app.js"),
        "// @flow\nimport type { Mode } from \"./mode.js\";\nexport const mode: Mode = \"onSubmit\";\n",
    )
    .unwrap();
    fs::write(
        src.join("mode.js"),
        "// @flow\nexport type Mode = \"onSubmit\" | \"onChange\";\n",
    )
    .unwrap();
    let clean = check_json(dir.path());
    assert_eq!(clean["errors"], serde_json::json!(0), "{clean}");

    // `app.js` is not touched. The type it is checked against is.
    fs::write(
        src.join("mode.js"),
        "// @flow\nexport type Mode = \"onChange\";\n",
    )
    .unwrap();
    let after = check_json(dir.path());

    assert_eq!(after["typeCheck"]["filesFromCache"], serde_json::json!(0));
    let codes: Vec<&str> = after["typeCheck"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap_or("<none>"))
        .collect();
    assert_eq!(codes, ["incompatible-type"], "{after}");
}

#[test]
fn a_file_in_the_project_is_typed_by_the_file_it_imports_from() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("app.js"),
        "// @flow\nimport type { Mode } from \"./mode.js\";\n         export const mode: Mode = \"onSubmit\";\nexport const wrong: Mode = \"never\";\n",
    )
    .unwrap();
    fs::write(
        src.join("mode.js"),
        "// @flow\nexport type Mode = \"onSubmit\" | \"onChange\";\n",
    )
    .unwrap();

    let value = check_json(dir.path());

    let untyped = value["typeCheck"]["untypedModules"].as_array().unwrap();
    assert!(
        untyped.is_empty(),
        "`./mode.js` is a file in this project, not a hole: {untyped:?}"
    );
    // The import is a type, so the correct annotation is silent and only the
    // wrong one is reported. Before uf resolved across modules this file
    // produced three `value-as-type` errors and no real finding.
    let codes: Vec<&str> = value["typeCheck"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap_or("<none>"))
        .collect();
    assert_eq!(codes, ["incompatible-type"], "{value}");
}

#[test]
fn a_workspace_package_is_typed_through_the_manifest_that_publishes_it() {
    // The end-to-end half of ubugeeei-prod/uf#248: that `uf check` really does
    // hand the checker the `package.json` it resolves a package name through.
    // The library tests state a batch directly and so cannot see this; only a
    // project on disk can.
    let dir = tempfile::tempdir().unwrap();
    let package = dir.path().join("packages/cell");
    fs::create_dir_all(&package).unwrap();
    fs::write(
        package.join("package.json"),
        "{\n  \"name\": \"@uniflowed/cell\",\n  \"exports\": { \".\": \"./index.js\" }\n}\n",
    )
    .unwrap();
    fs::write(
        package.join("index.js"),
        "// @flow\nexport type Cell<T> = { readonly read: () => T };\n",
    )
    .unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("app.js"),
        "// @flow\nimport type { Cell } from \"@uniflowed/cell\";\n\
         export const bad: Cell<number> = 1;\n",
    )
    .unwrap();

    let value = check_json(dir.path());

    let untyped = value["typeCheck"]["untypedModules"].as_array().unwrap();
    assert!(
        untyped.is_empty(),
        "this project publishes `@uniflowed/cell`, so it is not a hole: {untyped:?}"
    );
    // The type is real, so the wrong value is the only finding. Before the
    // manifest was read this file produced one `value-as-type` and nothing
    // about the `1`.
    let codes: Vec<&str> = value["typeCheck"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["incompatible-type"], "{value}");
}

#[test]
fn imports_that_uf_cannot_type_yet_are_named_rather_than_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    // A package name no manifest in this project publishes, so it resolves
    // through `node_modules`, which the checker is not handed — and a relative
    // path to a file the scan never walked. Both are `any`, and both have to be
    // said out loud.
    fs::write(
        src.join("app.js"),
        "// @flow\nimport { thing } from \"some-package\";\n         import { other } from \"./generated/table.js\";\n         export const used: mixed = [thing, other];\n",
    )
    .unwrap();

    let value = check_json(dir.path());

    let untyped = value["typeCheck"]["untypedModules"].as_array().unwrap();
    assert!(
        untyped.iter().any(|name| name == "some-package"),
        "{untyped:?}"
    );
    assert!(
        untyped.iter().any(|name| name == "./generated/table.js"),
        "{untyped:?}"
    );
}

/// A hand-written library definition is full of `any`, and that is what one is
/// *for*: the point of `declare module "editor-pkg"` is to describe a package
/// that ships no types, and every member uf cannot transcribe is an `any` on
/// purpose. `flow/unclear-type` fired on each of them, because the scan
/// collects `flow-typed/` like any other directory and the libdef was handed
/// to the linter as well as to the type environment.
///
/// So `files checked` counted a file `flow check` would never lint, and a
/// project whose types were right failed its build over the shape its
/// declarations have to have. ubugeeei-prod/uf#699.
#[test]
fn a_library_definition_is_merged_rather_than_linted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join(".flowconfig"), "[libs]\nflow-typed\n").unwrap();
    let libs = root.join("flow-typed");
    fs::create_dir_all(&libs).unwrap();
    fs::write(
        libs.join("editor.js"),
        "// @flow\ndeclare module \"editor-pkg\" {\n  declare export namespace languages {\n    declare type FoldingRangeProvider = any;\n  }\n}\n",
    )
    .unwrap();
    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("probe.js"),
        "// @flow\nimport * as Editor from \"editor-pkg\";\n\nexport const provider: Editor.languages.FoldingRangeProvider = null;\n",
    )
    .unwrap();

    let value = check_json(root);

    let rules: Vec<&str> = value["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["rule"].as_str().unwrap())
        .collect();
    assert!(rules.is_empty(), "the libdef was linted: {rules:?}");
    // The other half of the same fact, and the one a reader sees: a libdef is
    // not among the files the run says it checked. Without it the assertion
    // above could pass because the rule stopped firing rather than because the
    // file stopped being linted.
    assert_eq!(value["filesChecked"], serde_json::json!(1), "{value}");
    // And it is still *merged*, which is the difference between not linting a
    // libdef and not reading one: the annotation resolves, so nothing about
    // `Editor.languages.FoldingRangeProvider` is reported.
    let types: Vec<&str> = value["typeCheck"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap_or("<none>"))
        .collect();
    assert!(types.is_empty(), "{types:?}");
    assert_eq!(
        value["typeCheck"]["libdefs"],
        serde_json::json!(1),
        "{value}"
    );
}

/// Naming one says what it is, rather than "no file matched" about a file the
/// reader is looking at.
#[test]
fn asking_to_lint_a_library_definition_says_that_is_what_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join(".flowconfig"), "[libs]\nflow-typed\n").unwrap();
    let libs = root.join("flow-typed");
    fs::create_dir_all(&libs).unwrap();
    fs::write(libs.join("globals.js"), "declare type Kind = string;\n").unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["check", "flow-typed/globals.js"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("flow-typed/globals.js is a library definition"),
        "{stderr}"
    );
}
