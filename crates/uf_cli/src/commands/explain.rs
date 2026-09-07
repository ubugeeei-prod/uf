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
use uf_config::env_files;
use uf_config::{DeployAdapter, Prerender, RenderingPlan, ResolvedConfig, load_config};
use uf_pm::{DependencyKind, Operation, command_for, detect_package_manager, installable};
use uf_term::KeyValue;

use crate::commands::builder;
use crate::commands::task::fetchable;
use crate::support::{DEVELOPMENT, PRODUCTION, TEST, project_label};
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
    "dev", "build", "preview", "start", "doc", "test", "fmt", "lint", "check", "run", "exec",
    "install", "add", "remove", "update", "patch", "pm", "catalog", "why", "upgrade", "use", "env",
    "prepare", "publish", "release", "lsp",
];

pub(crate) fn explain(cwd: &Utf8Path, ui: &mut Ui, command: &str, as_json: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let stages = match command {
        "dev" => dev_stages(&resolved),
        "build" => build_stages(&resolved),
        "preview" => preview_stages(&resolved),
        "start" => start_stages(&resolved),
        "doc" => doc_stages(),
        "test" => test_stages(&resolved),
        "fmt" => fmt_stages(&resolved),
        "lint" => lint_stages(&resolved),
        "check" => check_stages(&resolved),
        "run" => run_stages(&resolved),
        "exec" => exec_stages(&resolved),
        "install" => install_stages(&resolved),
        "add" => dependency_stages(
            &resolved,
            Operation::Add {
                kind: DependencyKind::Prod,
            },
            "writes the specifiers into dependencies, the lockfile and node_modules",
        ),
        "remove" | "uninstall" => dependency_stages(
            &resolved,
            Operation::Remove,
            "takes the names out of every dependency field, the lockfile and node_modules",
        ),
        "update" => dependency_stages(
            &resolved,
            Operation::Update,
            "moves the lockfile to the newest versions the manifest ranges already allow",
        ),
        // The listing is uf's own — it reads the workspace's manifests and
        // nothing else — and `uf catalog set` rewrites them and then delegates
        // an install, which is the part with a provider worth naming.
        "pm" => query_stages(
            &resolved,
            Operation::Install,
            "`uf pm approve-builds` reads node_modules for the packages that declare an install \
             script and the root manifest for the ones this project has approved; the manager is \
             what enforces the list, which is why the list lives in the field the manager reads",
        ),
        "patch" => query_stages(
            &resolved,
            Operation::Patch,
            "the manager extracts a copy of the package into a temporary directory and prints \
             the path; `uf patch --commit` turns your edits into a patch file and reinstalls",
        ),
        "catalog" => dependency_stages(
            &resolved,
            Operation::Install,
            "`uf catalog` reads the manifests and reports; `uf catalog set` rewrites the range \
             in each of them and the manager installs what they now say",
        ),
        "why" => why_stages(&resolved),
        "ls" => query_stages(
            &resolved,
            Operation::List,
            "the manager reads its own lockfile and prints the tree it installed",
        ),
        "audit" => query_stages(
            &resolved,
            Operation::Audit,
            "the manager sends the tree to its registry's advisory endpoint and reports what \
             came back",
        ),
        "search" => query_stages(
            &resolved,
            Operation::Search,
            "the manager queries its own registry — which is the reason uf does not substitute \
             another manager's search for one that has none",
        ),
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
        env_stage(resolved, DEVELOPMENT),
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

/// `uf exec`, which is four different commands wearing one name.
///
/// Worth explaining precisely because of that: the answer to "what will
/// `ufx foo` do" is one of four things, and which one depends on a directory
/// listing the reader cannot see. It used to be a fifth — write a JSON file and
/// exit 0 — which is the thing nobody could have guessed.
///
/// The stages are `exec_package`'s branches in `exec_package`'s order, and that
/// is the whole contract of this function. The explicit-path stage was missing
/// for a while and the omission was not cosmetic: `ufx ./scripts/codegen.js`
/// runs *before* the package-manager branch, so a reader who checked here was
/// told their path would be fetched from a registry and refused without
/// `--yes`, when in fact it runs with no consent asked for at all. Being wrong
/// about which of two paths asks permission is the one thing this command must
/// not be.
fn exec_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "uf's own packages",
            provider: "uf".to_string(),
            detail: "@uniflowed/create, @uniflowed/test and @uniflowed/pm run in this process"
                .to_string(),
        },
        Stage {
            name: "installed binaries",
            provider: "the project".to_string(),
            detail: format!(
                "anything in {}, run directly with your arguments and its exit status",
                resolved.root.join("node_modules/.bin")
            ),
        },
        Stage {
            name: "an explicit path",
            provider: "the project".to_string(),
            detail: format!(
                "a name that is a file — `ufx ./scripts/codegen.js` — is executed as written from {}, with no --yes asked for",
                resolved.root
            ),
        },
        Stage {
            name: "everything else",
            provider: provider_for(
                fetchable(detect_package_manager(&resolved.root).package_manager),
                Operation::DlxExec,
            ),
            detail: format!(
                "a package that is not installed: refused unless --yes, because fetching a name {} does not pin runs code the project never asked for",
                resolved.config.pm.lockfile
            ),
        },
    ]
}

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

/// `uf add`, `uf remove` and `uf update`, which differ only in the row above.
///
/// The provider is the manager that will actually run, spelled as the command
/// line it will be spawned as — `command_for` with the detection, so a pnpm
/// project is told `pnpm add` and a Bun project `bun add`. That is the whole
/// point of asking: these three delegate, and a plan that hid the delegate
/// would document the opposite of what happens.
fn dependency_stages(
    resolved: &ResolvedConfig,
    operation: Operation<'_>,
    what: &str,
) -> Vec<Stage> {
    let manager = installable(&detect_package_manager(&resolved.root)).0;
    vec![
        Stage {
            name: "workspace discovery",
            provider: "uf_pm".to_string(),
            detail: "every package.json this project owns, before anything is fetched".to_string(),
        },
        Stage {
            name: "lifecycle scripts",
            provider: "uf".to_string(),
            detail: if resolved.config.pm.allow_lifecycle_scripts {
                "allowed by pm.allowLifecycleScripts".to_string()
            } else {
                "refused; --ignore-scripts is passed to the manager below".to_string()
            },
        },
        Stage {
            name: "resolution and install",
            provider: provider_for(manager, operation),
            detail: what.to_string(),
        },
        Stage {
            name: "uf's own lockfile",
            provider: resolver_name(resolved).to_string(),
            detail: format!(
                "rewrites {} and the store under {} from the manifests the manager changed",
                resolved.config.pm.lockfile, resolved.config.pm.store_dir
            ),
        },
    ]
}

/// The command a manager runs for an operation, or the fact that it has none.
///
/// `uf explain` answers "which provider does each part", and for a manager
/// without the command the honest answer is that nothing does — not the
/// command some other manager would have run.
fn provider_for(manager: uf_pm::PackageManager, operation: Operation<'_>) -> String {
    command_for(manager, operation).map_or_else(
        || format!("none — {manager} has no `{}`", operation.name()),
        |invocation| invocation.to_string(),
    )
}

/// `uf ls`, `uf audit` and `uf search`: one stage, because they write nothing.
///
/// The same shape as [`why_stages`]. Separate because the answer for a manager
/// that has no such command is the interesting one, and [`provider_for`] is
/// where it is said.
fn query_stages(resolved: &ResolvedConfig, operation: Operation<'_>, what: &str) -> Vec<Stage> {
    let manager = installable(&detect_package_manager(&resolved.root)).0;
    vec![Stage {
        name: "the answer",
        provider: provider_for(manager, operation),
        detail: what.to_owned(),
    }]
}

/// `uf why`, which changes nothing and therefore has one stage.
fn why_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    let manager = installable(&detect_package_manager(&resolved.root)).0;
    vec![Stage {
        name: "the answer",
        provider: provider_for(manager, Operation::Why),
        detail: format!(
            "the manager reads its own lockfile and prints the chain; uf writes nothing, not even {}",
            resolved.config.pm.lockfile
        ),
    }]
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

/// What happens to an imported image or font, and what does not.
///
/// Red line 7, in the form that matters most for this stage: uf will not emit
/// AVIF or a lossy WebP, and a reader who does not know that will conclude the
/// pipeline is broken rather than bounded. The detail line says what the
/// project asked for and `uf explain` is where somebody looks when the output
/// surprised them, so the limit belongs in it rather than only in the
/// documentation.
///
/// A project that turned both halves off is told the stage does nothing, which
/// is a different answer from the stage not being listed: a pipeline whose
/// shape changes when a feature is disabled cannot be compared between two
/// projects.
fn assets_stage(resolved: &ResolvedConfig) -> Stage {
    let images = &resolved.config.app.builtins.images;
    let fonts = &resolved.config.app.builtins.fonts;
    match (images.enabled, fonts.enabled) {
        (false, false) => Stage {
            name: "assets",
            provider: "none".to_string(),
            detail: "images and fonts are both disabled; imports are Vite's to resolve".to_string(),
        },
        (images_on, fonts_on) => {
            let mut detail = String::new();
            if images_on {
                detail.push_str(&format!(
                    "images: {} widths at quality {}, png/jpeg and webp-when-smaller \
                     (no avif, no lossy webp — neither encoder is in this binary)",
                    images.widths.len(),
                    images.quality,
                ));
            }
            if images_on && fonts_on {
                detail.push_str("; ");
            }
            if fonts_on {
                detail.push_str(&format!(
                    "fonts: self-hosted, font-display {}, fallback scaled from {} \
                     (not subsetted)",
                    fonts.display, fonts.fallback,
                ));
            }
            Stage {
                name: "assets",
                provider: "uf assets (in this binary)".to_string(),
                detail,
            }
        }
    }
}

/// Where the values in `process.env` and `import.meta.env` come from.
///
/// The mode is resolved the way the command would resolve it, so a project with
/// a profile or an `env.active` sees the files it will actually get. A mode
/// that cannot be resolved falls back to the command's default rather than
/// failing: `uf explain` is what somebody runs *because* something is wrong.
///
/// It says so, though. A hand-edited `.uf/profile` or an `env.active`
/// that is not a mode is exactly the fault somebody runs this command about,
/// and a stage that quietly described `development` instead would answer the
/// question they asked with a description of a project they do not have — and
/// the file list below would be the fallback's, not theirs.
///
/// The files are the cascade, not a reading of the disk. `uf explain` describes
/// a pipeline and nothing here calls [`env_files::load`], so the list is what a
/// command will look for and an absent file is skipped when it does — which the
/// wording has to say, or a reader trying to find out why their variable is
/// unset will read a file name here and conclude the file was found.
/// `uf inspect` is the command that reports what was actually read.
fn env_stage(resolved: &ResolvedConfig, default_mode: &str) -> Stage {
    let (mode, unresolved) =
        match env_files::resolve_mode(&resolved.root, &resolved.config, None, default_mode) {
            Ok(mode) => (mode, None),
            Err(error) => (default_mode.to_owned(), Some(error.to_string())),
        };
    let files = if resolved.config.env.files.is_empty() {
        format!(".env, .env.local, .env.{mode}, .env.{mode}.local")
    } else {
        resolved
            .config
            .env
            .files
            .iter()
            .map(compact_str::CompactString::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    // The project's own prefix, not the default one: a project that set
    // `vite: { envPrefix: "PUBLIC_" }` would otherwise be told here that
    // `PUBLIC_TOKEN` stays on the server, which is the one mistake this line
    // exists to prevent.
    let prefix = env_files::client_prefixes(&resolved.config).join(", ");
    let cascade = format!(
        "looks for {files}, skipping any that are absent; later wins, the process \
         environment beats all, {prefix} reaches the client"
    );
    match unresolved {
        None => Stage {
            name: "environment",
            provider: format!("uf (mode {mode})"),
            detail: cascade,
        },
        Some(reason) => Stage {
            name: "environment",
            provider: format!("uf (mode {mode}, the fallback)"),
            detail: format!("this project's mode could not be resolved — {reason}; {cascade}"),
        },
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
        env_stage(resolved, DEVELOPMENT),
        Stage {
            name: "dev server",
            provider: builder_provider(resolved),
            detail: "module graph, HMR, plugin pipeline — through the builder's driver".to_string(),
        },
        transform_stage(),
        assets_stage(resolved),
        Stage {
            name: "rendering",
            provider: "@uniflowed/router".to_string(),
            detail: "server-renders each request, route handlers first".to_string(),
        },
    ]
}

/// Which builder is about to run, and which build of it.
///
/// Both halves, because ubugeeei-prod/uf#549 asks both questions: a project
/// that swapped the builder wants to see the swap took, and a project pinned
/// to a build of it that is not uf's wants to see *that*. The version comes
/// from the resolved package's own manifest rather than from anything uf
/// records, so it is the copy that will run.
///
/// A builder that cannot be resolved is named as the failure it is rather than
/// silently reported as `vite`: `uf explain` exists so that a person does not
/// have to guess what a command will do, and "the builder you named is not
/// installed" is exactly the answer they came for.
fn builder_provider(resolved: &ResolvedConfig) -> String {
    match builder::resolve(&resolved.root, &resolved.config) {
        Ok(builder) => builder.label(),
        Err(error) => format!("{} (unresolved: {error})", resolved.config.builder.module),
    }
}

fn build_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "configuration",
            provider: "uf".to_string(),
            detail: "uf.config.js, with `vite` merged over what uf generates".to_string(),
        },
        host_stage(resolved),
        env_stage(resolved, PRODUCTION),
        transform_stage(),
        assets_stage(resolved),
        Stage {
            name: "bundle",
            // The builder the project selected, and not what it uses inside
            // it. A project chose Vite — it is in their config, explicitly or
            // by default — and did not choose the bundler underneath, so
            // naming that one would hand them something to reason about that
            // they never picked. `uf_plugin`'s naming invariant enforces this,
            // and it is the finer point of red line 7: uf names the providers
            // it orchestrates, and a provider's internals stay the provider's.
            provider: builder_provider(resolved),
            detail: match RenderingPlan::resolve(&resolved.config).emits_a_server() {
                true => "client bundle, then the server bundle".to_string(),
                // Said, because the difference is the whole of what
                // `build.staticBuild` does and none of it is visible from the
                // output directory. The bundle is still built — the prerender
                // renders through it — and then removed.
                false => "client bundle, then a server bundle the build removes".to_string(),
            },
        },
        prerender_stage(resolved),
        adapter_stage(resolved),
    ]
}

/// What the build renders now, and what it leaves to a server.
///
/// The answer to a question `uf explain build` could not answer until
/// ubugeeei-prod/uf#336: `app.rendering.modes` and `build.staticBuild` select
/// between three behaviours, and a reader looking at `dist/` cannot tell which
/// one ran — a route with no document could be one the build refused to write,
/// one it was told not to write, or one that failed.
fn prerender_stage(resolved: &ResolvedConfig) -> Stage {
    let plan = RenderingPlan::resolve(&resolved.config);
    let detail = match plan.prerender() {
        Prerender::Everything => {
            "every route, to static HTML; a route that can only be answered per request is an \
             error naming it"
                .to_string()
        }
        Prerender::Possible => {
            "every route it can, to static HTML; the rest are rendered per request".to_string()
        }
        Prerender::Nothing => {
            "nothing: `app.rendering.modes` allows no `ssg`, so every route is rendered per \
             request"
                .to_string()
        }
    };
    Stage {
        name: "prerender",
        provider: match plan.source().key() {
            Some(key) => format!("@uniflowed/router ({key})"),
            None => "@uniflowed/router".to_string(),
        },
        detail,
    }
}

/// Which deploy adapter a build will write for, named rather than assumed.
///
/// Red line 7, and the last unticked box of ubugeeei-prod/uf#250: an
/// integrated toolchain that cannot say what it is doing is a black box, and
/// "which of the seven targets did that build produce" is a question a person
/// asks at exactly the moment they can least afford to guess.
///
/// A project that has asked for none is told so, and told what the build
/// therefore is: `dist/` plus a server bundle that needs the checkout around
/// it. That sentence is the honest description of `uf build` today, and it is
/// the reason `--adapter` exists.
fn adapter_stage(resolved: &ResolvedConfig) -> Stage {
    match resolved.config.app.runtime.deploy.adapter {
        Some(adapter) => Stage {
            name: "adapter",
            provider: format!("uf ({})", adapter.as_str()),
            // The entry that is actually written, not `server.js` for all of
            // them: `uf explain` describing a file the build does not produce
            // is the thing this stage exists to stop.
            detail: format!(
                ".uf/deploy/{}: {} and a copy of {}",
                adapter.as_str(),
                adapter_entries(adapter),
                resolved.config.build.out_dir
            ),
        },
        None => Stage {
            name: "adapter",
            provider: "none".to_string(),
            detail: format!(
                "{} plus a server bundle that needs this checkout; \
                 `uf build --adapter node` writes a directory that does not",
                resolved.config.build.out_dir
            ),
        },
    }
}

/// The files an adapter writes beside its copy of the output directory.
///
/// `handler.js` is in every one of them, which is the seam; what changes is
/// the entry wrapped around it and the platform file, if any, beside it.
fn adapter_entries(adapter: DeployAdapter) -> &'static str {
    match adapter {
        DeployAdapter::Node => "handler.js, server.js",
        DeployAdapter::Container => "handler.js, server.js, Dockerfile",
        DeployAdapter::Edge => "handler.js, worker.js, wrangler.json",
        DeployAdapter::Serverless => "handler.js, lambda.js",
        // `uf build` refuses these, so this is what `uf explain build` says
        // about a project that has configured one: the same "nothing", named.
        DeployAdapter::Bun | DeployAdapter::Deno | DeployAdapter::Static => "nothing yet",
    }
}

/// `uf preview`, whose whole question is who answers a request.
///
/// Both servers are named in both plans, because the thing a reader wants to
/// know here is which one they are talking to — the two exist precisely
/// because one of them has Vite in the loop and the other does not.
fn preview_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    vec![
        Stage {
            name: "configuration",
            provider: "uf".to_string(),
            detail: "uf.config.js, with `vite` merged over what uf generates".to_string(),
        },
        host_stage(resolved),
        env_stage(resolved, PRODUCTION),
        Stage {
            name: "server",
            provider: format!("{} (preview)", builder_provider(resolved)),
            detail: format!(
                "serves {} directly, with `vite.preview` in effect",
                resolved.config.build.out_dir
            ),
        },
        Stage {
            name: "requests vite did not answer",
            // A build that emits no server has nothing mounted behind Vite's
            // file server, which is what makes this preview the right check
            // for it. Naming `@uniflowed/server` here anyway would describe a
            // graph this command does not have — the same rule `start_stages`
            // follows about `@uniflowed/vite`.
            provider: match RenderingPlan::resolve(&resolved.config).emits_a_server() {
                true => "@uniflowed/server".to_string(),
                false => "none".to_string(),
            },
            detail: match RenderingPlan::resolve(&resolved.config).emits_a_server() {
                true => {
                    "route handlers, then a render — from .uf/build/server/server.js".to_string()
                }
                false => format!(
                    "nothing: {} emits no server, so a 404 here is the 404 a static host gives",
                    plan_key(resolved)
                ),
            },
        },
    ]
}

/// The config key that made this a build with no server, for a message that
/// has to name one.
fn plan_key(resolved: &ResolvedConfig) -> &'static str {
    RenderingPlan::resolve(&resolved.config)
        .source()
        .key()
        .unwrap_or("this project")
}

/// `uf start`, whose answer is that nothing here is Vite's.
///
/// Or that there is nothing to start. A project that emits no server is
/// refused by the command, so the plan says so here rather than describing a
/// socket nobody can bind — `uf explain` naming a stage that cannot run is the
/// black box red line 7 exists to prevent.
fn start_stages(resolved: &ResolvedConfig) -> Vec<Stage> {
    let plan = RenderingPlan::resolve(&resolved.config);
    if !plan.emits_a_server() {
        return vec![
            Stage {
                name: "configuration",
                provider: "uf".to_string(),
                detail: "uf.config.js; the build is read, not rebuilt".to_string(),
            },
            Stage {
                name: "server",
                provider: "none".to_string(),
                detail: format!(
                    "`uf start` refuses: {}. Deploy {} to a static host, or check it with \
                     `uf preview`",
                    plan.because(),
                    resolved.config.build.out_dir
                ),
            },
        ];
    }
    vec![
        Stage {
            name: "configuration",
            provider: "uf".to_string(),
            detail: "uf.config.js; the build is read, not rebuilt".to_string(),
        },
        host_stage(resolved),
        env_stage(resolved, PRODUCTION),
        Stage {
            name: "server",
            // `@uniflowed/server`, and no longer `@uniflowed/vite`: the socket,
            // the file lookup and the request translation moved there when the
            // first deploy adapter needed them, and a deployment may not link
            // the package named after the bundler. Naming the old one here
            // would be `uf explain` describing a graph uf no longer has.
            provider: "@uniflowed/server (node:http)".to_string(),
            detail: format!(
                "static files from {}, then route handlers, then a render",
                resolved.config.build.out_dir
            ),
        },
        Stage {
            name: "application",
            provider: "@uniflowed/router".to_string(),
            detail: ".uf/build/server/server.js, imported once at startup".to_string(),
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
        env_stage(resolved, TEST),
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
