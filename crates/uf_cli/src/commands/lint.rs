//! `uf lint` and `uf check`: grouped diagnostics with code frames, and the
//! `--fix` that writes the ones uf can answer.
//!
//! # What `--fix` does to the report
//!
//! It runs first, over the files on disk, and then the command lints the
//! project it just rewrote. Nothing a reader is shown is carried over from
//! before the fixes: the diagnostics, the counts and the exit status all come
//! from the same fresh pass, so `uf lint --fix` followed by `uf lint` cannot
//! disagree with itself. The cost is a second walk of the project, paid only
//! when fixes were asked for.
//!
//! # What the exit status means
//!
//! **The status describes what is left, never what was done.** A run that
//! fixed forty findings and left one error fails; a run that fixed nothing
//! and left none passes.
//!
//! - `0` — no errors remain. Warnings may: they do not fail `uf lint` today
//!   and `--fix` is not the place to change that, so a project with sixteen
//!   warnings and no errors exits `0` whether or not anything was fixed.
//! - `1` — errors remain, a file could not be read, or writing a fix failed.
//!
//! Without `--fix` this is exactly what `uf lint` already meant, which is the
//! point: `--fix` adds writing to the command, not a second dialect of
//! success. There is no third status for "fixed everything" — the two the
//! command has are the two a shell script branches on, and a `--check`-like
//! run is simply the default one, which writes nothing.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::load_config;
use uf_infra::FxHashSet;
use uf_lint::{Diagnostic, LintCache, LintReport, Severity, SourceFile, lint_sources_cached};
use uf_project::{SourceKind, scan_selected_source_files_matching};
use uf_term::{CodeFrame, DiagnosticLevel, Status, format_duration};

#[cfg(feature = "upstream-typecheck")]
use crate::commands::check::libdefs;
use crate::fix::files::{FixMode, FixSummary, fix_project};
use crate::fix::{RuleFix, rule_fix};
use crate::support::{
    ignore_deprecation, plural, problem_summary, project_label, quoted_list,
    render_ignore_deprecation, selects, unreadable_lines,
};
use crate::ui::Ui;

pub(crate) mod plugins;

/// Which of the two lint entry points is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LintCommand {
    /// `uf lint`.
    Lint,
    /// `uf check`.
    Check,
}

impl LintCommand {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Lint => "uf lint",
            Self::Check => "uf check",
        }
    }

    /// The command a reader runs to apply a tier of fixes: `uf lint --fix`.
    fn fix_command(self, fix: RuleFix) -> &'static str {
        match (self, fix) {
            (_, RuleFix::Formatter) => "uf fmt",
            (Self::Lint, RuleFix::Safe) => "uf lint --fix",
            (Self::Lint, RuleFix::Unsafe) => "uf lint --fix-unsafe",
            (Self::Check, RuleFix::Safe) => "uf check --fix",
            (Self::Check, RuleFix::Unsafe) => "uf check --fix-unsafe",
        }
    }
}

pub(crate) fn lint_command(
    cwd: &Utf8Path,
    ui: &mut Ui,
    command: LintCommand,
    json: bool,
    fix: FixMode,
    paths: &[String],
) -> Result<()> {
    let started = Instant::now();
    let mut progress = ui.progress();
    let fixed = if fix.writes() {
        progress.draw("applying fixes");
        Some(fix_project(cwd, paths, fix)?)
    } else {
        None
    };
    progress.draw("scanning sources");
    let LintRun {
        report,
        sources,
        unreadable,
        root,
        ignore_deprecation,
        project_rules,
        ..
    } = run_lint(cwd, paths)?;
    progress.finish();
    drop(progress);

    if json {
        let mut payload = lint_payload(command, &report, fixed.as_ref());
        // Only when the pass ran, so the report of a project with no project
        // rule is the report it was before they existed.
        if project_rules.ran {
            payload["projectRules"] = plugins::payload(&project_rules);
        }
        ui.json(&payload)?;
    } else {
        render_lint_report(
            ui,
            command,
            project_label(&root),
            &report,
            &sources,
            fixed.as_ref(),
            started,
        );
        plugins::render(ui, &project_rules);
        render_unreadable(ui, &unreadable);
        render_ignore_deprecation(ui, ignore_deprecation);
    }

    // Before the diagnostics count: a file nobody could read has no
    // diagnostics, and reporting "0 errors" over it would be a lie.
    if !unreadable.is_empty() {
        bail!(uf_infra::cstr!(
            "{} could not be read",
            plural(unreadable.len(), "file")
        ));
    }
    // Before it for the same reason: an enabled rule that could not answer has
    // no findings, and "0 errors" would be the run speaking for it.
    if !project_rules.problems.is_empty() {
        bail!(uf_infra::cstr!(
            "{} kept project rules from answering",
            plural(project_rules.problems.len(), "problem")
        ));
    }
    let errors = severity_count(&report, Severity::Error);
    if errors > 0 {
        bail!(uf_infra::cstr!(
            "{} failed with {}",
            command.title(),
            plural(errors, "error")
        ));
    }
    Ok(())
}

/// The lint report, the sources it read, and the files it could not read.
///
/// `paths` narrows the run to the files whose relative path contains one of
/// the patterns, which is what `uf test` already means by a path argument.
/// Empty means the whole project.
/// What one pass of the linter over a project produced.
pub(crate) struct LintRun {
    /// The linter's own findings.
    pub(crate) report: LintReport,
    /// Every source the scan collected, in the order it collected them.
    pub(crate) sources: Vec<SourceFile>,
    /// Files that could not be read, already rendered as lines.
    pub(crate) unreadable: Vec<String>,
    /// The project root the scan ran from.
    ///
    /// Returned rather than resolved again by the caller: `uf check` keeps its
    /// type-check cache under it, and reading the config twice to learn the
    /// same answer is how the two come to disagree.
    pub(crate) root: Utf8PathBuf,
    /// Every Flow source and manifest the scan collected, before `paths`
    /// narrowed it — empty when nothing was narrowed away.
    ///
    /// A narrowed lint run still needs the manifest files in this set for
    /// project-wide context such as `import/no-extraneous-dependencies`, but
    /// diagnostics are still emitted only for [`Self::sources`]. The *checker*
    /// uses the rest too, because an import is only typed against a file in the
    /// same batch, and the file a narrowed run imports is exactly the file
    /// narrowing removed. `uf check` walks this set to find what its selection
    /// reaches; see `uf_check::module_closure`.
    ///
    /// Empty rather than a copy when `paths` selected everything, because then
    /// [`Self::sources`] already is the whole scan and holding a second copy of
    /// a project's text is a real cost for no answer.
    pub(crate) available: Vec<SourceFile>,
    /// What to say about `lint.ignore`, when this project still writes it.
    ///
    /// Carried out of the run rather than re-read by the caller for the same
    /// reason [`Self::root`] is: two reads of one config file are two chances
    /// to disagree about what it said.
    pub(crate) ignore_deprecation: Option<&'static str>,
    /// What the project's own rules cost and what kept any of them from
    /// answering. Their findings are already in [`Self::report`], sorted among
    /// uf's; see [`plugins`].
    pub(crate) project_rules: plugins::ProjectRules,
}

pub(crate) fn run_lint(cwd: &Utf8Path, paths: &[String]) -> Result<LintRun> {
    collect_and_lint(cwd, paths, true)
}

/// [`run_lint`], or only its collection when `lint` is false.
///
/// `uf check --no-lint` wants the batch the linter would have read and not one
/// verdict of the linter's: a negative type test asks the checker whether a
/// fixture is refused, and on a warm cache the lint was nine tenths of what
/// that question cost. Skipping it leaves an empty report over the same files,
/// so everything downstream — the batch, the counts, the payload — is shaped
/// exactly as before.
pub(crate) fn collect_and_lint(cwd: &Utf8Path, paths: &[String], lint: bool) -> Result<LintRun> {
    let resolved = load_config(cwd)?;
    // Flow only. Discovery also returns the JSON, CSS and TypeScript that
    // `uf fmt` hands to the non-Flow formatter, and uf's linter is a Flow
    // linter — parsing a stylesheet with it produces a syntax error about a
    // file nobody asked it to read. `package.json` is the exception it already
    // made: the linter reads it, which is why `is_flow` is the wrong question
    // for the formatter and the right one here.
    let mut scan =
        scan_selected_source_files_matching(&resolved.root, &resolved.config, paths, lint_kind)?;
    // Narrowed before the read failures are rendered as well as before the
    // sources: a file outside what was asked about must not fail the run.
    scan.unreadable
        .retain(|failure| selects(paths, &failure.relative_path));
    let unreadable = unreadable_lines(&scan.unreadable);
    // A library definition is not a source file the project owns. It declares
    // the environment the sources are checked in — `declare module` and a
    // top-level `declare type` are library syntax — so `uf check` merges it and
    // takes it out of the batch, and the lint must leave it alone for the same
    // reason. The scan collects `flow-typed/` like any other directory, so
    // until this filter existed a hand-written libdef was linted as a source:
    // `declare type Provider = any` came back as `flow/unclear-type` and failed
    // the run, over an `any` that is what a libdef for an untyped dependency is
    // made of. ubugeeei-prod/uf#699.
    //
    // Empty when the checker is not compiled in, since `lib_paths` is Flow's
    // own `.flowconfig` parser and comes with it; a build without it merges no
    // libdefs either, so nothing is being both merged and linted.
    #[cfg(feature = "upstream-typecheck")]
    let declared: FxHashSet<String> = match uf_check::lib_paths(resolved.root.as_std_path()) {
        Ok(paths) => libdefs::declared_paths(&resolved.root, &paths),
        Err(_) => FxHashSet::default(),
    };
    #[cfg(not(feature = "upstream-typecheck"))]
    let declared: FxHashSet<String> = FxHashSet::default();
    let collected = scan
        .files
        .into_iter()
        .filter(|file| file.kind.is_flow() || file.kind == SourceKind::PackageManifest)
        .filter(|file| !declared.contains(file.relative_path.as_str()))
        .map(|file| SourceFile {
            path: file.relative_path,
            source: file.source,
        })
        .collect::<Vec<_>>();
    // Narrowing is what makes the two sets differ, so it is also the only case
    // that pays for keeping both.
    let (sources, available) = if paths.is_empty() {
        (collected, Vec::new())
    } else {
        let selected = collected
            .iter()
            .filter(|file| selects(paths, &file.path))
            .cloned()
            .collect::<Vec<_>>();
        (selected, collected)
    };
    if sources.is_empty() && !paths.is_empty() && unreadable.is_empty() {
        // "no file matched" is true and unhelpful when the file is right
        // there and uf declined to lint it, so say which it was instead.
        if let Some(libdef) = declared.iter().find(|path| selects(paths, path)) {
            bail!(uf_infra::cstr!(
                "{libdef} is a library definition, not a source file uf lints"
            ));
        }
        bail!(uf_infra::cstr!("no file matched {}", quoted_list(paths)));
    }
    if !lint {
        return Ok(LintRun {
            report: LintReport {
                diagnostics: Vec::new(),
                files_checked: sources.len(),
                unavailable: Vec::new(),
            },
            sources,
            unreadable,
            root: resolved.root,
            available,
            ignore_deprecation: ignore_deprecation(&resolved.config),
            project_rules: plugins::ProjectRules::default(),
        });
    }
    // Under the project root, as `.uf/cache/check` is: what the React tree
    // rules worked out about a module is kept there between runs, so a module
    // nobody touched is not handed to the React Compiler again. Swept once,
    // before this run adds to it, by the policy every `.uf/cache/` directory
    // shares. ubugeeei-prod/uf#1442.
    let cache = LintCache::open(resolved.root.as_std_path());
    if let Some(cache) = &cache {
        cache.sweep();
    }
    let context = if available.is_empty() {
        &sources
    } else {
        &available
    };
    let mut report = lint_sources_cached(&sources, context, &resolved.config, cache.as_ref())?;
    // Over the same narrowed sources, so a path argument means the same thing
    // to a project rule as to uf's own. Nothing starts when none is enabled.
    let mut project_rules = plugins::run(&resolved.root, &resolved.config, &sources)?;
    if !project_rules.diagnostics.is_empty() {
        report.diagnostics.append(&mut project_rules.diagnostics);
        plugins::sort(&mut report.diagnostics);
    }
    Ok(LintRun {
        report,
        sources,
        unreadable,
        root: resolved.root,
        available,
        ignore_deprecation: ignore_deprecation(&resolved.config),
        project_rules,
    })
}

pub(crate) fn severity_count(report: &LintReport, severity: Severity) -> usize {
    report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == severity)
        .count()
}

/// Every file `uf lint` with no paths would read, for a caller that already
/// holds the project's root and configuration: the language server's type
/// session, which checks the project the way `uf check` does.
///
/// A file that cannot be read is left out rather than failing the scan; the
/// session answers about the files it has.
#[cfg(feature = "upstream-typecheck")]
pub(crate) fn project_sources(
    root: &Utf8Path,
    config: &uf_config::UniflowedConfig,
) -> Result<Vec<SourceFile>> {
    let scan = scan_selected_source_files_matching(root, config, &[], lint_kind)?;
    Ok(scan
        .files
        .into_iter()
        .filter(|file| lint_kind(file.kind))
        .map(|file| SourceFile {
            path: file.relative_path,
            source: file.source,
        })
        .collect())
}

fn lint_kind(kind: SourceKind) -> bool {
    kind.is_flow() || kind == SourceKind::PackageManifest
}

/// The machine-readable report.
///
/// `fixed` is present only when `--fix` ran, and it describes the pass that
/// preceded the diagnostics rather than the diagnostics themselves: every
/// count under `diagnostics` is from after the fixes were written.
pub(crate) fn lint_payload(
    command: LintCommand,
    report: &LintReport,
    fixed: Option<&FixSummary>,
) -> serde_json::Value {
    let mut payload = json!({
        "command": command.title(),
        "filesChecked": report.files_checked,
        "errors": severity_count(report, Severity::Error),
        "warnings": severity_count(report, Severity::Warn),
        "diagnostics": report.diagnostics.iter().map(|diagnostic| json!({
            "rule": diagnostic.rule,
            "severity": match diagnostic.severity {
                Severity::Error => "error",
                Severity::Warn => "warning",
            },
            "path": diagnostic.path,
            "line": diagnostic.line,
            "column": diagnostic.column,
            "message": diagnostic.message,
        })).collect::<Vec<_>>(),
        "unavailableRules": report.unavailable.iter().map(|unavailable| json!({
            "rule": unavailable.rule,
            "reason": unavailable.reason(),
        })).collect::<Vec<_>>(),
    });
    if let Some(fixed) = fixed
        && let Some(object) = payload.as_object_mut()
    {
        object.insert(
            "fixed".to_owned(),
            json!({
                "applied": fixed.applied,
                "files": fixed.changed,
                "refused": fixed.refused,
                "needsUnsafeFix": fixed.needs_unsafe,
                "needsFormatter": fixed.needs_fmt,
            }),
        );
    }
    payload
}

/// The path a diagnostic is reported under.
pub(crate) fn diagnostic_path(diagnostic: &Diagnostic) -> &str {
    diagnostic.path.as_deref().unwrap_or("<memory>")
}

/// Group diagnostics into runs that share a path.
///
/// [`LintReport::diagnostics`] is sorted by path, so one pass is enough.
pub(crate) fn group_by_path(diagnostics: &[Diagnostic]) -> Vec<&[Diagnostic]> {
    let mut groups = Vec::new();
    let mut start = 0;
    for index in 1..=diagnostics.len() {
        let boundary = index == diagnostics.len()
            || diagnostic_path(&diagnostics[index]) != diagnostic_path(&diagnostics[start]);
        if boundary && index > start {
            groups.push(&diagnostics[start..index]);
            start = index;
        }
    }
    groups
}

/// The length in bytes of the identifier starting at a byte column, so the
/// caret covers the offending token instead of a single character.
pub(crate) fn identifier_span(line: &str, column: usize) -> usize {
    let start = column.saturating_sub(1);
    if start >= line.len() {
        return 1;
    }
    let bytes = &line.as_bytes()[start..];
    let length = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
        .count();
    length.max(1)
}

/// The files discovery skipped, after the diagnostics.
///
/// Shared by `uf lint` and `uf check`: both walk the project, and both used
/// to stop at the first file that was not UTF-8 without linting any of the
/// rest.
pub(crate) fn render_unreadable(ui: &mut Ui, unreadable: &[String]) {
    if unreadable.is_empty() {
        return;
    }
    let lines = unreadable.iter().map(String::as_str).collect::<Vec<_>>();
    ui.render(|renderer, out| {
        renderer.status(
            out,
            Status::Warn,
            uf_infra::cstr!("{} could not be read", plural(lines.len(), "file")).as_str(),
        );
        renderer.bullet_list(out, 2, &lines);
        renderer.blank(out);
    });
}

fn render_lint_report(
    ui: &mut Ui,
    command: LintCommand,
    project: &str,
    report: &LintReport,
    sources: &[SourceFile],
    fixed: Option<&FixSummary>,
    started: Instant,
) {
    let errors = severity_count(report, Severity::Error);
    let warnings = severity_count(report, Severity::Warn);

    ui.render(|renderer, out| {
        renderer.banner(out, command.title(), Some(project));
    });

    // Each file's findings under its own header, and nothing after them that
    // repeats which files those were: the headers already say it, and a table
    // of the same paths at the bottom was one more thing to read past on the
    // way to the verdict.
    for group in group_by_path(&report.diagnostics) {
        render_group(ui, group, sources);
    }
    // Above the verdict, because the verdict counts what is left and this
    // counts what went: a reader who sees "16 warnings" wants the line that
    // says forty other findings are already gone next to it, not after it.
    if let Some(fixed) = fixed {
        render_fix_summary(ui, fixed);
    }
    render_verdict(
        ui,
        command,
        report,
        Verdict {
            errors,
            warnings,
            elapsed: started.elapsed(),
            fixed: fixed.is_some(),
        },
    );
}

/// `1 fix` / `3 fixes`.
///
/// Its own function because [`plural`] appends an `s`, and "fixs" is a typo in
/// the tool as far as anybody reading it is concerned.
fn fix_count(count: usize) -> String {
    if count == 1 {
        String::from("1 fix")
    } else {
        uf_infra::into_string(uf_infra::cstr!("{count} fixes"))
    }
}

/// What `--fix` wrote, and what it deliberately did not.
///
/// Printed even when nothing was written: "nothing here had a fix" is the
/// answer to the question `--fix` asks, and a command that says nothing after
/// being asked to change files reads as one that failed silently.
pub(crate) fn render_fix_summary(ui: &mut Ui, fixed: &FixSummary) {
    let headline = if fixed.applied == 0 {
        String::from("no finding here had a fix to apply")
    } else {
        uf_infra::cstr!(
            "applied {} in {}",
            fix_count(fixed.applied),
            plural(fixed.changed.len(), "file")
        )
        .into_string()
    };
    let changed: Vec<&str> = fixed.changed.iter().map(String::as_str).collect();
    let refused: Vec<&str> = fixed.refused.iter().map(String::as_str).collect();
    let mut notes = Vec::new();
    if fixed.needs_unsafe > 0 {
        notes.push(
            uf_infra::cstr!(
                "{} would be fixed by `--fix-unsafe`, which can change what the program does",
                plural(fixed.needs_unsafe, "finding")
            )
            .into_string(),
        );
    }
    if fixed.needs_fmt > 0 {
        notes.push(
            uf_infra::cstr!(
                "{} would be cleared by `uf fmt`",
                plural(fixed.needs_fmt, "finding")
            )
            .into_string(),
        );
    }

    ui.render(|renderer, out| {
        renderer.status(
            out,
            if fixed.applied == 0 {
                Status::Info
            } else {
                Status::Success
            },
            &headline,
        );
        renderer.bullet_list(out, 2, &changed);
        if !refused.is_empty() {
            renderer.blank(out);
            renderer.status(
                out,
                Status::Warn,
                uf_infra::cstr!("{} left unfixed", plural(refused.len(), "file")).as_str(),
            );
            renderer.bullet_list(out, 2, &refused);
        }
        for note in &notes {
            renderer.status(out, Status::Info, note);
        }
        renderer.blank(out);
    });
}

/// One file's diagnostics: a header naming the file, then the code frames.
pub(crate) fn render_group(ui: &mut Ui, group: &[Diagnostic], sources: &[SourceFile]) {
    let path = diagnostic_path(&group[0]);
    let lines: Option<Vec<&str>> = sources
        .iter()
        .find(|source| source.path == path)
        .map(|source| source.source.lines().collect());
    let group_errors = group
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let header = problem_summary(group_errors, group.len() - group_errors);

    // The header prints the path on its own; the frames below go through
    // `CodeFrame`, which does this for itself. #640.
    let drawn = uf_term::safe_path(path);
    ui.render(|renderer, out| {
        renderer.theme().path.paint(renderer.color(), &drawn, out);
        out.push_str("  ");
        renderer.theme().muted.paint(renderer.color(), &header, out);
        out.push('\n');
        renderer.blank(out);

        for diagnostic in group {
            let source_line = lines
                .as_ref()
                .and_then(|lines| lines.get(diagnostic.line.saturating_sub(1)).copied());
            let level = match diagnostic.severity {
                Severity::Error => DiagnosticLevel::Error,
                Severity::Warn => DiagnosticLevel::Warning,
            };
            let mut frame = CodeFrame::new(
                level,
                &diagnostic.message,
                path,
                diagnostic.line,
                diagnostic.column,
            )
            .with_rule(diagnostic.rule);
            if let Some(line) = source_line {
                frame = frame
                    .with_source_line(line)
                    .with_span(identifier_span(line, diagnostic.column));
            }
            renderer.code_frame_at(out, &frame, 2);
            renderer.blank(out);
        }
    });
}

/// What the line a lint run ends on counts.
pub(crate) struct Verdict {
    /// Errors left, from every pass the command ran.
    pub(crate) errors: usize,
    /// Warnings left, from every pass the command ran.
    pub(crate) warnings: usize,
    /// How long the command took, from start to verdict.
    pub(crate) elapsed: Duration,
    /// Whether `--fix` ran, in which case its own summary has already said
    /// what the fixes could not reach and the hint is not repeated.
    pub(crate) fixed: bool,
}

/// The line a lint run ends on, and what to do next.
///
/// `✗ 3 errors, 1 warning · 10 files checked · 41ms`, then a hint for each
/// thing a reader can act on: the findings a command can fix, and the rules
/// that were enabled and skipped. The skipped rules are counted, not listed:
/// the list was the same five names on every run of every project, and
/// `uf lint --rules` marks each of them, while `--json` names them.
pub(crate) fn render_verdict(
    ui: &mut Ui,
    command: LintCommand,
    report: &LintReport,
    verdict: Verdict,
) {
    let Verdict {
        errors,
        warnings,
        elapsed,
        fixed,
    } = verdict;
    let headline = problem_summary(errors, warnings);
    let files = uf_infra::into_string(uf_infra::cstr!(
        "{} checked",
        plural(report.files_checked, "file")
    ));
    let took = format_duration(elapsed);
    let fixable = if fixed {
        None
    } else {
        fixable_hint(command, &report.diagnostics)
    };
    let skipped = report.unavailable.len();
    let skipped = (skipped > 0).then(|| {
        uf_infra::cstr!(
            "{} skipped: they need Flow type inference, which uf does not have yet; \
             `uf lint --rules` marks them",
            plural(skipped, "enabled rule")
        )
        .into_string()
    });

    ui.render(|renderer, out| {
        let status = if errors > 0 {
            Status::Error
        } else if warnings > 0 {
            Status::Warn
        } else {
            Status::Success
        };
        renderer.summary(out, status, &headline, &[&files, &took]);
        if let Some(fixable) = &fixable {
            renderer.hint(out, 2, fixable);
        }
        if let Some(skipped) = &skipped {
            renderer.hint(out, 2, skipped);
        }
    });
}

/// How many of `diagnostics` a command can fix, by the command that fixes
/// them: `2 fixable with `uf lint --fix`; 1 with `uf fmt``.
///
/// Said about the rule rather than tried against the line, so it can count a
/// finding a fixer would decline once it re-reads the line; `--fix` reports
/// what it actually wrote.
fn fixable_hint(command: LintCommand, diagnostics: &[Diagnostic]) -> Option<String> {
    let mut counts = [
        (RuleFix::Safe, 0usize),
        (RuleFix::Unsafe, 0),
        (RuleFix::Formatter, 0),
    ];
    for diagnostic in diagnostics {
        if let Some(fix) = rule_fix(diagnostic.rule)
            && let Some((_, count)) = counts.iter_mut().find(|(tier, _)| *tier == fix)
        {
            *count += 1;
        }
    }
    let parts: Vec<String> = counts
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(tier, count)| {
            uf_infra::into_string(uf_infra::cstr!(
                "{count} with `{}`",
                command.fix_command(*tier)
            ))
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(uf_infra::into_string(uf_infra::cstr!(
        "fixable: {}",
        parts.join("; ")
    )))
}

mod rules;

pub(crate) use rules::rules_command;

#[cfg(test)]
mod tests;
