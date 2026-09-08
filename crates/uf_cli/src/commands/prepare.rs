//! `uf prepare`: the checks and code generation a commit should not go without.
//!
//! A pre-commit command, and the shape of one: it asks git what is staged,
//! regenerates the Flow types that are derived from sources rather than
//! written by hand, and then lints and format-checks the staged files. Every
//! step in [`uf_prepare::PreparePlan`] runs here, and nothing runs here that is
//! not in the plan — the list used to name six steps and perform one.
//!
//! # Order, and why generation comes first
//!
//! The generated files are Flow source in the project, so `uf lint` and
//! `uf fmt --check` see them. Generating after the checks would check the
//! previous run's output; generating before means a commit hook that writes a
//! file the linter then rejects fails loudly here rather than in CI.
//! `crates/uf_cli/tests/prepare.rs` pins that both generated files survive
//! both checks.
//!
//! # Which failures stop the run
//!
//! A generation step that fails stops it: the checks after it would be reading
//! types that were not written. A *check* that fails does not, because a
//! developer fixing a commit wants both lists at once rather than one, a fix,
//! and then the other. Whatever happens, `.uf/prepare.json` records what each
//! step actually did, including the steps that never started — a record that
//! says a check passed when it never ran is worse than no record.
//!
//! # `--fix`, and why it still fails
//!
//! With `--fix` the two check steps write instead of only reporting: the lint
//! step applies [`FixMode::Safe`] fixes and the format step formats. Safe
//! only, never `--fix-unsafe`: a hook that runs on every commit is the last
//! place an edit that can change what the program does should arrive
//! unasked-for, and the flag to ask for one is on `uf lint`.
//!
//! **A run that changed a file fails, even when nothing is wrong any more.**
//! What was rewritten is in the working tree and not in the index, so the
//! commit git is about to make is not the code uf just fixed. Failing stops
//! that commit and hands the developer a diff to read and stage. The
//! alternative — staging it here — would mean `uf prepare` adding content to a
//! commit somebody has already written the message for, which is the kind of
//! helpfulness that gets a hook uninstalled.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;
use serde_json::json;
use uf_config::{ResolvedConfig, load_config};
use uf_fmt::{NonFlowOutcome, format_source};
use uf_lint::{LintReport, Severity, SourceFile, lint_sources};
use uf_prepare::{
    GeneratedFile, GeneratedFileKind, NoStagedSet, PrepareStep, StagedFiles, StepOutcome,
    StepStatus, default_plan, discover_staged_files,
};
use uf_project::{ProjectFile, SourceKind, scan_selected_source_files};
use uf_router::write_router_manifest;
use uf_rsc::{
    BuildId, IGNORED_DIRECTORIES, ProjectScanOptions, SERVER_ACTION_TYPES_FILE_NAME,
    analyze_project, server_action_types, write_server_action_types,
};
use uf_term::{KeyValue, Status, Tone, push_spaces};

use crate::commands::lint::{group_by_path, render_group, severity_count};
use crate::fix::files::{FixMode, fix_files};
use crate::support::{
    enabled, plural, problem_summary, project_label, relative_to, unreadable_lines,
    write_json_file, yes_no,
};
use crate::ui::Ui;

/// How many staged paths are listed before the list is summarised.
///
/// A commit of two hundred files should not print two hundred lines above the
/// diagnostics somebody has to read. The full list is always in
/// `.uf/prepare.json`.
const STAGED_PATHS_SHOWN: usize = 10;

/// The width the step names are padded to, so the details line up.
const STEP_NAME_WIDTH: usize = 30;

/// What one step did, and the lines that belong under it.
struct StepReport {
    /// The record that goes into `prepare.json`.
    outcome: StepOutcome,
    /// Paths or messages to list under the step on screen.
    lines: Vec<String>,
}

impl StepReport {
    fn ok(step: PrepareStep, detail: impl Into<compact_str::CompactString>) -> Self {
        Self {
            outcome: StepOutcome::ok(step, detail),
            lines: Vec::new(),
        }
    }

    fn failed(step: PrepareStep, detail: impl Into<compact_str::CompactString>) -> Self {
        Self {
            outcome: StepOutcome::failed(step, detail),
            lines: Vec::new(),
        }
    }

    fn skipped(step: PrepareStep, detail: impl Into<compact_str::CompactString>) -> Self {
        Self {
            outcome: StepOutcome::skipped(step, detail),
            lines: Vec::new(),
        }
    }

    fn with_lines(mut self, lines: Vec<String>) -> Self {
        self.lines = lines;
        self
    }
}

/// Whether a failure in this step means the rest of the run is meaningless.
///
/// Generation, yes: everything after it reads what it was supposed to write.
/// A check, no: `uf prepare` reporting the lint errors *and* the unformatted
/// files is one round trip for the person fixing them instead of two.
const fn halts_the_run(step: PrepareStep) -> bool {
    match step {
        PrepareStep::DiscoverStagedFiles
        | PrepareStep::GenerateRouterTypes
        | PrepareStep::GenerateServerActionTypes => true,
        PrepareStep::RunLint | PrepareStep::RunFormatCheck => false,
    }
}

/// The state one run of the plan accumulates.
struct Run<'a> {
    resolved: &'a ResolvedConfig,
    /// Whether the two check steps write what they can rather than only
    /// reporting it.
    fix: bool,
    /// Whether anything this run wrote is now unstaged in the working tree.
    ///
    /// Set by either check step. It is what turns a `--fix` run that found
    /// nothing left to complain about into a failing one; see the module
    /// header.
    rewrote: bool,
    /// What git said, once the first step has asked.
    staged: StagedFiles,
    /// Files the generated ones are checked alongside, read once and shared by
    /// the two check steps.
    sources: Vec<ProjectFile>,
    /// Discovery failures, narrowed to the staged set.
    unreadable: Vec<String>,
    /// Whether discovery has run.
    scanned: bool,
    /// What was written, for `prepare.json`.
    generated: Vec<GeneratedFile>,
    /// The lint step's diagnostics and the sources they point into.
    ///
    /// Held rather than printed where they are found, because the report is
    /// drawn in one pass at the end: printing them from inside the step put
    /// the code frames above the banner, so the first thing on screen was a
    /// diagnostic from a command that had not introduced itself yet.
    diagnostics: Option<(LintReport, Vec<SourceFile>)>,
}

pub(crate) fn prepare(cwd: &camino::Utf8Path, ui: &mut Ui, fix: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let plan = default_plan();
    let mut run = Run {
        resolved: &resolved,
        fix,
        rewrote: false,
        // Replaced by the first step. Until then "no staged set" is the
        // widest, and so the safest, thing to believe.
        staged: StagedFiles::Unavailable(NoStagedSet::NotAWorkingTree),
        sources: Vec::new(),
        unreadable: Vec::new(),
        scanned: false,
        generated: Vec::new(),
        diagnostics: None,
    };

    let mut reports: Vec<StepReport> = Vec::with_capacity(plan.steps.len());
    let mut halted = false;
    for step in &plan.steps {
        if halted {
            reports.push(StepReport {
                outcome: StepOutcome::not_run(*step),
                lines: Vec::new(),
            });
            continue;
        }
        let report = match step {
            PrepareStep::DiscoverStagedFiles => run.discover_staged(),
            PrepareStep::GenerateRouterTypes => run.generate_router_types(),
            PrepareStep::GenerateServerActionTypes => run.generate_server_action_types(),
            PrepareStep::RunLint => run.run_lint(),
            PrepareStep::RunFormatCheck => run.run_format_check(),
        };
        halted = report.outcome.status.is_failure() && halts_the_run(*step);
        reports.push(report);
    }

    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("prepare.json");
    let failed: Vec<PrepareStep> = reports
        .iter()
        .filter(|report| report.outcome.status.is_failure())
        .map(|report| report.outcome.step)
        .collect();
    let outcomes: Vec<&StepOutcome> = reports.iter().map(|report| &report.outcome).collect();
    write_json_file(
        &manifest,
        &json!({
            "version": 2,
            "lintStagedCompatible": plan.lint_staged_compatible,
            "codeGenerator": plan.code_generator,
            "writeGeneratedFiles": plan.write_generated_files,
            "cache": plan.cache,
            "staged": staged_payload(&run.staged),
            "steps": outcomes,
            "generated": run.generated,
            "ok": failed.is_empty(),
        }),
    )?;

    render(
        ui,
        &resolved,
        &plan,
        &manifest,
        &run.staged,
        &reports,
        run.diagnostics.as_ref(),
    );

    if let Some(first) = failed.first() {
        // The step is named, and it is the *first* one that failed: a reader
        // fixing a commit starts at the top of the list, and a message that
        // named the last failure would send them to the end of it.
        //
        // A run that rewrote something always has a failed step to name —
        // each check step fails when it writes — so the staging instruction is
        // a clause on that message rather than one instead of it. Both halves
        // matter: which step, and what is now left to do.
        bail!(
            "uf prepare failed at {}{}{}",
            first.name(),
            if failed.len() > 1 {
                format!(" ({} steps failed)", failed.len())
            } else {
                String::new()
            },
            if run.rewrote {
                " — it rewrote files; review them, stage them, and commit again"
            } else {
                ""
            }
        );
    }
    Ok(())
}

impl Run<'_> {
    /// Ask git what is staged.
    fn discover_staged(&mut self) -> StepReport {
        let staged = match discover_staged_files(&self.resolved.root) {
            Ok(staged) => staged,
            Err(error) => {
                return StepReport::failed(PrepareStep::DiscoverStagedFiles, error.to_string());
            }
        };
        let report = match &staged {
            StagedFiles::Unavailable(reason) => StepReport::ok(
                PrepareStep::DiscoverStagedFiles,
                format!("checking every file: {}", reason.reason()),
            ),
            StagedFiles::Staged(files) if files.is_empty() => StepReport::ok(
                PrepareStep::DiscoverStagedFiles,
                "nothing is staged for commit",
            ),
            StagedFiles::Staged(files) => StepReport::ok(
                PrepareStep::DiscoverStagedFiles,
                plural(files.len(), "staged file"),
            )
            .with_lines(staged_lines(files)),
        };
        self.staged = staged;
        report
    }

    /// Write the route table.
    fn generate_router_types(&mut self) -> StepReport {
        if !self.resolved.config.app.router.enabled {
            return StepReport::skipped(
                PrepareStep::GenerateRouterTypes,
                "the router is disabled in uf.config.js",
            );
        }
        match write_router_manifest(&self.resolved.root, &self.resolved.config) {
            Ok(Some(path)) => {
                let relative = relative_to(&self.resolved.root, &path);
                self.generated.push(GeneratedFile {
                    path: relative.as_str().into(),
                    kind: GeneratedFileKind::RouterTypes,
                });
                StepReport::ok(
                    PrepareStep::GenerateRouterTypes,
                    format!("wrote {relative}"),
                )
            }
            // `write_router_manifest` answers `None` for a disabled router,
            // which the branch above already handled.
            Ok(None) => StepReport::skipped(
                PrepareStep::GenerateRouterTypes,
                "the router is disabled in uf.config.js",
            ),
            Err(error) => StepReport::failed(PrepareStep::GenerateRouterTypes, error.to_string()),
        }
    }

    /// Write the server action table.
    ///
    /// Over the whole project rather than the staged set: an action declared
    /// in a file nobody touched this commit is still an action, and a table
    /// that listed only the ones being committed would be wrong the moment it
    /// was written.
    fn generate_server_action_types(&mut self) -> StepReport {
        let step = PrepareStep::GenerateServerActionTypes;
        if !self.resolved.config.app.rsc || !self.resolved.config.app.server_actions {
            return StepReport::skipped(step, "server actions are disabled in uf.config.js");
        }

        let target = self.resolved.root.join(SERVER_ACTION_TYPES_FILE_NAME);
        let analysis = match analyze_project(
            &self.resolved.root,
            &BuildId::from_env_or_generate(),
            &self.scan_options(),
        ) {
            Ok(analysis) => analysis,
            Err(error) => return StepReport::failed(step, error.to_string()),
        };

        // A project with no `"use server"` anywhere gets no file about server
        // actions. One that had them and lost them keeps its file up to date
        // rather than keeping a table that is no longer true.
        if analysis.registry.is_empty() && !target.exists() {
            return StepReport::skipped(step, "this project declares no server actions");
        }

        let callable = server_action_types(&analysis.registry).len();
        match write_server_action_types(&self.resolved.root, &analysis.registry) {
            Ok(path) => {
                let relative = relative_to(&self.resolved.root, &path);
                self.generated.push(GeneratedFile {
                    path: relative.as_str().into(),
                    kind: GeneratedFileKind::ServerActionTypes,
                });
                StepReport::ok(
                    step,
                    format!("wrote {relative}, {}", plural(callable, "callable action")),
                )
            }
            Err(error) => StepReport::failed(step, error.to_string()),
        }
    }

    /// Lint the staged files.
    fn run_lint(&mut self) -> StepReport {
        let step = PrepareStep::RunLint;
        if let Err(error) = self.scan() {
            return StepReport::failed(step, error.to_string());
        }
        if !self.unreadable.is_empty() {
            // Before any count of diagnostics: a file nobody could read has no
            // diagnostics, and reporting "no problems" over it would be a lie.
            return StepReport::failed(
                step,
                format!(
                    "{} could not be read",
                    plural(self.unreadable.len(), "file")
                ),
            )
            .with_lines(self.unreadable.clone());
        }

        // Fixing first, so that what is linted is what is now on disk and the
        // count a reader is shown is what a second `uf prepare` would find.
        let mut fixed = Vec::new();
        if self.fix {
            let resolved = self.resolved;
            match fix_files(resolved, &mut self.sources, FixMode::Safe) {
                Ok(summary) => {
                    self.rewrote |= !summary.changed.is_empty();
                    fixed = summary.changed;
                }
                Err(error) => return StepReport::failed(step, error.to_string()),
            }
        }

        let sources: Vec<SourceFile> = self
            .sources
            .iter()
            .filter(|file| file.kind.is_flow() || file.kind == SourceKind::PackageManifest)
            .map(|file| SourceFile {
                path: file.relative_path.clone(),
                source: file.source.clone(),
            })
            .collect();
        if sources.is_empty() {
            return StepReport::skipped(step, self.nothing_to_check("lints"));
        }

        let report = match lint_sources(&sources, &self.resolved.config) {
            Ok(report) => report,
            Err(error) => return StepReport::failed(step, error.to_string()),
        };
        let errors = severity_count(&report, Severity::Error);
        let warnings = severity_count(&report, Severity::Warn);
        let mut detail = format!(
            "{} checked, {}",
            plural(report.files_checked, "file"),
            problem_summary(errors, warnings)
        );
        if !fixed.is_empty() {
            detail = format!("{detail}, fixed {}", plural(fixed.len(), "file"));
        }
        // Kept for the report to draw with their code frames, exactly as
        // `uf lint` draws them: a pre-commit hook that says "2 errors" and
        // makes the reader run a second command has not saved anybody a step.
        if !report.diagnostics.is_empty() {
            self.diagnostics = Some((report.clone(), sources));
        }
        if errors > 0 || !fixed.is_empty() {
            StepReport::failed(step, detail).with_lines(fixed)
        } else {
            StepReport::ok(step, detail)
        }
    }

    /// Check that the staged files are formatted, or format them.
    fn run_format_check(&mut self) -> StepReport {
        let step = PrepareStep::RunFormatCheck;
        if let Err(error) = self.scan() {
            return StepReport::failed(step, error.to_string());
        }

        let non_flow: Vec<String> = self
            .sources
            .iter()
            .filter(|file| file.kind.is_non_flow_formattable())
            .map(|file| file.relative_path.clone())
            .collect();
        let flow: Vec<&ProjectFile> = self
            .sources
            .iter()
            .filter(|file| file.kind.is_formattable())
            .collect();
        if flow.is_empty() && non_flow.is_empty() {
            return StepReport::skipped(step, self.nothing_to_check("formats"));
        }

        let scanned = flow.len();
        let fix = self.fix;
        let mut unformatted = Vec::new();
        let mut unprintable = Vec::new();
        for file in flow {
            match format_source(&file.source, &self.resolved.config.fmt) {
                Ok(result) if result.changed => {
                    // A file that could not be written joins the list of files
                    // that could not be printed: both mean "this one is still
                    // unformatted, and here is why", which is what the step
                    // has to say either way.
                    if fix && let Err(error) = fs::write(&file.absolute_path, &result.output) {
                        unprintable.push(format!("{}: {error}", file.relative_path));
                        continue;
                    }
                    unformatted.push(file.relative_path.clone());
                }
                Ok(_) => {}
                // The formatter prints from a syntax tree and there is no tree
                // to print when the source does not parse. `uf fmt` reports
                // that and leaves the file alone; so does this.
                Err(error) => unprintable.push(format!("{}: {error}", file.relative_path)),
            }
        }
        self.rewrote |= fix && !unformatted.is_empty();

        // The other formatter, over the other pile — and only when there is a
        // pile: a commit of Flow files must not need Biome installed.
        let mut lines = unformatted.clone();
        lines.extend(unprintable.iter().cloned());
        // A `--fix` run that formatted something still fails; see the module
        // header. What changed is in the working tree and not in the index.
        let mut failed = !unformatted.is_empty() || !unprintable.is_empty();
        let mut detail = if fix {
            format!(
                "formatted {} of {}",
                plural(unformatted.len(), "file"),
                scanned
            )
        } else {
            format!(
                "{} of {} {} formatting",
                plural(unformatted.len(), "file"),
                scanned,
                if unformatted.len() == 1 {
                    "needs"
                } else {
                    "need"
                }
            )
        };
        // Which non-Flow files the other formatter rewrote, if it wrote at all.
        // Applied below rather than here: the `Err` arm replaces `detail`, and
        // an arm that both replaced and appended to it would have to know
        // which of the two happened.
        let mut rewritten = Vec::new();
        match uf_fmt::non_flow::run(
            &self.resolved.root,
            &non_flow,
            // Its check mode, unless this run is fixing. In writing mode it
            // formats rather than reporting, so the `Unformatted` arm below is
            // unreachable under `--fix` rather than wrong there.
            !fix,
            &self.resolved.config.fmt,
        ) {
            Ok(NonFlowOutcome::Formatted { rewritten: written }) => rewritten = written,
            Ok(NonFlowOutcome::Unformatted) => {
                failed = true;
                lines.push(format!(
                    "{} reports that some of the staged non-Flow files need formatting",
                    self.resolved.config.fmt.non_flow.formatter.as_str()
                ));
            }
            // A commit is not the place to discover that uf's own default
            // formatter is missing from a project that never named it. The
            // step says what it did not look at and passes; a hook that
            // refuses a clean commit over that is a hook that gets
            // uninstalled. See ubugeeei-prod/uf#441.
            Ok(NonFlowOutcome::Skipped { formatter, paths }) => {
                lines.push(uf_fmt::non_flow::skipped_message(&formatter, paths.len()));
                lines.extend(paths);
            }
            Err(error) => {
                failed = true;
                detail = error.to_string();
            }
        }

        // The module header's rule, for the other half of the project. It has
        // to be asked rather than inferred: a formatter in write mode exits 0
        // whether it rewrote every file or none of them, so a `--fix` run that
        // reformatted a staged `.json` used to pass the hook while the bytes
        // git was about to commit were the ones from before the rewrite.
        if !rewritten.is_empty() {
            self.rewrote = true;
            failed = true;
            detail = format!(
                "{detail}, {} rewritten by {}",
                plural(rewritten.len(), "non-Flow file"),
                self.resolved.config.fmt.non_flow.formatter.as_str()
            );
            lines.extend(rewritten);
        }

        // The lines go under the step either way: a step that passed while
        // leaving files unlooked-at has something to say, and saying it only
        // on failure would hide exactly the case this reports.
        if failed {
            StepReport::failed(step, detail).with_lines(lines)
        } else {
            StepReport::ok(step, detail).with_lines(lines)
        }
    }

    /// Whether the checks should read this file.
    ///
    /// The staged set, plus whatever this run generated. `router.js` and
    /// `server-actions.js` are git-ignored in a scaffolded project, so they
    /// are never in the index and narrowing to the index alone would leave
    /// them unexamined — a commit hook that writes a file the linter rejects
    /// is a hook that gets uninstalled. `uf prepare` wrote them, so it checks
    /// them.
    fn checks(&self, relative_path: &str) -> bool {
        self.staged.selects(relative_path)
            || self.generated.iter().any(|file| file.path == relative_path)
    }

    /// The paths discovery is asked about by name: the staged set and this
    /// run's generated files.
    ///
    /// Empty when there is no staged set, because that is the "check
    /// everything" case and naming nothing is what widens the walk back to the
    /// whole project. It is a list of exact paths, never a prefix, so it can
    /// only ever *add* the files [`Self::checks`] was going to keep anyway —
    /// it cannot widen the set the checks run over.
    fn named_paths(&self) -> Vec<String> {
        let staged = match &self.staged {
            StagedFiles::Staged(files) => files.as_slice(),
            StagedFiles::Unavailable(_) => &[],
        };
        staged
            .iter()
            .map(compact_str::CompactString::to_string)
            .chain(self.generated.iter().map(|file| file.path.to_string()))
            .collect()
    }

    /// Why a check step had nothing to read.
    fn nothing_to_check(&self, verb: &str) -> String {
        if self.staged.is_empty_index() {
            String::from("nothing is staged for commit")
        } else {
            format!("no staged file is one uf {verb}")
        }
    }

    /// Read the project's files once, narrowed to the staged set.
    ///
    /// Discovery rather than reading the staged paths directly, so that the
    /// ignore rules, the file kinds and the unreadable-file handling are the
    /// same ones `uf lint` and `uf fmt` use rather than a second copy that
    /// drifts. It also reads the *working tree*, which is what makes the
    /// difference from `lint-staged` visible: a file that is half staged is
    /// checked as it is on disk.
    fn scan(&mut self) -> Result<(), uf_project::ProjectError> {
        if self.scanned {
            return Ok(());
        }
        // Nothing staged and nothing generated: there is no file this run could
        // select, and walking the project to prove it is the difference between
        // a commit hook that costs forty milliseconds on this repository and
        // one that costs three hundred and fifty. Discovery reads every source
        // in the project, so it is not something to do for an empty answer.
        self.scanned = true;
        if self.staged.is_empty_index() && self.generated.is_empty() {
            return Ok(());
        }
        // The staged and generated files by name, because both are files a
        // project can legitimately tell git to ignore while this run still has
        // to read them, and a discovery walk that honours `.gitignore` would
        // not offer either. Naming a path is asking about it; see
        // `uf_project::scan_selected_source_files`.
        //
        // * The generated ones — `router.js` is in this repository's own
        //   `.gitignore` — because without them `uf prepare` wrote two files
        //   and then silently checked neither.
        // * The staged ones because git stops applying `.gitignore` to a path
        //   the moment that path is in the index. A force-added file is as
        //   much a part of the commit as any other, and dropping it excluded
        //   from the checks the one file the commit is about.
        let named = self.named_paths();
        let mut scan =
            scan_selected_source_files(&self.resolved.root, &self.resolved.config, &named)?;
        scan.unreadable
            .retain(|failure| self.checks(&failure.relative_path));
        self.unreadable = unreadable_lines(&scan.unreadable);
        self.sources = scan
            .files
            .into_iter()
            .filter(|file| self.checks(&file.relative_path))
            .collect();
        Ok(())
    }

    /// How the RSC scan walks this project.
    ///
    /// The project's own `ignore` on top of the directories the scan always
    /// skips. Without it `uf prepare` walks whatever a project has chosen to
    /// keep out of its own tooling — in this repository that is `upstream/`, a
    /// vendored copy of Flow, and the walk went from tens of milliseconds to
    /// seconds. A pre-commit hook has to be quick or it gets uninstalled.
    fn scan_options(&self) -> ProjectScanOptions {
        let ignored_directories = IGNORED_DIRECTORIES
            .iter()
            .map(|name| compact_str::CompactString::from(*name))
            .chain(
                self.resolved
                    .config
                    .project_ignore()
                    .entries
                    .iter()
                    // An ignore entry with a separator names one place rather
                    // than a kind of directory, and the RSC scan filters by
                    // directory name only. Prefix entries are left to the
                    // scan, which reads them and finds nothing to act on.
                    .filter(|ignored| !ignored.contains('/'))
                    .cloned(),
            )
            .collect();
        ProjectScanOptions {
            ignored_directories,
            ..ProjectScanOptions::default()
        }
    }
}

/// The `staged` object of `prepare.json`.
fn staged_payload(staged: &StagedFiles) -> serde_json::Value {
    match staged {
        StagedFiles::Staged(files) => json!({ "source": "git", "files": files }),
        StagedFiles::Unavailable(reason) => {
            json!({ "source": "none", "reason": reason.reason() })
        }
    }
}

/// The staged paths, cut off before they out-shout the diagnostics.
fn staged_lines(files: &[compact_str::CompactString]) -> Vec<String> {
    let mut lines: Vec<String> = files
        .iter()
        .take(STAGED_PATHS_SHOWN)
        .map(|file| file.to_string())
        .collect();
    if files.len() > STAGED_PATHS_SHOWN {
        lines.push(format!("and {} more", files.len() - STAGED_PATHS_SHOWN));
    }
    lines
}

/// The mark a step is drawn with.
const fn step_status(status: StepStatus) -> Status {
    match status {
        StepStatus::Ok => Status::Success,
        StepStatus::Failed => Status::Error,
        StepStatus::Skipped | StepStatus::NotRun => Status::Skip,
    }
}

fn render(
    ui: &mut Ui,
    resolved: &ResolvedConfig,
    plan: &uf_prepare::PreparePlan,
    manifest: &Utf8PathBuf,
    staged: &StagedFiles,
    reports: &[StepReport],
    diagnostics: Option<&(LintReport, Vec<SourceFile>)>,
) {
    let root = resolved.root.as_str().to_string();
    let manifest_path = manifest.to_string();
    let cache = plan.cache.name().to_string();
    let staged_summary = match staged {
        StagedFiles::Staged(files) => plural(files.len(), "file"),
        StagedFiles::Unavailable(reason) => format!("every file ({})", reason.reason()),
    };
    let steps: Vec<(Status, String, Vec<&str>)> = reports
        .iter()
        .map(|report| {
            (
                step_status(report.outcome.status),
                format!(
                    "{:<STEP_NAME_WIDTH$}{}",
                    report.outcome.step.name(),
                    report.outcome.detail
                ),
                report.lines.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let failures: Vec<&str> = reports
        .iter()
        .filter(|report| report.outcome.status.is_failure())
        .map(|report| report.outcome.step.name())
        .collect();
    let summary = match failures.as_slice() {
        [] => String::from("prepare passed"),
        [only] => format!("prepare failed at {only}"),
        many => format!("prepare failed at {}", many.join(" and ")),
    };

    ui.render(|renderer, out| {
        renderer.banner(out, "uf prepare", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("root", &root, Tone::Path),
                KeyValue::toned("manifest", &manifest_path, Tone::Path),
                KeyValue::new("staged", &staged_summary),
                KeyValue::new(
                    "lint-staged compatible",
                    yes_no(plan.lint_staged_compatible),
                ),
                KeyValue::new("code generator", enabled(plan.code_generator)),
                KeyValue::new("cache", &cache),
            ],
        );
        renderer.blank(out);
        renderer.heading(out, 2, "steps");
        renderer.blank(out);
        for (status, line, lines) in &steps {
            push_spaces(out, 2);
            renderer.status(out, *status, line);
            renderer.bullet_list(out, 6, lines);
        }
        renderer.blank(out);
    });

    if let Some((report, sources)) = diagnostics {
        for group in group_by_path(&report.diagnostics) {
            render_group(ui, group, sources);
        }
    }

    ui.render(|renderer, out| {
        renderer.status(
            out,
            if failures.is_empty() {
                Status::Success
            } else {
                Status::Error
            },
            &summary,
        );
    });
}
