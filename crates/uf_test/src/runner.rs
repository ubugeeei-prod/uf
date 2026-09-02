//! The driver: schedule the files, fan them out, put the report back together.
//!
//! # Determinism
//!
//! The schedule decides *when* a file runs and never *what* it produces. Every
//! file is executed from its own source with no shared mutable state, results
//! are re-sorted by path before the report is assembled, and every list inside
//! the report has a total order. A run on one thread and a run on sixteen
//! therefore produce equal reports, which is asserted directly in the tests.
//!
//! The single exception is `--bail`, which by construction depends on which
//! files happened to finish first. Bailing marks the files it skipped
//! [`FileStatus::NotRun`] and sets [`TestSummary::bailed`], so the report says
//! plainly that it is not a complete picture.
//!
//! # Threads
//!
//! Nothing here creates a Flow parser, so unlike `uf_lint` this pool needs no
//! `uf_flow::prepare_thread` broadcast: discovery and the assertion subset are
//! byte scans over `&str`, with no per-thread setup and no stack budget to
//! blow. If a real JavaScript engine ever lands behind [`crate::assertion`],
//! that broadcast has to be added here before any parsing moves off the calling
//! thread — see the comment in `uf_lint::lint_sources` for why.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use rayon::prelude::*;

use crate::discovery::merge_plans;
use crate::execution::execute_file;
use crate::filter::TestFilter;
use crate::options::{Bail, Concurrency, RunOptions};
use crate::plan::TestPlan;
use crate::report::{
    FileReport, FileStatus, TestRunReport, TestStatus, TestSummary, UnsupportedAssertion,
};
use crate::schedule::{ScheduleBasis, ScheduleEntry, schedule_files};
use crate::timings::TestTimings;

/// Notified as each file finishes, so a caller can draw a progress line.
///
/// Called from worker threads, hence [`Sync`]; implementations are expected to
/// be cheap and to do their own locking.
pub trait RunObserver: Sync {
    /// One file finished. `completed` counts finished files including this one.
    fn file_finished(&self, completed: usize, total: usize, report: &FileReport);
}

/// An observer that does nothing, for runs nobody is watching.
#[derive(Debug, Clone, Copy, Default)]
pub struct SilentObserver;

impl RunObserver for SilentObserver {
    fn file_finished(&self, _completed: usize, _total: usize, _report: &FileReport) {}
}

/// A configured test run.
///
/// The thread pool for an explicit worker count is built once and reused, not
/// rebuilt per run: spawning eight threads costs several milliseconds, which is
/// a third of a run over a thousand files, and in watch mode it would be paid
/// again on every keystroke.
#[derive(Debug, Default)]
pub struct TestRunner {
    options: RunOptions,
    filter: TestFilter,
    timings: TestTimings,
    pool: OnceLock<Option<rayon::ThreadPool>>,
}

impl TestRunner {
    /// A runner with default options, no filter, and no recorded timings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the run options.
    pub fn with_options(mut self, options: RunOptions) -> Self {
        self.options = options;
        // The cached pool is sized from the options it was built for.
        self.pool = OnceLock::new();
        self
    }

    /// Set the filter applied to paths and test names.
    pub fn with_filter(mut self, filter: TestFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Seed the scheduler with durations recorded by a previous run.
    pub fn with_timings(mut self, timings: TestTimings) -> Self {
        self.timings = timings;
        self
    }

    /// The options this runner will use.
    pub fn options(&self) -> &RunOptions {
        &self.options
    }

    /// The filter this runner will apply.
    pub fn filter(&self) -> &TestFilter {
        &self.filter
    }

    /// The order `sources` would run in, after path filtering.
    pub fn schedule<'a>(&self, sources: &'a [(&'a str, &'a str)]) -> Vec<ScheduleEntry> {
        let selected = self.select(sources);
        schedule_files(&selected, &self.timings)
    }

    /// Run every file, reporting nothing as it goes.
    pub fn run(&self, sources: &[(&str, &str)]) -> TestRunReport {
        self.run_observed(sources, &SilentObserver)
    }

    /// Run every file, notifying `observer` as each one finishes.
    pub fn run_observed(
        &self,
        sources: &[(&str, &str)],
        observer: &dyn RunObserver,
    ) -> TestRunReport {
        let started = Instant::now();
        let selected = self.select(sources);
        let schedule = schedule_files(&selected, &self.timings);

        let state = RunState {
            failures: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            total: schedule.len(),
        };
        let outcomes = if self.options.concurrency.is_serial() {
            schedule
                .iter()
                .map(|entry| self.run_one(&selected, entry, &state, observer))
                .collect::<Vec<_>>()
        } else {
            self.run_parallel(&selected, &schedule, &state, observer)
        };

        assemble(outcomes, &schedule, started, self.options.bail, &state)
    }

    /// Files that survive the path filter, in the caller's order.
    fn select<'a>(&self, sources: &'a [(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        sources
            .iter()
            .filter(|(file, _)| self.filter.matches_path(file))
            .copied()
            .collect()
    }

    fn run_parallel(
        &self,
        selected: &[(&str, &str)],
        schedule: &[ScheduleEntry],
        state: &RunState,
        observer: &dyn RunObserver,
    ) -> Vec<(TestPlan, FileReport)> {
        // `with_min_len(1)` is what makes the schedule mean anything: rayon
        // otherwise splits the slice into contiguous halves, which hands one
        // worker a block of expensive files and another a block of cheap ones.
        // Splitting to single items lets every worker take the next unstarted
        // file, which is the LPT hand-out `schedule_files` assumes.
        let run = || {
            schedule
                .par_iter()
                .with_min_len(1)
                .map(|entry| self.run_one(selected, entry, state, observer))
                .collect::<Vec<_>>()
        };

        match self.pool() {
            Some(pool) => pool.install(run),
            // Either the caller asked for the global pool, or building a
            // private one failed. A pool that will not build is not a reason to
            // lose the run; the global pool is a correct, if wider, place for it.
            None => run(),
        }
    }

    /// The private pool for an explicit worker count, built at most once.
    fn pool(&self) -> Option<&rayon::ThreadPool> {
        self.pool
            .get_or_init(|| match self.options.concurrency {
                Concurrency::Fixed(threads) => rayon::ThreadPoolBuilder::new()
                    .num_threads(threads.get())
                    .thread_name(|index| format!("uf-test-{index}"))
                    .build()
                    .ok(),
                Concurrency::Auto | Concurrency::Serial => None,
            })
            .as_ref()
    }

    fn run_one(
        &self,
        selected: &[(&str, &str)],
        entry: &ScheduleEntry,
        state: &RunState,
        observer: &dyn RunObserver,
    ) -> (TestPlan, FileReport) {
        let Some((file, source)) = selected.get(entry.index).copied() else {
            return (TestPlan::default(), not_run(entry));
        };

        if self
            .options
            .bail
            .is_reached(state.failures.load(Ordering::Relaxed))
        {
            return (TestPlan::default(), not_run(entry));
        }

        let (plan, report) = execute_file(file, source, &self.options, &self.filter);
        let failed = report
            .records
            .iter()
            .filter(|record| record.status.is_failed())
            .count();
        if failed > 0 {
            state.failures.fetch_add(failed, Ordering::Relaxed);
        }
        let completed = state.completed.fetch_add(1, Ordering::Relaxed) + 1;
        observer.file_finished(completed, state.total, &report);
        (plan, report)
    }
}

fn not_run(entry: &ScheduleEntry) -> FileReport {
    FileReport {
        file: entry.file.to_string(),
        status: FileStatus::NotRun,
        duration_micros: 0,
        records: Vec::new(),
    }
}

struct RunState {
    failures: AtomicUsize,
    completed: AtomicUsize,
    total: usize,
}

/// Put the per-file outcomes back into one deterministic report.
fn assemble(
    outcomes: Vec<(TestPlan, FileReport)>,
    schedule: &[ScheduleEntry],
    started: Instant,
    bail: Bail,
    state: &RunState,
) -> TestRunReport {
    let (plans, mut files): (Vec<TestPlan>, Vec<FileReport>) = outcomes.into_iter().unzip();
    files.sort_by(|a, b| a.file.cmp(&b.file));

    let plan = merge_plans(plans);
    let mut summary = summarize(&plan, &files);
    summary.scheduled_warm = schedule
        .iter()
        .filter(|entry| entry.basis == ScheduleBasis::Recorded)
        .count();
    summary.scheduled_cold = schedule.len() - summary.scheduled_warm;
    summary.bailed = bail.is_reached(state.failures.load(Ordering::Relaxed));
    summary.duration_micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);

    TestRunReport {
        plan,
        files,
        summary,
    }
}

fn summarize(plan: &TestPlan, files: &[FileReport]) -> TestSummary {
    let mut summary = TestSummary {
        files: files.len(),
        unsupported_declarations: plan.unsupported.len(),
        ..TestSummary::default()
    };

    for file in files {
        if file.status.is_fatal() {
            summary.failed_files += 1;
        }
        for record in &file.records {
            match &record.status {
                TestStatus::Passed => summary.passed += 1,
                TestStatus::Failed { unsupported, .. } => {
                    summary.failed += 1;
                    summary.unsupported_assertions += count_unsupported(unsupported);
                }
                TestStatus::Unsupported { assertions } => {
                    summary.unsupported += 1;
                    summary.unsupported_assertions += count_unsupported(assertions);
                }
                TestStatus::Skipped { .. } => summary.skipped += 1,
                TestStatus::Todo => summary.todo += 1,
            }
        }
    }

    summary
}

fn count_unsupported(assertions: &[UnsupportedAssertion]) -> usize {
    assertions.len()
}

/// Execute the native source-level test subset with default options.
///
/// The one-line entry point: discovery, scheduling and execution over the given
/// sources, on the global thread pool.
pub fn run_tests<'a>(sources: impl IntoIterator<Item = (&'a str, &'a str)>) -> TestRunReport {
    let sources: Vec<(&str, &str)> = sources.into_iter().collect();
    TestRunner::new().run(&sources)
}

/// An observer that funnels every completion into one locked callback.
///
/// The lock is held only for the callback, and callers use it for a rate-limited
/// progress line, so contention is a non-issue in practice.
pub struct LockedObserver<F: FnMut(usize, usize, &FileReport) + Send> {
    inner: Mutex<F>,
}

impl<F: FnMut(usize, usize, &FileReport) + Send> LockedObserver<F> {
    /// Wrap `callback` so it can be called from worker threads.
    pub fn new(callback: F) -> Self {
        Self {
            inner: Mutex::new(callback),
        }
    }
}

impl<F: FnMut(usize, usize, &FileReport) + Send> RunObserver for LockedObserver<F> {
    fn file_finished(&self, completed: usize, total: usize, report: &FileReport) {
        if let Ok(mut callback) = self.inner.lock() {
            callback(completed, total, report);
        }
    }
}
