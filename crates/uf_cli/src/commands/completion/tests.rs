use clap::CommandFactory;

use super::*;

/// This repository's own task names.
const TASKS: &[&str] = &["build", "ci", "docs:build", "rust:test", "test:lib"];

fn words(line: &[&str]) -> Vec<String> {
    line.iter().map(ToString::to_string).collect()
}

fn tasks() -> Vec<Task<'static>> {
    TASKS.iter().map(|name| Task { name, args: &[] }).collect()
}

fn complete_line(line: &[&str]) -> Vec<String> {
    candidates(&words(line), &tasks())
}

#[test]
fn a_fresh_command_line_offers_every_subcommand() {
    let out = complete_line(&[""]);

    for command in ["build", "check", "dev", "run", "test"] {
        assert!(out.contains(&command.to_string()), "{command} missing");
    }
}

#[test]
fn a_partial_subcommand_narrows_to_what_starts_with_it() {
    assert_eq!(complete_line(&["ru"]), vec!["run"]);
    // In the list's own order, which is `uf --help`'s: `init` is where
    // `create` was, ahead of the alphabet.
    assert_eq!(
        complete_line(&["in"]),
        vec!["init", "info", "inspect", "install"]
    );
}

/// The whole point: task names come from the project, not from the parser.
#[test]
fn run_completes_the_projects_own_task_names() {
    assert_eq!(complete_line(&["run", ""]), TASKS);
    assert_eq!(
        complete_line(&["run", "rust"]),
        vec!["rust:test"],
        "a prefix narrows to the tasks that start with it"
    );
    assert_eq!(complete_line(&["run", "docs"]), vec!["docs:build"]);
}

#[test]
fn a_task_name_that_matches_nothing_completes_to_nothing() {
    assert!(complete_line(&["run", "zzz"]).is_empty());
}

#[test]
fn enum_arguments_complete_to_their_variants() {
    assert_eq!(
        complete_line(&["release", ""]),
        vec!["alpha", "patch", "minor", "major"]
    );
    assert!(complete_line(&["explain", ""]).contains(&"build".to_string()));
}

/// `uf env <TAB>` offers the toolchain commands `uf env` has, not the two a
/// hand-written list had (ubugeeei-prod/uf#1012), and every other parent offers
/// its own the same way — `help` included, because clap accepts `uf env help`.
#[test]
fn a_parent_completes_to_its_subcommands() {
    let env = complete_line(&["env", ""]);
    for command in ["doctor", "use", "install", "list", "update", "exec", "gc"] {
        assert!(
            env.contains(&command.to_string()),
            "`uf env {command}` is not offered"
        );
    }
    assert_eq!(complete_line(&["env", "u"]), vec!["use", "update"]);

    assert_eq!(complete_line(&["create", ""]), vec!["app", "lib", "help"]);
    assert_eq!(
        complete_line(&["i18n", ""]),
        vec!["extract", "merge", "help"]
    );
    assert_eq!(complete_line(&["routes", ""]), vec!["list", "add", "help"]);
    assert_eq!(
        complete_line(&["ui", ""]),
        vec!["add", "list", "diff", "help"]
    );
    assert_eq!(complete_line(&["pm", ""]), vec!["approve-builds", "help"]);
    assert_eq!(complete_line(&["catalog", ""]), vec!["set", "help"]);
}

/// `uf ui add <TAB>` offers the components this binary carries, and keeps
/// offering them after the first, because `uf ui add button dialog` is one
/// command.
#[test]
fn ui_add_and_diff_complete_to_the_registrys_components() {
    let registry = uf_ui::Registry::embedded().expect("the registry reads");
    let every: Vec<String> = registry
        .components()
        .iter()
        .map(|component| component.name.to_owned())
        .collect();

    assert_eq!(complete_line(&["ui", "add", ""]), every);
    assert_eq!(complete_line(&["ui", "add", "button", ""]), every);
    assert_eq!(complete_line(&["ui", "diff", "di"]), vec!["dialog"]);
    assert!(complete_line(&["ui", "list", ""]).is_empty());
}

/// Completion offers exactly what `uf explain` answers, because it is the same
/// list rather than a copy of it.
///
/// The copy had drifted both ways at once: `install`, `run`, `exec`, `env` and
/// the rest were answerable and never offered, and four names were offered by a
/// list that did not have them (ubugeeei-prod/uf#425). A reader who presses TAB
/// to find out what is explainable was told less than the truth in one
/// direction and more in the other.
#[test]
fn explain_completes_everything_it_can_explain() {
    let offered = complete_line(&["explain", ""]);

    for command in [
        "install",
        "run",
        "exec",
        "env",
        "lsp",
        "mcp",
        "self-update",
        "use",
        "ls",
        "audit",
        "search",
        "uninstall",
    ] {
        assert!(
            offered.contains(&command.to_string()),
            "`uf explain {command}` is answered and not offered"
        );
    }
    assert_eq!(offered.len(), crate::commands::explain::KNOWN.len());
}

#[test]
fn a_flag_completes_to_the_global_flags() {
    assert_eq!(complete_line(&["--c"]), vec!["--cwd", "--color"]);
    assert!(complete_line(&["build", "--"]).contains(&"--help".to_string()));
}

#[test]
fn a_flags_value_completes_to_that_flags_values() {
    assert_eq!(
        complete_line(&["--color", ""]),
        vec!["auto", "always", "never"]
    );
    assert_eq!(complete_line(&["--color", "a"]), vec!["auto", "always"]);
}

/// A directory is something the shell completes better than uf can, so uf says
/// nothing rather than offering a worse list.
#[test]
fn a_directory_argument_is_left_to_the_shell() {
    assert!(complete_line(&["--cwd", ""]).is_empty());
}

/// The subcommand is the first *positional* word, so a global flag before it
/// must not be mistaken for one.
#[test]
fn global_flags_before_the_subcommand_do_not_confuse_it() {
    assert_eq!(
        complete_line(&["--color", "never", "run", "ru"]),
        vec!["rust:test"]
    );
    assert_eq!(complete_line(&["--cwd", "/tmp", "run", "ci"]), vec!["ci"]);
}

#[test]
fn an_argument_nothing_is_known_about_completes_to_nothing() {
    assert!(complete_line(&["lsp", ""]).is_empty());
    assert!(complete_line(&["run", "build", ""]).is_empty());
}

#[test]
fn an_empty_word_list_offers_the_subcommands() {
    assert!(candidates(&[], &tasks()).contains(&"build".to_string()));
}

/// Every parent, at every depth, offers exactly what the parser accepts under
/// it — completion offering a command that does not exist, or hiding one that
/// does, is the whole failure this guards.
///
/// The test that stood here compared the top-level list with clap and nothing
/// below it, so `env`'s list fell five subcommands behind with every test
/// green, and the test beside that list asserted the subset
/// (ubugeeei-prod/uf#1012). This walks the whole tree instead, hidden parents
/// included: `uf create` is hidden from `--help` and can still be typed.
///
/// Both directions. A visible subcommand that is not offered is a command
/// nobody finds by TAB; an offered word the parser has no subcommand for is a
/// command line that fails on Enter. Hidden commands are the one deliberate
/// absence — `uf transform` is spawned by the Vite plugin and `uf __complete`
/// by a completion script, and neither is a thing a person types — and `help`
/// is present because the parser accepts it under every parent.
#[test]
fn every_parent_offers_exactly_the_parsers_subcommands() {
    let mut parser = crate::Cli::command();
    parser.build();

    let mut walked = Vec::new();
    let mut pending = vec![(Vec::<&str>::new(), &parser)];
    while let Some((path, command)) = pending.pop() {
        for sub in command.get_subcommands() {
            let mut deeper = path.clone();
            deeper.push(sub.get_name());
            pending.push((deeper, sub));
        }

        let accepted = command
            .get_subcommands()
            .filter(|sub| !sub.is_hide_set())
            .flat_map(|sub| std::iter::once(sub.get_name()).chain(sub.get_all_aliases()))
            .map(ToOwned::to_owned)
            .collect::<std::collections::BTreeSet<_>>();
        if accepted.is_empty() {
            continue;
        }

        let mut line = path.clone();
        line.push("");
        let offered = complete_line(&line)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            offered,
            accepted,
            "`uf {} <TAB>` and the parser disagree",
            path.join(" ")
        );
        walked.push(path.join(" "));
    }

    for parent in ["", "env", "create", "ui", "help"] {
        assert!(
            walked.iter().any(|path| path == parent),
            "the walk never reached `uf {parent}`, so it proves nothing about it"
        );
    }
}

// --- the shipped scripts -----------------------------------------------

#[test]
fn every_shell_ships_a_script_that_calls_back_into_uf() {
    for shell in [
        Shell::Bash,
        Shell::Zsh,
        Shell::Fish,
        Shell::Elvish,
        Shell::PowerShell,
    ] {
        let script = scripts::script(shell);

        assert!(!script.is_empty(), "{shell:?} ships no script");
        assert!(
            script.contains("uf __complete"),
            "{shell:?} does not ask uf for candidates, so task names will not complete"
        );
        assert!(
            script.contains("ufr") && script.contains("ufx"),
            "{shell:?} does not complete the alias binaries"
        );
        assert!(
            script.contains("# uf completion for"),
            "{shell:?} does not say how to install it"
        );
    }
}

/// A task's declared arguments complete like `uf run` fills them.
#[test]
fn run_completes_a_tasks_declared_arguments() {
    let mut target = TaskArgument::named("target");
    target.choices = vec!["staging".into(), "production".into()];
    let mut region = TaskArgument::named("region");
    region.choices = vec!["us".into(), "eu".into()];
    let args = [target, TaskArgument::named("tag"), region];
    let tasks = [Task {
        name: "deploy",
        args: &args,
    }];
    let complete = |line: &[&str]| candidates(&words(line), &tasks);

    assert_eq!(complete(&["run", "deploy", ""]), ["staging", "production"]);
    assert_eq!(complete(&["run", "deploy", "p"]), ["production"]);
    // The second argument takes any value, which completes to nothing.
    assert!(complete(&["run", "deploy", "staging", ""]).is_empty());
    assert_eq!(
        complete(&["run", "deploy", "staging", "v1", ""]),
        ["us", "eu"]
    );
    // By name: the value of the flag just written, then what is left.
    assert_eq!(complete(&["run", "deploy", "--region", ""]), ["us", "eu"]);
    assert_eq!(
        complete(&["run", "deploy", "--target=staging", "--"]),
        ["--tag", "--region"]
    );
    assert_eq!(
        complete(&["run", "deploy", "--tag", "v1", ""]),
        ["staging", "production"]
    );
    assert!(complete(&["run", "deploy", "staging", "v1", "eu", ""]).is_empty());
}
