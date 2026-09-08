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
use uf_config::load_config;

use crate::cli::Shell;
use crate::ui::Ui;

/// Every subcommand `uf` accepts, in help order.
///
/// Written out rather than read from clap: `Commands` is an enum whose variants
/// clap knows the spelling of, and asking clap for them at runtime costs a
/// command tree build on a path that has to feel instant. The list is checked
/// against clap's own in `tests`, so it cannot drift.
const COMMANDS: &[&str] = &[
    "add",
    "audit",
    "build",
    "pm",
    "patch",
    "catalog",
    "check",
    "init",
    "new",
    "dev",
    "doc",
    "env",
    "exec",
    "explain",
    "fmt",
    "i18n",
    "info",
    "inspect",
    "install",
    "i",
    "clean",
    "lint",
    "ls",
    "lsp",
    "prepare",
    "preview",
    "publish",
    "release",
    "remove",
    "uninstall",
    "routes",
    "run",
    "search",
    "start",
    "test",
    "update",
    "upgrade",
    "use",
    "why",
    "completion",
    "help",
];

/// Flags accepted anywhere.
const GLOBAL_FLAGS: &[&str] = &["--cwd", "--color", "--help", "--version"];

/// What `--color` takes.
const COLOR_VALUES: &[&str] = &["auto", "always", "never"];

/// What `uf release` takes.
const RELEASE_BUMPS: &[&str] = &["alpha", "patch", "minor", "major"];

/// What `uf create` takes.
const CREATE_KINDS: &[&str] = &["app", "lib"];

/// Print the completion script for `shell`.
pub(crate) fn completion(ui: &mut Ui, shell: Shell) {
    ui.plain(scripts::script(shell));
}

/// Print one candidate per line, for the shipped scripts to consume.
///
/// `words` is everything typed after `uf`, with the word being completed last —
/// possibly empty, which is what "the cursor is at a fresh word" looks like.
pub(crate) fn complete(cwd: &Utf8Path, ui: &mut Ui, words: &[String]) -> Result<()> {
    let tasks = task_names(cwd);
    let names = tasks.iter().map(String::as_str).collect::<Vec<_>>();
    let candidates = candidates(words, &names);

    let mut out = String::new();
    for candidate in &candidates {
        out.push_str(candidate);
        out.push('\n');
    }
    ui.plain(&out);
    Ok(())
}

/// The project's task names, or nothing when there is no readable project.
///
/// A completion that failed loudly would print an error into the middle of
/// someone's command line. There is nothing to say here: either uf knows the
/// tasks or it does not, and not knowing them completes to nothing.
fn task_names(cwd: &Utf8Path) -> Vec<String> {
    load_config(cwd).map_or_else(
        |_| Vec::new(),
        |resolved| {
            resolved
                .config
                .tasks
                .keys()
                .map(ToString::to_string)
                .collect()
        },
    )
}

/// What could come next, given the words typed so far.
///
/// Pure, so the whole surface is testable without a shell or a project on disk.
fn candidates(words: &[String], tasks: &[&str]) -> Vec<String> {
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

    if current.starts_with('-') {
        return matching(current, GLOBAL_FLAGS.iter().copied());
    }

    // The subcommand: the first word that is not a global flag or its value.
    let mut positional = Vec::new();
    let mut skip_next = false;
    for word in before {
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
        }
    }

    match positional.as_slice() {
        [] => matching(current, COMMANDS.iter().copied()),
        ["run"] => matching(current, tasks.iter().copied()),
        ["release"] => matching(current, RELEASE_BUMPS.iter().copied()),
        ["create"] => matching(current, CREATE_KINDS.iter().copied()),
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
        ["env"] => matching(current, ["doctor", "use"]),
        ["catalog"] => matching(current, ["set"]),
        ["pm"] => matching(current, ["approve-builds"]),
        ["i18n"] => matching(current, ["extract", "merge"]),
        ["routes"] => matching(current, ["list", "add"]),
        _ => Vec::new(),
    }
}

/// Those of `pool` that start with `prefix`, in the order `pool` gave them.
fn matching<'a>(prefix: &str, pool: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    pool.into_iter()
        .filter(|candidate| candidate.starts_with(prefix))
        .map(ToOwned::to_owned)
        .collect()
}
