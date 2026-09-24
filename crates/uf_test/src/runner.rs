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

use std::borrow::Cow;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;

use crate::discovery::merge_plans;
use crate::filter::TestFilter;
use crate::host::{FileOutcome, HostCommand, SpawnError, Worker};
use crate::options::{Bail, Concurrency, RunOptions};
use crate::plan::{KnownSite, TestPlan};
use crate::report::{FileReport, FileStatus, TestRunReport, TestStatus, TestSummary};
use crate::schedule::{ScheduleEntry, auto_workers, schedule_files};
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

/// One file whose discovery plan was already computed by the caller.
///
/// `uf test` has to know which project files are tests before it schedules the
/// run. Carrying the plan into the runner lets that discovery pass serve the
/// scheduler, the registered-nothing check and the final report, instead of
/// scanning the same source again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTestFile {
    /// The file to run.
    pub file: TestFile,
    /// What discovery found in that file.
    pub plan: TestPlan,
}

impl PlannedTestFile {
    /// Pair a test file with its discovery plan.
    pub fn new(file: TestFile, plan: TestPlan) -> Self {
        Self { file, plan }
    }
}

/// One selected file, and what discovery read out of it.
///
/// The plan travels with the file because two decisions need it after the
/// worker has answered: whether the file registered the tests it declared
/// (see [`FileStatus::RegisteredNothing`]), and what the report's own plan is.
#[derive(Debug)]
struct SelectedFile<'a> {
    file: &'a TestFile,
    plan: Cow<'a, TestPlan>,
}

/// Notified as the run starts, as each file starts, and as each file
/// finishes, so a caller can report a file the moment it is done rather than
/// when the whole run is.
///
/// Called from worker threads, hence [`Sync`]; implementations are expected to
/// be cheap and to do their own locking. Only [`RunObserver::file_finished`]
/// is required: the other two default to doing nothing, because a caller that
/// only draws results has no use for them.
pub trait RunObserver: Sync {
    /// The pool is about to start: `files` will run on `workers` processes.
    ///
    /// Not called for a run with nothing to run, and not called when the host
    /// would not start, which is a [`RunError`] instead.
    fn run_started(&self, _files: usize, _workers: usize) {}

    /// A worker has been handed `file` and is about to run it.
    ///
    /// Not called for a file the run bailed before, which never starts, nor
    /// again for a retry, which is part of the file's one run.
    fn file_started(&self, _file: &str) {}

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

    /// Run already-discovered files, reporting nothing as it goes.
    ///
    /// # Errors
    ///
    /// [`RunError`] when no host is configured or the host will not start.
    pub fn run_planned(&self, files: &[PlannedTestFile]) -> Result<TestRunReport, RunError> {
        self.run_planned_observed(files, &SilentObserver)
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
        let started = Instant::now();
        let selected = self.select(files);
        self.run_selected(started, selected, observer)
    }

    /// Run already-discovered files, notifying `observer` as each one finishes.
    ///
    /// # Errors
    ///
    /// [`RunError`] when no host is configured or the host will not start.
    pub fn run_planned_observed(
        &self,
        files: &[PlannedTestFile],
        observer: &dyn RunObserver,
    ) -> Result<TestRunReport, RunError> {
        let started = Instant::now();
        let selected = self.select_planned(files);
        self.run_selected(started, selected, observer)
    }

    fn run_selected<'a>(
        &self,
        started: Instant,
        selected: Vec<SelectedFile<'a>>,
        observer: &dyn RunObserver,
    ) -> Result<TestRunReport, RunError> {
        let schedule = schedule_files(&sources_of(&selected), &self.timings);
        if schedule.is_empty() {
            return Ok(assemble(
                Vec::new(),
                selected,
                &schedule,
                started,
                self.options.bail,
                0,
            ));
        }

        let host = self.host.as_ref().ok_or(RunError::NoHost)?;

        let workers = self.worker_count(&schedule);
        // A host that will not start is a run that cannot happen. Finding that
        // out once, here, turns it into one clear error instead of `workers`
        // identical file failures.
        //
        // The process started to find out is the first worker, handed to the
        // first thread that wants one. It used to be a probe killed on the
        // spot, which was a whole host process started and thrown away on
        // every run, before any worker that did work had been started.
        let first = Worker::spawn(host)?;
        observer.run_started(schedule.len(), workers);

        let state = RunState {
            next: AtomicUsize::new(0),
            failures: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
            total: schedule.len(),
            outcomes: Mutex::new(vec![None; schedule.len()]),
            first: Mutex::new(Some(first)),
            start_ups: Mutex::new(Vec::with_capacity(workers)),
        };

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

        let mut start_ups = state
            .start_ups
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let outcomes = state
            .outcomes
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut report = assemble(
            outcomes,
            selected,
            &schedule,
            started,
            self.options.bail,
            state.failures.load(Ordering::Relaxed),
        );
        report.summary.workers = workers;
        report.summary.worker_start_micros = median(&mut start_ups);
        Ok(report)
    }

    /// How many workers to start: never more than there are files, never more
    /// than the configured concurrency, and — when the concurrency was left to
    /// `uf` — no more than the recorded durations can keep busy for longer than
    /// a worker takes to start. See [`crate::auto_workers`].
    ///
    /// `-j` is taken as asked. A person who wrote a number has already decided
    /// what the machine can afford, and a run that second-guessed it would be
    /// impossible to reason about when comparing two numbers.
    fn worker_count(&self, schedule: &[ScheduleEntry]) -> usize {
        let files = schedule.len().max(1);
        match self.options.concurrency {
            Concurrency::Serial => 1,
            Concurrency::Fixed(count) => count.get().min(files),
            Concurrency::Auto => auto_workers(
                schedule,
                self.timings.worker_start_micros(),
                std::thread::available_parallelism().map_or(1, |count| count.get()),
            ),
        }
    }

    /// Files that survive the path filter, in the caller's order, each with
    /// what discovery says it contains.
    ///
    /// Discovered once here rather than per use: the schedule, the check that
    /// a file registered what it declared, and the plan in the report are
    /// three readings of one scan, and a suite of a thousand files should pay
    /// for it once.
    fn select<'a>(&self, files: &'a [TestFile]) -> Vec<SelectedFile<'a>> {
        files
            .iter()
            .filter(|file| self.filter.matches_path(&file.relative))
            .map(|file| SelectedFile {
                plan: Cow::Owned(crate::discovery::discover_tests(
                    &file.relative,
                    &file.source,
                )),
                file,
            })
            .collect()
    }

    /// Files that survive the path filter, with caller-provided discovery plans.
    fn select_planned<'a>(&self, files: &'a [PlannedTestFile]) -> Vec<SelectedFile<'a>> {
        files
            .iter()
            .filter(|planned| self.filter.matches_path(&planned.file.relative))
            .map(|planned| SelectedFile {
                file: &planned.file,
                plan: Cow::Borrowed(&planned.plan),
            })
            .collect()
    }

    /// One worker: take the next file until there are none, or the run bails.
    fn drive(
        &self,
        host: &HostCommand,
        selected: &[SelectedFile<'_>],
        schedule: &[ScheduleEntry],
        state: &RunState,
        observer: &dyn RunObserver,
    ) {
        let mut worker: Option<Worker> = None;
        loop {
            let at = state.next.fetch_add(1, Ordering::Relaxed);
            if at >= schedule.len() {
                retire(&mut worker, host);
                return;
            }
            if self.bailed(state) {
                // Leave the slot empty; `assemble` reports it as not run.
                continue;
            }
            let Some(selected) = selected.get(schedule[at].index) else {
                continue;
            };
            let file = selected.file;
            // Before the worker is found, so a file whose worker will not
            // start is still one that started and finished: a caller counting
            // what is running never sees a finish it did not see begin.
            observer.file_started(&file.relative);

            if worker.is_none() {
                worker = state.take_first();
            }
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

            // Before the clock starts: reading the plan is the runner's work,
            // not the file's.
            let sites = selected.plan.known_sites();
            let started = Instant::now();
            let mut outcome = worker
                .as_mut()
                .map(|worker| self.run_one(worker, file, &sites))
                .unwrap_or(FileOutcome {
                    status: FileStatus::HostFailed {
                        message: String::from("no worker"),
                    },
                    records: Vec::new(),
                    output: Vec::new(),
                });

            // A worker's first file is also the worker booting: `uf` starts the
            // clock when it writes the request, and the process is not ready to
            // read it for tens of milliseconds. Measured here — before a retry
            // sends the worker another request, or a failure retires it — and
            // taken back out of the file's duration below, so the slowest-files
            // table and the recorded timings describe files rather than which
            // file each worker happened to start on.
            let first_answer = micros(started.elapsed());
            let start_up = worker.as_ref().and_then(Worker::start_micros);
            let charged = match (start_up, worker.as_ref().and_then(Worker::reported_micros)) {
                (Some(_), Some(reported)) => first_answer.saturating_sub(reported),
                _ => 0,
            };
            if let Some(start_up) = start_up {
                state.note_start_up(start_up);
            }

            // A file that timed out or lost its host killed the worker; the
            // next file needs a fresh one.
            if !matches!(
                outcome.status,
                FileStatus::Completed | FileStatus::LoadFailed { .. }
            ) {
                retire(&mut worker, host);
            } else if self.options.retry.max_attempts() > 1 {
                self.retry_failures(host, file, &mut outcome, &mut worker);
            }

            // A file that loaded, finished, and reported not one case, when
            // discovery said it holds cases. The worker reports every
            // declaration it registered — a skip and a filtered-out case
            // included — so an empty report from a file with declarations in
            // it means none of them reached `@uniflowed/test`, and the run
            // executed nothing while counting the file as done. See
            // [`FileStatus::RegisteredNothing`], and ubugeeei-prod/uf#482 for
            // the migration this hid: two hundred cases reported green.
            //
            // After the retries, not before: a status set here is not a reason
            // to throw away a working worker, and nothing above can produce
            // this shape without also producing records.
            // Benchmarks count. The worker reports every one, run or skipped,
            // so a benchmark that never reached `@uniflowed/test` is as missing
            // as a test would be.
            let declared = selected.plan.runnable_count() + selected.plan.bench_count();
            if declared > 0
                && outcome.records.is_empty()
                && matches!(outcome.status, FileStatus::Completed)
            {
                outcome.status = FileStatus::RegisteredNothing { declared };
            }

            let report = FileReport {
                file: file.relative.clone(),
                status: outcome.status,
                duration_micros: micros(started.elapsed()).saturating_sub(charged),
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

    /// Run one file once, telling the worker where discovery placed its
    /// declarations.
    fn run_one(&self, worker: &mut Worker, file: &TestFile, sites: &[KnownSite]) -> FileOutcome {
        worker.run_file_with_sites(
            file.absolute.as_str(),
            &file.relative,
            self.filter.name_pattern(),
            self.options.effective_file_timeout(),
            self.options.effective_file_timeout() * MAX_CASES_PER_FILE_BUDGET,
            sites,
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
                    retire(worker, host);
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

/// How long a worker is given to exit by itself when it is discarded.
///
/// It only ever matters to a run collecting coverage — see
/// [`Worker::shutdown`] — and it is a bound rather than a wait: a worker that
/// has not gone by then is killed, and the run loses that worker's counts
/// rather than its ending.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Let go of a worker, giving it the chance to write what it measured.
///
/// A run that is not collecting coverage has nothing to wait for and drops the
/// worker as it always did, because waiting for a process to exit is time a
/// suite spends doing nothing: `uf test` is 0.20 s on a thousand tests and the
/// wait would be a measurable part of that. A run that *is* collecting has to
/// wait, because the counts are written at exit and a killed process writes
/// none.
fn retire(worker: &mut Option<Worker>, host: &HostCommand) {
    let Some(mut worker) = worker.take() else {
        return;
    };
    if host.collects_coverage() {
        worker.shutdown(SHUTDOWN_GRACE);
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
    /// The worker started to prove the host starts, until a thread takes it.
    ///
    /// Still here when the run ends only if no thread ever wanted a worker, and
    /// then dropping it kills it.
    first: Mutex<Option<Worker>>,
    /// What each worker cost to start, from the ones that timed a first file.
    start_ups: Mutex<Vec<u64>>,
}

impl RunState {
    fn record(&self, at: usize, report: FileReport, observer: &dyn RunObserver) {
        let completed = self.completed.fetch_add(1, Ordering::SeqCst) + 1;
        observer.file_finished(completed, self.total, &report);
        if let Ok(mut outcomes) = self.outcomes.lock() {
            outcomes[at] = Some(report);
        }
    }

    /// The first worker, for the first thread to ask.
    fn take_first(&self) -> Option<Worker> {
        self.first.lock().ok().and_then(|mut first| first.take())
    }

    /// Keep one worker's start-up for the run's estimate.
    fn note_start_up(&self, micros: u64) {
        if let Ok(mut start_ups) = self.start_ups.lock() {
            start_ups.push(micros);
        }
    }
}

/// The median of `values`, or `None` when there are none.
///
/// The median rather than the mean, because one worker that started behind a
/// compile on the same core is not what the next run should plan around.
fn median(values: &mut [u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

/// A duration in whole microseconds, saturating rather than wrapping.
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn sources_of<'a>(files: &[SelectedFile<'a>]) -> Vec<(&'a str, &'a str)> {
    files
        .iter()
        .map(|selected| {
            (
                selected.file.relative.as_str(),
                selected.file.source.as_str(),
            )
        })
        .collect()
}

/// Put the report together from what the workers returned.
///
/// Files are sorted by path, not by the order they finished, so the report
/// does not depend on the schedule.
fn assemble(
    outcomes: Vec<Option<FileReport>>,
    selected: Vec<SelectedFile<'_>>,
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

    // The scan every file was selected on, handed over rather than repeated:
    // the schedule, the check that a file registered what it declared, and
    // this plan are three readings of one discovery pass.
    let plan = merge_plans(
        selected
            .into_iter()
            .map(|selected| selected.plan.into_owned()),
    );

    let mut summary = TestSummary {
        files: files.len(),
        unsupported_declarations: plan.unsupported.len(),
        foreign_declarations: plan.foreign_count(),
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
    summary.count_files(&files);

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
