//! `uf env`: the per-repository toolchain.
//!
//! Nothing here reaches the network. Acquiring a real Node is exercised by
//! hand and by `uf_env`'s own tests over a staged directory; what these check
//! is the part a user meets — what the commands say, and that the store and
//! the roots are the ones the environment points them at rather than the
//! machine's.

mod support;

use std::fs;

use support::{assert_plain, uf};

/// A project with a toolchain, and a store and roots of its own.
fn project(dir: &std::path::Path, toolchain: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    fs::write(
        dir.join("uf.config.js"),
        format!(
            "// @flow\nimport {{ defineConfig }} from \"@uniflowed/config\";\n\n\
             export default defineConfig({{ env: {{ toolchain: {toolchain} }} }});\n"
        ),
    )
    .unwrap();
    (dir.join("store"), dir.join("roots"))
}

#[test]
fn env_list_reports_what_is_pinned_and_what_is_installed() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ node: "24.14.0" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "list"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("node@24.14.0"), "{stdout}");
    assert!(stdout.contains("missing"), "{stdout}");
    assert!(stdout.contains("the store"), "{stdout}");
    assert_plain(&stdout);
}

/// A project that pins nothing is not an error. That is every project today,
/// and `uf env install` turning into a failure for them would be a change
/// nobody asked for.
#[test]
fn a_project_that_pins_nothing_is_told_so_and_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), "{}");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "install"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("declares no toolchain"), "{stdout}");
    assert!(!dir.path().join(".uniflowed/env/bin").exists());
}

/// A range is refused, and the message says which tool and what was written.
#[test]
fn a_version_that_is_not_exact_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ node: "^24" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "install"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("node"), "{stderr}");
    assert!(stderr.contains("not an exact version"), "{stderr}");
}

/// A name uf does not install is refused with the list of names it does.
#[test]
fn a_tool_uf_does_not_install_is_refused_with_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ cargo: "1.0.0" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "install"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("node, bun, deno, npm, pnpm and yarn"),
        "{stderr}"
    );
}

/// Collection on an empty store says so rather than reporting a count of
/// nothing as if it had worked.
#[test]
fn gc_on_an_empty_store_says_there_is_nothing_to_collect() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), "{}");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "gc", "--dry-run"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("nothing to collect"), "{stdout}");
    assert_plain(&stdout);
}

/// `uf env exec` before `uf env install` names the command that fixes it.
#[test]
fn exec_without_an_environment_names_the_command_that_makes_one() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ node: "24.14.0" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "exec", "--", "node", "--version"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("uf env install"), "{stderr}");
}
