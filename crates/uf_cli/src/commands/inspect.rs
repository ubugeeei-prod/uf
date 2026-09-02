//! `uf inspect`: a sectioned view of the resolved project, or the whole thing
//! as JSON.

use anyhow::Result;
use camino::Utf8Path;
use serde_json::json;
use uf_config::{ResolvedConfig, load_config};
use uf_lib::{
    builtin_modules, hook_descriptors, motion_contract, orm_contract, std_module_descriptors,
    tui_contract, ui_components, vrt_plan,
};
use uf_pm::{DetectionOptions, PackageManagerPlan, detect_package_manager_with};
use uf_rm::RuntimeManagerPlan;
use uf_router::discover_routes;
use uf_runtime::RuntimeContract;
use uf_term::{KeyValue, Tone};
use uf_test::NativeTestRunnerPlan;

use crate::support::{enabled, project_label, yes_no};
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
    let package_manager_detection = detect_project_package_manager(resolved);
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
            "packageManagerDetection": package_manager_detection,
            "runtimeManager": runtime_manager,
            "reactCompiler": {
                "enabled": resolved.config.app.builtins.react_compiler.enabled,
                "mode": resolved.config.app.builtins.react_compiler.mode,
            }
        }
    }))
}
