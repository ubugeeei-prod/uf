//! `uf run` and `ufx`: the two commands that hand control to another process.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::{ResolvedConfig, TaskDefinition, TaskRunnerEngine, load_config};
use uf_pm::PackageManagerPlan;
use uf_term::{Cell, Column, KeyValue, Status, Table, Tone, display_width, truncate_to_width};

use crate::cli::{AppTemplate, CreateCommand};
use crate::commands::{create, pm, test};
use crate::suggest::closest;
use crate::support::{plural, project_label, safe_file_label, write_json_file};
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

pub(crate) fn exec_package(
    cwd: &Utf8Path,
    ui: &mut Ui,
    package: &str,
    args: &[String],
) -> Result<()> {
    let resolved = load_config(cwd)?;
    let package_manager = PackageManagerPlan::infer_from_config(&resolved.config);
    let cache_dir = resolved.root.join(".uf/exec-cache");
    fs::create_dir_all(&cache_dir).with_context(|| format!("failed to create {cache_dir}"))?;
    let manifest = cache_dir.join(format!("{}.json", safe_file_label(package)));
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "package": package,
            "args": args,
            "resolver": package_manager.resolver,
            "lockfile": package_manager.lockfile.as_str(),
        }),
    )?;

    let resolver = format!("{:?}", package_manager.resolver);
    let lockfile = package_manager.lockfile.to_string();
    let manifest_path = manifest.to_string();
    let argument_count = args.len().to_string();
    ui.render(|renderer, out| {
        renderer.banner(out, "ufx", Some(package));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("resolver", &resolver),
                KeyValue::toned("lockfile", &lockfile, Tone::Path),
                KeyValue::toned("arguments", &argument_count, Tone::Number),
                KeyValue::toned("manifest", &manifest_path, Tone::Path),
            ],
        );
        renderer.blank(out);
    });

    if exec_uniflowed_virtual_package(cwd, ui, package, args)? {
        return Ok(());
    }

    let candidate = Utf8PathBuf::from(package);
    let executable = if candidate.is_absolute() {
        candidate
    } else {
        resolved.root.join(candidate)
    };
    if executable.exists() {
        let status = ProcessCommand::new(executable.as_std_path())
            .args(args)
            .current_dir(resolved.root.as_std_path())
            .status()
            .with_context(|| format!("failed to execute {executable}"))?;
        if !status.success() {
            bail!("{package} exited with {status}");
        }
        return Ok(());
    }

    ui.render(|renderer, out| {
        renderer.status(
            out,
            Status::Info,
            "cached execution request for registry resolution",
        );
    });
    Ok(())
}

/// Packages `uf` implements itself, rather than fetching from a registry.
fn exec_uniflowed_virtual_package(
    cwd: &Utf8Path,
    ui: &mut Ui,
    package: &str,
    args: &[String],
) -> Result<bool> {
    match package {
        "@uniflowed/create" | "uf/create" => {
            let Some(kind) = args.first().map(String::as_str) else {
                bail!("ufx {package} requires app or lib");
            };
            match kind {
                "app" => {
                    let mut cursor = 1;
                    let template = if args.get(cursor).map(String::as_str) == Some("react") {
                        cursor += 1;
                        AppTemplate::React
                    } else {
                        AppTemplate::React
                    };
                    let path = args.get(cursor).map(Utf8PathBuf::from);
                    create::create(
                        cwd,
                        ui,
                        CreateCommand::App {
                            template,
                            path,
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
