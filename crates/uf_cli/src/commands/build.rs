//! `uf build`: banner, per-phase timings, a summary, and what was produced.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use compact_str::CompactString;
use serde_json::json;
use uf_bundle::{
    BudgetMetric, BundleBudgets, BundleReport, ReportOptions, build_report, collect_assets,
    evaluate, write_report,
};
use uf_bundler::{BundleOptions, BundleOutput, build_entries, build_pipeline};
use uf_config::{PipelineMode, load_config};
use uf_router::{Route, discover_routes, write_router_manifest};
use uf_rsc::{BuildId, ProjectScanOptions, analyze_project};
use uf_term::{Cell, Column, KeyValue, PhaseTimer, Status, Table, Tone, Tree, format_duration};

use crate::support::{plural, project_label, relative_to, write_json_file};
use crate::ui::Ui;

/// How many assets `--size-report` names before the list is cut off.
const LARGEST_ASSETS_SHOWN: usize = 20;

pub(crate) fn build(cwd: &Utf8Path, ui: &mut Ui, size_report: bool) -> Result<()> {
    let mut timer = PhaseTimer::start();
    let mut progress = ui.progress();

    progress.draw("loading configuration");
    let resolved = timer.measure("config", || load_config(cwd))?;

    progress.tick("discovering routes");
    let routes = timer.measure("routes", || {
        discover_routes(&resolved.root, &resolved.config)
    })?;
    let router_manifest = timer.measure("router types", || {
        write_router_manifest(&resolved.root, &resolved.config)
    })?;

    let out_dir = resolved.root.join(resolved.config.build.out_dir.as_str());
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {out_dir}"))?;
    let build_manifest = out_dir.join("uf-build-manifest.json");
    let payload = json!({
        "version": 1,
        "engine": "uf-native",
        "pluginContract": "uf-plugin-v1",
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
            "default": resolved.config.app.runtime.default,
            "capabilityJsHost": &resolved.config.app.runtime.capability_js_host,
            "wintertc": true,
            "hermes": false,
        },
        "cache": {
            "route": resolved.config.app.rendering.cache.route,
            "fetch": resolved.config.app.rendering.cache.fetch,
            "data": resolved.config.app.rendering.cache.data,
            "actions": resolved.config.app.rendering.cache.actions,
        },
    });
    progress.tick("writing manifest");
    timer.measure("manifest", || write_json_file(&build_manifest, &payload))?;

    progress.tick("analysing server components");
    let rsc = timer.measure("rsc analysis", || {
        analyze_project(
            &resolved.root,
            &BuildId::from_env_or_generate(),
            &ProjectScanOptions::default(),
        )
    })?;
    let rsc_manifest = timer.measure("rsc manifest", || {
        uf_rsc::write_manifest(&out_dir, &rsc.manifest())
    })?;

    progress.tick("bundling");
    let container = timer.measure("pipeline", || {
        build_pipeline(
            &resolved.config,
            &resolved.root,
            PipelineMode::Build,
            &routes,
        )
    })?;
    let entries = build_entries(&resolved.config, &resolved.root, &routes);
    let bundle_options = BundleOptions::new(resolved.root.clone(), out_dir.clone())
        .with_entries(entries)
        .with_sourcemap(resolved.config.build.sourcemap);
    let bundled = timer.measure("bundle", || uf_bundler::bundle(&bundle_options, &container))?;
    let written = timer.measure("emit", || {
        uf_bundler::write_bundle(&bundle_options, &bundled, &container)
    })?;

    progress.tick("measuring shipped assets");
    let (size, size_report_path) = timer.measure("bundle size", || -> Result<_> {
        let assets = collect_assets(&out_dir, &ReportOptions::default())?;
        let report = build_report(assets, &route_assets(&resolved.root, &routes, &bundled));
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
    let module_count = rsc.graph.modules().len().to_string();
    let client_count = rsc.graph.client_boundaries().len().to_string();
    let action_count = rsc.callable_action_count().to_string();
    let diagnostic_count = rsc.graph.diagnostics().len().to_string();

    let chunk_count = bundled.stats.chunks.to_string();
    let bundled_module_count = bundled.stats.modules_kept.to_string();
    let dropped_export_count = bundled.stats.exports_dropped.to_string();

    let mut outputs = vec![
        relative_to(&resolved.root, &build_manifest),
        relative_to(&resolved.root, &rsc_manifest),
        relative_to(&resolved.root, &size_report_path),
    ];
    if let Some(manifest) = &router_manifest {
        outputs.push(relative_to(&resolved.root, manifest));
    }
    for path in &written {
        outputs.push(relative_to(&resolved.root, path));
    }
    outputs.sort();
    let output_paths = outputs.iter().map(String::as_str).collect::<Vec<_>>();
    let root = project_label(&resolved.root).to_string();
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

    ui.render(|renderer, out| {
        renderer.banner(out, "uf build", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.timings(out, 2, &phases, Some(total));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("entries", &entries),
                KeyValue::toned("routes", &route_count, Tone::Number),
                KeyValue::toned("modules", &module_count, Tone::Number),
                KeyValue::toned("bundled modules", &bundled_module_count, Tone::Number),
                KeyValue::toned("chunks", &chunk_count, Tone::Number),
                KeyValue::toned("dropped exports", &dropped_export_count, Tone::Number),
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
            &Tree::from_paths(&root, output_paths.iter().copied()),
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });

    enforce_budgets(ui, &size, &resolved.config.build.budgets)
}

/// Attribute emitted chunks to the routes that need them.
///
/// A route's initial JavaScript is the transitive closure of its entry chunk:
/// every file a browser downloads before the route can render. Nothing is
/// attributed lazily yet, because nothing is code-split behind a dynamic
/// `import()` yet — so the lazy column is honestly empty rather than a guess.
fn route_assets(
    root: &Utf8Path,
    routes: &[Route],
    bundled: &BundleOutput,
) -> Vec<(CompactString, Vec<CompactString>, Vec<CompactString>)> {
    routes
        .iter()
        .map(|route| {
            let module = route.page.strip_prefix(root).unwrap_or(&route.page);
            (route.path.clone(), bundled.closure_of(module), Vec::new())
        })
        .collect()
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
