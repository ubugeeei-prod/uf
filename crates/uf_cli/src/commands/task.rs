//! `uf run` and `ufx`: the two commands that hand control to another process.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::env;
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::{ResolvedConfig, TaskDefinition, TaskRunnerEngine, load_config};
use uf_pm::{Operation, PackageManager, command_for, detect_package_manager};
use uf_term::{Cell, Column, Status, Table, Tone, display_width, truncate_to_width};

use crate::cli::CreateCommand;
use crate::commands::{create, pm, test};
use crate::suggest::closest;
use crate::support::{plural, project_label};
use crate::ui::Ui;

pub(crate) fn run_task(cwd: &Utf8Path, script: &str, args: &[String]) -> Result<()> {
    let resolved = load_config(cwd)?;
    let mut visited = BTreeSet::new();
    run_named_task(&resolved, script, args, &mut visited)
}

/// Widest a task's command is shown at on the menu.
///
/// Shorter than [`COMMAND_WIDTH`] because the menu also carries a name column
/// and a two-space gutter, and a row that wraps stops being a row.
const MENU_COMMAND_WIDTH: usize = 44;

/// Every task this project defines: its name, and what it runs.
///
/// Returns nothing rather than an error when there is no config or it does not
/// load. This is asked on the way to drawing a menu, and a directory that is
/// not a uf project should get a menu of uf's commands rather than a parse
/// error about a file the reader has not written yet.
pub(crate) fn task_names(cwd: &Utf8Path) -> Vec<(String, String)> {
    let Ok(resolved) = load_config(cwd) else {
        return Vec::new();
    };
    resolved
        .config
        .tasks
        .iter()
        .map(|(name, task)| {
            let command = match task {
                TaskDefinition::Command(command) => command.to_string(),
                TaskDefinition::Detailed(details) => details.command.to_string(),
            };
            (name.to_string(), elide(&command, MENU_COMMAND_WIDTH))
        })
        .collect()
}

/// Widest a command is printed at in the task table.
///
/// A task may legitimately be a hundred characters of shell — this repository
/// has one — and printing it in full pushes every column after it off the
/// screen. The name is what the reader needs; the command is context.
const COMMAND_WIDTH: usize = 56;

/// `text`, cut to `width` columns with a trailing ellipsis if it did not fit.
fn elide(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_owned();
    }
    let mut out = truncate_to_width(text, width.saturating_sub(1)).to_owned();
    out.push('…');
    out
}

/// `uf run` with no task name: what this project can do.
///
/// A tool that requires a name before it will tell you the names is a tool you
/// have to read the config to use. This is the answer to "what can I run here",
/// and it is the same list the unknown-task error points at.
pub(crate) fn list_tasks(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let resolved = load_config(cwd)?;
    let tasks = &resolved.config.tasks;

    if tasks.is_empty() {
        ui.render(|renderer, out| {
            renderer.banner(out, "uf run", None);
            renderer.blank(out);
            renderer.status(
                out,
                Status::Info,
                "this project defines no tasks; add them under `tasks` in uf.config.js",
            );
        });
        return Ok(());
    }

    // Owned first, borrowed second: `Table` holds `&str`, so every cell's text
    // has to outlive the table rather than be built into it.
    let rows = tasks
        .iter()
        .map(|(name, task)| {
            let (command, after) = match task {
                TaskDefinition::Command(command) => (command.to_string(), String::new()),
                TaskDefinition::Detailed(details) => (
                    details.command.to_string(),
                    details
                        .depends_on
                        .iter()
                        .map(compact_str::CompactString::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            };
            // A task with no command of its own is Vite Task's, and saying so
            // is more useful than an empty cell.
            let runs = if command.trim().is_empty() {
                String::from("vite task")
            } else {
                elide(&command, COMMAND_WIDTH)
            };
            (name.to_string(), runs, after)
        })
        .collect::<Vec<_>>();

    let mut table = Table::new(vec![
        Column::left("task"),
        Column::left("runs"),
        Column::left("after"),
    ]);
    for (name, runs, after) in &rows {
        table.push(vec![
            Cell::toned(name, Tone::Accent),
            Cell::new(runs),
            Cell::toned(after, Tone::Muted),
        ]);
    }

    let count = tasks.len();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf run", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.table(out, 2, &table);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Info,
            &format!("{}; run one with `uf run <task>`", plural(count, "task")),
        );
    });

    Ok(())
}

fn run_named_task(
    resolved: &ResolvedConfig,
    script: &str,
    args: &[String],
    visited: &mut BTreeSet<String>,
) -> Result<()> {
    if !visited.insert(script.to_string()) {
        return Ok(());
    }

    let Some(task) = resolved.config.tasks.get(script) else {
        bail!(unknown_task(resolved, script));
    };

    if let TaskDefinition::Detailed(details) = task {
        for dependency in &details.depends_on {
            run_named_task(resolved, dependency.as_str(), &[], visited)?;
        }
    }

    execute_task(resolved, script, task, args)
}

/// The error for a task name that is not in `uf.config.js`.
///
/// `task "biuld" is not defined in uf.config.js` is true and unhelpful: the
/// reader knows what they typed, and what they want is the name they meant.
/// So the message names the closest tasks, and — when there are few enough to
/// read — every task the project defines, because someone who has just arrived
/// in a repository does not know what is on offer and should not have to open
/// the config to find out.
fn unknown_task(resolved: &ResolvedConfig, script: &str) -> String {
    let names = resolved
        .config
        .tasks
        .keys()
        .map(compact_str::CompactString::as_str)
        .collect::<Vec<_>>();

    let mut message = format!("task {script:?} is not defined in uf.config.js");
    if names.is_empty() {
        message.push_str("\n\n  this project defines no tasks");
        return message;
    }

    let suggestions = closest(script, names.iter().copied());
    if !suggestions.is_empty() {
        message.push_str("\n\n  did you mean: ");
        message.push_str(&suggestions.join(", "));
    }

    // A long list is a wall of text rather than an answer; past this many, the
    // suggestions are the help and `uf run` with no task name is where the rest
    // lives.
    const LISTED_IN_FULL: usize = 12;
    if names.len() <= LISTED_IN_FULL {
        message.push_str("\n\n  tasks: ");
        message.push_str(&names.join(", "));
    } else {
        message.push_str(&format!(
            "\n\n  {} tasks are defined; `uf run` lists them",
            names.len()
        ));
    }
    message
}

/// Run one task.
///
/// A task that names a command is run by uf, because `uf.config.js` is where
/// its meaning is written down and Vite Task has no way to read it — handing
/// `ci` to `vp run ci` asked Vite+ for a script it had never heard of, so
/// every task defined here failed on a machine that had `vp` and on one that
/// did not. A task with no command of its own is Vite+'s, and is handed over.
fn execute_task(
    resolved: &ResolvedConfig,
    script: &str,
    task: &TaskDefinition,
    args: &[String],
) -> Result<()> {
    if task.command().trim().is_empty()
        && resolved.config.task_runner.engine == TaskRunnerEngine::ViteTask
    {
        return execute_vite_task(resolved, script, args);
    }

    let command = if args.is_empty() {
        task.command().to_string()
    } else {
        format!("{} {}", task.command(), args.join(" "))
    };
    let mut process = ProcessCommand::new("sh");
    process.arg("-c").arg(&command);

    if let TaskDefinition::Detailed(details) = task {
        if let Some(cwd) = &details.cwd {
            process.current_dir(resolved.root.join(cwd.as_str()));
        } else {
            process.current_dir(&resolved.root);
        }
        process.envs(
            details
                .env
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
    } else {
        process.current_dir(&resolved.root);
    }

    let status = process.status().with_context(|| {
        format!("failed to run task {script:?} through the fallback task runner")
    })?;
    if !status.success() {
        bail!("task {script:?} exited with {status}");
    }
    Ok(())
}

fn execute_vite_task(resolved: &ResolvedConfig, script: &str, args: &[String]) -> Result<()> {
    let runner = env::var_os("UF_VITE_TASK_BIN").unwrap_or_else(|| "vp".into());
    let mut process = ProcessCommand::new(runner);
    process.arg("run").arg(script);
    if !args.is_empty() {
        process.arg("--").args(args);
    }
    let status = process
        .current_dir(resolved.root.as_std_path())
        .status()
        .with_context(|| {
            format!(
                "failed to run task {script:?} through Vite Task; install Vite+ and make `vp` available"
            )
        })?;
    if !status.success() {
        bail!("Vite Task task {script:?} exited with {status}");
    }
    Ok(())
}

/// `uf exec PACKAGE [ARGS...]`, also spelled `ufx`.
///
/// Four paths, tried in order, and every one of them either runs something or
/// fails:
///
///  1. a package uf implements itself, run in this process;
///  2. a binary the project has already installed, in `node_modules/.bin`;
///  3. a path the caller gave, executed as written;
///  4. a package that is not installed — fetched and run through the detected
///     package manager, but only when the caller said `--yes`.
///
/// # What this used to do
///
/// It wrote `.uf/exec-cache/<package>.json`, printed "cached execution request
/// for registry resolution", and exited 0 having run nothing. Nothing ever
/// read that file back, so the directory was named a cache and cached nothing;
/// it existed so that a command with nothing to do had something to write. A
/// CI job whose step was `ufx some-codegen` went green having generated
/// nothing. See ubugeeei-prod/uf#274.
///
/// # Why fetching asks first
///
/// Fetching a package that is not in the lockfile and running its binary is
/// the most dangerous thing a package manager does, and uf has already taken a
/// position one step later: `pm.allowLifecycleScripts` is false by default and
/// `uf install` passes `--ignore-scripts` through to whichever manager runs.
/// Downloading and executing an unpinned name silently would be the same hole
/// one step earlier, so it needs `--yes` — the flag `npx` spells the same way,
/// for the same reason. `docs/security.md` records the decision.
///
/// A binary in `node_modules/.bin` needs no such consent: it is already
/// installed, already in the tree the lockfile pins, and running it is what
/// `npm exec` and `pnpm exec` do without asking.
pub(crate) fn exec_package(
    cwd: &Utf8Path,
    ui: &mut Ui,
    package: &str,
    args: &[String],
    yes: bool,
) -> Result<()> {
    let resolved = load_config(cwd)?;

    // uf's own packages first, and they are the only path that renders
    // anything: every other one hands stdout to a child process, and a banner
    // printed above somebody else's output is uf writing on a report it did
    // not produce. `uf run` has held that line since it was written.
    if exec_uniflowed_virtual_package(cwd, ui, package, args)? {
        return Ok(());
    }

    if let Some(binary) = installed_binary(&resolved.root, package) {
        return spawn_executable(&resolved.root, ui, &binary, args, package);
    }

    // A path, executed as written. `ufx ./scripts/codegen.js` is a thing
    // people do, and it is not a package name.
    let candidate = Utf8PathBuf::from(package);
    let executable = if candidate.is_absolute() {
        candidate
    } else {
        resolved.root.join(candidate)
    };
    if executable.is_file() {
        return spawn_executable(&resolved.root, ui, &executable, args, package);
    }

    let detection = detect_package_manager(&resolved.root);
    let manager = fetchable(detection.package_manager);
    let mut invocation = command_for(manager, Operation::DlxExec);
    invocation.args.push(Cow::Owned(package.to_owned()));
    invocation
        .args
        .extend(args.iter().map(|arg| Cow::Owned(arg.clone())));

    if !yes {
        // Naming the exact command it would otherwise run, so the decision is
        // made by reading rather than by trusting. `uf add` is not a command
        // uf has, so the hint says what a person can actually type.
        bail!(
            "{package} is not installed in this project, and fetching it would run code \
             {lockfile} does not pin.\n\
             Declare it in package.json and run `uf install`, or say so explicitly:\n\
             `uf exec --yes {package}`, which runs `{invocation}`.",
            lockfile = resolved.config.pm.lockfile,
        );
    }

    // On stderr, so the fetched binary still owns stdout. Printed rather than
    // silent because "uf downloaded and ran something" is not a thing a person
    // should have to infer from a network light.
    let announcement = format!("fetching and running {package} with `{invocation}`");
    ui.render_err(|renderer, out| {
        renderer.status(out, Status::Info, &announcement);
    });

    let status = ProcessCommand::new(invocation.program)
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(resolved.root.as_std_path())
        .status()
        .with_context(|| format!("failed to run `{invocation}`"))?;
    if !status.success() {
        adopt_exit_status(ui, status, package);
    }
    Ok(())
}

/// The project's installed binary for `package`, when there is one.
///
/// `node_modules/.bin` is where every package manager links a dependency's
/// executables, so this is the same lookup `npm exec` does before it considers
/// fetching anything. A scoped name is linked under its bare binary name —
/// `@scope/thing` installs `thing` — which is why the last segment is what is
/// looked up.
fn installed_binary(root: &Utf8Path, package: &str) -> Option<Utf8PathBuf> {
    let name = package.rsplit('/').next().unwrap_or(package);
    // Nothing good comes of joining a caller's string onto a path when it can
    // climb out of it, and `uf exec ../../evil` must not become a lookup in
    // somebody else's `node_modules`.
    if name.is_empty() || name.contains(std::path::is_separator) || name.starts_with('.') {
        return None;
    }
    let binary = root.join("node_modules/.bin").join(name);
    binary.is_file().then_some(binary)
}

/// Run one executable, forwarding its arguments and its exit status.
fn spawn_executable(
    root: &Utf8Path,
    ui: &mut Ui,
    executable: &Utf8Path,
    args: &[String],
    package: &str,
) -> Result<()> {
    let status = ProcessCommand::new(executable.as_std_path())
        .args(args)
        .current_dir(root.as_std_path())
        .status()
        .with_context(|| format!("failed to execute {executable}"))?;
    if !status.success() {
        adopt_exit_status(ui, status, package);
    }
    Ok(())
}

/// Report a child's failure and leave with the child's own exit code.
///
/// Not a `bail!`. An error out of `run` becomes [`ExitCode::FAILURE`], which is
/// `1`, so every code a child could exit with — `jest`'s, `eslint`'s, a
/// codegen script's `42` — arrived at the caller as the same number, and a
/// script branching on it could tell nothing apart. `docs/app/reference/cli`
/// has said "exiting with its status" since this command started running
/// anything at all, and #353 said "exit status adopted"; neither was true of
/// any of the three paths that spawn.
///
/// `std::process::exit`, because there is nowhere else to put a number:
/// `run` answers `Result<()>` for every command and `main` turns that into one
/// of two codes. `uf env exec` reached the same conclusion and makes the same
/// call, and its comment says the rest. Nothing is buffered past this point —
/// `Ui` writes and flushes inside `render_err` — so there is nothing for the
/// skipped destructors to lose.
///
/// A child killed by a signal has no code of its own, and answers `1` here.
/// That loses which signal it was, and matching `uf env exec` is worth more
/// than fixing it in one of the two commands: a sibling pair that disagreed
/// about the same event would be the harder thing to reason about.
fn adopt_exit_status(ui: &mut Ui, status: std::process::ExitStatus, package: &str) -> ! {
    // Said before leaving, because the number alone does not say whose it is:
    // a reader looking at `42` should not have to guess whether uf failed or
    // the thing uf ran did.
    ui.error(&anyhow::anyhow!("{package} exited with {status}"));
    std::process::exit(status.code().unwrap_or(1));
}

/// The manager that can fetch and run a package that is not installed.
///
/// [`PackageManager::Uf`] is what detection reports both when a project pins
/// uf and when it shows no evidence of any manager, and uf's own resolver
/// cannot fetch yet — the same reason [`uf_pm::run_install`] substitutes npm,
/// and the same substitution. `npx` comes with Node.js, which a uf project
/// needs anyway.
pub(crate) fn fetchable(manager: PackageManager) -> PackageManager {
    match manager {
        PackageManager::Uf => PackageManager::Npm,
        other => other,
    }
}

/// Packages `uf` implements itself, rather than fetching from a registry.
///
/// The banner is here rather than in [`exec_package`] because this is the only
/// path where uf is the thing that runs: every other one spawns a process and
/// gives it stdout, and a heading printed above another program's output is uf
/// signing a report it did not write.
fn exec_uniflowed_virtual_package(
    cwd: &Utf8Path,
    ui: &mut Ui,
    package: &str,
    args: &[String],
) -> Result<bool> {
    if !matches!(
        package,
        "@uniflowed/create"
            | "uf/create"
            | "@uniflowed/test"
            | "uf/test"
            | "@uniflowed/pm"
            | "uf/pm"
    ) {
        return Ok(false);
    }

    ui.render(|renderer, out| {
        renderer.banner(out, "ufx", Some(package));
        renderer.blank(out);
    });

    match package {
        "@uniflowed/create" | "uf/create" => {
            let Some(kind) = args.first().map(String::as_str) else {
                bail!("ufx {package} requires app or lib");
            };
            match kind {
                "app" => {
                    // The same two positionals `uf create app` takes, handed
                    // over unresolved: `create::app_arguments` decides which
                    // of them is a template, so the two front doors cannot
                    // read the same command differently. They did — this one
                    // already accepted `ufx @uniflowed/create app my-site`
                    // while `uf create app my-site` was rejected (#322).
                    create::create(
                        cwd,
                        ui,
                        CreateCommand::App {
                            template_or_path: args.get(1).cloned(),
                            path: args.get(2).map(Utf8PathBuf::from),
                            name: None,
                            force: args.iter().any(|arg| arg == "--force"),
                        },
                    )?;
                }
                "lib" => {
                    create::create(
                        cwd,
                        ui,
                        CreateCommand::Lib {
                            path: args.get(1).map(Utf8PathBuf::from),
                            name: None,
                            force: args.iter().any(|arg| arg == "--force"),
                        },
                    )?;
                }
                other => bail!("unknown @uniflowed/create target {other:?}"),
            }
            Ok(true)
        }
        "@uniflowed/test" | "uf/test" => {
            test::test(
                cwd,
                ui,
                test::TestArgs {
                    list: args.iter().any(|arg| arg == "--list"),
                    watch: args.iter().any(|arg| arg == "--watch"),
                    ..test::TestArgs::default()
                },
            )?;
            Ok(true)
        }
        "@uniflowed/pm" | "uf/pm" => {
            pm::install(cwd, ui)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}
