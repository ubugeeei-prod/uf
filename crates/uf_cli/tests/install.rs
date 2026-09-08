//! `uf install` against a real package manager.
//!
//! Every test here runs npm. None of them needs the network: a project with no
//! dependencies is still a whole install as far as npm is concerned, and the
//! one test that wants a failure gets it from a package name no registry will
//! ever answer for — which fails the same way with no network at all.
//!
//! What is being defended is the part that cannot be unit-tested: that reading
//! npm's output instead of letting it inherit the terminal did not change what
//! reaches a pipe. `uf install` now draws a redrawn region while it works, and
//! a redrawn region in a CI log is a screenful of escape sequences.

mod support;

use std::fs;
use std::path::Path;

use support::{assert_plain, uf};

/// A project npm can install: a config, and a manifest with nothing in it.
fn project(dir: &Path, dependencies: &str) {
    fs::write(
        dir.join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\
         export default defineConfig({ app: { router: { enabled: false } } });\n",
    )
    .unwrap();
    fs::write(
        dir.join("package.json"),
        format!(
            "{{\n  \"name\": \"install-fixture\",\n  \"version\": \"1.0.0\",\n  \
             \"dependencies\": {{{dependencies}}}\n}}\n"
        ),
    )
    .unwrap();
}

/// Run `uf install` in `dir`, returning `(stdout, stderr, success)`.
fn install(dir: &Path) -> (String, String, bool) {
    let output = uf().current_dir(dir).arg("install").output().unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

/// The line whose first word is `key`, from a key/value block.
fn row<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .map(str::trim_start)
        .find(|line| line.starts_with(&format!("{key} ")))
        .unwrap_or_else(|| panic!("`uf install` renders a `{key}` row, got:\n{text}"))
}

/// Neither stream may carry an escape sequence when neither is a terminal.
///
/// This is the whole reason `uf_term::Live` asks about the capability instead
/// of drawing: a cursor movement per package would make a CI log unreadable,
/// and it would do it silently.
#[test]
fn a_pipe_gets_no_escape_sequence_from_either_stream() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "");

    let (stdout, stderr, success) = install(dir.path());

    assert!(success, "{stderr}");
    assert_plain(&stdout);
    assert_plain(&stderr);
    // Neither the region's own movements nor a spinner frame.
    assert!(!stdout.contains("\x1b[") && !stderr.contains("\x1b["));
    assert!(!stdout.contains('\r'), "no line was rewritten in place");
}

#[test]
fn the_summary_says_which_manager_ran_and_what_it_wrote() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "");

    let (stdout, stderr, success) = install(dir.path());

    assert!(success, "{stderr}");
    assert!(stdout.contains("uf install"), "{stdout}");
    assert_eq!(row(&stdout, "manager"), "manager    npm");
    assert!(
        row(&stdout, "command").contains("--ignore-scripts"),
        "the refusal to run lifecycle scripts is part of the report:\n{stdout}"
    );
    assert!(
        row(&stdout, "lockfile").contains("package-lock.json"),
        "{stdout}"
    );
    // `install_workspace` writes `uf.lock` a step earlier, so by the time the
    // manager is detected there *is* evidence and it names uf — whose resolver
    // cannot fetch. The old report said "no lockfile or packageManager field"
    // here, which was not true of any project that had run `uf install` once.
    assert_eq!(
        row(&stdout, "chosen by"),
        "chosen by  uf.lock names uf, whose resolver cannot fetch yet"
    );
}

/// The second run of the day, which is most of them.
#[test]
fn an_install_that_changes_nothing_says_so_in_one_line() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "");

    let (first, stderr, success) = install(dir.path());
    assert!(success, "{stderr}");
    let (second, stderr, success) = install(dir.path());
    assert!(success, "{stderr}");

    assert!(
        second.contains("already up to date"),
        "the lockfile did not move, so there is nothing to report:\n{second}"
    );
    assert!(
        !second.contains("next steps"),
        "a report about nothing happening is noise:\n{second}"
    );
    // The first run created the lockfile, so it is not the same screen.
    assert!(first.contains("package-lock.json"));
}

/// npm's own words survive, which is the entire point of a failure.
#[test]
fn a_failed_install_still_says_why_in_the_manager_s_words() {
    let dir = tempfile::tempdir().unwrap();
    // No registry answers for this, with or without a network.
    project(
        dir.path(),
        "\"@uf-does-not-exist/nothing-is-published-here\": \"^99.0.0\"",
    );

    let (stdout, stderr, success) = install(dir.path());

    assert!(!success, "this install cannot succeed:\n{stdout}{stderr}");
    assert!(
        stderr.contains("npm error") || stderr.contains("npm ERR!"),
        "npm's own diagnosis must reach the terminal:\n{stderr}"
    );
    assert!(
        stderr.contains("error:"),
        "and uf's own failure line with it:\n{stderr}"
    );
    assert_plain(&stderr);
}

/// The lines uf asked npm for are the only lines uf keeps.
#[test]
fn the_verbosity_uf_asked_for_is_not_printed_back_at_the_reader() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "");

    let (stdout, stderr, _) = install(dir.path());

    for stream in [&stdout, &stderr] {
        assert!(
            !stream.contains("npm http "),
            "`--loglevel=http` is uf's own request and belongs in the region, \
             not in the log:\n{stream}"
        );
    }
    assert!(
        row(&stdout, "command").contains("--loglevel"),
        "and the report says uf asked for it:\n{stdout}"
    );
}

/// A project that binds `@company` to a registry only it publishes to.
fn bound_scope_project(dir: &Path) {
    fs::write(
        dir.join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\
         export default defineConfig({\n  \
           app: { router: { enabled: false } },\n  \
           pm: { scopes: { \"@company\": \"https://npm.company.example\" } },\n\
         });\n",
    )
    .unwrap();
    fs::write(
        dir.join("package.json"),
        "{\n  \"name\": \"confusion-fixture\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .unwrap();
}

/// Dependency confusion, end to end: a lockfile that resolves a bound scope
/// from the public registry must not be installed from.
///
/// ubugeeei-prod/uf#553. The refusal happens before npm is spawned — there is
/// no network in this test and none is needed, because the evidence is in the
/// lockfile the repository was cloned with. That is the point: the attack has
/// already succeeded on whichever machine wrote this file, and installing from
/// it is letting it succeed again here.
#[test]
fn an_install_refuses_a_lockfile_that_resolves_a_bound_scope_elsewhere() {
    let dir = tempfile::tempdir().unwrap();
    bound_scope_project(dir.path());
    // The attacker's copy, on the registry anybody may publish to.
    fs::write(
        dir.path().join("package-lock.json"),
        r#"{
  "name": "confusion-fixture",
  "version": "1.0.0",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "confusion-fixture", "version": "1.0.0" },
    "node_modules/@company/internal-thing": {
      "version": "9.9.9",
      "resolved": "https://registry.npmjs.org/@company/internal-thing/-/internal-thing-9.9.9.tgz",
      "integrity": "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
    }
  }
}
"#,
    )
    .unwrap();

    let (stdout, stderr, success) = install(dir.path());

    assert!(!success, "this install must be refused:\n{stdout}{stderr}");
    assert!(
        stderr.contains("@company/internal-thing"),
        "the package has to be named:\n{stderr}"
    );
    assert!(
        stderr.contains("https://npm.company.example"),
        "and the registry the scope is bound to:\n{stderr}"
    );
    assert!(
        stderr.contains("registry.npmjs.org"),
        "and the one that actually answered:\n{stderr}"
    );
    assert!(
        stderr.contains("@scope:registry"),
        "and what to do about it:\n{stderr}"
    );
    // Nothing was installed: the refusal is before the manager runs.
    assert!(!dir.path().join("node_modules").exists(), "{stderr}");
    assert_plain(&stderr);
}

/// The same project, resolving from the registry it bound: an ordinary install.
///
/// The other half of the check, and the one that keeps it from being a
/// refusal of every scoped package. The lockfile names the bound registry, so
/// nothing is found and npm is allowed to run.
#[test]
fn a_bound_scope_resolved_from_its_own_registry_is_not_refused() {
    let dir = tempfile::tempdir().unwrap();
    bound_scope_project(dir.path());
    fs::write(
        dir.path().join("package-lock.json"),
        r#"{
  "name": "confusion-fixture",
  "version": "1.0.0",
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "confusion-fixture", "version": "1.0.0" },
    "node_modules/@company/internal-thing": {
      "version": "1.0.0",
      "resolved": "https://npm.company.example/@company/internal-thing/-/internal-thing-1.0.0.tgz",
      "integrity": "sha512-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
    }
  }
}
"#,
    )
    .unwrap();

    let (stdout, stderr, success) = install(dir.path());

    assert!(
        !stderr.contains("scope is not bound to"),
        "a package from the registry its scope names is not a finding:\n{stderr}"
    );
    assert!(success, "and the install runs:\n{stdout}{stderr}");
}
