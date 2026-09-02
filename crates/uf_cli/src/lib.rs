use std::collections::BTreeSet;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::net::{TcpListener, TcpStream};
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
use uf_pm::{PackageManagerPlan, install_workspace};
use uf_prepare::default_plan;
use uf_project::{CreateKind, CreateOptions, collect_source_files, create_project};
use uf_rm::{RuntimeManagerPlan, RuntimeReference, RuntimeUsePlan, XdgEnv, XdgLayout};
use uf_router::{discover_routes, write_router_manifest};
use uf_runtime::RuntimeContract;
use uf_test::{NativeTestRunnerPlan, discover_tests, merge_plans, run_tests};

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
    Dev {
        #[arg(long, hide = true)]
        once: bool,
    },
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
        Commands::Dev { once } => dev(&cwd, once),
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
    let routes = discover_routes(&resolved.root, &resolved.config)?;
    let manifest = write_router_manifest(&resolved.root, &resolved.config)?;
    let out_dir = resolved.root.join(resolved.config.build.out_dir.as_str());
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {out_dir}"))?;
    let build_manifest = out_dir.join("uf-build-manifest.json");
    let payload = json!({
        "version": 1,
        "engine": "uf-native",
        "bundlerCompatibility": ["vite", "rolldown"],
        "entries": resolved.config.build.entries,
        "routes": routes.iter().map(|route| json!({
            "path": route.path,
            "page": route.page,
            "layout": route.has_layout,
            "middleware": route.has_middleware,
            "params": route.params.iter().map(|param| json!({
                "name": param.name,
                "kind": format!("{:?}", param.kind),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "runtime": {
            "default": "uf",
            "wintertc": true,
            "hermes": true,
        },
        "cache": {
            "route": resolved.config.app.rendering.cache.route,
            "fetch": resolved.config.app.rendering.cache.fetch,
            "data": resolved.config.app.rendering.cache.data,
            "actions": resolved.config.app.rendering.cache.actions,
        },
    });
    write_json_file(&build_manifest, &payload)?;
    println!(
        "uf build: entries={} outDir={} sourcemap={} backend=uf-native/vite-compatible/rolldown-compatible manifest={}",
        resolved
            .config
            .build
            .entries
            .iter()
            .map(|entry| entry.as_str())
            .collect::<Vec<_>>()
            .join(","),
        resolved.config.build.out_dir,
        resolved.config.build.sourcemap,
        build_manifest
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

fn dev(cwd: &Utf8Path, once: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let listener = bind_dev_listener(
        resolved.config.dev.host.as_str(),
        resolved.config.dev.port,
        resolved.config.dev.strict_port,
    )?;
    let address = listener.local_addr()?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    write_json_file(
        &state_dir.join("dev-server.json"),
        &json!({
            "host": address.ip().to_string(),
            "port": address.port(),
            "engine": "uf-native",
            "viteCompatibility": true,
            "rolldownCompatibility": true,
            "health": "/__uf/health",
        }),
    )?;
    let _ = write_router_manifest(&resolved.root, &resolved.config)?;
    println!(
        "uf dev: http://{}:{} backend=uf-native/vite-compatible-dev-server",
        address.ip(),
        address.port()
    );

    if once {
        return Ok(());
    }

    for stream in listener.incoming() {
        let stream = stream.with_context(|| "failed to accept dev server connection")?;
        serve_dev_request(stream)?;
    }
    Ok(())
}

fn bind_dev_listener(host: &str, port: u16, strict_port: bool) -> Result<TcpListener> {
    match TcpListener::bind((host, port)) {
        Ok(listener) => Ok(listener),
        Err(_) if !strict_port => TcpListener::bind((host, 0)).with_context(|| {
            format!("failed to bind requested port {host}:{port} and fallback port")
        }),
        Err(error) => Err(error).with_context(|| format!("failed to bind {host}:{port}")),
    }
}

fn serve_dev_request(mut stream: TcpStream) -> Result<()> {
    let mut buffer = [0u8; 2048];
    let bytes = stream
        .read(&mut buffer)
        .with_context(|| "failed to read dev server request")?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (content_type, body) = if path == "/__uf/health" {
        (
            "application/json",
            r#"{"status":"ok","engine":"uf-native"}"#,
        )
    } else {
        ("text/plain; charset=utf-8", "uf dev server\n")
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .with_context(|| "failed to write dev server response")
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
    println!(
        "ufx: package={} resolver={:?} lockfile={} args={} manifest={}",
        package,
        package_manager.resolver,
        package_manager.lockfile,
        args.len(),
        manifest
    );
    if exec_uniflowed_virtual_package(cwd, package, args)? {
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

    println!("ufx: cached execution request for registry resolution");
    Ok(())
}

fn exec_uniflowed_virtual_package(cwd: &Utf8Path, package: &str, args: &[String]) -> Result<bool> {
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
                    create(
                        cwd,
                        CreateCommand::App {
                            template,
                            path,
                            name: None,
                            force: args.iter().any(|arg| arg == "--force"),
                        },
                    )?;
                }
                "lib" => {
                    create(
                        cwd,
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
            test(cwd, args.iter().any(|arg| arg == "--list"))?;
            Ok(true)
        }
        "@uniflowed/pm" | "uf/pm" => {
            install(cwd)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn safe_file_label(value: &str) -> String {
    let mut output = String::with_capacity(value.len().max(1));
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
        } else {
            output.push('_');
        }
    }
    if output.is_empty() {
        output.push('_');
    }
    output
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
        println!(
            "lint rules: {} ({} need the type checker)",
            uf_lint::rules().len(),
            uf_lint::rules()
                .iter()
                .filter(|descriptor| !descriptor.requirement.is_available())
                .count()
        );
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
        "lintRules": uf_lint::rules(),
        "ui": ui_components(),
        "engines": {
            "parser": "official-flow-parser",
            "build": "vite-compatible/rolldown",
            "devServer": "vite-compatible",
            "runtime": "hermes-wintertc-native-contract",
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
    let report = apply_runtime_use_plan(&plan)?;
    println!(
        "uf use: runtime={}@{} autoSwitch={} shim={} state={} manifest={} binary={}",
        plan.requested.name,
        plan.requested.version,
        plan.auto_switch,
        report.shim,
        report.active_runtime,
        report.runtime_manifest,
        report.runtime_binary
    );
    for step in &plan.steps {
        println!("runtime step: {step:?}");
    }
    Ok(())
}

#[derive(Debug)]
struct RuntimeUseApplyReport {
    active_runtime: Utf8PathBuf,
    runtime_manifest: Utf8PathBuf,
    runtime_binary: Utf8PathBuf,
    shim: Utf8PathBuf,
}

fn apply_runtime_use_plan(plan: &RuntimeUsePlan) -> Result<RuntimeUseApplyReport> {
    let version_dir = Utf8PathBuf::from(plan.layout.versions_dir.as_str())
        .join(plan.requested.name.as_str())
        .join(plan.requested.version.as_str());
    let runtime_bin_dir = version_dir.join("bin");
    fs::create_dir_all(&runtime_bin_dir)
        .with_context(|| format!("failed to create {runtime_bin_dir}"))?;

    let runtime_binary = runtime_bin_dir.join(if cfg!(windows) { "uf.exe" } else { "uf" });
    let current_exe = std::env::current_exe().with_context(|| "failed to locate current uf")?;
    fs::copy(&current_exe, runtime_binary.as_std_path()).with_context(|| {
        format!(
            "failed to install runtime binary from {} to {runtime_binary}",
            current_exe.display()
        )
    })?;
    mark_executable(&runtime_binary)?;

    let runtime_manifest = version_dir.join("runtime.json");
    write_json_file(
        &runtime_manifest,
        &json!({
            "name": plan.requested.name.as_str(),
            "version": plan.requested.version.as_str(),
            "binary": runtime_binary.as_str(),
            "source": "current-exe",
            "autoSwitch": plan.auto_switch,
        }),
    )?;

    let state_dir = Utf8PathBuf::from(plan.layout.state_dir.as_str());
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let active_runtime = state_dir.join("active-runtime.json");
    write_json_file(
        &active_runtime,
        &json!({
            "name": plan.requested.name.as_str(),
            "version": plan.requested.version.as_str(),
            "manifest": runtime_manifest.as_str(),
            "binary": runtime_binary.as_str(),
        }),
    )?;

    let shim = Utf8PathBuf::from(plan.layout.shim_path.as_str());
    if let Some(parent) = shim.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {parent}"))?;
    }
    write_runtime_shim(&shim, &runtime_binary)?;
    mark_executable(&shim)?;

    Ok(RuntimeUseApplyReport {
        active_runtime,
        runtime_manifest,
        runtime_binary,
        shim,
    })
}

fn write_runtime_shim(shim: &Utf8Path, runtime_binary: &Utf8Path) -> Result<()> {
    #[cfg(windows)]
    let contents = format!("@echo off\r\n\"{}\" %*\r\n", runtime_binary);
    #[cfg(not(windows))]
    let contents = format!(
        "#!/usr/bin/env sh\nset -eu\nexec {} \"$@\"\n",
        shell_quote(runtime_binary)
    );

    fs::write(shim, contents).with_context(|| format!("failed to write runtime shim {shim}"))
}

#[cfg(unix)]
fn mark_executable(path: &Utf8Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to read permissions for {path}"))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to update permissions for {path}"))
}

#[cfg(not(unix))]
fn mark_executable(_path: &Utf8Path) -> Result<()> {
    Ok(())
}

fn shell_quote(path: &Utf8Path) -> String {
    format!("'{}'", path.as_str().replace('\'', "'\\''"))
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
    let report = install_workspace(&resolved.root, &resolved.config)?;
    println!(
        "uf install: resolver={:?} lockfile={} store={} scripts={:?} packages={} storeEntries={}",
        plan.resolver,
        report.lockfile,
        report.store_manifest,
        plan.scripts,
        report.packages.len(),
        report.store_entries.len()
    );
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
    if !report.unavailable.is_empty() {
        let names = report
            .unavailable
            .iter()
            .map(|unavailable| unavailable.rule)
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "note: {} enabled rule(s) need Flow type inference, which uf does not implement yet: {names}",
            report.unavailable.len()
        );
    }
    println!(
        "uf lint: checked {} file(s), {} diagnostic(s), {} rule(s) not yet available",
        report.files_checked,
        report.diagnostics.len(),
        report.unavailable.len()
    );
}

fn lsp(_cwd: &Utf8Path) -> Result<()> {
    if std::io::stdin().is_terminal() {
        println!("uf lsp: JSON-RPC stdio server ready");
        return Ok(());
    }

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .with_context(|| "failed to read LSP stdin")?;
    if input.trim().is_empty() {
        return Ok(());
    }

    let id = json_rpc_id(&input).unwrap_or(serde_json::Value::Null);
    let response = json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "serverInfo": {
                "name": "uf-lsp",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "textDocumentSync": 1,
                "documentFormattingProvider": true,
                "diagnosticProvider": {
                    "interFileDependencies": true,
                    "workspaceDiagnostics": true,
                },
            },
        },
    });
    let body = serde_json::to_string(&response)?;
    print!("Content-Length: {}\r\n\r\n{body}", body.len());
    Ok(())
}

fn json_rpc_id(input: &str) -> Option<serde_json::Value> {
    let start = input.find('{')?;
    let value = serde_json::from_str::<serde_json::Value>(&input[start..]).ok()?;
    value.get("id").cloned()
}

fn publish(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("publish.json");
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "registry": resolved.config.publish.registry.as_str(),
            "dryRun": resolved.config.publish.dry_run,
            "firstPublish": {
                "mode": resolved.config.publish.first_publish.mode,
                "localBootstrap": resolved.config.publish.first_publish.local_bootstrap,
            },
            "trustedPublish": {
                "provider": resolved.config.publish.trusted_publish.provider,
                "tokenless": resolved.config.publish.trusted_publish.tokenless,
                "trigger": resolved.config.publish.trusted_publish.trigger,
            },
        }),
    )?;
    println!(
        "uf publish: registry={} dryRun={} firstPublish={:?} localBootstrap={} trustedProvider={:?} tokenless={} trigger={:?} manifest={}",
        resolved.config.publish.registry,
        resolved.config.publish.dry_run,
        resolved.config.publish.first_publish.mode,
        resolved.config.publish.first_publish.local_bootstrap,
        resolved.config.publish.trusted_publish.provider,
        resolved.config.publish.trusted_publish.tokenless,
        resolved.config.publish.trusted_publish.trigger,
        manifest
    );
    Ok(())
}

fn release(cwd: &Utf8Path, bump: ReleaseBump) -> Result<()> {
    let resolved = load_config(cwd)?;
    let current_version = env!("CARGO_PKG_VERSION");
    let next_version = bump_semver(current_version, bump)?;
    let tag = format!("{}{}", resolved.config.release.tag_prefix, next_version);
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("release.json");
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "bump": format!("{bump:?}"),
            "currentVersion": current_version,
            "nextVersion": next_version,
            "tag": tag,
            "command": resolved.config.release.command.as_str(),
            "publish": resolved.config.release.publish,
            "trustedTrigger": resolved.config.publish.trusted_publish.trigger,
        }),
    )?;
    println!(
        "uf release: bump={:?} tag={} command={} publish={} trustedTrigger={:?} manifest={}",
        bump,
        tag,
        resolved.config.release.command,
        resolved.config.release.publish,
        resolved.config.publish.trusted_publish.trigger,
        manifest
    );
    Ok(())
}

fn bump_semver(version: &str, bump: ReleaseBump) -> Result<String> {
    let mut parts = version.split('.');
    let major = parse_semver_part(parts.next(), "major")?;
    let minor = parse_semver_part(parts.next(), "minor")?;
    let patch = parse_semver_part(parts.next(), "patch")?;
    if parts.next().is_some() {
        bail!("version {version:?} is not a three-part semver");
    }

    let next = match bump {
        ReleaseBump::Patch => (major, minor, patch + 1),
        ReleaseBump::Minor => (major, minor + 1, 0),
        ReleaseBump::Major => (major + 1, 0, 0),
    };
    Ok(format!("{}.{}.{}", next.0, next.1, next.2))
}

fn parse_semver_part(part: Option<&str>, name: &str) -> Result<u64> {
    let part = part.ok_or_else(|| anyhow!("version is missing {name}"))?;
    part.parse()
        .with_context(|| format!("version {name} part {part:?} is not numeric"))
}

fn prepare(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let plan = default_plan();
    let router_manifest = write_router_manifest(&resolved.root, &resolved.config)?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("prepare.json");
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "routerManifest": router_manifest,
            "lintStagedCompatible": plan.lint_staged_compatible,
            "codeGenerator": plan.code_generator,
            "writeGeneratedFiles": plan.write_generated_files,
            "cache": format!("{:?}", plan.cache),
            "steps": plan.steps.iter().map(|step| format!("{step:?}")).collect::<Vec<_>>(),
        }),
    )?;
    println!(
        "uf prepare: root={} manifest={} lintStagedCompatible={} codeGenerator={} cache={:?}",
        resolved.root, manifest, plan.lint_staged_compatible, plan.code_generator, plan.cache
    );
    for step in plan.steps {
        println!("prepare step: {step:?}");
    }
    Ok(())
}

fn write_json_file(path: &Utf8Path, value: &serde_json::Value) -> Result<()> {
    let mut contents = serde_json::to_string_pretty(value)?;
    contents.push('\n');
    fs::write(path, contents).with_context(|| format!("failed to write {path}"))
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
            .iter()
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

    let sources = files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.source.as_str()));
    let report = run_tests(sources);
    for failure in &report.failures {
        println!("{} {}: {}", failure.file, failure.name, failure.message);
    }
    println!(
        "uf test: passed={} failed={} unsupportedAssertions={} runtime={:?} target={:?}",
        report.passed,
        report.failed,
        report.unsupported_assertions,
        runner.runtime,
        runner.performance_target
    );
    if !report.is_success() {
        bail!(
            "test failed with {} failure(s) and {} unsupported assertion(s)",
            report.failed,
            report.unsupported_assertions
        );
    }
    Ok(())
}

fn upgrade(cwd: &Utf8Path) -> Result<()> {
    let resolved = load_config(cwd)?;
    let package_manager = PackageManagerPlan::infer_from_config(&resolved.config);
    let runtime_manager = RuntimeManagerPlan::infer_from_config(&resolved.config);
    let install = install_workspace(&resolved.root, &resolved.config)?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("upgrade.json");
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "packageManager": {
                "resolver": package_manager.resolver,
                "lockfile": install.lockfile.as_str(),
                "storeManifest": install.store_manifest.as_str(),
                "packages": install.packages.len(),
                "storeEntries": install.store_entries.len(),
            },
            "runtimeManager": {
                "engine": runtime_manager.engine,
                "acquisition": runtime_manager.acquisition,
                "hosts": &runtime_manager.hosts,
            },
        }),
    )?;
    println!(
        "uf upgrade: packageResolver={:?} runtimeEngine={:?} acquisition={:?} lockfile={} manifest={}",
        package_manager.resolver,
        runtime_manager.engine,
        runtime_manager.acquisition,
        install.lockfile,
        manifest
    );
    Ok(())
}
