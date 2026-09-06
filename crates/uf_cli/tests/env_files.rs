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

/// `uf inspect` says why it could not read the environment.
///
/// The failure used to be dropped, and the section then printed `mode
/// unknown`, `files none found` and `variables 0` — which is exactly what a
/// project with no `.env` files at all looks like. The one command a person
/// runs *because* a file is wrong reported nothing wrong with it, in text and
/// as `env: null` in JSON.
///
/// Still a success, and still no values: the reason names a file and a line.
#[test]
fn inspect_says_why_the_environment_could_not_be_read() {
    let dir = project(&[(".env", "GREETING\nSECRET_TOKEN=hunter2\n")]);

    let stdout = run(dir.path(), &["inspect"]);
    assert!(stdout.contains("could not be read"), "{stdout}");
    assert!(
        stdout.contains(".env:1"),
        "the reason must name the file and the line:\n{stdout}"
    );
    assert!(
        !stdout.contains("hunter2"),
        "a value must never be printed:\n{stdout}"
    );

    let json = run(dir.path(), &["inspect", "--json"]);
    let payload: serde_json::Value = serde_json::from_str(&json).unwrap();
    let reason = payload["env"]["error"]
        .as_str()
        .unwrap_or_else(|| panic!("`env` must carry the reason rather than being null:\n{json}"));
    assert!(reason.contains(".env:1"), "{reason}");
    assert!(
        !json.contains("hunter2"),
        "a value must never reach the JSON either"
    );
}

/// A task's own `env` is not one of the values uf read from a file, and the
/// process it starts is told which is which.
///
/// `UF_ENV_INJECTED` is how one uf process tells the next one which of the
/// variables it is handing over came from a file: the next uf lets its own
/// files overrule those and nothing else. A task's `env` block is not a file
/// value — it is the configuration writing `NAME=… uf build` — so naming it in
/// the marker let `.env.production` win over the task inside a nested
/// `uf build`, reversing the one override the task was written to make.
#[test]
fn a_task_env_value_is_not_handed_on_as_a_file_value() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({\n  \
         tasks: {\n    \
         show: {\n      \
         command: 'echo \"GREETING=[$GREETING]\"; echo \"API_URL=[$API_URL]\"; \
         echo \"INJECTED=[$UF_ENV_INJECTED]\"',\n      \
         env: { GREETING: \"from the task\" },\n    \
         },\n  \
         },\n});\n",
    )
    .unwrap();
    fs::write(
        dir.path().join(".env"),
        "GREETING=from .env\nAPI_URL=https://example.test\n",
    )
    .unwrap();

    let stdout = run(dir.path(), &["run", "show"]);

    assert!(stdout.contains("GREETING=[from the task]"), "{stdout}");
    assert!(
        stdout.contains("API_URL=[https://example.test]"),
        "a name the task did not override is still the file's:\n{stdout}"
    );

    let injected = stdout
        .lines()
        .find_map(|line| line.strip_prefix("INJECTED=["))
        .and_then(|line| line.strip_suffix(']'))
        .unwrap_or_else(|| panic!("the task printed no marker:\n{stdout}"));
    assert!(
        !injected.contains("GREETING"),
        "the task's own value must not be handed on as a file value: {injected:?}"
    );
    assert!(
        injected.contains("API_URL"),
        "a value that did come from a file must still be named: {injected:?}"
    );
}

/// One of uf's own packages resolves its own environment, so `uf exec` must not
/// resolve one for it first.
///
/// `uf exec @uniflowed/test` runs `uf test`, whose mode is `test` — a suite
/// that reaches for the development database is a suite that can destroy one.
/// Loading the `development` cascade above the dispatch meant a
/// `.env.development` that does not parse stopped a command that was never
/// going to read it.
#[test]
fn a_virtual_package_is_not_stopped_by_the_development_cascade() {
    let dir = project(&[
        (".env.development", "GREETING\n"),
        (".env.test", "GREETING=from .env.test\n"),
    ]);

    let stdout = run(dir.path(), &["exec", "@uniflowed/test", "--list"]);

    assert_plain(&stdout);
}

/// `uf explain` describes the cascade a command will look for, and names the
/// prefix *this* project publishes with.
///
/// Two claims, both about wording that could mislead the person who ran it.
/// The file list is the cascade and not a reading of the disk — nothing here
/// opens a file — so a reader hunting an unset variable must not take a name
/// in it as a file that was found. And a project that configured
/// `vite: { envPrefix: … }` has to be told its own prefix: hearing `VITE_`
/// would tell it that a value already in its browser bundle is server-only.
#[test]
fn explain_describes_the_cascade_and_this_projects_client_prefix() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({ vite: { envPrefix: \"PUBLIC_\" } });\n",
    )
    .unwrap();

    let stdout = run(dir.path(), &["explain", "dev"]);

    assert!(
        stdout.contains("looks for") && stdout.contains(".env.development"),
        "the stage must say the cascade is what it looks for:\n{stdout}"
    );
    assert!(
        stdout.contains("PUBLIC_ reaches the client"),
        "the stage must name this project's prefix:\n{stdout}"
    );
    assert!(
        !stdout.contains("VITE_"),
        "and must not name the one this project replaced:\n{stdout}"
    );
}

/// A mode a nested `uf` must not overrule is not one it may be told about by
/// accident either.
///
/// The marker is inherited, and `Command::env` only adds: when a task overrides
/// every name uf was handed, there is nothing left to write and the child would
/// keep the parent's marker — still naming the variable the task just claimed.
/// This project has no `.env` files precisely so that the marker uf writes is
/// empty, which is the only case where the inherited one survives.
#[test]
fn an_inherited_marker_does_not_outlive_the_name_a_task_took_over() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({\n  \
         tasks: {\n    \
         show: {\n      \
         command: 'echo \"GREETING=[$GREETING]\"; echo \"INJECTED=[$UF_ENV_INJECTED]\"',\n      \
         env: { GREETING: \"from the task\" },\n    \
         },\n  \
         },\n});\n",
    )
    .unwrap();

    // What a `uf run` one level up would have left for this one.
    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run", "show"])
        .env("UF_ENV_INJECTED", "GREETING")
        .env("GREETING", "from the parent")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(output.status.success(), "{stdout}");

    assert!(stdout.contains("GREETING=[from the task]"), "{stdout}");
    assert!(
        stdout.contains("INJECTED=[]"),
        "the inherited marker still names the task's own value:\n{stdout}"
    );
}

/// `uf explain` says when it could not resolve the mode, rather than describing
/// the fallback as though it were the answer.
///
/// A hand-edited `.uniflowed/profile` or an `env.active` that is not a mode is
/// the fault somebody runs this command about. It still answers — that is what
/// the command is for — but the file list it prints belongs to `development`
/// and not to this project, and saying so is the difference between an answer
/// and a wrong one.
#[test]
fn explain_says_when_it_could_not_resolve_the_mode() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({ env: { active: \"local\" } });\n",
    )
    .unwrap();

    let stdout = run(dir.path(), &["explain", "dev"]);

    assert!(
        stdout.contains("could not be resolved"),
        "the stage must say the mode is not this project's:\n{stdout}"
    );
    assert!(
        stdout.contains("the fallback"),
        "and must say the mode it is describing is the default:\n{stdout}"
    );
    assert!(
        stdout.contains("is not a mode"),
        "and must carry the reason:\n{stdout}"
    );
}
