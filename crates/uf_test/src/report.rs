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

/// Why an assertion could not be evaluated by the native subset.
///
/// Executing arbitrary Flow needs a JavaScript engine, which `uf` does not have
/// yet. Until it does, anything outside the subset is named here rather than
/// counted as a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum UnsupportedReason {
    /// The matcher is not implemented natively, e.g. `toContain`.
    Matcher {
        /// Matcher name as written, without the leading dot.
        matcher: String,
    },
    /// The matcher is implemented, but its operands are not constant-evaluable.
    Expression,
    /// The `expect(...)` call is not balanced, so nothing can be read from it.
    Malformed,
}

impl UnsupportedReason {
    /// A one-line explanation for a terminal report.
    pub fn describe(&self) -> String {
        match self {
            Self::Matcher { matcher } => {
                format!("matcher `{matcher}` needs a JavaScript engine uf does not have yet")
            }
            Self::Expression => {
                "operands are not constant-evaluable by the native assertion subset".to_string()
            }
            Self::Malformed => "the expect(...) call is not balanced".to_string(),
        }
    }
}

/// One assertion the native subset refused to decide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedAssertion {
    /// The assertion source, truncated to [`MAX_EXPRESSION_BYTES`].
    pub expression: String,
    /// Why it could not be evaluated.
    pub reason: UnsupportedReason,
    /// One-based line of the `expect` call.
    pub line: usize,
    /// One-based column of the `expect` call.
    pub column: usize,
    /// Byte length of the assertion, for a code frame caret.
    pub span: usize,
}

/// Longest source excerpt copied into a report.
///
/// A generated file can contain a megabyte-long expression; a report is not the
/// place to reproduce it.
pub const MAX_EXPRESSION_BYTES: usize = 200;

/// One assertion that did not hold, or one thrown `Error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionFailure {
    /// The failure message shown to the developer.
    pub message: String,
    /// One-based line of the failing assertion.
    pub line: usize,
    /// One-based column of the failing assertion.
    pub column: usize,
    /// Byte length of the failing expression, for a code frame caret.
    pub span: usize,
}

/// How one declaration ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "status")]
pub enum TestStatus {
    /// Every assertion in the native subset held.
    Passed,
    /// At least one assertion did not hold, or the body threw.
    Failed {
        /// Why it failed, in source order.
        failures: Vec<AssertionFailure>,
        /// Assertions in the same body that could not be evaluated.
        unsupported: Vec<UnsupportedAssertion>,
    },
    /// Nothing failed, but the body uses something the subset cannot evaluate,
    /// so the case cannot be claimed as a pass.
    Unsupported {
        /// Each assertion that could not be evaluated, named.
        assertions: Vec<UnsupportedAssertion>,
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

    /// Assertions that could not be evaluated, in either state that carries
    /// them.
    pub fn unsupported_assertions(&self) -> &[UnsupportedAssertion] {
        match self {
            Self::Failed { unsupported, .. } => unsupported,
            Self::Unsupported { assertions } => assertions,
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
    /// The file is larger than the runner will scan.
    TooLarge {
        /// The file's size in bytes.
        bytes: usize,
        /// The accepted limit in bytes.
        limit: usize,
    },
    /// Executing the file panicked. The file is named and the run continues.
    Panicked {
        /// The panic payload, when it was a string.
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
            Self::TimedOut { .. } | Self::TooLarge { .. } | Self::Panicked { .. }
        )
    }

    /// A one-line explanation for a terminal report.
    pub fn describe(&self) -> String {
        match self {
            Self::Completed => "completed".to_string(),
            Self::TimedOut { budget_micros } => {
                format!("exceeded the per-file budget of {budget_micros}us")
            }
            Self::TooLarge { bytes, limit } => {
                format!("is {bytes} bytes, past the {limit} byte limit")
            }
            Self::Panicked { message } => format!("panicked: {message}"),
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
    /// Tests that could not be decided by the native assertion subset.
    pub unsupported: usize,
    /// Individual assertions that could not be evaluated.
    pub unsupported_assertions: usize,
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
        self.failed == 0
            && self.unsupported == 0
            && self.unsupported_assertions == 0
            && self.unsupported_declarations == 0
            && self.failed_files == 0
            && !self.bailed
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
