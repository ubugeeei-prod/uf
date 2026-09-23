//! `uf check`: the linter, and then Flow's own type inference.
//!
//! `uf lint` answers whether the source is well-formed and idiomatic. `uf
//! check` answers that *and* whether the types hold, by running the checker
//! from `uf_check`. Both halves render through the same `uf_term` code frames,
//! so a type error and a lint error look alike on screen — the difference a
//! reader cares about is the rule or error code in the header, not the shape of
//! the block.
//!
//! When the checker is not compiled in, the type-checking half reports itself
//! unavailable and `uf check` is exactly `uf lint` under another name.

use anyhow::{Result, bail};
use camino::Utf8Path;
use serde_json::{Value, json};
#[cfg(feature = "upstream-typecheck")]
use uf_check::{
    BuiltinsTiming, CheckCache, CheckError, CheckLimits, CheckReport, ModuleRequires, Source,
    TypeDiagnostic, active_backend, backend_name, check_sources_cached, lib_paths,
    module_closure_cached,
};
#[cfg(feature = "upstream-typecheck")]
use uf_infra::FxHashSet;
use uf_lint::{LintReport, Severity, SourceFile};
use uf_term::Status;
#[cfg(feature = "upstream-typecheck")]
use uf_term::{CodeFrame, DiagnosticLevel, KeyValue, Tone, push_spaces};

use crate::commands::lint::{
    LintCommand, LintRun, Verdict, group_by_path, lint_payload, plugins, render_fix_summary,
    render_group, render_unreadable, render_verdict, run_lint, severity_count,
};
use crate::fix::files::{FixMode, FixSummary, fix_project};
#[cfg(feature = "upstream-typecheck")]
use crate::support::problem_summary;
use crate::support::{plural, project_label, render_ignore_deprecation};
use crate::ui::Ui;

/// How many untyped imports are named before the list is summarised.
#[cfg(feature = "upstream-typecheck")]
const UNTYPED_MODULES_SHOWN: usize = 5;

/// Where a check's time went, when `UF_PROFILE` asks.
///
/// The linter and the checker are full of `profile_span!`s already, and
/// `uf_profiler`'s header says what they are for: a benchmark says the run got
/// slower, and a profile says which part did. Until this existed nothing
/// outside `uf_check`'s own examples could turn them on, so the spans could
/// only be read over a batch of one file assembled by a test — never over a
/// project, which is where the interesting half of the time is. `uf check` on
/// this repository spends a tenth of a warm run inside inference; the question
/// this answers is what the other nine tenths are.
///
/// Off unless asked for. Each span costs a relaxed atomic load while the gate
/// is shut, and the table is written to stderr so that `--json` stays one
/// document on stdout.
struct Profile {
    started: Option<std::time::Instant>,
}

impl Profile {
    /// Open the gate when `UF_PROFILE` is set to anything but the empty string.
    fn start() -> Self {
        let asked = std::env::var_os("UF_PROFILE").is_some_and(|value| !value.is_empty());
        if !asked {
            return Self { started: None };
        }
        uf_profiler::scope::enable();
        Self {
            started: Some(std::time::Instant::now()),
        }
    }

    /// The spans, deepest cost first, or nothing at all.
    ///
    /// Self time rather than inclusive, because inclusive always names the
    /// outermost span and every reader already knows the whole run is the
    /// whole run. Allocations are not reported: counting them needs the
    /// counting allocator, and this binary has `mimalloc` — a column of zeroes
    /// would say something false. `uf_check`'s `alloc_report` example is where
    /// the allocation question is asked.
    fn render(&self) {
        let Some(started) = self.started else {
            return;
        };
        let elapsed = started.elapsed();
        uf_profiler::scope::flush_thread_spans();
        let mut spans = uf_profiler::scope::take_collected_spans();
        // One row per name: a span entered on the check thread and on this one
        // arrives as two records, and they are one question.
        spans.sort_by_key(|span| span.name);
        let mut merged: Vec<uf_profiler::ScopeRecord> = Vec::new();
        for span in spans {
            match merged.last_mut() {
                Some(last) if last.name == span.name => {
                    last.hits += span.hits;
                    last.inclusive += span.inclusive;
                    last.self_time += span.self_time;
                    last.slowest = last.slowest.max(span.slowest);
                }
                _ => merged.push(span),
            }
        }
        merged.sort_by_key(|span| std::cmp::Reverse(span.self_time));
        eprintln!("\n  {elapsed:.2?} in {} spans", merged.len());
        eprintln!("  {:<28}{:>8}{:>12}{:>12}", "span", "hits", "self", "total");
        for span in &merged {
            eprintln!(
                "  {:<28}{:>8}{:>12}{:>12}",
                span.name,
                span.hits,
                format!("{:.2?}", span.self_time),
                format!("{:.2?}", span.inclusive),
            );
        }
    }
}

/// What the run did, beyond what the report describes.
///
/// The batch counts are reported because they are the difference between "your
/// file is clean" and "your file is clean, and so are the eleven modules that
/// had to be typed to say so" — and because they are what explains the time.
#[cfg(feature = "upstream-typecheck")]
#[derive(Clone)]
struct Batch {
    /// Files the reader asked about.
    requested: usize,
    /// Modules in the batch only because those files import them.
    imported: usize,
    /// What the closure walk paid for the shared builtin environment, if it
    /// needed one before inference did.
    builtins: Option<BuiltinsTiming>,
    /// How many library definitions the project added to Flow's own.
    ///
    /// Reported because it is the difference between "this project declares
    /// nothing" and "uf did not find what it declares": both look like a clean
    /// footer and one of them is a project about to be told its own types do
    /// not exist.
    libdefs: usize,
    /// The dependencies typed from their TypeScript declarations because they
    /// ship no Flow, each with how many of its places are `any`.
    ///
    /// Reported once per package, because a package typed from a translation
    /// is typed less completely than one typed from Flow its authors wrote,
    /// and a reader deciding how far to trust a clean check has to know by
    /// how much.
    translated: Vec<declarations::TranslatedPackage>,
    /// What `--explain-any` asked about, when it was passed.
    explained: Option<declarations::Explanation>,
}

/// Severity counts from the type-checking half of `uf check`.
#[derive(Clone, Copy)]
enum TypeSeverity {
    /// Type errors.
    Error,
    /// Type warnings.
    Warning,
}

/// What the type-checking half of `uf check` produced.
///
/// The three cases are genuinely different and a reader must be able to tell
/// them apart: no checker in this build, a clean or failing check, and a
/// checker that could not run at all.
enum TypeCheck {
    /// No checker was compiled in.
    Unavailable,
    /// Inference ran over the project.
    ///
    /// The batch beside the report is not derivable from it: a report describes
    /// the files it was handed, and which of them a *reader* asked about is the
    /// caller's question.
    ///
    /// Boxed because it is several times the size of the other variants —
    /// the translated packages and an `--explain-any` answer ride in it — and
    /// there is one of these per run, so the allocation is nothing.
    #[cfg(feature = "upstream-typecheck")]
    Checked(CheckReport, Box<Batch>),
    /// Inference could not run.
    #[cfg(feature = "upstream-typecheck")]
    Failed(CheckError),
}

impl TypeCheck {
    #[cfg(feature = "upstream-typecheck")]
    fn report(&self) -> Option<&CheckReport> {
        match self {
            Self::Checked(report, _) => Some(report),
            Self::Unavailable | Self::Failed(_) => None,
        }
    }

    #[cfg(feature = "upstream-typecheck")]
    fn diagnostics(&self) -> &[TypeDiagnostic] {
        self.report()
            .map_or(&[][..], |report| report.diagnostics.as_slice())
    }

    fn count(&self, severity: TypeSeverity) -> usize {
        #[cfg(feature = "upstream-typecheck")]
        {
            let severity = match severity {
                TypeSeverity::Error => uf_check::Severity::Error,
                TypeSeverity::Warning => uf_check::Severity::Warning,
            };
            self.report().map_or(0, |report| report.count(severity))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = severity;
            0
        }
    }

    /// What the batch was made of, for a run that produced one.
    #[cfg(feature = "upstream-typecheck")]
    fn batch(&self) -> Option<&Batch> {
        match self {
            Self::Checked(_, batch) => Some(batch),
            Self::Unavailable | Self::Failed(_) => None,
        }
    }

    fn status(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            #[cfg(feature = "upstream-typecheck")]
            Self::Checked(..) => "checked",
            #[cfg(feature = "upstream-typecheck")]
            Self::Failed(_) => "failed",
        }
    }
}

fn type_backend_name() -> String {
    #[cfg(feature = "upstream-typecheck")]
    {
        backend_name(active_backend()).to_string()
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    {
        "unavailable".to_string()
    }
}

pub(crate) fn check(
    cwd: &Utf8Path,
    ui: &mut Ui,
    json: bool,
    fix: FixMode,
    paths: &[String],
    explain_any: Option<&str>,
) -> Result<()> {
    let profile = Profile::start();
    let started = std::time::Instant::now();
    let mut progress = ui.progress();
    // Before the scan, and only the *lint* fixes: `uf check` is `uf lint` plus
    // inference, and inference has no fix catalogue of its own. A type error
    // is not something uf knows how to rewrite.
    let fixed = if fix.writes() {
        progress.draw("applying fixes");
        Some(fix_project(cwd, paths, fix)?)
    } else {
        None
    };
    progress.draw("scanning sources");
    uf_profiler::profile_span!("cli::lint_run");
    let LintRun {
        report: lint,
        sources,
        unreadable,
        root,
        available,
        ignore_deprecation,
        project_rules,
    } = run_lint(cwd, paths)?;
    progress.draw("type checking");
    let types = type_check(&sources, &available, &root, explain_any);
    progress.finish();
    drop(progress);

    if json {
        let mut body = payload(&lint, &types, fixed.as_ref());
        if project_rules.ran {
            body["projectRules"] = plugins::payload(&project_rules);
        }
        ui.json(&body)?;
    } else {
        render(
            ui,
            project_label(&root),
            Report {
                lint: &lint,
                sources: &sources,
                types: &types,
                fixed: fixed.as_ref(),
            },
            started,
        );
        plugins::render(ui, &project_rules);
        render_unreadable(ui, &unreadable);
        render_ignore_deprecation(ui, ignore_deprecation);
    }

    // Before the counts: a file nobody could read has no diagnostics, and
    // "0 errors" over it would be a lie.
    if !unreadable.is_empty() {
        bail!("{} could not be read", plural(unreadable.len(), "file"));
    }
    // `uf check` is `uf lint` plus inference, so a project rule that could not
    // answer fails it for the reason it fails `uf lint`.
    if !project_rules.problems.is_empty() {
        bail!(
            "{} kept project rules from answering",
            plural(project_rules.problems.len(), "problem")
        );
    }
    // After the report and before the verdict: a run that ends in `bail!` is
    // exactly the run somebody profiling wants the table from, and a `?` on
    // the way out would swallow it.
    profile.render();
    let errors = severity_count(&lint, Severity::Error) + types.count(TypeSeverity::Error);
    if errors > 0 {
        bail!(
            "{} failed with {}",
            LintCommand::Check.title(),
            plural(errors, "error")
        );
    }
    #[cfg(feature = "upstream-typecheck")]
    if let TypeCheck::Failed(error) = types {
        return Err(error.into());
    }
    Ok(())
}

/// Run inference over the sources the linter collected, and the modules those
/// sources import.
///
/// Two kinds of diagnostic are dropped rather than rendered. **Syntax errors**,
/// because `uf lint` has already reported the same one with its own rule id and
/// printing it twice helps nobody — `uf_check` reports them because a library
/// caller needs to know why inference did not run. And **anything about a file
/// nobody asked about**, because a dependency is in the batch to be typed
/// against, not to be reported on.
#[cfg(feature = "upstream-typecheck")]
fn type_check(
    sources: &[SourceFile],
    available: &[SourceFile],
    root: &Utf8Path,
    explain_any: Option<&str>,
) -> TypeCheck {
    let limits = CheckLimits::default();
    // The project's own library definitions, before anything is merged: they
    // are part of the environment every file is checked in, so a batch that
    // asked for the environment first would be handed one without them.
    // ubugeeei-prod/uf#480.
    uf_profiler::profile_span!("cli::type_check");
    let libdefs = match lib_paths(root.as_std_path()) {
        Ok(paths) => libdefs::load(root, &paths),
        Err(error) if error.is_unavailable() => return TypeCheck::Unavailable,
        Err(error) => return TypeCheck::Failed(error),
    };
    let libs: Vec<Source<'_>> = libdefs.iter().map(as_input).collect();
    // A library definition is not a file to check. It *declares* the
    // environment every other file is checked in — `declare module` and a
    // top-level `declare type` are library syntax, and a source file that used
    // them would be reported for using them — so it is merged and then taken
    // out of the batch, which is what `flow check` does with a lib file too.
    // The scan collects `flow-typed/` like any other directory, so without
    // this the same file would be both the environment and a file checked
    // against it.
    let declared: FxHashSet<&str> = libs.iter().map(|lib| lib.path).collect();
    let checked: Vec<&SourceFile> = sources
        .iter()
        .filter(|source| !declared.contains(source.path.as_str()))
        .collect();
    let seeds: Vec<&str> = checked.iter().map(|source| source.path.as_str()).collect();
    // What the walk searches. `available` is empty when `paths` selected
    // everything, and then the selection already is every file the scan found.
    let project: Vec<&SourceFile> = if available.is_empty() {
        checked.clone()
    } else {
        available
            .iter()
            .filter(|source| !declared.contains(source.path.as_str()))
            .collect()
    };

    // Under the project root, because that is what the cache is about: the same
    // sources checked from two roots are two projects, and `.uf/` is where uf
    // already keeps per-project state that `.gitignore` covers.
    //
    // Opened before the walk rather than before the check, because the walk
    // reads it too: a file the last check filed a record for is read, not
    // parsed, and so is whether Flow's libdefs declare what it imports.
    let cache = {
        uf_profiler::profile_span!("cli::cache_open");
        CheckCache::open(root.as_std_path())
    };
    let requires = cache
        .clone()
        .map_or_else(ModuleRequires::default, ModuleRequires::backed_by);
    let Closure {
        paths: batch_paths,
        installed,
        declarations,
        builtins,
    } = match closure(root, &project, &seeds, &libs, &limits, requires) {
        Ok(closure) => closure,
        Err(error) if error.is_unavailable() => return TypeCheck::Unavailable,
        Err(error) => return TypeCheck::Failed(error),
    };

    let reached: FxHashSet<&str> = batch_paths.iter().map(String::as_str).collect();
    let batch: Vec<Source<'_>> = project
        .iter()
        .copied()
        .chain(installed.iter())
        .chain(declarations.sources().iter())
        .filter(|source| reached.contains(source.path.as_str()))
        .map(as_input)
        .collect();
    let mut counts = Batch {
        requested: checked.len(),
        imported: batch.len().saturating_sub(checked.len()),
        builtins,
        libdefs: libs.len(),
        // Filled once the check has run: what a translated package reports
        // includes the errors Flow finds inside it.
        translated: Vec::new(),
        explained: None,
    };

    // Before the run adds to it, once, and silently. A check is about to write
    // one record per file it could not answer from disk, and this is what
    // stops the directory from being every record every build of `uf` has ever
    // produced — which it was, without a ceiling, until #218.
    if let Some(cache) = cache.as_ref() {
        uf_profiler::profile_span!("cli::cache_sweep");
        cache.sweep();
    }
    match check_sources_cached(&batch, &libs, &limits, cache.as_ref()) {
        Ok(mut report) => {
            // Before the filter below, which drops every diagnostic about a
            // file nobody asked about — and a translation is such a file.
            counts.translated = declarations.translated(&report.diagnostics);
            counts.explained =
                explain_any.map(|name| declarations.explain(name, &report.diagnostics));
            let asked_about: FxHashSet<&str> =
                checked.iter().map(|source| source.path.as_str()).collect();
            report.diagnostics.retain(|diagnostic| {
                // A dependency was checked so that the files asked about could
                // be typed against it, not so that its own errors could be
                // reported. `uf check packages/form/watch.js` must not fail on
                // a file the author did not name — and in a project that has
                // errors elsewhere, one that did would be unusable.
                diagnostic.kind != uf_check::DiagnosticKind::Parse
                    && asked_about.contains(diagnostic.primary.path.as_str())
            });
            TypeCheck::Checked(report, Box::new(counts))
        }
        Err(error) if error.is_unavailable() => TypeCheck::Unavailable,
        Err(error) => TypeCheck::Failed(error),
    }
}

/// What [`closure`] found: the paths the batch reaches, and the sources it had
/// to bring in from outside the scan to reach them.
#[cfg(feature = "upstream-typecheck")]
struct Closure {
    /// Every path the batch holds, in the walk's order.
    paths: Vec<String>,
    /// Packages read from `node_modules`, Flow they ship.
    installed: Vec<SourceFile>,
    /// Packages typed from a translation of their TypeScript declarations.
    declarations: declarations::Declarations,
    /// What the walk paid for the builtin environment, if it needed one.
    builtins: Option<BuiltinsTiming>,
}

/// The batch `seeds` are checked in: what was asked about plus what it imports,
/// from `project` and from the packages installed under `root`.
///
/// Because an import is only typed against a file in the same batch. A run
/// that skipped this checked its files against nothing: every
/// `import type { Control } from "@uniflowed/form"` was an `any`-typed value,
/// so the annotations written against it were neither right nor wrong —
/// ubugeeei-prod/uf#403.
///
/// In rounds, because a package read from `node_modules` imports packages of
/// its own. It terminates because `read` never lets a package be looked for
/// twice and there are finitely many of them.
///
/// `uf check` and the language server's session both assemble their batch
/// here, so a hover is answered against the same modules a check is.
#[cfg(feature = "upstream-typecheck")]
fn closure(
    root: &Utf8Path,
    project: &[&SourceFile],
    seeds: &[&str],
    libs: &[Source<'_>],
    limits: &CheckLimits,
    mut requires: ModuleRequires,
) -> Result<Closure, CheckError> {
    let mut installed: Vec<SourceFile> = Vec::new();
    let mut read: FxHashSet<String> = FxHashSet::default();
    // The packages with no Flow and with TypeScript declarations, typed from a
    // translation of those. ubugeeei-prod/uf#946.
    let mut declarations = declarations::Declarations::open(root);
    let mut builtins = None;
    // `requires` is held across the rounds rather than rebuilt inside each one:
    // what a file imports is the same answer every time it is asked, and asking
    // again was the largest row in a warm check's profile. Backed by the check
    // cache, it answers the first round from disk as well. See
    // `uf_check::ModuleRequires`.
    loop {
        // In its own scope: the walk borrows `installed` and `declarations`,
        // and the round that follows it grows both.
        let (paths, unresolved) = {
            uf_profiler::profile_span!("cli::closure_round");
            let pool: Vec<Source<'_>> = project
                .iter()
                .copied()
                .chain(installed.iter())
                .chain(declarations.sources().iter())
                .map(as_input)
                .collect();
            let closure = module_closure_cached(seeds, &pool, libs, limits, &mut requires)?;
            if builtins.is_none() {
                builtins = closure.builtins;
            }
            (
                closure
                    .sources
                    .iter()
                    .map(|source| source.path.to_owned())
                    .collect::<Vec<String>>(),
                closure.unresolved,
            )
        };
        let more = {
            uf_profiler::profile_span!("cli::load_packages");
            dependencies::load_packages(root, &unresolved, &mut read, &mut declarations)
        };
        // A package translated this round changes the batch without adding a
        // file to `installed`, so it keeps the rounds going too — its
        // declarations import packages of their own.
        let translated = declarations.flush();
        if more.is_empty() && !translated {
            return Ok(Closure {
                paths,
                installed,
                declarations,
                builtins,
            });
        }
        installed.extend(more);
    }
}

/// A whole project as one batch, owned: what the language server's type
/// session is loaded with.
#[cfg(feature = "upstream-typecheck")]
pub(crate) struct ProjectBatch {
    /// The project's library definitions, in merge order.
    pub(crate) libs: Vec<SourceFile>,
    /// Every source the scan found, and every module those import.
    pub(crate) sources: Vec<SourceFile>,
}

/// Assemble the batch `uf check` with no paths would check, from sources
/// already scanned: every one of them is a seed.
///
/// # Errors
///
/// As `uf check`'s type half: a `.flowconfig` that does not parse, or a
/// closure walk the checker refused.
#[cfg(feature = "upstream-typecheck")]
pub(crate) fn project_batch(
    root: &Utf8Path,
    scanned: &[SourceFile],
) -> Result<ProjectBatch, CheckError> {
    let limits = CheckLimits::default();
    let libdefs = libdefs::load(root, &lib_paths(root.as_std_path())?);
    let declared: FxHashSet<&str> = libdefs.iter().map(|lib| lib.path.as_str()).collect();
    let project: Vec<&SourceFile> = scanned
        .iter()
        .filter(|source| !declared.contains(source.path.as_str()))
        .collect();
    let seeds: Vec<&str> = project.iter().map(|source| source.path.as_str()).collect();
    let libs: Vec<Source<'_>> = libdefs.iter().map(as_input).collect();
    let found = closure(
        root,
        &project,
        &seeds,
        &libs,
        &limits,
        ModuleRequires::default(),
    )?;
    let reached: FxHashSet<&str> = found.paths.iter().map(String::as_str).collect();
    let sources = project
        .iter()
        .copied()
        .chain(found.installed.iter())
        .chain(found.declarations.sources().iter())
        .filter(|source| reached.contains(source.path.as_str()))
        .cloned()
        .collect();
    Ok(ProjectBatch {
        libs: libdefs,
        sources,
    })
}

/// One scanned file as the checker takes it.
#[cfg(feature = "upstream-typecheck")]
fn as_input(source: &SourceFile) -> Source<'_> {
    Source::new(&source.path, &source.source)
}

#[cfg(not(feature = "upstream-typecheck"))]
fn type_check(
    _sources: &[SourceFile],
    _available: &[SourceFile],
    _root: &Utf8Path,
    _explain_any: Option<&str>,
) -> TypeCheck {
    TypeCheck::Unavailable
}

fn payload(lint: &LintReport, types: &TypeCheck, fixed: Option<&FixSummary>) -> Value {
    let mut value = lint_payload(LintCommand::Check, lint, fixed);
    let errors = severity_count(lint, Severity::Error) + types.count(TypeSeverity::Error);
    let warnings = severity_count(lint, Severity::Warn) + types.count(TypeSeverity::Warning);

    let type_check = type_check_payload(types);

    value["errors"] = json!(errors);
    value["warnings"] = json!(warnings);
    value["typeCheck"] = type_check;
    value
}

#[cfg(feature = "upstream-typecheck")]
fn type_check_payload(types: &TypeCheck) -> Value {
    let mut value = json!({
        "backend": type_backend_name(),
        "status": types.status(),
        "diagnostics": types.diagnostics(),
    });
    if let Some(report) = types.report() {
        value["filesChecked"] = json!(report.files_checked);
        if let Some(batch) = types.batch() {
            value["requested"] = json!(batch.requested);
            value["imported"] = json!(batch.imported);
            value["libdefs"] = json!(batch.libdefs);
            value["translatedPackages"] = json!(batch.translated);
            if let Some(explained) = &batch.explained {
                value["explainAny"] = json!(explained);
            }
        }
        value["filesSkipped"] = json!(report.files_skipped);
        value["filesFromCache"] = json!(report.files_from_cache);
        value["elapsedMs"] = json!(report.elapsed.as_secs_f64() * 1000.0);
        let builtins = types
            .batch()
            .and_then(|batch| batch.builtins)
            .unwrap_or(report.builtins);
        value["builtinsMs"] = json!(builtins.cold_elapsed.as_secs_f64() * 1000.0);
        value["builtinsCold"] = json!(builtins.cold);
        value["builtinsNeeded"] = json!(builtins.needed);
        value["untypedModules"] = json!(report.untyped_modules);
        value["hostConditionalModules"] = json!(report.host_conditional_modules);
    }
    if let TypeCheck::Failed(error) = types {
        value["error"] = json!(error.to_string());
    }
    value
}

#[cfg(not(feature = "upstream-typecheck"))]
fn type_check_payload(types: &TypeCheck) -> Value {
    json!({
        "backend": type_backend_name(),
        "status": types.status(),
        "diagnostics": [],
    })
}

/// Everything one `uf check` run found, for the report.
struct Report<'a> {
    lint: &'a LintReport,
    sources: &'a [SourceFile],
    types: &'a TypeCheck,
    fixed: Option<&'a FixSummary>,
}

fn render(ui: &mut Ui, project: &str, report: Report<'_>, started: std::time::Instant) {
    let Report {
        lint,
        sources,
        types,
        fixed,
    } = report;
    let lint_errors = severity_count(lint, Severity::Error);
    let lint_warnings = severity_count(lint, Severity::Warn);

    ui.render(|renderer, out| {
        renderer.banner(out, LintCommand::Check.title(), Some(project));
    });

    // As in `uf lint`: the file headers name the files, so nothing after the
    // findings lists them again.
    for group in group_by_path(&lint.diagnostics) {
        render_group(ui, group, sources);
    }

    render_type_diagnostics(ui, sources, types);
    // What inference read and what it could not type, before the verdict
    // rather than after it: the verdict is the line a reader scrolls to, so
    // it is the last one.
    render_type_footer(ui, types);

    if let Some(fixed) = fixed {
        render_fix_summary(ui, fixed);
    }
    render_verdict(
        ui,
        LintCommand::Check,
        lint,
        Verdict {
            errors: lint_errors + types.count(TypeSeverity::Error),
            warnings: lint_warnings + types.count(TypeSeverity::Warning),
            elapsed: started.elapsed(),
            fixed: fixed.is_some(),
        },
    );
}

/// Type diagnostics, grouped by file, as code frames.
#[cfg(feature = "upstream-typecheck")]
fn render_type_diagnostics(ui: &mut Ui, sources: &[SourceFile], types: &TypeCheck) {
    let diagnostics = types.diagnostics();
    if diagnostics.is_empty() {
        return;
    }

    let mut start = 0;
    while start < diagnostics.len() {
        let path = diagnostics[start].primary.path.as_str();
        let mut end = start;
        while end < diagnostics.len() && diagnostics[end].primary.path == path {
            end += 1;
        }
        render_type_group(ui, sources, &diagnostics[start..end]);
        start = end;
    }
}

#[cfg(not(feature = "upstream-typecheck"))]
fn render_type_diagnostics(_ui: &mut Ui, _sources: &[SourceFile], _types: &TypeCheck) {}

#[cfg(feature = "upstream-typecheck")]
fn render_type_group(ui: &mut Ui, sources: &[SourceFile], group: &[TypeDiagnostic]) {
    let path = group[0].primary.path.as_str();
    let lines: Option<Vec<&str>> = sources
        .iter()
        .find(|source| source.path == path)
        .map(|source| source.source.lines().collect());
    let errors = group
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .count();
    let header = problem_summary(errors, group.len() - errors);
    let messages: Vec<String> = group.iter().map(TypeDiagnostic::message_text).collect();
    // `[1]` in the message and `[1]` on the note have to be the same marker, or
    // a reader has no way to tell two references apart.
    let notes: Vec<Vec<String>> = group
        .iter()
        .map(|diagnostic| {
            diagnostic
                .related
                .iter()
                .map(|related| format!("[{}] is here", related.id))
                .collect()
        })
        .collect();

    // As in `lint::render_group`: the header draws the path itself, and the
    // frames under it are `CodeFrame`'s job. #640.
    let drawn = uf_term::safe_path(path);
    ui.render(|renderer, out| {
        renderer.theme().path.paint(renderer.color(), &drawn, out);
        out.push_str("  ");
        renderer.theme().muted.paint(renderer.color(), &header, out);
        out.push('\n');
        renderer.blank(out);

        for ((diagnostic, message), notes) in group.iter().zip(&messages).zip(&notes) {
            let level = if diagnostic.is_error() {
                DiagnosticLevel::Error
            } else {
                DiagnosticLevel::Warning
            };
            let line = diagnostic.primary.start.line as usize;
            let column = diagnostic.primary.start.column as usize;
            let mut frame = CodeFrame::new(level, message, path, line, column);
            if let Some(code) = diagnostic.code {
                frame = frame.with_rule(code);
            }
            if let Some(span) = diagnostic.primary.single_line_len() {
                frame = frame.with_span(span);
            }
            if let Some(source_line) = lines
                .as_ref()
                .and_then(|lines| lines.get(line.saturating_sub(1)).copied())
            {
                frame = frame.with_source_line(source_line);
            }
            renderer.code_frame_at(out, &frame, 2);

            // Flow's messages point at other locations by number; without the
            // locations themselves a reader cannot follow `[1]` anywhere.
            for (related, note_label) in diagnostic.related.iter().zip(notes) {
                let related_line = related.span.start.line as usize;
                let mut note = CodeFrame::new(
                    DiagnosticLevel::Note,
                    note_label,
                    related.span.path.as_str(),
                    related_line,
                    related.span.start.column as usize,
                );
                if let Some(span) = related.span.single_line_len() {
                    note = note.with_span(span);
                }
                let related_source = lines
                    .as_ref()
                    .filter(|_| related.span.path == path)
                    .and_then(|lines| lines.get(related_line.saturating_sub(1)).copied());
                if let Some(source_line) = related_source {
                    note = note.with_source_line(source_line);
                }
                renderer.code_frame_at(out, &note, 4);
            }
            renderer.blank(out);
        }
    });
}

/// One line saying what the type checker did, under the verdict.
fn render_type_footer(ui: &mut Ui, types: &TypeCheck) {
    match types {
        TypeCheck::Unavailable => ui.render(|renderer, out| {
            renderer.status(
                out,
                Status::Info,
                "type inference is not compiled into this build",
            );
            renderer.blank(out);
        }),
        #[cfg(feature = "upstream-typecheck")]
        TypeCheck::Failed(error) => {
            let detail = error.to_string();
            ui.render(|renderer, out| {
                renderer.status(out, Status::Warn, &detail);
                renderer.blank(out);
            });
        }
        #[cfg(feature = "upstream-typecheck")]
        TypeCheck::Checked(report, batch) => {
            let files = report.files_checked.to_string();
            // Only shown when a selection pulled more in. A whole-project run
            // imports nothing it was not also asked about, and a reader should
            // not have to work that out from a zero.
            let requested = format!(
                "{} of {}",
                batch.requested,
                batch.requested + batch.imported
            );
            let inference = format!("{:.1?}", report.elapsed);
            let builtins_timing = batch.builtins.unwrap_or(report.builtins);
            let builtins = if builtins_timing.needed {
                format!(
                    "{:.1?} ({})",
                    builtins_timing.cold_elapsed,
                    if builtins_timing.cold { "cold" } else { "warm" }
                )
            } else {
                String::from("not needed")
            };
            // Only shown when it happened. A project with nothing opted out
            // should not have to read a line saying so.
            let skipped = report.files_skipped.to_string();
            // Only shown when the project declares any; see below.
            let libdefs = batch.libdefs.to_string();
            // Only shown when the cache answered something: a project being
            // checked for the first time should not have to read a zero.
            let cached = format!("{} of {files}", report.files_from_cache);
            let mut rows = vec![
                KeyValue::toned("types checked", &files, Tone::Number),
                KeyValue::toned("inference", &inference, Tone::Muted),
                KeyValue::toned("builtins", &builtins, Tone::Muted),
            ];
            if report.files_from_cache > 0 {
                rows.insert(1, KeyValue::toned("unchanged", &cached, Tone::Muted));
            }
            if batch.imported > 0 {
                rows.insert(1, KeyValue::toned("asked about", &requested, Tone::Muted));
            }
            if report.files_skipped > 0 {
                rows.insert(1, KeyValue::toned("@noflow", &skipped, Tone::Muted));
            }
            // Only when the project has some. A project with no `[libs]` and
            // no `flow-typed` should not have to read a zero to find that out.
            if batch.libdefs > 0 {
                rows.insert(1, KeyValue::toned("libdefs", &libdefs, Tone::Muted));
            }
            let untyped = untyped_module_list(report);
            let host_conditional = host_conditional_module_list(report);
            let translated = translated_package_list(&batch.translated);
            let explained = batch.explained.as_ref().map(explanation_lines);
            ui.render(|renderer, out| {
                renderer.key_values(out, 2, &rows);
                if !untyped.is_empty() {
                    renderer.blank(out);
                    push_spaces(out, 2);
                    renderer.status(
                        out,
                        Status::Info,
                        "these imports are typed as any; uf resolved no module for them",
                    );
                    let items: Vec<&str> = untyped.iter().map(String::as_str).collect();
                    renderer.bullet_list(out, 4, &items);
                }
                if !host_conditional.is_empty() {
                    renderer.blank(out);
                    push_spaces(out, 2);
                    renderer.status(
                        out,
                        Status::Info,
                        "these packages only publish host-specific exports; uf check typed them as any instead of guessing a host",
                    );
                    let items: Vec<&str> = host_conditional.iter().map(String::as_str).collect();
                    renderer.bullet_list(out, 4, &items);
                }
                if !translated.is_empty() {
                    renderer.blank(out);
                    push_spaces(out, 2);
                    renderer.status(
                        out,
                        Status::Info,
                        "these packages ship no Flow; uf check typed them from their TypeScript declarations",
                    );
                    let items: Vec<&str> = translated.iter().map(String::as_str).collect();
                    renderer.bullet_list(out, 4, &items);
                }
                if let Some((heading, lines)) = &explained {
                    renderer.blank(out);
                    push_spaces(out, 2);
                    renderer.status(out, Status::Info, heading);
                    let items: Vec<&str> = lines.iter().map(String::as_str).collect();
                    renderer.bullet_list(out, 4, &items);
                }
                renderer.blank(out);
            });
        }
    }
}

/// `--explain-any`'s answer: a heading, then one line per place — the holes,
/// then the errors Flow reports inside the translation — each naming the
/// declaration file, the line, the declaration and why.
#[cfg(feature = "upstream-typecheck")]
fn explanation_lines(explained: &declarations::Explanation) -> (String, Vec<String>) {
    let package = &explained.package;
    if !explained.translated {
        return (
            format!(
                "uf check typed no package named {package} from TypeScript declarations in this \
                 run: nothing imported it, it ships Flow, or it has no declarations"
            ),
            Vec::new(),
        );
    }
    let heading = format!(
        "{package}: {} typed as any, {} inside its translation",
        plural(explained.holes.len(), "hole"),
        plural(explained.findings.len(), "Flow error"),
    );
    let mut lines = Vec::with_capacity(explained.holes.len() + explained.findings.len());
    for hole in &explained.holes {
        lines.push(format!(
            "{}:{} {} [{}] {}",
            hole.path, hole.line, hole.declaration, hole.construct, hole.reason
        ));
    }
    for finding in &explained.findings {
        lines.push(format!(
            "{}:{} {} [{}] {}",
            finding.path,
            finding.line,
            finding
                .declaration
                .as_deref()
                .unwrap_or("(outside a declaration)"),
            finding.code.as_deref().unwrap_or("error"),
            finding.message
        ));
    }
    (heading, lines)
}

/// Each translated package as the footer names it: its name, its version, and
/// how many of its places are `any` — once, however many files import it.
#[cfg(feature = "upstream-typecheck")]
fn translated_package_list(packages: &[declarations::TranslatedPackage]) -> Vec<String> {
    packages
        .iter()
        .map(|package| {
            let name = match (&package.types_package, &package.version) {
                (Some(types), Some(version)) => format!("{}, from {types}@{version}", package.name),
                (Some(types), None) => format!("{}, from {types}", package.name),
                (None, Some(version)) => format!("{}@{version}", package.name),
                (None, None) => package.name.clone(),
            };
            let holes = match package.holes {
                0 => "no holes".to_owned(),
                1 => "1 hole typed as any".to_owned(),
                holes => format!("{holes} holes typed as any"),
            };
            match package.findings {
                0 => format!("{name}: {holes}"),
                1 => format!("{name}: {holes}, 1 Flow error inside its translation"),
                findings => {
                    format!("{name}: {holes}, {findings} Flow errors inside its translation")
                }
            }
        })
        .collect()
}

/// The untyped imports to name on screen, with the tail summarised.
///
/// A project pulls in more packages than a terminal should list, and the full
/// set is always in `--json`.
#[cfg(feature = "upstream-typecheck")]
fn untyped_module_list(report: &CheckReport) -> Vec<String> {
    limited_module_list(&report.untyped_modules)
}

#[cfg(feature = "upstream-typecheck")]
fn host_conditional_module_list(report: &CheckReport) -> Vec<String> {
    limited_module_list(&report.host_conditional_modules)
}

#[cfg(feature = "upstream-typecheck")]
fn limited_module_list<T: std::fmt::Display>(modules: &[T]) -> Vec<String> {
    let mut named: Vec<String> = modules
        .iter()
        .take(UNTYPED_MODULES_SHOWN)
        .map(ToString::to_string)
        .collect();
    let overflow = modules.len().saturating_sub(UNTYPED_MODULES_SHOWN);
    if overflow > 0 {
        named.push(format!("and {overflow} more"));
    }
    named
}

#[cfg(feature = "upstream-typecheck")]
mod declarations;
#[cfg(feature = "upstream-typecheck")]
mod dependencies;
// `pub(crate)` rather than private: `uf lint` asks it which files are library
// definitions, in order not to report on them.
#[cfg(feature = "upstream-typecheck")]
pub(crate) mod libdefs;

#[cfg(test)]
mod tests;
