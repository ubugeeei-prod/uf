//! The driver: schedule the files, fan them across worker processes, put the
//! report back together.
//!
//! # What runs where
//!
//! `uf` decides *which* files run and *in what order*, bounds them, and
//! assembles the report; the host executes the JavaScript. The split is the
//! whole design: scheduling a thousand files longest-first from recorded
//! durations, and rendering what came back, are the parts that are faster in
//! Rust, and executing a test body is the part Rust cannot do at all.
//!
//! # Determinism
//!
//! The schedule decides *when* a file runs and never *what* it produces. Each
//! file runs alone in its worker with no shared state, results are re-sorted
//! by path before the report is assembled, and every list inside the report
//! has a total order. A run on one worker and a run on sixteen therefore
//! produce equal reports, which the tests assert directly.
//!
//! The single exception is `--bail`, which by construction depends on which
//! files happened to finish first. Bailing marks the files it skipped
//! [`FileStatus::NotRun`] and sets [`TestSummary::bailed`], so the report says
//! plainly that it is not a complete picture.
//!
//! # Retries
//!
//! A retry re-runs the *file* with a filter naming the failing case, because
//! a case cannot be re-entered without re-importing the module it lives in.
//! That is slower than re-calling a closure would be, and it is the only
//! honest option: a retried test must see the same module state a first run
//! would.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use camino::Utf8PathBuf;

use crate::discovery::merge_plans;
use crate::filter::TestFilter;
use crate::host::{FileOutcome, HostCommand, SpawnError, Worker};
use crate::options::{Bail, Concurrency, RunOptions};
use crate::report::{FileReport, FileStatus, TestRunReport, TestStatus, TestSummary};
use crate::schedule::{ScheduleEntry, schedule_files};
use crate::timings::TestTimings;

/// One file a run will execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFile {
    /// Path as it appears in the report, relative to the project root.
    pub relative: String,
    /// Absolute path the worker imports.
    pub absolute: Utf8PathBuf,
    /// The file's source, for discovery and for rendering code frames.
    pub source: String,
}

impl TestFile {
    /// A file at `absolute`, reported as `relative`.
    pub fn new(
        relative: impl Into<String>,
        absolute: impl Into<Utf8PathBuf>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            relative: relative.into(),
            absolute: absolute.into(),
            source: source.into(),
        }
    }
}

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
#[derive(Debug, Default)]
pub struct TestRunner {
    options: RunOptions,
    filter: TestFilter,
    timings: TestTimings,
    host: Option<HostCommand>,
}

/// Why a run could not start.
///
/// Everything that can go wrong with *one file* is a [`FileStatus`]; this is
/// only for what would go wrong with all of them.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// No JavaScript host is configured, so nothing can execute.
    #[error(
        "no JavaScript host is configured for `uf test`; install Node.js or Bun, or name an \
         installed host in `app.runtime.capabilityJsHost.default`"
    )]
    NoHost,
    /// The host could not be started, which every file would hit.
    #[error("{0}")]
    Spawn(#[from] SpawnError),
}

impl TestRunner {
    /// A runner with default options, no filter, and no recorded timings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the run options.
    pub fn with_options(mut self, options: RunOptions) -> Self {
        self.options = options;
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

    /// Set the host the workers run on.
    pub fn with_host(mut self, host: HostCommand) -> Self {
        self.host = Some(host);
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

    /// The order `files` would run in, after path filtering.
    pub fn schedule(&self, files: &[TestFile]) -> Vec<ScheduleEntry> {
        let selected = self.select(files);
        schedule_files(&sources_of(&selected), &self.timings)
    }

    /// Run every file, reporting nothing as it goes.
    ///
    /// # Errors
    ///
    /// [`RunError`] when no host is configured or the host will not start.
    pub fn run(&self, files: &[TestFile]) -> Result<TestRunReport, RunError> {
        self.run_observed(files, &SilentObserver)
    }

    /// Run every file, notifying `observer` as each one finishes.
    ///
    /// # Errors
    ///
    /// [`RunError`] when no host is configured or the host will not start.
    pub fn run_observed(
        &self,
        files: &[TestFile],
        observer: &dyn RunObserver,
    ) -> Result<TestRunReport, RunError> {
        let host = self.host.as_ref().ok_or(RunError::NoHost)?;
        let started = Instant::now();
        let selected = self.select(files);
        let schedule = schedule_files(&sources_of(&selected), &self.timings);

        let state = RunState {
            next: AtomicUsize::new(0),
            failures: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            total: schedule.len(),
            outcomes: Mutex::new(vec![None; schedule.len()]),
        };

        let workers = self.worker_count(schedule.len());
        // A host that will not start is a run that cannot happen. Finding that
        // out once, here, turns it into one clear error instead of `workers`
        // identical file failures.
        Worker::spawn(host)?.kill();

        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            for _ in 0..workers {
                handles
                    .push(scope.spawn(|| self.drive(host, &selected, &schedule, &state, observer)));
            }
            for handle in handles {
                let _ = handle.join();
            }
        });

        let outcomes = state
            .outcomes
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(assemble(
            outcomes,
            &selected,
            &schedule,
            started,
            self.options.bail,
            state.failures.load(Ordering::Relaxed),
        ))
    }

    /// How many workers to start: never more than there are files, and never
    /// more than the configured concurrency.
    fn worker_count(&self, files: usize) -> usize {
        let requested = match self.options.concurrency {
            Concurrency::Serial => 1,
            Concurrency::Fixed(count) => count.get(),
            Concurrency::Auto => {
                std::thread::available_parallelism().map_or(1, |count| count.get())
            }
        };
        requested.min(files.max(1)).max(1)
    }

    /// Files that survive the path filter, in the caller's order.
    fn select<'a>(&self, files: &'a [TestFile]) -> Vec<&'a TestFile> {
        files
            .iter()
            .filter(|file| self.filter.matches_path(&file.relative))
            .collect()
    }

    /// One worker: take the next file until there are none, or the run bails.
    fn drive(
        &self,
        host: &HostCommand,
        selected: &[&TestFile],
        schedule: &[ScheduleEntry],
        state: &RunState,
        observer: &dyn RunObserver,
    ) {
        let mut worker: Option<Worker> = None;
        loop {
            let at = state.next.fetch_add(1, Ordering::SeqCst);
            if at >= schedule.len() {
                return;
            }
            if self.bailed(state) {
                // Leave the slot empty; `assemble` reports it as not run.
                continue;
            }
            let Some(file) = selected
                .iter()
                .find(|file| file.relative == schedule[at].file)
            else {
                continue;
            };

            if worker.is_none() {
                worker = match Worker::spawn(host) {
                    Ok(worker) => Some(worker),
                    Err(error) => {
                        state.record(
                            at,
                            FileReport {
                                file: file.relative.clone(),
                                status: FileStatus::HostFailed {
                                    message: error.message,
                                },
                                duration_micros: 0,
                                records: Vec::new(),
                                output: Vec::new(),
                            },
                            observer,
                        );
                        continue;
                    }
                };
            }

            let started = Instant::now();
            let mut outcome = worker
                .as_mut()
                .map(|worker| self.run_one(worker, file))
                .unwrap_or(FileOutcome {
                    status: FileStatus::HostFailed {
                        message: String::from("no worker"),
                    },
                    records: Vec::new(),
                    output: Vec::new(),
                });

            // A file that timed out or lost its host killed the worker; the
            // next file needs a fresh one.
            if !matches!(
                outcome.status,
                FileStatus::Completed | FileStatus::LoadFailed { .. }
            ) {
                worker = None;
            } else if self.options.retry.max_attempts() > 1 {
                self.retry_failures(host, file, &mut outcome, &mut worker);
            }

            let report = FileReport {
                file: file.relative.clone(),
                status: outcome.status,
                duration_micros: u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
                records: outcome.records,
                output: outcome.output,
            };
            let failed = report
                .records
                .iter()
                .filter(|record| record.status.is_failed())
                .count()
                + usize::from(report.status.is_fatal());
            if failed > 0 {
                state.failures.fetch_add(failed, Ordering::SeqCst);
            }
            state.record(at, report, observer);
        }
    }

    /// Run one file once.
    fn run_one(&self, worker: &mut Worker, file: &TestFile) -> FileOutcome {
        worker.run_file(
            file.absolute.as_str(),
            &file.relative,
            self.filter.name_pattern(),
            self.options.effective_file_timeout(),
            self.options.effective_file_timeout() * MAX_CASES_PER_FILE_BUDGET,
        )
    }

    /// Re-run each failing case, up to the configured number of attempts.
    ///
    /// The file is re-imported with a filter naming exactly one case, so a
    /// retry sees the module state a first run would rather than whatever the
    /// previous attempt left behind.
    fn retry_failures(
        &self,
        host: &HostCommand,
        file: &TestFile,
        outcome: &mut FileOutcome,
        worker: &mut Option<Worker>,
    ) {
        let attempts = self.options.retry.max_attempts();
        let failing: Vec<String> = outcome
            .records
            .iter()
            .filter(|record| record.status.is_failed())
            .map(|record| record.name.clone())
            .collect();

        for name in failing {
            for attempt in 2..=attempts {
                if worker.is_none() {
                    match Worker::spawn(host) {
                        Ok(fresh) => *worker = Some(fresh),
                        Err(_) => return,
                    }
                }
                let Some(active) = worker.as_mut() else {
                    return;
                };
                let retried = active.run_file(
                    file.absolute.as_str(),
                    &file.relative,
                    Some(&name),
                    self.options.effective_file_timeout(),
                    self.options.effective_file_timeout() * MAX_CASES_PER_FILE_BUDGET,
                );
                if !matches!(retried.status, FileStatus::Completed) {
                    *worker = None;
                    return;
                }
                let Some(fresh) = retried.records.into_iter().find(|record| {
                    record.name == name && !matches!(record.status, TestStatus::Skipped { .. })
                }) else {
                    return;
                };
                let passed = fresh.status.is_passed();
                if let Some(slot) = outcome
                    .records
                    .iter_mut()
                    .find(|record| record.name == name)
                {
                    slot.status = fresh.status;
                    // The output goes with the status: what a reader is shown
                    // must have come from the attempt they are being shown the
                    // result of.
                    slot.output = fresh.output;
                    slot.attempts = attempt;
                }
                if passed {
                    break;
                }
            }
        }
    }

    fn bailed(&self, state: &RunState) -> bool {
        match self.options.bail {
            Bail::Off => false,
            Bail::After(limit) => state.failures.load(Ordering::SeqCst) >= limit.get(),
        }
    }
}

/// How much longer than one case's budget a whole file may take.
///
/// A file is many cases, and the per-case budget is what bounds a hanging
/// test; this is the backstop for a worker that stops answering entirely, so
/// it is deliberately generous.
const MAX_CASES_PER_FILE_BUDGET: u32 = 60;

/// Shared state across the pool.
#[derive(Debug)]
struct RunState {
    next: AtomicUsize,
    failures: AtomicUsize,
    completed: AtomicUsize,
    total: usize,
    outcomes: Mutex<Vec<Option<FileReport>>>,
}

impl RunState {
    fn record(&self, at: usize, report: FileReport, observer: &dyn RunObserver) {
        let completed = self.completed.fetch_add(1, Ordering::SeqCst) + 1;
        observer.file_finished(completed, self.total, &report);
        if let Ok(mut outcomes) = self.outcomes.lock() {
            outcomes[at] = Some(report);
        }
    }
}

fn sources_of<'a>(files: &[&'a TestFile]) -> Vec<(&'a str, &'a str)> {
    files
        .iter()
        .map(|file| (file.relative.as_str(), file.source.as_str()))
        .collect()
}

/// Put the report together from what the workers returned.
///
/// Files are sorted by path, not by the order they finished, so the report
/// does not depend on the schedule.
fn assemble(
    outcomes: Vec<Option<FileReport>>,
    selected: &[&TestFile],
    schedule: &[ScheduleEntry],
    started: Instant,
    bail: Bail,
    failures: usize,
) -> TestRunReport {
    let mut files: Vec<FileReport> = Vec::with_capacity(outcomes.len());
    for (at, outcome) in outcomes.into_iter().enumerate() {
        files.push(outcome.unwrap_or_else(|| FileReport {
            file: schedule[at].file.to_string(),
            status: FileStatus::NotRun,
            duration_micros: 0,
            records: Vec::new(),
            output: Vec::new(),
        }));
    }
    files.sort_by(|a, b| a.file.cmp(&b.file));

    let plan = merge_plans(
        selected
            .iter()
            .map(|file| crate::discovery::discover_tests(&file.relative, &file.source)),
    );

    let mut summary = TestSummary {
        files: files.len(),
        unsupported_declarations: plan.unsupported.len(),
        scheduled_warm: schedule
            .iter()
            .filter(|entry| matches!(entry.basis, crate::schedule::ScheduleBasis::Recorded))
            .count(),
        scheduled_cold: schedule
            .iter()
            .filter(|entry| !matches!(entry.basis, crate::schedule::ScheduleBasis::Recorded))
            .count(),
        duration_micros: u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        bailed: matches!(bail, Bail::After(limit) if failures >= limit.get()),
        ..TestSummary::default()
    };
    for file in &files {
        if file.status.is_fatal() {
            summary.failed_files += 1;
        }
        for record in &file.records {
            match &record.status {
                TestStatus::Passed => summary.passed += 1,
                TestStatus::Failed { .. } => summary.failed += 1,
                TestStatus::Skipped { .. } => summary.skipped += 1,
                TestStatus::Todo => summary.todo += 1,
            }
        }
    }

    TestRunReport {
        plan,
        files,
        summary,
    }
}

/// An observer that serialises calls to a closure.
///
/// Progress is drawn from several workers at once and a terminal is not
/// re-entrant, so the closure is behind a lock rather than every caller
/// remembering to take one.
pub struct LockedObserver<F> {
    inner: Mutex<F>,
}

impl<F> LockedObserver<F>
where
    F: FnMut(usize, usize, &FileReport) + Send,
{
    /// Wrap `body` so it is called from one thread at a time.
    pub fn new(body: F) -> Self {
        Self {
            inner: Mutex::new(body),
        }
    }
}

impl<F> RunObserver for LockedObserver<F>
where
    F: FnMut(usize, usize, &FileReport) + Send,
{
    fn file_finished(&self, completed: usize, total: usize, report: &FileReport) {
        if let Ok(mut body) = self.inner.lock() {
            body(completed, total, report);
        }
    }
}

/// Run `files` with default options on `host`.
///
/// # Errors
///
/// [`RunError`] when the host will not start.
pub fn run_tests(files: &[TestFile], host: HostCommand) -> Result<TestRunReport, RunError> {
    TestRunner::new().with_host(host).run(files)
}
