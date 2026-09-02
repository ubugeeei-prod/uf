#![deny(missing_docs)]
//! Native test discovery, scheduling and execution for `uf test` and
//! `@uniflowed/test`.
//!
//! # What this crate does and does not execute
//!
//! There is **no JavaScript engine here**. A test body is decided by reading it:
//! [`assertion`] evaluates the handful of expression and matcher shapes that can
//! be settled from the source text, and everything else is reported by name as
//! unsupported. That distinction is the crate's central honesty guarantee — a
//! matcher `uf` cannot evaluate fails the run and is printed with its file, line
//! and expression, because a runner that quietly passes what it could not run is
//! worse than no runner at all.
//!
//! # The pieces
//!
//! * [`discover_tests`] finds `describe` / `it` / `test` declarations, their
//!   `.only` / `.skip` / `.todo` suffixes, and the forms it recognises but
//!   cannot expand ([`UnsupportedDeclaration`]).
//! * [`TestPlan::resolve`] turns byte ranges into nesting and applies `.only`
//!   precedence, in one pass.
//! * [`schedule_files`] orders files longest-expected-first, from durations a
//!   previous run recorded in `.uf/test-timings.json` ([`timings`]) and from
//!   file size when nothing was recorded.
//! * [`TestRunner`] fans the schedule across a rayon pool, bounds each file's
//!   work, and reassembles a report that does not depend on the schedule.
//! * [`ImportGraph`] answers what one edit invalidates, and [`Watcher`] notices
//!   the edit, which is watch mode.
//!
//! # Bounds
//!
//! Everything the crate reads is untrusted: source files, import specifiers, and
//! a timings file anything can write. Sources are bounded in size and in
//! declaration count, per-file execution is bounded in wall-clock time, paths
//! are validated before they are joined onto the project root, and recorded
//! durations are range-checked before the scheduler believes them.
//!
//! ```
//! use uf_test::{TestRunner, TestStatus};
//!
//! let report = TestRunner::new().run(&[(
//!     "src/math.test.js",
//!     "it('adds', () => { expect(1 + 1).toBe(2); });",
//! )]);
//!
//! assert!(report.is_success());
//! assert_eq!(report.summary.passed, 1);
//! assert!(matches!(
//!     report.files[0].records[0].status,
//!     TestStatus::Passed
//! ));
//! ```

mod assertion;
mod discovery;
mod execution;
mod filter;
mod graph;
mod options;
mod path;
mod plan;
mod report;
mod retry_schedule;
mod runner;
mod runner_plan;
mod scan;
mod schedule;
mod timings;
mod watch;

use thiserror::Error;

pub use crate::discovery::{MAX_CASES_PER_FILE, MAX_SOURCE_BYTES, discover_tests, merge_plans};
pub use crate::filter::{MAX_PATTERN_BYTES, PathPatternList, TestFilter};
pub use crate::graph::{ImportGraph, MAX_IMPORTS_PER_MODULE, MAX_MODULES, MODULE_EXTENSIONS};
pub use crate::options::{
    Bail, Concurrency, DEFAULT_FILE_TIMEOUT, DEFAULT_MAX_ASSERTIONS_PER_TEST, MAX_ATTEMPTS,
    MAX_FILE_TIMEOUT, MAX_RETRY_DELAY, MIN_FILE_TIMEOUT, RetryPolicy, RunOptions,
};
pub use crate::path::{MAX_RELATIVE_PATH_BYTES, is_safe_relative, normalize_relative};
pub use crate::plan::{
    AncestorList, NAME_SEPARATOR, PlanResolution, Selection, SkipReason, TestCase, TestKind,
    TestModifier, TestPlan, UnsupportedDeclaration,
};
pub use crate::report::{
    AssertionFailure, FileReport, FileStatus, MAX_EXPRESSION_BYTES, TestRecord, TestRunReport,
    TestStatus, TestSummary, UnsupportedAssertion, UnsupportedReason,
};
pub use crate::retry_schedule::{Attempt, Decision, MAX_DELAY, Schedule};
pub use crate::runner::{LockedObserver, RunObserver, SilentObserver, TestRunner, run_tests};
pub use crate::runner_plan::{
    NativeTestRunnerPlan, TestHost, TestHostList, TestImportList, TestPerformanceTarget,
    TestRuntime, TestScheduler,
};
pub use crate::schedule::{
    COLD_NANOS_PER_BYTE, ScheduleBasis, ScheduleEntry, cold_weight_micros, makespan_micros,
    schedule_files,
};
pub use crate::timings::{
    CACHE_DIRECTORY, MAX_TIMING_ENTRIES, MAX_TIMING_MICROS, MAX_TIMINGS_BYTES, TIMINGS_FILE_NAME,
    TIMINGS_VERSION, TestTimings, TimingsAudit, TimingsError, load_timings, save_timings,
    timings_path,
};
pub use crate::watch::{
    ChangeSet, DEFAULT_POLL_INTERVAL, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL, WatchOptions, Watcher,
    next_poll_at,
};

/// Errors emitted by native test execution.
///
/// A misbehaving *file* is not an error — it is a [`FileStatus`], so the run
/// keeps going and names it. These are the failures that stop a run before it
/// can produce a report at all.
#[derive(Debug, Error)]
pub enum TestError {
    /// The JavaScript execution backend has not been wired yet.
    #[error("native JavaScript execution backend is not enabled yet")]
    RuntimeUnavailable,
    /// Recorded timings could not be read or written.
    #[error(transparent)]
    Timings(#[from] TimingsError),
}

#[cfg(test)]
mod tests;
