//! What `uf test` puts on the screen, and what it puts on a pipe.
//!
//! Two audiences, two rules. A person gets a status line per test, a code frame
//! under each failure pointing at the assertion that broke, the slowest files,
//! and a summary. A program gets JSON with no styling, no progress, and no
//! field whose value depends on anything but the suite — every duration is
//! grouped into its own key so a caller can diff two runs by ignoring them.

use anyhow::Result;
use camino::Utf8Path;
use uf_project::ProjectFile;
use uf_term::{
    Align, Cell, CodeFrame, Column, DiagnosticLevel, KeyValue, Phase, Status, Table, Tone,
    format_duration, push_padded, push_spaces,
};
use uf_test::{
    FileStatus, SkipReason, TestFilter, TestRecord, TestRunReport, TestStatus,
    UnsupportedAssertion, discover_tests, merge_plans,
};

use super::{SLOWEST_SHOWN, TestArgs, runner_plan, timings_label};
use crate::support::{plural, project_label};
use crate::ui::{Ui, widest};
use std::time::Duration;

/// `uf test --list`: every declaration, where it is, and whether it would run.
pub(super) fn render_list(
    ui: &mut Ui,
    root: &Utf8Path,
    files: &[ProjectFile],
    filter: &TestFilter,
) -> Result<()> {
    let plan = merge_plans(
        files
            .iter()
            .filter(|file| filter.matches_path(&file.relative_path))
            .map(|file| discover_tests(&file.relative_path, &file.source)),
    );
    let resolution = plan.resolve();
    let runner = runner_plan();

    let mut rows = Vec::with_capacity(plan.cases.len());
    for (index, case) in plan.cases.iter().enumerate() {
        let name = resolution.full_name(&plan, index);
        if !filter.matches_name(&name) {
            continue;
        }
        rows.push((
            format!("{}:{}:{}", case.file, case.line, case.column),
            name,
            selection_label(resolution.selection(index)),
        ));
    }

    let unsupported: Vec<(String, String)> = plan
        .unsupported
        .iter()
        .map(|entry| {
            (
                format!("{}:{}:{}", entry.file, entry.line, entry.column),
                entry.call.to_string(),
            )
        })
        .collect();
    let discovered = plural(plan.runnable_count(), "runnable test");
    let runtime = format!("{:?}", runner.runtime);
    let target = format!("{:?}", runner.performance_target);
    let label = project_label(root).to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf test", Some("discovery"));
        renderer.blank(out);
        let mut table = Table::new(vec![
            Column::left("location"),
            Column::left("test"),
            Column::left("selection"),
        ]);
        for (location, name, selection) in &rows {
            table.push(vec![
                Cell::toned(location, Tone::Path),
                Cell::new(name.as_str()),
                Cell::toned(selection, Tone::Muted),
            ]);
        }
        renderer.table(out, 2, &table);
        if !unsupported.is_empty() {
            renderer.blank(out);
            renderer.heading(out, 2, "unsupported declarations");
            for (location, call) in &unsupported {
                push_spaces(out, 2);
                renderer.status(out, Status::Warn, &format!("{location}  {call}"));
            }
        }
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("project", &label),
                KeyValue::new("runtime", &runtime),
                KeyValue::new("target", &target),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Info, &format!("discovered {discovered}"));
    });
    Ok(())
}

fn selection_label(selection: uf_test::Selection) -> String {
    match selection {
        uf_test::Selection::Run => "run".to_string(),
        uf_test::Selection::Todo => "todo".to_string(),
        uf_test::Selection::Skipped(SkipReason::Explicit) => "skip".to_string(),
        uf_test::Selection::Skipped(SkipReason::NotOnly) => "not .only".to_string(),
        uf_test::Selection::Skipped(SkipReason::Filtered) => "filtered".to_string(),
    }
}

/// Everything one run produced, rendered for a person.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_report(
    ui: &mut Ui,
    root: &Utf8Path,
    files: &[ProjectFile],
    report: &TestRunReport,
    phases: &[Phase],
    duration: Duration,
    args: &TestArgs,
    timing_note: Option<&str>,
    record_note: Option<&str>,
) {
    let runner = runner_plan();
    let label = project_label(root).to_string();
    let runtime = format!("{:?}", runner.runtime);
    let target = format!("{:?}", runner.performance_target);
    let workers = args.options().concurrency.threads().to_string();
    let cache = timings_label(root);
    let cache = crate::support::relative_to(root, &cache);
    let summary_line = summary_line(report, duration);
    let counts = counts(report);
    let phases = phases.to_vec();

    let file_width = widest(report.records().map(|record| record.file.as_str()));
    let slowest = slowest_rows(report);
    let file_problems = file_problems(report);
    let unsupported_declarations: Vec<String> = report
        .plan
        .unsupported
        .iter()
        .map(|entry| {
            format!(
                "{}:{}:{} {}",
                entry.file, entry.line, entry.column, entry.call
            )
        })
        .collect();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf test", Some(&label));
        renderer.blank(out);

        let mut line = String::new();
        for file in &report.files {
            for record in &file.records {
                line.clear();
                push_padded(&mut line, &record.file, file_width + 2, Align::Left);
                line.push_str(&record.name);
                push_spaces(out, 2);
                renderer.status(out, status_of(&record.status), &line);
                render_details(renderer, out, files, record);
            }
        }

        if !file_problems.is_empty() {
            renderer.blank(out);
            renderer.heading(out, 2, "files that did not complete");
            for problem in &file_problems {
                push_spaces(out, 2);
                renderer.status(out, Status::Error, problem);
            }
        }

        if !unsupported_declarations.is_empty() {
            renderer.blank(out);
            renderer.heading(out, 2, "unsupported declarations");
            for entry in &unsupported_declarations {
                push_spaces(out, 2);
                renderer.status(out, Status::Warn, entry);
            }
        }

        if !slowest.is_empty() {
            renderer.blank(out);
            renderer.heading(out, 2, "slowest files");
            let mut table = Table::new(vec![Column::left("file"), Column::right("duration")]);
            for (file, taken) in &slowest {
                table.push(vec![
                    Cell::toned(file, Tone::Path),
                    Cell::toned(taken, Tone::Number),
                ]);
            }
            renderer.table(out, 2, &table);
        }

        renderer.blank(out);
        renderer.timings(out, 2, &phases, Some(duration));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("passed", &counts.passed, Tone::Good),
                KeyValue::toned("failed", &counts.failed, Tone::Bad),
                KeyValue::toned("skipped", &counts.skipped, Tone::Muted),
                KeyValue::toned("todo", &counts.todo, Tone::Muted),
                KeyValue::toned("unsupported assertions", &counts.unsupported, Tone::Warn),
                KeyValue::new("files", &counts.files),
                KeyValue::new("workers", &workers),
                KeyValue::new("schedule", &counts.schedule),
                KeyValue::new("timings", &cache),
                KeyValue::new("runtime", &runtime),
                KeyValue::new("target", &target),
            ],
        );

        for note in [timing_note, record_note].into_iter().flatten() {
            push_spaces(out, 2);
            renderer.status(out, Status::Warn, note);
        }

        renderer.blank(out);
        renderer.status(
            out,
            if report.is_success() {
                Status::Success
            } else {
                Status::Error
            },
            &summary_line,
        );
    });
}

fn status_of(status: &TestStatus) -> Status {
    match status {
        TestStatus::Passed => Status::Success,
        TestStatus::Failed { .. } => Status::Error,
        TestStatus::Unsupported { .. } => Status::Warn,
        TestStatus::Skipped { .. } | TestStatus::Todo => Status::Skip,
    }
}

/// Draw a code frame under a failing or unsupported test.
///
/// The frame points at the assertion, not at the `it(` line, because the `it(`
/// line is not where the developer has to look.
fn render_details(
    renderer: &uf_term::Renderer,
    out: &mut String,
    files: &[ProjectFile],
    record: &TestRecord,
) {
    let source = files
        .iter()
        .find(|file| file.relative_path == record.file)
        .map(|file| file.source.as_str());

    match &record.status {
        TestStatus::Failed {
            failures,
            unsupported,
        } => {
            for failure in failures {
                let frame = CodeFrame {
                    level: DiagnosticLevel::Error,
                    rule: None,
                    message: &failure.message,
                    path: &record.file,
                    line: failure.line,
                    column: failure.column,
                    span: failure.span,
                    source_line: source.and_then(|source| line_at(source, failure.line)),
                    label: None,
                };
                renderer.code_frame_at(out, &frame, 6);
            }
            render_unsupported(renderer, out, source, record, unsupported);
        }
        TestStatus::Unsupported { assertions } => {
            render_unsupported(renderer, out, source, record, assertions)
        }
        _ => {}
    }
}

fn render_unsupported(
    renderer: &uf_term::Renderer,
    out: &mut String,
    source: Option<&str>,
    record: &TestRecord,
    assertions: &[UnsupportedAssertion],
) {
    for assertion in assertions {
        let message = assertion.reason.describe();
        let frame = CodeFrame {
            level: DiagnosticLevel::Warning,
            rule: Some("unsupported"),
            message: &message,
            path: &record.file,
            line: assertion.line,
            column: assertion.column,
            span: assertion.span,
            source_line: source.and_then(|source| line_at(source, assertion.line)),
            label: None,
        };
        renderer.code_frame_at(out, &frame, 6);
    }
}

/// The one-based `line` of `source`, without its terminator.
fn line_at(source: &str, line: usize) -> Option<&str> {
    source
        .lines()
        .nth(line.checked_sub(1)?)
        .map(|line| line.trim_end_matches('\r'))
}

struct Counts {
    passed: String,
    failed: String,
    skipped: String,
    todo: String,
    unsupported: String,
    files: String,
    schedule: String,
}

fn counts(report: &TestRunReport) -> Counts {
    let summary = &report.summary;
    Counts {
        passed: summary.passed.to_string(),
        failed: summary.failed.to_string(),
        skipped: summary.skipped.to_string(),
        todo: summary.todo.to_string(),
        unsupported: summary.unsupported_assertions.to_string(),
        files: summary.files.to_string(),
        schedule: format!(
            "{} recorded, {} by size",
            summary.scheduled_warm, summary.scheduled_cold
        ),
    }
}

fn summary_line(report: &TestRunReport, duration: Duration) -> String {
    let summary = &report.summary;
    let mut line = format!("{} passed, {} failed", summary.passed, summary.failed);
    if summary.skipped > 0 {
        line.push_str(&format!(", {} skipped", summary.skipped));
    }
    if summary.todo > 0 {
        line.push_str(&format!(", {} todo", summary.todo));
    }
    if summary.unsupported_assertions > 0 {
        line.push_str(&format!(", {} unsupported", summary.unsupported_assertions));
    }
    if summary.unsupported_declarations > 0 {
        line.push_str(&format!(
            ", {} unexpandable",
            summary.unsupported_declarations
        ));
    }
    if summary.bailed {
        line.push_str(" (bailed)");
    }
    line.push_str(" in ");
    line.push_str(&format_duration(duration));
    line
}

fn slowest_rows(report: &TestRunReport) -> Vec<(String, String)> {
    report
        .slowest_files(SLOWEST_SHOWN)
        .into_iter()
        .filter(|file| file.duration_micros > 0)
        .map(|file| (file.file.clone(), format_duration(file.duration())))
        .collect()
}

fn file_problems(report: &TestRunReport) -> Vec<String> {
    report
        .files
        .iter()
        .filter(|file| file.status != FileStatus::Completed)
        .map(|file| format!("{} {}", file.file, file.status.describe()))
        .collect()
}
