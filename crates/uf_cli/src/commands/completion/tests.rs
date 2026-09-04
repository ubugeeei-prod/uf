use clap::CommandFactory;

use super::*;

/// This repository's own task names.
const TASKS: &[&str] = &["build", "ci", "docs:build", "rust:test", "test:lib"];

fn words(line: &[&str]) -> Vec<String> {
    line.iter().map(ToString::to_string).collect()
}

fn complete_line(line: &[&str]) -> Vec<String> {
    candidates(&words(line), TASKS)
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
    assert_eq!(complete_line(&["in"]), vec!["info", "inspect", "install"]);
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
    assert_eq!(complete_line(&["create", ""]), vec!["app", "lib"]);
    assert_eq!(complete_line(&["env", ""]), vec!["doctor", "use"]);
    assert!(complete_line(&["explain", ""]).contains(&"build".to_string()));
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
    assert!(candidates(&[], TASKS).contains(&"build".to_string()));
}

/// The hand-written table has to be clap's, or completion offers a command
/// that does not exist and hides one that does.
///
/// Hidden commands are deliberately absent: `uf transform` is spawned by the
/// Vite plugin and `uf __complete` by a completion script, and neither is a
/// thing a person types. `help` is present and is clap's own, which is why it
/// is added here rather than found among the subcommands.
#[test]
fn the_command_list_matches_the_argument_parser() {
    let command = crate::Cli::command();
    let mut from_clap = command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set())
        .flat_map(|sub| {
            std::iter::once(sub.get_name().to_owned())
                .chain(sub.get_all_aliases().map(ToOwned::to_owned))
        })
        .collect::<std::collections::BTreeSet<_>>();
    from_clap.insert("help".to_owned());

    let ours = COMMANDS
        .iter()
        .map(ToString::to_string)
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        ours, from_clap,
        "the completion command list and the parser disagree"
    );
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
