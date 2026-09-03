//! What a run produced.
//!
//! Every state a declaration can end in is a variant, not a flag: there is no
//! way to build a record that both passed and failed, and no way to report a
//! skip without saying why. The runner, the JSON payload and the terminal
//! renderer all read the same values, so `uf test` and `uf test --json` can
//! never disagree about an outcome.
//!
//! Durations are stored as whole microseconds rather than [`std::time::Duration`]
//! so that a report round-trips through JSON unchanged and can be compared for
//! equality. [`TestRunReport::without_timings`] zeroes every one of them, which
//! is how the determinism tests compare two runs that necessarily took
//! different amounts of time.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::plan::{SkipReason, TestPlan};

/// Longest source excerpt copied into a report.
///
/// A generated file can contain a megabyte-long expression; a report is not the
/// place to reproduce it.
pub const MAX_EXPRESSION_BYTES: usize = 200;

/// One assertion that did not hold, or one thrown `Error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionFailure {
    /// The failure message the matcher wrote, shown as it was written.
    pub message: String,
    /// One-based line of the failing assertion.
    pub line: usize,
    /// One-based column of the failing assertion.
    pub column: usize,
    /// Byte length of the failing expression, for a code frame caret.
    pub span: usize,
    /// What the matcher wanted, when it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// What arrived, when the matcher said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received: Option<String>,
    /// The stack, with the runner's own frames removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

/// How one declaration ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "status")]
pub enum TestStatus {
    /// The body ran and every assertion held.
    Passed,
    /// An assertion did not hold, the body threw, or it exceeded its budget.
    Failed {
        /// Why it failed. A body stops at its first failure, so this is one
        /// entry today; it is a list because a future concurrent case could
        /// produce more and a report should not have to change shape for it.
        failures: Vec<AssertionFailure>,
    },
    /// Excluded before it ran.
    Skipped {
        /// Why it was excluded.
        reason: SkipReason,
    },
    /// Declared with `.todo` and never written.
    Todo,
}

impl TestStatus {
    /// Whether the case counts towards a green run.
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }

    /// Whether the case fails the run.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Why it failed, or nothing when it did not.
    pub fn failures(&self) -> &[AssertionFailure] {
        match self {
            Self::Failed { failures } => failures,
            _ => &[],
        }
    }
}

/// One executed, skipped or unwritten declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRecord {
    /// Relative source file path.
    pub file: String,
    /// Fully qualified name: enclosing `describe` names, then the test name.
    pub name: String,
    /// One-based line of the registration call.
    pub line: usize,
    /// One-based column of the registration call.
    pub column: usize,
    /// How the case ended.
    pub status: TestStatus,
    /// How many times the case was executed, including retries.
    pub attempts: u32,
    /// How long the case took, in microseconds. Zero for one that never ran.
    #[serde(default)]
    pub duration_micros: u64,
}

/// Why a file did not finish, when it did not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "status")]
pub enum FileStatus {
    /// Every declaration in the file was executed or deliberately skipped.
    Completed,
    /// The file exceeded the per-file budget; declarations after that point did
    /// not run.
    TimedOut {
        /// The budget that was exceeded, in microseconds.
        budget_micros: u64,
    },
    /// The module threw while it was being imported, so there were no tests
    /// to run. Reported apart from a failing test, because "0 tests" for a
    /// module that could not load would be a lie.
    LoadFailed {
        /// What the host reported.
        message: String,
        /// The stack, with the runner's own frames removed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stack: Option<String>,
    },
    /// The worker died, or said something the runner could not read.
    HostFailed {
        /// What went wrong.
        message: String,
    },
    /// The run bailed before this file was scheduled.
    NotRun,
}

impl FileStatus {
    /// Whether the file fails the run on its own, before any assertion.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::TimedOut { .. } | Self::LoadFailed { .. } | Self::HostFailed { .. }
        )
    }

    /// A one-line explanation for a terminal report.
    pub fn describe(&self) -> String {
        match self {
            Self::Completed => "completed".to_string(),
            Self::TimedOut { budget_micros } => {
                format!("exceeded the per-file budget of {budget_micros}us")
            }
            Self::LoadFailed { message, .. } => format!("failed to load: {message}"),
            Self::HostFailed { message } => format!("the host failed: {message}"),
            Self::NotRun => "was not scheduled because the run bailed".to_string(),
        }
    }
}

/// What one file contributed to the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReport {
    /// Relative source file path.
    pub file: String,
    /// How the file ended.
    pub status: FileStatus,
    /// Wall-clock time spent on this file, in microseconds.
    pub duration_micros: u64,
    /// Every declaration in the file, in source order.
    pub records: Vec<TestRecord>,
}

impl FileReport {
    /// Wall-clock time spent on this file.
    pub fn duration(&self) -> Duration {
        Duration::from_micros(self.duration_micros)
    }
}

/// Run-wide counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSummary {
    /// How many files were scheduled.
    pub files: usize,
    /// Runnable tests that passed.
    pub passed: usize,
    /// Runnable tests that failed.
    pub failed: usize,
    /// Declarations excluded by `.skip`, `.only` or a filter.
    pub skipped: usize,
    /// Declarations marked `.todo`.
    pub todo: usize,
    /// Registration forms discovery recognised but cannot expand.
    pub unsupported_declarations: usize,
    /// Files that did not complete.
    pub failed_files: usize,
    /// Files ordered from a duration a previous run recorded.
    pub scheduled_warm: usize,
    /// Files ordered from their size, because nothing was recorded for them.
    pub scheduled_cold: usize,
    /// Total wall-clock time of the run, in microseconds.
    pub duration_micros: u64,
    /// Whether `--bail` stopped the run early.
    pub bailed: bool,
}

impl TestSummary {
    /// Total wall-clock time of the run.
    pub fn duration(&self) -> Duration {
        Duration::from_micros(self.duration_micros)
    }

    /// Whether the run is green.
    pub fn is_success(&self) -> bool {
        self.failed == 0 && self.failed_files == 0 && !self.bailed
    }
}

/// Native test execution report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRunReport {
    /// Every declaration discovered, in file order.
    pub plan: TestPlan,
    /// Per-file outcomes, in file order.
    pub files: Vec<FileReport>,
    /// Run-wide counts.
    pub summary: TestSummary,
}

impl TestRunReport {
    /// Return whether the run is successful.
    pub fn is_success(&self) -> bool {
        self.summary.is_success()
    }

    /// Every record in the run, in file order.
    pub fn records(&self) -> impl Iterator<Item = &TestRecord> {
        self.files.iter().flat_map(|file| file.records.iter())
    }

    /// Every failing record in the run, in file order.
    pub fn failures(&self) -> impl Iterator<Item = &TestRecord> {
        self.records().filter(|record| record.status.is_failed())
    }

    /// The `count` slowest files, slowest first, ties broken by path so the
    /// list is stable.
    pub fn slowest_files(&self, count: usize) -> Vec<&FileReport> {
        let mut files: Vec<&FileReport> = self.files.iter().collect();
        files.sort_by(|a, b| {
            b.duration_micros
                .cmp(&a.duration_micros)
                .then(a.file.cmp(&b.file))
        });
        files.truncate(count);
        files
    }

    /// The same report with every measured duration zeroed.
    ///
    /// Two runs of one suite differ only in how long they took; this is what
    /// lets a test assert that nothing *else* differs.
    pub fn without_timings(mut self) -> Self {
        self.summary.duration_micros = 0;
        for file in &mut self.files {
            file.duration_micros = 0;
        }
        self
    }
}
