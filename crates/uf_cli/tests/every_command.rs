//! Every command in `uf --help`, and what it exits with.
//!
//! The other files here are organised by subject — `testing.rs` is the runner,
//! `vite.rs` is the bundler, `env.rs` is the toolchain. This one is organised
//! by *surface*: it takes the list `uf --help` prints as a checklist and asks
//! the same four questions of every entry on it. A project that is fine, a
//! project with no `uf.config.js`, an argument that names nothing, and the
//! number the process exits with.
//!
//! # The exit codes
//!
//! `docs/app/reference/cli/_uf.page.mdx` documents three, and a script that
//! wants to tell "uf disagrees with your code" from "uf never started" has
//! nothing else to read:
//!
//! | Code | Meaning |
//! | --- | --- |
//! | 0 | Success |
//! | 1 | The command ran and reported a problem |
//! | 2 | uf could not run the command |
//!
//! What is checked below is the part uf actually implements: 0, 1 for a
//! problem the command found, and 2 for an argument it could not parse. uf
//! has no classification for its *runtime* failures, so a broken
//! `uf.config.js` and a missing `@uniflowed/vite` — both squarely "could not
//! run" — exit 1 today. Those cases are asserted here as failures with the
//! message that names the fix, and deliberately not as a number, because
//! writing `1` into a test would make the reference documentation wrong on
//! purpose.

mod support;

use std::fs;
use std::path::Path;

use support::{create_app, uf};

/// Ran, found nothing to report.
const SUCCESS: i32 = 0;
/// Ran, found a problem: a failing test, a lint error, a file to format.
const FOUND_A_PROBLEM: i32 = 1;
/// Did not run: an argument uf could not parse.
const COULD_NOT_RUN: i32 = 2;

/// Every command `uf --help` lists, and where it is exercised end to end.
///
/// The commands this file cannot run without a network, a package manager or a
/// JavaScript host are named with the file that does run them, so the
/// checklist stays complete rather than quietly shrinking to whatever was
/// convenient to run here. `every_command_the_help_lists_is_checked_here`
/// fails when a command is added to the parser and not to this table.
const COVERAGE: &[(&str, &str)] = &[
    ("add", "dependencies.rs: runs the package manager"),
    ("build", "vite.rs and cli.rs: needs @uniflowed/vite"),
    ("check", "here, and typecheck.rs for the diagnostics"),
    ("completion", "here, and cli.rs for the script's shape"),
    // Where `create` was, because this list is `uf --help`'s order and that is
    // the parser's declaration order rather than the alphabet.
    ("init", "here, and cli.rs for the scaffold's contents"),
    ("new", "here, and cli.rs for the scaffold's contents"),
    ("dev", "vite.rs and cli.rs: binds a socket"),
    ("doc", "here, and cli.rs for the generated Markdown"),
    ("env", "here, and env.rs for the store"),
    ("exec", "cli.rs: runs a package, or refuses to fetch one"),
    ("explain", "here, and cli.rs for what it says"),
    ("fmt", "here, and cli.rs for what it rewrites"),
    ("info", "here, and output.rs for the brand surface"),
    ("inspect", "here, and inspect.rs for the resolved config"),
    ("install", "workflow.rs: runs the package manager"),
    // Where the parser declares it, which is what `uf --help` prints.
    ("clean", "here, and clean/tests.rs for what it removes"),
    ("lint", "here, and output.rs for the report"),
    ("lsp", "cli.rs: speaks a protocol over stdio"),
    (
        "prepare",
        "here, and prepare.rs for the staged set and the steps",
    ),
    ("preview", "vite.rs: builds, then binds a socket"),
    ("publish", "workflow.rs: names a registry"),
    ("release", "workflow.rs: writes a changelog"),
    ("remove", "dependencies.rs: runs the package manager"),
    (
        "routes",
        "here, and cli.rs for the table and the files it writes",
    ),
    ("run", "here, and cli.rs for the task runner"),
    ("start", "vite.rs: serves a build over a socket"),
    ("test", "here, and testing.rs for the runner"),
    ("update", "dependencies.rs: runs the package manager"),
    (
        "pm",
        "here, and approve/tests.rs for the listing and npm's sentence",
    ),
    (
        "patch",
        "dependencies.rs: asks the package manager, or names patch-package",
    ),
    (
        "catalog",
        "here, and catalog/tests.rs for the table and the disagreements",
    ),
    ("upgrade", "workflow.rs: resolves new versions"),
    ("use", "workflow.rs: rewrites the config"),
    ("ls", "dependencies.rs: asks the package manager"),
    ("audit", "dependencies.rs: asks the package manager"),
    (
        "search",
        "dependencies.rs: asks the package manager, or says it cannot",
    ),
    ("why", "dependencies.rs: asks the package manager"),
    // Last, because `uf --help` prints clap's own `help` last.
    ("help", "here"),
];

/// The commands that answer a question about the project and change nothing.
///
/// Anything that writes, installs, binds a port or speaks a protocol is in
/// [`COVERAGE`] against the file that runs it instead. `uf doc` is here in its
/// `--json` form for exactly that reason: without it, it writes `api.md`, and
/// `uf prepare` is absent because it generates `router.js` and runs the linter
/// — it has a test of its own below, over a fixture it is allowed to change,
/// and `prepare.rs` for the staged set and the steps.
const READ_ONLY: &[&[&str]] = &[
    &["check"],
    &["completion", "bash"],
    &["doc", "--json"],
    &["env", "doctor"],
    &["env", "list"],
    &["env", "gc", "--dry-run"],
    &["pm", "approve-builds"],
    &["catalog"],
    &["explain", "dev"],
    &["fmt", "--check"],
    &["info"],
    &["inspect"],
    &["lint"],
    &["routes", "list"],
    &["run"],
    &["test", "--list"],
];

/// Run `uf` against `dir`, returning its exit code, stdout and stderr.
///
/// `UF_STORE` and `UF_ROOTS` point inside the project's own `.uf`, which
/// discovery always ignores: `uf env` must not read or write the machine's
/// shared store while the suite is running.
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
        output
            .status
            .code()
            .expect("uf exited rather than being killed by a signal"),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A scaffolded application: the project that is fine.
fn scaffolded() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    create_app(dir.path());
    dir
}

/// A project with sources and a `package.json`, and no `uf.config.js`.
///
/// The `package.json` is what anchors the project root, so root discovery
/// stops here rather than walking up out of the temporary directory and
/// finding whatever is above it.
fn without_a_config() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::write(
        dir.path().join("package.json"),
        "{\n  \"name\": \"no-config\",\n  \"version\": \"0.0.0\",\n  \"private\": true\n}\n",
    )
    .expect("a manifest");
    fs::create_dir_all(dir.path().join("src")).expect("a source directory");
    fs::write(
        dir.path().join("src/app.js"),
        "// @flow\nexport const answer: number = 42;\n",
    )
    .expect("a source file");
    dir
}

/// The command names `uf --help` prints, in the order it prints them.
fn commands_in_the_help() -> Vec<String> {
    let output = uf().arg("--help").output().expect("uf started");
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
    stdout
        .split_once("Commands:")
        .expect("the help lists commands")
        .1
        .split_once("\nOptions:")
        .expect("commands come before options")
        .0
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("     "))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}

/// The checklist is the help, not a list somebody remembered to update.
#[test]
fn every_command_the_help_lists_is_checked_here() {
    let listed = commands_in_the_help();
    let covered = COVERAGE
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        listed, covered,
        "`uf --help` and this file's checklist have diverged; add the new \
         command to COVERAGE, and to the test that runs it"
    );
}

/// An argument uf cannot parse is `2` for every command, not `1`.
///
/// The distinction is the whole point of having two codes: `uf lint` answering
/// `1` because a rule fired and `uf lint --oops` answering `1` because there
/// is no such flag are different things happening, and CI cannot tell them
/// apart if both are `1`. Every command answered `1` until this was fixed.
#[test]
fn an_argument_uf_cannot_parse_could_not_run() {
    let dir = scaffolded();

    for (command, _) in COVERAGE {
        let (code, stdout, stderr) = run(dir.path(), &[command, "--uf-no-such-flag"]);
        assert_eq!(
            code, COULD_NOT_RUN,
            "`uf {command} --uf-no-such-flag` exited {code}\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stderr.contains("--uf-no-such-flag"),
            "`uf {command}` did not name the argument it refused: {stderr}"
        );
        // clap's own error, on the stream a script reads errors from. A parse
        // failure printed on stdout would be inside `uf lint --json`'s output.
        assert!(
            stdout.is_empty(),
            "`uf {command}` wrote to stdout: {stdout}"
        );
    }

    // The same at the root, where there is no subcommand to blame.
    assert_eq!(run(dir.path(), &["--uf-no-such-flag"]).0, COULD_NOT_RUN);
    assert_eq!(run(dir.path(), &["no-such-command"]).0, COULD_NOT_RUN);
}

/// Asking for help is a success, for every command that has any.
#[test]
fn help_and_version_are_a_success_for_every_command() {
    let dir = scaffolded();

    for (command, _) in COVERAGE {
        // `uf help --help` is clap's own shape: `help` takes a command name,
        // and `--help` is not one. It is asked for its help the other way.
        let args: &[&str] = if *command == "help" {
            &["help", "fmt"]
        } else {
            &[command, "--help"]
        };
        let (code, stdout, stderr) = run(dir.path(), args);
        assert_eq!(code, SUCCESS, "`uf {args:?}` exited {code}: {stderr}");
        assert!(!stdout.is_empty(), "`uf {args:?}` printed no help");
    }

    assert_eq!(run(dir.path(), &["--version"]).0, SUCCESS);
    assert_eq!(run(dir.path(), &["--help"]).0, SUCCESS);
    assert_eq!(run(dir.path(), &["help"]).0, SUCCESS);
}

/// Every command that only reads the project succeeds on a scaffolded one.
///
/// `uf create` writes what the templates say, and the very next thing anyone
/// does is run the toolchain over it. A command that cannot survive uf's own
/// scaffold has no chance on a real project.
#[test]
fn every_read_only_command_succeeds_in_a_scaffolded_project() {
    let dir = scaffolded();

    for args in READ_ONLY {
        let (code, stdout, stderr) = run(dir.path(), args);
        assert_eq!(
            code,
            SUCCESS,
            "`uf {}` exited {code}\nstdout: {stdout}\nstderr: {stderr}",
            args.join(" ")
        );
    }
}

/// A project with no `uf.config.js` is a project, not a failure.
///
/// Zero-config is the documented entry point — `uf.config.js` is where a
/// project *overrides* the defaults — so every command that reads one has to
/// work without it. The alternative is a toolchain you must configure before
/// you can ask it anything, including what it thinks the configuration is.
#[test]
fn every_read_only_command_succeeds_without_a_uf_config_js() {
    let dir = without_a_config();
    assert!(!dir.path().join("uf.config.js").exists());

    // The sources are still found, so "no config" means the defaults rather
    // than an empty project that trivially passes everything below.
    let (_, stdout, _) = run(dir.path(), &["lint", "--json"]);
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");
    assert_eq!(report["filesChecked"], 2, "{stdout}");

    for args in READ_ONLY {
        let (code, stdout, stderr) = run(dir.path(), args);
        assert_eq!(
            code,
            SUCCESS,
            "`uf {}` exited {code}\nstdout: {stdout}\nstderr: {stderr}",
            args.join(" ")
        );
    }

    // And it says so, rather than pretending a file was read: the answer to
    // "which configuration is this" must not be silence.
    let (_, stdout, _) = run(dir.path(), &["inspect"]);
    assert!(stdout.contains("zero-config defaults"), "{stdout}");
}

/// A path argument that names nothing is refused, by every command that takes
/// one and in the same words.
///
/// Silence is the dangerous answer here. `uf lint pacakges/ui` that reports no
/// problems, and `uf test pacakges/ui` that reports no failures, are both a
/// green CI run over nothing at all — and `uf test` used to be exactly that
/// while its three siblings already refused.
#[test]
fn a_path_that_names_nothing_is_refused_by_every_command_that_takes_one() {
    let dir = scaffolded();

    for args in [
        &["check", "no-such-path"][..],
        &["fmt", "no-such-path"],
        &["fmt", "--check", "no-such-path"],
        &["lint", "no-such-path"],
        &["test", "no-such-path"],
        &["test", "--list", "no-such-path"],
    ] {
        let (code, stdout, stderr) = run(dir.path(), args);
        assert_eq!(
            code,
            FOUND_A_PROBLEM,
            "`uf {}` exited {code}\nstdout: {stdout}\nstderr: {stderr}",
            args.join(" ")
        );
        assert!(
            stderr.contains("no file matched `no-such-path`"),
            "`uf {}` did not say what matched nothing: {stderr}",
            args.join(" ")
        );
    }
}

/// And a path that does name something is kept, so the test above is not
/// passing because every path argument is refused.
#[test]
fn a_path_that_names_something_selects_it() {
    let dir = scaffolded();

    for args in [
        &["check", "app/"][..],
        &["fmt", "--check", "app/"],
        &["lint", "app/"],
        &["test", "--list", "app/"],
    ] {
        let (code, _, stderr) = run(dir.path(), args);
        assert_eq!(
            code,
            SUCCESS,
            "`uf {}` exited {code}: {stderr}",
            args.join(" ")
        );
    }

    // Narrowed, not merely accepted: the manifest at the root is outside
    // `app/`, so the linter reads fewer files than it would unfiltered.
    let (_, all, _) = run(dir.path(), &["lint", "--json"]);
    let (_, narrowed, _) = run(dir.path(), &["lint", "--json", "app/"]);
    let all: serde_json::Value = serde_json::from_str(&all).expect("JSON");
    let narrowed: serde_json::Value = serde_json::from_str(&narrowed).expect("JSON");
    assert!(
        narrowed["filesChecked"].as_u64() < all["filesChecked"].as_u64(),
        "app/ checked {} of {}",
        narrowed["filesChecked"],
        all["filesChecked"]
    );
}

/// A problem the command found is `1`, on the stream a person reads.
///
/// The other half of `an_argument_uf_cannot_parse_could_not_run`: `1` has to
/// still mean "ran, and here is what it found", or moving the parse failures
/// to `2` would have bought nothing.
#[test]
fn a_problem_the_command_found_is_one() {
    let dir = without_a_config();
    fs::write(
        dir.path().join("src/deprecated.js"),
        "// @flow\ntype B = bool;\nexport type C = B;\n",
    )
    .expect("a source with a lint error");
    fs::write(
        dir.path().join("src/unformatted.js"),
        "// @flow\nconst   a   =   1;\nexport const b: number = a;\n",
    )
    .expect("a source that needs formatting");

    let (code, stdout, _) = run(dir.path(), &["lint"]);
    assert_eq!(code, FOUND_A_PROBLEM);
    assert!(stdout.contains("flow/deprecated-type"), "{stdout}");

    let (code, stdout, _) = run(dir.path(), &["check"]);
    assert_eq!(code, FOUND_A_PROBLEM);
    assert!(stdout.contains("flow/deprecated-type"), "{stdout}");

    let (code, stdout, _) = run(dir.path(), &["fmt", "--check"]);
    assert_eq!(code, FOUND_A_PROBLEM);
    assert!(stdout.contains("src/unformatted.js"), "{stdout}");

    // `uf fmt` without `--check` fixes it rather than reporting it, which is
    // the only reason `--check` has its own exit code at all.
    assert_eq!(run(dir.path(), &["fmt"]).0, SUCCESS);
    assert_eq!(run(dir.path(), &["fmt", "--check"]).0, SUCCESS);
}

/// A `uf.config.js` uf cannot understand stops the commands that read one, and
/// names the file.
///
/// Not asserted as a number: this is the reference's "uf could not run the
/// command", which is documented as `2` and is `1` today because uf does not
/// classify its runtime errors. Pinning the `1` here would make the
/// documentation wrong on purpose; what is pinned is that it fails, that it
/// fails before doing any work, and that it says which file to open.
#[test]
fn a_config_uf_cannot_understand_stops_the_commands_that_read_one() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    fs::write(dir.path().join("uf.config.js"), "export default 42;\n").expect("a config");
    fs::create_dir_all(dir.path().join("src")).expect("a source directory");
    fs::write(dir.path().join("src/app.js"), "// @flow\n").expect("a source");

    for args in READ_ONLY.iter().chain(&[&["prepare"][..]]) {
        // `uf completion`, `uf info`, `uf env doctor` and `uf env gc` answer
        // questions about the machine rather than the project, and are expected
        // to keep answering them when the project is the thing that is broken.
        if matches!(args[0], "completion" | "info")
            || args == &["env", "doctor"]
            || args == &["env", "gc", "--dry-run"]
        {
            continue;
        }
        let (code, stdout, stderr) = run(dir.path(), args);
        assert_ne!(
            code,
            SUCCESS,
            "`uf {}` succeeded over a config it cannot read\nstdout: {stdout}",
            args.join(" ")
        );
        assert!(
            stderr.contains("uf.config.js"),
            "`uf {}` did not name the file: {stderr}",
            args.join(" ")
        );
    }

    // The three that do not read the project still work, because the first
    // thing anyone does with a broken project is ask uf about itself.
    assert_eq!(run(dir.path(), &["info"]).0, SUCCESS);
    assert_eq!(run(dir.path(), &["completion", "bash"]).0, SUCCESS);
    assert_eq!(run(dir.path(), &["env", "doctor"]).0, SUCCESS);
    assert_eq!(run(dir.path(), &["env", "gc", "--dry-run"]).0, SUCCESS);
}

/// A file uf cannot read fails the run and names it, rather than being skipped.
///
/// And the rest of the project is still processed: one unreadable path is not
/// a reason to do nothing. A directory the walk cannot open is the same case —
/// it used to end the walk, so `uf lint` reported the failure and nothing else.
#[test]
fn an_unreadable_path_fails_the_run_and_the_rest_is_still_done() {
    use std::os::unix::fs::PermissionsExt;

    let dir = without_a_config();
    // A mode of `0o000` does not stop a process with `CAP_DAC_OVERRIDE` —
    // root in a container, which is how some CI images run. Asserting anyway
    // would make a correct scanner look broken there, so the test asks first.
    let probe = dir.path().join("src/probe.js");
    fs::write(&probe, "// @flow\n").expect("a probe");
    let enforced = fs::set_permissions(&probe, fs::Permissions::from_mode(0o000)).is_ok()
        && fs::read_to_string(&probe).is_err();
    let _ = fs::set_permissions(&probe, fs::Permissions::from_mode(0o644));
    fs::remove_file(&probe).expect("to take the probe away");
    if !enforced {
        return;
    }

    fs::write(dir.path().join("src/blob.js"), [0xff, 0xfe, 0xfa]).expect("a source that is bytes");
    fs::create_dir_all(dir.path().join("src/secret")).expect("a directory");
    fs::write(dir.path().join("src/secret/hidden.js"), "// @flow\n").expect("a source");
    fs::set_permissions(
        dir.path().join("src/secret"),
        fs::Permissions::from_mode(0o000),
    )
    .expect("to lock the directory");

    let lint = run(dir.path(), &["lint"]);
    let fmt = run(dir.path(), &["fmt", "--check"]);
    let _ = fs::set_permissions(
        dir.path().join("src/secret"),
        fs::Permissions::from_mode(0o755),
    );

    // The readable files were still processed. Before an unreadable directory
    // was reported rather than returned, `uf lint` printed that one error and
    // stopped, having linted nothing at all — the linter reads `package.json`
    // and `src/app.js`, and the formatter only the latter.
    for (command, (code, stdout, stderr), processed) in [
        ("lint", lint, "files checked  2"),
        ("fmt --check", fmt, "of 1 need formatting"),
    ] {
        assert_eq!(
            code, FOUND_A_PROBLEM,
            "`uf {command}` exited {code}\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stdout.contains("src/blob.js") && stdout.contains("src/secret"),
            "`uf {command}` did not name both unreadable paths:\n{stdout}"
        );
        assert!(
            stdout.contains(processed),
            "`uf {command}` did not process the rest of the project:\n{stdout}"
        );
    }
}

/// `uf create` refuses to overwrite, and says how to mean it.
#[test]
fn create_writes_a_project_and_will_not_overwrite_one() {
    let dir = tempfile::tempdir().expect("a temporary directory");

    let (code, _, stderr) = run(dir.path(), &["create", "lib", "."]);
    assert_eq!(code, SUCCESS, "{stderr}");
    assert!(dir.path().join("uf.config.js").is_file());

    let (code, _, stderr) = run(dir.path(), &["create", "lib", "."]);
    assert_eq!(code, FOUND_A_PROBLEM, "{stderr}");
    assert!(stderr.contains("--force"), "{stderr}");

    let (code, _, stderr) = run(dir.path(), &["create", "lib", ".", "--force"]);
    assert_eq!(code, SUCCESS, "{stderr}");
}

/// `uf explain` and `uf run` refuse a name they do not have, and say what they
/// do have.
///
/// "no such thing" on its own is the least useful thing either could answer:
/// the reader has already established that.
#[test]
fn a_name_neither_explain_nor_run_has_is_refused_with_the_list() {
    let dir = scaffolded();

    let (code, _, stderr) = run(dir.path(), &["explain", "no-such-command"]);
    assert_eq!(code, FOUND_A_PROBLEM, "{stderr}");
    assert!(
        stderr.contains("dev") && stderr.contains("build"),
        "{stderr}"
    );

    let (code, _, stderr) = run(dir.path(), &["run", "no-such-task"]);
    assert_eq!(code, FOUND_A_PROBLEM, "{stderr}");
    assert!(stderr.contains("tasks:"), "{stderr}");

    // A project with no tasks at all says that instead of an empty list.
    let empty = without_a_config();
    let (code, _, stderr) = run(empty.path(), &["run", "no-such-task"]);
    assert_eq!(code, FOUND_A_PROBLEM, "{stderr}");
    assert!(stderr.contains("no tasks"), "{stderr}");
}

/// `uf prepare` is the one command here that writes: it generates the router.
///
/// With a config and without one, because a zero-config project is the one
/// most likely to be run through `uf prepare` before it has a `uf.config.js`
/// at all — and generating a router is not something a project has to opt in
/// to by writing a config file first.
#[test]
fn prepare_generates_the_router_with_or_without_a_config() {
    for (label, dir) in [
        ("scaffolded", scaffolded()),
        ("zero-config", without_a_config()),
    ] {
        let (code, stdout, stderr) = run(dir.path(), &["prepare"]);
        assert_eq!(code, SUCCESS, "{label}: exited {code}\n{stdout}{stderr}");
        assert!(
            dir.path().join("router.js").is_file(),
            "{label}: uf prepare generated no router.js"
        );
        // And what it generated is something uf itself accepts, which is the
        // only reason generating it during `uf prepare` is safe: a commit hook
        // that writes a file the linter then rejects is a hook nobody keeps.
        let (code, stdout, stderr) = run(dir.path(), &["lint"]);
        assert_eq!(
            code, SUCCESS,
            "{label}: the generated router does not lint clean\n{stdout}{stderr}"
        );
    }
}
