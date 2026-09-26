//! `uf build`: banner, per-phase timings, a summary, and what was produced.
//!
//! The build belongs to the project's **builder** — `@uniflowed/vite` unless
//! `builder.module` names another — driven on the project's JavaScript host
//! (see [`super::builder`] for which one and [`super::vite`] for the protocol):
//! a client bundle, a server bundle, and the routes this project prerenders,
//! as HTML. uf's own phases run around it:
//! the config, the route table and its generated types, the server-component
//! analysis and its diagnostics, and — once Vite has written `dist/` — the
//! shipped-size report and the budgets it enforces.
//!
//! Two of those phases can fail the build: an RSC contract violation, before
//! Vite runs, and a bundle-size budget, after it.
//!
//! `--compile` adds one more phase after all of that, in [`super::compile`]:
//! the whole application, linked with an embedded copy of the output directory
//! and a JavaScript runtime, as one executable file. `--adapter` adds the
//! phase between the two, in [`super::deploy`]: a directory that runs on a
//! host with a JavaScript runtime and nothing else.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde_json::json;
use uf_bundle::{
    BudgetMetric, BundleBudgets, BundleReport, ByteSize, ReportOptions, build_report,
    collect_assets, evaluate, write_report,
};
use uf_config::{
    DeployAdapter, FrameworkPreset, LibraryPlan, Navigation, Prerender, RenderingPlan,
    RuntimeTarget, UniflowedConfig,
};
use uf_router::{
    Route, RouteTarget, discover_routes_for_target, discover_server_modules_for_target,
    write_router_manifest_for_target,
};
use uf_rsc::{
    BuildId, ProjectScanOptions, RSC_MANIFEST_BUILD_DIR, RSC_MANIFEST_ENV, RscAnalysis,
    RscDiagnostic, RscSeverity, analyze_project,
};
use uf_term::{
    Cell, CodeFrame, Column, DiagnosticLevel, KeyValue, PhaseTimer, Status, Table, Tone, Tree,
    format_duration,
};

use crate::commands::builder;
use crate::commands::compile;
use crate::commands::deploy;
use crate::commands::lint::identifier_span;
use crate::commands::runtimes;
use crate::commands::vite::{
    Driver, Event, LinkContext, load_project_config, render_error, render_log,
};
use crate::support::{
    PRODUCTION, plural, problem_summary, project_env, project_label, relative_to, write_json_file,
};
use crate::ui::Ui;

mod guards;
mod library;
mod native;
mod native_links;
mod nonce;
mod previous_assets;
mod request_state;
mod site;
mod spa;
pub(crate) mod watch;

/// How many assets `--size-report` names before the list is cut off.
const LARGEST_ASSETS_SHOWN: usize = 20;

/// Where the build writes its notes about itself.
///
/// The output directory is what gets deployed, and everything in it is served
/// — by a static host, by `uf preview`, and by `uf start`. So the line this
/// directory draws is **who the file is for**:
///
/// * the output directory is for the *visitor*: documents, chunks, styles,
///   the metadata files a crawler fetches, and `.vite/manifest.json`, which
///   the server reads off disk at startup and which is Vite's own file in
///   Vite's own place;
/// * this directory is for *whoever ran the build*: `uf-build-manifest.json`,
///   `uf-rsc-manifest.json`, `uf-bundle-report.json` and `openapi.json`.
///   Nothing reads them to answer a request. Between them they name every
///   route including the ones that were never prerendered, the source file
///   behind each one, the size of every chunk, and the route-handler contract
///   declared for API clients — a map of an application, handed to anyone who
///   guesses the filename. See ubugeeei-prod/uf#339.
///
/// Beside `.uf/build/server`, `.uf/build/compile` and `.uf/build/deploy`,
/// which the other halves of the build already use, and outside the output
/// directory so `emptyOutDir` cannot sweep it away.
///
/// A deploy step that wants these files still has them; it copies `dist/`, and
/// this is one directory up from there rather than gone.
const BUILD_META_DIR: &str = ".uf/build/meta";

/// One document the prerender wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prerendered {
    /// The URL it was rendered for.
    pub(crate) url: String,
    /// The file it was written to, relative to the project root.
    pub(crate) file: String,
    /// The status the render answered with.
    ///
    /// Almost always `200`. The exception is `404.html`, rendered from the
    /// router-root not-found boundary and reported as `/404` — a document that
    /// is served and is not a page, which is a distinction [`site`] needs and
    /// nothing else did until it existed.
    pub(crate) status: u16,
    /// Whether the build wrote it for regeneration.
    ///
    /// Such a page's document is under `dist/__uf/regenerate/` rather than at
    /// its own URL, because a server answers the page and regenerates it once
    /// its lifetime passes. A static host has no server, which is why
    /// `deploy::static_host` refuses it by name.
    pub(crate) regenerates: bool,
    /// Whether the build wrote it as a static shell.
    ///
    /// Such a page read the request inside a `<Suspense>` boundary, so the
    /// build wrote the markup around the boundary and left the boundary for a
    /// server to fill per request. `file` is the server's record of the shell
    /// beside the server bundle, and nothing is at the page's URL under
    /// `dist/`. A static host has no server to fill it, which is why
    /// `deploy::static_host` refuses it by name.
    pub(crate) partial: bool,
}

/// What Vite reported building.
#[derive(Debug, Default)]
struct ViteBuild {
    /// Prerendered pages, in the order the prerender reported them.
    pages: Vec<Prerendered>,
    /// Warnings Vite logged, shown after the summary.
    warnings: Vec<String>,
    /// Every route this build wrote no document for, from the driver.
    ///
    /// Reported rather than derived: which routes a prerender skipped depends
    /// on what each page module exports, and `uf` does not evaluate page
    /// modules. `None` when the builder said nothing, which is what an older
    /// `@uniflowed/vite` does.
    per_request: Option<Vec<String>>,
    /// How the client route table came out of the server-component split.
    ///
    /// `(pages kept, routes)`, reported by the plugin that generated the table
    /// rather than computed here. `None` when the bundler said nothing, which
    /// is what an older `@uniflowed/vite` does; the summary then omits the row
    /// rather than printing a number nothing produced.
    split: Option<(u64, u64)>,
}

/// Show the client route count only when the split actually removed a page.
fn split_summary(split: Option<(u64, u64)>) -> Option<String> {
    split
        .filter(|(pages, routes)| pages < routes)
        .map(|(pages, routes)| format!("{pages} of {routes}"))
}

/// What `uf build` writes about the bundles, beyond the bundles themselves.
///
/// One value rather than a flag apiece: both are reports on the same output,
/// asked for together as often as apart, and the build takes enough arguments
/// that two more booleans side by side are two to pass in the wrong order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BuildReports {
    /// `--size-report`: print what each chunk costs.
    pub(crate) size: bool,
    /// `--analyze`: write the per-route analysis and its page.
    pub(crate) analyze: bool,
}

pub(crate) fn build(
    cwd: &Utf8Path,
    ui: &mut Ui,
    reports: BuildReports,
    requested_mode: Option<&str>,
    standalone: bool,
    requested_target: Option<&str>,
    requested_adapter: Option<DeployAdapter>,
) -> Result<()> {
    let BuildReports {
        size: size_report,
        analyze,
    } = reports;
    let mut timer = PhaseTimer::start();
    let mut progress = ui.progress();

    progress.draw("loading configuration");
    let resolved = timer.measure("config", || {
        load_project_config(cwd, requested_mode, PRODUCTION)
    })?;

    // Which of the two builds this is, decided once. A project whose
    // `app.router.enabled` is false is a library, and everything below this
    // point — the route table, the RSC analysis, the client entry, the
    // prerender — is an application's. `uf new --lib` scaffolded a project
    // that ran all of it and failed on a missing `app.js`; see
    // ubugeeei-prod/uf#268 and [`library`].
    if let Some(plan) = LibraryPlan::resolve(&resolved.config) {
        progress.finish();
        drop(progress);
        refuse_an_application_artefact(
            &plan,
            standalone,
            requested_adapter.or(resolved.config.app.runtime.deploy.adapter),
            requested_target,
        )?;
        if analyze {
            anyhow::bail!(
                "`uf build --analyze` attributes modules to routes, and this project builds a \
                 library, which has none; `--size-report` measures what it emits"
            );
        }
        return library::build(ui, timer, &resolved, &plan, requested_mode, size_report);
    }

    let root = resolved.root.clone();
    let app_target =
        application_target(&resolved.config, requested_target, standalone, "uf build")?;
    if app_target != RouteTarget::Web {
        progress.finish();
        drop(progress);
        if standalone
            || requested_adapter
                .or(resolved.config.app.runtime.deploy.adapter)
                .is_some()
            || analyze
        {
            bail!(
                "native Metro builds do not support --compile, --adapter or --analyze; native compilation and signing belong to Expo/EAS, Xcode or Gradle"
            );
        }
        return native::build(ui, &resolved, app_target, requested_mode);
    }
    // What this project said a build may produce, resolved once. Two settings
    // decide it — `app.rendering.modes` and `build.staticBuild` — and reading
    // them apart at the four places below is how they would come to disagree;
    // see `uf_config`'s `RenderingPlan`.
    let plan = RenderingPlan::resolve(&resolved.config);

    progress.tick("discovering routes");
    let routes = timer.measure("routes", || {
        discover_routes_for_target(&resolved.root, &resolved.config, app_target)
    })?;
    // A native target's route table is a module per platform beside the web
    // router's `router.js`, rather than `router.js` itself: Metro gives an iOS
    // bundle that imports `./router` the `router.ios.js` table, and a project
    // with both targets keeps its web route types. See ubugeeei-prod/uf#981.
    let router_modules = timer.measure("router types", || {
        if app_target == RouteTarget::Web {
            write_router_manifest_for_target(&resolved.root, &resolved.config, app_target)
                .map(|manifest| manifest.into_iter().collect::<Vec<_>>())
        } else {
            uf_router::native::write_native_router_modules(&resolved.root, &resolved.config)
                .map(|modules| modules.files)
        }
    })?;
    // The other half of the same tree: the route handlers and middleware,
    // which have no page and so appear in no `Route`. Only `--adapter static`
    // reads them — a project that needs a server is one this target has to
    // refuse by name — and the walk is one pass over a directory that was just
    // walked, so it is done here rather than made conditional on a flag.
    let server_modules = timer.measure("server modules", || {
        discover_server_modules_for_target(&resolved.root, &resolved.config, app_target)
    })?;

    let out_dir = resolved.root.join(resolved.config.build.out_dir.as_str());
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {out_dir}"))?;

    progress.tick("analysing server components");
    let rsc = timer.measure("rsc analysis", || {
        analyze_project(
            &resolved.root,
            &BuildId::from_env_or_generate(),
            &ProjectScanOptions::default(),
        )
    })?;

    // Before Vite, not after: a module that breaks the RSC contract is not
    // going to be fixed by bundling it, and a build that spends thirty seconds
    // on the bundle before saying so is thirty seconds of the wrong answer.
    // The count in the summary below is what this used to be — `rsc
    // diagnostics 5`, exit 0, and the messages in a JSON file nobody reads.
    // See ubugeeei-prod/uf#281.
    //
    // The graph's own findings, and one it cannot make alone: a route whose
    // document is written once — prerendered, or kept in the route cache —
    // and whose render reads the request. That takes the route table and the
    // rendering plan beside the graph; see [`request_state`].
    let mut diagnostics = rsc.graph.diagnostics().to_vec();
    diagnostics.extend(request_state::request_state_in_static_routes(
        &resolved.root,
        &resolved.config,
        plan,
        app_target,
        &routes,
        &rsc.graph,
    ));
    diagnostics.sort_by(|left, right| {
        left.module()
            .cmp(right.module())
            .then(left.line().cmp(&right.line()))
            .then(left.rule().cmp(right.rule()))
    });
    if !diagnostics.is_empty() {
        progress.finish();
        render_rsc_diagnostics(ui, &root, &diagnostics);
        let errors = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity() == RscSeverity::Error)
            .count();
        if errors > 0 {
            bail!(
                "{}",
                plural(errors, "React Server Components contract violation")
            );
        }
    }

    // The bundler's copy of the analysis, and the reason it is written here
    // rather than beside the record written below. `@uniflowed/vite` decides
    // which routes keep their page module *while it is emitting the bundle*,
    // so it needs the manifest before this build has produced anything; the
    // copy in [`BUILD_META_DIR`] is written when the build is over and is the
    // one a reader should trust, because `uf dev` rewrites this one on every
    // edit. The path is handed to the driver as `UF_RSC_MANIFEST`.
    let rsc_input = uf_rsc::write_manifest(&root.join(RSC_MANIFEST_BUILD_DIR), &rsc.manifest())?;

    progress.tick("resolving the JavaScript host");
    // The progress line is finished before anything is said, or the sentence
    // would be written onto the end of the spinner's line.
    let build_runtime = runtimes::resolve(&resolved, runtimes::Role::Build, &mut |message| {
        progress.finish();
        ui.render_err(|renderer, out| renderer.status(out, Status::Info, message));
    })?;
    let host = build_runtime.host.clone();
    let builder = builder::resolve(&root, &resolved.config)?;
    // `production` unless the project or the command line said another mode,
    // which is what selects `.env.production` over `.env.development` — the
    // half of ubugeeei-prod/uf#259 that made a build and a dev server disagree
    // about the same variable.
    let env = build_runtime.environment(project_env(&resolved, requested_mode, PRODUCTION)?);

    // Asked for before anything is built. `--compile` on a machine with no
    // usable backend fails either way; failing now costs the user nothing, and
    // failing after the bundle costs them the build. The same call resolves
    // `--target`, so an unknown triple, a triple the chosen backend cannot
    // cross-compile to, and a permission set it cannot enforce are all refused
    // here rather than a minute later.
    // Every backend that could produce this binary, in the order the project's
    // hosts are accepted. More than one because `--build-sea` can fail for a
    // reason that is about the `node` binary rather than about the application
    // — see `compile::wrap` — and a build that has already been told it may
    // infer a host should not stop over that.
    let runtimes = if standalone {
        timer.measure("runtime", || {
            let declared_runtime = if resolved.config.runtime_tool().is_some() {
                Some(runtimes::resolve(
                    &resolved,
                    runtimes::Role::Runtime,
                    &mut |message| {
                        progress.finish();
                        ui.render_err(|renderer, out| renderer.status(out, Status::Info, message));
                    },
                )?)
            } else {
                None
            };
            compile::runtimes(
                &root,
                &resolved.config,
                requested_target,
                declared_runtime.as_ref(),
            )
        })?
    } else {
        Vec::new()
    };
    let runtime = runtimes.first();
    // And said out loud when producing the binary needs the network. A build
    // that stops for ninety megabytes in the middle should have announced it
    // at the start; `uf build` owns the terminal, and a silent minute is not
    // one of the things it renders.
    if let Some(runtime) = runtime
        && let Some(target) = runtime.target
        && runtime.fetches_a_runtime()
    {
        let notice = format!(
            "the {} runtime for {} is not cached yet; producing this binary downloads it (about \
             90 MB) into .uf/cache/bun",
            runtime.backend.name(),
            target.triple
        );
        ui.render(|renderer, out| renderer.status(out, Status::Info, &notice));
    }
    // The same rule for the same reason: an adapter nobody has written is a
    // sentence, and a sentence is cheaper before the bundle than after it.
    // Not refused for a build that emits no server, and the distinction is
    // worth being explicit about. `build.staticBuild` is a claim about what
    // the *ordinary* build leaves behind, and `.uf/build/server` is what it
    // removes; `--adapter` and `--compile` link the application again from
    // source and read none of it, so each still produces a correct artefact —
    // a static site inside a Worker, or inside one executable, is a
    // deployment somebody wants. What such a project cannot do is `uf start`,
    // which has a bundle to load and does not have it; that is refused in
    // `commands::serve`, where it is a fact rather than an opinion.
    let adapter = deploy::resolve(&resolved.config.app.runtime.deploy, requested_adapter)?;
    // Before the build rather than after it: a project whose scheduled work
    // this target would never run should hear so in a second, not after a
    // bundle. ubugeeei-prod/uf#531.
    let declared_schedules = match adapter {
        Some(adapter) => {
            let found = deploy::schedules::discover_schedules(&root, &resolved.config)?;
            deploy::schedules::refuse_unrunnable(adapter, &found)?;
            found
        }
        None => Vec::new(),
    };
    // The fourth thing that needs a process, and the only one `uf` can see
    // without evaluating a module. Checked here rather than in the builder for
    // exactly that reason — see `refuse_unanswerable_actions`.
    refuse_unanswerable_actions(plan, &rsc, adapter, standalone)?;
    // And everything else that needs one, for the one plan whose builder will
    // never find it. The other three prerender something, so a page that reads
    // a request throws while it is being rendered and the build says so; a
    // shell build renders no route at all, so the same page would build,
    // deploy, and fail in a browser. See [`spa`].
    //
    // Not lifted by `--adapter` or `--compile`, unlike the actions above, and
    // the difference is which declaration each is about. `staticBuild` is a
    // claim about the *artefact*, so a build that also emits a server has
    // honoured it; `modes: ["csr"]` is a claim about the **routes** — that
    // every one of them is rendered in a browser — and that is still true
    // inside a Worker or an executable, because neither of them writes the
    // per-route documents the shell exists instead of.
    // The config file `app.router`'s rules are named by when a target refuses
    // them, relative to the project like every other file a refusal names.
    let config_file = resolved.config_path.as_deref().map_or_else(
        || String::from("uf.config.js"),
        |file| relative_to(&resolved.root, file),
    );
    if plan.prerender() == Prerender::Shell {
        spa::refuse(
            &resolved.root,
            &routes,
            &server_modules,
            &rsc.graph,
            spa::rule_findings(&resolved.config.app.router, &config_file),
            &plan.because(),
        )?;
    }

    // What an earlier `--analyze` left must not be read as this build's. The
    // graph is the builder's to write again, and an analysis beside a build it
    // does not describe is worse than none.
    let analysis_dir = resolved.root.join(BUILD_META_DIR);
    for stale in [
        uf_bundle::MODULE_GRAPH_FILE,
        uf_bundle::ANALYSIS_FILE,
        uf_bundle::ANALYSIS_VIEW_FILE,
    ] {
        let path = analysis_dir.join(stale);
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error).with_context(|| format!("failed to remove {path}"));
        }
    }

    // The previous build's hashed files, set aside before the builder empties
    // the output directory and put back after the size report — so a tab still
    // on that build keeps finding its chunks for one more build. See
    // [`previous_assets`].
    let previous = previous_assets::stash(&resolved.root, &out_dir)?;

    progress.tick("building with vite");
    let vite = timer.measure("vite", || -> Result<ViteBuild> {
        let mut driver = Driver::spawn(
            &host,
            &builder,
            &root,
            "build",
            &build_arguments(
                &resolved.config.build.out_dir,
                plan,
                app_target,
                regenerates_pages(&resolved.config, plan),
                prerenders_partially(&resolved.config, plan),
                analyze,
            ),
            &env,
            &[(RSC_MANIFEST_ENV, rsc_input.as_str())],
        )?;
        let mut report = ViteBuild::default();
        while let Some(event) = driver.next_event()? {
            match event {
                Event::Phase { name } => progress.tick(&format!("vite: {name}")),
                Event::Page {
                    url,
                    file,
                    status,
                    regenerates,
                    partial,
                    ..
                } => report.pages.push(Prerendered {
                    url,
                    file,
                    status,
                    regenerates,
                    partial,
                }),
                // Reported as it happens and not fatal here: the driver keeps
                // going and ends the build itself, so the reader sees every
                // route that failed rather than the first one.
                Event::PageFailed { url, error } => {
                    render_log(
                        ui,
                        crate::commands::vite::LogLevel::Error,
                        &format!("{url} failed to render"),
                    );
                    let _ = render_error(ui, &root, &error);
                }
                Event::Rendering { per_request, .. } => report.per_request = Some(per_request),
                Event::RscSplit { pages, routes } => report.split = Some((pages, routes)),
                Event::Log { level, message } => match level {
                    crate::commands::vite::LogLevel::Warn => report.warnings.push(message),
                    crate::commands::vite::LogLevel::Error => render_log(ui, level, &message),
                    crate::commands::vite::LogLevel::Info => {}
                },
                Event::Error(error) => {
                    let failure = render_error(ui, &root, &error);
                    let _ = driver.finish("uf build");
                    return Err(failure);
                }
                // A build has no watcher and no browser, so `SourceChanged`,
                // `EnvChanged` and `Diagnostic` never reach it; they are in the
                // match because the driver's channel is one vocabulary and
                // every reader has to know the whole of it.
                Event::ConfigLoaded { .. }
                | Event::Listening { .. }
                | Event::SourceChanged
                | Event::EnvChanged { .. }
                | Event::Diagnostic(_)
                | Event::Done { .. }
                | Event::Config { .. } => {}
            }
        }
        driver.finish("the Vite build")?;
        Ok(report)
    })?;

    // Which of the documents Vite just wrote are under a `$middleware.js`.
    // Answerable only here, because the pages are what the prerender produced
    // rather than what the route table said it might; the reason it is a
    // report and not a refusal is argued in [`guards`].
    let unguarded = guards::unguarded_pages(&resolved.root, &routes, &vite.pages);

    // Written after Vite so a stale copy cannot be read as this build's, and
    // so the manifest describes the build that actually happened.
    progress.tick("writing manifests");
    let meta_dir = resolved.root.join(BUILD_META_DIR);
    fs::create_dir_all(&meta_dir).with_context(|| format!("failed to create {meta_dir}"))?;
    let build_manifest = meta_dir.join("uf-build-manifest.json");
    let openapi_document = meta_dir.join("openapi.json");
    let association_files =
        native_links::write(&resolved.root, &out_dir, &resolved.config, &routes)?;
    let payload = json!({
        "version": 2,
        "engine": "vite",
        "transform": "uf transform",
        "host": host.name(),
        "target": app_target.as_str(),
        "targetContract": target_contract(app_target),
        "entries": resolved.config.build.entries,
        "nativeLinks": resolved.config.app.router.native_links,
        "associationFiles": association_files.iter().map(|file| relative_to(&resolved.root, file)).collect::<Vec<_>>(),
        "routes": routes.iter().map(|route| json!({
            "path": route.path,
            "page": relative_to(&resolved.root, &route.page),
            "params": route.params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "pages": vite.pages.iter().map(|page| json!({ "url": page.url, "file": page.file })).collect::<Vec<_>>(),
        // The same list the summary warns about, as data: which deployment of
        // `dist/` is happening is a fact the build does not have, and a deploy
        // step that does have it needs somewhere to read this from that is not
        // a terminal.
        "prerenderedUnderMiddleware": unguarded.iter().map(|page| json!({
            "url": page.url,
            "file": page.file,
            "middleware": page.middleware,
        })).collect::<Vec<_>>(),
        // What this build decided to render when, and every route it wrote no
        // document for. The same list the summary prints, as data: a deploy
        // step needs to know whether the output directory is the whole
        // application or half of it, and that is not a question it can answer
        // by looking at the files.
        "rendering": {
            "prerender": plan.prerender().as_str(),
            "server": plan.emits_a_server(),
            "declaredBy": plan.source().key(),
            "perRequest": vite.per_request.clone().unwrap_or_default(),
            // Beside the prerender because a deploy step asks the same
            // question about it: whether what is in the output directory is
            // answered by the browser or by the documents alone. `client`
            // for every project that has not set it, so the field is a fact
            // rather than a presence to test for.
            "navigation": plan.navigation().as_str(),
        },
        "runtime": {
            "default": resolved.config.app.runtime.default,
            "capabilityJsHost": &resolved.config.app.runtime.capability_js_host,
        },
        "cache": {
            "route": resolved.config.app.rendering.cache.route,
            "fetch": resolved.config.app.rendering.cache.fetch,
            "data": resolved.config.app.rendering.cache.data,
            // Where the entries go, for a deploy step that has to provision it:
            // `"filesystem"` needs a writable directory that outlives the
            // process, and a module specifier needs whatever that module
            // connects to. Absent means memory, which needs nothing.
            "store": resolved.config.app.rendering.cache.store.as_deref(),
            "storeDir": resolved.config.app.rendering.cache.store_dir.as_deref(),
        },
    });
    timer.measure("manifest", || write_json_file(&build_manifest, &payload))?;
    let rsc_manifest = timer.measure("rsc manifest", || {
        uf_rsc::write_manifest(&meta_dir, &rsc.manifest())
    })?;

    // `sitemap.xml` and `robots.txt`, from the documents the prerender just
    // reported. Into the output directory rather than beside it — unlike the
    // three manifests above, these two exist to be fetched — and before the
    // size report, so what the report measures is everything a visitor can
    // ask for. See [`site`] for which URLs go in and which do not.
    progress.tick("writing metadata files");
    let metadata_files = timer.measure("metadata", || {
        site::write(
            &out_dir,
            &resolved.config.site,
            &resolved.config.app.router,
            &vite.pages,
            &unguarded,
        )
    })?;

    progress.tick("measuring shipped assets");
    let (size, size_report_path) = timer.measure("bundle size", || -> Result<_> {
        let assets = collect_assets(&out_dir, &ReportOptions::default())?;
        let report = build_report(assets, &route_assets(&out_dir, &routes));
        let path = write_report(&meta_dir, &report)?;
        Ok((report, path))
    })?;
    // After the report, which measures what *this* build ships, and before
    // `--compile` and `--adapter`, which copy the output directory and should
    // carry the previous build's chunks with it.
    // Always, so the record in `.uf/build/meta` is this build's even when
    // there was nothing to keep.
    let carried = timer.measure("previous assets", || {
        previous_assets::restore(previous, &out_dir, &meta_dir)
    })?;
    // `--analyze`: the graph the builder just wrote, attributed to routes.
    // After the size report, which it complements rather than repeats: that
    // says what each shipped file weighs, and this says why each module is in
    // one.
    let analysis = if analyze {
        progress.tick("analyzing the bundles by route");
        Some(timer.measure("analysis", || {
            analyze_bundles(&resolved.root, &meta_dir, &routes)
        })?)
    } else {
        None
    };
    // After the size report and not before it: the binary is written into the
    // output directory, and an executable counted among the shipped assets
    // would put every budget in `uf.config.js` permanently over.
    // The two link steps below take the same six answers, so they are built
    // once: the second Vite run of a build has to see what the first one saw.
    let link = LinkContext {
        host: &host,
        builder: &builder,
        root: &root,
        out_dir: &out_dir,
        env: &env,
        rsc_manifest: &rsc_input,
    };
    let compiled = match runtime {
        Some(runtime) => {
            // The download is a phase of its own in the line the reader is
            // watching, because it is the one step of a build whose duration
            // has nothing to do with the size of their application.
            progress.tick(&match (runtime.target, runtime.fetches_a_runtime()) {
                (Some(target), true) => {
                    format!("fetching the {} runtime, then compiling", target.triple)
                }
                (Some(target), false) => {
                    format!("compiling a standalone binary for {}", target.triple)
                }
                (None, _) => String::from("compiling a standalone binary"),
            });
            Some(timer.measure("compile", || compile::compile(ui, &runtimes, link))?)
        }
        None => None,
    };
    // After the binary, so that a build asked for both copies the binary's
    // exclusion rather than the binary itself: `--compile` writes into
    // `dist/`, and `deploy` copies `dist/`.
    let deployed = match adapter {
        // The one target that links nothing. `dist/` is already what a static
        // host serves, so what this does instead of a second Vite run is
        // decide whether this project can be served that way at all — and say
        // which routes cannot be, rather than publishing the half that can.
        Some(DeployAdapter::Static) => {
            progress.tick("writing the static adapter's output");
            let actions = rsc
                .registry
                .callable_actions()
                .map(|action| (action.export.to_string(), action.module.clone()))
                .collect::<Vec<_>>();
            let site = deploy::SiteFacts {
                routes: &routes,
                server_modules: &server_modules,
                pages: &vite.pages,
                actions: &actions,
                router: &resolved.config.app.router,
                config_file: &config_file,
            };
            Some(timer.measure("adapter", || deploy::deploy_static(&root, &out_dir, site))?)
        }
        Some(adapter) => {
            progress.tick(&format!(
                "writing the {} adapter's output",
                adapter.as_str()
            ));
            Some(timer.measure("adapter", || {
                deploy::deploy(ui, adapter, link, &declared_schedules)
            })?)
        }
        None => None,
    };

    progress.finish();
    drop(progress);

    let total = timer.total();
    let entries = resolved
        .config
        .build
        .entries
        .iter()
        .map(|entry| entry.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let route_count = routes.len().to_string();
    let page_count = vite.pages.len().to_string();
    let module_count = rsc.graph.modules().len().to_string();
    let client_count = rsc.graph.client_boundaries().len().to_string();
    // "1 of 2", not a percentage and not a bare count: the reader is being told
    // how much of the route table the browser was handed, and both halves of
    // that are the fact.
    //
    // Only when the split removed something, which is the same rule the guards
    // table below follows: a row saying `29 of 29` is a row that reports no
    // finding, on every build of every application whose root layout imports
    // one client component. The absence of the row means what it meant before
    // any of this existed — every page went to the browser.
    let split_count = split_summary(vite.split);
    let action_count = rsc.callable_action_count().to_string();
    let kept = (carried > 0).then(|| format!("{} (for one build)", plural(carried, "file")));
    // What the build decided, in the words a reader can act on. Named in the
    // summary rather than left to be inferred from a page count, because "the
    // build wrote no document for /posts/:slug" and "the build is broken" look
    // identical from `dist/`.
    let rendering = match plan.prerender() {
        Prerender::Everything => "every route prerendered",
        Prerender::Possible => "prerendered where it can be, the rest per request",
        Prerender::Nothing => "nothing prerendered; every route per request",
        Prerender::Shell => "one shell prerendered; every route rendered in the browser",
    };
    // And what it decided about the other axis. A row rather than a line only
    // when it is not the default: `app.rendering.navigation` is `client` in
    // every project that has not heard of it, and a summary row saying so on
    // every build is a row nobody reads. A build that ships no client router
    // has to say so — it is the difference between a link that resolves in the
    // page and a link that fetches a document, and nothing in `dist/` shows
    // which one happened.
    let navigation = match plan.navigation() {
        Navigation::Client => None,
        Navigation::Document => Some("a document request; this build ships no client router"),
    };
    let per_request = vite.per_request.clone().unwrap_or_default();
    let per_request_count = per_request.len().to_string();
    let diagnostic_count = rsc.graph.diagnostics().len().to_string();

    let mut outputs = vec![
        relative_to(&resolved.root, &build_manifest),
        relative_to(&resolved.root, &rsc_manifest),
        relative_to(&resolved.root, &size_report_path),
    ];
    if openapi_document.is_file() {
        outputs.push(relative_to(&resolved.root, &openapi_document));
    }
    if let Some((analysis_json, analysis_view)) = &analysis {
        outputs.push(relative_to(&resolved.root, analysis_json));
        outputs.push(relative_to(&resolved.root, analysis_view));
    }
    for module in &router_modules {
        outputs.push(relative_to(&resolved.root, module));
    }
    for page in &vite.pages {
        outputs.push(page.file.clone());
    }
    for file in &metadata_files.files {
        outputs.push(relative_to(&resolved.root, file));
    }
    for file in &association_files {
        outputs.push(relative_to(&resolved.root, file));
    }
    if let Some(compiled) = &compiled {
        outputs.push(relative_to(&resolved.root, &compiled.binary));
    }
    if let Some(deployed) = &deployed {
        outputs.push(relative_to(&resolved.root, &deployed.directory));
    }
    outputs.sort();
    outputs.dedup();
    let output_paths = outputs.iter().map(String::as_str).collect::<Vec<_>>();
    let project = project_label(&resolved.root).to_string();
    let phases = timer.phases().to_vec();
    let summary = format!("build succeeded in {}", format_duration(total));

    let asset_count = size.assets.len().to_string();
    let raw = size.total.raw.to_string();
    let gzip = size.total.gzip.to_string();
    let brotli = size.total.brotli.to_string();
    let largest: Vec<(String, &str, String, String)> = if size_report {
        size.largest_assets(BudgetMetric::Gzip)
            .iter()
            .take(LARGEST_ASSETS_SHOWN)
            .map(|asset| {
                (
                    asset.path.to_string(),
                    asset.kind.as_str(),
                    asset.size.gzip.to_string(),
                    asset.size.raw.to_string(),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut warnings = vite.warnings.clone();
    // In the report rather than before the build, where a line on stdout would
    // be drawn over by the progress on stderr.
    warnings.extend(resolved.config.builder_module_deprecation());
    // Said rather than assumed. A project with a `public/robots.txt` gets the
    // one it wrote, and the silent version of that is a person reading
    // `site.robots` in `uf.config.js` and wondering why none of it applies.
    for kept in &metadata_files.kept {
        warnings.push(format!(
            "{} was already in the output directory, so `site` did not write it",
            relative_to(&resolved.root, kept)
        ));
    }
    // Prerendered documents under a rule whose value names `{uf.nonce}`. A
    // nonce is minted per request and the file was written without one, so the
    // policy refuses uf's own client entry and the page never hydrates — in
    // production only, which is why ubugeeei-prod/uf#1171 is worth a line here.
    //
    // **This is the only place the level is read.** `nonce::REFUSES_THE_BUILD`
    // decides whether that sentence is a refusal or a warning, and the sentence
    // is the same either way, so changing uf's mind is that constant and
    // nothing at this call site. It is read *here*, after the build, because
    // the check is driven from the documents Vite actually wrote rather than
    // from the route table; [`nonce`] argues why that is worth a refusal which
    // arrives later than this file's others.
    let nonced = nonce::nonced_pages(&vite.pages, &resolved.config.app.router.headers);
    if !nonced.is_empty() {
        let said = nonce::message(&nonced, &config_file);
        if nonce::REFUSES_THE_BUILD {
            bail!("{said}");
        }
        warnings.push(said);
    }
    let guarded_rows: Vec<(String, String, String)> = unguarded
        .iter()
        .map(|page| {
            (
                page.url.clone(),
                page.file.clone(),
                page.middleware.join(", "),
            )
        })
        .collect();
    let guarded_summary = format!(
        "{} prerendered to {} a host serves without running the middleware that guards {}",
        plural(guarded_rows.len(), "route"),
        if guarded_rows.len() == 1 {
            "a document"
        } else {
            "documents"
        },
        if guarded_rows.len() == 1 {
            "it"
        } else {
            "them"
        },
    );
    let host_name = host.name();
    let adapter_summary = deployed.as_ref().map(|deployed| {
        let directory = relative_to(&resolved.root, &deployed.directory);
        (
            deployed.adapter.as_str(),
            directory.clone(),
            deployed.files.to_string(),
            ByteSize::from_bytes(deployed.bytes).to_string(),
            // The command, spelled out, because the whole claim of this
            // directory is that nothing else is needed to run it — and a
            // reader who has to guess whether it is `node server.js` or
            // `npm start` does not yet believe that claim. It differs per
            // adapter, and for the two that are uploaded rather than started
            // it is the upload; see `deploy::next_command`.
            deploy::next_command(deployed.adapter, &resolved.root, &directory),
        )
    });
    // Which runtime went inside, which machine it is for, and whether making
    // it reached the network. The first of those was a constant until
    // ubugeeei-prod/uf#312 gave the flag a second backend; it is a row rather
    // than a word in the documentation because "compiled" now means one of two
    // different `node:` API surfaces, and which one is a fact about the
    // artefact a person is about to deploy.
    let binary = compiled.as_ref().map(|compiled| {
        (
            relative_to(&resolved.root, &compiled.binary),
            compiled.runtime.label(),
            compiled
                .runtime
                .source
                .as_ref()
                .map(|source| source.key.clone()),
            compiled.runtime.target.map(|target| target.triple),
            ByteSize::from_bytes(compiled.bytes).to_string(),
            compiled.embedded.files.to_string(),
            ByteSize::from_bytes(compiled.embedded.bytes).to_string(),
            compiled
                .fetched
                .map(|bytes| ByteSize::from_bytes(bytes).to_string()),
        )
    });

    ui.render(|renderer, out| {
        renderer.banner(out, "uf build", Some(&project));
        renderer.timings(out, 2, &phases, Some(total));
        renderer.blank(out);
        let mut summary_rows = vec![
            KeyValue::new("engine", "vite"),
            KeyValue::toned("host", host_name, Tone::Muted),
            KeyValue::toned("target", app_target.as_str(), Tone::Muted),
            KeyValue::new("entries", &entries),
            KeyValue::toned("routes", &route_count, Tone::Number),
            KeyValue::toned("prerendered pages", &page_count, Tone::Number),
            KeyValue::toned("modules", &module_count, Tone::Number),
            KeyValue::toned("client components", &client_count, Tone::Number),
        ];
        if let Some(split) = &split_count {
            summary_rows.push(KeyValue::toned(
                "pages in the client bundle",
                split,
                Tone::Number,
            ));
        }
        summary_rows.push(KeyValue::toned(
            "server actions",
            &action_count,
            Tone::Number,
        ));
        summary_rows.push(KeyValue::toned(
            "rsc diagnostics",
            &diagnostic_count,
            Tone::Number,
        ));
        summary_rows.push(KeyValue::new("rendering", rendering));
        // Only when something was kept: a first build, or a fresh checkout,
        // has no previous build and no row to report it.
        if let Some(kept) = &kept {
            summary_rows.push(KeyValue::toned(
                "kept from the last build",
                kept,
                Tone::Muted,
            ));
        }
        if let Some(navigation) = navigation {
            summary_rows.push(KeyValue::new("navigation", navigation));
        }
        renderer.key_values(out, 2, &summary_rows);
        renderer.blank(out);

        renderer.heading(out, 2, "shipped");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::toned("assets", &asset_count, Tone::Number),
                KeyValue::toned("raw", &raw, Tone::Number),
                KeyValue::toned("gzip", &gzip, Tone::Accent),
                KeyValue::toned("brotli", &brotli, Tone::Number),
            ],
        );
        if !largest.is_empty() {
            renderer.blank(out);
            let mut table = Table::new(vec![
                Column::left("asset"),
                Column::left("kind"),
                Column::right("gzip"),
                Column::right("raw"),
            ]);
            for (path, kind, gzip, raw) in &largest {
                table.push(vec![
                    Cell::toned(path, Tone::Path),
                    Cell::toned(kind, Tone::Muted),
                    Cell::toned(gzip, Tone::Accent),
                    Cell::toned(raw, Tone::Number),
                ]);
            }
            renderer.table(out, 4, &table);
        }
        renderer.blank(out);

        if let Some((adapter, directory, files, bytes, run)) = &adapter_summary {
            renderer.heading(out, 2, "adapter");
            renderer.key_values(
                out,
                4,
                &[
                    KeyValue::toned("target", adapter, Tone::Muted),
                    KeyValue::toned("directory", directory, Tone::Path),
                    KeyValue::toned("files", files, Tone::Number),
                    KeyValue::toned("bytes", bytes, Tone::Accent),
                    KeyValue::new("run", run),
                ],
            );
            renderer.blank(out);
        }

        if let Some((path, runtime, runtime_key, target, bytes, files, embedded, fetched)) = &binary
        {
            renderer.heading(out, 2, "standalone");
            let mut rows = vec![
                KeyValue::toned("binary", path, Tone::Path),
                KeyValue::toned("runtime", runtime, Tone::Muted),
            ];
            if let Some(runtime_key) = runtime_key {
                rows.push(KeyValue::toned("runtime key", runtime_key, Tone::Muted));
            }
            // Only when `--target` was asked for. A row naming this machine on
            // every ordinary `--compile` is a row that reports no finding,
            // which is the same rule the guards table and the split count
            // below follow — and the reader who typed a triple is the one who
            // needs to see which one uf built.
            if let Some(target) = target {
                rows.push(KeyValue::toned("target", target, Tone::Accent));
            }
            rows.push(KeyValue::toned("bytes", bytes, Tone::Accent));
            rows.push(KeyValue::toned("embedded assets", files, Tone::Number));
            rows.push(KeyValue::toned("embedded bytes", embedded, Tone::Number));
            if let Some(fetched) = fetched {
                rows.push(KeyValue::toned("runtime downloaded", fetched, Tone::Number));
            }
            renderer.key_values(out, 4, &rows);
            renderer.blank(out);
        }

        renderer.heading(out, 2, "output");
        renderer.tree(
            out,
            4,
            &Tree::from_paths(&project, output_paths.iter().copied()),
        );
        renderer.blank(out);

        if !per_request.is_empty() {
            renderer.heading(out, 2, "answered by a server");
            renderer.key_values(
                out,
                4,
                &[KeyValue::toned("routes", &per_request_count, Tone::Number)],
            );
            let mut table = Table::new(vec![Column::left("route")]);
            for route in &per_request {
                table.push(vec![Cell::toned(route, Tone::Accent)]);
            }
            renderer.table(out, 4, &table);
            renderer.blank(out);
        }

        if !guarded_rows.is_empty() {
            renderer.heading(out, 2, "guards");
            let mut table = Table::new(vec![
                Column::left("route"),
                Column::left("document"),
                Column::left("middleware"),
            ]);
            for (url, file, middleware) in &guarded_rows {
                table.push(vec![
                    Cell::toned(url, Tone::Accent),
                    Cell::toned(file, Tone::Path),
                    Cell::toned(middleware, Tone::Path),
                ]);
            }
            renderer.table(out, 4, &table);
            renderer.blank(out);
        }

        for warning in &warnings {
            renderer.status(out, Status::Warn, warning);
        }
        if !guarded_rows.is_empty() {
            renderer.status(out, Status::Warn, &guarded_summary);
        }
        renderer.summary(out, Status::Success, &summary, &[]);
    });

    enforce_budgets(ui, &size, &resolved.config.build.budgets)
}

/// Refuse a build that leaves a callable server action with nowhere to run.
///
/// A `"use server"` export the browser can reach is an HTTP endpoint. The
/// client bundle carries a `createServerReference` for it — an id and a
/// `fetch` — so the button is wired either way; what a build with no server
/// removes is the thing at the other end. Nothing between that build and the
/// first person to click reports it, and what they get is whatever the static
/// host does with an unknown path. That is the same failure the rest of this
/// change is about, one layer below the route table: a `dist/` with a hole in
/// it, found from a 404.
///
/// # Why here and not in the builder
///
/// [`guards`] argues that the three route-shaped reasons a build
/// needs a server — a page with parameters and no `generateStaticParams`, a
/// route handler, a middleware — belong in **one** refusal, in the builder,
/// because telling a project to fix one, rebuild, and hear about the next is
/// three builds to learn three facts. A server action is not one of the three:
/// it is not a route, uf finds it without evaluating a module (the RSC
/// analysis has already run), and its fix is a different sentence. Refusing it
/// here costs the reader nothing they would otherwise have been told in the
/// same breath, and it happens before the bundle rather than after it.
///
/// # Why an adapter or `--compile` lifts it
///
/// Both link the application again from source and write something that can
/// answer a request, so the endpoint exists in what they produce.
/// `build.staticBuild` is a claim about the ordinary output directory, and a
/// build that also emits a server has honoured the claim and answered the
/// action. It is the same distinction the `deploy::resolve` call site makes
/// about the server bundle.
fn refuse_unanswerable_actions(
    plan: RenderingPlan,
    rsc: &RscAnalysis,
    adapter: Option<DeployAdapter>,
    standalone: bool,
) -> Result<()> {
    if plan.emits_a_server() || adapter.is_some() || standalone {
        return Ok(());
    }
    let mut listed = rsc
        .registry
        .callable_actions()
        .map(|action| format!("  {} — {}", action.module, action.export))
        .collect::<Vec<_>>();
    if listed.is_empty() {
        return Ok(());
    }
    // Sorted, because the registry's order follows the module scan and a
    // message that reorders itself between two builds of the same tree is a
    // message nobody can diff.
    listed.sort();
    bail!(
        "{} in this project {} callable from the browser, and {}\n{}\n\n\
         A `\"use server\"` export the browser can reach is an endpoint, and a deployment of \
         documents has nothing to answer it with. Keep the module out of the client's reach, \
         or drop `build.staticBuild` and deploy a server — `uf build --adapter <target>` and \
         `uf build --compile` each write one.",
        plural(listed.len(), "server action"),
        if listed.len() == 1 { "is" } else { "are" },
        plan.because(),
        listed.join("\n"),
    )
}

/// Refuse application-only build flags on a project that is a library.
///
/// `--compile` and `--adapter` produce a **deployment**: an executable that
/// serves the application, or a directory a host runs it from. `--target`
/// without `--compile` chooses the application surface whose routes uf should
/// discover. A library has no application to serve or route table to narrow, so
/// each would have to invent one, and what it invented would be an empty server
/// that starts and 404s everything.
///
/// Refused by name and before anything is built, which is the rule
/// ubugeeei-prod/uf#638 applied to the same class of flags: a target uf cannot
/// produce is a sentence, and a sentence is cheaper before the bundle than
/// after it.
/// The adapter is whichever of `--adapter` and `app.runtime.deploy.adapter`
/// asked, because a setting read and not honoured is the failure this whole
/// change is about: a project that declared a deploy target and got a library
/// would have been told nothing.
fn refuse_an_application_artefact(
    plan: &LibraryPlan,
    standalone: bool,
    adapter: Option<DeployAdapter>,
    requested_target: Option<&str>,
) -> Result<()> {
    let asked = match (standalone, adapter, requested_target) {
        (true, _, _) => "`uf build --compile` writes an executable that serves an application",
        (_, Some(_), _) => "a deploy adapter writes a directory a host serves an application from",
        (false, None, Some(_)) => "`uf build --target` chooses which application routes to build",
        (false, None, None) => return Ok(()),
    };
    bail!(
        "{asked}, and {}. A library is imported rather than served: `uf build` writes its \
         modules to the output directory, and what sends them anywhere is `uf publish`.",
        plan.because(),
    )
}

/// Which application target an ordinary build, or `uf dev`, resolves.
///
/// `--target` used to mean only the standalone binary's platform triple, so
/// `standalone` leaves that meaning with [`compile::runtimes`]. Without
/// `--compile`, it names the app surface instead: web, native, iOS or Android.
///
/// `command` is how the refusal names what was run, because `uf dev` and
/// `uf build` resolve a target by the same rule and a reader should see the
/// command they typed in the sentence that refuses it.
pub(crate) fn application_target(
    config: &UniflowedConfig,
    requested: Option<&str>,
    standalone: bool,
    command: &str,
) -> Result<RouteTarget> {
    let target = match requested.filter(|_| !standalone) {
        Some(requested) => parse_application_target(requested)?,
        None => default_application_target(config),
    };
    let needed = runtime_target_for_route_target(target);
    if config.app.targets.contains(&needed) {
        return Ok(target);
    }
    let declared = config
        .app
        .targets
        .iter()
        .map(runtime_target_name)
        .collect::<Vec<_>>()
        .join(", ");
    let declared = if declared.is_empty() {
        String::from("nothing")
    } else {
        declared
    };
    bail!(
        "`{command} --target {}` needs `app.targets` to include `{}`; this project declares {}",
        target.as_str(),
        runtime_target_name(&needed),
        declared,
    )
}

/// The target `uf build` and `uf dev` resolve when `--target` names none:
/// native for a `react-native` framework project, and web for every other.
pub(crate) fn default_application_target(config: &UniflowedConfig) -> RouteTarget {
    match config.app.framework {
        FrameworkPreset::ReactNative => RouteTarget::Native,
        FrameworkPreset::Uniflowed | FrameworkPreset::React => RouteTarget::Web,
    }
}

fn parse_application_target(requested: &str) -> Result<RouteTarget> {
    if let Ok(target) = requested.parse() {
        return Ok(target);
    }
    if compile::TARGETS
        .iter()
        .any(|target| target.triple == requested)
    {
        bail!(
            "`--target {requested}` is a platform target for `uf build --compile`; add \
             `--compile`, or choose an application target: web, native, ios or android"
        );
    }
    bail!(
        "`--target {requested}` is not an application target uf builds.\n  \
         accepted application targets: web, native, ios, android\n  \
         platform targets are accepted with `uf build --compile --target <triple>`"
    )
}

fn runtime_target_for_route_target(target: RouteTarget) -> RuntimeTarget {
    match target {
        RouteTarget::Web => RuntimeTarget::Web,
        RouteTarget::Native | RouteTarget::Ios | RouteTarget::Android => RuntimeTarget::ReactNative,
    }
}

fn runtime_target_name(target: &RuntimeTarget) -> &'static str {
    match target {
        RuntimeTarget::Web => "web",
        RuntimeTarget::ReactNative => "react-native",
        RuntimeTarget::Server => "server",
        RuntimeTarget::Hermes => "hermes",
    }
}

fn target_contract(target: RouteTarget) -> serde_json::Value {
    match target {
        RouteTarget::Web => json!({
            "runtime": "web",
            "renderer": { "status": "implemented", "kind": "react-dom" },
            "router": { "status": "implemented", "kind": "document" },
            "transform": { "status": "implemented", "kind": "vite" },
        }),
        RouteTarget::Native | RouteTarget::Ios | RouteTarget::Android => json!({
            "runtime": "react-native",
            "renderer": {
                "status": "pending",
                "kind": "react-native-host-config",
                "package": "@uniflowed/react-native",
                "reason": "React Native rendering still needs a native test renderer and host config"
            },
            "router": {
                "status": "implemented",
                "kind": "navigator",
                "package": "@uniflowed/router/native",
                "helper": "createNativeScreenRouter",
                "screenManifestHelper": "createNativeScreenManifest",
                "reason": "Native rendering still needs the renderer/host-config contract before the navigator can mount screens"
            },
            "transform": {
                "status": "implemented",
                "kind": "metro",
                "platform": target.as_str(),
                "sourceExtensions": ["js", "jsx", "mjs", "cjs"],
                "pipeline": ["flow", "react-compiler", "stylex-native-objects", "metro-babel"],
                "config": {
                    "package": "@uniflowed/react-native/metro",
                    "helper": "withUniflowedMetro",
                    "transformer": "@uniflowed/react-native/metro-transformer.cjs"
                },
                "stylex": {
                    "css": "refused",
                    "reason": "React Native Metro bundles cannot attach StyleX CSS output yet"
                }
            },
        }),
    }
}

/// What `driver.js build` is told, beyond where to put the output.
///
/// `--target` is the application platform whose reserved files the builder
/// should resolve; `--prerender` and `--static-build` are the rendering
/// decision; `--because` is the *sentence* that decision came from, so a
/// refusal in the builder quotes the same config key a refusal in `uf` does.
/// Without it the driver would have to reconstruct "which setting made this a
/// static build" from a flag that no longer says, and the two halves of one
/// rule would tell a reader to look in two places.
///
/// `app.rendering.navigation` is deliberately **not** a fifth. It is not a
/// decision about this build — it is the same answer for `uf dev`, `uf build`
/// and `uf preview`, and neither of the first two is handed a rendering plan
/// at all — so a builder reads it out of `uf.config.js` the way it reads
/// `app.react.strictMode`. Passing it here as well would make the client entry
/// a function of two sources that agree until one of them is a flag somebody
/// forgot to forward.
fn build_arguments(
    out_dir: &str,
    plan: RenderingPlan,
    target: RouteTarget,
    regenerate: bool,
    partial: bool,
    analyze: bool,
) -> Vec<String> {
    let mut args = vec![
        String::from("--out-dir"),
        out_dir.to_string(),
        String::from("--target"),
        target.as_str().to_string(),
        String::from("--prerender"),
        plan.prerender().as_str().to_string(),
        String::from("--because"),
        plan.because(),
    ];
    if !plan.emits_a_server() {
        args.push(String::from("--static-build"));
    }
    if regenerate {
        args.push(String::from("--regenerate"));
    }
    if partial {
        args.push(String::from("--partial"));
    }
    if analyze {
        args.push(String::from("--analyze"));
    }
    args
}

/// Whether a prerendered page that states a lifetime is written for
/// regeneration rather than as a document that never changes.
///
/// Three things have to hold, and each is a declaration the builder cannot see
/// on its own: `app.rendering.modes` allows `isr`, `rendering.cache.route` is
/// on — a regenerated page *is* the route cache, started from the build — and
/// the plan leaves a server behind to do the regenerating. Decided here, beside
/// the rest of the rendering decision, so the builder is told one word.
fn regenerates_pages(config: &uf_config::UniflowedConfig, plan: RenderingPlan) -> bool {
    config
        .app
        .rendering
        .modes
        .contains(&uf_config::RenderingMode::Isr)
        && config.app.rendering.cache.route
        && plan.emits_a_server()
}

/// Whether a prerendered page that reads the request inside a `<Suspense>`
/// boundary is written as a static shell rather than refused.
///
/// Three declarations and one fact about the renderer: `app.rendering.modes`
/// allows `ppr`, the plan prerenders and leaves a server behind to fill the
/// holes, and routes render as Server Components — the renderer that writes a
/// shell. `app.rsc: false` renders a route from its modules, whole or not at
/// all. Decided here, beside the rest of the rendering decision, so the builder
/// is told one word and [`request_state`] asks the same question.
pub(crate) fn prerenders_partially(
    config: &uf_config::UniflowedConfig,
    plan: RenderingPlan,
) -> bool {
    config
        .app
        .rendering
        .modes
        .contains(&uf_config::RenderingMode::Ppr)
        && config.app.rsc
        && plan.emits_a_server()
        && matches!(
            plan.prerender(),
            uf_config::Prerender::Everything | uf_config::Prerender::Possible
        )
}

/// Print the RSC analysis's diagnostics, grouped by module.
///
/// The same shape `uf lint` and `uf check` print — a path, then a code frame
/// per diagnostic — because a person should not have to learn two diagnostic
/// formats to read two of uf's commands. `uf_rsc` accumulates violations as
/// typed data precisely so that a reporter can be written once against them,
/// and until now none had been: the build turned the whole list into
/// `diagnostics.len()` and printed the number.
///
/// A module that cannot be read still gets its header and its message, with no
/// source line under it. That is a diagnostic about the module's *content*,
/// so refusing to report it because the file has since moved would lose the
/// finding to a race.
fn render_rsc_diagnostics(ui: &mut Ui, root: &Utf8Path, diagnostics: &[RscDiagnostic]) {
    ui.render(|renderer, out| {
        renderer.blank(out);
    });

    for group in group_by_module(diagnostics) {
        let module = group[0].module().to_string();
        let source = diagnostic_source(root, &module, &group);
        let lines: Vec<&str> = source.lines().collect();
        let errors = group
            .iter()
            .filter(|diagnostic| diagnostic.severity() == RscSeverity::Error)
            .count();
        let header = problem_summary(errors, group.len() - errors);
        let rendered: Vec<(DiagnosticLevel, &'static str, String, usize, usize)> = group
            .iter()
            .map(|diagnostic| {
                let level = match diagnostic.severity() {
                    RscSeverity::Error => DiagnosticLevel::Error,
                    RscSeverity::Warn => DiagnosticLevel::Warning,
                };
                (
                    level,
                    diagnostic.rule(),
                    diagnostic.to_string(),
                    diagnostic.line() as usize,
                    diagnostic.column() as usize,
                )
            })
            .collect();

        // A module id out of the bundler names a file in the checkout. #640.
        let drawn = uf_term::safe_path(&module);
        ui.render(|renderer, out| {
            renderer.theme().path.paint(renderer.color(), &drawn, out);
            out.push_str("  ");
            renderer.theme().muted.paint(renderer.color(), &header, out);
            out.push('\n');
            renderer.blank(out);

            for (level, rule, message, line, column) in &rendered {
                let mut frame =
                    CodeFrame::new(*level, message, &module, *line, *column).with_rule(rule);
                if let Some(source_line) = lines.get(line.saturating_sub(1)).copied() {
                    frame = frame
                        .with_source_line(source_line)
                        .with_span(identifier_span(source_line, *column));
                }
                renderer.code_frame_at(out, &frame, 2);
                renderer.blank(out);
            }
        });
    }
}

/// The diagnostics of one module at a time, in the order they were found.
///
/// Grouped rather than sorted: `uf_rsc` returns them in graph order, which is
/// the order the modules were reached, and re-sorting would lose that for no
/// gain — the reason a module is in the client graph at all is the module
/// before it.
/// The source of `module`, when reading it is both any use and inside the
/// project.
///
/// Two conditions, and they are not the same question asked twice.
///
/// *Any use*: a code frame needs a line to underline, and `line() == 0` is how
/// a diagnostic says it has none. Reading a file to throw it away is only
/// wasted work — except that the one variant with no line is
/// `ModulePathOutsideProject`, whose `module` is by definition a path
/// `is_inside_project` has just rejected: absolute, climbing out with `..`, or
/// carrying a drive letter or a URL scheme.
///
/// *Inside the project*: `Utf8Path::join` with an absolute right-hand side
/// *replaces* the root rather than extending it. So `root.join(module)` for
/// that same diagnostic resolved to the outside path itself, the file was read,
/// and — because `line().saturating_sub(1)` is `0` for a line of `0` —
/// `lines.get(0)` put its first line into the build output. The diagnostic
/// whose entire content is "this path is not in your project" was the one that
/// made uf read it.
///
/// Lexical, and deliberately not `canonicalize`: the check is about what `join`
/// does with the string, the scanner produces relative paths and does not
/// follow symlinks (`collect_module_paths`), and a `stat` per module to
/// re-establish something already true by construction would buy nothing. This
/// is the belt on top of the braces, and it costs no syscall.
///
/// An empty string means "no source line", which is what the reporter already
/// does with a file that has since moved: the header and the message still
/// print. A diagnostic about a module's *content* must not be lost because the
/// file could not be read.
fn diagnostic_source(root: &Utf8Path, module: &str, group: &[&RscDiagnostic]) -> String {
    if group.iter().all(|diagnostic| diagnostic.line() == 0) {
        return String::new();
    }
    let path = Utf8Path::new(module);
    if path.is_absolute() || path.components().any(|part| part.as_str() == "..") {
        return String::new();
    }
    fs::read_to_string(root.join(path)).unwrap_or_default()
}

fn group_by_module(diagnostics: &[RscDiagnostic]) -> Vec<Vec<&RscDiagnostic>> {
    let mut groups: Vec<Vec<&RscDiagnostic>> = Vec::new();
    for diagnostic in diagnostics {
        match groups
            .iter_mut()
            .find(|group| group[0].module() == diagnostic.module())
        {
            Some(group) => group.push(diagnostic),
            None => groups.push(vec![diagnostic]),
        }
    }
    groups
}

/// Attribute the client entry to every route.
///
/// Vite's manifest names the entry chunk and the stylesheets it pulls in;
/// every route loads those before it can render. Route-level code splitting
/// is attributed lazily once the router's dynamic imports are measured.
fn route_assets(
    out_dir: &Utf8Path,
    routes: &[Route],
) -> Vec<(CompactString, Vec<CompactString>, Vec<CompactString>)> {
    let initial = entry_assets(out_dir);
    routes
        .iter()
        .map(|route| (route.path.clone(), initial.clone(), Vec::new()))
        .collect()
}

/// The entry chunk and its stylesheets from `.vite/manifest.json`, or nothing
/// when the manifest is missing.
fn entry_assets(out_dir: &Utf8Path) -> Vec<CompactString> {
    let manifest: Utf8PathBuf = out_dir.join(".vite/manifest.json");
    let Ok(text) = fs::read_to_string(&manifest) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(entries) = value.as_object() else {
        return Vec::new();
    };
    let mut assets = Vec::new();
    for chunk in entries.values() {
        if chunk.get("isEntry").and_then(serde_json::Value::as_bool) != Some(true) {
            continue;
        }
        if let Some(file) = chunk.get("file").and_then(serde_json::Value::as_str) {
            assets.push(CompactString::new(file));
        }
        for css in chunk
            .get("css")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            assets.push(CompactString::new(css));
        }
    }
    assets
}

/// Read the module graph the builder wrote for `--analyze`, attribute it to
/// `routes`, and write the analysis and its view beside it.
///
/// A builder that wrote no graph is refused by name rather than analyzed as
/// empty: an analysis that lists nothing reads as an application that ships
/// nothing, when what happened is that the builder did not say.
fn analyze_bundles(
    root: &Utf8Path,
    meta_dir: &Utf8Path,
    routes: &[Route],
) -> Result<(Utf8PathBuf, Utf8PathBuf)> {
    let graph_path = meta_dir.join(uf_bundle::MODULE_GRAPH_FILE);
    if !graph_path.is_file() {
        anyhow::bail!(
            "`uf build --analyze` reads the module graph a builder writes to {}, and this \
             build's builder wrote none; `@uniflowed/vite` writes it",
            relative_to(root, &graph_path)
        );
    }
    let graph = uf_bundle::read_module_graph(&graph_path)?;
    let files = routes
        .iter()
        .map(|route| uf_bundle::RouteFiles {
            path: route.path.clone(),
            page: relative_to(root, &route.page),
            directory: relative_to(root, &route.directory),
        })
        .collect::<Vec<_>>();
    let analysis = uf_bundle::analyze(&graph, &files)?;
    Ok(uf_bundle::write_analysis(meta_dir, &analysis)?)
}

/// Fail the build when a shipped asset breaks `build.budgets`.
///
/// Budgets are unset by default, so this is a report until a project opts in.
/// Once it does, every violation is listed at once rather than one per CI run.
fn enforce_budgets(ui: &mut Ui, report: &BundleReport, budgets: &BundleBudgets) -> Result<()> {
    let outcome = evaluate(report, budgets);
    if outcome.is_within_budget() {
        return Ok(());
    }

    let violations: Vec<String> = outcome.violations.iter().map(ToString::to_string).collect();
    ui.render_err(|renderer, out| {
        for violation in &violations {
            renderer.status(out, Status::Error, violation);
        }
    });
    bail!(
        "bundle size exceeded {}",
        plural(outcome.violations.len(), "budget")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_bundle_row_is_absent_when_no_page_was_removed() {
        assert_eq!(split_summary(Some((2, 5))), Some("2 of 5".to_owned()));
        assert_eq!(split_summary(Some((5, 5))), None);
        assert_eq!(split_summary(None), None);
    }

    /// A `ClientOnlyApiInServerModule` for `module` at `line`, which is the
    /// ordinary shape: a diagnostic that points somewhere.
    fn positioned(module: &str, line: u32) -> RscDiagnostic {
        RscDiagnostic::ClientOnlyApiInServerModule {
            module: Utf8PathBuf::from(module),
            api: "localStorage",
            line,
            column: 3,
        }
    }

    #[test]
    fn a_diagnostic_with_a_line_gets_its_source() {
        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();
        fs::write(root.join("page.js"), "// @flow\nlocalStorage.clear();\n").unwrap();

        let diagnostic = positioned("page.js", 2);
        let source = diagnostic_source(root, "page.js", &[&diagnostic]);

        assert!(source.contains("localStorage.clear();"), "{source:?}");
    }

    #[test]
    fn a_diagnostic_with_no_line_reads_nothing() {
        // `ModulePathOutsideProject` is the only variant whose `line()` is `0`,
        // and it is also the only one whose `module` is a path the graph has
        // *rejected* — so this is not merely an optimisation. Without the skip,
        // `root.join(module)` on an absolute path drops the root entirely,
        // `line.saturating_sub(1)` is `0`, and `lines.get(0)` puts the first
        // line of somebody else's file into the build output.
        let outside = tempfile::tempdir().unwrap();
        let outside = Utf8Path::from_path(outside.path()).unwrap();
        let secret = outside.join("elsewhere.txt");
        fs::write(&secret, "the first line of a file uf was not asked about\n").unwrap();

        let root = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(root.path()).unwrap();

        let diagnostic = RscDiagnostic::ModulePathOutsideProject {
            module: secret.clone(),
        };
        let source = diagnostic_source(root, secret.as_str(), &[&diagnostic]);

        assert_eq!(source, "", "a diagnostic with no line read a file anyway");
    }

    #[test]
    fn a_module_path_that_climbs_out_of_the_project_reads_nothing() {
        // The same hole reached the other way, and with a line on it so the
        // skip above cannot be what closes it. `RscGraphBuilder` keeps a
        // caller-supplied path, and a relative one that climbs is still
        // outside.
        let outside = tempfile::tempdir().unwrap();
        let outside = Utf8Path::from_path(outside.path()).unwrap();
        fs::write(outside.join("elsewhere.txt"), "not this project's\n").unwrap();
        let root = outside.join("project");
        fs::create_dir_all(&root).unwrap();

        let module = "../elsewhere.txt";
        let diagnostic = positioned(module, 1);
        let source = diagnostic_source(&root, module, &[&diagnostic]);

        assert_eq!(source, "", "a climbing module path read a file anyway");
    }

    #[test]
    fn react_native_alias_is_an_application_target() {
        assert_eq!(
            parse_application_target("react-native").unwrap(),
            RouteTarget::Native
        );
    }

    #[test]
    fn native_target_contract_names_the_metro_platform() {
        for (target, platform) in [
            (RouteTarget::Native, "native"),
            (RouteTarget::Ios, "ios"),
            (RouteTarget::Android, "android"),
        ] {
            let contract = target_contract(target);
            assert_eq!(contract["runtime"], serde_json::json!("react-native"));
            assert_eq!(contract["renderer"]["status"], serde_json::json!("pending"));
            assert_eq!(
                contract["router"]["status"],
                serde_json::json!("implemented")
            );
            assert_eq!(contract["router"]["kind"], serde_json::json!("navigator"));
            assert_eq!(
                contract["router"]["package"],
                serde_json::json!("@uniflowed/router/native")
            );
            assert_eq!(
                contract["router"]["helper"],
                serde_json::json!("createNativeScreenRouter")
            );
            assert_eq!(
                contract["router"]["screenManifestHelper"],
                serde_json::json!("createNativeScreenManifest")
            );
            assert_eq!(contract["transform"]["kind"], serde_json::json!("metro"));
            assert_eq!(
                contract["transform"]["platform"],
                serde_json::json!(platform)
            );
            assert_eq!(
                contract["transform"]["status"],
                serde_json::json!("implemented")
            );
            assert_eq!(
                contract["transform"]["sourceExtensions"],
                serde_json::json!(["js", "jsx", "mjs", "cjs"])
            );
            assert_eq!(
                contract["transform"]["pipeline"],
                serde_json::json!([
                    "flow",
                    "react-compiler",
                    "stylex-native-objects",
                    "metro-babel"
                ])
            );
            assert_eq!(
                contract["transform"]["config"]["package"],
                serde_json::json!("@uniflowed/react-native/metro")
            );
            assert_eq!(
                contract["transform"]["config"]["helper"],
                serde_json::json!("withUniflowedMetro")
            );
            assert_eq!(
                contract["transform"]["config"]["transformer"],
                serde_json::json!("@uniflowed/react-native/metro-transformer.cjs")
            );
            assert_eq!(
                contract["transform"]["stylex"]["css"],
                serde_json::json!("refused")
            );
        }
    }
}
