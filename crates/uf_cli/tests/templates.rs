//! Every template `uf new` writes, through the chain a project runs on it:
//! formatted, linted, type checked, tested and built.
//!
//! A template is the first uf code anybody reads and the first they run, so
//! "it scaffolds" is not the claim worth testing — "what it scaffolds passes
//! uf's own checks and builds" is. Each scaffold goes under the repository's
//! `.uf/`, like every fixture that imports `@uniflowed/*`, so its dependencies
//! resolve to this checkout's packages rather than to a registry. The tests
//! need Node and the workspace installed, and skip only where
//! `UF_ALLOW_FIXTURE_SKIP` says a machine cannot run them; CI sets nothing, so
//! there they always run. ubugeeei-prod/uf#975.

mod support;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use support::{Project, uf, uf_path};

/// What every template has to pass before it is built.
const CHECKS: [&[&str]; 4] = [&["fmt", "--check"], &["lint"], &["check"], &["test"]];

/// Whether a project can be built here: Node on PATH and the workspace
/// installed.
fn ready() -> bool {
    let mut missing = Vec::new();
    if !Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        missing.push("`node` is not on PATH".to_owned());
    }
    let driver =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../node_modules/@uniflowed/vite/driver.js");
    if !driver.is_file() {
        missing.push(format!("{} does not exist; run `npm ci`", driver.display()));
    }
    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "a template cannot be built here, so this test would prove nothing: {}",
        missing.join("; ")
    );
    eprintln!("skipping: {}", missing.join("; "));
    false
}

/// `PATH` with this build's `uf` first, so a task whose command is `uf …`
/// runs the binary under test rather than whichever `uf` the machine has.
fn path_with_uf() -> OsString {
    let binary = PathBuf::from(uf_path());
    let mut entries = vec![
        binary
            .parent()
            .expect("the binary is in a directory")
            .to_path_buf(),
    ];
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(entries).expect("PATH entries join")
}

/// Run `uf args` in `dir`, failing the test with everything it printed.
fn step(dir: &Path, args: &[&str]) {
    let output = uf()
        .arg("--cwd")
        .arg(dir)
        .args(["--color", "never"])
        .args(args)
        .env("PATH", path_with_uf())
        .output()
        .expect("uf started");
    assert!(
        output.status.success(),
        "`uf {}` failed in {}:\n{}{}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `uf new <scratch>/scaffold` with `args` after it. The scratch directory is
/// returned too, because the scaffold is removed with it.
fn scaffold(args: &[&str]) -> (Project, PathBuf) {
    let scratch = Project::new(&[]);
    let dir = scratch.path().join("scaffold");
    let target = dir.to_str().expect("a UTF-8 path").to_owned();
    let mut new = vec!["new", target.as_str()];
    new.extend_from_slice(args);
    step(scratch.path(), &new);
    (scratch, dir)
}

/// The checks, then `build`.
fn chain(dir: &Path, build: &[&str]) {
    for args in CHECKS {
        step(dir, args);
    }
    step(dir, build);
}

#[test]
fn the_react_template_is_formatted_linted_checked_tested_and_built() {
    if !ready() {
        return;
    }
    let (_scratch, dir) = scaffold(&["--name", "template-react"]);

    chain(&dir, &["build"]);

    assert!(
        dir.join("dist/index.html").is_file(),
        "the build wrote no page"
    );
}

#[test]
fn the_library_template_is_formatted_linted_checked_tested_and_built() {
    if !ready() {
        return;
    }
    let (_scratch, dir) = scaffold(&["--lib", "--name", "template-lib"]);

    chain(&dir, &["build"]);

    assert!(dir.join("dist").is_dir(), "the build wrote nothing");
}

/// The checks run once, at the root, over both packages; the build is the
/// root's `build` task, which builds the library before the application that
/// bundles it.
#[test]
fn the_monorepo_template_is_checked_at_its_root_and_built_across_its_packages() {
    if !ready() {
        return;
    }
    let (_scratch, dir) = scaffold(&["monorepo", "--name", "acme"]);
    for file in [
        "package.json",
        "uf.config.js",
        "apps/web/uf.config.js",
        "apps/web/app/$page.js",
        "packages/ui/uf.config.js",
        "packages/ui/index.test.js",
    ] {
        assert!(dir.join(file).is_file(), "the template wrote no {file}");
    }
    // What a workspace install does for the package the application imports:
    // links it under the root's `node_modules`. Done by hand, because the
    // install would fetch `@uniflowed/*` from a registry instead of resolving
    // this checkout's packages.
    let scope = dir.join("node_modules/@acme");
    std::fs::create_dir_all(&scope).expect("the scope directory");
    #[cfg(unix)]
    std::os::unix::fs::symlink("../../packages/ui", scope.join("ui")).expect("the workspace link");

    chain(&dir, &["run", "build"]);

    assert!(
        dir.join("packages/ui/dist").is_dir(),
        "the library was not built"
    );
    assert!(
        dir.join("apps/web/dist/index.html").is_file(),
        "the application was not built"
    );
}

#[test]
fn a_word_that_is_not_a_template_is_refused_with_the_templates_named() {
    let scratch = Project::new(&[]);

    let output = uf()
        .arg("--cwd")
        .arg(scratch.path())
        .args(["new", "site", "monorepoo"])
        .output()
        .expect("uf started");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("react, monorepo"), "{stderr}");
    assert!(
        !scratch.path().join("site").exists(),
        "it scaffolded anyway"
    );
}
