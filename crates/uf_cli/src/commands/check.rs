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
use uf_check::{
    CheckError, CheckLimits, CheckReport, Source, TypeDiagnostic, active_backend, backend_name,
    check_sources,
};
use uf_lint::{LintReport, Severity, SourceFile};
use uf_term::{CodeFrame, DiagnosticLevel, KeyValue, Status, Tone, push_spaces};

use crate::commands::lint::{
    LintCommand, group_by_path, lint_payload, render_file_summary, render_group, render_verdict,
    run_lint, severity_count,
};
use crate::support::{plural, problem_summary};
use crate::ui::Ui;

/// How many untyped imports are named before the list is summarised.
const UNTYPED_MODULES_SHOWN: usize = 5;

/// What the type-checking half of `uf check` produced.
///
/// The three cases are genuinely different and a reader must be able to tell
/// them apart: no checker in this build, a clean or failing check, and a
/// checker that could not run at all.
enum TypeCheck {
    /// No checker was compiled in.
    Unavailable,
    /// Inference ran over the project.
    Checked(CheckReport),
    /// Inference could not run.
    Failed(CheckError),
}

impl TypeCheck {
    fn report(&self) -> Option<&CheckReport> {
        match self {
            Self::Checked(report) => Some(report),
            Self::Unavailable | Self::Failed(_) => None,
        }
    }

    fn diagnostics(&self) -> &[TypeDiagnostic] {
        self.report()
            .map_or(&[][..], |report| report.diagnostics.as_slice())
    }

    fn count(&self, severity: uf_check::Severity) -> usize {
        self.report().map_or(0, |report| report.count(severity))
    }

    fn status(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Checked(_) => "checked",
            Self::Failed(_) => "failed",
        }
    }
}

pub(crate) fn check(cwd: &Utf8Path, ui: &mut Ui, json: bool) -> Result<()> {
    let mut progress = ui.progress();
    progress.draw("scanning sources");
    let (lint, sources) = run_lint(cwd)?;
    progress.draw("type checking");
    let types = type_check(&sources);
    progress.finish();
    drop(progress);

    if json {
        ui.json(&payload(&lint, &types))?;
    } else {
        render(ui, &lint, &sources, &types);
    }

    let errors = severity_count(&lint, Severity::Error) + types.count(uf_check::Severity::Error);
    if errors > 0 {
        bail!(
            "{} failed with {}",
            LintCommand::Check.title(),
            plural(errors, "error")
        );
    }
    if let TypeCheck::Failed(error) = types {
        return Err(error.into());
    }
    Ok(())
}

/// Run inference over every source the linter collected.
///
/// Syntax errors are dropped here rather than in `uf_check`: the checker
/// reports them because a library caller needs to know why inference did not
/// run, but `uf lint` has already reported the same syntax error with its own
/// rule id, and printing it twice helps nobody.
fn type_check(sources: &[SourceFile]) -> TypeCheck {
    let inputs: Vec<Source<'_>> = sources
        .iter()
        .map(|source| Source::new(&source.path, &source.source))
        .collect();

    match check_sources(&inputs, &CheckLimits::default()) {
        Ok(mut report) => {
            report
                .diagnostics
                .retain(|diagnostic| diagnostic.kind != uf_check::DiagnosticKind::Parse);
            TypeCheck::Checked(report)
        }
        Err(error) if error.is_unavailable() => TypeCheck::Unavailable,
        Err(error) => TypeCheck::Failed(error),
    }
}

fn payload(lint: &LintReport, types: &TypeCheck) -> Value {
    let mut value = lint_payload(LintCommand::Check, lint);
    let errors = severity_count(lint, Severity::Error) + types.count(uf_check::Severity::Error);
    let warnings = severity_count(lint, Severity::Warn) + types.count(uf_check::Severity::Warning);

    let mut type_check = json!({
        "backend": backend_name(active_backend()),
        "status": types.status(),
        "diagnostics": types.diagnostics(),
    });
    if let Some(report) = types.report() {
        type_check["filesChecked"] = json!(report.files_checked);
        type_check["elapsedMs"] = json!(report.elapsed.as_secs_f64() * 1000.0);
        type_check["builtinsMs"] = json!(report.builtins.cold_elapsed.as_secs_f64() * 1000.0);
        type_check["builtinsCold"] = json!(report.builtins.cold);
        type_check["untypedModules"] = json!(report.untyped_modules);
    }
    if let TypeCheck::Failed(error) = types {
        type_check["error"] = json!(error.to_string());
    }

    value["errors"] = json!(errors);
    value["warnings"] = json!(warnings);
    value["typeCheck"] = type_check;
    value
}

fn render(ui: &mut Ui, lint: &LintReport, sources: &[SourceFile], types: &TypeCheck) {
    let lint_errors = severity_count(lint, Severity::Error);
    let lint_warnings = severity_count(lint, Severity::Warn);
    let groups = group_by_path(&lint.diagnostics);

    ui.render(|renderer, out| {
        renderer.banner(out, LintCommand::Check.title(), None);
        renderer.blank(out);
    });

    for group in &groups {
        render_group(ui, group, sources);
    }
    if groups.len() > 1 {
        render_file_summary(ui, &groups);
    }

    render_type_diagnostics(ui, sources, types);

    render_verdict(
        ui,
        lint,
        lint_errors + types.count(uf_check::Severity::Error),
        lint_warnings + types.count(uf_check::Severity::Warning),
    );
    render_type_footer(ui, types);
}

/// Type diagnostics, grouped by file, as code frames.
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

    ui.render(|renderer, out| {
        renderer.theme().path.paint(renderer.color(), path, out);
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
            renderer.blank(out);
            renderer.status(
                out,
                Status::Info,
                "type inference is not compiled into this build",
            );
        }),
        TypeCheck::Failed(error) => {
            let detail = error.to_string();
            ui.render(|renderer, out| {
                renderer.blank(out);
                renderer.status(out, Status::Warn, &detail);
            });
        }
        TypeCheck::Checked(report) => {
            let files = report.files_checked.to_string();
            let inference = format!("{:.1?}", report.elapsed);
            let builtins = format!(
                "{:.1?} ({})",
                report.builtins.cold_elapsed,
                if report.builtins.cold { "cold" } else { "warm" }
            );
            let untyped = untyped_module_list(report);
            ui.render(|renderer, out| {
                renderer.blank(out);
                renderer.key_values(
                    out,
                    2,
                    &[
                        KeyValue::toned("types checked", &files, Tone::Number),
                        KeyValue::toned("inference", &inference, Tone::Muted),
                        KeyValue::toned("builtins", &builtins, Tone::Muted),
                    ],
                );
                if !untyped.is_empty() {
                    renderer.blank(out);
                    push_spaces(out, 2);
                    renderer.status(
                        out,
                        Status::Info,
                        "these imports are typed as any; uf does not check across modules yet",
                    );
                    let items: Vec<&str> = untyped.iter().map(String::as_str).collect();
                    renderer.bullet_list(out, 4, &items);
                }
            });
        }
    }
}

/// The untyped imports to name on screen, with the tail summarised.
///
/// A project pulls in more packages than a terminal should list, and the full
/// set is always in `--json`.
fn untyped_module_list(report: &CheckReport) -> Vec<String> {
    let mut named: Vec<String> = report
        .untyped_modules
        .iter()
        .take(UNTYPED_MODULES_SHOWN)
        .map(ToString::to_string)
        .collect();
    let overflow = report
        .untyped_modules
        .len()
        .saturating_sub(UNTYPED_MODULES_SHOWN);
    if overflow > 0 {
        named.push(format!("and {overflow} more"));
    }
    named
}

#[cfg(test)]
mod tests;
