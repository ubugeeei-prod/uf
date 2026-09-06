//! `.env` files, from the command line: which files a command reads, what
//! wins, what it says when a file is malformed, and that `uf env use` writes
//! something that is then actually read.
//!
//! Everything here except [`the_test_runner_sees_the_environment`] runs without
//! Node: `uf run` is `sh -c`, so a task that prints a variable is the cheapest
//! honest proof that a value reached a process uf started. The build and the
//! dev server are in `tests/vite.rs`, where the fixtures that need a bundler
//! live — including the one that matters most, that a value without the client
//! prefix is absent from `dist/`.
//!
//! Before this, none of it happened: `env.files` and `env.active` had no
//! reader, `uf env use` wrote `.uniflowed/env` — the path `uf env install`
//! needs as a *directory* — and the driver switched Vite's loader off. See
//! ubugeeei-prod/uf#259.

mod support;

use std::fs;
use std::path::Path;

use support::{Project, assert_plain, host_ready, uf};

/// A project whose one task prints the variables these tests ask about.
///
/// Each value is printed inside brackets and in its own quoted `echo`, so a
/// variable that is unset reads as `GREETING=[]` rather than as nothing at all,
/// and a value with two spaces in it still has two spaces here.
fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({\n  \
         tasks: {\n    \
         show: 'echo \"GREETING=[$GREETING]\"; echo \"API_URL=[$API_URL]\"; \
         echo \"SECRET_TOKEN=[$SECRET_TOKEN]\"',\n  },\n});\n",
    )
    .unwrap();
    for (name, contents) in files {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    dir
}

/// Run `uf <args>` in `root` and return its stdout, failing loudly if it did.
fn run(root: &Path, args: &[&str]) -> String {
    let output = uf().arg("--cwd").arg(root).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "`uf {}` failed:\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// The same, for a command that must fail: its stdout and stderr together.
fn refuse(root: &Path, args: &[&str]) -> String {
    let output = uf().arg("--cwd").arg(root).args(args).output().unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "this had to fail:\n{said}");
    said
}

#[test]
fn a_task_reads_the_cascade_in_the_documented_order() {
    let dir = project(&[
        (".env", "GREETING=from .env\nAPI_URL=https://example.test\n"),
        (".env.local", "GREETING=from .env.local\n"),
        (".env.development", "GREETING=from .env.development\n"),
    ]);

    let stdout = run(dir.path(), &["run", "show"]);

    // `.env.development` is later in the cascade than `.env.local`, so it wins;
    // `API_URL` came from `.env` because nothing else mentioned it.
    assert!(
        stdout.contains("GREETING=[from .env.development]"),
        "{stdout}"
    );
    assert!(
        stdout.contains("API_URL=[https://example.test]"),
        "{stdout}"
    );
}

/// The bug in the issue's title, from the other end: a mode picks the file.
#[test]
fn the_mode_chooses_which_file_wins() {
    let dir = project(&[
        (".env", "GREETING=shared\n"),
        (".env.development", "GREETING=development\n"),
        (".env.production", "GREETING=production\n"),
    ]);

    assert!(run(dir.path(), &["run", "show"]).contains("GREETING=[development]"));
    assert!(
        run(dir.path(), &["run", "--mode", "production", "show"]).contains("GREETING=[production]"),
        "--mode must select `.env.<mode>`"
    );
}

#[test]
fn the_process_environment_beats_every_file() {
    let dir = project(&[(".env", "GREETING=from the file\n")]);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run", "show"])
        .env("GREETING", "from the shell")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("GREETING=[from the shell]"), "{stdout}");
    assert!(!stdout.contains("from the file"), "{stdout}");
}

/// Quoting, escapes, `export`, comments and interpolation, end to end.
///
/// The unit tests in `uf_config::env_files` own the rules one at a time; this
/// is the one that says the rules are the ones a person meets.
#[test]
fn the_parser_handles_quotes_comments_and_interpolation() {
    let dir = project(&[(
        ".env",
        "# the host everything else is built from\n\
         export HOST=example.test\n\
         API_URL=\"https://${HOST}/api\"  # trailing comment\n\
         GREETING='hello  world'\n\
         SECRET_TOKEN=abc\\$123\n",
    )]);

    let stdout = run(dir.path(), &["run", "show"]);

    assert!(
        stdout.contains("API_URL=[https://example.test/api]"),
        "{stdout}"
    );
    assert!(stdout.contains("GREETING=[hello  world]"), "{stdout}");
    assert!(stdout.contains("SECRET_TOKEN=[abc$123]"), "{stdout}");
}

#[test]
fn a_malformed_file_stops_the_command_and_names_the_line() {
    let dir = project(&[(".env", "GREETING=fine\nthis is not a line\n")]);

    let said = refuse(dir.path(), &["run", "show"]);

    assert!(said.contains(".env:2"), "{said}");
    assert!(said.contains("has no `=`"), "{said}");

    // And a name that could not be one, which is the other half of the rule.
    let dir = project(&[(".env", "1GREETING=nope\n")]);
    let said = refuse(dir.path(), &["run", "show"]);
    assert!(said.contains(".env:1"), "{said}");
    assert!(said.contains("is not a variable name"), "{said}");
}

#[test]
fn an_undefined_interpolation_is_refused_rather_than_emptied() {
    let dir = project(&[(".env", "API_URL=https://$TYPOED/api\n")]);

    let said = refuse(dir.path(), &["run", "show"]);

    assert!(said.contains("$TYPOED"), "{said}");
    assert!(said.contains("is not defined"), "{said}");
}

/// `env.files` replaces the cascade, which is the other half of "a key that is
/// a decoration is worse than no key".
#[test]
fn env_files_in_the_config_replaces_the_cascade() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({\n  \
         env: { files: [\"config/shared.env\"] },\n  \
         tasks: { show: 'echo \"GREETING=[$GREETING]\"' },\n});\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("config")).unwrap();
    fs::write(
        dir.path().join("config/shared.env"),
        "GREETING=named file\n",
    )
    .unwrap();
    fs::write(dir.path().join(".env"), "GREETING=the cascade\n").unwrap();

    let stdout = run(dir.path(), &["run", "show"]);

    assert!(stdout.contains("GREETING=[named file]"), "{stdout}");
    assert!(!stdout.contains("the cascade"), "{stdout}");
}

/// `uf env use` writes something, and the next command reads it.
///
/// This is the round trip the issue asks for. It used to write
/// `.uniflowed/env`, which nothing read and which `uf env install` needs as a
/// directory.
#[test]
fn env_use_records_a_profile_that_later_commands_read() {
    let dir = project(&[
        (".env", "GREETING=shared\n"),
        (".env.development", "GREETING=development\n"),
        (".env.staging", "GREETING=staging\n"),
    ]);

    assert!(run(dir.path(), &["run", "show"]).contains("GREETING=[development]"));

    let said = run(dir.path(), &["env", "use", "staging"]);
    assert!(said.contains("staging"), "{said}");
    assert_plain(&said);
    assert!(
        dir.path().join(".uniflowed/profile").is_file(),
        "the profile is written where `uf env install` does not need the name"
    );

    assert!(
        run(dir.path(), &["run", "show"]).contains("GREETING=[staging]"),
        "the profile must choose the mode for later commands"
    );
    // And what was typed still wins over what was recorded.
    assert!(
        run(dir.path(), &["run", "--mode", "development", "show"])
            .contains("GREETING=[development]")
    );
}

/// `uf env use` and `uf env install` no longer claim one path.
#[test]
fn env_use_leaves_the_toolchain_directory_alone() {
    let dir = project(&[]);
    fs::create_dir_all(dir.path().join(".uniflowed/env/bin")).unwrap();
    fs::write(dir.path().join(".uniflowed/env/bin/node"), "").unwrap();

    run(dir.path(), &["env", "use", "staging"]);

    assert!(
        dir.path().join(".uniflowed/env/bin/node").is_file(),
        "`uf env use` must not write over `uf env install`'s directory"
    );
}

/// A profile becomes a file name, so it is checked before it is written.
#[test]
fn a_profile_that_could_escape_the_project_is_refused() {
    let dir = project(&[]);

    let said = refuse(dir.path(), &["env", "use", "../../etc"]);

    assert!(said.contains("is not a mode"), "{said}");
    assert!(!dir.path().join(".uniflowed/profile").exists());
}

/// `uf inspect` answers "where do these values come from" — with names, and
/// never with values.
#[test]
fn inspect_reports_the_mode_and_the_files_but_no_values() {
    let dir = project(&[
        (".env", "SECRET_TOKEN=hunter2\n"),
        (".env.development", "GREETING=hello\n"),
    ]);

    let stdout = run(dir.path(), &["inspect"]);
    assert!(stdout.contains("environment"), "{stdout}");
    assert!(stdout.contains(".env, .env.development"), "{stdout}");
    assert!(stdout.contains("VITE_"), "{stdout}");
    assert!(
        !stdout.contains("hunter2"),
        "a value must never be printed:\n{stdout}"
    );

    let json = run(dir.path(), &["inspect", "--json"]);
    let payload: serde_json::Value = serde_json::from_str(&json).unwrap();
    let env = &payload["env"];
    assert_eq!(env["mode"], "development");
    assert_eq!(
        env["variables"],
        serde_json::json!(["GREETING", "SECRET_TOKEN"])
    );
    assert_eq!(env["clientVisible"], serde_json::json!([]));
    assert!(
        !json.contains("hunter2"),
        "a value must never reach the JSON either"
    );
}

/// `uf exec` runs a tool against this project, so it gets this project's values.
///
/// A path rather than an installed binary, which is the one of `uf exec`'s
/// three paths that needs no `node_modules`; all three are given the same
/// environment in the same place.
#[cfg(unix)]
#[test]
fn exec_runs_a_command_with_the_projects_values() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project(&[(".env", "GREETING=for the tool\n")]);
    let script = dir.path().join("show.sh");
    fs::write(&script, "#!/bin/sh\necho \"GREETING=[$GREETING]\"\n").unwrap();
    fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let stdout = run(dir.path(), &["exec", "./show.sh"]);

    assert!(stdout.contains("GREETING=[for the tool]"), "{stdout}");
}

/// `uf test`'s workers get the same values, from `.env.test`.
///
/// The mode is `test`, not `development`: a suite that reaches for the
/// development database is a suite that can destroy it, so the file that names
/// the test one has to be the file that wins.
///
/// The `.env` here also sets `UF_BINARY`, which is uf's own — it names the
/// binary every module in the run is transformed through, and a cloned
/// repository's `.env` must not be able to answer it. uf's variables are
/// written *after* the project's for exactly that reason, so this run has to
/// succeed; obey the file and every import in it fails.
#[test]
fn the_test_runner_sees_the_environment() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[
        (
            "app/env.test.js",
            "// @flow\nimport { expect, test } from \"@uniflowed/test\";\n\n\
             test(\"the environment file reached the worker\", () => {\n  \
             expect(process.env.UF_FIXTURE_GREETING).toBe(\"from .env.test\");\n\
             });\n",
        ),
        (
            ".env",
            "UF_FIXTURE_GREETING=from .env\nUF_BINARY=/definitely-not-a-binary\n",
        ),
        (
            ".env.development",
            "UF_FIXTURE_GREETING=from .env.development\n",
        ),
        (".env.test", "UF_FIXTURE_GREETING=from .env.test\n"),
    ]);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .arg("test")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
