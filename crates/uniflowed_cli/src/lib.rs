use std::fs;
use std::process::{Command as ProcessCommand, ExitCode};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use uniflowed_config::{ResolvedConfig, load_config};
use uniflowed_fmt::format_source;
use uniflowed_lib::{builtin_modules, hook_descriptors, ui_components};
use uniflowed_lint::{Severity, SourceFile, lint_sources};
use uniflowed_project::{CreateKind, CreateOptions, collect_source_files, create_project};
use uniflowed_router::{discover_routes, write_router_manifest};
use uniflowed_test::{discover_tests, merge_plans};

#[derive(Debug, Parser)]
#[command(name = "uf", version, about = "Unified Toolchain for Flow (React)")]
struct Cli {
    #[arg(long, global = true, value_name = "DIR")]
    cwd: Option<Utf8PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Build,
    Check,
    Create {
        #[command(subcommand)]
        command: CreateCommand,
    },
    Dev,
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
    Fmt {
        #[arg(long)]
        check: bool,
    },
    Inspect {
        #[arg(long)]
        json: bool,
    },
    Install,
    Lint,
    Lsp,
    Publish,
    Run {
        script: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Test {
        #[arg(long)]
        list: bool,
    },
    Upgrade,
}

#[derive(Debug, Subcommand)]
enum CreateCommand {
    App {
        #[arg(value_enum, default_value = "react")]
        template: AppTemplate,
        path: Option<Utf8PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        force: bool,
    },
    Lib {
        path: Option<Utf8PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AppTemplate {
    React,
}

#[derive(Debug, Subcommand)]
enum EnvCommand {
    Doctor,
    Use { name: String },
}

pub fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let cwd = resolve_cwd(cli.cwd)?;

    match cli.command {
        Commands::Build => build(&cwd),
        Commands::Check => check(&cwd),
        Commands::Create { command } => create(&cwd, command),
        Commands::Dev => dev(&cwd),
        Commands::Env { command } => env(&cwd, command),
        Commands::Fmt { check } => fmt(&cwd, check),
        Commands::Inspect { json } => inspect(&cwd, json),
        Commands::Install => install(&cwd),
        Commands::Lint => lint(&cwd),
        Commands::Lsp => lsp(&cwd),
        Commands::Publish => publish(&cwd),
        Commands::Run { script, args } => run_task(&cwd, &script, &args),
        Commands::Test { list } => test(&cwd, list),
        Commands::Upgrade => upgrade(&cwd),
    }
}

fn resolve_cwd(cwd: Option<Utf8PathBuf>) -> Result<Utf8PathBuf> {
    match cwd {
        Some(path) if path.is_absolute() => Ok(path),
        Some(path) => Ok(current_dir()?.join(path)),
        None => current_dir(),
    }
}

fn current_dir() -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(std::env::current_dir()?)
        .map_err(|path| anyhow!("current directory is not UTF-8: {}", path.display()))
}

fn build(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let manifest = write_router_manifest(&resolved.root, &resolved.config)?;
    println!(
        "uf build: entries={} outDir={} sourcemap={} backend=vite-compatible/rolldown-planned",
        resolved
            .config
            .build
            .entries
            .iter()
            .map(|entry| entry.as_str())
            .collect::<Vec<_>>()
            .join(","),
        resolved.config.build.out_dir,
        resolved.config.build.sourcemap
    );
    if let Some(manifest) = manifest {
        println!("generated {}", manifest);
    }
    Ok(())
}

fn check(cwd: &Utf8Path) -> Result<()> {
    let report = run_lint(cwd)?;
    print_lint_report(&report);
    if report.has_errors() {
        bail!(
            "check failed with {} diagnostic(s)",
            report.diagnostics.len()
        );
    }
    println!("uf check: Flow syntax and framework rules passed");
    Ok(())
}

fn create(cwd: &Utf8Path, command: CreateCommand) -> Result<()> {
    let (kind, target, name, force) = match command {
        CreateCommand::App {
            template: AppTemplate::React,
            path,
            name,
            force,
        } => {
            let target = resolve_target(cwd, path)?;
            let name = name.unwrap_or_else(|| project_name(&target, "uniflowed-app"));
            (CreateKind::AppReact, target, name, force)
        }
        CreateCommand::Lib { path, name, force } => {
            let target = resolve_target(cwd, path)?;
            let name = name.unwrap_or_else(|| project_name(&target, "uniflowed-lib"));
            (CreateKind::Lib, target, name, force)
        }
    };

    let report = create_project(&target, &CreateOptions { name, kind, force })?;
    println!("created {} file(s) in {}", report.files.len(), report.root);
    Ok(())
}

fn resolve_target(cwd: &Utf8Path, path: Option<Utf8PathBuf>) -> Result<Utf8PathBuf> {
    Ok(match path {
        Some(path) if path.is_absolute() => path,
        Some(path) => cwd.join(path),
        None => cwd.to_path_buf(),
    })
}

fn project_name(path: &Utf8Path, fallback: &str) -> String {
    path.file_name()
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn dev(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    println!(
        "uf dev: http://{}:{} backend=vite-compatible-dev-server-planned",
        resolved.config.dev.host, resolved.config.dev.port
    );
    Ok(())
}

fn env(cwd: &Utf8Path, command: EnvCommand) -> Result<()> {
    match command {
        EnvCommand::Doctor => env_doctor(cwd),
        EnvCommand::Use { name } => {
            let dir = cwd.join(".uniflowed");
            fs::create_dir_all(&dir).with_context(|| format!("failed to create {dir}"))?;
            fs::write(dir.join("env"), format!("{name}\n"))
                .with_context(|| "failed to write .uniflowed/env")?;
            println!("uf env: active={name}");
            Ok(())
        }
    }
}

fn env_doctor(cwd: &Utf8Path) -> Result<()> {
    println!("uf env doctor: {}", cwd);
    for (name, arg) in [
        ("rustc", "--version"),
        ("cargo", "--version"),
        ("nix", "--version"),
        ("git", "--version"),
        ("bun", "--version"),
    ] {
        match command_output(name, arg) {
            Ok(output) => println!("ok {name}: {output}"),
            Err(error) => println!("missing {name}: {error}"),
        }
    }
    Ok(())
}

fn command_output(bin: &str, arg: &str) -> Result<String> {
    let output = ProcessCommand::new(bin).arg(arg).output()?;
    if !output.status.success() {
        bail!("{bin} exited with {}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().next().unwrap_or("").to_string())
}

fn fmt(cwd: &Utf8Path, check: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let files = collect_source_files(&resolved.root, &resolved.config)?;
    let mut changed = Vec::new();

    for file in files {
        let result = format_source(&file.source, &resolved.config.fmt)?;
        if result.changed {
            if check {
                changed.push(file.relative_path);
            } else {
                fs::write(&file.absolute_path, result.output)
                    .with_context(|| format!("failed to write {}", file.absolute_path))?;
                changed.push(file.relative_path);
            }
        }
    }

    if check && !changed.is_empty() {
        for path in &changed {
            println!("would format {path}");
        }
        bail!("{} file(s) need formatting", changed.len());
    }

    println!(
        "uf fmt: {} file(s) {}",
        changed.len(),
        if check { "checked" } else { "formatted" }
    );
    Ok(())
}

fn inspect(cwd: &Utf8Path, as_json: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&inspect_payload(&resolved)?)?
        );
    } else {
        println!("uf inspect");
        println!("root: {}", resolved.root);
        println!(
            "config: {}",
            resolved
                .config_path
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "zero-config defaults".to_string())
        );
        println!("command: uf");
        println!(
            "app: router={} rsc={} serverActions={} style={:?} reactCompilerMode={:?}",
            resolved.config.app.router.root,
            resolved.config.app.rsc,
            resolved.config.app.server_actions,
            resolved.config.app.builtins.style,
            resolved.config.app.builtins.react_compiler.mode
        );
        println!("native modules: {}", builtin_modules().len());
        println!("ui components: {}", ui_components().len());
        println!("hooks: {}", hook_descriptors().len());
    }
    Ok(())
}

fn inspect_payload(resolved: &ResolvedConfig) -> Result<serde_json::Value> {
    let routes = discover_routes(&resolved.root, &resolved.config)?
        .into_iter()
        .map(|route| {
            json!({
                "path": route.path,
                "page": route.page,
                "params": route.params.into_iter().map(|param| {
                    json!({
                        "name": param.name,
                        "kind": format!("{:?}", param.kind),
                    })
                }).collect::<Vec<_>>(),
                "hasLayout": route.has_layout,
                "hasMiddleware": route.has_middleware,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "command": "uf",
        "config": resolved,
        "routes": routes,
        "nativeModules": builtin_modules(),
        "hooks": hook_descriptors(),
        "ui": ui_components(),
        "engines": {
            "parser": "official-flow-parser",
            "build": "vite-compatible/rolldown",
            "devServer": "vite-compatible",
            "runtime": "hermes-planned",
            "reactCompiler": {
                "enabled": resolved.config.app.builtins.react_compiler.enabled,
                "mode": resolved.config.app.builtins.react_compiler.mode,
            }
        }
    }))
}

fn install(_cwd: &Utf8Path) -> Result<()> {
    println!("uf install: native package manager solver is planned; no changes made");
    Ok(())
}

fn lint(cwd: &Utf8Path) -> Result<()> {
    let report = run_lint(cwd)?;
    print_lint_report(&report);
    if report.has_errors() {
        bail!(
            "lint failed with {} diagnostic(s)",
            report.diagnostics.len()
        );
    }
    Ok(())
}

fn run_lint(cwd: &Utf8Path) -> Result<uniflowed_lint::LintReport> {
    let resolved = load_config(cwd)?;
    let files = collect_source_files(&resolved.root, &resolved.config)?;
    let sources = files
        .into_iter()
        .map(|file| SourceFile {
            path: file.relative_path,
            source: file.source,
        })
        .collect::<Vec<_>>();
    lint_sources(&sources, &resolved.config).map_err(Into::into)
}

fn print_lint_report(report: &uniflowed_lint::LintReport) {
    for diagnostic in &report.diagnostics {
        let severity = match diagnostic.severity {
            Severity::Warn => "warning",
            Severity::Error => "error",
        };
        println!(
            "{}:{}:{}: {severity}[{}] {}",
            diagnostic.path.as_deref().unwrap_or("<memory>"),
            diagnostic.line,
            diagnostic.column,
            diagnostic.rule,
            diagnostic.message
        );
    }
    println!(
        "uf lint: checked {} file(s), {} diagnostic(s)",
        report.files_checked,
        report.diagnostics.len()
    );
}

fn lsp(_cwd: &Utf8Path) -> Result<()> {
    println!("uf lsp: parser/config server boundary is ready; JSON-RPC loop is planned");
    Ok(())
}

fn publish(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    println!(
        "uf publish: registry={} dryRun={} native publisher planned",
        resolved.config.publish.registry, resolved.config.publish.dry_run
    );
    Ok(())
}

fn run_task(cwd: &Utf8Path, script: &str, args: &[String]) -> Result<()> {
    let resolved = load_config(cwd)?;
    let Some(task) = resolved.config.tasks.get(script) else {
        bail!("task {script:?} is not defined in uniflowed.config.flow");
    };
    let command = if args.is_empty() {
        task.command().to_string()
    } else {
        format!("{} {}", task.command(), args.join(" "))
    };

    let status = ProcessCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&resolved.root)
        .status()
        .with_context(|| format!("failed to run task {script:?}"))?;
    if !status.success() {
        bail!("task {script:?} exited with {status}");
    }
    Ok(())
}

fn test(cwd: &Utf8Path, list: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let files = collect_source_files(&resolved.root, &resolved.config)?;
    let plan = merge_plans(
        files
            .into_iter()
            .map(|file| discover_tests(&file.relative_path, &file.source)),
    );

    for case in &plan.cases {
        println!("{}:{}:{} {}", case.file, case.line, case.column, case.name);
    }

    if list {
        println!(
            "uf test: discovered {} runnable test(s)",
            plan.runnable_count()
        );
        return Ok(());
    }

    bail!(
        "native JavaScript execution backend is not enabled yet; use `uf test --list` for discovery"
    )
}

fn upgrade(_cwd: &Utf8Path) -> Result<()> {
    println!("uf upgrade: native dependency upgrader is planned; no changes made");
    Ok(())
}
