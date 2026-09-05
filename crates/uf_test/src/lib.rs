#![deny(missing_docs)]
//! Test discovery, scheduling and execution for `uf test`.
//!
//! # Where the work happens
//!
//! `uf` cannot execute JavaScript, and a runner that pretends otherwise is a
//! runner that lies about what passed. So this crate owns everything a test
//! run needs *except* running the code: which files there are, what order they
//! go in, how many run at once, what bounds them, what a retry means, what an
//! edit invalidates, and what the report says. Executing a test body belongs
//! to the project's Capability JS Host — Node.js or Bun — which runs
//! `@uniflowed/test`'s worker and streams results back ([`host`]).
//!
//! That split is also the performance argument. Ordering a thousand files
//! longest-first from durations a previous run recorded, fanning them across
//! cores, and assembling a report are the parts that are faster in Rust; the
//! host does nothing but evaluate the code, one file at a time per process,
//! with no test framework of its own to load.
//!
//! # The pieces
//!
//! * [`discover_tests`] finds `describe` / `it` / `test` declarations without
//!   running anything, which is what `uf test --list` shows and what watch
//!   mode reasons about.
//! * [`schedule_files`] orders files longest-expected-first, from durations a
//!   previous run recorded in `.uf/test-timings.json` ([`timings`]) and from
//!   file size when nothing was recorded.
//! * [`TestRunner`] fans that schedule across [`Worker`] processes, bounds each
//!   file, retries what the policy says to retry, and reassembles a report that
//!   does not depend on the schedule.
//! * [`ImportGraph`] answers what one edit invalidates, and [`Watcher`] notices
//!   the edit, which is watch mode.
//!
//! # Bounds
//!
//! Everything the crate reads is untrusted: source files, import specifiers,
//! whatever a worker writes, and a timings file anything can write. Sources are
//! bounded in size and in declaration count, every file has a wall-clock
//! deadline the worker cannot talk its way out of, paths are validated before
//! they are joined onto the project root, and recorded durations are
//! range-checked before the scheduler believes them.
//!
//! ```no_run
//! use camino::Utf8PathBuf;
//! use uf_test::{HostCommand, HostKind, TestFile, TestRunner};
//!
//! let host = HostCommand::new(
//!     HostKind::Node,
//!     Utf8PathBuf::from("node"),
//!     Utf8PathBuf::from("node_modules/@uniflowed/test/worker.js"),
//!     Utf8PathBuf::from("."),
//! );
//! let files = vec![TestFile::new(
//!     "src/math.test.js",
//!     "/project/src/math.test.js",
//!     "it('adds', () => { expect(1 + 1).toBe(2); });",
//! )];
//!
//! let report = TestRunner::new().with_host(host).run(&files)?;
//! assert!(report.is_success());
//! # Ok::<(), uf_test::RunError>(())
//! ```

mod discovery;
mod filter;
mod graph;
mod host;
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
pub use crate::host::{FileOutcome, HostCommand, HostKind, SpawnError, Worker};
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
    AssertionFailure, FileReport, FileStatus, MAX_EXPRESSION_BYTES, MAX_OUTPUT_BYTES_PER_FILE,
    OutputChunk, OutputStream, TestRecord, TestRunReport, TestStatus, TestSummary,
};
pub use crate::retry_schedule::{Attempt, Decision, MAX_DELAY, Schedule};
pub use crate::runner::{
    LockedObserver, RunError, RunObserver, SilentObserver, TestFile, TestRunner, run_tests,
};
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
