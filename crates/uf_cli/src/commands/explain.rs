//! `uf explain`: what a command will actually do, and who will do it.
//!
//! An integrated toolchain that cannot say what it is doing is a black box,
//! and a black box is where an integration's problems stop being annoying and
//! start being unfixable — `docs/red-lines.md`, line 7. `uf inspect` prints
//! the resolved configuration; this answers a different question, which is the
//! one someone asks when a command surprises them: *which provider runs each
//! stage, at what version, and which files decided that?*
//!
//! Every stage names the thing that implements it. Where that is somebody
//! else's tool, it says so — the point of the exercise is that uf orchestrates
//! providers rather than being one, and a plan that hid the providers would be
//! documenting the opposite.

use anyhow::{Result, bail};
use camino::Utf8Path;
use serde_json::json;
use uf_config::{ResolvedConfig, load_config};
use uf_term::KeyValue;

use crate::support::project_label;
use crate::ui::Ui;

/// One step of a command, and what performs it.
struct Stage {
    name: &'static str,
    provider: String,
    detail: String,
}

/// The commands `uf explain` knows how to describe.
///
/// The order is the order they are tried in a project: build the thing, check
/// it, then ship it. Commands that do their whole job in this binary and
/// delegate nothing — `info`, `inspect`, `explain`, `completion`, `create` —
/// are absent on purpose: there is no provider to name, and an entry saying
/// "uf" three times would be a list of nothing.
const KNOWN: &[&str] = &[
    "dev", "build", "doc", "test", "fmt", "lint", "check", "run", "install", "upgrade", "use",
    "env", "prepare", "publish", "release", "lsp",
];

pub(crate) fn explain(cwd: &Utf8Path, ui: &mut Ui, command: &str, as_json: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let stages = match command {
        "dev" => dev_stages(&resolved),
        "build" => build_stages(&resolved),
        "doc" => doc_stages(),
        "test" => test_stages(&resolved),
        "fmt" => fmt_stages(&resolved),
        "lint" => lint_stages(&resolved),
        "check" => check_stages(&resolved),
        "run" => run_stages(&resolved),
        "install" => install_stages(&resolved),
        "upgrade" => upgrade_stages(&resolved),
        "use" | "env" => runtime_stages(&resolved),
        "prepare" => prepare_stages(&resolved),
        "publish" => publish_stages(&resolved),
        "release" => release_stages(&resolved),
        "lsp" => lsp_stages(&resolved),
        other => bail!(
            "uf explain does not describe {other:?}; it knows {}",
            KNOWN.join(", ")
        ),
    };

    let sources = config_sources(&resolved);

    if as_json {
        ui.json(&json!({
            "command": format!("uf {command}"),
            "root": resolved.root.as_str(),
            "stages": stages
                .iter()
                .map(|stage| json!({
                    "name": stage.name,
                    "provider": stage.provider,
                    "detail": stage.detail,
                }))
                .collect::<Vec<_>>(),
            "configurationSources": sources,
        }))?;
        return Ok(());
    }

    let label = project_label(&resolved.root);
    let heading = format!("uf {command}");
    ui.render(|renderer, out| {
        renderer.banner(out, "uf explain", Some(label));
        renderer.blank(out);
        renderer.heading(out, 2, &heading);
        renderer.blank(out);

        for (index, stage) in stages.iter().enumerate() {
            let step = format!("{}. {}", index + 1, stage.name);
            renderer.heading(out, 4, &step);
            renderer.key_values(
                out,
                8,
                &[
                    KeyValue::new("provider", &stage.provider),
                    KeyValue::new("what", &stage.detail),
                ],
            );
        }

        renderer.blank(out);
        renderer.heading(out, 2, "configuration");
        let rows: Vec<&str> = sources.iter().map(String::as_str).collect();
        renderer.bullet_list(out, 4, &rows);
    });
    Ok(())
}

/// Where the answers came from.
///
/// Listed because "why is it doing that" is nearly always answered by a file
/// the reader had forgotten about, or by there being no file at all.
fn config_sources(resolved: &ResolvedConfig) -> Vec<String> {
    let mut sources = Vec::new();
    match &resolved.config_path {
        Some(path) => sources.push(path.to_string()),
        None => sources.push("zero-config defaults (no uf.config.js found)".to_string()),
    }
    if resolved.root.join("package.json").exists() {
        sources.push("package.json".to_string());
    }
    if resolved.root.join(".flowconfig").exists() {
        sources.push(".flowconfig".to_string());
    }
    sources
}

/// What to call the package resolver in a sentence.
///
/// `format!("{:?}")` would print `UfNative`, and lowercasing that gives
/// `ufnative` — a word nobody wrote and nobody can search for. A provider's
/// name is the thing a reader has to recognise, so it is spelled out. When a
/// second resolver lands this stops compiling, which is the right way to be
/// reminded to name it.
fn resolver_name(resolved: &ResolvedConfig) -> &'static str {
    match resolved.config.pm.resolver {
        uf_config::PackageManagerResolver::UfNative => "uf (its own resolver)",
    }
}

/// `uf run`, whose whole question is which runner executes a task.
fn run_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    // `#[non_exhaustive]`, so the fallback is the debug name: a provider added
    // upstream should read oddly here rather than not compile here.
    let engine = match resolved.config.task_runner.engine {
        uf_config::TaskRunnerEngine::ViteTask => "vite task".to_string(),
        other => format!("{other:?}"),
    };
    vec![
        Stage {
            name: "task lookup",
            provider: "uf".to_string(),
            detail: format!(
                "`tasks` in uf.config.js, {} defined here",
                resolved.config.tasks.len()
            ),
        },
        Stage {
            name: "scheduling",
            provider: engine,
            detail: "dependency order and caching, for a task with no command".to_string(),
        },
        Stage {
            name: "execution",
            provider: "uf".to_string(),
            detail: format!(
                "a task with a `command` runs here; package scripts are {}",
                if resolved.config.task_runner.allow_package_scripts {
                    "allowed"
                } else {
                    "refused"
                }
            ),
        },
    ]
}

/// `uf install`, whose whole question is which resolver decides a tree.
fn install_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "workspace discovery",
            provider: "uf_pm".to_string(),
            detail: "every package.json this project owns".to_string(),
        },
        Stage {
            name: "resolution",
            provider: resolver_name(resolved).to_string(),
            detail: format!(
                "writes {}, and the content-addressed store under {}",
                resolved.config.pm.lockfile, resolved.config.pm.store_dir
            ),
        },
        Stage {
            name: "lifecycle scripts",
            provider: "uf".to_string(),
            detail: if resolved.config.pm.allow_lifecycle_scripts {
                "allowed by pm.allowLifecycleScripts".to_string()
            } else {
                "refused; a dependency does not get to run code at install".to_string()
            },
        },
    ]
}

fn upgrade_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "packages",
            provider: resolver_name(resolved).to_string(),
            detail: format!("re-resolves against {}", resolved.config.pm.lockfile),
        },
        Stage {
            name: "toolchain",
            provider: "uf_rm".to_string(),
            detail: format!(
                "acquisition {:?}, applied {:?}",
                resolved.config.rm.acquisition, resolved.config.rm.apply
            ),
        },
    ]
}

/// `uf use` and `uf env`, which are the same question about the same manager.
fn runtime_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "inference",
            provider: "uf_rm".to_string(),
            detail: if resolved.config.rm.infer_from_config {
                "reads the version this project asks for".to_string()
            } else {
                "off; the version is whatever is active".to_string()
            },
        },
        Stage {
            name: "acquisition",
            provider: match resolved.config.rm.acquisition {
                uf_config::RuntimeManagerAcquisition::Auto => "automatic".to_string(),
            },
            detail: format!("auto-switch {}", resolved.config.rm.auto_switch),
        },
        host_stage(resolved),
    ]
}

fn prepare_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "staged files",
            provider: "uf_prepare".to_string(),
            detail: "lint-staged compatible: the files a commit is about".to_string(),
        },
        Stage {
            name: "checks",
            provider: "uf".to_string(),
            detail: "the same fmt, lint and check the commands run".to_string(),
        },
        Stage {
            name: "generation",
            provider: "@uniflowed/router".to_string(),
            detail: "route metadata and generated types, into .uf/".to_string(),
        },
        host_stage(resolved),
    ]
}

fn publish_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    let publish = &resolved.config.publish;
    vec![
        Stage {
            name: "registry",
            provider: publish.registry.to_string(),
            detail: format!(
                "dry run {}, first publish {:?}",
                publish.dry_run, publish.first_publish.mode
            ),
        },
        Stage {
            name: "authentication",
            provider: "npm trusted publishing (OIDC)".to_string(),
            detail: "the workflow's own identity; there is no token to hold".to_string(),
        },
        Stage {
            name: "manifest",
            provider: "uf".to_string(),
            detail: "tools/release/published-packages.txt, in order".to_string(),
        },
    ]
}

fn release_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "version",
            provider: "uf".to_string(),
            detail: "the next prerelease from the current one".to_string(),
        },
        Stage {
            name: "metadata",
            provider: "uf".to_string(),
            detail: "writes the tag's manifest; the tag itself is a push".to_string(),
        },
        Stage {
            name: "publish",
            provider: resolved.config.publish.registry.to_string(),
            detail: "on the tag, by the workflow — see `uf explain publish`".to_string(),
        },
    ]
}

fn lsp_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "transport",
            provider: "uf".to_string(),
            detail: "JSON-RPC over stdio, Content-Length framed".to_string(),
        },
        Stage {
            name: "formatting",
            provider: "uf_fmt".to_string(),
            detail: format!(
                "the same printer `uf fmt` uses, at {} columns",
                resolved.config.fmt.line_width
            ),
        },
    ]
}

fn transform_stage() -> Stage {
    Stage {
        name: "Flow to JavaScript",
        provider: "uf transform (in this binary)".to_string(),
        detail: "flow_parser, Hermes lowering, the React Compiler crate, oxc".to_string(),
    }
}

fn host_stage(resolved: &ResolvedConfig) -> Stage {
    Stage {
        name: "JavaScript host",
        provider: format!("{:?}", resolved.config.app.runtime.default).to_lowercase(),
        detail: "runs Vite and any JavaScript plugin".to_string(),
    }
}

fn dev_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "configuration",
            provider: "uf".to_string(),
            detail: "uf.config.js, with `vite` merged over what uf generates".to_string(),
        },
        host_stage(resolved),
        Stage {
            name: "dev server",
            provider: "vite (@uniflowed/vite driver)".to_string(),
            detail: "module graph, HMR, plugin pipeline".to_string(),
        },
        transform_stage(),
        Stage {
            name: "rendering",
            provider: "@uniflowed/router".to_string(),
            detail: "server-renders each request, route handlers first".to_string(),
        },
    ]
}

fn build_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "configuration",
            provider: "uf".to_string(),
            detail: "uf.config.js, with `vite` merged over what uf generates".to_string(),
        },
        host_stage(resolved),
        transform_stage(),
        Stage {
            name: "bundle",
            // Vite, not what Vite uses inside it. A project chose Vite — it is
            // in their config — and did not choose the bundler underneath, so
            // naming that one would hand them something to reason about that
            // they never picked. `uf_plugin`'s naming invariant enforces this,
            // and it is the finer point of red line 7: uf names the providers
            // it orchestrates, and a provider's internals stay the provider's.
            provider: "vite".to_string(),
            detail: "client bundle, then the server bundle".to_string(),
        },
        Stage {
            name: "prerender",
            provider: "@uniflowed/router".to_string(),
            detail: "every route without parameters, to static HTML".to_string(),
        },
    ]
}

fn doc_stages() -> Vec<Stage> {
    vec![
        Stage {
            name: "discovery",
            provider: "uf_project".to_string(),
            detail: "project-owned JavaScript files, after `lint.ignore`".to_string(),
        },
        Stage {
            name: "parse",
            provider: "flow_parser (Meta official Rust port)".to_string(),
            detail: "Flow syntax tree and source comments".to_string(),
        },
        Stage {
            name: "render",
            provider: "uf_doc".to_string(),
            detail: "JSDoc on exported Flow declarations, to Markdown".to_string(),
        },
    ]
}

fn test_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "discovery",
            provider: "uf_test (in this binary)".to_string(),
            detail: "reads which files declare tests, without importing them".to_string(),
        },
        Stage {
            name: "scheduling",
            provider: format!("{:?}", resolved.config.test.runner.scheduler),
            detail: "one file per worker, longest expected first".to_string(),
        },
        host_stage(resolved),
        transform_stage(),
        Stage {
            name: "execution",
            provider: resolved.config.test.module.to_string(),
            detail: "runs the bodies and streams one line per case".to_string(),
        },
    ]
}

fn fmt_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "Flow files",
            provider: format!("{:?}", resolved.config.fmt.flow.parser),
            detail: format!(
                "the official parser, printed at {} columns",
                resolved.config.fmt.line_width
            ),
        },
        Stage {
            name: "everything else",
            // The provider's own name rather than the variant's, because it is
            // the binary uf runs and the thing a reader would install.
            provider: resolved.config.fmt.non_flow.formatter.as_str().to_string(),
            detail: match uf_fmt::non_flow::invocation(
                resolved.config.fmt.non_flow.formatter,
                false,
                &resolved.config.fmt,
            ) {
                // The exact command, so `uf explain` answers "what will it run"
                // rather than "which one is selected".
                Some(invocation) => format!(
                    "JSON, CSS and TypeScript, by `{} {}`",
                    invocation.program,
                    invocation
                        .arguments
                        .iter()
                        .map(compact_str::CompactString::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                None => "nothing: JSON, CSS and TypeScript are left alone".to_string(),
            },
        },
    ]
}

fn lint_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "uf rules",
            provider: format!("{:?}", resolved.config.lint.engine),
            detail: format!("{} rules configured", resolved.config.lint.rules.len()),
        },
        Stage {
            name: "Flow's own lints",
            provider: format!("{:?}", resolved.config.lint.flow.parser),
            detail: format!("built-ins: {:?}", resolved.config.lint.flow.builtins),
        },
    ]
}

fn check_stages(_resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![Stage {
        name: "type checking",
        provider: "flow (upstream)".to_string(),
        detail: "uf does not type-check; Flow is the type system".to_string(),
    }]
}
