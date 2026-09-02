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
use uf_term::{KeyValue, Status, Tone};

use crate::cli::{AppTemplate, CreateCommand};
use crate::commands::{create, pm, test};
use crate::support::{safe_file_label, write_json_file};
use crate::ui::Ui;

pub(crate) fn run_task(cwd: &Utf8Path, script: &str, args: &[String]) -> Result<()> {
    let resolved = load_config(cwd)?;
    let mut visited = BTreeSet::new();
    run_named_task(&resolved, script, args, &mut visited)
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
        bail!("task {script:?} is not defined in uf.config.js");
    };

    if let TaskDefinition::Detailed(details) = task {
        for dependency in &details.depends_on {
            run_named_task(resolved, dependency.as_str(), &[], visited)?;
        }
    }

    execute_task(resolved, script, task, args)
}

fn execute_task(
    resolved: &ResolvedConfig,
    script: &str,
    task: &TaskDefinition,
    args: &[String],
) -> Result<()> {
    if resolved.config.task_runner.engine == TaskRunnerEngine::ViteTask {
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
