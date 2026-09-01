use std::collections::BTreeSet;
use std::fs;
use std::process::{Command as ProcessCommand, ExitCode};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use uf_config::{ResolvedConfig, TaskDefinition, load_config};
use uf_fmt::format_source;
use uf_lib::{
    builtin_modules, hook_descriptors, motion_contract, orm_contract, std_module_descriptors,
    tui_contract, ui_components, vrt_plan,
};
use uf_lint::{Severity, SourceFile, lint_sources};
use uf_pm::PackageManagerPlan;
use uf_prepare::default_plan;
use uf_project::{CreateKind, CreateOptions, collect_source_files, create_project};
use uf_rm::{RuntimeManagerPlan, RuntimeReference, RuntimeUsePlan, XdgEnv, XdgLayout};
use uf_router::{discover_routes, write_router_manifest};
use uf_runtime::RuntimeContract;
use uf_test::{NativeTestRunnerPlan, discover_tests, merge_plans};

#[derive(Debug, Parser)]
#[command(version, about = "Unified Toolchain for Flow (React)")]
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
    Exec {
        package: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
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
    Prepare,
    Publish,
    Release {
        #[arg(value_enum)]
        bump: ReleaseBump,
    },
    Run {
        script: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Test {
        #[arg(long)]
        list: bool,
    },
    Use {
        runtime: String,
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReleaseBump {
    Patch,
    Minor,
    Major,
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
            if let Some(error) = error.downcast_ref::<clap::Error>() {
                let _ = error.print();
                return if matches!(
                    error.kind(),
                    ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
                ) {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                };
            }
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = parse_cli()?;
    let cwd = resolve_cwd(cli.cwd)?;

    match cli.command {
        Commands::Build => build(&cwd),
        Commands::Check => check(&cwd),
        Commands::Create { command } => create(&cwd, command),
        Commands::Dev => dev(&cwd),
        Commands::Env { command } => env(&cwd, command),
        Commands::Exec { package, args } => exec_package(&cwd, &package, &args),
        Commands::Fmt { check } => fmt(&cwd, check),
        Commands::Inspect { json } => inspect(&cwd, json),
        Commands::Install => install(&cwd),
        Commands::Lint => lint(&cwd),
        Commands::Lsp => lsp(&cwd),
        Commands::Prepare => prepare(&cwd),
        Commands::Publish => publish(&cwd),
        Commands::Release { bump } => release(&cwd, bump),
        Commands::Run { script, args } => run_task(&cwd, &script, &args),
        Commands::Test { list } => test(&cwd, list),
        Commands::Use { runtime } => use_runtime(&cwd, &runtime),
        Commands::Upgrade => upgrade(&cwd),
    }
}

fn parse_cli() -> Result<Cli> {
    let mut args = std::env::args_os().collect::<Vec<_>>();
    let bin_name = args
        .first()
        .and_then(|arg| std::path::Path::new(arg).file_stem())
        .and_then(|stem| stem.to_str())
        .unwrap_or("uf");

    match bin_name {
        "ufr" => {
            args.insert(1, "run".into());
            Cli::try_parse_from(args).map_err(Into::into)
        }
        "ufx" => {
            args.insert(1, "exec".into());
            Cli::try_parse_from(args).map_err(Into::into)
        }
        _ => Cli::try_parse_from(args).map_err(Into::into),
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

fn exec_package(cwd: &Utf8Path, package: &str, args: &[String]) -> Result<()> {
    let resolved = load_config(cwd)?;
    let package_manager = PackageManagerPlan::infer_from_config(&resolved.config);
    println!(
        "ufx: package={} resolver={:?} lockfile={} args={}",
        package,
        package_manager.resolver,
        package_manager.lockfile,
        args.len()
    );
    println!("ufx: native temporary package execution is planned; no changes made");
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
        println!(
            "server: engine={:?} adapters={}",
            resolved.config.server.engine,
            resolved.config.server.native.adapters.len()
        );
        println!(
            "task runner: {:?} packageScripts={}",
            resolved.config.task_runner.engine, resolved.config.task_runner.allow_package_scripts
        );
        println!(
            "test runner: {:?} target={:?}",
            resolved.config.test.runner.runtime, resolved.config.test.runner.performance_target
        );
        println!(
            "package manager: {:?} lockfile={}",
            resolved.config.pm.resolver, resolved.config.pm.lockfile
        );
        println!(
            "runtime manager: inferFromConfig={} module={}",
            resolved.config.rm.infer_from_config, resolved.config.rm.module
        );
        println!("native modules: {}", builtin_modules().len());
        println!("std modules: {}", std_module_descriptors().len());
        println!("ui components: {}", ui_components().len());
        println!("tui components: {}", tui_contract().components.len());
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
    let runtime = RuntimeContract::wintertc_hermes_native();
    let test_runner = NativeTestRunnerPlan::self_hosted();
    let package_manager = PackageManagerPlan::infer_from_config(&resolved.config);
    let runtime_manager = RuntimeManagerPlan::infer_from_config(&resolved.config);

    Ok(json!({
        "command": "uf",
        "config": resolved,
        "routes": routes,
        "nativeModules": builtin_modules(),
        "stdModules": std_module_descriptors(),
        "orm": orm_contract(),
        "motion": motion_contract(),
        "tui": tui_contract(),
        "vrt": vrt_plan(),
        "hooks": hook_descriptors(),
        "ui": ui_components(),
        "engines": {
            "parser": "official-flow-parser",
            "build": "vite-compatible/rolldown",
            "devServer": "vite-compatible",
            "runtime": "hermes-planned",
            "runtimeContract": runtime,
            "server": {
                "engine": resolved.config.server.engine,
                "streaming": resolved.config.server.native.streaming,
                "zeroCopyHttp": resolved.config.server.native.zero_copy_http,
                "adapters": &resolved.config.server.native.adapters,
            },
            "packageGenerator": {
                "engine": resolved.config.package.generator,
                "targets": &resolved.config.package.targets,
                "typescriptDeclarationsToFlow": resolved.config.package.typescript_declarations_to_flow,
            },
            "taskRunner": {
                "engine": resolved.config.task_runner.engine,
                "allowPackageScripts": resolved.config.task_runner.allow_package_scripts,
            },
            "testRunner": test_runner,
            "packageManager": package_manager,
            "runtimeManager": runtime_manager,
            "reactCompiler": {
                "enabled": resolved.config.app.builtins.react_compiler.enabled,
                "mode": resolved.config.app.builtins.react_compiler.mode,
            }
        }
    }))
}

fn use_runtime(cwd: &Utf8Path, runtime: &str) -> Result<()> {
    let resolved = load_config(cwd)?;
    let requested = RuntimeReference::parse(runtime)
        .ok_or_else(|| anyhow!("runtime must look like uf@0.1.0"))?;
    let plan = RuntimeUsePlan::new(
        requested,
        xdg_layout_from_process(),
        resolved.config.rm.auto_switch,
    );
    println!(
        "uf use: runtime={}@{} autoSwitch={} shim={}",
        plan.requested.name, plan.requested.version, plan.auto_switch, plan.layout.shim_path
    );
    for step in plan.steps {
        println!("runtime step: {step:?}");
    }
    Ok(())
}

fn xdg_layout_from_process() -> XdgLayout {
    let home = std::env::var("HOME").unwrap_or_else(|_| "$HOME".to_string());
    let config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let data_home = std::env::var("XDG_DATA_HOME").ok();
    let cache_home = std::env::var("XDG_CACHE_HOME").ok();
    let state_home = std::env::var("XDG_STATE_HOME").ok();
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();

    XdgLayout::from_env(XdgEnv {
        home: &home,
        config_home: config_home.as_deref(),
        data_home: data_home.as_deref(),
        cache_home: cache_home.as_deref(),
        state_home: state_home.as_deref(),
        runtime_dir: runtime_dir.as_deref(),
    })
}

fn install(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let plan = PackageManagerPlan::infer_from_config(&resolved.config);
    println!(
        "uf install: resolver={:?} lockfile={} store={} scripts={:?}",
        plan.resolver, plan.lockfile, plan.store.directory, plan.scripts
    );
    println!("uf install: native resolver/apply loop is planned; no changes made");
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

fn run_lint(cwd: &Utf8Path) -> Result<uf_lint::LintReport> {
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

fn print_lint_report(report: &uf_lint::LintReport) {
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
        "uf publish: registry={} dryRun={} firstPublish={:?} localBootstrap={} trustedProvider={:?} tokenless={} trigger={:?}",
        resolved.config.publish.registry,
        resolved.config.publish.dry_run,
        resolved.config.publish.first_publish.mode,
        resolved.config.publish.first_publish.local_bootstrap,
        resolved.config.publish.trusted_publish.provider,
        resolved.config.publish.trusted_publish.tokenless,
        resolved.config.publish.trusted_publish.trigger
    );
    Ok(())
}

fn release(cwd: &Utf8Path, bump: ReleaseBump) -> Result<()> {
    let resolved = load_config(cwd)?;
    println!(
        "uf release: bump={:?} tagPrefix={} command={} publish={} trustedTrigger={:?}",
        bump,
        resolved.config.release.tag_prefix,
        resolved.config.release.command,
        resolved.config.release.publish,
        resolved.config.publish.trusted_publish.trigger
    );
    println!("uf release: tag calculation and push are planned; no changes made");
    Ok(())
}

fn prepare(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let plan = default_plan();
    println!(
        "uf prepare: root={} lintStagedCompatible={} codeGenerator={} cache={:?}",
        resolved.root, plan.lint_staged_compatible, plan.code_generator, plan.cache
    );
    for step in plan.steps {
        println!("prepare step: {step:?}");
    }
    Ok(())
}

fn run_task(cwd: &Utf8Path, script: &str, args: &[String]) -> Result<()> {
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

    let status = process
        .status()
        .with_context(|| format!("failed to run task {script:?} through uf Vite Task runner"))?;
    if !status.success() {
        bail!("task {script:?} exited with {status}");
    }
    Ok(())
}

fn test(cwd: &Utf8Path, list: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let runner = NativeTestRunnerPlan::self_hosted();
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
            "uf test: discovered {} runnable test(s) runtime={:?} target={:?}",
            plan.runnable_count(),
            runner.runtime,
            runner.performance_target
        );
        return Ok(());
    }

    bail!(
        "native JavaScript execution backend is not enabled yet; use `uf test --list` for discovery"
    )
}

fn upgrade(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let package_manager = PackageManagerPlan::infer_from_config(&resolved.config);
    let runtime_manager = RuntimeManagerPlan::infer_from_config(&resolved.config);
    println!(
        "uf upgrade: packageResolver={:?} runtimeEngine={:?} acquisition={:?}",
        package_manager.resolver, runtime_manager.engine, runtime_manager.acquisition
    );
    println!("uf upgrade: native package/runtime upgrade loop is planned; no changes made");
    Ok(())
}
