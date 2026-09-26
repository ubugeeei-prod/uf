//! What `uf test` puts on the screen, and what it puts on a pipe.
//!
//! Two audiences, two rules. A person gets a line per file as that file
//! finishes — its mark, its duration and its counts — with a code frame under
//! each failure pointing at the assertion that broke, and then, once the run
//! is over, the failures again by name, whatever the tests printed, the
//! slowest files, and a summary. A program gets JSON with no styling, no
//! progress, and no field whose value depends on anything but the suite —
//! every duration is grouped into its own key so a caller can diff two runs by
//! ignoring them.
//!
//! The per-file lines are drawn by [`render_file`], which the stream calls as
//! each file finishes (see [`super::stream`]) and [`render_report`] calls for a
//! report nobody watched being made — `uf test --merge-shards`, which ran
//! nothing itself.

use anyhow::Result;
use camino::Utf8Path;
use uf_project::ProjectFile;
use uf_term::{
    Align, Cell, CodeFrame, Column, DiagnosticLevel, KeyValue, Phase, Status, Table, Tone,
    display_width, format_duration, push_padded, push_spaces,
};
use uf_test::{
    FileReport, FileStatus, OutputChunk, OutputStream, SkipReason, TestFilter, TestRecord,
    TestRunReport, TestStatus, discover_tests, merge_plans,
};

use super::coverage::CoverageSection;
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
    benches: bool,
) -> Result<()> {
    let plan = merge_plans(
        files
            .iter()
            .filter(|file| filter.matches_path(&file.relative_path))
            .map(|file| discover_tests(&file.relative_path, &file.source)),
    );
    let mut resolution = plan.resolve();
    // The kind of run decides as the worker does: benchmarks are skipped by a
    // run of the tests, and tests by a run of the benchmarks.
    resolution.select_run_kind(&plan, benches);
    let runner = runner_plan();

    let mut rows = Vec::with_capacity(plan.cases.len());
    for (index, case) in plan.cases.iter().enumerate() {
        let name = resolution.full_name(&plan, index);
        if !filter.matches_name(&name) {
            continue;
        }
        rows.push((
            uf_infra::into_string(uf_infra::cstr!(
                "{}:{}:{}",
                case.file,
                case.line,
                case.column
            )),
            name,
            selection_label(resolution.selection(index)),
        ));
    }

    let unsupported: Vec<(String, String)> = plan
        .unsupported
        .iter()
        .map(|entry| {
            (
                uf_infra::into_string(uf_infra::cstr!(
                    "{}:{}:{}",
                    entry.file,
                    entry.line,
                    entry.column
                )),
                entry.describe(),
            )
        })
        .collect();
    let discovered = if benches {
        plural(plan.bench_count(), "benchmark")
    } else {
        plural(plan.runnable_count(), "runnable test")
    };
    if ui.is_json() {
        let selected: Vec<String> = super::test_bearing(files.to_vec())
            .into_iter()
            .filter(|file| filter.matches_path(&file.relative_path))
            .map(|file| file.relative_path)
            .collect();
        let tests: Vec<_> = rows.iter().map(|(location, name, selection)| {
            serde_json::json!({ "location": location, "name": name, "selection": selection })
        }).collect();
        ui.json(&serde_json::json!({
            "command": "uf test", "list": true, "files": selected,
            "tests": tests, "unsupported": unsupported,
        }))?;
        return Ok(());
    }
    let runtime = uf_infra::into_string(uf_infra::cstr!("{:?}", runner.runtime));
    let target = uf_infra::into_string(uf_infra::cstr!("{:?}", runner.performance_target));
    let label = project_label(root).to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf test", Some("discovery"));
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
                renderer.status(
                    out,
                    Status::Warn,
                    uf_infra::cstr!("{location}  {call}").as_str(),
                );
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
        renderer.status(
            out,
            Status::Info,
            uf_infra::cstr!("discovered {discovered}").as_str(),
        );
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
        uf_test::Selection::Skipped(SkipReason::Bench) => "bench".to_string(),
        uf_test::Selection::Skipped(SkipReason::NotBench) => "not a bench".to_string(),
    }
}

/// Everything one run produced, rendered for a person.
///
/// `streamed` says whether the files were already drawn one by one as they
/// finished, with the banner above them. When they were, this draws only what
/// can be said once the run is over; when they were not — a merge of shards —
/// it draws the banner and every file first, in path order.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_report(
    ui: &mut Ui,
    root: &Utf8Path,
    files: &[ProjectFile],
    report: &TestRunReport,
    phases: &[Phase],
    duration: Duration,
    args: &TestArgs,
    host: Option<&uf_test::HostCommand>,
    timing_note: Option<&str>,
    record_note: Option<&str>,
    coverage: Option<&CoverageSection>,
    streamed: bool,
) {
    let label = project_label(root).to_string();
    // The browser is named, not just labelled. Every other host answers to one
    // word because a project declared it and `uf explain` can print it; a
    // browser was *found* on this machine, and a run whose result depends on
    // which binary answered and does not say which is a run nobody can
    // reproduce from the report.
    let runtime = match host {
        Some(host) => match host.browser.as_ref() {
            Some(browser) => uf_infra::into_string(uf_infra::cstr!(
                "{browser} (driven from {})",
                host.kind.program()
            )),
            None => host.kind.program().to_string(),
        },
        // `uf test --merge-shards` started no host: each shard ran on its own.
        None => String::from("each shard's own"),
    };
    // What the run started, which is not what `-j` allowed once `uf` sizes the
    // pool from recorded durations; the configured count stands in for a run
    // that started none.
    let workers = match report.summary.workers {
        0 => args.options().concurrency.threads(),
        started => started,
    }
    .to_string();
    let cache = timings_label(root);
    let cache = crate::support::relative_to(root, &cache);
    let summary_line = summary_line(report, duration);
    let counts = counts(report);
    let phases = phases.to_vec();

    let path_width = path_column(report.files.iter().map(|file| file.file.as_str()));
    let failed_tests = failed_tests(report);
    let (output_groups, output_hidden) = other_output(report);
    let output_note = (output_hidden > 0).then(|| {
        uf_infra::into_string(uf_infra::cstr!(
            "{} not shown; `uf test --json` has every one",
            plural(output_hidden, "more line")
        ))
    });
    let slowest = slowest_rows(report);
    let coverage_rows = coverage.map(coverage_block);
    let file_problems = file_problems(report);
    let unsupported_declarations: Vec<String> = report
        .plan
        .unsupported
        .iter()
        .map(|entry| {
            uf_infra::into_string(uf_infra::cstr!(
                "{}:{}:{} {}",
                entry.file,
                entry.line,
                entry.column,
                entry.describe()
            ))
        })
        .collect();

    ui.render(|renderer, out| {
        if !streamed {
            render_banner(renderer, out, &label);
            for file in &report.files {
                let source = files
                    .iter()
                    .find(|project| project.relative_path == file.file)
                    .map(|project| project.source.as_str());
                render_file(renderer, out, source, file, path_width);
            }
        }

        // Once more by name, and only when the files scrolled past as they
        // finished: the frames are up there, but the end of a long run is
        // where a reader looks, and "which ones" should not need a search.
        if streamed && !failed_tests.is_empty() {
            renderer.blank(out);
            renderer.heading(out, 2, "failed");
            for failed in &failed_tests {
                push_spaces(out, 2);
                renderer.status(out, Status::Error, failed);
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

        if let Some(coverage) = &coverage_rows {
            renderer.blank(out);
            renderer.heading(out, 2, "coverage");
            if coverage.show_table {
                let mut table = Table::new(vec![
                    Column::left("file"),
                    Column::right("lines"),
                    Column::right("functions"),
                    Column::right("branches"),
                    Column::left("uncovered"),
                ]);
                for row in &coverage.rows {
                    table.push(vec![
                        Cell::toned(&row.file, Tone::Path),
                        Cell::toned(&row.lines, tone_for(row.lines_meets)),
                        Cell::toned(&row.functions, tone_for(row.functions_meets)),
                        Cell::toned(&row.branches, tone_for(row.branches_meets)),
                        Cell::toned(&row.uncovered, Tone::Muted),
                    ]);
                }
                renderer.table(out, 2, &table);
            }
            renderer.blank(out);
            renderer.key_values(
                out,
                2,
                &[
                    KeyValue::toned("lines", &coverage.total_lines, Tone::Number),
                    KeyValue::toned("functions", &coverage.total_functions, Tone::Number),
                    KeyValue::toned("branches", &coverage.total_branches, Tone::Number),
                ],
            );
            for note in &coverage.notes {
                push_spaces(out, 2);
                renderer.status(out, Status::Info, note);
            }
            for violation in &coverage.violations {
                push_spaces(out, 2);
                renderer.status(out, Status::Error, violation);
            }
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

/// The banner a run opens with, and the blank line under it.
pub(super) fn render_banner(renderer: &uf_term::Renderer, out: &mut String, label: &str) {
    renderer.banner(out, "uf test", Some(label));
}

/// The widest a file line pads its path to.
///
/// A path column exists so the durations after it line up. One deeply nested
/// file should not push every other line's duration off the right edge of an
/// eighty-column terminal, so past this the long path simply runs on and its
/// own line is the only one out of step.
const PATH_COLUMN_MAX: usize = 56;

/// How wide the duration column on a file line is: `999.9ms` and `12.34s`
/// both fit, so the counts after it start in one column.
const DURATION_COLUMN: usize = 7;

/// The column a file line pads its path to, for these paths.
///
/// Worked out from every file the run *will* report rather than the ones
/// finished so far, so the first line drawn is already aligned with the last.
pub(super) fn path_column<'a>(paths: impl IntoIterator<Item = &'a str>) -> usize {
    widest(paths).min(PATH_COLUMN_MAX)
}

/// The mark a file line carries.
///
/// Red when anything in the file failed or the file itself did not complete;
/// green when something passed and nothing failed; the skip mark for a file in
/// which nothing ran at all, because every case was skipped or left to do.
pub(super) fn file_status(file: &FileReport) -> Status {
    let failed = file.status != FileStatus::Completed
        || file.records.iter().any(|record| record.status.is_failed());
    if failed {
        Status::Error
    } else if file.records.iter().any(|record| record.status.is_passed()) {
        Status::Success
    } else {
        Status::Skip
    }
}

/// One finished file: a line saying how it went, and under it only what a
/// reader needs to act on — each failure with its code frame and what it
/// printed, why a file did not complete, a skip that gave a reason, and a
/// test that only passed on a retry.
///
/// A passing test is not given a line of its own. It is counted on the file's
/// line, and `uf test --json` names every one; a suite of six hundred tests
/// drawn a line each is a screen of green that pushes the one red line out of
/// sight.
///
/// `source` is the file's text, for the code frames; a frame without it still
/// names the line.
pub(super) fn render_file(
    renderer: &uf_term::Renderer,
    out: &mut String,
    source: Option<&str>,
    file: &FileReport,
    path_width: usize,
) {
    let color = renderer.color();
    let theme = renderer.theme();
    let status = file_status(file);

    push_spaces(out, 2);
    renderer
        .status_style(status)
        .paint(color, status.glyph(renderer.glyph_set()), out);
    out.push(' ');
    theme.path.paint(color, &file.file, out);
    push_spaces(
        out,
        path_width.saturating_sub(display_width(&file.file)) + 2,
    );
    let mut cell = String::new();
    // A file that never ran took no time, and `0ns` would read as a claim that
    // it did, very quickly.
    if file.duration_micros > 0 {
        push_padded(
            &mut cell,
            &format_duration(file.duration()),
            DURATION_COLUMN,
            Align::Right,
        );
    } else {
        push_spaces(&mut cell, DURATION_COLUMN);
    }
    theme.muted.paint(color, &cell, out);
    out.push_str("  ");
    push_counts(renderer, out, file);
    out.push('\n');

    if file.status != FileStatus::Completed {
        push_spaces(out, 4);
        renderer.status(out, Status::Error, &file.status.describe());
        if let FileStatus::LoadFailed {
            stack: Some(stack), ..
        } = &file.status
        {
            for line in stack.lines().take(STACK_LINES_SHOWN) {
                push_spaces(out, 6);
                renderer.line(out, theme.muted, &printable(line.trim_end()));
            }
        }
    }

    for record in &file.records {
        match &record.status {
            TestStatus::Failed { .. } => {
                push_spaces(out, 4);
                renderer.status(out, Status::Error, &record.name);
                render_details(renderer, out, source, record);
            }
            TestStatus::Skipped {
                message: Some(_), ..
            } => {
                push_spaces(out, 4);
                renderer.status(out, Status::Skip, &record.name);
                render_details(renderer, out, source, record);
            }
            TestStatus::Passed if record.attempts > 1 => {
                push_spaces(out, 4);
                renderer.status(
                    out,
                    Status::Warn,
                    &uf_infra::into_string(uf_infra::cstr!(
                        "{}  passed on attempt {}",
                        record.name,
                        record.attempts
                    )),
                );
            }
            _ => {}
        }
    }
}

/// How many lines of a load failure's stack a file line shows.
///
/// The top of the stack is where the module threw; below that it is the
/// loader importing it, which is the same for every file. `--json` has it all.
const STACK_LINES_SHOWN: usize = 8;

/// `3 passed · 1 failed · 1 skipped`, each count in its own colour, and only
/// the counts that are not zero.
fn push_counts(renderer: &uf_term::Renderer, out: &mut String, file: &FileReport) {
    let (mut passed, mut failed, mut skipped, mut todo) = (0usize, 0usize, 0usize, 0usize);
    for record in &file.records {
        match record.status {
            TestStatus::Passed => passed += 1,
            TestStatus::Failed { .. } => failed += 1,
            TestStatus::Skipped { .. } => skipped += 1,
            TestStatus::Todo => todo += 1,
        }
    }
    let color = renderer.color();
    let theme = renderer.theme();
    let separator = match renderer.glyph_set() {
        uf_term::GlyphSet::Unicode => " · ",
        uf_term::GlyphSet::Ascii => ", ",
    };
    let mut first = true;
    for (count, word, style) in [
        (passed, "passed", theme.success),
        (failed, "failed", theme.error),
        (skipped, "skipped", theme.muted),
        (todo, "todo", theme.muted),
    ] {
        if count == 0 {
            continue;
        }
        if !first {
            theme.muted.paint(color, separator, out);
        }
        first = false;
        style.paint(color, uf_infra::cstr!("{count} {word}").as_str(), out);
    }
    if first {
        theme.muted.paint(color, "no tests", out);
    }
}

/// Every failing test, as `file:line  name`, in report order.
fn failed_tests(report: &TestRunReport) -> Vec<String> {
    report
        .failures()
        .map(|record| {
            uf_infra::into_string(uf_infra::cstr!(
                "{}:{}  {}",
                record.file,
                record.line,
                record.name
            ))
        })
        .collect()
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
            take(
                uf_infra::into_string(uf_infra::cstr!("{}  {}", record.file, record.name)),
                &record.output,
            );
        }
    }
    (groups, hidden)
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
    source: Option<&str>,
    record: &TestRecord,
) {
    if let TestStatus::Skipped {
        message: Some(message),
        ..
    } = &record.status
    {
        push_spaces(out, 8);
        renderer.status(out, Status::Skip, message);
    }

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
        schedule: uf_infra::into_string(uf_infra::cstr!(
            "{} recorded, {} by size",
            summary.scheduled_warm,
            summary.scheduled_cold
        )),
    }
}

fn summary_line(report: &TestRunReport, duration: Duration) -> String {
    let summary = &report.summary;
    let mut line = uf_infra::into_string(uf_infra::cstr!(
        "{} passed, {} failed",
        summary.passed,
        summary.failed
    ));
    if summary.skipped > 0 {
        uf_infra::append!(line, ", {} skipped", summary.skipped);
    }
    if summary.todo > 0 {
        uf_infra::append!(line, ", {} todo", summary.todo);
    }
    // Two words for the two kinds, because they mean different things to the
    // reader: an unexpandable form ran and was reported, and one from another
    // runner did not run at all — which is why only the second is red.
    let unexpandable = summary
        .unsupported_declarations
        .saturating_sub(summary.foreign_declarations);
    if unexpandable > 0 {
        uf_infra::append!(line, ", {unexpandable} unexpandable");
    }
    if summary.foreign_declarations > 0 {
        uf_infra::append!(line, ", {} another runner's", summary.foreign_declarations);
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
        .map(|file| {
            uf_infra::into_string(uf_infra::cstr!("{} {}", file.file, file.status.describe()))
        })
        .collect()
}

/// One row of the coverage table, already turned into text.
///
/// Rendered ahead of the closure that draws it because the closure borrows the
/// renderer and cannot allocate a percentage while it holds it.
struct CoverageLine {
    file: String,
    lines: String,
    functions: String,
    branches: String,
    uncovered: String,
    lines_meets: bool,
    functions_meets: bool,
    branches_meets: bool,
}

/// The whole coverage section, ready to draw.
struct CoverageBlock {
    rows: Vec<CoverageLine>,
    total_lines: String,
    total_functions: String,
    total_branches: String,
    notes: Vec<String>,
    violations: Vec<String>,
    show_table: bool,
}

/// Turn the collected coverage into the block the report draws.
///
/// A per-file ratio is coloured against the *per-file* threshold when the
/// project set one, and against nothing when it did not: a green number the
/// project never asked for is a number that means "somebody decided this was
/// enough", and nobody did.
fn coverage_block(section: &CoverageSection) -> CoverageBlock {
    let failing: std::collections::BTreeSet<(String, uf_test::Metric)> = section
        .violations
        .iter()
        .filter_map(|violation| {
            violation
                .file
                .as_ref()
                .map(|file| (file.clone(), violation.metric))
        })
        .collect();
    let meets = |file: &str, metric: uf_test::Metric| !failing.contains(&(file.to_owned(), metric));

    let rows = section
        .rows
        .iter()
        .map(|row| CoverageLine {
            file: row.file.clone(),
            lines: ratio_text(row.lines),
            functions: ratio_text(row.functions),
            branches: ratio_text(row.branches),
            uncovered: row.uncovered.clone(),
            lines_meets: meets(&row.file, uf_test::Metric::Lines),
            functions_meets: meets(&row.file, uf_test::Metric::Functions),
            branches_meets: meets(&row.file, uf_test::Metric::Branches),
        })
        .collect();

    let mut notes = Vec::new();
    if !section.never_loaded.is_empty() {
        // Named as a count and not folded into the percentage: a file no test
        // imports has no measured line to divide by, so counting it either way
        // would be an invention. The count is the honest form of it.
        notes.push(uf_infra::into_string(uf_infra::cstr!(
            "{} no test loaded, so nothing above is about {}: {}",
            plural(section.never_loaded.len(), "project file"),
            if section.never_loaded.len() == 1 {
                "it"
            } else {
                "them"
            },
            preview(&section.never_loaded),
        )));
    }
    if !section.unmapped.is_empty() {
        // Named, because this is the report admitting what it could not see.
        // A `@noflow` module the loader passes through untouched has no author
        // position for a count to belong to, and silently leaving it out is how
        // a coverage number starts describing a smaller program than the one
        // that ran.
        notes.push(uf_infra::into_string(uf_infra::cstr!(
            "{} ran with no source map back to Flow and {} left out: {}",
            plural(section.unmapped.len(), "module"),
            if section.unmapped.len() == 1 {
                "was"
            } else {
                "were"
            },
            preview(&section.unmapped),
        )));
    }
    for path in &section.written {
        notes.push(uf_infra::into_string(uf_infra::cstr!("wrote {path}")));
    }

    CoverageBlock {
        rows,
        total_lines: ratio_text(section.totals.lines),
        total_functions: ratio_text(section.totals.functions),
        total_branches: ratio_text(section.totals.branches),
        notes,
        violations: section
            .violations
            .iter()
            .map(uf_test::ThresholdViolation::describe)
            .collect(),
        show_table: section.show_table,
    }
}

/// The first few of a list, with the rest counted.
fn preview(paths: &[String]) -> String {
    const SHOWN: usize = 3;
    let head = paths
        .iter()
        .take(SHOWN)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    match paths.len().saturating_sub(SHOWN) {
        0 => head,
        more => uf_infra::into_string(uf_infra::cstr!("{head}, and {more} more")),
    }
}

fn ratio_text(ratio: uf_test::Ratio) -> String {
    uf_infra::into_string(uf_infra::cstr!(
        "{:.2}% ({}/{})",
        ratio.percent(),
        ratio.covered,
        ratio.total
    ))
}

/// Red only for a per-file threshold the project set and this file missed.
///
/// Not green for the ones it met: a colour that says "enough" is a judgement,
/// and the only place that judgement exists is a threshold somebody wrote down.
/// A project with none gets numbers and no verdict.
const fn tone_for(meets: bool) -> Tone {
    if meets { Tone::Plain } else { Tone::Bad }
}
