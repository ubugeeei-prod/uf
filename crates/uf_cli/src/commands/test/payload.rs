//! The `--json` document.
//!
//! One rule: every value is a function of the suite alone, except the
//! `durationMicros` fields. A caller diffing two runs can therefore ignore
//! exactly those and expect byte-identical output — which is what the
//! integration tests assert.

use serde_json::{Value, json};
use uf_test::{FileReport, FileStatus, SkipReason, TestRecord, TestRunReport, TestStatus};

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
        "unsupported": summary.unsupported,
        "unsupportedAssertions": summary.unsupported_assertions,
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
    })
}

fn status_name(status: &FileStatus) -> &'static str {
    match status {
        FileStatus::Completed => "completed",
        FileStatus::TimedOut { .. } => "timed-out",
        FileStatus::TooLarge { .. } => "too-large",
        FileStatus::Panicked { .. } => "panicked",
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
        "status": test_status_name(&record.status),
        "failures": match &record.status {
            TestStatus::Failed { failures, .. } => failures.iter().map(|failure| json!({
                "message": failure.message,
                "line": failure.line,
                "column": failure.column,
            })).collect::<Vec<_>>(),
            _ => Vec::new(),
        },
        "unsupported": record.status.unsupported_assertions().iter().map(|assertion| json!({
            "expression": assertion.expression,
            "reason": assertion.reason.describe(),
            "line": assertion.line,
            "column": assertion.column,
        })).collect::<Vec<_>>(),
    })
}

fn test_status_name(status: &TestStatus) -> &'static str {
    match status {
        TestStatus::Passed => "passed",
        TestStatus::Failed { .. } => "failed",
        TestStatus::Unsupported { .. } => "unsupported",
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
