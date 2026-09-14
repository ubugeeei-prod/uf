//! `uf ui`: what it writes into a project, what it refuses to replace, and how
//! it compares a copy with the registry it came from.
//!
//! Every project here already names the packages the components import, so no
//! test reaches a package manager or the network. Adding a package that is
//! missing is `uf add`, which `dependencies.rs` runs; what is left is the part
//! only `uf ui` does, and all of it happens on disk.

mod support;

use std::fs;
use std::path::Path;

use serde_json::Value;
use support::{create_app, uf};

/// The version the stamps in these projects name: this build's.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run `uf` in `dir`, returning its exit code, stdout and stderr.
fn run(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let output = uf()
        .arg("--cwd")
        .arg(dir)
        .args(["--color", "never"])
        .args(args)
        .output()
        .expect("uf started");
    (
        output
            .status
            .code()
            .expect("uf exited rather than being killed"),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A scaffolded application whose manifest already names every package the
/// registry's components import.
fn app() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    create_app(dir.path());
    let manifest_path = dir.path().join("package.json");
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("the manifest"))
            .expect("the manifest is JSON");
    let dependencies = manifest["dependencies"]
        .as_object_mut()
        .expect("the scaffold declares dependencies");
    for package in ["@uniflowed/stylex", "@uniflowed/ui"] {
        dependencies.insert(package.to_owned(), Value::String(VERSION.to_owned()));
    }
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("serialises"),
    )
    .expect("the manifest is written");
    dir
}

fn component(dir: &Path, name: &str) -> std::path::PathBuf {
    dir.join("app/components/ui").join(format!("{name}.js"))
}

#[test]
fn add_writes_the_component_and_the_components_it_imports() {
    let dir = app();
    let (code, stdout, stderr) = run(dir.path(), &["ui", "add", "dialog"]);
    assert_eq!(code, 0, "{stdout}{stderr}");

    let dialog =
        fs::read_to_string(component(dir.path(), "dialog")).expect("dialog.js was written");
    let button =
        fs::read_to_string(component(dir.path(), "button")).expect("button.js was written");
    assert!(dialog.contains("from \"./button.js\""), "{dialog}");
    assert!(
        dialog
            .lines()
            .last()
            .is_some_and(|line| line.starts_with(&format!(
                "// Written by `uf ui add dialog` from uf {VERSION}, sha256 "
            ))),
        "the last line of dialog.js is not its stamp:\n{dialog}"
    );
    assert!(
        button.contains("// Written by `uf ui add button`"),
        "{button}"
    );

    assert!(stdout.contains("app/components/ui/dialog.js"), "{stdout}");
    assert!(stdout.contains("app/components/ui/button.js"), "{stdout}");
    assert!(stdout.contains("wrote 2 files"), "{stdout}");
}

#[test]
fn adding_again_writes_nothing() {
    let dir = app();
    assert_eq!(run(dir.path(), &["ui", "add", "tabs"]).0, 0);
    let before = fs::read_to_string(component(dir.path(), "tabs")).expect("tabs.js");

    let (code, stdout, stderr) = run(dir.path(), &["ui", "add", "tabs"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("nothing to write"), "{stdout}");
    assert_eq!(
        fs::read_to_string(component(dir.path(), "tabs")).expect("tabs.js"),
        before
    );
}

/// The refusal the issue asks for: the file is named, the way to overwrite is
/// said, and nothing — not even the component that had no conflict — is written.
#[test]
fn an_edited_component_is_refused_by_name_and_nothing_is_written() {
    let dir = app();
    assert_eq!(run(dir.path(), &["ui", "add", "select"]).0, 0);
    let path = component(dir.path(), "select");
    let edited = fs::read_to_string(&path)
        .expect("select.js")
        .replace("minWidth: \"12rem\"", "minWidth: \"16rem\"");
    fs::write(&path, &edited).expect("an edit");

    let (code, stdout, stderr) = run(dir.path(), &["ui", "add", "tabs", "select"]);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert!(
        stderr.contains("app/components/ui/select.js has changed since `uf ui add` wrote it"),
        "{stderr}"
    );
    assert!(stderr.contains("uf ui diff select"), "{stderr}");
    assert!(stderr.contains("uf ui add select --overwrite"), "{stderr}");
    assert!(stderr.contains("nothing was written"), "{stderr}");

    assert!(
        !component(dir.path(), "tabs").exists(),
        "a refused run wrote tabs.js"
    );
    assert_eq!(fs::read_to_string(&path).expect("select.js"), edited);
}

#[test]
fn overwrite_replaces_the_edit_with_this_ufs_version() {
    let dir = app();
    assert_eq!(run(dir.path(), &["ui", "add", "button"]).0, 0);
    let path = component(dir.path(), "button");
    let fresh = fs::read_to_string(&path).expect("button.js");
    fs::write(&path, fresh.replace("\"36px\"", "\"40px\"")).expect("an edit");

    let (code, stdout, stderr) = run(dir.path(), &["ui", "add", "button", "--overwrite"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("replaced"), "{stdout}");
    assert_eq!(fs::read_to_string(&path).expect("button.js"), fresh);
}

/// A file the project wrote itself is refused when it is asked for, and kept
/// when it is only needed: it still satisfies `./button.js`.
#[test]
fn a_file_uf_ui_add_did_not_write_is_refused_when_named_and_kept_when_needed() {
    let dir = app();
    let path = component(dir.path(), "button");
    fs::create_dir_all(path.parent().expect("a directory")).expect("the directory");
    let own = "// @flow\nexport component Button(children: React.Node) {\n  return <button type=\"button\">{children}</button>;\n}\n";
    fs::write(&path, own).expect("the project's own button");

    let (code, _, stderr) = run(dir.path(), &["ui", "add", "button"]);
    assert_eq!(code, 1, "{stderr}");
    assert!(
        stderr.contains("was not written by `uf ui add`"),
        "{stderr}"
    );

    let (code, stdout, stderr) = run(dir.path(), &["ui", "add", "dialog"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("kept: not written by uf ui add"),
        "{stdout}"
    );
    assert_eq!(fs::read_to_string(&path).expect("button.js"), own);
    assert!(component(dir.path(), "dialog").exists());
}

#[test]
fn diff_says_an_edit_is_the_projects_own_and_shows_it() {
    let dir = app();
    assert_eq!(run(dir.path(), &["ui", "add", "tabs"]).0, 0);
    let path = component(dir.path(), "tabs");
    let text = fs::read_to_string(&path).expect("tabs.js");
    fs::write(
        &path,
        text.replace("gap: ufTokens.space4,", "gap: ufTokens.space6,"),
    )
    .expect("an edit");

    let (code, stdout, stderr) = run(dir.path(), &["ui", "diff", "tabs"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains("every line below is this project's"),
        "{stdout}"
    );
    assert!(stdout.contains("-    gap: ufTokens.space4,"), "{stdout}");
    assert!(stdout.contains("+    gap: ufTokens.space6,"), "{stdout}");

    let (code, stdout, stderr) = run(dir.path(), &["ui", "diff", "tabs", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("--json is only JSON");
    let tabs = &report["components"][0];
    assert_eq!(tabs["name"], "tabs");
    assert_eq!(tabs["state"], "edited");
    assert_eq!(tabs["registryMoved"], false);
    assert_eq!(tabs["from"], VERSION);
    assert!(
        tabs["diff"]
            .as_str()
            .is_some_and(|diff| diff.contains("+    gap: ufTokens.space6,")),
        "{stdout}"
    );
}

#[test]
fn diff_of_an_untouched_copy_says_it_is_the_same() {
    let dir = app();
    assert_eq!(run(dir.path(), &["ui", "add", "button"]).0, 0);
    let (code, stdout, stderr) = run(dir.path(), &["ui", "diff"]);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(
        stdout.contains(&format!("the same as uf {VERSION}'s version")),
        "{stdout}"
    );
}

#[test]
fn diff_with_nothing_added_says_so_and_diff_of_a_missing_component_says_how_to_add_it() {
    let dir = app();
    let (code, stdout, _) = run(dir.path(), &["ui", "diff"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("no component has been added"), "{stdout}");

    let (code, _, stderr) = run(dir.path(), &["ui", "diff", "dialog"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("uf ui add dialog"), "{stderr}");
}

#[test]
fn list_says_what_the_registry_has_and_what_the_project_added() {
    let dir = app();
    assert_eq!(run(dir.path(), &["ui", "add", "dialog"]).0, 0);

    let (code, stdout, stderr) = run(dir.path(), &["ui", "list", "--json"]);
    assert_eq!(code, 0, "{stderr}");
    let report: Value = serde_json::from_str(&stdout).expect("--json is only JSON");
    assert_eq!(report["version"], VERSION);
    assert_eq!(report["directory"], "app/components/ui");
    let state = |name: &str| {
        report["components"]
            .as_array()
            .expect("a list")
            .iter()
            .find(|each| each["name"] == name)
            .unwrap_or_else(|| panic!("no {name} in {stdout}"))["state"]
            .clone()
    };
    assert_eq!(state("dialog"), "current");
    assert_eq!(state("button"), "current");
    assert_eq!(state("tabs"), "missing");

    let (code, stdout, _) = run(dir.path(), &["ui", "list"]);
    assert_eq!(code, 0);
    for name in ["button", "dialog", "select", "tabs"] {
        assert!(stdout.contains(name), "{stdout}");
    }
}

#[test]
fn an_unknown_name_is_refused_with_the_closest_names() {
    let dir = app();
    let (code, _, stderr) = run(dir.path(), &["ui", "add", "buton"]);
    assert_eq!(code, 1);
    assert!(stderr.contains("no component named `buton`"), "{stderr}");
    assert!(stderr.contains("did you mean: button"), "{stderr}");
    assert!(!dir.path().join("app/components").exists());
}

/// What `uf ui add` writes is code `uf fmt --check` and `uf lint` accept as it
/// stands. A command that wrote files the next command in the same toolchain
/// reported would be teaching that the report is noise.
#[test]
fn what_uf_ui_add_writes_is_formatted_and_lints_clean() {
    let dir = app();
    let (code, stdout, stderr) = run(
        dir.path(),
        &["ui", "add", "button", "dialog", "tabs", "select"],
    );
    assert_eq!(code, 0, "{stdout}{stderr}");

    let (code, stdout, stderr) = run(dir.path(), &["fmt", "--check", "app/components"]);
    assert_eq!(
        code, 0,
        "uf fmt --check rejects what uf ui add wrote:\n{stdout}{stderr}"
    );
    let (code, stdout, stderr) = run(dir.path(), &["lint", "app/components"]);
    assert_eq!(
        code, 0,
        "uf lint rejects what uf ui add wrote:\n{stdout}{stderr}"
    );
}
