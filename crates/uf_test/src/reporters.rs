//! The formats a machine reads.
//!
//! # Why these three, and why not a fourth of uf's own
//!
//! `uf test --json` already exists and is uf's own shape: it carries every
//! declaration, every failure with its position, every line a test printed, and
//! it promises that nothing in it but the `durationMicros` fields depends on
//! anything other than the suite (`uf_cli`'s `test::payload`). It is the right
//! document for a tool somebody writes *for uf*. It is the wrong document for
//! a tool nobody wrote for uf, which is what a merge gate is made of, so none
//! of these three is an alternative to it — they are the shapes the software a
//! team already runs can read without being taught anything.
//!
//! * **LCOV** is what every code-host coverage integration and every diff
//!   annotator reads: Codecov, Coveralls, `genhtml`, the GitLab and GitHub
//!   coverage widgets. It is the format with the fewest ways to be wrong —
//!   lines, functions and branches, one record per file, no schema.
//! * **Cobertura** is what the JVM-shaped half of CI reads, and what Azure
//!   Pipelines and Jenkins' coverage plugins want. It carries the same numbers
//!   in XML, which is the only reason it exists here.
//! * **JUnit XML** is what every CI system parses for *test results* without
//!   being configured: it is how a failing case becomes an annotation on a pull
//!   request rather than a line in a log nobody opens.
//!
//! # Determinism
//!
//! Two runs of one suite must produce byte-identical coverage documents, so
//! nothing here reads a clock: Cobertura's mandatory `timestamp` is zero and
//! says why below. The JUnit document carries durations, because a CI system
//! showing test times is the point of it, and it is the only file here that
//! changes between two identical runs.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::coverage::{Coverage, FileCoverage, Metric, Ratio};
use crate::plan::SkipReason;
use crate::report::{TestRunReport, TestStatus};

/// The coverage document as LCOV.
///
/// One record per file in path order, and within a record everything in line
/// order, so a diff between two runs is a diff of the numbers.
#[must_use]
pub fn lcov(coverage: &Coverage) -> String {
    let mut out = String::new();
    for (path, file) in coverage.files() {
        // `TN:` is the test name, which LCOV allows to be empty and which
        // means nothing here: a uf run is one suite.
        out.push_str("TN:\n");
        let _ = writeln!(out, "SF:{path}");

        let names = function_names(file);
        for (function, name) in file.functions().zip(&names) {
            let _ = writeln!(out, "FN:{},{name}", function.at.line);
        }
        for (function, name) in file.functions().zip(&names) {
            let _ = writeln!(out, "FNDA:{},{name}", function.hits);
        }
        let functions = file.totals().functions;
        let _ = writeln!(out, "FNF:{}", functions.total);
        let _ = writeln!(out, "FNH:{}", functions.covered);

        // LCOV addresses a branch by line, block and ordinal within the block.
        // uf has one block per line and numbers the branches on it by column,
        // which is the only ordering that is the same in every run.
        let mut ordinal: BTreeMap<u32, u32> = BTreeMap::new();
        for branch in file.branches() {
            let at = ordinal.entry(branch.at.line).or_insert(0);
            let taken = if branch.hits == 0 {
                String::from("-")
            } else {
                branch.hits.to_string()
            };
            let _ = writeln!(out, "BRDA:{},0,{at},{taken}", branch.at.line);
            *at += 1;
        }
        let branches = file.totals().branches;
        let _ = writeln!(out, "BRF:{}", branches.total);
        let _ = writeln!(out, "BRH:{}", branches.covered);

        for (line, hits) in file.lines() {
            let _ = writeln!(out, "DA:{line},{hits}");
        }
        let lines = file.totals().lines;
        let _ = writeln!(out, "LF:{}", lines.total);
        let _ = writeln!(out, "LH:{}", lines.covered);
        out.push_str("end_of_record\n");
    }
    out
}

/// A name per function, unique within the file.
///
/// LCOV keys `FNDA` by name, so two functions in one file that share a name —
/// two callbacks V8 could not name, two nested closures called `render` —
/// would otherwise be one entry with one of their counts. The position is
/// appended only to the ones that need it, so the common case still reads as
/// the name the author wrote.
fn function_names(file: &FileCoverage) -> Vec<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let base: Vec<String> = file
        .functions()
        .map(|function| {
            if function.name.is_empty() {
                String::from("(anonymous)")
            } else {
                function.name.clone()
            }
        })
        .collect();
    for name in &base {
        *seen.entry(name.clone()).or_insert(0) += 1;
    }
    file.functions()
        .zip(&base)
        .map(|(function, name)| {
            if seen.get(name).copied().unwrap_or(0) > 1 {
                format!("{name}:{}:{}", function.at.line, function.at.column)
            } else {
                name.clone()
            }
        })
        .collect()
}

/// The coverage document as Cobertura XML.
///
/// Every file is one `class` in one unnamed `package`. Cobertura's model is
/// Java's — packages holding classes holding methods — and inventing a package
/// tree out of directory names would be a guess about the project's shape that
/// no reader of the file needs: the `filename` attribute is what every consumer
/// keys on.
#[must_use]
pub fn cobertura(coverage: &Coverage) -> String {
    let totals = coverage.totals();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" ?>\n");
    out.push_str(
        "<!DOCTYPE coverage SYSTEM \
         \"http://cobertura.sourceforge.net/xml/coverage-04.dtd\">\n",
    );
    // `timestamp` is required by the DTD and is zero on purpose: two runs of
    // one suite produce the same numbers, and a document that differs only in
    // when it was written cannot be compared with `diff`. A CI system that
    // wants the time has the file's mtime.
    let _ = writeln!(
        out,
        "<coverage lines-valid=\"{}\" lines-covered=\"{}\" line-rate=\"{}\" \
         branches-valid=\"{}\" branches-covered=\"{}\" branch-rate=\"{}\" \
         complexity=\"0\" version=\"uf\" timestamp=\"0\">",
        totals.lines.total,
        totals.lines.covered,
        rate(totals.lines),
        totals.branches.total,
        totals.branches.covered,
        rate(totals.branches),
    );
    out.push_str("  <sources>\n    <source>.</source>\n  </sources>\n");
    out.push_str("  <packages>\n");
    let _ = writeln!(
        out,
        "    <package name=\"\" line-rate=\"{}\" branch-rate=\"{}\" complexity=\"0\">",
        rate(totals.lines),
        rate(totals.branches)
    );
    out.push_str("      <classes>\n");
    for (path, file) in coverage.files() {
        let file_totals = file.totals();
        let _ = writeln!(
            out,
            "        <class name=\"{}\" filename=\"{}\" line-rate=\"{}\" \
             branch-rate=\"{}\" complexity=\"0\">",
            escape(path),
            escape(path),
            rate(file_totals.lines),
            rate(file_totals.branches)
        );
        out.push_str("          <methods>\n");
        for function in file.functions() {
            let _ = writeln!(
                out,
                "            <method name=\"{}\" signature=\"\" line-rate=\"{}\" \
                 branch-rate=\"1\" complexity=\"0\">",
                escape(&function.name),
                if function.hits > 0 { "1" } else { "0" }
            );
            let _ = writeln!(
                out,
                "              <lines><line number=\"{}\" hits=\"{}\"/></lines>",
                function.at.line, function.hits
            );
            out.push_str("            </method>\n");
        }
        out.push_str("          </methods>\n");
        out.push_str("          <lines>\n");
        let branches = branches_by_line(file);
        for (line, hits) in file.lines() {
            match branches.get(&line) {
                Some(ratio) => {
                    let _ = writeln!(
                        out,
                        "            <line number=\"{line}\" hits=\"{hits}\" branch=\"true\" \
                         condition-coverage=\"{:.0}% ({}/{})\"/>",
                        ratio.percent(),
                        ratio.covered,
                        ratio.total
                    );
                }
                None => {
                    let _ = writeln!(out, "            <line number=\"{line}\" hits=\"{hits}\"/>");
                }
            }
        }
        out.push_str("          </lines>\n");
        out.push_str("        </class>\n");
    }
    out.push_str("      </classes>\n    </package>\n  </packages>\n</coverage>\n");
    out
}

/// How many of a line's branches were taken, for the lines that have any.
fn branches_by_line(file: &FileCoverage) -> BTreeMap<u32, Ratio> {
    let mut by_line: BTreeMap<u32, Ratio> = BTreeMap::new();
    for branch in file.branches() {
        let entry = by_line.entry(branch.at.line).or_default();
        entry.total += 1;
        if branch.hits > 0 {
            entry.covered += 1;
        }
    }
    by_line
}

/// A Cobertura rate: a fraction between zero and one, four decimals.
fn rate(ratio: Ratio) -> String {
    format!("{:.4}", ratio.percent() / 100.0)
}

/// The run's results as JUnit XML.
///
/// One `testsuite` per file, one `testcase` per declaration. A file that could
/// not be loaded, timed out or lost its worker has no declarations to report,
/// so it becomes a single `testcase` carrying an `error` — which is what a CI
/// system needs to show a red mark rather than an empty suite that looks like
/// a pass.
#[must_use]
pub fn junit(report: &TestRunReport) -> String {
    let summary = &report.summary;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"uf test\" tests=\"{}\" failures=\"{}\" errors=\"{}\" \
         skipped=\"{}\" time=\"{}\">",
        summary.passed + summary.failed + summary.skipped + summary.todo,
        summary.failed,
        summary.failed_files,
        summary.skipped + summary.todo,
        seconds(summary.duration_micros)
    );
    for file in &report.files {
        let failures = file
            .records
            .iter()
            .filter(|record| record.status.is_failed())
            .count();
        let skipped = file
            .records
            .iter()
            .filter(|record| matches!(record.status, TestStatus::Skipped { .. } | TestStatus::Todo))
            .count();
        let errors = usize::from(file.status.is_fatal());
        let _ = writeln!(
            out,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{failures}\" errors=\"{errors}\" \
             skipped=\"{skipped}\" time=\"{}\">",
            escape(&file.file),
            file.records.len() + errors,
            seconds(file.duration_micros)
        );
        for record in &file.records {
            let _ = write!(
                out,
                "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\"",
                escape(&record.name),
                escape(&file.file),
                seconds(record.duration_micros)
            );
            match &record.status {
                TestStatus::Passed => out.push_str("/>\n"),
                TestStatus::Skipped { reason } => {
                    let _ = writeln!(
                        out,
                        ">\n      <skipped message=\"{}\"/>\n    </testcase>",
                        skip_reason(*reason)
                    );
                }
                TestStatus::Todo => {
                    out.push_str(">\n      <skipped message=\"todo\"/>\n    </testcase>\n");
                }
                TestStatus::Failed { failures } => {
                    out.push_str(">\n");
                    for failure in failures {
                        let _ = writeln!(
                            out,
                            "      <failure message=\"{}\" type=\"AssertionError\">{}</failure>",
                            escape(&failure.message),
                            escape(failure_body(&file.file, failure))
                        );
                    }
                    out.push_str("    </testcase>\n");
                }
            }
        }
        if file.status.is_fatal() {
            let _ = writeln!(
                out,
                "    <testcase name=\"{}\" classname=\"{}\" time=\"{}\">\n      \
                 <error message=\"{}\" type=\"FileError\"/>\n    </testcase>",
                escape(&file.file),
                escape(&file.file),
                seconds(file.duration_micros),
                escape(file.status.describe())
            );
        }
        out.push_str("  </testsuite>\n");
    }
    out.push_str("</testsuites>\n");
    out
}

/// Why a case was skipped, in the words `--json` uses.
const fn skip_reason(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::Explicit => "skipped",
        SkipReason::NotOnly => "not-only",
        SkipReason::Filtered => "filtered",
    }
}

fn failure_body(file: &str, failure: &crate::report::AssertionFailure) -> String {
    let mut body = format!(
        "{file}:{}:{}: {}",
        failure.line, failure.column, failure.message
    );
    if let Some(expected) = &failure.expected {
        let _ = write!(body, "\nexpected: {expected}");
    }
    if let Some(received) = &failure.received {
        let _ = write!(body, "\nreceived: {received}");
    }
    if let Some(stack) = &failure.stack {
        let _ = write!(body, "\n{stack}");
    }
    body
}

/// Microseconds as the seconds a JUnit reader expects.
fn seconds(micros: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a run long enough to lose microsecond precision here has already timed out"
    )]
    let seconds = micros as f64 / 1_000_000.0;
    format!("{seconds:.3}")
}

/// XML text, with everything XML cannot carry taken out.
///
/// Both halves matter. The five predefined entities are escaped because a test
/// name is written by whoever wrote the test and `<` in one must not close a
/// tag. Control characters are *dropped* rather than escaped, because XML 1.0
/// has no way to write most of them at all — a test that prints an ANSI escape
/// would otherwise produce a document no parser accepts, which is a green run
/// that CI reports as a broken artefact.
fn escape(text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(ch),
            ch if ch.is_control() => {}
            ch => out.push(ch),
        }
    }
    out
}

/// A one-line-per-file coverage table for a terminal.
///
/// Kept here beside the machine formats rather than in the renderer because it
/// is the same data answering the same question, and because a person and a
/// CI system disagreeing about the number is exactly the failure this whole
/// feature exists to prevent.
#[must_use]
pub fn text_rows(coverage: &Coverage) -> Vec<CoverageRow> {
    let mut rows: Vec<CoverageRow> = coverage
        .files()
        .map(|(path, file)| {
            let totals = file.totals();
            CoverageRow {
                file: path.to_owned(),
                lines: totals.lines,
                functions: totals.functions,
                branches: totals.branches,
                uncovered: uncovered_lines(file),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.file.cmp(&b.file));
    rows
}

/// One file's line in the terminal table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageRow {
    /// Project-relative path.
    pub file: String,
    /// Line coverage.
    pub lines: Ratio,
    /// Function coverage.
    pub functions: Ratio,
    /// Branch coverage.
    pub branches: Ratio,
    /// The author lines that never ran, as ranges: `12-15, 20`.
    pub uncovered: String,
}

impl CoverageRow {
    /// The ratio for one metric.
    #[must_use]
    pub const fn of(&self, metric: Metric) -> Ratio {
        match metric {
            Metric::Lines => self.lines,
            Metric::Functions => self.functions,
            Metric::Branches => self.branches,
        }
    }
}

/// The never-executed lines, collapsed into ranges.
///
/// Bounded, because a file nobody tested has as many uncovered lines as it has
/// lines and the summary is one row per file.
fn uncovered_lines(file: &FileCoverage) -> String {
    const MAX_RANGES: usize = 12;
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for (line, hits) in file.lines() {
        if hits > 0 {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.1 + 1 == line => last.1 = line,
            _ => ranges.push((line, line)),
        }
    }
    let more = ranges.len().saturating_sub(MAX_RANGES);
    ranges.truncate(MAX_RANGES);
    let mut out = ranges
        .iter()
        .map(|(from, to)| {
            if from == to {
                from.to_string()
            } else {
                format!("{from}-{to}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    if more > 0 {
        let _ = write!(out, ", and {more} more");
    }
    out
}
