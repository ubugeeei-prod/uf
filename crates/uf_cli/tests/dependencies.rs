//! `uf add`, `uf remove`, `uf update`, `uf patch`, `uf why` and `uf install
//! --frozen-lockfile`, against a real package manager.
//!
//! Every test here runs npm, and none of them needs the network. A dependency
//! written as a path — `./vendor/tiny` — is a whole install as far as npm is
//! concerned: it resolves it, writes it into `package.json`, writes it into
//! `package-lock.json`, and links it into `node_modules`. That is the entire
//! surface these commands are responsible for, so it is the surface asserted on
//! below: the manifest, the lockfile and the tree, after each command, against
//! files rather than against a summary.
//!
//! The one test that wants a failure gets it from a name no registry will ever
//! answer for, which fails the same way with no network at all.
//!
//! What `install.rs` is to `uf install`, this is to the four commands beside
//! it. The layout of what they print is pinned in
//! `crates/uf_cli/src/commands/pm/deps/tests.rs`, where it can be rendered
//! without asking npm to resolve anything.

mod support;

use std::fs;
use std::path::Path;

use serde_json::Value;
use support::{assert_plain, uf};

/// A uf project npm can install into, with `vendored` local packages beside it.
///
/// The config turns the router off for the same reason `install.rs`'s does:
/// this is a package-manager test and a route table is not part of it.
fn project(dir: &Path, vendored: &[(&str, &str)]) {
    fs::write(
        dir.join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\
         export default defineConfig({ app: { router: { enabled: false } } });\n",
    )
    .unwrap();
    fs::write(
        dir.join("package.json"),
        "{\n  \"name\": \"deps-fixture\",\n  \"version\": \"1.0.0\"\n}\n",
    )
    .unwrap();
    for (name, version) in vendored {
        let package = dir.join("vendor").join(name);
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("package.json"),
            format!("{{ \"name\": \"{name}\", \"version\": \"{version}\" }}\n"),
        )
        .unwrap();
        fs::write(package.join("index.js"), "module.exports = 1;\n").unwrap();
    }
}

/// Run `uf` in `dir`, returning `(stdout, stderr, success)`.
fn run(dir: &Path, args: &[&str]) -> (String, String, bool) {
    let output = uf()
        .arg("--cwd")
        .arg(dir)
        .args(["--color", "never"])
        .args(args)
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

/// The same, asserting it worked and returning what it said.
fn ok(dir: &Path, args: &[&str]) -> String {
    let (stdout, stderr, success) = run(dir, args);
    assert!(success, "`uf {}` failed:\n{stdout}{stderr}", args.join(" "));
    stdout
}

/// A JSON file the command was supposed to write.
fn json(dir: &Path, name: &str) -> Value {
    let path = dir.join(name);
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} was not written: {error}", path.display()));
    serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

/// The line whose first word is `key`, from a key/value block.
fn row<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .map(str::trim_start)
        .find(|line| line.starts_with(&format!("{key} ")))
        .unwrap_or_else(|| panic!("no `{key}` row in:\n{text}"))
}

/// The `uf.lock` entry for the package at `path`, relative to the root.
fn locked<'a>(lock: &'a Value, path: &str) -> &'a Value {
    lock["packages"]
        .as_array()
        .expect("uf.lock lists packages")
        .iter()
        .find(|package| package["path"] == path)
        .unwrap_or_else(|| panic!("uf.lock has no package at {path:?}:\n{lock:#}"))
}

/// The whole of what `uf add` is for: three files, all four written.
#[test]
fn add_writes_the_manifest_the_lockfile_and_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);

    let stdout = ok(dir.path(), &["add", "./vendor/tiny"]);

    // The manifest: the specifier npm resolved, in `dependencies`.
    let manifest = json(dir.path(), "package.json");
    assert_eq!(manifest["dependencies"]["tiny"], "file:vendor/tiny");
    assert!(manifest["devDependencies"].is_null(), "{manifest:#}");

    // The manager's lockfile: the package, pinned where npm put it.
    let lock = json(dir.path(), "package-lock.json");
    assert_eq!(lock["packages"]["vendor/tiny"]["version"], "1.2.3");
    assert_eq!(
        lock["packages"][""]["dependencies"]["tiny"],
        "file:vendor/tiny"
    );

    // The tree: npm links a path dependency rather than copying it, which is
    // still `node_modules/tiny` resolving to the package.
    assert!(
        dir.path().join("node_modules/tiny/package.json").is_file(),
        "node_modules/tiny does not resolve"
    );

    // And uf's own lockfile, rewritten from the manifest npm just changed.
    // Without this step `uf.lock` would still describe the project as it was
    // before the command that was just run.
    let uf_lock = json(dir.path(), "uf.lock");
    assert_eq!(
        locked(&uf_lock, ".")["dependencies"]["tiny"],
        "file:vendor/tiny"
    );
    assert_eq!(locked(&uf_lock, "vendor/tiny")["version"], "1.2.3");
    assert!(
        dir.path().join(".uf/store/manifest.json").is_file(),
        "the content-addressed store was not written"
    );

    // The report says which manager, what named it, and what it ran.
    assert_eq!(row(&stdout, "manager"), "manager    npm");
    assert!(row(&stdout, "chosen by").contains("uf.lock"), "{stdout}");
    assert_eq!(
        row(&stdout, "command"),
        "command    npm install --ignore-scripts ./vendor/tiny"
    );
    assert!(
        stdout.contains("1 package recorded in dependencies"),
        "{stdout}"
    );

    // And the dependency tree, which for a project whose only dependency is a
    // path used to be missing altogether. npm records a link with no `version`
    // of its own — the version is on the row for the directory it points at —
    // and `uf_pm::delta` skipped every row without one, so `uf add` reported
    // an empty tree straight after linking something into it.
    // ubugeeei-prod/uf#426; this fixture is the case the issue names.
    let tree = stdout
        .split_once("dependency tree")
        .unwrap_or_else(|| panic!("the tree section is missing:\n{stdout}"))
        .1;
    // Read after the heading, because npm's own "added 1 package" line is
    // above it and uf's count row is the one under test.
    assert!(
        row(tree, "added").split_whitespace().eq(["added", "1"]),
        "{stdout}"
    );
    assert!(
        tree.lines()
            .any(|line| line.contains("tiny") && line.contains("1.2.3")),
        "the linked package is not in the tree table at its own version:\n{stdout}"
    );
    assert_plain(&stdout);
}

/// Each flag writes its own `package.json` field, and only its own.
#[test]
fn add_records_each_dependency_kind_in_its_own_field() {
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        &[
            ("plain", "1.0.0"),
            ("tool", "2.0.0"),
            ("maybe", "3.0.0"),
            ("host", "4.0.0"),
        ],
    );

    for (flag, name) in [
        (None, "plain"),
        (Some("--dev"), "tool"),
        (Some("--optional"), "maybe"),
        (Some("--peer"), "host"),
    ] {
        let spec = format!("./vendor/{name}");
        let mut args = vec!["add"];
        args.extend(flag);
        args.push(&spec);
        let stdout = ok(dir.path(), &args);
        let field = match flag {
            Some("--dev") => "devDependencies",
            Some("--optional") => "optionalDependencies",
            Some("--peer") => "peerDependencies",
            _ => "dependencies",
        };
        assert!(
            stdout.contains(&format!("recorded in {field}")),
            "uf add {flag:?} said something else:\n{stdout}"
        );
    }

    let manifest = json(dir.path(), "package.json");
    assert_eq!(manifest["dependencies"]["plain"], "file:vendor/plain");
    assert_eq!(manifest["devDependencies"]["tool"], "file:vendor/tool");
    assert_eq!(
        manifest["optionalDependencies"]["maybe"],
        "file:vendor/maybe"
    );
    assert_eq!(manifest["peerDependencies"]["host"], "file:vendor/host");

    // Each one landed in exactly one field, so no flag is quietly ignored.
    for field in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        assert_eq!(
            manifest[field].as_object().map(serde_json::Map::len),
            Some(1),
            "{field} holds more than the one package that asked for it:\n{manifest:#}"
        );
    }
}

/// The second `uf add` of the same thing changes nothing and says so.
#[test]
fn adding_the_same_package_twice_changes_nothing_the_second_time() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);

    let first = ok(dir.path(), &["add", "./vendor/tiny"]);
    let manifest = fs::read_to_string(dir.path().join("package.json")).unwrap();
    let lock = fs::read_to_string(dir.path().join("package-lock.json")).unwrap();
    let uf_lock = fs::read_to_string(dir.path().join("uf.lock")).unwrap();

    let second = ok(dir.path(), &["add", "./vendor/tiny"]);

    assert!(first.contains("recorded in dependencies"), "{first}");
    assert!(second.contains("already up to date"), "{second}");
    assert!(
        !second.contains("manifest") && !second.contains("dependency tree"),
        "a report about nothing happening is noise:\n{second}"
    );
    assert_eq!(
        manifest,
        fs::read_to_string(dir.path().join("package.json")).unwrap()
    );
    assert_eq!(
        lock,
        fs::read_to_string(dir.path().join("package-lock.json")).unwrap()
    );
    assert_eq!(
        uf_lock,
        fs::read_to_string(dir.path().join("uf.lock")).unwrap()
    );
}

/// `uf remove` is the other direction, through the same three files.
#[test]
fn remove_takes_it_out_of_the_manifest_the_lockfile_and_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3"), ("tool", "2.0.0")]);
    ok(dir.path(), &["add", "./vendor/tiny"]);
    ok(dir.path(), &["add", "--dev", "./vendor/tool"]);

    let stdout = ok(dir.path(), &["remove", "tiny"]);

    let manifest = json(dir.path(), "package.json");
    assert!(
        manifest["dependencies"]["tiny"].is_null(),
        "still in the manifest:\n{manifest:#}"
    );
    assert_eq!(
        manifest["devDependencies"]["tool"], "file:vendor/tool",
        "the other one was not touched:\n{manifest:#}"
    );

    let lock = json(dir.path(), "package-lock.json");
    assert!(
        lock["packages"]["node_modules/tiny"].is_null(),
        "still in the lockfile:\n{lock:#}"
    );
    assert!(
        !dir.path().join("node_modules/tiny").exists(),
        "still in node_modules"
    );

    let uf_lock = json(dir.path(), "uf.lock");
    assert!(locked(&uf_lock, ".")["dependencies"]["tiny"].is_null());

    assert!(
        stdout.contains("1 package taken out of dependencies"),
        "{stdout}"
    );

    // A name the manifest does not list is not an error: the project ends up
    // the way it was asked to be either way.
    let (stdout, stderr, success) = run(dir.path(), &["remove", "tiny"]);
    assert!(success, "{stdout}{stderr}");
    assert!(stdout.contains("already up to date"), "{stdout}");
}

/// `uf update` moves the lockfile and leaves the manifest's ranges alone.
#[test]
fn update_moves_the_lockfile_and_not_the_manifest_ranges() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    ok(dir.path(), &["add", "./vendor/tiny"]);
    let manifest = fs::read_to_string(dir.path().join("package.json")).unwrap();

    let stdout = ok(dir.path(), &["update"]);

    assert_eq!(
        row(&stdout, "command"),
        "command    npm update --ignore-scripts"
    );
    assert_eq!(
        manifest,
        fs::read_to_string(dir.path().join("package.json")).unwrap(),
        "`uf update` rewrote a range it does not own"
    );
    assert!(stdout.contains("already up to date"), "{stdout}");
}

/// `uf why` answers out of the manager's own lockfile, and writes nothing.
#[test]
fn why_answers_from_the_manager_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    ok(dir.path(), &["add", "./vendor/tiny"]);
    let before = fs::read_to_string(dir.path().join("uf.lock")).unwrap();
    fs::remove_dir_all(dir.path().join(".uf")).unwrap();

    let stdout = ok(dir.path(), &["why", "tiny"]);

    // Who answered, before the answer, because the answer is what was asked
    // for and should be the last thing on the screen.
    assert_eq!(row(&stdout, "manager"), "manager    npm");
    assert_eq!(row(&stdout, "command"), "command    npm explain tiny");
    assert!(
        stdout.contains("tiny@1.2.3") && stdout.contains("node_modules/tiny"),
        "npm's explanation did not reach the terminal:\n{stdout}"
    );
    // Asking why a package is installed must not install anything.
    assert_eq!(
        before,
        fs::read_to_string(dir.path().join("uf.lock")).unwrap()
    );
    assert!(
        !dir.path().join(".uf").exists(),
        "`uf why` wrote the store it had just been asked a question about"
    );
    assert_plain(&stdout);
}

/// A name nothing depends on fails, in the manager's words and uf's.
#[test]
fn why_a_package_that_is_not_there_fails_and_says_what_to_look_at() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    ok(dir.path(), &["add", "./vendor/tiny"]);

    let (stdout, stderr, success) = run(dir.path(), &["why", "not-installed"]);

    assert!(!success, "this cannot succeed:\n{stdout}{stderr}");
    assert!(
        stderr.contains("No dependencies found matching not-installed"),
        "npm's own diagnosis must reach the terminal:\n{stderr}"
    );
    assert!(
        stderr.contains("not in this project's tree"),
        "and uf's sentence about what to do with it:\n{stderr}"
    );
    assert_plain(&stderr);
}

/// The install CI runs: it pins, and it refuses a lockfile that has drifted.
#[test]
fn a_frozen_install_refuses_a_lockfile_the_manifest_has_moved_past() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    ok(dir.path(), &["add", "./vendor/tiny"]);

    // In sync: the frozen install is an install like any other.
    let stdout = ok(dir.path(), &["install", "--frozen-lockfile"]);
    assert_eq!(
        row(&stdout, "command"),
        "command    npm ci --ignore-scripts --loglevel=http"
    );
    assert!(dir.path().join("node_modules/tiny/package.json").is_file());

    // Now the lockfile no longer pins what the manifest asks for, which is
    // exactly the drift `npm ci` exists to catch. The manifest is left alone on
    // purpose: this is the manager's check, and uf's own `uf.lock` check —
    // which fires first when a manifest moves — has its own test below.
    let mut lock = json(dir.path(), "package-lock.json");
    let packages = lock["packages"].as_object_mut().expect("a package map");
    packages.remove("vendor/tiny");
    packages.remove("node_modules/tiny");
    fs::write(
        dir.path().join("package-lock.json"),
        serde_json::to_string_pretty(&lock).unwrap(),
    )
    .unwrap();

    let (stdout, stderr, success) = run(dir.path(), &["install", "--frozen-lockfile"]);
    assert!(!success, "a stale lockfile has to fail:\n{stdout}{stderr}");
    assert!(
        stderr.contains("can only install packages when your package.json and package-lock.json"),
        "the manager's own diagnosis must reach the terminal:\n{stderr}"
    );
    assert!(
        stderr.contains("run `uf install` and commit the lockfile it writes"),
        "and uf's sentence about what to do with it:\n{stderr}"
    );

    // And a plain install fixes it, which is what that sentence promises.
    ok(dir.path(), &["install"]);
    ok(dir.path(), &["install", "--frozen-lockfile"]);
}

/// A `uf.lock` the workspace has moved past is the same failure, found first.
///
/// `uf install --frozen-lockfile` promises to change nothing; `uf.lock` is
/// derived from the manifests, so one that comes out different is drift and not
/// a write to make quietly. The check has to leave the file it was handed —
/// otherwise the CI step that fails cannot show a diff of it.
#[test]
fn a_frozen_install_refuses_a_uf_lock_the_manifests_have_moved_past() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    ok(dir.path(), &["add", "./vendor/tiny"]);

    let stale = fs::read_to_string(dir.path().join("uf.lock"))
        .unwrap()
        .replace("file:vendor/tiny", "file:vendor/gone");
    fs::write(dir.path().join("uf.lock"), &stale).unwrap();

    let (stdout, stderr, success) = run(dir.path(), &["install", "--frozen-lockfile"]);

    assert!(!success, "a stale uf.lock has to fail:\n{stdout}{stderr}");
    assert!(
        stderr.contains("uf.lock does not match this workspace's package manifests"),
        "{stderr}"
    );
    assert!(stderr.contains("run `uf install`"), "{stderr}");
    assert_eq!(
        stale,
        fs::read_to_string(dir.path().join("uf.lock")).unwrap(),
        "the check rewrote the file it was checking"
    );
}

/// A specifier the manager would read as a flag is refused, and refused before
/// anything at all has been written.
#[test]
fn a_specifier_that_would_be_read_as_a_flag_is_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);

    let (stdout, stderr, success) = run(dir.path(), &["add", "--", "--global"]);

    assert!(!success, "{stdout}{stderr}");
    assert!(stderr.contains("--global"), "{stderr}");
    assert!(stderr.contains("would read as a flag"), "{stderr}");
    assert!(
        stderr.contains("./--global"),
        "and what to write instead:\n{stderr}"
    );
    // Nothing ran, so nothing was written: not the manifest, not either
    // lockfile, not the store.
    assert!(!dir.path().join("uf.lock").exists(), "uf.lock was written");
    assert!(!dir.path().join(".uf").exists(), "the store was written");
    assert!(!dir.path().join("package-lock.json").exists());
    assert!(!dir.path().join("node_modules").exists());
    assert_plain(&stderr);
}

/// The project's own refusal of npm scripts guards `uf add`, not only
/// `uf install` — and adding a dependency is when a script most often arrives.
#[test]
fn a_manifest_that_declares_scripts_stops_an_add_before_anything_is_fetched() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    fs::write(
        dir.path().join("package.json"),
        "{\n  \"name\": \"deps-fixture\",\n  \"version\": \"1.0.0\",\n  \
         \"scripts\": { \"postinstall\": \"echo owned\" }\n}\n",
    )
    .unwrap();

    let (stdout, stderr, success) = run(dir.path(), &["add", "./vendor/tiny"]);

    assert!(!success, "{stdout}{stderr}");
    assert!(stderr.contains("declares scripts"), "{stderr}");
    assert!(stderr.contains("uf tasks"), "{stderr}");
    assert!(
        !dir.path().join("node_modules").exists(),
        "npm ran before the refusal"
    );
    assert!(!dir.path().join("package-lock.json").exists());
}

/// A workspace member's manifest is still locked after a root `uf add`.
///
/// `install_workspace` discovers every `package.json` the project owns, and the
/// rewrite that follows the manager has to be the same discovery — an `uf add`
/// that dropped the members out of `uf.lock` would be a command that fixed one
/// file by breaking another.
#[test]
fn a_workspace_member_is_still_locked_after_an_add() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    let member = dir.path().join("packages/ui");
    fs::create_dir_all(&member).unwrap();
    fs::write(
        member.join("package.json"),
        "{\n  \"name\": \"@fixture/ui\",\n  \"version\": \"0.3.0\",\n  \
         \"dependencies\": { \"tiny\": \"file:../../vendor/tiny\" }\n}\n",
    )
    .unwrap();

    ok(dir.path(), &["add", "./vendor/tiny"]);

    let uf_lock = json(dir.path(), "uf.lock");
    let ui = locked(&uf_lock, "packages/ui");
    assert_eq!(ui["name"], "@fixture/ui");
    assert_eq!(ui["version"], "0.3.0");
    assert_eq!(ui["dependencies"]["tiny"], "file:../../vendor/tiny");
    assert_eq!(
        locked(&uf_lock, ".")["dependencies"]["tiny"],
        "file:vendor/tiny"
    );
}

/// `uf explain` names the manager each of these will actually spawn.
///
/// The commands delegate, and a plan that did not say to whom would be the
/// black box `docs/red-lines.md` line 7 forbids.
#[test]
fn explain_names_the_command_each_of_these_will_spawn() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[]);

    for (command, expected) in [
        ("add", "npm install"),
        ("remove", "npm uninstall"),
        ("update", "npm update"),
        ("why", "npm explain"),
    ] {
        let stdout = ok(dir.path(), &["explain", command]);
        assert!(
            stdout.contains(expected),
            "uf explain {command} did not name `{expected}`:\n{stdout}"
        );
    }
}

/// npm has no `patch`, and uf says so in its own words rather than handing npm
/// a subcommand it has never heard of.
///
/// The distinction is the whole point of ubugeeei-prod/uf#494: a passthrough
/// would fail too, with npm's "Unknown command: patch" and a suggestion to run
/// `npm help`, which tells a reader nothing about what to do next. A refusal
/// names the two managers that have it and the package the rest of the
/// ecosystem uses.
#[test]
fn patch_on_a_manager_that_has_none_is_a_refusal_rather_than_a_passthrough() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), &[("tiny", "1.2.3")]);
    ok(dir.path(), &["add", "./vendor/tiny"]);

    let (stdout, stderr, success) = run(dir.path(), &["patch", "tiny"]);

    assert!(!success, "this cannot succeed:\n{stdout}{stderr}");
    assert!(
        !stderr.contains("Unknown command"),
        "npm was handed a subcommand it does not have:\n{stderr}"
    );
    assert!(
        stderr.contains("patch") && stderr.contains("npm"),
        "the refusal names neither the operation nor the manager:\n{stderr}"
    );
    assert!(
        stderr.contains("patch-package"),
        "the refusal does not say what to do instead:\n{stderr}"
    );
    assert!(
        stderr.contains("pnpm and yarn 2+"),
        "the refusal does not name the managers that can:\n{stderr}"
    );
    // A command that refuses must not have touched the project on the way to
    // refusing.
    assert!(
        !dir.path().join("patches").exists(),
        "`uf patch` wrote a patch directory for a manager that cannot patch"
    );
    assert_plain(&stderr);
}
