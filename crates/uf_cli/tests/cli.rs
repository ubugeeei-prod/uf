//! End-to-end coverage for the commands that scaffold, build, and serve.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use support::{assert_plain, binary, create_app, uf};

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

    // Compared line by line, skipping the one that carries a duration: two
    // runs of the same command differ by a few milliseconds and that is not a
    // difference between the alias and the command.
    let lines = |output: &[u8]| {
        String::from_utf8_lossy(output)
            .lines()
            .filter(|line| !line.contains("ms"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(&short.stdout), lines(&long.stdout));
    assert!(
        String::from_utf8_lossy(&short.stdout).contains("uf install"),
        "`uf i` should report itself as `uf install`"
    );
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

#[test]
fn alias_binaries_print_the_root_version() {
    for name in ["ufr", "ufx"] {
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
    }
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
    assert!(stdout.contains("UfNative"));
    assert!(stdout.contains("exec-cache"));
    assert!(stdout.contains("created 8 files"));
    assert!(dir.path().join("app.js").exists());
    assert!(
        dir.path()
            .join(".uf/exec-cache/_uniflowed_create.json")
            .exists()
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
    assert!(stdout.contains("✓ created 8 files"));
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
        stderr.contains("dev, build, test, fmt, lint, check"),
        "{stderr}"
    );
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

#[test]
fn lsp_initialize_returns_native_capabilities() {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let input = format!("Content-Length: {}\r\n\r\n{body}", body.len());

    let output = uf().arg("lsp").write_stdin(input).output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("Content-Length: "));
    assert!(stdout.contains(r#""name":"uf-lsp""#));
    assert!(stdout.contains(r#""documentFormattingProvider":true"#));
    assert!(stdout.contains(r#""workspaceDiagnostics":true"#));
    assert_plain(&stdout);
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

#[test]
fn env_use_records_the_active_environment() {
    let dir = tempfile::tempdir().unwrap();

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
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("✓ active environment: staging")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(".uniflowed/env")).unwrap(),
        "staging\n"
    );
}
