//! `uf prepare`: what it narrows to, what it writes, and what it refuses.
//!
//! The subject here is the pre-commit run. `every_command.rs` asks `uf prepare`
//! the four questions it asks every command; this file asks the ones only this
//! command has an answer for — which files a run is about, what happens when
//! there is no git to ask, and whether the record it leaves behind matches
//! what actually happened.
//!
//! Every project here is a real scaffold with a real `git init`, because the
//! staged set is the whole point and a fixture that fakes it would prove
//! nothing.

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::{create_app, uf};

/// Ran, found nothing to report.
const SUCCESS: i32 = 0;
/// Ran, found a problem.
const FOUND_A_PROBLEM: i32 = 1;

/// A file with a lint error in it and no formatting problem.
///
/// `security/no-eval` is an error by default and the source is already
/// formatted, so a run that fails is failing the lint step and not the other
/// one.
const LINTS_BADLY: &str =
    "// @flow\n\nexport function run(source: string): mixed {\n  return eval(source);\n}\n";

/// A file that lints clean and is not formatted.
const FORMATS_BADLY: &str = "// @flow\n\nexport const spaced: number =    1;\n";

/// A file with one lint error uf can fix without being asked, and one it will
/// not fix without being asked.
///
/// `flow/deprecated-type` has a safe fix; `flow/non-const-var-export` has only
/// an unsafe one, and a commit hook is the last place that should arrive
/// unasked-for.
const FIXES_ITSELF: &str =
    "// @flow\n\nexport type Flag = bool;\n\nexport let count: number = 0;\n";

/// A file with nothing wrong with it.
const CLEAN: &str = "// @flow\n\nexport const answer: number = 42;\n";

/// Run `uf` against `dir`, returning its exit code, stdout and stderr.
fn run(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let output = uf()
        .arg("--cwd")
        .arg(dir)
        .args(["--color", "never"])
        .args(args)
        .env("UF_STORE", dir.join(".uf/store"))
        .env("UF_ROOTS", dir.join(".uf/roots"))
        .output()
        .expect("uf started");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Run git in `dir`, failing the test with git's own message when it refuses.
fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git started");
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A scaffolded React app that is also a git repository.
fn a_repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    create_app(dir.path());
    git(dir.path(), &["init", "--quiet"]);
    dir
}

/// The `.uf/prepare.json` of the last run.
fn record(dir: &Path) -> serde_json::Value {
    let text =
        fs::read_to_string(dir.join(".uf/prepare.json")).expect("uf prepare wrote its record");
    serde_json::from_str(&text).expect("the record is JSON")
}

/// One field of one step in the record.
fn field_of(record: &serde_json::Value, step: &str, field: &str) -> String {
    record["steps"]
        .as_array()
        .expect("the record lists its steps")
        .iter()
        .find(|entry| entry["step"] == step)
        .unwrap_or_else(|| panic!("{step} is not in the record: {record}"))[field]
        .as_str()
        .unwrap_or_else(|| panic!("{step} has no {field}: {record}"))
        .to_string()
}

/// The status of one step in the record.
fn status_of(record: &serde_json::Value, step: &str) -> String {
    field_of(record, step, "status")
}

/// A file that is not staged is not checked, and the same file staged is.
///
/// The whole premise of a pre-commit command in one test: a project can have a
/// problem in it and still commit something unrelated, and the moment the
/// problem is what is being committed, `uf prepare` says so.
#[test]
fn a_run_is_about_the_staged_files_and_nothing_else() {
    let dir = a_repository();
    fs::write(dir.path().join("clean.js"), CLEAN).expect("a clean file");
    fs::write(dir.path().join("broken.js"), LINTS_BADLY).expect("a file with a lint error");
    git(dir.path(), &["add", "clean.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);
    assert_eq!(
        code, SUCCESS,
        "an unstaged lint error failed the commit\n{stdout}{stderr}"
    );
    assert!(!stdout.contains("no-eval"), "{stdout}");

    git(dir.path(), &["add", "broken.js"]);
    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);
    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    assert!(stdout.contains("security/no-eval"), "{stdout}");
    assert!(stderr.contains("run-lint"), "{stderr}");
}

/// A project outside git checks everything, and says why.
///
/// The alternative — narrowing to an empty set and reporting success — would
/// make `uf prepare` a no-op in an exported tarball, a fresh directory, or a
/// CI job that unpacked an archive. It checks more rather than less, and it
/// names the reason so nobody has to guess which of the two happened.
#[test]
fn a_project_with_no_git_checks_every_file_and_says_so() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    create_app(dir.path());
    fs::write(dir.path().join("broken.js"), LINTS_BADLY).expect("a file with a lint error");

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    assert!(
        stdout.contains("not a git working tree"),
        "the run did not say why there was no staged set:\n{stdout}"
    );
    assert!(stdout.contains("security/no-eval"), "{stdout}");
    assert_eq!(record(dir.path())["staged"]["source"], "none");
}

/// A repository with an empty index has nothing to check.
///
/// Not the same as having no git at all, and the record has to be able to tell
/// them apart: one means "check everything", the other means "check nothing".
#[test]
fn an_empty_index_skips_the_checks_rather_than_widening_to_everything() {
    let dir = a_repository();
    fs::write(dir.path().join("broken.js"), LINTS_BADLY).expect("a file with a lint error");

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);

    assert_eq!(
        code, SUCCESS,
        "an unstaged lint error failed a commit of nothing\n{stdout}{stderr}"
    );
    assert!(stdout.contains("nothing is staged for commit"), "{stdout}");
    let record = record(dir.path());
    assert_eq!(record["staged"]["source"], "git");
    assert_eq!(
        record["staged"]["files"].as_array().expect("a list").len(),
        0
    );
    // One file, and it is `router.js`: this run generated it, so this run
    // checks it. Everything else in the project — including the file with the
    // lint error in it — is outside the set. An empty set that was read as
    // "everything" would report ten files here and fail the commit.
    assert_eq!(
        field_of(&record, "run-lint", "detail"),
        "1 file checked, no problems"
    );
    assert_eq!(status_of(&record, "run-format-check"), "ok");
}

/// A staged file that needs formatting fails the format step, not the lint one.
#[test]
fn a_staged_file_that_needs_formatting_fails_the_format_step() {
    let dir = a_repository();
    fs::write(dir.path().join("ugly.js"), FORMATS_BADLY).expect("an unformatted file");
    git(dir.path(), &["add", "ugly.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    assert!(stderr.contains("run-format-check"), "{stderr}");
    let record = record(dir.path());
    assert_eq!(status_of(&record, "run-lint"), "ok");
    assert_eq!(status_of(&record, "run-format-check"), "failed");
    assert_eq!(record["ok"], false);
}

/// The record says which step failed, and does not claim the others passed.
///
/// A generation failure stops the run, and the steps after it are recorded as
/// `not-run` rather than as anything that could be mistaken for a check that
/// happened. This is the failure the old command could not have: it wrote a
/// file naming six steps whatever the outcome.
#[test]
fn a_generation_failure_stops_the_run_and_the_record_says_which_steps_never_ran() {
    let dir = a_repository();
    fs::write(dir.path().join("clean.js"), CLEAN).expect("a clean file");
    git(dir.path(), &["add", "clean.js"]);
    // A directory where the generated file goes: `uf prepare` cannot write it,
    // and nothing after it can be trusted to have read the right types.
    fs::create_dir(dir.path().join("router.js")).expect("a directory in the way");

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    assert!(stderr.contains("generate-router-types"), "{stderr}");
    let record = record(dir.path());
    assert_eq!(status_of(&record, "discover-staged-files"), "ok");
    assert_eq!(status_of(&record, "generate-router-types"), "failed");
    assert_eq!(status_of(&record, "run-lint"), "not-run");
    assert_eq!(status_of(&record, "run-format-check"), "not-run");
    assert_eq!(record["ok"], false);
    assert_eq!(
        record["generated"].as_array().expect("a list").len(),
        0,
        "the record claimed a file it never wrote"
    );
}

/// A staged file `.gitignore` names is still checked.
///
/// git stops applying `.gitignore` to a path the moment that path is in the
/// index, so a force-added file is as much a part of the commit as any other.
/// Discovery reads `.gitignore` now (ubugeeei-prod/uf#483), and a run that let
/// it narrow the staged set would have skipped the one file the commit is
/// about while reporting "no problems" — a hook that passes because it never
/// opened the file is worse than no hook.
#[test]
fn a_staged_file_the_gitignore_names_is_still_checked() {
    let dir = a_repository();
    fs::write(dir.path().join("ignored.js"), LINTS_BADLY).expect("a file with a lint error");
    let gitignore = dir.path().join(".gitignore");
    let scaffolded = fs::read_to_string(&gitignore).expect("the scaffold's .gitignore");
    fs::write(&gitignore, format!("{scaffolded}ignored.js\n")).expect("a .gitignore naming it");
    git(dir.path(), &["add", "--force", "ignored.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    assert!(
        stdout.contains("security/no-eval"),
        "the staged file was not linted:\n{stdout}"
    );
}

/// A project with server actions: the types are written, and they are checked.
///
/// `server-actions.js` and `router.js` are git-ignored in a scaffolded project,
/// so they are never in the index. Only one file is staged here, and the lint
/// step still reports three — the staged one and the two this run generated.
/// Without that, a commit hook could write a file its own linter rejects.
#[test]
fn the_generated_files_are_checked_even_though_they_are_never_staged() {
    let dir = a_repository();
    with_a_server_action(dir.path());
    git(dir.path(), &["add", "uf.config.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);

    assert_eq!(code, SUCCESS, "{stdout}{stderr}");
    assert!(
        dir.path().join("server-actions.js").is_file(),
        "no server-actions.js was written:\n{stdout}"
    );
    assert!(
        stdout.contains("3 files checked, no problems"),
        "the generated files were not linted:\n{stdout}"
    );
    assert!(
        stdout.contains("2 callable actions"),
        "the run did not say what it generated:\n{stdout}"
    );
}

/// The generated types point at the real declaration, so Flow checks the call.
#[test]
fn the_generated_action_types_carry_the_real_signature() {
    let dir = a_repository();
    with_a_server_action(dir.path());
    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);
    assert_eq!(code, SUCCESS, "{stdout}{stderr}");

    let generated =
        fs::read_to_string(dir.path().join("server-actions.js")).expect("the generated types");
    assert!(
        generated.contains("Generated by `uf prepare`. Do not edit"),
        "{generated}"
    );
    assert!(
        generated.contains("import typeof * as Module0 from \"./app/actions.js\";"),
        "{generated}"
    );
    assert!(
        generated.contains("\"app/actions.js#createUser\": Module0[\"createUser\"],"),
        "{generated}"
    );
    // And it is a file uf itself accepts, which is the only reason writing it
    // during a commit hook is safe.
    let (code, stdout, stderr) = run(dir.path(), &["lint"]);
    assert_eq!(
        code, SUCCESS,
        "the generated types do not lint\n{stdout}{stderr}"
    );
    let (code, stdout, stderr) = run(dir.path(), &["fmt", "--check"]);
    assert_eq!(
        code, SUCCESS,
        "the generated types are not formatted\n{stdout}{stderr}"
    );
}

/// Twice over the same sources is byte for byte the same file.
///
/// The action registry is keyed on a build id that changes every run, so this
/// only holds because no id reaches the generated file. The record is compared
/// too: `prepare.json` carries no clock and no ordering that depends on the
/// file system.
#[test]
fn running_twice_generates_the_same_bytes() {
    let dir = a_repository();
    with_a_server_action(dir.path());
    git(dir.path(), &["add", "-A"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);
    assert_eq!(code, SUCCESS, "{stdout}{stderr}");
    let first: Vec<String> = ["router.js", "server-actions.js", ".uf/prepare.json"]
        .iter()
        .map(|name| fs::read_to_string(dir.path().join(name)).expect("a generated file"))
        .collect();

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);
    assert_eq!(code, SUCCESS, "{stdout}{stderr}");
    let second: Vec<String> = ["router.js", "server-actions.js", ".uf/prepare.json"]
        .iter()
        .map(|name| fs::read_to_string(dir.path().join(name)).expect("a generated file"))
        .collect();

    similar_asserts::assert_eq!(first, second);
}

/// A commit is not the place to discover uf's default formatter is missing.
///
/// The same decision `uf fmt` makes, in the command that runs on somebody's
/// commit: a formatter uf chose and the project never named is a suggestion,
/// so the format step says which files it did not look at and passes. A hook
/// that refuses a clean commit over a tool the project never asked for is a
/// hook that gets uninstalled. See ubugeeei-prod/uf#441.
#[test]
fn a_missing_default_formatter_does_not_fail_the_commit() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    create_app(dir.path());
    fs::write(dir.path().join("package-lock.json"), "{\n  \"a\": 1\n}\n")
        .expect("a lockfile, as `uf install` leaves one");

    // Neither the formatter nor git is on this `PATH`, so nothing about the
    // outcome depends on what happens to be installed on this machine: with no
    // staged set, `uf prepare` checks every file, the lockfile included.
    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["--color", "never", "prepare"])
        .env("PATH", dir.path().join("no-tools"))
        .env("UF_STORE", dir.path().join(".uf/store"))
        .env("UF_ROOTS", dir.path().join(".uf/roots"))
        .output()
        .expect("uf started");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert_eq!(
        output.status.code().unwrap_or(-1),
        SUCCESS,
        "a clean commit failed over a formatter the project never named\n{stdout}{stderr}"
    );
    assert!(
        stdout.contains("biome is not installed"),
        "the step must still say what it did not look at:\n{stdout}"
    );
    assert!(stdout.contains("1 non-Flow file was skipped"), "{stdout}");
    assert!(stdout.contains("package-lock.json"), "{stdout}");
    assert_eq!(status_of(&record(dir.path()), "run-format-check"), "ok");
}

/// Give `root` a server action a client boundary can reach.
///
/// The page already imports `Counter`, which is a `"use client"` module, so
/// importing the action module from the page is what makes the action a
/// callable endpoint — the same rule the RSC manifest applies.
fn with_a_server_action(root: &Path) {
    fs::write(
        root.join("app/actions.js"),
        "// @flow\n\"use server\";\n\nexport async function createUser(name: string, age: number): Promise<string> {\n  return `${name}:${String(age)}`;\n}\n\nexport async function deleteUser(id: string): Promise<void> {}\n",
    )
    .expect("an action module");
    let page = root.join("app/$page.js");
    let source = fs::read_to_string(&page).expect("the scaffolded page");
    fs::write(
        &page,
        source.replacen(
            "// @flow\n",
            "// @flow\nimport { createUser, deleteUser } from \"./actions.js\";\n",
            1,
        ),
    )
    .expect("a page that reaches the actions");
}

/// `--fix` formats the staged files instead of only saying they need it, and
/// then fails so the commit stops with the diff still to be staged.
#[test]
fn fix_formats_the_staged_files_and_stops_the_commit_anyway() {
    let dir = a_repository();
    fs::write(dir.path().join("ugly.js"), FORMATS_BADLY).expect("an unformatted file");
    git(dir.path(), &["add", "ugly.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare", "--fix"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    assert!(stderr.contains("run-format-check"), "{stderr}");
    assert!(stderr.contains("stage them, and commit again"), "{stderr}");
    let formatted = fs::read_to_string(dir.path().join("ugly.js")).expect("the file is there");
    assert_eq!(
        formatted, "// @flow\n\nexport const spaced: number = 1;\n",
        "the format step did not write"
    );

    // Staging what it wrote is all that was left to do, and the same command
    // passes: nothing about the file was wrong, only unstaged.
    git(dir.path(), &["add", "ugly.js"]);
    let (code, stdout, stderr) = run(dir.path(), &["prepare", "--fix"]);
    assert_eq!(code, SUCCESS, "{stdout}{stderr}");
}

/// The lint step writes what it safely can, and leaves what it cannot.
#[test]
fn fix_applies_the_safe_lint_fixes_and_not_the_unsafe_ones() {
    let dir = a_repository();
    fs::write(dir.path().join("mixed.js"), FIXES_ITSELF).expect("a fixable file");
    git(dir.path(), &["add", "mixed.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare", "--fix"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    let fixed = fs::read_to_string(dir.path().join("mixed.js")).expect("the file is there");
    assert!(fixed.contains("Flag = boolean"), "{fixed}");
    assert!(
        fixed.contains("export let count"),
        "a commit hook applied an unsafe fix\n{fixed}"
    );
    assert_eq!(status_of(&record(dir.path()), "run-lint"), "failed");
}

/// Without the flag nothing is written, which is what every existing hook gets.
#[test]
fn a_run_without_fix_writes_nothing_it_used_to_only_report() {
    let dir = a_repository();
    fs::write(dir.path().join("ugly.js"), FORMATS_BADLY).expect("an unformatted file");
    git(dir.path(), &["add", "ugly.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{stdout}{stderr}");
    assert_eq!(
        fs::read_to_string(dir.path().join("ugly.js")).expect("the file is there"),
        FORMATS_BADLY
    );
}

/// A clean commit is unchanged by the flag: `--fix` with nothing to fix passes.
#[test]
fn fix_over_a_clean_commit_passes() {
    let dir = a_repository();
    fs::write(dir.path().join("clean.js"), CLEAN).expect("a clean file");
    git(dir.path(), &["add", "clean.js"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare", "--fix"]);

    assert_eq!(code, SUCCESS, "{stdout}{stderr}");
}

/// `--fix` fails when the *other* formatter rewrote a staged file, too.
///
/// The rule the module header states — a run that changed a file fails, because
/// what it wrote is in the working tree and not in the index — held for the
/// Flow half only. Biome exits 0 whether it rewrote every file or none of them,
/// so a staged `.json` was reformatted on disk, the hook reported success, and
/// git committed the bytes from before the rewrite. See the review on #455.
///
/// The formatter here is a stub, which is deliberate twice over: the test says
/// nothing about Biome's own opinions, and it runs on a machine that has never
/// installed it.
#[cfg(unix)]
#[test]
fn fix_stops_the_commit_when_the_non_flow_formatter_rewrote_a_staged_file() {
    let dir = a_repository();
    stub_formatter(dir.path());
    fs::write(dir.path().join("data.json"), UNFORMATTED_JSON).expect("an unformatted file");
    git(dir.path(), &["add", "data.json"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare", "--fix"]);

    assert_eq!(
        code, FOUND_A_PROBLEM,
        "the hook passed over a file it had just rewritten and left unstaged\n{stdout}{stderr}"
    );
    assert!(stderr.contains("run-format-check"), "{stderr}");
    assert!(stderr.contains("stage them, and commit again"), "{stderr}");
    assert!(
        stdout.contains("data.json"),
        "the step did not say which file\n{stdout}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("data.json")).expect("the file is there"),
        FORMATTED_JSON,
        "the format step did not write"
    );
    let record = record(dir.path());
    assert_eq!(status_of(&record, "run-format-check"), "failed");
    assert!(
        field_of(&record, "run-format-check", "detail").contains("1 non-Flow file rewritten"),
        "the record does not say what happened: {record}"
    );
    assert_eq!(record["ok"], false);

    // Staging what it wrote is all that was left to do, and the same command
    // passes: nothing about the file was wrong, only unstaged.
    git(dir.path(), &["add", "data.json"]);
    let (code, stdout, stderr) = run(dir.path(), &["prepare", "--fix"]);
    assert_eq!(code, SUCCESS, "{stdout}{stderr}");
}

/// A staged non-Flow file the formatter is content with does not fail a commit.
///
/// The other half of the test above, and the one that says the check is a
/// comparison rather than "the formatter ran, so something changed".
#[cfg(unix)]
#[test]
fn fix_passes_when_the_non_flow_formatter_had_nothing_to_change() {
    let dir = a_repository();
    stub_formatter(dir.path());
    fs::write(dir.path().join("data.json"), FORMATTED_JSON).expect("a formatted file");
    git(dir.path(), &["add", "data.json"]);

    let (code, stdout, stderr) = run(dir.path(), &["prepare", "--fix"]);

    assert_eq!(code, SUCCESS, "{stdout}{stderr}");
    assert_eq!(status_of(&record(dir.path()), "run-format-check"), "ok");
}

/// What the stub below is content with, and what it writes over anything else.
#[cfg(unix)]
const FORMATTED_JSON: &str = "{\n  \"answer\": 42\n}\n";

/// And a file it is not content with.
#[cfg(unix)]
const UNFORMATTED_JSON: &str = "{\"answer\":42}\n";

/// Install a stand-in for the project's non-Flow formatter in `root`.
///
/// `node_modules/.bin` is where uf looks for a project's formatter first, so
/// this needs nothing of the machine's `PATH` — and this machine has no Biome.
/// It has a real formatter's two modes: it rewrites what it dislikes when it is
/// given `--write`, which is how uf spells "format" to Biome, and reports it by
/// exiting 1 when it is not.
#[cfg(unix)]
fn stub_formatter(root: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let bin = root.join("node_modules/.bin");
    fs::create_dir_all(&bin).expect("a place for the stub");
    let stub = bin.join("biome");
    fs::write(
        &stub,
        format!(
            "#!/usr/bin/env sh\n\
             set -eu\n\
             write=no\n\
             case \" $* \" in *' --write '*) write=yes ;; esac\n\
             status=0\n\
             formatted={FORMATTED_JSON:?}\n\
             for arg in \"$@\"; do\n\
             \x20 [ -f \"$arg\" ] || continue\n\
             \x20 [ \"$(cat \"$arg\")\" != \"$(printf '%b' \"$formatted\")\" ] || continue\n\
             \x20 if [ \"$write\" = yes ]; then printf '%b' \"$formatted\" > \"$arg\"; else status=1; fi\n\
             done\n\
             exit \"$status\"\n"
        ),
    )
    .expect("the stub is written");
    let mut permissions = fs::metadata(&stub)
        .expect("the stub is there")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&stub, permissions).expect("the stub is executable");
}
