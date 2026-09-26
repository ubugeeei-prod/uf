//! `uf completion`: shell completion, including the parts only uf knows.
//!
//! Generated completion is normally a static picture of the argument parser,
//! taken at build time. That covers subcommand and flag names and stops exactly
//! where it gets interesting: the argument someone actually mistypes is a *task
//! name*, which lives in `uf.config.js` and is different in every project.
//!
//! So the shipped scripts are thin. Each one collects the words typed so far
//! and asks `uf __complete` what could come next; the answer is computed by the
//! same binary, reading the same config, so a task added to `uf.config.js` is
//! completable immediately with no regeneration and nothing to reinstall.
//!
//! [`candidates`] is that answer, and it is a pure function of the words and
//! the project — which is what makes it testable without a shell.

mod scripts;

#[cfg(test)]
mod tests;

use anyhow::Result;
use camino::Utf8Path;
use clap::CommandFactory;
use uf_config::{TaskArgument, load_config};

use crate::Cli;
use crate::cli::Shell;
use crate::ui::Ui;

/// Flags accepted anywhere.
const GLOBAL_FLAGS: &[&str] = &["--cwd", "--color", "--help", "--version"];

/// What `--color` takes.
const COLOR_VALUES: &[&str] = &["auto", "always", "never"];

/// What `uf release` takes.
const RELEASE_BUMPS: &[&str] = &["alpha", "patch", "minor", "major"];

/// Print the completion script for `shell`.
pub(crate) fn completion(ui: &mut Ui, shell: Shell) {
    ui.plain(scripts::script(shell));
}

/// Print one candidate per line, for the shipped scripts to consume.
///
/// `words` is everything typed after `uf`, with the word being completed last —
/// possibly empty, which is what "the cursor is at a fresh word" looks like.
pub(crate) fn complete(cwd: &Utf8Path, ui: &mut Ui, words: &[String]) -> Result<()> {
    let tasks = project_tasks(cwd);
    let tasks = tasks
        .iter()
        .map(|(name, args)| Task {
            name: name.as_str(),
            args,
        })
        .collect::<Vec<_>>();
    let candidates = candidates(words, &tasks);

    let mut out = String::new();
    for candidate in &candidates {
        out.push_str(candidate);
        out.push('\n');
    }
    ui.plain(&out);
    Ok(())
}

/// The project's tasks and the arguments each declares, or nothing when there
/// is no readable project.
///
/// A completion that failed loudly would print an error into the middle of
/// someone's command line. There is nothing to say here: either uf knows the
/// tasks or it does not, and not knowing them completes to nothing.
fn project_tasks(cwd: &Utf8Path) -> Vec<(String, Vec<TaskArgument>)> {
    load_config(cwd).map_or_else(
        |_| Vec::new(),
        |resolved| {
            resolved
                .config
                .tasks
                .iter()
                .map(|(name, task)| (name.to_string(), task.args().to_vec()))
                .collect()
        },
    )
}

/// One task, as completion needs it.
#[derive(Debug, Clone, Copy)]
struct Task<'a> {
    name: &'a str,
    args: &'a [TaskArgument],
}

/// What could come next, given the words typed so far.
///
/// Pure, so the whole surface is testable without a shell or a project on disk.
fn candidates(words: &[String], tasks: &[Task<'_>]) -> Vec<String> {
    let (current, before) = match words.split_last() {
        Some((current, before)) => (current.as_str(), before),
        None => ("", &[][..]),
    };

    // A value that belongs to the flag before it, rather than a fresh word.
    if let Some(previous) = before.last() {
        match previous.as_str() {
            "--color" => return matching(current, COLOR_VALUES.iter().copied()),
            // A directory, which the shell completes better than uf can.
            "--cwd" => return Vec::new(),
            _ => {}
        }
    }

    // The subcommand: the first word that is not a global flag or its value.
    let mut positional = Vec::new();
    let mut at = Vec::new();
    let mut skip_next = false;
    for (index, word) in before.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if word == "--cwd" || word == "--color" {
            skip_next = true;
            continue;
        }
        if !word.starts_with('-') {
            positional.push(word.as_str());
            at.push(index);
        }
    }

    // After `uf run <task>`, the words belong to the task, and a task that
    // declares its `args` says what they may be. One that declares none
    // completes as it always did.
    if let ["run", name, ..] = positional.as_slice()
        && let Some(task) = tasks
            .iter()
            .find(|task| task.name == *name && !task.args.is_empty())
    {
        return task_argument(current, &before[at[1] + 1..], task.args);
    }

    if current.starts_with('-') {
        return matching(current, GLOBAL_FLAGS.iter().copied());
    }

    match positional.as_slice() {
        ["run"] => matching(current, tasks.iter().map(|task| task.name)),
        ["release"] => matching(current, RELEASE_BUMPS.iter().copied()),
        // `explain::KNOWN` itself, not a copy of part of it. These were two
        // hand-maintained lists and nothing compared them, so completion
        // offered a subset — missing `install`, `upgrade`, `run`, `exec` and
        // `env`, every one of which `uf explain` answers — and a person using
        // tab completion to find out what is explainable was told less than
        // the truth (ubugeeei-prod/uf#425).
        //
        // A test that failed when they differed was the other option, and it
        // would have kept the second list rather than removed it. This is a
        // list that cannot drift because there is only one of it.
        ["explain"] => matching(current, super::explain::KNOWN.iter().copied()),
        // The registry's own names, read out of this binary, so a component
        // added to `registry/ui/` completes in the release that carries it.
        ["ui", "add" | "diff" | "update", ..] => matching(current, ui_components()),
        // Everything else that completes is a subcommand, and those are the
        // parser's: at the top level, which is the empty path, and under every
        // parent below it.
        path => subcommands(path).map_or_else(Vec::new, |names| {
            matching(current, names.iter().map(String::as_str))
        }),
    }
}

/// What the word being typed after `uf run <task> <after...>` could be.
///
/// The same rules `uf run` fills a task's arguments by, run over what has been
/// typed so far: the value of a `--name` just written, a `--name` not yet
/// given, or the choices of the next argument a bare word would fill. An
/// argument with no `choices` takes any value, and completes to nothing — the
/// shell's own file completion is a better guess at it than uf's.
fn task_argument(current: &str, after: &[String], declared: &[TaskArgument]) -> Vec<String> {
    let declared_named = |word: &str| {
        let name = word.strip_prefix("--")?;
        declared.iter().position(|argument| argument.name == name)
    };
    let choices = |at: usize| {
        matching(
            current,
            declared[at]
                .choices
                .iter()
                .map(compact_str::CompactString::as_str),
        )
    };

    if let Some(at) = after.last().and_then(|word| declared_named(word)) {
        return choices(at);
    }

    let mut filled = vec![false; declared.len()];
    let mut words = after.iter();
    while let Some(word) = words.next() {
        let (name, inline) = match word.split_once('=') {
            Some((name, _)) => (name, true),
            None => (word.as_str(), false),
        };
        if let Some(at) = declared_named(name) {
            filled[at] = true;
            if !inline {
                words.next();
            }
        } else if !word.starts_with('-')
            && let Some(slot) = filled.iter_mut().find(|slot| !**slot)
        {
            *slot = true;
        }
    }

    if current.starts_with('-') {
        let flags: Vec<String> = declared
            .iter()
            .zip(&filled)
            .filter(|(_, filled)| !**filled)
            .map(|(argument, _)| uf_infra::into_string(uf_infra::cstr!("--{}", argument.name)))
            .collect();
        return matching(current, flags.iter().map(String::as_str));
    }
    filled
        .iter()
        .position(|filled| !filled)
        .map_or_else(Vec::new, choices)
}

/// Every name the parser accepts for a visible subcommand of the command
/// `path` names, in `uf --help` order — or `None` when `path` names no command,
/// or names one that takes arguments rather than subcommands.
///
/// Read from clap rather than written out, at every depth. Each parent's
/// subcommands used to be a list in [`candidates`], and `env`'s had stopped at
/// `doctor` and `use` while the parser grew `install`, `list`, `update`, `exec`
/// and `gc` — so a person finding the toolchain commands by TAB never met
/// `uf env install`, and the test beside the list asserted the subset
/// (ubugeeei-prod/uf#1012). That is #425's shape one level down, and it gets
/// #425's fix: a list that cannot drift because there is only one of it, so
/// adding a subcommand to `cli.rs` is adding it to completion.
///
/// The top level is the same walk with an empty path. It had a hand-written
/// copy, on the argument that building the command tree is a cost on a path
/// that has to feel instant; but `uf __complete` only reaches this module
/// through `Cli::try_parse_from`, which has built that tree once already, so
/// the copy saved one repetition of work every completion request pays anyway.
///
/// `build` first, because that is what puts clap's own `help` beside every
/// parent's subcommands — `uf env help` is a command the parser accepts — and
/// fills in the tree under `uf help`. Hidden commands are not offered, since
/// `uf __complete` and `uf transform` are spawned by scripts rather than typed,
/// but a hidden command someone *has* typed is still walked into, so
/// `uf create <TAB>` offers `app` and `lib`. Every alias is offered beside its
/// command, hidden ones too: hiding `uninstall` keeps `uf --help` short, and
/// completion is for finishing a word someone has already started typing.
fn subcommands(path: &[&str]) -> Option<Vec<String>> {
    let mut parser = Cli::command();
    parser.build();

    let mut current = &parser;
    for word in path {
        current = current.get_subcommands().find(|command| {
            command.get_name() == *word || command.get_all_aliases().any(|alias| alias == *word)
        })?;
    }

    let names = current
        .get_subcommands()
        .filter(|command| !command.is_hide_set())
        .flat_map(|command| std::iter::once(command.get_name()).chain(command.get_all_aliases()))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    (!names.is_empty()).then_some(names)
}

/// The components `uf ui add` can write, or none from a binary whose registry
/// cannot be read — a completion has nowhere to print an error.
fn ui_components() -> Vec<&'static str> {
    uf_ui::Registry::embedded().map_or_else(
        |_| Vec::new(),
        |registry| {
            registry
                .components()
                .iter()
                .map(|component| component.name)
                .collect()
        },
    )
}

/// Those of `pool` that start with `prefix`, in the order `pool` gave them.
fn matching<'a>(prefix: &str, pool: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    pool.into_iter()
        .filter(|candidate| candidate.starts_with(prefix))
        .map(ToOwned::to_owned)
        .collect()
}
