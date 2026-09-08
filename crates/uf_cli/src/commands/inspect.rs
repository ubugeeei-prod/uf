//! `uf inspect`: a sectioned view of the resolved project, or the whole thing
//! as JSON.

use anyhow::Result;
use camino::Utf8Path;
use serde_json::json;
use uf_config::env_files::{self, ProjectEnv};
use uf_config::{ResolvedConfig, load_config};
use uf_lib::{
    builtin_modules, hook_descriptors, std_module_descriptors, tui_contract, ui_components,
};
use uf_plugin::{PipelineMode, resolve_pipeline};
use uf_pm::{DetectionOptions, PackageManagerPlan, detect_package_manager_with};
use uf_rm::RuntimeManagerPlan;
use uf_router::discover_routes;
use uf_runtime::RuntimeContract;
use uf_term::{KeyValue, Tone};
use uf_test::NativeTestRunnerPlan;

use crate::support::{DEVELOPMENT, enabled, project_label, relative_to, yes_no};
use crate::ui::Ui;

pub(crate) fn inspect(cwd: &Utf8Path, ui: &mut Ui, as_json: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    if as_json {
        ui.json(&inspect_payload(&resolved)?)?;
        return Ok(());
    }

    let detection = detect_project_package_manager(&resolved);
    let config_path = resolved
        .config_path
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "zero-config defaults".to_string());
    let root = resolved.root.as_str().to_string();
    let router_root = resolved.config.app.router.root.to_string();
    let style = format!("{:?}", resolved.config.app.builtins.style);
    let compiler = format!("{:?}", resolved.config.app.builtins.react_compiler.mode);
    let server_engine = format!("{:?}", resolved.config.server.engine);
    let adapters = resolved.config.server.native.adapters.len().to_string();
    let task_engine = format!("{:?}", resolved.config.task_runner.engine);
    let test_runtime = format!("{:?}", resolved.config.test.runner.runtime);
    let test_target = format!("{:?}", resolved.config.test.runner.performance_target);
    let pm_resolver = format!("{:?}", resolved.config.pm.resolver);
    let pm_lockfile = resolved.config.pm.lockfile.to_string();
    let detected = detection.package_manager.to_string();
    let detected_source = detection.source.kind().to_string();
    let alternatives = detection.alternatives.len().to_string();
    let issues = detection.issues.len().to_string();
    let rm_module = resolved.config.rm.module.to_string();
    let native_modules = builtin_modules().len().to_string();
    let std_modules = std_module_descriptors().len().to_string();
    let ui_component_count = ui_components().len().to_string();
    let tui_component_count = tui_contract().components.len().to_string();
    let hooks = hook_descriptors().len().to_string();
    let lint_rules = uf_lint::rules().len().to_string();
    let environment = inspected_env(&resolved);
    let env_mode = environment
        .as_ref()
        .map_or_else(|_| String::new(), |env| env.mode().to_owned());
    let env_files = environment.as_ref().map_or_else(
        |_| String::new(),
        |env| {
            env.files()
                .iter()
                .map(|file| relative_to(&resolved.root, file))
                .collect::<Vec<_>>()
                .join(", ")
        },
    );
    let env_variables = environment
        .as_ref()
        .map_or_else(|_| String::new(), |env| env.values().len().to_string());
    let env_client = environment
        .as_ref()
        .map_or_else(|_| String::new(), |env| env.client_prefixes().join(", "));
    let lint_unavailable = uf_lint::rules()
        .iter()
        .filter(|descriptor| !descriptor.requirement.is_available())
        .count()
        .to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf inspect", Some(project_label(&resolved.root)));
        renderer.blank(out);

        renderer.heading(out, 2, "project");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::toned("root", &root, Tone::Path),
                KeyValue::toned("config", &config_path, Tone::Path),
            ],
        );
        renderer.blank(out);

        renderer.heading(out, 2, "app");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::new("router root", &router_root),
                KeyValue::new("rsc", enabled(resolved.config.app.rsc)),
                KeyValue::new(
                    "server actions",
                    enabled(resolved.config.app.server_actions),
                ),
                KeyValue::new("style", &style),
                KeyValue::new("react compiler", &compiler),
            ],
        );
        renderer.blank(out);

        renderer.heading(out, 2, "runners");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::new("server", &server_engine),
                KeyValue::toned("server adapters", &adapters, Tone::Number),
                KeyValue::new("tasks", &task_engine),
                KeyValue::new(
                    "package scripts",
                    if resolved.config.task_runner.allow_package_scripts {
                        "allowed"
                    } else {
                        "forbidden"
                    },
                ),
                KeyValue::new("tests", &test_runtime),
                KeyValue::new("test target", &test_target),
                KeyValue::new("runtime module", &rm_module),
                KeyValue::new(
                    "runtime inference",
                    enabled(resolved.config.rm.infer_from_config),
                ),
            ],
        );
        renderer.blank(out);

        renderer.heading(out, 2, "package manager");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::new("configured", &pm_resolver),
                KeyValue::toned("lockfile", &pm_lockfile, Tone::Path),
                KeyValue::toned("detected", &detected, Tone::Accent),
                KeyValue::new("detected from", &detected_source),
                KeyValue::new("ambiguous", yes_no(detection.is_ambiguous())),
                KeyValue::toned("alternatives", &alternatives, Tone::Number),
                KeyValue::toned("issues", &issues, Tone::Number),
            ],
        );
        renderer.blank(out);

        renderer.heading(out, 2, "environment");
        match &environment {
            Ok(_) => renderer.key_values(
                out,
                4,
                &[
                    KeyValue::new("mode", &env_mode),
                    KeyValue::toned(
                        "files",
                        if env_files.is_empty() {
                            "none found"
                        } else {
                            &env_files
                        },
                        Tone::Path,
                    ),
                    // A count and never the values: `uf inspect` is pasted into
                    // issues.
                    KeyValue::toned("variables", &env_variables, Tone::Number),
                    KeyValue::new("client prefix", &env_client),
                ],
            ),
            // Two lines and no third: printing `mode`, `files` and `variables`
            // beside a failure would be reporting a reading that was never
            // taken. The reason is the whole of what this command knows.
            Err(reason) => renderer.key_values(
                out,
                4,
                &[
                    KeyValue::new("files", "could not be read"),
                    KeyValue::new("reason", reason),
                ],
            ),
        }
        renderer.blank(out);

        renderer.heading(out, 2, "catalogue");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::toned("native modules", &native_modules, Tone::Number),
                KeyValue::toned("std modules", &std_modules, Tone::Number),
                KeyValue::toned("ui components", &ui_component_count, Tone::Number),
                KeyValue::toned("tui components", &tui_component_count, Tone::Number),
                KeyValue::toned("hooks", &hooks, Tone::Number),
                KeyValue::toned("lint rules", &lint_rules, Tone::Number),
                KeyValue::toned("rules needing types", &lint_unavailable, Tone::Muted),
            ],
        );
    });
    Ok(())
}

/// Infer which package manager drives the project, honouring `pm.packageManager`.
///
/// The walk starts at the resolved project root and is free to reach the nearest
/// ancestor workspace root, which is how a package inside a pnpm or yarn monorepo
/// inherits the manager its repository already uses.
fn detect_project_package_manager(resolved: &ResolvedConfig) -> uf_pm::Detection {
    detect_package_manager_with(
        &resolved.root,
        &DetectionOptions::from_config(&resolved.config),
    )
}

fn inspect_payload(resolved: &ResolvedConfig) -> Result<serde_json::Value> {
    let routes = discover_routes(&resolved.root, &resolved.config)?
        .into_iter()
        .map(|route| {
            // `hasMiddleware` keeps meaning what it has always meant — this
            // directory declares one — and `middleware` is the chain that
            // actually runs, inherited from every directory above it. They are
            // different answers for every route below the one that declares a
            // guard, and only the second one tells a reader whether the route
            // is guarded.
            let has_own_middleware = route.has_own_middleware();
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
                "hasMiddleware": has_own_middleware,
                "middleware": route.middleware,
            })
        })
        .collect::<Vec<_>>();
    let runtime = RuntimeContract::capability_js_hosts();
    let test_runner = NativeTestRunnerPlan::runtime_agnostic();
    let package_manager = PackageManagerPlan::infer_from_config(&resolved.config);
    let package_manager_detection = detect_project_package_manager(resolved);
    let runtime_manager = RuntimeManagerPlan::infer_from_config(&resolved.config);
    // Every stage of the build is a plugin, so the resolved order here is the
    // order that actually runs — including whatever `plugins: [...]` adds.
    let pipeline = resolve_pipeline(&resolved.config, &resolved.root, PipelineMode::Build)?;

    let environment = inspected_env(resolved);
    Ok(json!({
        "command": "uf",
        "config": resolved,
        // Names, never values. `uf inspect --json` is what a person pastes into
        // an issue, and half of what is in a `.env` file is a credential.
        //
        // And `error` rather than `null` when it could not be read: a reader
        // that gets `null` has to guess whether this project has no `.env`
        // files or has one that does not parse, and those are opposite answers
        // to the question they are asking. The message names a file and a line
        // and never a value; see `EnvFileError`.
        "env": match &environment {
            Ok(env) => json!({
                "mode": env.mode(),
                "files": env.files().iter().map(|file| relative_to(&resolved.root, file)).collect::<Vec<_>>(),
                "variables": env.values().keys().collect::<Vec<_>>(),
                "clientVisible": env.values().keys().filter(|name| env.is_client_visible(name)).collect::<Vec<_>>(),
                "clientPrefix": env.client_prefixes(),
            }),
            Err(reason) => json!({ "error": reason }),
        },
        "plugins": pipeline.report(),
        "routes": routes,
        "nativeModules": builtin_modules(),
        "stdModules": std_module_descriptors(),
        "tui": tui_contract(),
        "hooks": hook_descriptors(),
        "lintRules": uf_lint::rules(),
        "ui": ui_components(),
        "engines": {
            "parser": "official-flow-parser",
            "build": "vite",
            "devServer": "vite",
            "runtime": "capability-js-host-contract",
            "runtimeContract": runtime,
            // Beside the contract on purpose. The contract names the hosts uf
            // is *written for*; this says what each of them does when you run
            // it, and the two were being read as one thing — which is how
            // `hosts: [node, deno, bun]` came to be quoted as a statement that
            // a uf project runs on Deno. See `uf_runtime::HostSupport`.
            "hostSupport": host_support(),
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
            "packageManagerDetection": package_manager_detection,
            "runtimeManager": runtime_manager,
            "reactCompiler": {
                "enabled": resolved.config.app.builtins.react_compiler.enabled,
                "mode": resolved.config.app.builtins.react_compiler.mode,
            }
        }
    }))
}

/// The environment `uf inspect` reports, or nothing when it cannot be read.
///
/// `development`, because `uf inspect` is a question asked at a terminal and
/// that is the mode a terminal is in; a mode a project pinned with `uf env use`
/// or `env.active` still wins.
///
/// A `.env` file that does not parse is not `uf inspect`'s to refuse: this
/// command is what a person runs to find out what is wrong, and failing it on
/// the file they are asking about would leave them nothing to read.
///
/// So the failure is carried rather than dropped. `.ok()` used to throw it
/// away, and the section then printed `mode unknown`, `files none found` and
/// `variables 0` — which is what a project with no `.env` files at all looks
/// like, so the one command that exists to say what is wrong said nothing was.
/// The reason is a file, a line and a message; `EnvFileError` never puts a
/// value in one.
fn inspected_env(resolved: &ResolvedConfig) -> Result<ProjectEnv, String> {
    let mode = env_files::resolve_mode(&resolved.root, &resolved.config, None, DEVELOPMENT)
        .map_err(|error| error.to_string())?;
    env_files::load(&resolved.root, &resolved.config, &mode).map_err(|error| error.to_string())
}

/// Every host, and what it actually does — the JSON half of `docs/hosts.md`.
///
/// `uf inspect --json` is what a person pastes into an issue, so the row a
/// reader needs is the one that says whether the host they are on is a host uf
/// runs on. `missing` and `trackingIssue` are carried through rather than
/// summarized: "planned" without "planned on what" is the shape of claim this
/// table replaced.
fn host_support() -> serde_json::Value {
    json!(
        uf_runtime::HOSTS
            .iter()
            .map(|support| json!({
                "host": support.host,
                "level": support.level.as_str(),
                "flowLoader": support.flow_loader,
                "enforcesPermissions": support
                    .enforces
                    .iter()
                    .map(|permission| permission.as_str())
                    .collect::<Vec<_>>(),
                "verifiedBy": support.verified_by,
                "missing": support.missing,
                "trackingIssue": support.tracking_issue,
            }))
            .collect::<Vec<_>>()
    )
}
