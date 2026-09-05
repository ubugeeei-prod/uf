//! `uf lint` and `uf check`: grouped diagnostics with code frames.

use anyhow::{Result, bail};
use camino::Utf8Path;
use serde_json::json;
use uf_config::load_config;
use uf_lint::{Diagnostic, LintReport, Severity, SourceFile, lint_sources};
use uf_project::{SourceKind, scan_source_files};
use uf_term::{
    Cell, CodeFrame, Column, DiagnosticLevel, KeyValue, Status, Table, Tone, push_spaces,
};

use crate::support::{plural, problem_summary, quoted_list, selects, unreadable_lines};
use crate::ui::Ui;

/// How many skipped rules are named before the list is summarised.
///
/// The full list is always in `--json`; on screen it must not out-shout the
/// diagnostics a reader actually has to act on.
const SKIPPED_RULES_SHOWN: usize = 5;

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
}

pub(crate) fn lint_command(
    cwd: &Utf8Path,
    ui: &mut Ui,
    command: LintCommand,
    json: bool,
    paths: &[String],
) -> Result<()> {
    let mut progress = ui.progress();
    progress.draw("scanning sources");
    let (report, sources, unreadable) = run_lint(cwd, paths)?;
    progress.finish();
    drop(progress);

    if json {
        ui.json(&lint_payload(command, &report))?;
    } else {
        render_lint_report(ui, command, &report, &sources);
        render_unreadable(ui, &unreadable);
    }

    // Before the diagnostics count: a file nobody could read has no
    // diagnostics, and reporting "0 errors" over it would be a lie.
    if !unreadable.is_empty() {
        bail!("{} could not be read", plural(unreadable.len(), "file"));
    }
    let errors = severity_count(&report, Severity::Error);
    if errors > 0 {
        bail!(
            "{} failed with {}",
            command.title(),
            plural(errors, "error")
        );
    }
    Ok(())
}

/// The lint report, the sources it read, and the files it could not read.
///
/// `paths` narrows the run to the files whose relative path contains one of
/// the patterns, which is what `uf test` already means by a path argument.
/// Empty means the whole project.
pub(crate) fn run_lint(
    cwd: &Utf8Path,
    paths: &[String],
) -> Result<(LintReport, Vec<SourceFile>, Vec<String>)> {
    let resolved = load_config(cwd)?;
    // Flow only. Discovery also returns the JSON, CSS and TypeScript that
    // `uf fmt` hands to the non-Flow formatter, and uf's linter is a Flow
    // linter — parsing a stylesheet with it produces a syntax error about a
    // file nobody asked it to read. `package.json` is the exception it already
    // made: the linter reads it, which is why `is_flow` is the wrong question
    // for the formatter and the right one here.
    let mut scan = scan_source_files(&resolved.root, &resolved.config)?;
    // Narrowed before the read failures are rendered as well as before the
    // sources: a file outside what was asked about must not fail the run.
    scan.unreadable
        .retain(|failure| selects(paths, &failure.relative_path));
    let unreadable = unreadable_lines(&scan.unreadable);
    let sources = scan
        .files
        .into_iter()
        .filter(|file| file.kind.is_flow() || file.kind == SourceKind::PackageManifest)
        .filter(|file| selects(paths, &file.relative_path))
        .map(|file| SourceFile {
            path: file.relative_path,
            source: file.source,
        })
        .collect::<Vec<_>>();
    if sources.is_empty() && !paths.is_empty() && unreadable.is_empty() {
        bail!("no file matched {}", quoted_list(paths));
    }
    let report = lint_sources(&sources, &resolved.config)?;
    Ok((report, sources, unreadable))
}

pub(crate) fn severity_count(report: &LintReport, severity: Severity) -> usize {
    report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == severity)
        .count()
}

pub(crate) fn lint_payload(command: LintCommand, report: &LintReport) -> serde_json::Value {
    json!({
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
    })
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
            &format!("{} could not be read", plural(lines.len(), "file")),
        );
        renderer.bullet_list(out, 2, &lines);
        renderer.blank(out);
    });
}

fn render_lint_report(
    ui: &mut Ui,
    command: LintCommand,
    report: &LintReport,
    sources: &[SourceFile],
) {
    let errors = severity_count(report, Severity::Error);
    let warnings = severity_count(report, Severity::Warn);
    let groups = group_by_path(&report.diagnostics);

    ui.render(|renderer, out| {
        renderer.banner(out, command.title(), None);
        renderer.blank(out);
    });

    for group in &groups {
        render_group(ui, group, sources);
    }
    if groups.len() > 1 {
        render_file_summary(ui, &groups);
    }
    render_verdict(ui, report, errors, warnings);
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

    ui.render(|renderer, out| {
        renderer.theme().path.paint(renderer.color(), path, out);
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

/// A per-file count table, printed once diagnostics span more than one file.
pub(crate) fn render_file_summary(ui: &mut Ui, groups: &[&[Diagnostic]]) {
    let rows: Vec<(String, String, String)> = groups
        .iter()
        .map(|group| {
            let group_errors = group
                .iter()
                .filter(|diagnostic| diagnostic.severity == Severity::Error)
                .count();
            (
                diagnostic_path(&group[0]).to_string(),
                group_errors.to_string(),
                (group.len() - group_errors).to_string(),
            )
        })
        .collect();

    ui.render(|renderer, out| {
        let mut table = Table::new(vec![
            Column::left("file"),
            Column::right("errors"),
            Column::right("warnings"),
        ]);
        for (path, errors, warnings) in &rows {
            table.push(vec![
                Cell::toned(path, Tone::Path),
                Cell::toned(errors, Tone::Bad),
                Cell::toned(warnings, Tone::Warn),
            ]);
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
    });
}

pub(crate) fn render_verdict(ui: &mut Ui, report: &LintReport, errors: usize, warnings: usize) {
    let files_checked = report.files_checked.to_string();
    let error_count = errors.to_string();
    let warning_count = warnings.to_string();
    let skipped = report.unavailable.len().to_string();
    let mut unavailable: Vec<&str> = report
        .unavailable
        .iter()
        .take(SKIPPED_RULES_SHOWN)
        .map(|unavailable| unavailable.rule)
        .collect();
    let overflow = report.unavailable.len().saturating_sub(SKIPPED_RULES_SHOWN);
    let and_more = format!("and {overflow} more");
    if overflow > 0 {
        unavailable.push(&and_more);
    }
    let verdict = problem_summary(errors, warnings);

    ui.render(|renderer, out| {
        let mut rows = vec![
            KeyValue::toned("files checked", &files_checked, Tone::Number),
            KeyValue::toned("errors", &error_count, Tone::Bad),
            KeyValue::toned("warnings", &warning_count, Tone::Warn),
        ];
        if !unavailable.is_empty() {
            rows.push(KeyValue::toned("rules skipped", &skipped, Tone::Muted));
        }
        renderer.key_values(out, 2, &rows);

        if !unavailable.is_empty() {
            renderer.blank(out);
            push_spaces(out, 2);
            renderer.status(
                out,
                Status::Info,
                "these rules need Flow type inference, which uf does not implement yet",
            );
            renderer.bullet_list(out, 4, &unavailable);
        }

        renderer.blank(out);
        let status = if errors > 0 {
            Status::Error
        } else if warnings > 0 {
            Status::Warn
        } else {
            Status::Success
        };
        renderer.status(out, status, &verdict);
    });
}

#[cfg(test)]
mod tests;
