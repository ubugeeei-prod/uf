//! The `--json` document.
//!
//! One rule: every value is a function of the suite alone, except the
//! `durationMicros` fields. A caller diffing two runs can therefore ignore
//! exactly those and expect byte-identical output — which is what the
//! integration tests assert.

use serde_json::{Value, json};
use uf_test::{
    FileReport, FileStatus, OutputChunk, SkipReason, TestRecord, TestRunReport, TestStatus,
};

/// Build the document.
pub(super) fn test_payload(report: &TestRunReport) -> Value {
    let summary = &report.summary;
    json!({
        "command": "uf test",
        "files": summary.files,
        "passed": summary.passed,
        "failed": summary.failed,
        "skipped": summary.skipped,
        "todo": summary.todo,
        "unsupportedDeclarations": summary.unsupported_declarations,
        "failedFiles": summary.failed_files,
        "scheduledWarm": summary.scheduled_warm,
        "scheduledCold": summary.scheduled_cold,
        "bailed": summary.bailed,
        "success": report.is_success(),
        "durationMicros": summary.duration_micros,
        "fileReports": report.files.iter().map(file_payload).collect::<Vec<_>>(),
        "tests": report.records().map(record_payload).collect::<Vec<_>>(),
        "declarations": report.plan.unsupported.iter().map(|entry| json!({
            "file": entry.file,
            "call": entry.call,
            "line": entry.line,
            "column": entry.column,
        })).collect::<Vec<_>>(),
    })
}

fn file_payload(file: &FileReport) -> Value {
    json!({
        "file": file.file,
        "status": status_name(&file.status),
        "reason": file.status.describe(),
        "durationMicros": file.duration_micros,
        "tests": file.records.len(),
        "output": output_payload(&file.output),
    })
}

/// What was printed, exactly as it was printed.
///
/// Unabridged, unlike the terminal report: the terminal has a screen to fit
/// and a reader to keep, and a program reading this has neither. The text is
/// whatever the test wrote, control characters included — JSON escaping is
/// what makes that safe to carry, so a consumer that renders it to a terminal
/// is the one that has to decide what to do with an escape sequence.
fn output_payload(chunks: &[OutputChunk]) -> Vec<Value> {
    chunks
        .iter()
        .map(|chunk| {
            json!({
                "stream": chunk.stream.as_str(),
                "text": chunk.text,
            })
        })
        .collect()
}

fn status_name(status: &FileStatus) -> &'static str {
    match status {
        FileStatus::Completed => "completed",
        FileStatus::TimedOut { .. } => "timed-out",
        FileStatus::LoadFailed { .. } => "load-failed",
        FileStatus::HostFailed { .. } => "host-failed",
        FileStatus::NotRun => "not-run",
    }
}

fn record_payload(record: &TestRecord) -> Value {
    json!({
        "file": record.file,
        "name": record.name,
        "line": record.line,
        "column": record.column,
        "attempts": record.attempts,
        "durationMicros": record.duration_micros,
        "status": test_status_name(&record.status),
        "output": output_payload(&record.output),
        "failures": record.status.failures().iter().map(|failure| json!({
            "message": failure.message,
            "line": failure.line,
            "column": failure.column,
            "expected": failure.expected,
            "received": failure.received,
        })).collect::<Vec<_>>(),
    })
}

fn test_status_name(status: &TestStatus) -> &'static str {
    match status {
        TestStatus::Passed => "passed",
        TestStatus::Failed { .. } => "failed",
        TestStatus::Skipped {
            reason: SkipReason::Explicit,
        } => "skipped",
        TestStatus::Skipped {
            reason: SkipReason::NotOnly,
        } => "not-only",
        TestStatus::Skipped {
            reason: SkipReason::Filtered,
        } => "filtered",
        TestStatus::Todo => "todo",
    }
}
