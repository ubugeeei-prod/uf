//! `uf build` for a project that is a library rather than an application.
//!
//! `uf create lib` has always written a project whose `uf.config.js` says
//! `app: { router: { enabled: false } }` and whose `build` task says
//! `uf build`, and that task could not succeed: the application build links
//! `virtual:uf/client`, which imports the router and the project's `app.js`,
//! and a library has neither. The scaffold failed at the first pass with
//! `Could not resolve '<root>/app.js'` — a file it does not have and never
//! had. See ubugeeei-prod/uf#268.
//!
//! Which of the two builds runs is [`uf_config::LibraryPlan`]'s answer, taken
//! once in [`super::build`] and never asked again.
//!
//! # What a published Flow library ships
//!
//! This is the decision the issue asked for, and the rest of this module
//! follows from it. **A library ships both halves: the Flow source it was
//! written in, and one plain-JavaScript build per entry.** Its `exports` names
//! them through conditions, with the compiled build as `default`:
//!
//! ```json
//! "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } },
//! "files": ["index.js", "dist"]
//! ```
//!
//! Four constraints decide it, and each rules out one of the simpler answers.
//!
//! **Compiled only** is refused by `docs/architecture.md`'s rule that there
//! are no `.js.flow` declaration files. A Flow library's types live in its
//! source and nowhere else, so a package that ships only `dist/` ships no
//! types at all — for the consumer uf cares most about, another uf project.
//! It would also break the sourcemap: `dist/index.js.map` names `../index.js`,
//! and a tarball without it is a map to a file nobody has.
//!
//! **Source only** is what uf's own `packages/*` do, and it works there for a
//! reason a user's library cannot borrow. `isFlowModule` in
//! `@uniflowed/host/transform` transforms `.js` under `node_modules` only for
//! `@uniflowed/*`, and `@uniflowed/vite` names the same prefix in
//! `optimizeDeps.exclude` and `ssr.noExternal`. That is a hard-coded deal for
//! one scope; extending it to every library in the world means uf keeping a
//! list of them. Someone who writes a Flow library and wants it consumed from
//! a plain Vite app, a Node service or a bundler that has never heard of Flow
//! has to emit JavaScript, and uf owns the only transform that can.
//!
//! **Which half is `default`** follows from what a resolver that knows nothing
//! does: it takes `default`. So `default` has to be the file every runtime can
//! evaluate, and the source goes behind an opt-in condition. `"flow"` names
//! the *language of the file behind it* rather than the toolchain in front of
//! it, so a consumer who has a Flow transform and is not uf can select the
//! same condition. Getting this backwards — source as `default` — publishes a
//! package that is a syntax error everywhere except inside uf.
//!
//! **`tools/ci/publishable.sh`** is the fourth, and it is why nothing of uf's
//! is in the output. A published package may not depend on an unpublished one,
//! and the compiled build carries no `@uniflowed/*` import that the author did
//! not write: uf's whole contribution is the transform, which leaves nothing
//! behind. A library that has no runtime dependency on the toolchain that
//! built it is a library that can be installed by someone who has never
//! installed uf, which is the second half of "consumable without adopting the
//! whole toolchain".
//!
//! # What uf does not do yet, and why the condition is still worth publishing
//!
//! uf's own application build does **not** select the `"flow"` condition, and
//! adding it today would break every project that consumed a uf library:
//! Vite would resolve `index.js` out of `node_modules`, `isFlowModule` would
//! decline to transform it because it is not `@uniflowed/*`, and the Flow
//! syntax would reach the bundler's parser. Teaching the transform which
//! third-party packages are Flow is a separate piece of work with its own
//! question — whose Flow settings apply to a dependency's source — and it is
//! Planned. Until it lands, a uf application consuming a uf library resolves
//! `default` like everybody else and gets the compiled build, which works.
//!
//! The condition is published now anyway, because it is what such a resolver
//! keys on and because a resolver that does not know it already does the right
//! thing.

use std::collections::BTreeSet;
use std::fs;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use globset::GlobBuilder;
use serde_json::json;
use uf_bundle::{
    BudgetMetric, BundleReport, ReportOptions, build_report, collect_assets, write_report,
};
use uf_config::{LibraryPlan, ResolvedConfig};
use uf_term::{Cell, Column, KeyValue, PhaseTimer, Status, Table, Tone, Tree, format_duration};

use crate::commands::builder;
use crate::commands::vite::{Driver, Event, render_error, render_log, resolve_host};
use crate::support::{
    PRODUCTION, plural, project_env, project_label, relative_to, write_json_file,
};
use crate::ui::Ui;

use super::{BUILD_META_DIR, LARGEST_ASSETS_SHOWN, enforce_budgets};

/// The manifest fields whose names a library build must leave as imports.
///
/// `devDependencies` is deliberately absent, and it is the same line
/// `tools/ci/publishable.sh` draws for the same reason: a dev dependency is
/// not installed for a consumer, so an import of one that survived into the
/// output would be a package the consumer does not have. Leaving it *inlined*
/// is the only answer that produces something installable — and a library that
/// imports a dev dependency at runtime has a bug this build should not hide by
/// externalising it into a missing module.
const EXTERNAL_FIELDS: [&str; 3] = ["dependencies", "peerDependencies", "optionalDependencies"];

/// Build the project as a library, and report what it wrote.
pub(crate) fn build(
    ui: &mut Ui,
    mut timer: PhaseTimer,
    resolved: &ResolvedConfig,
    plan: &LibraryPlan,
    requested_mode: Option<&str>,
    size_report: bool,
) -> Result<()> {
    let mut progress = ui.progress();
    let root = resolved.root.clone();
    let out_dir = root.join(resolved.config.build.out_dir.as_str());
    fs::create_dir_all(&out_dir).with_context(|| format!("failed to create {out_dir}"))?;

    progress.tick("resolving the JavaScript host");
    let host = resolve_host(&resolved.config)?;
    let builder = builder::resolve(&root, &resolved.config)?;
    let env = project_env(resolved, requested_mode, PRODUCTION)?;

    // Read before the bundle, because it is what the bundle is told. A
    // manifest that cannot be read is not an error: a library without one is a
    // library with no declared dependencies, which externalises nothing beyond
    // the host's built-ins and is a correct build of a project that imports
    // only its own modules — which is exactly the scaffold.
    let external = declared_dependencies(&root);

    progress.tick("building the library");
    let warnings = timer.measure("vite", || -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        let mut driver = Driver::spawn(
            &host,
            &builder,
            &root,
            "library",
            &arguments(&resolved.config.build.out_dir, plan, &external),
            &env,
            &[],
        )?;
        while let Some(event) = driver.next_event()? {
            match event {
                Event::Phase { name } => progress.tick(&format!("vite: {name}")),
                Event::Log { level, message } => match level {
                    crate::commands::vite::LogLevel::Error => render_log(ui, level, &message),
                    // Held until the summary rather than printed now, which is
                    // what the application build does with them and for the
                    // same reason: a warning above the report is a warning
                    // scrolled off the top of it.
                    crate::commands::vite::LogLevel::Warn => warnings.push(message),
                    crate::commands::vite::LogLevel::Info => {}
                },
                Event::Error(error) => {
                    let failure = render_error(ui, &root, &error);
                    let _ = driver.finish("uf build");
                    return Err(failure);
                }
                // A library build prerenders nothing, serves nothing and
                // watches nothing, so most of the vocabulary never reaches
                // here. It is matched in full because the driver's channel is
                // one vocabulary and every reader has to know the whole of it.
                Event::ConfigLoaded { .. }
                | Event::Page { .. }
                | Event::PageFailed { .. }
                | Event::Rendering { .. }
                | Event::RscSplit { .. }
                | Event::Listening { .. }
                | Event::SourceChanged
                | Event::EnvChanged { .. }
                | Event::Diagnostic(_)
                | Event::Done { .. }
                | Event::Config { .. } => {}
            }
        }
        driver.finish("the library build")?;
        Ok(warnings)
    })?;

    progress.tick("measuring the published modules");
    let meta_dir = root.join(BUILD_META_DIR);
    fs::create_dir_all(&meta_dir).with_context(|| format!("failed to create {meta_dir}"))?;
    let (size, size_report_path) = timer.measure("bundle size", || -> Result<_> {
        let assets = collect_assets(&out_dir, &ReportOptions::default())?;
        // No routes: a library has none, and the per-route half of the report
        // is a question about an application's first paint.
        let report = build_report(assets, &[]);
        let path = write_report(&meta_dir, &report)?;
        Ok((report, path))
    })?;

    // What a consumer will resolve, checked against what the build wrote. A
    // library whose `exports` names a file under the output directory that
    // this build did not produce is a package that installs and cannot be
    // imported, and every existing check would pass it: the manifest is
    // well-formed, the build succeeded, and the two disagree.
    let unresolved = timer.measure("exports", || unresolved_exports(&root, &out_dir));
    let unpublished = timer.measure("files", || unpublished_exports(&root));

    let build_manifest = meta_dir.join("uf-build-manifest.json");
    let payload = json!({
        "version": 2,
        // Which of the two builds this was. A reader — or a deploy step — with
        // only the manifest in hand could not tell a library's output from an
        // application's that prerendered nothing.
        "kind": "library",
        "engine": "vite",
        "transform": "uf transform",
        "host": host.name(),
        "entries": plan.entries(),
        "formats": plan.formats().iter().map(|format| format.as_str()).collect::<Vec<_>>(),
        "external": external,
        "modules": size.assets.iter().map(|asset| json!({
            "path": asset.path,
            "bytes": asset.size.raw.bytes(),
        })).collect::<Vec<_>>(),
        "runtime": {
            "default": resolved.config.app.runtime.default,
            "capabilityJsHost": &resolved.config.app.runtime.capability_js_host,
        },
    });
    timer.measure("manifest", || write_json_file(&build_manifest, &payload))?;

    progress.finish();
    drop(progress);

    let total = timer.total();
    let phases = timer.phases().to_vec();
    let project = project_label(&root).to_string();
    let summary = format!("build succeeded in {}", format_duration(total));
    let host_name = host.name();
    let because = plan.because();
    let entries = plan
        .entries()
        .iter()
        .map(compact_str::CompactString::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let formats = plan
        .formats()
        .iter()
        .map(|format| format.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let external_count = external.len().to_string();
    let module_count = size.assets.len().to_string();
    let raw = size.total.raw.to_string();
    let gzip = size.total.gzip.to_string();
    let largest = largest_rows(&size, size_report);

    let mut outputs = vec![
        relative_to(&root, &build_manifest),
        relative_to(&root, &size_report_path),
    ];
    for asset in &size.assets {
        outputs.push(format!("{}/{}", resolved.config.build.out_dir, asset.path));
    }
    outputs.sort();
    outputs.dedup();
    let output_paths = outputs.iter().map(String::as_str).collect::<Vec<_>>();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf build", Some(&project));
        renderer.blank(out);
        renderer.timings(out, 2, &phases, Some(total));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                // First, because it is the answer to the question a reader
                // asks when `dist/` does not hold what they expected — and the
                // one `uf explain build` prints in the same words.
                KeyValue::new("build", "library"),
                KeyValue::toned("because", &because, Tone::Muted),
                KeyValue::new("engine", "vite"),
                KeyValue::toned("host", host_name, Tone::Muted),
                KeyValue::new("entries", &entries),
                KeyValue::new("formats", &formats),
                KeyValue::toned("external packages", &external_count, Tone::Number),
            ],
        );
        renderer.blank(out);

        renderer.heading(out, 2, "published");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::toned("modules", &module_count, Tone::Number),
                KeyValue::toned("raw", &raw, Tone::Number),
                KeyValue::toned("gzip", &gzip, Tone::Accent),
            ],
        );
        if !largest.is_empty() {
            renderer.blank(out);
            let mut table = Table::new(vec![
                Column::left("module"),
                Column::right("gzip"),
                Column::right("raw"),
            ]);
            for (path, gzip, raw) in &largest {
                table.push(vec![
                    Cell::toned(path, Tone::Path),
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

        for warning in warnings.iter().chain(&unresolved).chain(&unpublished) {
            renderer.status(out, Status::Warn, warning);
        }
        renderer.status(out, Status::Success, &summary);
    });

    enforce_budgets(ui, &size, &resolved.config.build.budgets)
}

/// What `driver.js library` is told.
///
/// Every argument is a decision uf made rather than one the builder is left to
/// take: which modules, which formats, and which names stay imports. A second
/// builder implementing this contract gets the same answers, which is the
/// point of resolving them here — "dependencies are external" is uf's policy
/// about what a library is, not a bundler's default.
fn arguments(out_dir: &str, plan: &LibraryPlan, external: &[String]) -> Vec<String> {
    let mut args = vec![String::from("--out-dir"), out_dir.to_string()];
    for entry in plan.entries() {
        args.push(String::from("--entry"));
        args.push(entry.to_string());
    }
    for format in plan.formats() {
        args.push(String::from("--format"));
        args.push(format.as_str().to_string());
    }
    for name in external {
        args.push(String::from("--external"));
        args.push(name.clone());
    }
    args
}

/// Every package name the project's `package.json` declares, sorted.
///
/// Sorted and de-duplicated because the same name can appear in two fields —
/// a peer that is also a dev dependency is the ordinary shape — and because a
/// command line that reorders itself between two builds of one tree is a
/// command line nobody can diff.
pub(super) fn declared_dependencies(root: &Utf8Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut names = BTreeSet::new();
    for field in EXTERNAL_FIELDS {
        if let Some(map) = manifest.get(field).and_then(serde_json::Value::as_object) {
            names.extend(map.keys().cloned());
        }
    }
    names.into_iter().collect()
}

/// Subpaths whose `exports` target lives in the output directory and was not
/// written.
///
/// Only targets *inside* the output directory are checked, and that is the
/// whole of the rule. A library's `exports` legitimately names its Flow source
/// — `"flow": "./index.js"` — and that file is in the checkout rather than in
/// the build, so treating every missing target as a finding would report the
/// source of every correctly configured library.
///
/// A warning rather than a failure. The build did what it was asked; what is
/// wrong is a manifest uf does not own and will not rewrite, and a library
/// that builds one entry at a time while its `exports` describes the finished
/// package is a project mid-edit rather than a project that is broken.
fn unresolved_exports(root: &Utf8Path, out_dir: &Utf8Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(exports) = manifest.get("exports") else {
        return Vec::new();
    };

    let mut missing = Vec::new();
    let mut targets = Vec::new();
    collect_targets(exports, &mut targets);
    for target in targets {
        let Some(relative) = target.strip_prefix("./") else {
            continue;
        };
        let path: Utf8PathBuf = root.join(relative);
        if !path.starts_with(out_dir) || path.exists() {
            continue;
        }
        missing.push(target);
    }
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "package.json exports {} this build did not write: {}",
        plural(missing.len(), "file"),
        missing.join(", "),
    )]
}

/// Subpaths whose `exports` target is not included by `package.json#files`.
///
/// `files` is a publish-time allowlist, which makes it the other half of the
/// same contract [`unresolved_exports`] checks: one asks whether the build wrote
/// the target, the other asks whether npm will put that target in the tarball.
/// Without this, a library can build cleanly and publish a package that has an
/// `exports` map pointing at files npm omitted — especially the scaffold's
/// `dist/`, which `.gitignore` intentionally ignores unless `files` names it.
fn unpublished_exports(root: &Utf8Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(root.join("package.json")) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(exports) = manifest.get("exports") else {
        return Vec::new();
    };

    let Some(files) = manifest.get("files").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let files = files
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    let always_published = always_published_targets(&manifest);

    let mut missing = Vec::new();
    let mut targets = Vec::new();
    collect_targets(exports, &mut targets);
    for target in targets {
        let Some(relative) = export_target(&target) else {
            continue;
        };
        if always_published.contains(relative) || files_publish(relative, &files) {
            continue;
        }
        missing.push(target);
    }
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "package.json exports {} not covered by files: {}",
        plural(missing.len(), "file"),
        missing.join(", "),
    )]
}

/// A relative target out of `exports`, or nothing for package specifiers.
fn export_target(target: &str) -> Option<&str> {
    target.strip_prefix("./")
}

/// Targets npm publishes even when `files` does not name them.
fn always_published_targets(manifest: &serde_json::Value) -> BTreeSet<String> {
    let mut targets = BTreeSet::from([String::from("package.json")]);

    if let Some(target) = manifest
        .get("main")
        .and_then(serde_json::Value::as_str)
        .and_then(manifest_path)
    {
        targets.insert(target);
    }
    if let Some(bin) = manifest.get("bin") {
        match bin {
            serde_json::Value::String(path) => {
                targets.extend(manifest_path(path));
            }
            serde_json::Value::Object(commands) => {
                targets.extend(
                    commands
                        .values()
                        .filter_map(serde_json::Value::as_str)
                        .filter_map(manifest_path),
                );
            }
            _ => {}
        }
    }

    targets
}

/// A manifest path in the same relative spelling `exports` targets use here.
fn manifest_path(path: &str) -> Option<String> {
    let path = path.trim().trim_start_matches("./").trim_start_matches('/');
    if path.is_empty() || path.starts_with("../") {
        return None;
    }
    Some(path.to_string())
}

/// Whether `path` is a root metadata file npm publishes regardless of `files`.
fn is_always_published_root_file(path: &str) -> bool {
    if path.contains('/') {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    lower == "package.json"
        || has_optional_extension(&lower, "readme")
        || has_optional_extension(&lower, "license")
        || has_optional_extension(&lower, "licence")
}

/// Whether `name` is exactly `stem`, or `stem` followed by an extension.
fn has_optional_extension(name: &str, stem: &str) -> bool {
    name == stem
        || name
            .strip_prefix(stem)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

/// Apply the `files` allowlist in order, including negations.
fn files_publish(path: &str, files: &[&str]) -> bool {
    if is_always_published_root_file(path) {
        return true;
    }
    let mut published = false;
    for entry in files {
        let Some(pattern) = file_pattern(entry) else {
            continue;
        };
        if !file_pattern_matches(pattern.body, path)
            && (pattern.include || !file_negation_matches_anywhere(pattern.body, path))
        {
            continue;
        }
        published = pattern.include;
    }
    published
}

/// One parsed `files` entry.
struct FilePattern<'a> {
    include: bool,
    body: &'a str,
}

/// Parse a `files` entry into its include/exclude direction and pattern body.
fn file_pattern(entry: &str) -> Option<FilePattern<'_>> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    let (include, body) = entry
        .strip_prefix('!')
        .map_or((true, entry), |body| (false, body));
    let body = body
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/');
    if body.is_empty() {
        return None;
    }
    Some(FilePattern { include, body })
}

/// Whether a single `files` pattern selects `path`.
fn file_pattern_matches(pattern: &str, path: &str) -> bool {
    if !has_glob_syntax(pattern) {
        return path == pattern
            || path
                .strip_prefix(pattern)
                .is_some_and(|rest| rest.starts_with('/'));
    }
    if glob_matches(pattern, path) {
        return true;
    }
    let mut rest = path;
    while let Some((ancestor, _)) = rest.rsplit_once('/') {
        if glob_matches(pattern, ancestor) {
            return true;
        }
        rest = ancestor;
    }
    false
}

/// Slashless negations behave like ignore rules and subtract matching names at any depth.
fn file_negation_matches_anywhere(pattern: &str, path: &str) -> bool {
    if pattern.contains('/') {
        return false;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if has_glob_syntax(pattern) {
        glob_matches(pattern, name)
    } else {
        pattern == name
    }
}

/// Whether `pattern` needs glob matching rather than literal path matching.
fn has_glob_syntax(pattern: &str) -> bool {
    pattern
        .chars()
        .any(|character| matches!(character, '*' | '?' | '[' | '{'))
}

/// Match one root-relative glob pattern against one root-relative path.
fn glob_matches(pattern: &str, path: &str) -> bool {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .backslash_escape(true)
        .build()
        .map(|glob| glob.compile_matcher().is_match(path))
        .unwrap_or(false)
}

/// Every string an `exports` value can reach, however it is nested.
///
/// `exports` is a tree of subpaths, condition maps and arrays of fallbacks,
/// and a target can be at any depth. Walking it rather than reading the first
/// level is what makes the check see `{ ".": { "flow": …, "default": … } }`,
/// which is the shape a uf library actually publishes.
fn collect_targets(value: &serde_json::Value, into: &mut Vec<String>) {
    match value {
        serde_json::Value::String(target) => into.push(target.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_targets(item, into);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_targets(item, into);
            }
        }
        _ => {}
    }
}

/// The `--size-report` table, or nothing when it was not asked for.
fn largest_rows(size: &BundleReport, size_report: bool) -> Vec<(String, String, String)> {
    if !size_report {
        return Vec::new();
    }
    size.largest_assets(BudgetMetric::Gzip)
        .iter()
        .take(LARGEST_ASSETS_SHOWN)
        .map(|asset| {
            (
                asset.path.to_string(),
                asset.size.gzip.to_string(),
                asset.size.raw.to_string(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
