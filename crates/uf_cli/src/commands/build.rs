//! `uf build`: banner, per-phase timings, a summary, and what was produced.
//!
//! The build is Vite's, driven through `@uniflowed/vite` on the project's
//! JavaScript host (see [`super::vite`]): a client bundle, a server bundle,
//! and every static route prerendered to HTML. uf's own phases run around it:
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
use uf_config::{DeployAdapter, load_config};
use uf_router::{Route, discover_routes, write_router_manifest};
use uf_rsc::{
    BuildId, ProjectScanOptions, RSC_MANIFEST_BUILD_DIR, RSC_MANIFEST_ENV, RscDiagnostic,
    RscSeverity, analyze_project,
};
use uf_term::{
    Cell, CodeFrame, Column, DiagnosticLevel, KeyValue, PhaseTimer, Status, Table, Tone, Tree,
    format_duration,
};

use crate::commands::compile;
use crate::commands::deploy;
use crate::commands::lint::identifier_span;
use crate::commands::vite::{Driver, Event, package_dir, render_error, render_log, resolve_host};
use crate::support::{
    PRODUCTION, plural, problem_summary, project_env, project_label, relative_to, write_json_file,
};
use crate::ui::Ui;

mod guards;

/// How many assets `--size-report` names before the list is cut off.
const LARGEST_ASSETS_SHOWN: usize = 20;

/// What Vite reported building.
#[derive(Debug, Default)]
struct ViteBuild {
    /// Prerendered pages, as `(url, file)`.
    pages: Vec<(String, String)>,
    /// Warnings Vite logged, shown after the summary.
    warnings: Vec<String>,
    /// How the client route table came out of the server-component split.
    ///
    /// `(pages kept, routes)`, reported by the plugin that generated the table
    /// rather than computed here. `None` when the bundler said nothing, which
    /// is what an older `@uniflowed/vite` does; the summary then omits the row
    /// rather than printing a number nothing produced.
    split: Option<(u64, u64)>,
}

pub(crate) fn build(
    cwd: &Utf8Path,
    ui: &mut Ui,
    size_report: bool,
    requested_mode: Option<&str>,
    standalone: bool,
    requested_adapter: Option<DeployAdapter>,
) -> Result<()> {
    let mut timer = PhaseTimer::start();
    let mut progress = ui.progress();

    progress.draw("loading configuration");
    let resolved = timer.measure("config", || load_config(cwd))?;
    let root = resolved.root.clone();

    progress.tick("discovering routes");
    let routes = timer.measure("routes", || {
        discover_routes(&resolved.root, &resolved.config)
    })?;
    let router_manifest = timer.measure("router types", || {
        write_router_manifest(&resolved.root, &resolved.config)
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
    if !rsc.graph.diagnostics().is_empty() {
        progress.finish();
        render_rsc_diagnostics(ui, &root, rsc.graph.diagnostics());
        if rsc.graph.has_errors() {
            let errors = rsc
                .graph
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.severity() == RscSeverity::Error)
                .count();
            bail!(
                "{}",
                plural(errors, "React Server Components contract violation")
            );
        }
    }

    // The bundler's copy of the analysis, and the reason it is written here
    // rather than beside the one in `dist/` below. `@uniflowed/vite` decides
    // which routes keep their page module *while it is emitting the bundle*,
    // and `dist/` does not exist yet — Vite empties it on the way in. So the
    // same bytes go somewhere the build cannot sweep away, and the path is
    // handed to the driver as `UF_RSC_MANIFEST`.
    let rsc_input = uf_rsc::write_manifest(&root.join(RSC_MANIFEST_BUILD_DIR), &rsc.manifest())?;

    progress.tick("resolving the JavaScript host");
    let host = resolve_host(&resolved.config)?;
    let package = package_dir(&root)?;
    // `production` unless the project or the command line said another mode,
    // which is what selects `.env.production` over `.env.development` — the
    // half of ubugeeei-prod/uf#259 that made a build and a dev server disagree
    // about the same variable.
    let env = project_env(&resolved, requested_mode, PRODUCTION)?;

    // Asked for before anything is built. `--compile` on a machine without Bun
    // fails either way; failing now costs the user nothing, and failing after
    // the bundle costs them the build.
    let runtime = if standalone {
        Some(timer.measure("runtime", compile::runtime)?)
    } else {
        None
    };
    // The same rule for the same reason: an adapter nobody has written is a
    // sentence, and a sentence is cheaper before the bundle than after it.
    let adapter = deploy::resolve(&resolved.config.app.runtime.deploy, requested_adapter)?;

    progress.tick("building with vite");
    let vite = timer.measure("vite", || -> Result<ViteBuild> {
        let mut driver = Driver::spawn(
            &host,
            &package,
            &root,
            "build",
            &[
                String::from("--out-dir"),
                resolved.config.build.out_dir.to_string(),
            ],
            &env,
            &[(RSC_MANIFEST_ENV, rsc_input.as_str())],
        )?;
        let mut report = ViteBuild::default();
        while let Some(event) = driver.next_event()? {
            match event {
                Event::Phase { name } => progress.tick(&format!("vite: {name}")),
                Event::Page { url, file, .. } => report.pages.push((url, file)),
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
                // A build has no watcher, so `SourceChanged` never reaches
                // it; it is in the match because the driver's channel is one
                // vocabulary and every reader has to know the whole of it.
                Event::ConfigLoaded { .. }
                | Event::Listening { .. }
                | Event::SourceChanged
                | Event::Done { .. }
                | Event::Config { .. } => {}
            }
        }
        driver.finish("the Vite build")?;
        Ok(report)
    })?;

    // Which of the documents Vite just wrote are under a `_uf.middleware.js`.
    // Answerable only here, because the pages are what the prerender produced
    // rather than what the route table said it might; the reason it is a
    // report and not a refusal is argued in [`guards`].
    let unguarded = guards::unguarded_pages(&resolved.root, &routes, &vite.pages);

    // Written after Vite so `emptyOutDir` cannot sweep them away, and so the
    // manifest describes the build that actually happened.
    progress.tick("writing manifests");
    let build_manifest = out_dir.join("uf-build-manifest.json");
    let payload = json!({
        "version": 2,
        "engine": "vite",
        "transform": "uf transform",
        "host": host.name(),
        "entries": resolved.config.build.entries,
        "routes": routes.iter().map(|route| json!({
            "path": route.path,
            "page": relative_to(&resolved.root, &route.page),
            "params": route.params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "pages": vite.pages.iter().map(|(url, file)| json!({ "url": url, "file": file })).collect::<Vec<_>>(),
        // The same list the summary warns about, as data: which deployment of
        // `dist/` is happening is a fact the build does not have, and a deploy
        // step that does have it needs somewhere to read this from that is not
        // a terminal.
        "prerenderedUnderMiddleware": unguarded.iter().map(|page| json!({
            "url": page.url,
            "file": page.file,
            "middleware": page.middleware,
        })).collect::<Vec<_>>(),
        "runtime": {
            "default": resolved.config.app.runtime.default,
            "capabilityJsHost": &resolved.config.app.runtime.capability_js_host,
        },
        "cache": {
            "route": resolved.config.app.rendering.cache.route,
            "fetch": resolved.config.app.rendering.cache.fetch,
            "data": resolved.config.app.rendering.cache.data,
            "actions": resolved.config.app.rendering.cache.actions,
        },
    });
    timer.measure("manifest", || write_json_file(&build_manifest, &payload))?;
    let rsc_manifest = timer.measure("rsc manifest", || {
        uf_rsc::write_manifest(&out_dir, &rsc.manifest())
    })?;

    progress.tick("measuring shipped assets");
    let (size, size_report_path) = timer.measure("bundle size", || -> Result<_> {
        let assets = collect_assets(&out_dir, &ReportOptions::default())?;
        let report = build_report(assets, &route_assets(&out_dir, &routes));
        let path = write_report(&out_dir, &report)?;
        Ok((report, path))
    })?;
    // After the size report and not before it: the binary is written into the
    // output directory, and an executable counted among the shipped assets
    // would put every budget in `uf.config.js` permanently over.
    let compiled = match &runtime {
        Some(runtime) => {
            progress.tick("compiling a standalone binary");
            Some(timer.measure("compile", || {
                compile::compile(ui, runtime, &host, &package, &root, &out_dir, &env)
            })?)
        }
        None => None,
    };
    // After the binary, so that a build asked for both copies the binary's
    // exclusion rather than the binary itself: `--compile` writes into
    // `dist/`, and `deploy` copies `dist/`.
    let deployed = match adapter {
        Some(adapter) => {
            progress.tick(&format!(
                "writing the {} adapter's output",
                adapter.as_str()
            ));
            Some(timer.measure("adapter", || {
                deploy::deploy(ui, adapter, &host, &package, &root, &out_dir, &env)
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
    let split_count = vite
        .split
        .filter(|(pages, routes)| pages < routes)
        .map(|(pages, routes)| format!("{pages} of {routes}"));
    let action_count = rsc.callable_action_count().to_string();
    let diagnostic_count = rsc.graph.diagnostics().len().to_string();

    let mut outputs = vec![
        relative_to(&resolved.root, &build_manifest),
        relative_to(&resolved.root, &rsc_manifest),
        relative_to(&resolved.root, &size_report_path),
    ];
    if let Some(manifest) = &router_manifest {
        outputs.push(relative_to(&resolved.root, manifest));
    }
    for (_, file) in &vite.pages {
        outputs.push(file.clone());
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
    let warnings = vite.warnings.clone();
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
            // `npm start` does not yet believe that claim.
            format!("cd {directory} && node server.js"),
        )
    });
    let binary = compiled.as_ref().map(|compiled| {
        (
            relative_to(&resolved.root, &compiled.binary),
            ByteSize::from_bytes(compiled.bytes).to_string(),
            compiled.embedded.files.to_string(),
            ByteSize::from_bytes(compiled.embedded.bytes).to_string(),
        )
    });

    ui.render(|renderer, out| {
        renderer.banner(out, "uf build", Some(&project));
        renderer.blank(out);
        renderer.timings(out, 2, &phases, Some(total));
        renderer.blank(out);
        let mut summary_rows = vec![
            KeyValue::new("engine", "vite"),
            KeyValue::toned("host", host_name, Tone::Muted),
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

        if let Some((path, bytes, files, embedded)) = &binary {
            renderer.heading(out, 2, "standalone");
            renderer.key_values(
                out,
                4,
                &[
                    KeyValue::toned("binary", path, Tone::Path),
                    KeyValue::toned("runtime", "bun", Tone::Muted),
                    KeyValue::toned("bytes", bytes, Tone::Accent),
                    KeyValue::toned("embedded assets", files, Tone::Number),
                    KeyValue::toned("embedded bytes", embedded, Tone::Number),
                ],
            );
            renderer.blank(out);
        }

        renderer.heading(out, 2, "output");
        renderer.tree(
            out,
            4,
            &Tree::from_paths(&project, output_paths.iter().copied()),
        );
        renderer.blank(out);

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
        renderer.status(out, Status::Success, &summary);
    });

    enforce_budgets(ui, &size, &resolved.config.build.budgets)
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

        ui.render(|renderer, out| {
            renderer.theme().path.paint(renderer.color(), &module, out);
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
}
