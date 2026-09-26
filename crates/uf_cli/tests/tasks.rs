#![allow(clippy::disallowed_macros)]

//! `uf run`: the graph, the concurrency, and the cache.
//!
//! Every assertion here is about a side effect a task wrote, never about how
//! long something took. A test that says "the second run was faster" passes on
//! a quiet machine and fails on a busy one, and this repository's suite runs
//! beside four other compilers.

mod support;

use std::fs;
use std::path::Path;

use support::uf;

/// A project whose `uf.config.js` is `tasks`.
fn project(tasks: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        format!("export default defineConfig({{ tasks: {tasks} }});\n"),
    )
    .unwrap();
    dir
}

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn run(root: &Path, args: &[&str]) -> Run {
    let output = uf()
        .arg("--cwd")
        .arg(root)
        .arg("run")
        .args(args)
        .output()
        .unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

#[test]
fn config_module_constants_can_define_tasks() {
    if !support::host_ready() {
        return;
    }
    let project = support::Project::new(&[]);
    project.write(
        "uf.config.js",
        r#"// @flow
import { defineConfig } from "@uniflowed/config";

const output = "ran.txt";
const tasks = {
  hello: { command: `echo hi >> ${output}` },
};

export default defineConfig({ tasks });
"#,
    );

    let run = run(project.path(), &["hello"]);
    assert!(run.ok, "stdout:\n{}\nstderr:\n{}", run.stdout, run.stderr);
    assert_eq!(lines(project.path(), "ran.txt"), vec!["hi"]);
}

fn lines(root: &Path, name: &str) -> Vec<String> {
    fs::read_to_string(root.join(name))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Two dependencies with no path between them overlap, and the proof is that
/// each one waits for the other.
///
/// Timing would not be proof. This is: `a` cannot finish until `b` has started
/// and `b` cannot finish until `a` has, so a runner that runs them one after
/// the other cannot finish either — the first one waits out its budget and
/// fails. Before this change `uf run` walked `dependsOn` in a `for` loop, and
/// this test hung there for five seconds and then reported a failure.
#[test]
fn two_independent_dependencies_run_at_once() {
    // 100 turns at 50ms is five seconds, which is far longer than starting a
    // second `sh` takes and far shorter than a test suite notices.
    let wait = |mine: &str, theirs: &str| {
        format!(
            "touch {mine}; i=0; while [ $i -lt 100 ]; do \
             if [ -f {theirs} ]; then exit 0; fi; sleep 0.05; i=$((i+1)); done; exit 1"
        )
    };
    let dir = project(&format!(
        r#"{{
          "a": {{ "command": {a:?} }},
          "b": {{ "command": {b:?} }},
          "both": {{ "command": "echo both", "dependsOn": ["a", "b"] }}
        }}"#,
        a = wait("a.started", "b.started"),
        b = wait("b.started", "a.started"),
    ));

    let run = run(dir.path(), &["both"]);
    assert!(run.ok, "stdout:\n{}\nstderr:\n{}", run.stdout, run.stderr);
}

/// And they can be told not to.
#[test]
fn one_at_a_time_is_still_available() {
    let dir = project(
        r#"{
          "a": { "command": "echo a >> order.txt" },
          "b": { "command": "echo b >> order.txt" },
          "both": { "command": "echo done", "dependsOn": ["a", "b"] }
        }"#,
    );
    let run = run(dir.path(), &["both", "--concurrency", "1"]);
    assert!(run.ok, "{}", run.stderr);
    assert_eq!(lines(dir.path(), "order.txt"), vec!["a", "b"]);
}

/// A second run of a task whose inputs have not changed does not run it.
///
/// Asserted by the file the task appends to, not by a stopwatch.
#[test]
fn a_cached_task_does_not_run_again() {
    let dir = project(
        r#"{
          "check": {
            "command": "echo went >> ran.txt; echo looked at the sources",
            "inputs": ["src/**/*.js"]
          }
        }"#,
    );
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();

    assert!(run(dir.path(), &["check"]).ok);
    assert!(run(dir.path(), &["check"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt"), vec!["went"]);

    // And it does run again when an input it named changes.
    fs::write(dir.path().join("src/a.js"), "let a = 2;\n").unwrap();
    assert!(run(dir.path(), &["check"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt"), vec!["went", "went"]);
}

/// A replayed task prints what it printed, or a second run of a green
/// pipeline looks like nothing happened.
#[test]
fn a_replayed_task_replays_its_output() {
    let dir = project(
        r#"{
          "check": {
            "command": "echo 32 files checked; echo a warning >&2",
            "inputs": ["src/**/*.js"]
          }
        }"#,
    );
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();

    let first = run(dir.path(), &["check"]);
    assert!(first.ok, "{}", first.stderr);
    assert!(
        first.stdout.contains("32 files checked"),
        "{}",
        first.stdout
    );

    let second = run(dir.path(), &["check"]);
    assert!(second.ok, "{}", second.stderr);
    assert!(
        second.stdout.contains("32 files checked"),
        "the replay printed nothing:\n{}",
        second.stdout
    );
    assert!(
        second.stderr.contains("a warning"),
        "stderr was not replayed:\n{}",
        second.stderr
    );
}

/// The escape hatch, and the reason it exists: a cache has to be able to be
/// wrong without being the end of the road.
#[test]
fn force_runs_a_task_the_cache_would_have_replayed() {
    let dir = project(
        r#"{
          "check": { "command": "echo went >> ran.txt", "inputs": ["src/**/*.js"] }
        }"#,
    );
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();

    assert!(run(dir.path(), &["check"]).ok);
    assert!(run(dir.path(), &["check"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt").len(), 1);
    assert!(run(dir.path(), &["check", "--force"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt").len(), 2);
}

/// The honest default: a task that has not said what it reads always runs.
#[test]
fn a_task_that_declares_no_inputs_runs_every_time() {
    let dir = project(r#"{ "check": { "command": "echo went >> ran.txt" } }"#);
    for _ in 0..3 {
        assert!(run(dir.path(), &["check"]).ok);
    }
    assert_eq!(lines(dir.path(), "ran.txt").len(), 3);
}

/// And `cache: false` puts a task that *has* said back into that default.
#[test]
fn cache_false_opts_a_task_with_inputs_back_out() {
    let dir = project(
        r#"{
          "check": {
            "command": "echo went >> ran.txt",
            "inputs": ["src/**/*.js"],
            "cache": false
          }
        }"#,
    );
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();
    assert!(run(dir.path(), &["check"]).ok);
    assert!(run(dir.path(), &["check"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt").len(), 2);
}

/// `cache: true` on a task with nothing to key on is a mistake uf says out
/// loud, because the alternative is uf ignoring what somebody asked for.
#[test]
fn cache_true_without_inputs_is_refused() {
    let dir = project(r#"{ "check": { "command": "echo hi", "cache": true } }"#);
    let run = run(dir.path(), &["check"]);
    assert!(!run.ok);
    assert!(
        run.stderr.contains("declares no `inputs`"),
        "{}",
        run.stderr
    );
}

/// `--why` answers the question a cache makes people ask.
#[test]
fn why_names_the_input_that_changed() {
    let dir = project(
        r#"{
          "check": { "command": "echo hi", "inputs": ["src/**/*.js"] },
          "top": { "command": "echo top", "dependsOn": ["check"] }
        }"#,
    );
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();

    assert!(run(dir.path(), &["top"]).ok);
    let replayed = run(dir.path(), &["top", "--why"]);
    assert!(replayed.stderr.contains("replayed"), "{}", replayed.stderr);
    assert!(
        replayed
            .stderr
            .contains("declares no inputs, so it always runs"),
        "the task that cannot be cached has to say so too:\n{}",
        replayed.stderr
    );

    fs::write(dir.path().join("src/a.js"), "let a = 2;\n").unwrap();
    let changed = run(dir.path(), &["top", "--why"]);
    assert!(
        changed.stderr.contains("src/a.js changed"),
        "{}",
        changed.stderr
    );

    fs::write(dir.path().join("src/b.js"), "let b = 1;\n").unwrap();
    let added = run(dir.path(), &["top", "--why"]);
    assert!(added.stderr.contains("src/b.js is new"), "{}", added.stderr);
}

/// A `.env` value is in no file a cached run writes, and `--why` names the
/// variable that changed without printing either value.
///
/// Until #1006 the note `--why` reads kept the mode and every `.env` value a
/// task was given, verbatim, under `.uf/cache/task/last/`.
#[test]
fn a_dotenv_value_is_in_no_file_the_task_cache_writes() {
    const FIRST: &str = "not-a-real-secret-4f1c2b";
    const ROTATED: &str = "not-a-real-secret-9e7a3d";
    let dir =
        project(r#"{ "check": { "command": "echo went >> ran.txt", "inputs": ["src/**/*.js"] } }"#);
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();

    fs::write(dir.path().join(".env"), format!("API_TOKEN={FIRST}\n")).unwrap();
    let first = run(dir.path(), &["check"]);
    assert!(first.ok, "{}", first.stderr);
    assert_no_file_holds(&dir.path().join(".uf"), &[FIRST]);

    fs::write(dir.path().join(".env"), format!("API_TOKEN={ROTATED}\n")).unwrap();
    let second = run(dir.path(), &["check", "--why"]);
    assert!(second.ok, "{}", second.stderr);
    assert_eq!(lines(dir.path(), "ran.txt"), vec!["went", "went"]);
    assert!(
        second.stderr.contains("API_TOKEN has a different value"),
        "--why has to name the variable:\n{}",
        second.stderr
    );
    assert!(
        !second.stderr.contains(FIRST) && !second.stderr.contains(ROTATED),
        "{}",
        second.stderr
    );
    assert_no_file_holds(&dir.path().join(".uf"), &[FIRST, ROTATED]);
}

/// No file under `directory` holds any of `values` — and there are files, so
/// a search of nothing cannot pass.
fn assert_no_file_holds(directory: &Path, values: &[&str]) {
    let mut pending = vec![directory.to_path_buf()];
    let mut searched = 0;
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            for value in values {
                assert!(
                    !bytes
                        .windows(value.len())
                        .any(|window| window == value.as_bytes()),
                    "{} holds {value}",
                    path.display()
                );
            }
            searched += 1;
        }
    }
    // A record and a note, at the least.
    assert!(
        searched >= 2,
        "{searched} files under {}",
        directory.display()
    );
}

/// A result is only true while the files it produced are still the ones it
/// produced.
#[test]
fn a_deleted_output_is_rebuilt_rather_than_reported_as_done() {
    let dir = project(
        r#"{
          "build": {
            "command": "mkdir -p out; echo built > out/app.js; echo went >> ran.txt",
            "inputs": ["src/**/*.js"],
            "outputs": ["out/**"]
          }
        }"#,
    );
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();

    assert!(run(dir.path(), &["build"]).ok);
    assert!(run(dir.path(), &["build"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt").len(), 1);

    fs::remove_file(dir.path().join("out/app.js")).unwrap();
    let rebuilt = run(dir.path(), &["build", "--why"]);
    assert!(rebuilt.ok, "{}", rebuilt.stderr);
    assert!(
        rebuilt.stderr.contains("out/app.js is missing"),
        "{}",
        rebuilt.stderr
    );
    assert_eq!(lines(dir.path(), "ran.txt").len(), 2);
    assert!(dir.path().join("out/app.js").is_file());
}

/// Two tasks sharing a terminal have to be told apart.
#[test]
fn a_dependency_writes_behind_its_own_name() {
    let dir = project(
        r#"{
          "one": { "command": "echo from-one" },
          "top": { "command": "echo from-top", "dependsOn": ["one"] }
        }"#,
    );
    let run = run(dir.path(), &["top"]);
    assert!(run.ok, "{}", run.stderr);
    assert!(run.stdout.contains("one | from-one"), "{}", run.stdout);
    // And the task that was asked for keeps stdout as it found it: it runs
    // alone, so there is nothing to disambiguate it from.
    assert!(
        run.stdout.contains("\nfrom-top\n") || run.stdout.starts_with("from-top\n"),
        "{}",
        run.stdout
    );
}

/// A `dependsOn` that closes a loop used to be a run that quietly did half the
/// work: the visited set swallowed the second visit and the task it belonged
/// to never ran.
#[test]
fn a_dependency_cycle_is_reported() {
    let dir = project(
        r#"{
          "a": { "command": "echo a", "dependsOn": ["b"] },
          "b": { "command": "echo b", "dependsOn": ["a"] }
        }"#,
    );
    let run = run(dir.path(), &["a"]);
    assert!(!run.ok, "{}", run.stdout);
    assert!(run.stderr.contains("closes a loop"), "{}", run.stderr);
    assert!(run.stderr.contains("a → b → a"), "{}", run.stderr);
}

/// A `dependsOn` naming a task nobody defined says which task asked.
#[test]
fn an_undefined_dependency_names_the_task_that_wants_it() {
    let dir = project(r#"{ "a": { "command": "echo a", "dependsOn": ["bulid"] } }"#);
    let run = run(dir.path(), &["a"]);
    assert!(!run.ok);
    assert!(run.stderr.contains("\"a\" depends on it"), "{}", run.stderr);
}

/// A failing dependency stops the run, and the task that wanted it does not
/// run at all.
#[test]
fn a_failed_dependency_stops_the_task_that_wanted_it() {
    let dir = project(
        r#"{
          "bad": { "command": "exit 3" },
          "top": { "command": "echo went >> ran.txt", "dependsOn": ["bad"] }
        }"#,
    );
    let run = run(dir.path(), &["top"]);
    assert!(!run.ok);
    assert!(run.stderr.contains("\"bad\" exited with"), "{}", run.stderr);
    assert!(!dir.path().join("ran.txt").exists());
}

/// Two runs with different trailing arguments are two results, not one.
#[test]
fn arguments_are_part_of_what_is_cached() {
    let dir = project(
        r#"{
          "say": { "command": "echo said >> ran.txt; echo", "inputs": ["src/**/*.js"] }
        }"#,
    );
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();

    assert!(run(dir.path(), &["say", "one"]).ok);
    assert!(run(dir.path(), &["say", "one"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt").len(), 1);
    assert!(run(dir.path(), &["say", "two"]).ok);
    assert_eq!(lines(dir.path(), "ran.txt").len(), 2);
}

// --- Declared arguments --------------------------------------------------

/// A task that prints each word it is given on a line of its own, and
/// declares three arguments: one with choices, one free, one defaulted.
fn deploy() -> tempfile::TempDir {
    project(
        r#"{
          "deploy": {
            "command": "printf '%s\\n'",
            "args": [
              { "name": "target", "description": "Where to deploy", "choices": ["staging", "production"] },
              { "name": "tag" },
              { "name": "region", "choices": ["us", "eu"], "default": "eu" }
            ]
          }
        }"#,
    )
}

fn printed(run: &Run) -> Vec<&str> {
    run.stdout.lines().collect()
}

/// Positional or by name, the values reach the command in declared order,
/// the default fills what was not given, and anything undeclared follows.
#[test]
fn declared_arguments_are_given_in_order_or_by_name() {
    let dir = deploy();

    let run = run(dir.path(), &["deploy", "staging", "v1"]);
    assert!(run.ok, "{}", run.stderr);
    assert_eq!(printed(&run), ["staging", "v1", "eu"]);

    let run = run_named(dir.path());
    assert!(run.ok, "{}", run.stderr);
    assert_eq!(
        printed(&run),
        ["production", "two words", "us", "--dry-run"]
    );
}

fn run_named(root: &Path) -> Run {
    run(
        root,
        &[
            "deploy",
            "--tag",
            "two words",
            "--dry-run",
            "--region=us",
            "--target",
            "production",
        ],
    )
}

/// A value outside `choices` is refused by the argument's name, and nothing
/// runs.
#[test]
fn a_value_outside_the_choices_is_refused_by_name() {
    let dir = deploy();

    let run = run(dir.path(), &["deploy", "prod", "v1"]);
    assert!(!run.ok);
    assert!(run.stdout.is_empty(), "{}", run.stdout);
    assert!(
        run.stderr.contains(
            "task \"deploy\": <target> cannot be \"prod\"; it is one of: staging, production"
        ),
        "{}",
        run.stderr
    );
}

/// With nobody at a terminal — this test's stdin is not one — a missing
/// required argument is an error naming it and its choices, not a prompt that
/// waits for ever. `CI` alone is enough to mean the same.
#[test]
fn a_missing_argument_off_a_terminal_is_an_error_that_names_it() {
    let dir = deploy();

    for ci in [false, true] {
        let mut command = uf();
        command.arg("--cwd").arg(dir.path()).args(["run", "deploy"]);
        if ci {
            command.env("CI", "true");
        } else {
            command.env_remove("CI");
        }
        let output = command.output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{stderr}");
        assert!(output.stdout.is_empty());
        assert!(stderr.contains("missing <target>, <tag>"), "{stderr}");
        assert!(
            stderr.contains("<target>  Where to deploy — one of: staging, production"),
            "{stderr}"
        );
        assert!(
            stderr.contains("`uf run deploy <target> <tag> [region=eu]`"),
            "{stderr}"
        );
    }
}

/// A declaration `uf run` could not fill is refused before anything runs.
#[test]
fn an_argument_declaration_uf_cannot_fill_is_refused() {
    let dir = project(
        r#"{
          "t": { "command": "printf x", "args": [{ "name": "note", "required": false }, { "name": "then" }] }
        }"#,
    );
    let run = run(dir.path(), &["t", "a", "b"]);
    assert!(!run.ok);
    assert!(
        run.stderr
            .contains("<note> may be left out and has no default"),
        "{}",
        run.stderr
    );
}

/// Off a terminal, `uf run` alone still lists — now with what each task takes
/// — and `--list` lists anywhere.
#[test]
fn listing_shows_each_tasks_arguments() {
    let dir = deploy();
    for args in [&[][..], &["--list"][..]] {
        let run = run(dir.path(), args);
        assert!(run.ok, "{}", run.stderr);
        assert!(
            run.stdout
                .contains("printf '%s\\n' <target> <tag> [region=eu]"),
            "{}",
            run.stdout
        );
    }
}

/// `uf explain run` says which tasks declare arguments and what happens to a
/// missing one.
#[test]
fn explain_names_the_tasks_that_declare_arguments() {
    let dir = deploy();
    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["explain", "run"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("declared by deploy"), "{stdout}");
}

// --- What runs the command ------------------------------------------------

/// A command that is a program and its arguments is started by uf.
///
/// The proof is the error, because that is the one place the two paths cannot
/// be confused: a shell that cannot find a program says so itself and exits
/// 127, and uf reports "exited with". uf starting it itself fails before
/// there is a process at all, and names the program.
#[test]
fn a_program_uf_cannot_find_is_named_rather_than_left_to_a_shell() {
    let dir = project(r#"{ "gone": { "command": "uf-no-such-program --check" } }"#);

    let run = run(dir.path(), &["gone"]);
    assert!(!run.ok);
    assert!(
        run.stderr.contains("could not start `uf-no-such-program`"),
        "{}",
        run.stderr
    );
}

/// And a command that needs one is still a shell's, unchanged.
#[test]
fn a_command_with_shell_syntax_still_goes_to_the_shell() {
    let dir = project(r#"{ "gone": { "command": "uf-no-such-program --check; true" } }"#);

    let run = run(dir.path(), &["gone"]);
    assert!(run.ok, "{}", run.stderr);
}

/// Quoting is the shell's, in full: one argument, `#` and all.
///
/// `printf '%s\n'` writes one line per argument, so this fails loudly if the
/// quoted argument is split, if the `#` starts a comment — `uf test#library`
/// is a real task here — or if the backslash escape is dropped.
#[test]
fn words_are_split_and_quotes_removed_the_way_a_shell_would() {
    let dir = project(
        r#"{ "say": { "command": "printf '%s\\n' one 'two words' a#b \"three  spaces\" ''" } }"#,
    );

    let run = run(dir.path(), &["say"]);
    assert!(run.ok, "{}", run.stderr);
    assert_eq!(
        run.stdout.lines().collect::<Vec<_>>(),
        ["one", "two words", "a#b", "three  spaces", ""]
    );
}

/// `NAME=value` in front of the program is the child's environment.
///
/// There is no shell syntax in this command, so uf is the one applying it —
/// which is the point: a task that sets a variable in front of its program
/// does not need a shell to do it.
#[test]
fn an_inline_assignment_reaches_the_task_without_a_shell() {
    let dir =
        project(r#"{ "show": { "command": "MESSAGE='from the command' printenv MESSAGE" } }"#);

    let run = run(dir.path(), &["show"]);
    assert!(run.ok, "{}", run.stderr);
    assert_eq!(run.stdout.trim(), "from the command");
}

/// A relative program is resolved against the directory the task runs in.
#[test]
fn a_relative_program_is_resolved_against_the_tasks_directory() {
    let dir = project(r#"{ "here": { "command": "./say.sh", "cwd": "sub" } }"#);
    fs::create_dir_all(dir.path().join("sub")).unwrap();
    let script = dir.path().join("sub/say.sh");
    fs::write(&script, "#!/bin/sh\necho from sub\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let run = run(dir.path(), &["here"]);
    assert!(run.ok, "{}", run.stderr);
    assert_eq!(run.stdout.trim(), "from sub");
}

/// A command uf cannot read is refused before anything is started.
#[test]
fn an_unclosed_quote_is_refused_by_name() {
    let dir = project(r#"{ "bad": { "command": "echo 'unclosed" } }"#);

    let run = run(dir.path(), &["bad"]);
    assert!(!run.ok);
    assert!(
        run.stderr.contains("its command cannot be read")
            && run.stderr.contains("never closed")
            // Named once, by the runner, rather than twice.
            && run.stderr.matches("\"bad\"").count() == 1,
        "{}",
        run.stderr
    );
}

/// The cache lives where the other two do.
#[test]
fn records_are_written_under_the_projects_uf_directory() {
    let dir = project(r#"{ "check": { "command": "echo hi", "inputs": ["src/**/*.js"] } }"#);
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.js"), "let a = 1;\n").unwrap();
    assert!(run(dir.path(), &["check"]).ok);
    let entries = fs::read_dir(dir.path().join(".uf/cache/task"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|kind| kind == "json"))
        .count();
    assert_eq!(entries, 1);
}

// --- across a workspace ------------------------------------------------------

/// Three members, `utils` ← `ui` ← `app` by `package.json`, each with a
/// cacheable `build` that appends its own name to `order.txt` at the root —
/// which is the whole record of what ran, and in what order.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let write = |path: &str, contents: &str| {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    };
    write(
        "package.json",
        r#"{ "name": "root", "private": true, "workspaces": ["packages/*"] }"#,
    );
    write(
        "uf.config.js",
        "export default defineConfig({ tasks: {} });\n",
    );
    for (name, depends_on) in [
        ("utils", ""),
        ("ui", r#""utils": "workspace:*""#),
        ("app", r#""ui": "workspace:*""#),
    ] {
        write(
            &format!("packages/{name}/package.json"),
            &format!(r#"{{ "name": "{name}", "dependencies": {{ {depends_on} }} }}"#),
        );
        write(
            &format!("packages/{name}/src/index.js"),
            &format!("export const name = {name:?};\n"),
        );
        write(
            &format!("packages/{name}/uf.config.js"),
            &format!(
                "export default defineConfig({{ tasks: {{ build: {{ command: \"echo {name} >> ../../order.txt\", inputs: [\"src/**\"] }} }} }});\n"
            ),
        );
    }
    dir
}

/// `uf run build -r` runs every member's `build` after the members it depends
/// on, keeps each member's results in that member, and — once one member has
/// changed — runs that member and what depends on it again, and replays the
/// rest.
#[test]
fn a_task_runs_across_a_workspace_in_dependency_order_with_a_cache_per_package() {
    let dir = workspace();
    let root = dir.path();

    let first = run(root, &["build", "-r"]);
    assert!(
        first.ok,
        "stdout:\n{}\nstderr:\n{}",
        first.stdout, first.stderr
    );
    assert_eq!(lines(root, "order.txt"), vec!["utils", "ui", "app"]);
    for member in ["utils", "ui", "app"] {
        assert!(
            root.join("packages")
                .join(member)
                .join(".uf/cache/task")
                .is_dir(),
            "{member} keeps no records of its own"
        );
    }
    assert!(
        !root.join(".uf/cache/task").exists(),
        "the workspace root ran nothing, so it has nothing to record"
    );

    let second = run(root, &["build", "-r"]);
    assert!(second.ok, "{}", second.stderr);
    assert_eq!(
        lines(root, "order.txt"),
        vec!["utils", "ui", "app"],
        "nothing changed, so nothing ran again"
    );

    fs::write(
        root.join("packages/ui/src/index.js"),
        "export const name = \"ui, changed\";\n",
    )
    .unwrap();
    let third = run(root, &["build", "-r", "--why"]);
    assert!(third.ok, "{}", third.stderr);
    assert_eq!(
        lines(root, "order.txt"),
        vec!["utils", "ui", "app", "ui", "app"],
        "ui changed: ui and app run again, and utils is replayed"
    );
    assert!(
        third.stderr.lines().any(|line| {
            line.trim_start().starts_with("app#build") && line.ends_with("after ui#build")
        }),
        "--why prints the graph the run spans:\n{}",
        third.stderr
    );
    assert!(
        third.stderr.contains("ui#build changed"),
        "--why names the member whose change reran app:\n{}",
        third.stderr
    );
}

#[test]
fn filter_selects_members_by_name_path_glob_and_what_they_depend_on() {
    let dir = workspace();
    let root = dir.path();
    let ran = |selector: &str| -> Vec<String> {
        let _ = fs::remove_file(root.join("order.txt"));
        let run = run(root, &["build", "--force", "--filter", selector]);
        assert!(
            run.ok,
            "--filter {selector}\nstdout:\n{}\nstderr:\n{}",
            run.stdout, run.stderr
        );
        lines(root, "order.txt")
    };

    assert_eq!(ran("ui"), vec!["ui"]);
    assert_eq!(ran("./packages/utils"), vec!["utils"]);
    assert_eq!(ran("u*"), vec!["utils", "ui"]);
    assert_eq!(ran("app..."), vec!["utils", "ui", "app"]);
    assert_eq!(ran("app^..."), vec!["utils", "ui"]);
    assert_eq!(ran("...ui"), vec!["ui", "app"]);
    assert_eq!(ran("...^utils"), vec!["ui", "app"]);
}

/// A misspelt `--filter` that ran nothing and exited 0 would be a check that
/// passed without running.
#[test]
fn a_filter_that_selects_nothing_is_refused() {
    let dir = workspace();
    let run = run(dir.path(), &["build", "--filter", "nope"]);
    assert!(!run.ok);
    assert!(
        run.stderr.contains("selects no workspace member"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("members: app, ui, utils"),
        "{}",
        run.stderr
    );
}

#[test]
fn depends_on_names_a_task_in_another_member() {
    let dir = workspace();
    let root = dir.path();
    fs::write(
        root.join("uf.config.js"),
        "export default defineConfig({ tasks: { release: { command: \"echo root >> order.txt\", dependsOn: [\"app#build\"] } } });\n",
    )
    .unwrap();

    let release = run(root, &["release"]);
    assert!(
        release.ok,
        "stdout:\n{}\nstderr:\n{}",
        release.stdout, release.stderr
    );
    assert_eq!(lines(root, "order.txt"), vec!["app", "root"]);

    // From inside a member, where the sibling it names is only found through
    // the workspace above it.
    fs::write(
        root.join("packages/app/uf.config.js"),
        "export default defineConfig({ tasks: { build: { command: \"echo app >> ../../order.txt\", dependsOn: [\"ui#build\"] } } });\n",
    )
    .unwrap();
    fs::remove_file(root.join("order.txt")).unwrap();
    let inside = run(&root.join("packages/app"), &["build"]);
    assert!(
        inside.ok,
        "stdout:\n{}\nstderr:\n{}",
        inside.stdout, inside.stderr
    );
    assert_eq!(lines(root, "order.txt"), vec!["ui", "app"]);
}

#[test]
fn a_member_the_workspace_does_not_have_is_named_before_anything_runs() {
    let dir = workspace();
    let root = dir.path();
    fs::write(
        root.join("uf.config.js"),
        "export default defineConfig({ tasks: { release: { command: \"echo root >> order.txt\", dependsOn: [\"ap#build\"] } } });\n",
    )
    .unwrap();

    let release = run(root, &["release"]);
    assert!(!release.ok);
    assert!(
        release.stderr.contains("no workspace member named \"ap\""),
        "{}",
        release.stderr
    );
    assert!(release.stderr.contains("app"), "{}", release.stderr);
    assert!(!root.join("order.txt").exists());
}
