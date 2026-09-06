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

use std::fs;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde_json::json;
use uf_bundle::{
    BudgetMetric, BundleBudgets, BundleReport, ReportOptions, build_report, collect_assets,
    evaluate, write_report,
};
use uf_config::load_config;
use uf_router::{Route, discover_routes, write_router_manifest};
use uf_rsc::{BuildId, ProjectScanOptions, RscDiagnostic, RscSeverity, analyze_project};
use uf_term::{
    Cell, CodeFrame, Column, DiagnosticLevel, KeyValue, PhaseTimer, Status, Table, Tone, Tree,
    format_duration,
};

use crate::commands::lint::identifier_span;
use crate::commands::vite::{Driver, Event, package_dir, render_error, render_log, resolve_host};
use crate::support::{plural, problem_summary, project_label, relative_to, write_json_file};
use crate::ui::Ui;

/// How many assets `--size-report` names before the list is cut off.
const LARGEST_ASSETS_SHOWN: usize = 20;

/// What Vite reported building.
#[derive(Debug, Default)]
struct ViteBuild {
    /// Prerendered pages, as `(url, file)`.
    pages: Vec<(String, String)>,
    /// Warnings Vite logged, shown after the summary.
    warnings: Vec<String>,
}

pub(crate) fn build(cwd: &Utf8Path, ui: &mut Ui, size_report: bool) -> Result<()> {
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

    progress.tick("resolving the JavaScript host");
    let host = resolve_host(&resolved.config)?;
    let package = package_dir(&root)?;

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
        )?;
        let mut report = ViteBuild::default();
        while let Some(event) = driver.next_event()? {
            match event {
                Event::Phase { name } => progress.tick(&format!("vite: {name}")),
                Event::Page { url, file, .. } => report.pages.push((url, file)),
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
                Event::ConfigLoaded { .. }
                | Event::Listening { .. }
                | Event::Done { .. }
                | Event::Config { .. } => {}
            }
        }
        driver.finish("the Vite build")?;
        Ok(report)
    })?;

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
    let host_name = host.name();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf build", Some(&project));
        renderer.blank(out);
        renderer.timings(out, 2, &phases, Some(total));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("engine", "vite"),
                KeyValue::toned("host", host_name, Tone::Muted),
                KeyValue::new("entries", &entries),
                KeyValue::toned("routes", &route_count, Tone::Number),
                KeyValue::toned("prerendered pages", &page_count, Tone::Number),
                KeyValue::toned("modules", &module_count, Tone::Number),
                KeyValue::toned("client components", &client_count, Tone::Number),
                KeyValue::toned("server actions", &action_count, Tone::Number),
                KeyValue::toned("rsc diagnostics", &diagnostic_count, Tone::Number),
            ],
        );
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

        renderer.heading(out, 2, "output");
        renderer.tree(
            out,
            4,
            &Tree::from_paths(&project, output_paths.iter().copied()),
        );
        renderer.blank(out);
        for warning in &warnings {
            renderer.status(out, Status::Warn, warning);
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
        let source = fs::read_to_string(root.join(module.as_str())).unwrap_or_default();
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
