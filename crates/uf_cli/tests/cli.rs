//! End-to-end coverage for the commands that scaffold, build, and serve.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use support::{Project, assert_plain, binary, create_app, uf};

#[test]
fn uf_prints_help() {
    let output = uf().arg("--help").output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Unified Toolchain for Flow (React)"));
    assert!(stdout.contains("--color"));
}

/// Every command `uf --help` lists must say what it does.
///
/// clap prints the first line of a command's doc comment beside its name, and
/// prints nothing at all when there is no doc comment. Fifteen of the twenty
/// commands had none, so the front door of the toolchain was a list of bare
/// verbs — `build`, `check`, `create`, `dev` — with a description beside
/// exactly one of them.
#[test]
fn every_command_in_the_help_says_what_it_does() {
    let output = uf().arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    let commands = stdout
        .split_once("Commands:")
        .expect("help lists commands")
        .1
        .split_once("\nOptions:")
        .expect("commands come before options")
        .0;

    let undescribed = commands
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("     "))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            words.next().is_none().then_some(name)
        })
        .collect::<Vec<_>>();

    assert!(
        undescribed.is_empty(),
        "these commands have no description in `uf --help`: {}",
        undescribed.join(", ")
    );
}

/// `uf i` is `uf install`.
///
/// The one command a person types before anything else works, and every
/// package manager they have used has a one-letter form of it.
#[test]
fn install_has_a_one_letter_alias() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\
         export default defineConfig({ app: { router: { enabled: false } } });\n",
    )
    .unwrap();

    let long = uf()
        .current_dir(dir.path())
        .arg("install")
        .output()
        .unwrap();
    let short = uf().current_dir(dir.path()).arg("i").output().unwrap();

    assert_eq!(
        short.status.code(),
        long.status.code(),
        "`uf i` must be `uf install`, got {}",
        String::from_utf8_lossy(&short.stderr)
    );

    // Compared over uf's own rendering, and not over the package manager's.
    //
    // `uf install` prints its card and banner, hands the terminal to npm —
    // whose stdout is this same inherited pipe — and prints the key-values and
    // the verdict when it comes back. npm's lines are not evidence about the
    // alias, and they are not the same twice: the first of these two runs
    // populates what the second finds already there, so one says
    // `up to date in 186ms` and the other says nothing at all.
    //
    // Dropping every line containing `ms` was an attempt at the same idea, and
    // npm defeated it twice. It filtered by the substring rather than by the
    // shape, so `up to date in 1s` — what npm prints when the install took a
    // second, which is exactly the run that differs from its warm twin —
    // survived and failed the comparison. That is #321 and #341, the same
    // failure filed by two people. Widening the filter to match `1s` too would
    // still be guessing at which of a child process's lines to ignore; naming
    // the block uf rendered says what the test is for.
    //
    // uf's own block does carry one thing that is not the same twice: the
    // durations it reports. Those are blanked by their *grammar* — a number
    // followed by one of `uf_term::push_duration`'s four units — rather than
    // by dropping the lines that hold them, so the timings and the verdict are
    // still compared for everything except how long they took.
    let rendered = |output: &[u8]| {
        let text = String::from_utf8_lossy(output).into_owned();
        let lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
        let banner = lines
            .iter()
            .position(|line| line.starts_with("uf install"))
            .unwrap_or_else(|| panic!("`uf install` renders a banner, got:\n{text}"));
        let table = lines
            .iter()
            .position(|line| line.trim_start().starts_with("manager "))
            .unwrap_or_else(|| panic!("`uf install` names the manager it ran, got:\n{text}"));
        assert!(
            banner < table,
            "the banner comes before the key-values, got:\n{text}"
        );
        // The product card, the banner and its rule; then the key-values and
        // the verdict. What is skipped between them is npm's, and only npm's:
        // uf renders nothing while the manager owns the terminal, which is
        // what the blank lines around the call in `commands::pm::install` are
        // for.
        lines[..=banner + 1]
            .iter()
            .chain(&lines[table..])
            .map(|line| without_durations(line))
            .collect::<Vec<_>>()
    };
    assert_eq!(rendered(&short.stdout), rendered(&long.stdout));
    assert!(
        String::from_utf8_lossy(&short.stdout).contains("uf install"),
        "`uf i` should report itself as `uf install`"
    );
}

/// Replace every duration in `line` with a fixed marker.
///
/// The grammar is `uf_term::push_duration`'s: digits, optionally a decimal
/// part, then `ns`, `µs`, `ms` or `s`, and nothing alphanumeric after it. A
/// version, a byte count and a package name all fail that test, so only the
/// numbers that genuinely differ between two runs are blanked.
fn without_durations(line: &str) -> String {
    const UNITS: [&str; 4] = ["ns", "\u{b5}s", "ms", "s"];
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find(|ch: char| ch.is_ascii_digit()) {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest
            .find(|ch: char| !ch.is_ascii_digit() && ch != '.')
            .unwrap_or(rest.len());
        let (number, tail) = rest.split_at(end);
        let unit = UNITS
            .iter()
            .find(|unit| tail.starts_with(**unit))
            .filter(|unit| {
                tail[unit.len()..]
                    .chars()
                    .next()
                    .is_none_or(|ch| !ch.is_alphanumeric())
            });
        match unit {
            Some(unit) => {
                out.push_str("<duration>");
                rest = &tail[unit.len()..];
            }
            None => {
                out.push_str(number);
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The alias binaries are the longhand commands, and the help says so.
#[test]
fn alias_binaries_are_documented_as_the_commands_they_expand_to() {
    let output = uf().arg("--help").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    for (command, alias) in [("run", "ufr"), ("exec", "ufx")] {
        let line = stdout
            .lines()
            .find(|line| line.trim_start().starts_with(&format!("{command} ")))
            .unwrap_or_else(|| panic!("`{command}` is missing from the help"));
        assert!(
            line.contains(alias),
            "`uf {command}` should name its `{alias}` alias in the help, got {line:?}"
        );
    }
}

#[test]
fn doc_writes_api_markdown_from_exported_flow_jsdoc() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              app: { router: { enabled: false } },
            });
        "#,
    )
    .unwrap();
    fs::write(
        dir.path().join("src/api.js"),
        r#"
            // @flow

            /**
             * Reads a user.
             * @param id stable id
             */
            export function readUser(id: string): ?string {
              return id;
            }
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["doc", "--out", "generated"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let markdown = fs::read_to_string(dir.path().join("generated/api.md")).unwrap();
    assert!(markdown.contains("### readUser"), "{markdown}");
    assert!(markdown.contains("@param id stable id"), "{markdown}");
    assert!(
        markdown.contains("export function readUser(id: string): ?string { ... }"),
        "{markdown}"
    );
}

/// Completion output is consumed by a shell, so it must be nothing but the
/// script: no banner, no colour, no status line.
#[test]
fn a_completion_script_is_the_script_and_nothing_else() {
    for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
        let output = uf().args(["completion", shell]).output().unwrap();

        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.starts_with("# uf completion for"),
            "{shell}: {stdout:?}"
        );
        assert!(
            !stdout.contains('\u{1b}'),
            "{shell}: a completion script must carry no escape sequences"
        );
        assert!(
            stdout.contains("uf __complete"),
            "{shell}: the script must ask uf for candidates"
        );
    }
}

/// The reason completion is computed by the binary rather than generated: a
/// task added to `uf.config.js` is completable immediately.
#[test]
fn completion_offers_the_projects_own_task_names() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: {
                "smoke:test": { command: "true" },
                "smoke:build": { command: "true" },
                unrelated: { command: "true" },
              },
            });
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["__complete", "--", "run", "smoke"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut lines = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    lines.sort();

    assert_eq!(lines, vec!["smoke:build", "smoke:test"]);
}

/// Completion in a directory with no project must be silent, not an error: an
/// error here prints into the middle of somebody's command line.
#[test]
fn completion_outside_a_project_says_nothing_rather_than_failing() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["__complete", "--", "run", ""])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "{:?}", output.stdout);
}

/// A mistyped task name should name the task that was meant.
#[test]
fn an_unknown_task_suggests_the_one_that_was_meant() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: { build: { command: "true" }, check: { command: "true" } },
            });
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run", "biuld"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("did you mean"), "{stderr}");
    assert!(stderr.contains("build"), "{stderr}");
    assert!(
        stderr.contains("check"),
        "a short list of tasks should be named in full: {stderr}"
    );
}

/// `uf run` with no task name lists what the project defines.
#[test]
fn run_without_a_task_lists_them() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: { build: { command: "cargo build" } },
            });
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("run")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("build"), "{stdout}");
    assert!(stdout.contains("cargo build"), "{stdout}");
    assert!(stdout.contains("uf run <task>"), "{stdout}");
}

/// A repository is commonly more than one project, and `uf dev#docs` is how a
/// command says which one it means.
#[test]
fn a_command_runs_in_the_workspace_its_selector_names() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: { root: { command: "printf root" } },
            });
        "#,
    )
    .unwrap();
    let member = dir.path().join("site");
    fs::create_dir_all(&member).unwrap();
    fs::write(
        member.join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: { inner: { command: "printf inner" } },
            });
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run#site", "inner"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "inner");
}

/// The root is still the root when no selector is given.
#[test]
fn no_selector_leaves_the_command_where_it_was() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: { root: { command: "printf root" } },
            });
        "#,
    )
    .unwrap();
    let member = dir.path().join("site");
    fs::create_dir_all(&member).unwrap();
    fs::write(
        member.join("uf.config.js"),
        "export default {};
",
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run", "root"])
        .output()
        .unwrap();

    assert_eq!(String::from_utf8(output.stdout).unwrap(), "root");
}

#[test]
fn an_unknown_workspace_names_the_ones_that_exist() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "export default {};
",
    )
    .unwrap();
    let member = dir.path().join("docs");
    fs::create_dir_all(&member).unwrap();
    fs::write(
        member.join("uf.config.js"),
        "export default {};
",
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("inspect#dcos")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("did you mean"), "{stderr}");
    assert!(stderr.contains("docs"), "{stderr}");
}

/// A `#` in an argument is part of that argument. Only the subcommand carries
/// a selector, or a task named `build#2` would become a workspace lookup.
#[test]
fn a_hash_outside_the_subcommand_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: { "build#2": { command: "printf hashed" } },
            });
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run", "build#2"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "hashed");
}

/// Red line 3, as a test: a built-in provider a project can actually replace.
///
/// `NonFlowFormatter` had one variant and nothing read it — "the shape of
/// replaceability with none of the substance", in the architecture record's own
/// words. `uf explain fmt` naming whichever provider was selected, and the
/// exact command it will run, is the exit criterion that record set.
#[test]
fn the_non_flow_formatter_is_a_provider_a_project_can_replace() {
    let dir = tempfile::tempdir().unwrap();

    let selected = |formatter: &str| {
        fs::write(
            dir.path().join("uf.config.js"),
            format!(
                "export default defineConfig({{ fmt: {{ nonFlow: {{ formatter: \"{formatter}\" }} }} }});\n"
            ),
        )
        .unwrap();
        let output = uf()
            .arg("--cwd")
            .arg(dir.path())
            .args(["explain", "fmt"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };

    let biome = selected("biome");
    assert!(biome.contains("biome"), "{biome}");
    assert!(
        biome.contains("--indent-width"),
        "uf's settings must reach it: {biome}"
    );

    let prettier = selected("prettier");
    assert!(prettier.contains("prettier"), "{prettier}");
    assert!(prettier.contains("--tab-width"), "{prettier}");
    assert!(
        !prettier.contains("biome"),
        "selecting a provider must actually select it: {prettier}"
    );

    let none = selected("none");
    assert!(none.contains("left alone"), "{none}");
}

/// A project with no JSON must not need a formatter installed at all.
#[test]
fn formatting_a_project_with_no_non_flow_files_needs_no_formatter() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "export default {};
",
    )
    .unwrap();
    fs::write(
        dir.path().join("app.js"),
        "// @flow
export const a: number = 1;
",
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["fmt", "--check"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "a project with nothing for Biome to do must not need Biome: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Turning the provider off is how a project says "leave them alone", and it
/// must work without the binary being present.
#[test]
fn selecting_no_formatter_leaves_non_flow_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "export default defineConfig({ fmt: { nonFlow: { formatter: \"none\" } } });\n",
    )
    .unwrap();
    let ugly = "{\"a\":1,   \"b\":2}";
    fs::write(dir.path().join("data.json"), ugly).unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("fmt")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("data.json")).unwrap(),
        ugly,
        "`none` must leave the file exactly as it was"
    );
}

/// A project uf just scaffolded must not fail the first `uf fmt` a reader runs.
///
/// `fmt.nonFlow.formatter` defaults to Biome and `uf create` does not install
/// it, so uf's own default and uf's own scaffold disagreed on the first
/// command after `uf install`: `uf fmt` exited 1 over a JSON file nothing was
/// wrong with. A formatter uf chose and the project never named is a
/// suggestion — the run says what it could not look at, names the files, and
/// the exit code goes on answering for the files uf could format. See
/// ubugeeei-prod/uf#441.
#[test]
fn a_default_formatter_that_is_not_installed_warns_rather_than_failing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "export default defineConfig({});\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("app.js"),
        "// @flow\nexport const a: number = 1;\n",
    )
    .unwrap();
    fs::write(dir.path().join("package-lock.json"), "{\n  \"a\": 1\n}\n").unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["--color", "never", "fmt"])
        // Nothing is installed here, and nothing is on the way to being: the
        // formatter is missing because this directory does not exist.
        .env("PATH", dir.path().join("no-tools"))
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        output.status.success(),
        "the first `uf fmt` in a new project failed over uf's own default\n{stdout}{stderr}"
    );
    assert!(stdout.contains("biome is not installed"), "{stdout}");
    assert!(
        stdout.contains("1 non-Flow file was skipped"),
        "the count and its verb must agree:\n{stdout}"
    );
    assert!(
        stdout.contains("package-lock.json"),
        "a reader told a file was skipped must be told which:\n{stdout}"
    );
    assert!(
        stdout.contains("fmt.nonFlow.formatter"),
        "and how to make it stop:\n{stdout}"
    );
    // Once. It used to be printed as a warning and then again as the error
    // that ended the run.
    assert_eq!(
        stdout.matches("is not installed").count() + stderr.matches("is not installed").count(),
        1,
        "stdout:\n{stdout}stderr:\n{stderr}"
    );
}

/// A formatter the project asked for by name is a requirement, not a default.
///
/// The same missing binary, and the opposite answer, because the project wrote
/// `fmt.nonFlow.formatter` down itself. This is also the way a project makes
/// CI insist on the non-Flow half: name the formatter, and a run that could
/// not check those files fails.
#[test]
fn a_formatter_the_project_named_is_an_error_when_it_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "export default defineConfig({ fmt: { nonFlow: { formatter: \"biome\" } } });\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("app.js"),
        "// @flow\nexport const a: number = 1;\n",
    )
    .unwrap();
    fs::write(dir.path().join("package-lock.json"), "{\n  \"a\": 1\n}\n").unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["--color", "never", "fmt", "--check"])
        .env("PATH", dir.path().join("no-tools"))
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "a formatter the project asked for is a requirement\n{stdout}{stderr}"
    );
    assert!(
        stderr.contains("uf.config.js asks for biome"),
        "the error must say why this one is fatal:\n{stderr}"
    );
    assert!(stderr.contains("1 non-Flow file was skipped"), "{stderr}");
}

/// The warning must not become an excuse.
///
/// `uf fmt --check` still answers for the files uf can format, whether or not
/// somebody else's formatter is installed — which is the whole reason the exit
/// code was worth keeping honest.
#[test]
fn a_missing_default_formatter_does_not_excuse_an_unformatted_flow_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "export default defineConfig({});\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("app.js"),
        "// @flow\nexport const a: number =    1;\n",
    )
    .unwrap();
    fs::write(dir.path().join("package-lock.json"), "{\n  \"a\": 1\n}\n").unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["--color", "never", "fmt", "--check"])
        .env("PATH", dir.path().join("no-tools"))
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "an unformatted Flow file must still fail the check\n{stdout}{stderr}"
    );
    assert!(stderr.contains("1 file needs formatting"), "{stderr}");
    assert!(
        stdout.contains("biome is not installed"),
        "and the skip is still reported:\n{stdout}"
    );
}

#[test]
fn alias_binaries_print_the_root_version() {
    for name in ["uf", "ufr", "ufx"] {
        let output = binary(name).arg("--version").output().unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains(env!("CARGO_PKG_VERSION")),
            "{name} should expose the installed uf version, got {stdout:?}"
        );
        // The product, not the crate. clap names the command after the crate
        // unless it is told otherwise, and `uf --version` — the first thing
        // anyone runs after installing — answered `uf_cli 0.0.0-alpha.2`.
        assert!(
            stdout.starts_with("uf "),
            "{name} --version should name the command, got {stdout:?}"
        );
        assert!(!stdout.contains("uf_cli"), "{name}: {stdout:?}");
    }

    // And the aliases still say what they are in their usage line, which
    // comes from `argv[0]` rather than from the name.
    let usage = String::from_utf8(binary("ufr").arg("--help").output().unwrap().stdout).unwrap();
    assert!(usage.contains("Usage: ufr run"), "{usage}");
}

#[test]
fn ufr_keeps_version_flags_after_the_task_name_as_task_args() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: {
                show: { command: "printf %s \"$1\"" },
              },
            });
        "#,
    )
    .unwrap();

    let output = binary("ufr")
        .arg("--cwd")
        .arg(dir.path())
        .args(["show", "--", "--version"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // `--version` after the task name belongs to the task, not to `ufr`.
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "--version");
}

/// A task uf can run is run by uf, whatever the task runner engine says.
///
/// `uf.config.js` is where the task's meaning is written down, and Vite Task
/// has no way to read it — handing `ci` to `vp run ci` asked Vite+ for a
/// script it had never heard of, so every task defined here failed both on a
/// machine that had `vp` and on one that did not.
#[test]
fn a_task_with_a_command_is_run_by_uf_rather_than_handed_to_vite_task() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: {
                show: { command: "printf ours" },
              },
            });
        "#,
    )
    .unwrap();
    let runner = dir.path().join("vp");
    fs::write(
        &runner,
        "#!/bin/sh
printf vite-task
",
    )
    .unwrap();
    let mut permissions = fs::metadata(&runner).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&runner, permissions).unwrap();

    let output = binary("ufr")
        .arg("--cwd")
        .arg(dir.path())
        .arg("show")
        .env("UF_VITE_TASK_BIN", &runner)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "ours");
}

/// A task with no command of its own is Vite+'s, and is handed over.
#[test]
fn a_task_without_a_command_is_handed_to_vite_task() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: {
                show: { command: "" },
              },
            });
        "#,
    )
    .unwrap();
    let runner = dir.path().join("vp");
    fs::write(
        &runner,
        "#!/bin/sh
[ \"$1\" = run ] && [ \"$2\" = show ] && printf vite-task
",
    )
    .unwrap();
    let mut permissions = fs::metadata(&runner).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&runner, permissions).unwrap();

    let output = binary("ufr")
        .arg("--cwd")
        .arg(dir.path())
        .arg("show")
        .env("UF_VITE_TASK_BIN", &runner)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "vite-task");
}

#[test]
fn ufr_alias_runs_config_task() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              tasks: {
                hello: { command: "printf alias-ok" },
              },
            });
        "#,
    )
    .unwrap();
    let runner = dir.path().join("vp");
    fs::write(
        &runner,
        "#!/bin/sh\n[ \"$1\" = run ] && [ \"$2\" = hello ] && printf alias-ok\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&runner).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&runner, permissions).unwrap();

    let output = binary("ufr")
        .arg("--cwd")
        .arg(dir.path())
        .arg("hello")
        .env("UF_VITE_TASK_BIN", &runner)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "alias-ok",
        "a task owns stdout; uf must not render onto it"
    );
}

#[test]
fn ufx_alias_runs_uniflowed_create_package() {
    let dir = tempfile::tempdir().unwrap();

    let output = binary("ufx")
        .arg("--cwd")
        .arg(dir.path())
        .args(["@uniflowed/create", "app"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ufx \u{b7} @uniflowed/create"), "{stdout}");
    assert!(stdout.contains("created 9 files"));
    assert!(dir.path().join("app.js").exists());
    // `.uf/exec-cache/` used to be written here, and by every other `ufx`
    // invocation. Nothing ever read one back: the directory was named a cache
    // and cached nothing, and it existed so that a command with nothing to do
    // had something to write. See ubugeeei-prod/uf#274.
    assert!(!dir.path().join(".uf/exec-cache").exists());
}

/// A binary the project has installed runs, with its arguments and its status.
///
/// `uf exec` used to write a JSON file, print "cached execution request for
/// registry resolution", and exit 0 without running anything at all — so a CI
/// step spelled `ufx some-codegen` went green having generated nothing.
/// See ubugeeei-prod/uf#274.
#[test]
fn exec_runs_an_installed_binary_and_forwards_its_arguments_and_status() {
    let project = Project::new(&[]);
    let bin = project.path().join("node_modules/.bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("uf-fixture-tool");
    fs::write(
        &script,
        "#!/bin/sh\necho \"tool saw: $*\"\nexit \"${UF_FIXTURE_EXIT:-0}\"\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["exec", "uf-fixture-tool", "--flag", "value"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout, "tool saw: --flag value\n",
        "the binary owns stdout; uf must not render onto it"
    );

    // And its failure is uf's failure, with the child's own number on it.
    //
    // `42` rather than `1`, because `1` is what a wrapper that loses the
    // status also produces and the assertion would prove nothing. `uf exec`
    // used to `bail!`, which `main` turns into `ExitCode::FAILURE` — so
    // `jest`'s codes, `eslint`'s, and a codegen script's all arrived as the
    // same `1` and a script branching on one could tell nothing apart, while
    // `docs/app/reference/cli` said "exiting with its status".
    let failed = uf()
        .arg("--cwd")
        .arg(project.path())
        .env("UF_FIXTURE_EXIT", "42")
        .args(["exec", "uf-fixture-tool"])
        .output()
        .unwrap();
    assert_eq!(
        failed.status.code(),
        Some(42),
        "the child's exit status is uf's:\n{}",
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("uf-fixture-tool exited with"),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );
}

/// A path is the third way to spell what `uf exec` runs, and adopts a status too.
///
/// `ufx ./scripts/codegen.js` is a thing people do and it is not a package
/// name: `exec_package` tries it with `spawn_executable` between the
/// `node_modules/.bin` lookup and the package manager. It is the path
/// `uf explain exec` did not describe, and the one whose exit status a CI step
/// is most likely to be branching on — a codegen script that exits `42` to mean
/// "nothing to do" is exactly the shape.
#[test]
fn exec_runs_an_explicit_path_and_adopts_its_exit_status() {
    let project = Project::new(&[]);
    let script = project.path().join("scripts/codegen.js");
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    fs::write(
        &script,
        "#!/bin/sh\necho \"codegen saw: $*\"\nexit \"${UF_FIXTURE_EXIT:-0}\"\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["exec", "./scripts/codegen.js", "--write"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "codegen saw: --write\n",
        "the script owns stdout; uf must not render onto it"
    );

    let failed = uf()
        .arg("--cwd")
        .arg(project.path())
        .env("UF_FIXTURE_EXIT", "42")
        .args(["exec", "./scripts/codegen.js"])
        .output()
        .unwrap();
    assert_eq!(
        failed.status.code(),
        Some(42),
        "the script's exit status is uf's:\n{}",
        String::from_utf8_lossy(&failed.stderr)
    );
}

/// A package the project never installed is refused, loudly.
///
/// Fetching an unpinned name from a registry and executing its binary is the
/// most dangerous thing a package manager does, and uf already refuses to run
/// a dependency's install scripts without being asked. `--yes` is how you ask;
/// without it this is an error and not a shrug. See ubugeeei-prod/uf#274.
#[test]
fn exec_refuses_to_fetch_a_package_the_project_has_not_installed() {
    let project = Project::new(&[]);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["exec", "uf-nonexistent-fixture-package", "hello"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "stdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    for expected in [
        "uf-nonexistent-fixture-package is not installed",
        "uf.lock",
        "uf exec --yes uf-nonexistent-fixture-package",
        // Naming the command it would run, with the arguments forwarded, so
        // the reader can decide by reading rather than by trusting.
        "uf-nonexistent-fixture-package hello",
    ] {
        assert!(
            stderr.contains(expected),
            "missing {expected:?} in:\n{stderr}"
        );
    }
    assert!(
        !stderr.contains("cached execution request"),
        "the old shrug is still there:\n{stderr}"
    );
}

#[test]
fn creates_react_app_from_cli() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");

    let stdout = create_app(&app);

    assert!(app.join("app.js").exists());
    assert!(app.join("uf.config.js").exists());
    assert!(app.join("app/_uf.page.js").exists());
    assert!(app.join("app/Counter.js").exists());
    assert!(app.join("app/useCounter.js").exists());

    let package = fs::read_to_string(app.join("package.json")).unwrap();
    assert!(!package.contains(r#""scripts""#));

    // The generated files are shown as a tree, not as a flat count.
    assert!(stdout.contains("uf create"));
    assert!(stdout.contains("├─ app"));
    assert!(stdout.contains("└─ uf.config.js"));
    assert!(stdout.contains("next steps"));
    assert!(stdout.contains("1. cd app"));
    assert!(stdout.contains("2. uf install"));
    assert!(stdout.contains("3. uf dev"));
    // Eight source files and the `.gitignore` that keeps uf's output
    // out of the first commit.
    assert!(stdout.contains("✓ created 9 files"));
}

/// The one-word form, which is what the site tells a reader to type.
///
/// `uf create app` takes `[TEMPLATE] [PATH]`, and a single argument was read
/// as the template, so `uf create app my-site` — printed on the home page and
/// in the CLI reference — answered `invalid value 'my-site' for '[TEMPLATE]'`.
/// `ufx @uniflowed/create app my-site` already accepted it, so the two front
/// doors read the same command differently. See ubugeeei-prod/uf#322.
#[test]
fn a_single_argument_that_is_not_a_template_is_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("my-site");

    let output = uf().args(["create", "app"]).arg(&app).output().unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(app.join("uf.config.js").exists(), "it scaffolded nothing");
    assert!(app.join("app/_uf.page.js").exists());
}

/// And a single argument that *is* a template still is one.
///
/// `uf create app react` scaffolds into the current directory, which is the
/// behaviour the two-positional grammar already had and the reason a lone
/// argument cannot simply be the path. A directory that wants to be called
/// `react` is written out in full.
#[test]
fn a_single_argument_that_is_a_template_is_the_template() {
    let dir = tempfile::tempdir().unwrap();
    let inside = dir.path().join("here");
    fs::create_dir_all(&inside).unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(&inside)
        .args(["create", "app", "react"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    // Into `here`, not into `here/react`.
    assert!(inside.join("uf.config.js").exists());
    assert!(!inside.join("react").exists());
}

/// Two arguments keep their old meaning, and a typo in the first is refused.
///
/// Reading `uf create app raect my-site` as a directory called `raect` with a
/// second argument nobody looked at would scaffold silently into the wrong
/// place, which is worse than the error it replaced.
#[test]
fn with_two_arguments_the_first_one_must_be_a_template() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .args(["create", "app", "raect"])
        .arg(dir.path().join("my-site"))
        .output()
        .unwrap();

    assert!(!output.status.success(), "it accepted a template typo");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("`raect` is not a template"), "{stderr}");
    assert!(stderr.contains("templates: react"), "{stderr}");
    // And it says what the reader probably meant to type.
    assert!(stderr.contains("uf create app raect"), "{stderr}");
    assert!(
        !dir.path().join("raect").exists() && !dir.path().join("my-site").exists(),
        "it scaffolded something anyway"
    );
}

/// `uf explain` says which provider runs each stage.
///
/// An integrated toolchain that cannot say what it is doing is a black box,
/// and a black box is where an integration's problems stop being annoying and
/// become unfixable. See `docs/red-lines.md`, line 7.
#[test]
fn explain_names_the_provider_for_every_stage() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nexport default defineConfig({});\n",
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["explain", "dev"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // The providers, named — that uf drives Vite rather than being it is the
    // thing this command exists to make visible.
    assert!(stdout.contains("vite"), "{stdout}");
    assert!(stdout.contains("uf transform"), "{stdout}");
    assert!(stdout.contains("@uniflowed/router"), "{stdout}");
    // And where the answers came from.
    assert!(stdout.contains("uf.config.js"), "{stdout}");
    // Including the values: which `.env` files this command reads, in which
    // mode, and which of them reach the browser. "Where did this value come
    // from" is exactly the question this command exists for.
    assert!(stdout.contains("environment"), "{stdout}");
    assert!(stdout.contains(".env.development"), "{stdout}");
    assert!(stdout.contains("VITE_"), "{stdout}");
}

#[test]
fn explain_says_which_commands_it_knows() {
    let dir = tempfile::tempdir().unwrap();
    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["explain", "deploy"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("does not describe"), "{stderr}");
    // Listing them beats making the reader guess.
    assert!(
        stderr.contains("dev, build, preview, start, doc, test, fmt, lint, check"),
        "{stderr}"
    );
    assert!(
        stderr.contains("install, add, remove, update, why, upgrade"),
        "{stderr}"
    );
}

/// Commands that do their whole job in this binary, so there is no provider
/// to name.
///
/// The other half of {@link explain_describes_every_command_that_delegates}:
/// the test asks `uf` itself for its commands, so a new one has to land in
/// one list or the other. `help` and `completion` are clap's; `create`,
/// `explain`, `info` and `inspect` are uf's own work start to finish.
///
/// `exec` left this list when it started running things: three of its four
/// paths hand control to something else, so there is a provider to name.
const SELF_CONTAINED: &[&str] = &["completion", "create", "explain", "help", "info", "inspect"];

/// Every command `uf` has is either explained or classified.
///
/// `uf explain` described seven of twenty-two, and the fifteen it did not
/// were the ones where the question has an answer worth printing — which
/// package manager resolves a tree, which registry a publish reaches, which
/// runner schedules a task. See ubugeeei-prod/uf#166.
///
/// The command list comes from `uf __complete`, which is what the shell
/// completions ask, rather than from a literal here: a list written twice is
/// a list that disagrees with itself, and the failure mode is silent — a new
/// delegating command would be missing from `uf explain` and this test would
/// go on passing. Now it fails until the command is explained or named in
/// {@link SELF_CONTAINED}.
#[test]
fn explain_describes_every_command_that_delegates() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nexport default defineConfig({});\n",
    )
    .unwrap();

    let listed = uf().args(["__complete", ""]).output().unwrap();
    assert!(listed.status.success());
    let listed = String::from_utf8(listed.stdout).unwrap();
    let commands: Vec<&str> = listed
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        // `i` is `install` under another name, and explaining it twice would
        // say the same thing twice.
        .filter(|name| *name != "i")
        .collect();
    assert!(
        commands.len() > 15,
        "`uf __complete` listed {} commands, which is not the command set:\n{listed}",
        commands.len()
    );

    for command in commands {
        if SELF_CONTAINED.contains(&command) {
            continue;
        }
        let output = uf()
            .arg("--cwd")
            .arg(dir.path())
            .args(["explain", command])
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "uf explain {command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("provider"),
            "uf explain {command}:\n{stdout}"
        );
        // A provider name is something a reader can recognise. `{:?}` on a
        // config enum gives `UfNative`, and lowercasing it gives `ufnative`,
        // which is a word nobody wrote and nobody can search for.
        assert!(
            !stdout.contains("ufnative") && !stdout.contains("vitetask"),
            "uf explain {command} printed a debug name:\n{stdout}"
        );
        // And a detail is a sentence, not a struct.
        assert!(
            !stdout.contains("Config {"),
            "uf explain {command} printed a struct:\n{stdout}"
        );
    }
}

#[test]
fn explain_emits_json_when_asked() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nexport default defineConfig({});\n",
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["explain", "build", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("uf explain --json emits JSON");
    assert_eq!(value["command"], "uf build");
    assert!(
        value["stages"].as_array().is_some_and(|s| !s.is_empty()),
        "{value}"
    );
    assert!(
        value["configurationSources"].as_array().is_some(),
        "{value}"
    );
}

/// `uf explain exec` describes the path that runs without asking.
///
/// `exec_package` tries four things in order, and the explanation listed three:
/// uf's own packages, `node_modules/.bin`, and the package manager. The one it
/// skipped runs an explicit path — `ufx ./scripts/codegen.js` — and it sits
/// *before* the package-manager branch, so a reader who came here to find out
/// what would happen was told their path would be fetched from a registry and
/// refused without `--yes`, when in fact it executes with no consent asked for
/// at all.
///
/// Being wrong about which of two paths asks permission is the one thing this
/// command must not be, which is why the order is asserted and not only the
/// presence.
#[test]
fn explain_exec_describes_every_path_exec_actually_takes() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["explain", "exec", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("uf explain --json emits JSON");
    let stages = value["stages"].as_array().expect("stages is an array");
    let names: Vec<&str> = stages
        .iter()
        .map(|stage| stage["name"].as_str().unwrap())
        .collect();

    // One per branch of `exec_package`, in `exec_package`'s order.
    assert_eq!(
        names,
        vec![
            "uf's own packages",
            "installed binaries",
            "an explicit path",
            "everything else",
        ],
        "{value}"
    );
    // And the reader is told which of the two asks first.
    let explicit = stages[2]["detail"].as_str().unwrap();
    assert!(
        explicit.contains("--yes"),
        "the explicit-path stage must say whether it asks: {explicit}"
    );
}

#[test]
fn creating_a_library_suggests_running_its_tests() {
    let dir = tempfile::tempdir().unwrap();
    let lib = dir.path().join("kit");

    let output = uf().args(["create", "lib"]).arg(&lib).output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("3. uf test"));
}

/// A scaffolded project passes uf's own linter.
///
/// It did not. `uf create app react` wrote `export default component Page()`
/// and `export default component Counter()`, and uf's own
/// `react/no-default-export-component` warned about both — "framework routes
/// are wired by name; export components with a named export" — on the very
/// first command a new project runs. The layout in the same template already
/// used a named export, and `@uniflowed/router` documents the named `Page` as
/// what `uf create` scaffolds, so the two files were the odd ones out.
///
/// A starter that trips the toolchain's own rules teaches the rules are
/// noise.
#[test]
fn a_scaffolded_project_lints_clean() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf().arg("--cwd").arg(&app).arg("lint").output().unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "uf lint on a new project:\n{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("no problems"), "{stdout}");
    assert!(stdout.contains("warnings       0"), "{stdout}");
    assert!(stdout.contains("errors         0"), "{stdout}");
}

#[test]
fn creating_over_an_existing_project_reports_the_conflict_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf()
        .args(["create", "app", "react"])
        .arg(&app)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("--force"));
}

/// A project that has not installed `@uniflowed/vite` is told what to do,
/// after uf's own phases have run.
#[test]
fn build_without_the_vite_package_names_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf().arg("--cwd").arg(&app).arg("build").output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("@uniflowed/vite"), "{stderr}");
    assert!(stderr.contains("uf install"), "{stderr}");
    // The route types are still generated: they do not need a build.
    assert!(app.join("router.js").exists());
    assert!(
        fs::read_to_string(app.join("router.js"))
            .unwrap()
            .contains("export type RoutePath")
    );
}

/// Exposing the dev server needs an allowlist, and the refusal comes before
/// anything is started. See `docs/security.md`.
#[test]
fn dev_host_without_an_allowed_hosts_list_refuses_to_start() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("uf.config.js"),
        r#"
            export default defineConfig({
              dev: { port: 0 },
            });
        "#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["dev", "--host", "0.0.0.0"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("allowedHosts"), "{stderr}");
}

#[test]
fn dev_without_the_vite_package_names_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("uf.config.js"), "export default {};\n").unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("dev")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("@uniflowed/vite"), "{stderr}");
}

/// One framed message, the way an editor sends it.
fn framed(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

/// Run `uf lsp` over one stream of framed messages and parse what came back.
///
/// Reading the answers as JSON rather than as substrings is what lets a test
/// take an edit out of a code action and *apply* it, which is the only way to
/// find out whether the edit was any good.
fn lsp_session(messages: &[String]) -> Vec<serde_json::Value> {
    let output = uf()
        .arg("lsp")
        .write_stdin(messages.concat())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    frames(&String::from_utf8(output.stdout).unwrap())
}

/// Every framed message in a server's output.
fn frames(stdout: &str) -> Vec<serde_json::Value> {
    let mut parsed = Vec::new();
    let mut rest = stdout;
    while let Some(at) = rest.find("Content-Length: ") {
        let after = &rest[at + "Content-Length: ".len()..];
        let (length, body) = after
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("an unterminated frame in:\n{stdout}"));
        let length: usize = length.trim().parse().expect("a Content-Length");
        parsed.push(
            serde_json::from_str(&body[..length])
                .unwrap_or_else(|error| panic!("{error} in:\n{}", &body[..length])),
        );
        rest = &body[length..];
    }
    parsed
}

/// A session run from a directory other than the project, with `--cwd`.
fn lsp_session_in(root: &std::path::Path, messages: &[String]) -> Vec<serde_json::Value> {
    let output = uf()
        .arg("lsp")
        .arg("--cwd")
        .arg(root)
        .current_dir(std::env::temp_dir())
        .write_stdin(messages.concat())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    frames(&String::from_utf8(output.stdout).unwrap())
}

/// The answer to one request id.
fn answer(messages: &[serde_json::Value], id: u64) -> &serde_json::Value {
    messages
        .iter()
        .find(|message| message["id"] == serde_json::json!(id))
        .unwrap_or_else(|| panic!("no answer for id {id} in:\n{messages:#?}"))
}

/// Every `publishDiagnostics` the server sent, in order.
fn published(messages: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    messages
        .iter()
        .filter(|message| message["method"] == "textDocument/publishDiagnostics")
        .map(|message| &message["params"]["diagnostics"])
        .collect()
}

/// Apply LSP `TextEdit`s to `source`, the way an editor applies them.
///
/// Edits name positions in the document they were computed against, so they
/// are applied back to front and the offsets of the earlier ones stay true.
/// Characters are UTF-16 code units, which is the half of this an editor gets
/// right and a test helper written in a hurry does not.
fn apply(source: &str, edits: &[serde_json::Value]) -> String {
    let mut lines: Vec<String> = source.split('\n').map(str::to_owned).collect();
    let mut edits: Vec<&serde_json::Value> = edits.iter().collect();
    edits.sort_by_key(|edit| {
        std::cmp::Reverse((
            edit["range"]["start"]["line"].as_u64().unwrap(),
            edit["range"]["start"]["character"].as_u64().unwrap(),
        ))
    });

    for edit in edits {
        let start_line = edit["range"]["start"]["line"].as_u64().unwrap() as usize;
        let end_line = (edit["range"]["end"]["line"].as_u64().unwrap() as usize).min(lines.len());
        let start = utf16_to_byte(
            &lines[start_line],
            edit["range"]["start"]["character"].as_u64().unwrap() as usize,
        );
        let text = edit["newText"].as_str().unwrap();

        if end_line >= lines.len() {
            // A whole-document edit, whose end is past the last line.
            let mut head = lines[start_line][..start].to_owned();
            head.push_str(text);
            lines.truncate(start_line);
            lines.push(head);
            continue;
        }
        let end = utf16_to_byte(
            &lines[end_line],
            edit["range"]["end"]["character"].as_u64().unwrap() as usize,
        );
        let replacement = format!(
            "{}{text}{}",
            &lines[start_line][..start],
            &lines[end_line][end..]
        );
        lines.splice(start_line..=end_line, [replacement]);
    }
    lines.join("\n")
}

fn utf16_to_byte(line: &str, character: usize) -> usize {
    let mut units = 0usize;
    for (offset, letter) in line.char_indices() {
        if units >= character {
            return offset;
        }
        units += letter.len_utf16();
    }
    line.len()
}

#[test]
fn lsp_initialize_returns_native_capabilities() {
    let output = uf()
        .arg("lsp")
        .write_stdin(framed(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        ))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("Content-Length: "));
    assert!(stdout.contains(r#""name":"uf-lsp""#));
    assert!(stdout.contains(r#""documentFormattingProvider":true"#));
    assert!(stdout.contains(r#""hoverProvider":true"#));
    assert!(
        stdout.contains(r#""codeActionKinds":["quickfix","source.fixAll.uf"]"#),
        "{stdout}"
    );
    // `diagnosticProvider` is the *pull* model, where the editor asks. uf
    // pushes `textDocument/publishDiagnostics` instead, which is a
    // notification and has no capability to advertise. Advertising a pull
    // provider that nothing serves is what ubugeeei-prod/uf#162 was.
    assert!(!stdout.contains("diagnosticProvider"), "{stdout}");
    // Nor `source.organizeImports`, for the same reason: uf has no opinion
    // about import order anywhere in the workspace, so there is nothing to
    // organise them into. A capability is a promise, and this one would be
    // a promise to sort imports into an order uf has never decided on.
    assert!(!stdout.contains("organizeImports"), "{stdout}");
    assert_plain(&stdout);
}

/// Several messages on one open pipe, which is what an editor does.
///
/// The previous test was the whole of the coverage and it passed against a
/// server that read stdin to end of input, answered once and returned — so it
/// answered nothing at all to a client that keeps the pipe open, which is
/// every client. Two answers on one connection is the difference.
#[test]
fn lsp_answers_more_than_one_message_on_one_connection() {
    let input = [
        framed(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        framed(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#),
        framed(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]
    .concat();

    let output = uf().arg("lsp").write_stdin(input).output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.matches("Content-Length: ").count(),
        2,
        "one answer per request, and no answer to a notification:\n{stdout}"
    );
    assert!(stdout.contains(r#""id":1"#), "{stdout}");
    assert!(stdout.contains(r#""id":2"#), "{stdout}");
}

/// `--cwd` names the project, and the server reads that project's config.
///
/// An editor starts one server per workspace folder and says which folder it
/// is. `--cwd` was accepted, resolved and then dropped, so the server read `.`
/// — whatever directory the editor happened to be launched from — and answered
/// with uf's defaults while looking like it had read the project.
#[test]
fn lsp_reads_the_config_of_the_directory_cwd_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("uf.config.js"),
        "// @flow\nexport default { fmt: { indentWidth: 8 } };\n",
    )
    .unwrap();

    let messages = lsp_session_in(
        dir.path(),
        &[
            framed(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
            did_open("file:///a.js", "// @flow\nfunction f() {\nreturn 1;\n}\n"),
            framed(
                r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{"textDocument":{"uri":"file:///a.js"},"options":{}}}"#,
            ),
            framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
        ],
    );

    let edits = answer(&messages, 2)["result"].as_array().unwrap();
    let formatted = edits[0]["newText"].as_str().unwrap();
    assert!(
        formatted.contains("\n        return 1;"),
        "eight spaces, as the project asked for: {formatted:?}"
    );
}

/// A document opened and then formatted comes back as `uf fmt` would write it.
#[test]
fn lsp_formats_an_open_document() {
    let input = [
        framed(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        framed(
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///a.js","languageId":"javascript","version":1,"text":"// @flow\nconst   x=1\n"}}}"#,
        ),
        framed(
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{"textDocument":{"uri":"file:///a.js"},"options":{}}}"#,
        ),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]
    .concat();

    let output = uf().arg("lsp").write_stdin(input).output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(r#"const x = 1;"#),
        "the edit should carry the formatted document:\n{stdout}"
    );
    assert!(stdout.contains(r#""newText""#), "{stdout}");
}

/// Opening a document publishes what is wrong with it.
///
/// The same `uf_lint::lint_source` `uf lint` calls, so a marker in the editor
/// and a line in the terminal are the same diagnostic — including
/// `flow/syntax`, the parser's own errors, which is what an editor most wants
/// while a file is still being typed.
#[test]
fn lsp_publishes_diagnostics_when_a_document_opens() {
    let input = [
        framed(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        framed(
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///a.js","languageId":"javascript","version":1,"text":"// @flow\nconst x = ;\n"}}}"#,
        ),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]
    .concat();

    let output = uf().arg("lsp").write_stdin(input).output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(r#""method":"textDocument/publishDiagnostics""#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""code":"flow/syntax""#), "{stdout}");
    // A syntax error is an error, not a warning.
    assert!(stdout.contains(r#""severity":1"#), "{stdout}");
    assert!(stdout.contains(r#""source":"uf""#), "{stdout}");
    // Zero-based: the second line of the file.
    assert!(stdout.contains(r#""line":1"#), "{stdout}");
    // A notification carries no id to answer.
    assert!(!stdout.contains(r#""id":null"#), "{stdout}");
}

/// A change republishes, and closing clears.
///
/// An editor keeps whatever it was last told, so a file that is fixed and one
/// that is closed both have to be said out loud.
#[test]
fn lsp_republishes_on_change_and_clears_on_close() {
    let input = [
        framed(
            r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///a.js","text":"// @flow\nconst x = ;\n"}}}"#,
        ),
        framed(
            r#"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///a.js","version":2},"contentChanges":[{"text":"// @flow\nconst x = 1;\n"}]}}"#,
        ),
        framed(
            r#"{"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":"file:///a.js"}}}"#,
        ),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]
    .concat();

    let output = uf().arg("lsp").write_stdin(input).output().unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let published = stdout
        .matches(r#""method":"textDocument/publishDiagnostics""#)
        .count();
    assert_eq!(
        published, 3,
        "open, change and close each publish:\n{stdout}"
    );
    assert_eq!(
        stdout.matches(r#""diagnostics":[]"#).count(),
        2,
        "the fixed document and the closed one are both empty:\n{stdout}"
    );
}

/// A request uf does not serve is answered, not ignored.
///
/// An editor waiting on an id that never comes back is a hang, which is the
/// failure this whole command had. The answer is the one the specification
/// names — `MethodNotFound` — rather than a `null` result, because a null
/// result says "there are no document symbols here" and the truth is "uf does
/// not do document symbols".
#[test]
fn lsp_answers_a_request_it_does_not_serve() {
    let messages = lsp_session(&[
        framed(r#"{"jsonrpc":"2.0","id":7,"method":"textDocument/documentSymbol","params":{}}"#),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    assert_eq!(answer(&messages, 7)["error"]["code"], -32601);
    assert!(
        answer(&messages, 7)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("textDocument/documentSymbol"),
        "{messages:#?}"
    );
}

/// A notification uf does not serve gets nothing back at all.
///
/// The rule cuts both ways: a request without an answer hangs the editor, and
/// an answer to a notification is a response with no request, which some
/// clients treat as a protocol violation and close the connection over.
#[test]
fn lsp_says_nothing_to_a_notification_it_does_not_serve() {
    let messages = lsp_session(&[
        framed(r#"{"jsonrpc":"2.0","method":"$/setTrace","params":{"value":"verbose"}}"#),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    assert!(messages.is_empty(), "{messages:#?}");
}

/// A body that is not JSON is answered and the session carries on.
///
/// This is the difference between a server an editor can wedge and one it
/// cannot: the frame header said how many bytes the broken message was, so
/// the stream is still in sync and the next request is served normally.
#[test]
fn lsp_answers_a_malformed_message_and_keeps_serving() {
    let messages = lsp_session(&[
        framed("{ this is not json"),
        framed(r#"{"jsonrpc":"2.0","id":2}"#),
        framed(r#"{"jsonrpc":"2.0","id":3,"method":"initialize","params":{}}"#),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    // Parse error, with a null id because the message was too broken to have one.
    assert_eq!(messages[0]["error"]["code"], -32700);
    assert_eq!(messages[0]["id"], serde_json::Value::Null);
    // JSON, but not a request: no method.
    assert_eq!(answer(&messages, 2)["error"]["code"], -32600);
    // And the connection still works.
    assert_eq!(
        answer(&messages, 3)["result"]["serverInfo"]["name"],
        "uf-lsp"
    );
}

/// A request whose params name no document is a request that cannot be served.
#[test]
fn lsp_refuses_a_request_with_unusable_params() {
    let messages = lsp_session(&[
        framed(
            r#"{"jsonrpc":"2.0","id":1,"method":"textDocument/codeAction","params":{"textDocument":{"uri":"file:///a.js"}}}"#,
        ),
        framed(
            r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///a.js"}}}"#,
        ),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    assert_eq!(answer(&messages, 1)["error"]["code"], -32602);
    assert_eq!(answer(&messages, 2)["error"]["code"], -32602);
}

/// The document an editor opens, for the code action and hover tests.
fn did_open(uri: &str, text: &str) -> String {
    framed(&format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","languageId":"javascript","version":1,"text":{text}}}}}}}"#,
        text = serde_json::Value::String(text.to_owned())
    ))
}

fn code_action(id: u64, uri: &str, line: u64, character: u64, only: Option<&str>) -> String {
    let only = only.map_or_else(String::new, |kind| format!(r#","only":["{kind}"]"#));
    framed(&format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"textDocument/codeAction","params":{{"textDocument":{{"uri":"{uri}"}},"range":{{"start":{{"line":{line},"character":{character}}},"end":{{"line":{line},"character":{character}}}}},"context":{{"diagnostics":[]{only}}}}}}}"#
    ))
}

fn hover_at(id: u64, uri: &str, line: u64, character: u64) -> String {
    framed(&format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"textDocument/hover","params":{{"textDocument":{{"uri":"{uri}"}},"position":{{"line":{line},"character":{character}}}}}}}"#
    ))
}

/// The whole point of a code action: an edit that, applied, fixes the thing.
///
/// Two sessions, because that is what an editor does — it asks, it applies
/// what it was given, and the server sees the result as the next change. The
/// second session opens the *edited* text, so the "the diagnostic is gone"
/// half is the linter's own answer about the document the edit produced,
/// rather than a test asserting that a string was replaced.
#[test]
fn lsp_offers_a_quick_fix_whose_edit_lints_clean() {
    let source = "// @flow\ntype B = bool;\n";
    let messages = lsp_session(&[
        did_open("file:///a.js", source),
        code_action(2, "file:///a.js", 1, 9, None),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    // The document opened with exactly one problem.
    assert_eq!(published(&messages)[0].as_array().unwrap().len(), 1);

    let actions = answer(&messages, 2)["result"].as_array().unwrap();
    let quick_fix = actions
        .iter()
        .find(|action| action["kind"] == "quickfix")
        .unwrap_or_else(|| panic!("no quick fix in {actions:#?}"));
    assert_eq!(quick_fix["title"], "Replace `bool` with `boolean`");
    // The action carries the diagnostic it answers, so the editor can attach
    // it to the right squiggle.
    assert_eq!(quick_fix["diagnostics"][0]["code"], "flow/deprecated-type");

    let edits = quick_fix["edit"]["changes"]["file:///a.js"]
        .as_array()
        .unwrap();
    let fixed = apply(source, edits);
    assert_eq!(fixed, "// @flow\ntype B = boolean;\n");

    // And the linter agrees, because it is the one asked.
    let after = lsp_session(&[
        did_open("file:///a.js", &fixed),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);
    assert_eq!(
        published(&after)[0].as_array().unwrap().len(),
        0,
        "the edit should leave nothing to report: {:#?}",
        published(&after)[0]
    );
}

/// `source.fixAll` fixes every occurrence, not just the one under the cursor.
#[test]
fn lsp_fixes_every_occurrence_in_the_file_at_once() {
    let source = "// @flow\ntype A = bool;\ntype B = ?bool;\n";
    let messages = lsp_session(&[
        did_open("file:///a.js", source),
        code_action(2, "file:///a.js", 1, 9, Some("source.fixAll")),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    let actions = answer(&messages, 2)["result"].as_array().unwrap();
    // Filtered to `source.fixAll`, so the quick fix for the cursor is not here.
    assert_eq!(actions.len(), 1, "{actions:#?}");
    assert_eq!(actions[0]["kind"], "source.fixAll.uf");

    let edits = actions[0]["edit"]["changes"]["file:///a.js"]
        .as_array()
        .unwrap();
    assert_eq!(edits.len(), 2);
    assert_eq!(
        apply(source, edits),
        "// @flow\ntype A = boolean;\ntype B = ?boolean;\n"
    );
}

/// An unsafe fix is offered to a person and withheld from `codeActionsOnSave`.
///
/// The two halves of the same decision. Clicking a lightbulb is somebody
/// asking for the edit, so the quick fix is there — but not marked preferred,
/// which is what "apply the obvious fix" binds to. `source.fixAll` is the
/// unattended path an editor runs on every save, and an edit that can change
/// what the program does must not arrive with a keystroke.
#[test]
fn an_unsafe_fix_is_a_quick_fix_but_never_part_of_fix_all() {
    let source = "// @flow\ntype A = bool;\nexport let count: A = true;\n";
    let messages = lsp_session(&[
        did_open("file:///a.js", source),
        code_action(2, "file:///a.js", 2, 7, None),
        code_action(3, "file:///a.js", 1, 9, Some("source.fixAll")),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    let actions = answer(&messages, 2)["result"].as_array().unwrap();
    let quick_fix = actions
        .iter()
        .find(|action| action["title"] == "Replace `let` with `const`")
        .unwrap_or_else(|| panic!("no quick fix in {actions:#?}"));
    assert_eq!(
        quick_fix["isPreferred"], false,
        "an unsafe fix must not be the preferred one: {quick_fix:#?}"
    );

    let fix_all = answer(&messages, 3)["result"].as_array().unwrap();
    let edits = fix_all[0]["edit"]["changes"]["file:///a.js"]
        .as_array()
        .unwrap();
    assert_eq!(
        apply(source, edits),
        "// @flow\ntype A = boolean;\nexport let count: A = true;\n",
        "fix-all wrote the unsafe fix"
    );
}

/// A rule whose right answer depends on what the author meant gets no action.
///
/// `flow/unclear-type` could "fix" `any` to `mixed`, to an opaque type, or to
/// a generated router type. A code action that picked one would be a guess an
/// editor applies without asking.
#[test]
fn lsp_offers_nothing_for_a_rule_it_cannot_answer() {
    let messages = lsp_session(&[
        did_open("file:///a.js", "// @flow\ntype A = any;\n"),
        code_action(2, "file:///a.js", 1, 9, None),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    // The diagnostic is reported...
    assert_eq!(published(&messages)[0][0]["code"], "flow/unclear-type");
    // ...and no action is offered for it.
    assert_eq!(
        answer(&messages, 2)["result"].as_array().unwrap().len(),
        0,
        "{:#?}",
        answer(&messages, 2)
    );
}

/// Whitespace hygiene is the formatter's answer, and it is offered only where
/// the formatter really gives it.
///
/// The second half is the interesting one. `uf fmt` reprints from the syntax
/// tree and so preserves the inside of a template literal, while
/// `uniflowed/no-trailing-whitespace` measures the raw line and reports
/// trailing spaces inside one — so there the formatter is *not* the fix, and
/// offering it would be offering something that does not work.
#[test]
fn lsp_offers_the_formatter_only_where_it_clears_the_diagnostic() {
    let source = "// @flow\nconst a = 1;   \n";
    let messages = lsp_session(&[
        did_open("file:///a.js", source),
        code_action(2, "file:///a.js", 1, 12, None),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    let actions = answer(&messages, 2)["result"].as_array().unwrap();
    let format = actions
        .iter()
        .find(|action| action["title"] == "Format this document with uf fmt")
        .unwrap_or_else(|| panic!("no formatting action in {actions:#?}"));
    let edits = format["edit"]["changes"]["file:///a.js"]
        .as_array()
        .unwrap();
    assert_eq!(apply(source, edits), "// @flow\nconst a = 1;\n");

    // A tab in a *comment* is the live case for the guard: the formatter keeps
    // a comment's text, so offering it here would offer a fix that does not
    // fix. (A tab inside a string is no longer reported at all — the linter
    // was taught that those belong to the string, so `uf fmt` and `uf lint`
    // can converge.)
    let commented = "// @flow\n// a\tcomment\nexport const a = 1;\n";
    let messages = lsp_session(&[
        did_open("file:///a.js", commented),
        code_action(2, "file:///a.js", 1, 4, None),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    assert_eq!(
        published(&messages)[0][0]["code"],
        "uniflowed/no-tabs",
        "{:#?}",
        published(&messages)[0]
    );
    let offered = answer(&messages, 2)["result"].as_array().unwrap().clone();
    assert!(
        !offered
            .iter()
            .any(|action| action["title"] == "Format this document with uf fmt"),
        "{offered:#?}"
    );
}

/// A hover over a squiggle says which rule it is and what the rule is for.
#[test]
fn lsp_hovers_a_diagnostic_with_the_rule_behind_it() {
    let messages = lsp_session(&[
        did_open("file:///a.js", "// @flow\ntype B = bool;\n"),
        hover_at(2, "file:///a.js", 1, 10),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    let contents = answer(&messages, 2)["result"]["contents"].clone();
    assert_eq!(contents["kind"], "markdown");
    let value = contents["value"].as_str().unwrap();
    assert!(value.contains("flow/deprecated-type"), "{value}");
    assert!(value.contains("write `boolean`"), "{value}");
    // The catalogue entry `uf inspect` prints, not just the message.
    assert!(value.contains("Flow's own lint set"), "{value}");
    assert!(value.contains("default `error`"), "{value}");
    // And the range of the thing hovered, so the editor underlines it.
    assert_eq!(
        answer(&messages, 2)["result"]["range"]["start"]["character"],
        9
    );
}

/// A hover over an import specifier says what the specifier names.
#[test]
fn lsp_hovers_an_import_specifier() {
    let messages = lsp_session(&[
        did_open(
            "file:///a.js",
            "// @flow\nimport { Node } from \"@uniflowed/react\";\nimport { db } from \"@uniflowed/server\";\n",
        ),
        hover_at(2, "file:///a.js", 1, 30),
        hover_at(3, "file:///a.js", 2, 25),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    let module = answer(&messages, 2)["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(module.contains("@uniflowed/react"), "{module}");
    assert!(module.contains("framework"), "{module}");
    assert!(module.contains("`Node`"), "{module}");

    let server = answer(&messages, 3)["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(server.contains("Server-only"), "{server}");
}

/// A hover with no answer is `null`, and that is deliberate.
///
/// The type at a position is the answer a reader wants over an expression, and
/// uf cannot give it: `uf_check` runs Flow's inference but exposes only whole
/// file diagnostics, so there is no positional query to ask. An empty popup
/// would read as "uf looked and this has no type", which is a different and
/// false claim; `null` is the protocol's word for "nothing to say".
#[test]
fn lsp_has_no_hover_for_an_expression() {
    let messages = lsp_session(&[
        did_open("file:///a.js", "// @flow\nconst total = 1 + 2;\n"),
        hover_at(2, "file:///a.js", 1, 8),
        hover_at(3, "file:///a.js", 40, 0),
        hover_at(4, "file:///nope.js", 0, 0),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]);

    assert_eq!(answer(&messages, 2)["result"], serde_json::Value::Null);
    assert_eq!(answer(&messages, 3)["result"], serde_json::Value::Null);
    // A document the server was never told about is not an error either.
    assert_eq!(answer(&messages, 4)["result"], serde_json::Value::Null);
}

#[test]
fn fmt_reports_an_already_formatted_project() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf().arg("--cwd").arg(&app).arg("fmt").output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("uf fmt"));
    assert!(stdout.contains("✓"));
}

/// `uf fmt` must not rewrite `package.json`.
///
/// Discovery returns it because the linter reads it, and the formatter used to
/// take the same list — so `uf fmt` inserted a statement terminator after
/// `"@uniflowed/core": "latest"` and left the manifest unparseable. `uf install`
/// then failed on a project whose only crime was running `uf fmt`.
///
/// A scaffolded project is a real one: this is what `uf create && uf fmt` did.
#[test]
fn fmt_leaves_the_package_manifest_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let manifest = app.join("package.json");
    let before = fs::read_to_string(&manifest).unwrap();
    serde_json::from_str::<serde_json::Value>(&before).expect("the scaffold writes valid JSON");

    let output = uf().arg("--cwd").arg(&app).arg("fmt").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let after = fs::read_to_string(&manifest).unwrap();
    serde_json::from_str::<serde_json::Value>(&after)
        .expect("uf fmt must leave package.json parseable");
    assert_eq!(before, after, "uf fmt rewrote package.json");
}

/// And `--check` must not claim it needs formatting either, which is how a
/// green CI job would start failing for a file the formatter must never touch.
#[test]
fn fmt_check_ignores_the_package_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf()
        .arg("--cwd")
        .arg(&app)
        .args(["fmt", "--check"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("package.json"),
        "uf fmt --check listed package.json:\n{stdout}"
    );
    assert!(
        output.status.success(),
        "a freshly scaffolded project must pass `uf fmt --check`:\n{stdout}"
    );
}

/// `uf env use` records the profile, and says which files it selected.
///
/// It writes `.uniflowed/profile`. It used to write `.uniflowed/env`, which is
/// the directory `uf env install` links this project's toolchain into — so
/// whichever of the two ran second broke the other. What reads the profile back
/// is `tests/env_files.rs`; this is about what a person types and sees.
#[test]
fn env_use_records_the_active_environment() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}\n").unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "use", "staging"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("✓ mode staging"), "{stdout}");
    assert!(stdout.contains(".env.staging"), "{stdout}");
    assert_eq!(
        fs::read_to_string(dir.path().join(".uniflowed/profile")).unwrap(),
        "staging\n"
    );
}
