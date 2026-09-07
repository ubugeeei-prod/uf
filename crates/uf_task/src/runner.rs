//! Running a plan: what may overlap, what may be replayed, and what the
//! reader sees while it happens.
//!
//! # Output
//!
//! Two tasks writing to one terminal is the reason "run them at once" is not
//! simply better than "run them one at a time": four commands interleaving
//! half-lines is less readable than the sequential run it replaced. So every
//! task that *could* overlap with another has its output piped and each line
//! prefixed with its name, and the prefix is written together with the line
//! under one lock, so a line is never cut in half by another task's.
//!
//! The task the person actually asked for is the exception, and it is not an
//! exception to the rule so much as a consequence of it: it depends, directly
//! or transitively, on every other task in the plan, so by the time it starts
//! there is nothing left to interleave with. It gets uf's own stdin, stdout
//! and stderr, unprefixed — which is what keeps `uf run dev` a dev server with
//! a terminal rather than a pipe, and what keeps `uf run build | wc -c`
//! counting what it used to count.
//!
//! The one thing that overrides that is caching: a task uf may replay has to
//! be recorded, and recording means reading its output rather than handing it
//! the terminal. Such a task is piped and copied through as it arrives, so it
//! still streams — it just is not a terminal any more. That is the trade a
//! task makes by declaring `inputs`, and no task that wants a terminal
//! declares any.

use std::io::{BufRead as _, BufReader, Read, Write as _};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::time::Instant;

use camino::Utf8PathBuf;
use compact_str::CompactString;

use crate::cache::{
    Change, LastRun, MAX_RECORDED_OUTPUT, MAX_REMEMBERED_INPUTS, RECORD_VERSION, Record, TaskCache,
    encode,
};
use crate::digest::{Digest, Fields, hex};
use crate::inputs::{InputError, Patterns};

/// One task, resolved: everything the runner needs and nothing about how it
/// was configured.
#[derive(Debug, Clone)]
pub struct ScheduledTask {
    /// The task's name, as written in `uf.config.js`.
    pub name: CompactString,
    /// Indices of tasks that must finish first.
    pub dependencies: Vec<usize>,
    /// The command as it will be run, arguments already appended.
    ///
    /// In the key rather than reconstructed from it: two tasks that differ
    /// only in their arguments are two results.
    pub command: String,
    /// Everything it reads.
    pub inputs: Vec<CompactString>,
    /// Everything it writes.
    pub outputs: Vec<CompactString>,
    /// Whether uf may answer it from the cache at all.
    pub cacheable: bool,
    /// A digest over the environment uf will give it.
    pub environment: String,
}

/// How many tasks may run at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Concurrency {
    /// One at a time, which is what `uf run` did before there was a choice.
    Serial,
    /// Exactly this many.
    Fixed(usize),
    /// uf decides: [`DEFAULT_CONCURRENCY`], never more than the machine has
    /// cores.
    Auto,
}

/// What `Auto` means before the core count is applied.
///
/// Four, which is what `vp run` defaults to, and for the same reason: a task
/// is a whole process and most of the interesting ones — a compiler, a test
/// runner — are already using every core they can. Running eight of those at
/// once makes each of them slower without finishing sooner, and on a laptop it
/// makes the machine unusable while it happens.
pub const DEFAULT_CONCURRENCY: usize = 4;

impl Concurrency {
    fn workers(self) -> usize {
        let requested = match self {
            Self::Serial => 1,
            Self::Fixed(count) => count,
            Self::Auto => DEFAULT_CONCURRENCY,
        };
        let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        requested.min(cores).max(1)
    }
}

/// What a run was asked to do.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    pub concurrency: Concurrency,
    /// Run everything, whatever the cache says. Results are still recorded:
    /// `--force` is "do not trust what is there", not "do not remember this".
    pub force: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            concurrency: Concurrency::Auto,
            force: false,
        }
    }
}

/// Why a task ran instead of being replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunReason {
    /// `--force`.
    Forced,
    /// It declares no `inputs`, so there is nothing to key on. The default.
    NoInputs,
    /// It declares `cache: false`.
    CacheDisabled,
    /// Its inputs could not be resolved into a key, and why.
    UnusableInputs(InputError),
    /// Nothing is filed under this key, and what the previous run says is
    /// different.
    Changed(Change),
    /// A file it declares as an `output` is not there any more.
    OutputMissing(String),
    /// A file it declares as an `output` is not what it left there.
    OutputChanged(String),
}

impl std::fmt::Display for RunReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Forced => f.write_str("forced"),
            Self::NoInputs => f.write_str("declares no inputs, so it always runs"),
            Self::CacheDisabled => f.write_str("cache is off for this task"),
            Self::UnusableInputs(error) => write!(f, "its inputs cannot be keyed on: {error}"),
            Self::Changed(change) => write!(f, "{change}"),
            Self::OutputMissing(path) => write!(f, "{path} is missing"),
            Self::OutputChanged(path) => write!(f, "{path} was changed since it was built"),
        }
    }
}

/// What uf decided to do with a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Answered from `.uf/cache/task`; carries what running it cost last time.
    Replayed { saved_micros: u64 },
    /// Run, and why.
    Ran(RunReason),
    /// Not reached, because something else failed first.
    NotRun,
}

impl Decision {
    /// The word for a table.
    #[must_use]
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Replayed { .. } => "replayed",
            Self::Ran(_) => "ran",
            Self::NotRun => "not run",
        }
    }
}

/// How a task ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Succeeded,
    /// It ran and did not succeed, or could not be started; the message is one
    /// line and names the task.
    Failed(String),
    /// It was never reached.
    NotRun,
}

/// What happened to one task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskOutcome {
    pub name: CompactString,
    pub decision: Decision,
    pub status: Status,
    /// Wall clock for this run, in microseconds. A replay costs what the
    /// replay cost; [`Decision::Replayed`] carries what it saved.
    pub duration_micros: u64,
}

/// What happened to all of them, in plan order.
#[derive(Debug, Clone)]
pub struct RunReport {
    pub outcomes: Vec<TaskOutcome>,
}

impl RunReport {
    /// Every task that failed, in plan order.
    #[must_use]
    pub fn failures(&self) -> Vec<&TaskOutcome> {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome.status, Status::Failed(_)))
            .collect()
    }

    /// How many were answered from the cache.
    #[must_use]
    pub fn replayed(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome.decision, Decision::Replayed { .. }))
            .count()
    }
}

/// How the runner turns a task into a process.
///
/// A trait rather than a `sh -c` in this crate, because deciding what runs a
/// task is `uf run`'s question and not the scheduler's: an empty command goes
/// to Vite Task, and the shell that runs the rest is named by the platform.
/// Scheduling, keying and output are the same either way.
pub trait Spawn: Sync {
    /// A command ready to start, with its program, arguments, working
    /// directory and environment set and its stdio left alone.
    ///
    /// # Errors
    ///
    /// When the task cannot be turned into a process at all.
    fn command(&self, name: &str, command: &str) -> std::io::Result<Command>;
}

/// Told what the runner is doing, as it happens.
pub trait Observe: Sync {
    /// A task has finished, one way or another.
    fn finished(&self, outcome: &TaskOutcome);
}

/// Run `tasks`, which must be in dependency order with the requested task last.
///
/// Never returns an error: a task that fails is an outcome, and the caller
/// decides what a failed outcome means for the exit code. What it does return
/// is a report with one entry per task in the order they were given.
pub fn run(
    tasks: &[ScheduledTask],
    root: &camino::Utf8Path,
    cache: &TaskCache,
    options: RunOptions,
    spawn: &dyn Spawn,
    observer: &dyn Observe,
) -> RunReport {
    if tasks.is_empty() {
        return RunReport {
            outcomes: Vec::new(),
        };
    }
    let width = tasks
        .iter()
        .map(|task| task.name.chars().count())
        .max()
        .unwrap_or(0);
    let executor = Executor {
        tasks,
        root: root.to_path_buf(),
        cache,
        options,
        spawn,
        observer,
        sink: Sink::new(),
        label_width: width,
        stopping: AtomicBool::new(false),
    };
    executor.drive()
}

struct Executor<'a> {
    tasks: &'a [ScheduledTask],
    root: Utf8PathBuf,
    cache: &'a TaskCache,
    options: RunOptions,
    spawn: &'a dyn Spawn,
    observer: &'a dyn Observe,
    sink: Sink,
    label_width: usize,
    stopping: AtomicBool,
}

impl Executor<'_> {
    /// The scheduling loop: hand out every task whose dependencies are done,
    /// up to the concurrency limit, and stop handing out new ones once one has
    /// failed.
    fn drive(&self) -> RunReport {
        let count = self.tasks.len();
        let workers = self.options.concurrency.workers();
        let mut remaining: Vec<usize> = self
            .tasks
            .iter()
            .map(|task| task.dependencies.len())
            .collect();
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); count];
        for (at, task) in self.tasks.iter().enumerate() {
            for &dependency in &task.dependencies {
                dependents[dependency].push(at);
            }
        }
        // Plan order, so a run with one worker does exactly what the old
        // depth-first walk did and a reader can predict the schedule.
        let mut ready: Vec<usize> = (0..count).filter(|&at| remaining[at] == 0).collect();
        let mut outcomes: Vec<Option<TaskOutcome>> = vec![None; count];
        let mut in_flight = 0usize;

        std::thread::scope(|scope| {
            let (finished_tx, finished_rx) = channel::<(usize, TaskOutcome)>();
            loop {
                while in_flight < workers && !self.stopping.load(Ordering::SeqCst) {
                    let Some(at) = take_next(&mut ready) else {
                        break;
                    };
                    let sender = finished_tx.clone();
                    // The requested task is the last one, and it starts only
                    // after everything it depends on has finished — which,
                    // this plan being its own dependency closure, is
                    // everything. So it is alone, and it can have the
                    // terminal.
                    let alone = at + 1 == count;
                    scope.spawn(move || {
                        let outcome = self.run_one(at, alone);
                        let _ = sender.send((at, outcome));
                    });
                    in_flight += 1;
                }
                if in_flight == 0 {
                    break;
                }
                let Ok((at, outcome)) = finished_rx.recv() else {
                    break;
                };
                in_flight -= 1;
                let failed = matches!(outcome.status, Status::Failed(_));
                self.observer.finished(&outcome);
                outcomes[at] = Some(outcome);
                if failed {
                    // Everything already started is left to finish: killing a
                    // compiler halfway leaves a target directory nobody can
                    // explain, and the second failure is usually the one that
                    // says why.
                    self.stopping.store(true, Ordering::SeqCst);
                    continue;
                }
                for &dependent in &dependents[at] {
                    remaining[dependent] -= 1;
                    if remaining[dependent] == 0 {
                        ready.push(dependent);
                    }
                }
            }
        });

        RunReport {
            outcomes: outcomes
                .into_iter()
                .enumerate()
                .map(|(at, outcome)| {
                    outcome.unwrap_or_else(|| TaskOutcome {
                        name: self.tasks[at].name.clone(),
                        decision: Decision::NotRun,
                        status: Status::NotRun,
                        duration_micros: 0,
                    })
                })
                .collect(),
        }
    }

    fn run_one(&self, at: usize, alone: bool) -> TaskOutcome {
        let task = &self.tasks[at];
        let started = Instant::now();
        let presentation = if alone {
            Presentation::Plain
        } else {
            Presentation::Prefixed(format!("{:width$} | ", task.name, width = self.label_width))
        };

        let (reason, keyed) = match self.decide(task) {
            Verdict::Replay(record) => {
                // The output is replayed with the record, or a second run of a
                // green pipeline prints nothing and looks like it did nothing.
                self.sink.emit(&presentation, &record.stdout_bytes(), false);
                self.sink.emit(&presentation, &record.stderr_bytes(), true);
                return TaskOutcome {
                    name: task.name.clone(),
                    decision: Decision::Replayed {
                        saved_micros: record.duration_micros,
                    },
                    status: Status::Succeeded,
                    duration_micros: started.elapsed().as_micros() as u64,
                };
            }
            Verdict::Run { reason, keyed } => (reason, keyed),
        };

        // A task that will be recorded has to be read, so it cannot have the
        // terminal even when it is alone.
        let capture = keyed.is_some();
        let outcome = self.execute(task, &presentation, capture, alone);
        let duration = started.elapsed().as_micros() as u64;

        if let (Some(keyed), Captured::Some { stdout, stderr }, Status::Succeeded) =
            (&keyed, &outcome.captured, &outcome.status)
        {
            self.record(task, keyed, stdout, stderr, duration);
        }

        TaskOutcome {
            name: task.name.clone(),
            decision: Decision::Ran(reason),
            status: outcome.status,
            duration_micros: duration,
        }
    }

    /// Whether this task may be replayed, and — when it may not — the key and
    /// note to file its result under.
    ///
    /// Carries no key at all for a task uf will not cache, which is what makes
    /// "declares no inputs" and "the key says re-run" two different states
    /// rather than one: the first writes nothing, so there is never an entry
    /// for a later run to believe.
    fn decide(&self, task: &ScheduledTask) -> Verdict {
        let unkeyed = |reason| Verdict::Run {
            reason,
            keyed: None,
        };
        if !task.cacheable {
            return unkeyed(if task.inputs.is_empty() {
                RunReason::NoInputs
            } else {
                RunReason::CacheDisabled
            });
        }

        let patterns = match Patterns::compile(&task.inputs) {
            Ok(patterns) => patterns,
            Err(error) => return unkeyed(RunReason::UnusableInputs(error)),
        };
        let files = match patterns.resolve(&self.root) {
            Ok(files) => files,
            Err(error) => return unkeyed(RunReason::UnusableInputs(error)),
        };

        let mut fields = Fields::new("uf task cache v1");
        fields
            .push(&RECORD_VERSION.to_string())
            .push(task.name.as_str())
            .push(&task.command)
            .push(&task.environment)
            .push_digest(&patterns.digest("uf task inputs v1", &files));
        for pattern in &task.outputs {
            fields.push(pattern.as_str());
        }
        let key = fields.finish();

        let truncated = files.len() > MAX_REMEMBERED_INPUTS;
        let note = LastRun {
            version: RECORD_VERSION,
            task: task.name.to_string(),
            key: hex(&key),
            command: task.command.clone(),
            environment: task.environment.clone(),
            inputs: files.into_iter().take(MAX_REMEMBERED_INPUTS).collect(),
            inputs_truncated: truncated,
        };

        let keyed = Keyed { key, note };
        let rerun = |reason, keyed: Keyed| Verdict::Run {
            reason,
            keyed: Some(keyed),
        };

        if self.options.force {
            return rerun(RunReason::Forced, keyed);
        }

        let Some(record) = self.cache.read(&keyed.key, task.name.as_str()) else {
            let change = self
                .cache
                .read_last(task.name.as_str())
                .map_or(Change::NeverRun, |last| last.diff(&keyed.note));
            return rerun(RunReason::Changed(change), keyed);
        };

        // The key says the inputs are the same. The outputs are the other
        // half: a result is only still true while the files it produced are
        // the ones it produced.
        match self.outputs_moved(task, &record) {
            Some(reason) => rerun(reason, keyed),
            None => Verdict::Replay(record),
        }
    }

    /// The first declared output that is not what the record says it was.
    fn outputs_moved(&self, task: &ScheduledTask, record: &Record) -> Option<RunReason> {
        if task.outputs.is_empty() {
            return None;
        }
        let patterns = Patterns::compile(&task.outputs).ok()?;
        let now = patterns.resolve(&self.root).ok()?;
        for was in &record.outputs {
            match now.iter().find(|file| file.path == was.path) {
                None => return Some(RunReason::OutputMissing(was.path.clone())),
                Some(file) if file.digest != was.digest => {
                    return Some(RunReason::OutputChanged(was.path.clone()));
                }
                Some(_) => {}
            }
        }
        // A file that appeared where the record had none is somebody else's,
        // and re-running is the answer that cannot be wrong.
        now.iter()
            .find(|file| !record.outputs.iter().any(|was| was.path == file.path))
            .map(|file| RunReason::OutputChanged(file.path.clone()))
    }

    /// Store what a successful run produced.
    fn record(
        &self,
        task: &ScheduledTask,
        keyed: &Keyed,
        stdout: &[u8],
        stderr: &[u8],
        duration: u64,
    ) {
        if stdout.len() > MAX_RECORDED_OUTPUT || stderr.len() > MAX_RECORDED_OUTPUT {
            // Not recorded, and the note is not written either: a note whose
            // key has no record would report "the inputs changed" next time,
            // which is not what happened.
            return;
        }
        let outputs = if task.outputs.is_empty() {
            Vec::new()
        } else {
            match Patterns::compile(&task.outputs).and_then(|patterns| patterns.resolve(&self.root))
            {
                Ok(files) => files,
                // A task whose outputs cannot be read has produced something
                // uf cannot verify later, so there is nothing safe to file.
                Err(_) => return,
            }
        };
        self.cache.write(
            &keyed.key,
            &Record {
                version: RECORD_VERSION,
                task: task.name.to_string(),
                key: hex(&keyed.key),
                stdout: encode(stdout),
                stderr: encode(stderr),
                outputs,
                duration_micros: duration,
            },
        );
        self.cache.write_last(&keyed.note);
    }

    /// Start the task, copy its output where it belongs, and wait for it.
    fn execute(
        &self,
        task: &ScheduledTask,
        presentation: &Presentation,
        capture: bool,
        alone: bool,
    ) -> Executed {
        let mut command = match self.spawn.command(task.name.as_str(), &task.command) {
            Ok(command) => command,
            Err(error) => {
                return Executed {
                    status: Status::Failed(format!(
                        "task {:?} could not be started: {error}",
                        task.name
                    )),
                    captured: Captured::None,
                };
            }
        };

        // Piped whenever the output has to be prefixed or recorded, and only
        // then. Inheriting is what gives a task a terminal, and a task that
        // needs one — a dev server, a prompt — has to be able to get one.
        let piped = capture || matches!(presentation, Presentation::Prefixed(_));
        if piped {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            if !alone {
                // Two tasks reading one stdin is two tasks fighting over it.
                command.stdin(Stdio::null());
            }
        }

        let mut child: Child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return Executed {
                    status: Status::Failed(format!(
                        "task {:?} could not be started: {error}",
                        task.name
                    )),
                    captured: Captured::None,
                };
            }
        };

        // Scoped threads rather than `Arc`: the two readers cannot outlive this
        // call, so a reference is the whole of the ownership proof the
        // workspace's `disallowed-types` asks for.
        let stdout = Mutex::new(Vec::new());
        let stderr = Mutex::new(Vec::new());
        if piped {
            let out = child.stdout.take();
            let err = child.stderr.take();
            let into_stdout = &stdout;
            let into_stderr = &stderr;
            std::thread::scope(|scope| {
                if let Some(stream) = out {
                    scope.spawn(move || {
                        self.pump(stream, presentation, false, capture, into_stdout);
                    });
                }
                if let Some(stream) = err {
                    scope.spawn(move || {
                        self.pump(stream, presentation, true, capture, into_stderr);
                    });
                }
            });
        }

        let status = match child.wait() {
            Ok(status) if status.success() => Status::Succeeded,
            Ok(status) => Status::Failed(format!("task {:?} exited with {status}", task.name)),
            Err(error) => Status::Failed(format!(
                "task {:?} could not be waited on: {error}",
                task.name
            )),
        };
        Executed {
            status,
            captured: if capture {
                Captured::Some {
                    stdout: take(stdout),
                    stderr: take(stderr),
                }
            } else {
                Captured::None
            },
        }
    }

    /// Copy one stream, a line at a time, to the terminal and to the record.
    fn pump<R: Read>(
        &self,
        stream: R,
        presentation: &Presentation,
        is_stderr: bool,
        capture: bool,
        into: &Mutex<Vec<u8>>,
    ) {
        let mut reader = BufReader::new(stream);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            self.sink.emit(presentation, &line, is_stderr);
            if capture && let Ok(mut held) = into.lock() {
                // Bounded, so a task that prints for an hour cannot fill
                // memory; `record` refuses to file anything that reached the
                // bound, so the cut is never what a later run replays.
                if held.len() <= MAX_RECORDED_OUTPUT {
                    held.extend_from_slice(&line);
                }
            }
        }
    }
}

/// What a reader accumulated, however the lock ended up.
fn take(held: Mutex<Vec<u8>>) -> Vec<u8> {
    held.into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The key a result will be filed under, and what that key was built from.
struct Keyed {
    key: Digest,
    note: LastRun,
}

/// What [`Executor::decide`] worked out.
enum Verdict {
    /// There is a record for this key and its outputs are still where it left
    /// them.
    Replay(Record),
    /// It has to run; why, and where to file what it produces.
    Run {
        reason: RunReason,
        keyed: Option<Keyed>,
    },
}

/// One task's captured streams, when it was captured at all.
enum Captured {
    None,
    Some { stdout: Vec<u8>, stderr: Vec<u8> },
}

struct Executed {
    status: Status,
    captured: Captured,
}

/// Where the next chunk of a task's output goes.
enum Presentation {
    /// Straight through, as uf received it.
    Plain,
    /// Each line behind this prefix.
    Prefixed(String),
}

/// The lock that keeps two tasks' lines from cutting each other in half.
struct Sink {
    guard: Mutex<()>,
}

impl Sink {
    fn new() -> Self {
        Self {
            guard: Mutex::new(()),
        }
    }

    /// Write `chunk` — which is one line, or a record's whole output — to the
    /// right stream, prefixed if it is being prefixed.
    fn emit(&self, presentation: &Presentation, chunk: &[u8], is_stderr: bool) {
        if chunk.is_empty() {
            return;
        }
        let held = self.guard.lock();
        let _held = held.unwrap_or_else(std::sync::PoisonError::into_inner);
        let stdout = std::io::stdout();
        let stderr = std::io::stderr();
        let mut out: Box<dyn std::io::Write> = if is_stderr {
            Box::new(stderr.lock())
        } else {
            Box::new(stdout.lock())
        };
        match presentation {
            Presentation::Plain => {
                let _ = out.write_all(chunk);
            }
            Presentation::Prefixed(prefix) => {
                for line in split_lines(chunk) {
                    let _ = out.write_all(prefix.as_bytes());
                    let _ = out.write_all(line);
                    if !line.ends_with(b"\n") {
                        let _ = out.write_all(b"\n");
                    }
                }
            }
        }
        let _ = out.flush();
    }
}

/// `chunk` split after each newline, keeping the newline.
///
/// A record's output arrives in one piece and has to be prefixed line by line;
/// a live stream arrives a line at a time and goes through this unchanged.
fn split_lines(chunk: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut rest = chunk;
    while let Some(at) = rest.iter().position(|byte| *byte == b'\n') {
        lines.push(&rest[..=at]);
        rest = &rest[at + 1..];
    }
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines
}

/// The next task to hand out, in plan order.
fn take_next(ready: &mut Vec<usize>) -> Option<usize> {
    if ready.is_empty() {
        return None;
    }
    let at = ready
        .iter()
        .enumerate()
        .min_by_key(|(_, task)| **task)
        .map(|(position, _)| position)?;
    Some(ready.remove(at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_with_no_newline_is_still_a_line() {
        assert_eq!(split_lines(b"a\nb"), vec![&b"a\n"[..], &b"b"[..]]);
    }

    #[test]
    fn ready_tasks_are_handed_out_in_plan_order() {
        let mut ready = vec![3, 1, 2];
        assert_eq!(take_next(&mut ready), Some(1));
        assert_eq!(take_next(&mut ready), Some(2));
        assert_eq!(take_next(&mut ready), Some(3));
        assert_eq!(take_next(&mut ready), None);
    }

    #[test]
    fn concurrency_never_exceeds_the_cores_the_machine_has() {
        let cores = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        assert_eq!(Concurrency::Serial.workers(), 1);
        assert_eq!(Concurrency::Fixed(1000).workers(), cores);
        assert_eq!(Concurrency::Auto.workers(), DEFAULT_CONCURRENCY.min(cores));
    }
}
