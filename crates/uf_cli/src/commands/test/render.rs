//! What `uf test` puts on the screen, and what it puts on a pipe.
//!
//! Two audiences, two rules. A person gets a status line per test, a code frame
//! under each failure pointing at the assertion that broke, whatever the tests
//! printed, the slowest files, and a summary. A program gets JSON with no
//! styling, no progress, and no field whose value depends on anything but the
//! suite — every duration is grouped into its own key so a caller can diff two
//! runs by ignoring them.

use anyhow::Result;
use camino::Utf8Path;
use uf_project::ProjectFile;
use uf_term::{
    Align, Cell, CodeFrame, Column, DiagnosticLevel, KeyValue, Phase, Status, Table, Tone,
    format_duration, push_padded, push_spaces,
};
use uf_test::{
    FileStatus, OutputChunk, OutputStream, SkipReason, TestFilter, TestRecord, TestRunReport,
    TestStatus, discover_tests, merge_plans,
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
    host: &uf_test::HostCommand,
    timing_note: Option<&str>,
    record_note: Option<&str>,
) {
    let label = project_label(root).to_string();
    let runtime = host.kind.program().to_string();
    let workers = args.options().concurrency.threads().to_string();
    let cache = timings_label(root);
    let cache = crate::support::relative_to(root, &cache);
    let summary_line = summary_line(report, duration);
    let counts = counts(report);
    let phases = phases.to_vec();

    let file_width = widest(report.records().map(|record| record.file.as_str()));
    let (output_groups, output_hidden) = other_output(report);
    let output_note = (output_hidden > 0).then(|| {
        format!(
            "{} not shown; `uf test --json` has every one",
            plural(output_hidden, "more line")
        )
    });
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

        if !output_groups.is_empty() {
            renderer.blank(out);
            renderer.heading(out, 2, "output");
            for group in &output_groups {
                push_spaces(out, 2);
                renderer.line(out, renderer.theme().path, &group.label);
                for (stream, text) in &group.lines {
                    push_spaces(out, 4);
                    renderer.line(out, output_style(renderer, *stream), text);
                }
            }
            if let Some(note) = &output_note {
                push_spaces(out, 2);
                renderer.status(out, Status::Info, note);
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
                KeyValue::new("files", &counts.files),
                KeyValue::new("workers", &workers),
                KeyValue::new("schedule", &counts.schedule),
                KeyValue::new("timings", &cache),
                KeyValue::new("host", &runtime),
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

/// How many lines of output the run's `output` section shows.
///
/// The rule, in two halves. Output from a *failing* test is evidence for the
/// failure and is drawn under it in full — a person reading a red run wants
/// every line of it, and the capture is already bounded per file by
/// [`uf_test::MAX_OUTPUT_BYTES_PER_FILE`], so "in full" is a bounded promise.
/// Everything else — a `console.log` left in a passing test, a line printed
/// while the module loaded — is gathered into one section after the tests and
/// cut off here, because six hundred passing tests that each print a line
/// would push the summary and the failures off the screen, which is the one
/// thing the report exists to show. What the cut leaves out is counted rather
/// than hidden, and `--json` carries every line either way.
const OUTPUT_LINES_SHOWN: usize = 20;

/// One block in the `output` section: what printed it, and what it printed.
struct OutputGroup {
    /// The file, and the test within it when a test was running.
    label: String,
    lines: Vec<(OutputStream, String)>,
}

/// Chunks flattened into lines, ready to draw.
///
/// Consecutive chunks on one stream are joined before the split, so a
/// `process.stdout.write` with no newline in it continues the line it started
/// rather than becoming one of its own. Control characters are escaped: this
/// text was written by the code under test, and a test that prints an ANSI
/// sequence must not be able to redraw the summary above it.
fn output_lines(chunks: &[OutputChunk]) -> Vec<(OutputStream, String)> {
    let mut runs: Vec<(OutputStream, String)> = Vec::new();
    for chunk in chunks {
        match runs.last_mut() {
            Some((stream, text)) if *stream == chunk.stream => text.push_str(&chunk.text),
            _ => runs.push((chunk.stream, chunk.text.clone())),
        }
    }

    let mut lines = Vec::new();
    for (stream, text) in runs {
        // A trailing newline ends the last line rather than starting an empty
        // one; a newline anywhere else does start one, blank included.
        let body = text.strip_suffix('\n').unwrap_or(&text);
        if body.is_empty() && text.is_empty() {
            continue;
        }
        for line in body.split('\n') {
            lines.push((stream, printable(line)));
        }
    }
    lines
}

/// `line` with everything that could move the cursor written out instead.
fn printable(line: &str) -> String {
    if !line.chars().any(|ch| ch.is_control() && ch != '\t') {
        return line.to_string();
    }
    line.chars()
        .flat_map(|ch| {
            if ch.is_control() && ch != '\t' {
                ch.escape_debug().collect::<Vec<_>>()
            } else {
                vec![ch]
            }
        })
        .collect()
}

/// The style one stream's output is drawn in.
fn output_style(renderer: &uf_term::Renderer, stream: OutputStream) -> uf_term::Style {
    match stream {
        // Muted, so a chatty test recedes behind the results it sits among;
        // stderr in the warning colour, because a test that wrote to stderr
        // was usually saying something went wrong.
        OutputStream::Stdout => renderer.theme().muted,
        OutputStream::Stderr => renderer.theme().warning,
    }
}

/// Draw captured output as a labelled block.
fn render_output(
    renderer: &uf_term::Renderer,
    out: &mut String,
    lines: &[(OutputStream, String)],
    indent: usize,
) {
    if lines.is_empty() {
        return;
    }
    push_spaces(out, indent);
    renderer.line(out, renderer.theme().key, "output");
    for (stream, text) in lines {
        push_spaces(out, indent + 2);
        renderer.line(out, output_style(renderer, *stream), text);
    }
}

/// Everything printed that no failure will show, in the order it was printed.
///
/// Returns the groups to draw and how many lines the cap left out.
fn other_output(report: &TestRunReport) -> (Vec<OutputGroup>, usize) {
    let mut groups = Vec::new();
    let mut budget = OUTPUT_LINES_SHOWN;
    let mut hidden = 0;
    let mut take = |label: String, chunks: &[OutputChunk]| {
        let mut lines = output_lines(chunks);
        if lines.len() > budget {
            hidden += lines.len() - budget;
            lines.truncate(budget);
        }
        budget -= lines.len();
        if !lines.is_empty() {
            groups.push(OutputGroup { label, lines });
        }
    };

    for file in &report.files {
        // The file's own output comes first because it was printed first:
        // while the module was being imported, before any case ran.
        take(file.file.clone(), &file.output);
        for record in &file.records {
            if record.status.is_failed() {
                continue;
            }
            take(format!("{}  {}", record.file, record.name), &record.output);
        }
    }
    (groups, hidden)
}

fn status_of(status: &TestStatus) -> Status {
    match status {
        TestStatus::Passed => Status::Success,
        TestStatus::Failed { .. } => Status::Error,
        TestStatus::Skipped { .. } | TestStatus::Todo => Status::Skip,
    }
}

/// Draw a code frame, and what the matcher said, under a failing test.
///
/// The frame points at the assertion rather than at the `it(` line, because
/// the `it(` line is not where the developer has to look. When the matcher
/// said what it wanted and what it got, those are printed under the frame:
/// a rendered value is often longer than a line of source and belongs beside
/// the frame rather than inside it.
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

    for failure in record.status.failures() {
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
        if let (Some(expected), Some(received)) = (&failure.expected, &failure.received) {
            renderer.key_values(
                out,
                8,
                &[
                    KeyValue::toned("expected", expected, Tone::Good),
                    KeyValue::toned("received", received, Tone::Bad),
                ],
            );
        }
    }
    // Under the failure, not in the section below it: what a failing test
    // printed is usually half of why it failed, and making a reader look for
    // it somewhere else is making them do the joining.
    if record.status.is_failed() {
        render_output(renderer, out, &output_lines(&record.output), 8);
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
