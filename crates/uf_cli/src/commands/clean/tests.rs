//! What `uf clean` removes, and — mostly — what it does not.

use std::fs;

use super::*;

/// A project with a build output, a cache, an install and a lockfile.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
    for (path, contents) in [
        ("uf.config.js", "// @flow\nexport default {};\n"),
        ("package.json", "{ \"name\": \"c\", \"private\": true }\n"),
        ("package-lock.json", "{}\n"),
        ("uf.lock", "{}\n"),
        ("app.js", "// @flow\nexport const a = 1;\n"),
        ("dist/index.html", "<!doctype html>\n"),
        ("dist/docs/index.html", "<!doctype html>\n"),
        (".uf/cache/check/one.json", "{}\n"),
        ("node_modules/left-pad/index.js", "module.exports = 1;\n"),
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        fs::write(&file, contents).expect("a file");
    }
    dir
}

fn root_of(dir: &tempfile::TempDir) -> &Utf8Path {
    Utf8Path::from_path(dir.path()).expect("a UTF-8 path")
}

fn run(root: &Utf8Path, deps: bool, dry_run: bool) {
    // JSON mode, because these tests are about the filesystem and the human
    // render would otherwise print a table per assertion.
    let mut ui = Ui::new(uf_term::ColorChoice::Never, crate::ui::OutputMode::Json);
    clean(root, &mut ui, deps, dry_run).expect("clean runs");
}

#[test]
fn the_build_output_and_ufs_own_state_go() {
    let dir = project();
    let root = root_of(&dir);

    run(root, false, false);

    assert!(!root.join("dist").exists(), "dist survived");
    assert!(!root.join(".uf").exists(), ".uf survived");
}

/// The line is the network: everything else can be produced from this
/// checkout, and `node_modules` cannot.
#[test]
fn the_install_stays_unless_it_is_asked_for() {
    let dir = project();
    let root = root_of(&dir);

    run(root, false, false);
    assert!(root.join("node_modules").is_dir(), "node_modules went");

    run(root, true, false);
    assert!(!root.join("node_modules").exists(), "--deps left it");
}

/// A lockfile is an input. No flag removes one, because the next install must
/// resolve what the last one did.
#[test]
fn no_flag_removes_a_lockfile() {
    let dir = project();
    let root = root_of(&dir);

    run(root, true, false);

    assert!(root.join("uf.lock").is_file(), "uf.lock went");
    assert!(
        root.join("package-lock.json").is_file(),
        "package-lock.json went"
    );
    assert!(root.join("app.js").is_file(), "a source file went");
}

#[test]
fn a_dry_run_removes_nothing() {
    let dir = project();
    let root = root_of(&dir);

    run(root, true, true);

    assert!(root.join("dist").is_dir());
    assert!(root.join(".uf").is_dir());
    assert!(root.join("node_modules").is_dir());
}

/// `dist/docs` is inside `dist`, so it is measured once rather than twice —
/// the size in the table is what the reader decides on.
#[test]
fn a_nested_output_directory_is_counted_once() {
    let dir = project();
    let root = root_of(&dir);
    let mut targets = Vec::new();

    push(&mut targets, root, "dist", "a rebuild");
    push(&mut targets, root, "dist/docs", "a docs rebuild");

    assert_eq!(targets.len(), 1, "dist/docs was counted under its own name");
    assert_eq!(targets[0].files, 2, "both files are under dist");
}

/// And a project with nothing to remove says so rather than failing.
#[test]
fn a_clean_project_is_not_an_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = root_of(&dir);
    fs::write(root.join("uf.config.js"), "// @flow\nexport default {};\n").expect("a config");
    fs::write(root.join("package.json"), "{ \"name\": \"c\" }\n").expect("a manifest");

    run(root, true, false);
}
