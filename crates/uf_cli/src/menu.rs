//! What `uf` on its own does.
//!
//! It used to print the help: twenty commands, each a name and a sentence, and
//! then a prompt for the reader to type one of the names they had just read.
//! The list was the answer to a question nobody asked — someone who runs `uf`
//! with no arguments is not looking for documentation, they are deciding what
//! to do next — so it is a menu now, and the thing they were going to type is
//! the thing they press Enter on.
//!
//! The help has not gone anywhere. `uf --help` is still the help, and so is
//! `uf` itself the moment there is nobody to ask: a pipeline, a CI job, a
//! `dumb` terminal. Every one of those gets exactly what it got before, which
//! is what keeps a menu from being a breaking change.
//!
//! The project's own tasks are on the menu beside the commands, because
//! `uf run` is most of what a repository's own contributors type and a menu
//! that omitted them would be a menu of the half they use least.

use camino::Utf8Path;
use uf_term::prompt::{Choice, Outcome, Request, select};

use crate::commands::task::task_names;

/// The commands that need no arguments, in the order they should be offered.
///
/// Ordered by how often a person reaches for them rather than alphabetically:
/// a menu is read from the top, and `use` being three rows above `test` would
/// be alphabetical and wrong.
///
/// The list is deliberately not every command. `create`, `exec`, `explain` and
/// `use` all *require* an argument, and offering them here would mean choosing
/// one and being told off by the parser — a menu whose entries do not work is
/// worse than a list. They are still one `uf --help` away, and the last row
/// says so.
const RUNNABLE: &[(&str, &str)] = &[
    ("dev", "Start the development server"),
    ("build", "Build the project for production"),
    ("test", "Run the test suite"),
    ("check", "Lint the project, then type check it with Flow"),
    ("lint", "Lint the project"),
    ("fmt", "Format every file uf understands"),
    ("install", "Install the project's dependencies"),
    ("info", "Describe the project uf sees"),
    ("inspect", "Show the resolved pipeline, stage by stage"),
    ("prepare", "Get the working tree ready to release"),
    ("publish", "Publish what `uf prepare` staged"),
    ("upgrade", "Update uf itself"),
];

/// The row that goes back to what `uf` used to print.
const HELP: (&str, &str) = ("help", "Every command, including the ones taking arguments");

/// The heading over the commands.
const COMMANDS: &str = "commands";
/// The heading over the project's own tasks.
const TASKS: &str = "tasks";

/// What the reader chose, as the words they would otherwise have typed.
///
/// `Some(["run", "ci"])` for a task, `Some(["build"])` for a command, and
/// `None` when there is nothing to run — either because nobody was there to
/// ask or because they changed their mind.
pub(crate) enum Chosen {
    /// Run this, as if it had been typed.
    Run(Vec<String>),
    /// Print the help, which is what `uf` did before there was a menu.
    Help,
    /// Do nothing at all, quietly.
    Nothing,
}

/// Offer the commands and the project's tasks, and return what was picked.
pub(crate) fn choose(cwd: &Utf8Path) -> Chosen {
    // Owned first: `Choice` borrows its strings, so every task name has to
    // outlive the menu rather than be built into it.
    let tasks = task_names(cwd);
    let mut labels: Vec<String> = Vec::with_capacity(tasks.len());
    for (name, _) in &tasks {
        labels.push(format!("run {name}"));
    }

    let mut choices: Vec<Choice<'_>> = Vec::with_capacity(RUNNABLE.len() + tasks.len() + 1);
    for (name, about) in RUNNABLE {
        choices.push(Choice::grouped(name, about, COMMANDS));
    }
    for (label, (_, command)) in labels.iter().zip(&tasks) {
        // The command it runs, rather than nothing: a task's name says what it
        // is for and the command says what it will actually do, which is the
        // thing a reader is checking before they press Enter.
        choices.push(Choice::grouped(label, command, TASKS));
    }
    choices.push(Choice::grouped(HELP.0, HELP.1, COMMANDS));

    let request = Request::new("What would you like to run?", &choices);
    match select(&request) {
        Outcome::Chose(choice) if choice.name == HELP.0 => Chosen::Help,
        Outcome::Chose(choice) => Chosen::Run(
            choice
                .name
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect(),
        ),
        // No terminal: whatever `uf` printed before a menu existed, it still
        // prints, so nothing reading uf's output has to know this is here.
        Outcome::NotInteractive => Chosen::Help,
        Outcome::Cancelled => Chosen::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_offered_command_is_one_the_parser_accepts() {
        // A menu entry that does not parse is a menu entry that fails in front
        // of the person who chose it.
        for (name, _) in RUNNABLE {
            assert!(
                crate::parses_as_command(name),
                "`uf {name}` is on the menu and is not a command"
            );
        }
    }

    #[test]
    fn no_offered_command_requires_an_argument() {
        // The whole reason the menu is a subset: choosing `create` would parse
        // as a command and then be rejected for the argument it did not get.
        for (name, _) in RUNNABLE {
            assert!(
                crate::runs_with_no_arguments(name),
                "`uf {name}` needs an argument and cannot be a menu entry"
            );
        }
    }

    #[test]
    fn the_help_row_is_not_mistaken_for_a_command() {
        assert!(!RUNNABLE.iter().any(|(name, _)| *name == HELP.0));
    }
}
