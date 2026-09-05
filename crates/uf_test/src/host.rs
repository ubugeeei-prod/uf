//! The JavaScript host a test file actually runs in.
//!
//! `uf` schedules, bounds and reports; it cannot execute JavaScript, and
//! pretending otherwise is what the old source-level assertion subset did. So
//! a run fans files out to worker processes on the project's Capability JS
//! Host — Node.js or Bun, whichever `uf.config.js` names and the machine has —
//! each running `@uniflowed/test/worker.js`, each importing its file through
//! the host's Flow loader so the module is transformed by the same
//! `uf transform` a build uses.
//!
//! Three properties the design is built around:
//!
//! * **One file at a time per worker.** Two files sharing a process share
//!   globals and module state, and a suite that passes alone but fails beside
//!   another is the worst failure a runner can produce. Workers are reused
//!   across files — process start-up is the expensive part — but never
//!   interleaved.
//! * **A deadline the worker cannot talk its way out of.** The worker races
//!   each case against its own timeout, but a wedged event loop would never
//!   run that timer either, so the driver keeps its own wall clock and kills
//!   the process when it passes.
//! * **A dead worker is a reported file, not a lost run.** Whatever happens to
//!   one process — a crash, a `process.exit`, a stream that stops — the file
//!   is named with what went wrong and the run continues on a fresh worker.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

use crate::plan::SkipReason;
use crate::report::{
    AssertionFailure, FileStatus, MAX_OUTPUT_BYTES_PER_FILE, OutputChunk, OutputStream, TestRecord,
    TestStatus,
};

/// A JavaScript host that can run the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostKind {
    /// Node.js, with the loader hooks from `@uniflowed/host/register`.
    Node,
    /// Bun, with the plugin from `@uniflowed/host/bun-preload`.
    Bun,
    /// Deno.
    Deno,
}

impl HostKind {
    /// The executable's name on PATH.
    #[must_use]
    pub const fn program(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Bun => "bun",
            Self::Deno => "deno",
        }
    }
}

/// Everything needed to start one worker.
///
/// Built once per run and cloned per worker, so every worker in a run is
/// started exactly the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCommand {
    /// Which host this is.
    pub kind: HostKind,
    /// The host executable.
    pub program: Utf8PathBuf,
    /// Arguments before the worker module, e.g. the loader registration.
    pub leading_args: Vec<String>,
    /// The worker module to run.
    pub worker: Utf8PathBuf,
    /// The project root, which the worker runs in.
    pub root: Utf8PathBuf,
    /// The `uf` binary the worker's transform must go through.
    pub uf_binary: Option<Utf8PathBuf>,
    /// Whether this run may rewrite a snapshot that did not match.
    pub update_snapshots: bool,
}

impl HostCommand {
    /// A command that runs the worker with no loader registered.
    ///
    /// Useful on its own only for a project with no Flow in its tests;
    /// [`HostCommand::with_flow_loader`] is what a real run adds.
    #[must_use]
    pub fn new(
        kind: HostKind,
        program: Utf8PathBuf,
        worker: Utf8PathBuf,
        root: Utf8PathBuf,
    ) -> Self {
        Self {
            kind,
            program,
            leading_args: Vec::new(),
            worker,
            root,
            uf_binary: None,
            update_snapshots: false,
        }
    }

    /// Register the host's Flow loader, so an imported module is transformed.
    ///
    /// A host without one can still run the worker; it just cannot import
    /// Flow, which [`HostCommand::loads_flow`] reports so a caller can say so
    /// rather than let the failure arrive as a syntax error.
    #[must_use]
    pub fn with_flow_loader(mut self, register: &Utf8Path, bun_preload: &Utf8Path) -> Self {
        self.leading_args = match self.kind {
            // `--enable-source-maps` is what makes a stack frame name the line
            // the author wrote rather than the line the transform produced:
            // the loader appends a source map to every module it transforms,
            // and without this Node ignores it.
            HostKind::Node => vec![
                String::from("--enable-source-maps"),
                String::from("--import"),
                register.to_string(),
            ],
            HostKind::Bun => vec![String::from("--preload"), bun_preload.to_string()],
            // Deno has no loader hook in `@uniflowed/host` yet, so it can run
            // plain JavaScript tests and nothing else. Saying so is better
            // than a syntax error from a file the host could not transform.
            HostKind::Deno => vec![String::from("run"), String::from("-A")],
        };
        self
    }

    /// Whether this host transforms Flow on import.
    #[must_use]
    pub const fn loads_flow(&self) -> bool {
        matches!(self.kind, HostKind::Node | HostKind::Bun)
    }

    /// Point the worker's transform at a specific `uf` binary.
    #[must_use]
    pub fn with_uf_binary(mut self, binary: Utf8PathBuf) -> Self {
        self.uf_binary = Some(binary);
        self
    }

    /// Let this run rewrite a snapshot that did not match.
    ///
    /// Carried to the worker in the environment rather than in each request:
    /// it is a property of the run, and putting it on every request would let
    /// two files in one run disagree about it.
    #[must_use]
    pub fn with_snapshot_updates(mut self, update: bool) -> Self {
        self.update_snapshots = update;
        self
    }
}

/// One file handed to a worker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    /// Absolute path of the file to import.
    file: &'a str,
    /// Keep only cases whose full name contains this.
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a str>,
    /// Per-case budget in milliseconds.
    timeout_ms: u64,
}

/// One line the worker wrote.
#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
enum Event {
    /// One case finished.
    Test(TestEvent),
    /// The file finished, one way or another.
    File(FileEvent),
    /// Something printed. A test's `console.log` arrives here rather than as a
    /// raw line, which is what stops it from being read as a malformed event.
    Output(OutputEvent),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestEvent {
    name: String,
    #[serde(default)]
    line: usize,
    #[serde(default)]
    column: usize,
    #[serde(default)]
    duration_micros: u64,
    status: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    stack: Option<String>,
    #[serde(default)]
    expected: Option<String>,
    #[serde(default)]
    received: Option<String>,
    #[serde(default)]
    site: Option<Site>,
}

#[derive(Debug, Deserialize)]
struct Site {
    line: usize,
    column: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileEvent {
    status: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    stack: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputEvent {
    /// `"stdout"` or `"stderr"`. Anything else is treated as stdout: which of
    /// two streams a line came from is not worth failing a file over.
    #[serde(default)]
    stream: String,
    /// Full name of the case that was running, absent when none was.
    #[serde(default)]
    test: Option<String>,
    #[serde(default)]
    text: String,
}

/// What one file produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOutcome {
    /// How the file ended.
    pub status: FileStatus,
    /// Every case it reported, in the order they ran.
    pub records: Vec<TestRecord>,
    /// What the file printed outside any case.
    pub output: Vec<OutputChunk>,
}

/// Output the runner has read but cannot place yet.
///
/// A case's output is written before the event that reports the case, so a
/// chunk is held with the name it claims until that name arrives. Whatever is
/// still here when the file ends was printed outside any case — at import
/// time, from a `beforeAll`, or after the last case — and belongs to the file.
#[derive(Debug, Default)]
struct PendingOutput {
    chunks: Vec<(Option<String>, OutputChunk)>,
    bytes: usize,
}

impl PendingOutput {
    /// Keep one chunk, within the file's budget.
    fn push(&mut self, event: OutputEvent) {
        if event.text.is_empty() || self.bytes >= MAX_OUTPUT_BYTES_PER_FILE {
            return;
        }
        let mut text = event.text;
        if text.len() > MAX_OUTPUT_BYTES_PER_FILE - self.bytes {
            // On a character boundary, because half a code point is not one.
            let mut end = MAX_OUTPUT_BYTES_PER_FILE - self.bytes;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        // A chunk that truncated to nothing is dropped rather than kept. One
        // byte of budget left and a test writing multi-byte characters would
        // otherwise push an empty chunk per write for ever: `bytes` never
        // grows, so the budget never closes, and the vector is the only thing
        // that does grow.
        if text.is_empty() {
            return;
        }
        self.bytes += text.len();
        let stream = if event.stream == "stderr" {
            OutputStream::Stderr
        } else {
            OutputStream::Stdout
        };
        self.chunks.push((event.test, OutputChunk { stream, text }));
    }

    /// Everything printed by the case called `name`, in order, removed.
    fn take(&mut self, name: &str) -> Vec<OutputChunk> {
        let mut taken = Vec::new();
        self.chunks.retain_mut(|(test, chunk)| {
            if test.as_deref() == Some(name) {
                taken.push(std::mem::replace(
                    chunk,
                    OutputChunk {
                        stream: OutputStream::Stdout,
                        text: String::new(),
                    },
                ));
                false
            } else {
                true
            }
        });
        taken
    }

    /// Everything left, which is the file's own.
    fn drain(&mut self) -> Vec<OutputChunk> {
        self.bytes = 0;
        std::mem::take(&mut self.chunks)
            .into_iter()
            .map(|(_, chunk)| chunk)
            .collect()
    }
}

/// A worker process, and the thread reading its output.
///
/// The reader is a thread because a blocking read cannot be given a deadline;
/// the driver waits on the channel instead, which can.
#[derive(Debug)]
pub struct Worker {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<String>,
    reader: Option<JoinHandle<()>>,
}

/// Why a worker could not be started.
#[derive(Debug)]
pub struct SpawnError {
    /// What went wrong, already phrased for a reader.
    pub message: String,
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SpawnError {}

impl Worker {
    /// Start a worker.
    ///
    /// # Errors
    ///
    /// [`SpawnError`] when the host executable cannot be run, which is the one
    /// failure that is worth stopping the whole run for: every file would hit
    /// it.
    pub fn spawn(command: &HostCommand) -> Result<Self, SpawnError> {
        let mut process = Command::new(command.program.as_std_path());
        process
            .args(&command.leading_args)
            .arg(command.worker.as_str())
            .current_dir(command.root.as_std_path())
            .env("UF_PROJECT_ROOT", command.root.as_str())
            .env(
                "UF_UPDATE_SNAPSHOTS",
                if command.update_snapshots { "1" } else { "" },
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The worker's stderr is the host's own noise — an unhandled
            // warning, a deprecation — and it is not part of the report. It is
            // inherited so a person debugging sees it, rather than swallowed.
            .stderr(Stdio::inherit());
        if let Some(binary) = &command.uf_binary {
            process.env("UF_BINARY", binary.as_str());
        }

        let mut child = process.spawn().map_err(|error| SpawnError {
            message: format!("could not start `{}`: {error}", command.program),
        })?;
        let stdin = child.stdin.take().ok_or_else(|| SpawnError {
            message: String::from("the worker has no stdin"),
        })?;
        let stdout = child.stdout.take().ok_or_else(|| SpawnError {
            message: String::from("the worker has no stdout"),
        })?;

        let (sender, events) = channel();
        let reader = std::thread::Builder::new()
            .name(String::from("uf-test-worker"))
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else {
                        break;
                    };
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| SpawnError {
                message: format!("could not start the worker reader: {error}"),
            })?;

        Ok(Self {
            child,
            stdin,
            events,
            reader: Some(reader),
        })
    }

    /// Run one file and collect what it reported.
    ///
    /// `deadline` bounds the whole file. Passing it kills the worker, which is
    /// why the caller must replace it afterwards — [`FileOutcome`] carrying a
    /// [`FileStatus::TimedOut`] means this worker is gone.
    pub fn run_file(
        &mut self,
        file: &str,
        relative: &str,
        filter: Option<&str>,
        case_timeout: Duration,
        deadline: Duration,
    ) -> FileOutcome {
        let request = Request {
            file,
            filter,
            timeout_ms: case_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
        };
        let mut line = match serde_json::to_string(&request) {
            Ok(line) => line,
            Err(error) => {
                return Self::host_failed(relative, format!("unencodable request: {error}"));
            }
        };
        line.push('\n');
        if let Err(error) = self
            .stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.flush())
        {
            return Self::host_failed(relative, format!("could not reach the worker: {error}"));
        }

        let started = Instant::now();
        let mut records = Vec::new();
        // Whatever the file printed travels with it whatever happens to it: a
        // file that hung after printing is the case where the printing is most
        // of the evidence there is.
        let mut pending = PendingOutput::default();
        loop {
            let remaining = deadline.checked_sub(started.elapsed());
            let Some(remaining) = remaining else {
                self.kill();
                return FileOutcome {
                    status: FileStatus::TimedOut {
                        budget_micros: u64::try_from(deadline.as_micros()).unwrap_or(u64::MAX),
                    },
                    records,
                    output: pending.drain(),
                };
            };
            match self.events.recv_timeout(remaining) {
                Ok(line) => match serde_json::from_str::<Event>(&line) {
                    Ok(Event::Test(event)) => {
                        let mut record = record_of(relative, event);
                        record.output = pending.take(&record.name);
                        records.push(record);
                    }
                    Ok(Event::Output(event)) => pending.push(event),
                    Ok(Event::File(event)) => {
                        return FileOutcome {
                            status: file_status(event),
                            records,
                            output: pending.drain(),
                        };
                    }
                    Err(error) => {
                        self.kill();
                        return FileOutcome {
                            status: FileStatus::HostFailed {
                                message: format!("unreadable worker output: {error}: {line}"),
                            },
                            records,
                            output: pending.drain(),
                        };
                    }
                },
                Err(RecvTimeoutError::Timeout) => {
                    self.kill();
                    return FileOutcome {
                        status: FileStatus::TimedOut {
                            budget_micros: u64::try_from(deadline.as_micros()).unwrap_or(u64::MAX),
                        },
                        records,
                        output: pending.drain(),
                    };
                }
                // The reader ended, which means the process did: it exited or
                // crashed without finishing the file.
                Err(RecvTimeoutError::Disconnected) => {
                    let how = match self.child.try_wait() {
                        Ok(Some(status)) => format!("the worker exited ({status})"),
                        _ => String::from("the worker stopped writing"),
                    };
                    return FileOutcome {
                        status: FileStatus::HostFailed { message: how },
                        records,
                        output: pending.drain(),
                    };
                }
            }
        }
    }

    fn host_failed(file: &str, message: String) -> FileOutcome {
        let _ = file;
        FileOutcome {
            status: FileStatus::HostFailed { message },
            records: Vec::new(),
            output: Vec::new(),
        }
    }

    /// Stop the worker, waiting for its reader so no thread outlives the run.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.kill();
    }
}

fn file_status(event: FileEvent) -> FileStatus {
    match event.status.as_str() {
        "completed" => FileStatus::Completed,
        "load-failed" => FileStatus::LoadFailed {
            message: event
                .message
                .unwrap_or_else(|| String::from("the module threw while loading")),
            stack: event.stack,
        },
        other => FileStatus::HostFailed {
            message: event
                .message
                .unwrap_or_else(|| format!("the worker reported {other}")),
        },
    }
}

fn record_of(file: &str, event: TestEvent) -> TestRecord {
    let status = match event.status.as_str() {
        "passed" => TestStatus::Passed,
        "todo" => TestStatus::Todo,
        "skipped" => TestStatus::Skipped {
            reason: match event.reason.as_deref() {
                Some("not-only") => SkipReason::NotOnly,
                Some("filtered") => SkipReason::Filtered,
                _ => SkipReason::Explicit,
            },
        },
        _ => {
            let site = event.site;
            TestStatus::Failed {
                failures: vec![AssertionFailure {
                    message: event
                        .message
                        .unwrap_or_else(|| String::from("the test failed without a message")),
                    line: site.as_ref().map_or(event.line, |site| site.line),
                    column: site.as_ref().map_or(event.column, |site| site.column),
                    span: 1,
                    expected: event.expected,
                    received: event.received,
                    stack: event.stack,
                }],
            }
        }
    };
    TestRecord {
        file: file.to_string(),
        name: event.name,
        line: event.line,
        column: event.column,
        status,
        attempts: 1,
        duration_micros: event.duration_micros,
        output: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_command_registers_the_flow_loader() {
        let command = HostCommand::new(
            HostKind::Node,
            Utf8PathBuf::from("/usr/bin/node"),
            Utf8PathBuf::from("/p/worker.js"),
            Utf8PathBuf::from("/p"),
        )
        .with_flow_loader(
            Utf8Path::new("@uniflowed/host/register"),
            Utf8Path::new("/p/bun-preload.js"),
        );

        assert_eq!(
            command.leading_args,
            [
                "--enable-source-maps",
                "--import",
                "@uniflowed/host/register"
            ]
        );
        assert!(command.loads_flow());
    }

    #[test]
    fn a_bun_command_preloads_the_plugin() {
        let command = HostCommand::new(
            HostKind::Bun,
            Utf8PathBuf::from("/usr/bin/bun"),
            Utf8PathBuf::from("/p/worker.js"),
            Utf8PathBuf::from("/p"),
        )
        .with_flow_loader(
            Utf8Path::new("@uniflowed/host/register"),
            Utf8Path::new("/p/bun-preload.js"),
        );

        assert_eq!(command.leading_args, ["--preload", "/p/bun-preload.js"]);
        assert!(command.loads_flow());
    }

    #[test]
    fn deno_can_run_but_cannot_load_flow() {
        let command = HostCommand::new(
            HostKind::Deno,
            Utf8PathBuf::from("/usr/bin/deno"),
            Utf8PathBuf::from("/p/worker.js"),
            Utf8PathBuf::from("/p"),
        )
        .with_flow_loader(Utf8Path::new("a"), Utf8Path::new("b"));

        assert_eq!(command.leading_args, ["run", "-A"]);
        assert!(!command.loads_flow());
    }

    #[test]
    fn a_failed_case_carries_the_matchers_own_message_and_site() {
        let record = record_of(
            "src/a.test.js",
            TestEvent {
                name: String::from("a > b"),
                line: 3,
                column: 1,
                duration_micros: 10,
                status: String::from("failed"),
                reason: None,
                message: Some(String::from("expected 1 to be 2")),
                stack: Some(String::from("AssertionError: …")),
                expected: Some(String::from("2")),
                received: Some(String::from("1")),
                site: Some(Site {
                    line: 4,
                    column: 12,
                }),
            },
        );

        let failures = record.status.failures();
        assert_eq!(failures[0].message, "expected 1 to be 2");
        // The record points at the `it(` line and the failure at the assertion.
        assert_eq!(record.line, 3);
        assert_eq!(failures[0].line, 4);
        assert_eq!(failures[0].column, 12);
        assert_eq!(failures[0].expected.as_deref(), Some("2"));
    }

    /// The line the worker writes for `console.log(text)`.
    fn output_line(test: Option<&str>, text: &str) -> String {
        serde_json::json!({
            "event": "output",
            "stream": "stdout",
            "test": test,
            "text": format!("{text}\n"),
        })
        .to_string()
    }

    #[test]
    fn an_output_event_carries_the_stream_the_test_wrote_to() {
        let line = r#"{"event":"output","stream":"stderr","test":"a > b","text":"oh no\n"}"#;

        let Ok(Event::Output(event)) = serde_json::from_str::<Event>(line) else {
            panic!("an output line must parse as an output event");
        };
        assert_eq!(event.stream, "stderr");
        assert_eq!(event.test.as_deref(), Some("a > b"));
        assert_eq!(event.text, "oh no\n");
    }

    #[test]
    fn output_is_attributed_to_the_case_that_printed_it() {
        let mut pending = PendingOutput::default();
        for line in [
            output_line(None, "loading"),
            output_line(Some("a > b"), "from b"),
            output_line(Some("a > c"), "from c"),
        ] {
            let Ok(Event::Output(event)) = serde_json::from_str::<Event>(&line) else {
                panic!("an output line must parse as an output event");
            };
            pending.push(event);
        }

        let taken = pending.take("a > b");
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].text, "from b\n");
        assert_eq!(taken[0].stream, OutputStream::Stdout);
        // Taking one case's output leaves the other case's alone, and what no
        // case claims is the file's.
        assert_eq!(pending.take("a > c")[0].text, "from c\n");
        let file = pending.drain();
        assert_eq!(file.len(), 1);
        assert_eq!(file[0].text, "loading\n");
    }

    #[test]
    fn output_the_worker_never_named_a_case_for_belongs_to_the_file() {
        let mut pending = PendingOutput::default();
        let Ok(Event::Output(event)) = serde_json::from_str::<Event>(&output_line(
            Some("a case that never reported"),
            "orphan",
        )) else {
            panic!("an output line must parse as an output event");
        };
        pending.push(event);

        assert!(pending.take("a > b").is_empty());
        assert_eq!(pending.drain()[0].text, "orphan\n");
    }

    #[test]
    fn a_test_printing_a_protocol_line_does_not_become_one() {
        // The whole point of carrying output as a field: this is what
        // `console.log('{"event":"file","status":"completed"}')` puts on the
        // wire, and reading it must not end the file.
        let printed = r#"{"event":"file","status":"completed"}"#;
        let line = output_line(Some("a > b"), printed);

        let Ok(Event::Output(event)) = serde_json::from_str::<Event>(&line) else {
            panic!("a printed protocol line must stay an output event");
        };
        assert_eq!(event.text, format!("{printed}\n"));

        let mut pending = PendingOutput::default();
        pending.push(event);
        let taken = pending.take("a > b");
        assert_eq!(taken[0].text, format!("{printed}\n"));
    }

    #[test]
    fn a_stream_the_runner_does_not_know_is_treated_as_stdout() {
        // Which of two streams a line came from is not worth failing a file
        // over, so an unrecognised name takes the ordinary one.
        let line = r#"{"event":"output","stream":"tty","text":"hi\n"}"#;
        let Ok(Event::Output(event)) = serde_json::from_str::<Event>(line) else {
            panic!("an output line must parse as an output event");
        };

        let mut pending = PendingOutput::default();
        pending.push(event);
        assert_eq!(pending.drain()[0].stream, OutputStream::Stdout);
    }

    #[test]
    fn one_file_cannot_print_more_than_the_budget() {
        let mut pending = PendingOutput::default();
        for _ in 0..64 {
            pending.push(OutputEvent {
                stream: String::from("stdout"),
                test: None,
                text: "x".repeat(4096),
            });
        }

        let kept: usize = pending.drain().iter().map(|chunk| chunk.text.len()).sum();
        assert_eq!(kept, MAX_OUTPUT_BYTES_PER_FILE);
    }

    #[test]
    fn the_budget_cuts_on_a_character_boundary() {
        let mut pending = PendingOutput::default();
        pending.push(OutputEvent {
            stream: String::from("stdout"),
            test: None,
            text: "x".repeat(MAX_OUTPUT_BYTES_PER_FILE - 1),
        });
        // Two bytes of one character, with one byte of room: neither byte is
        // kept, because half of a character is not a character.
        pending.push(OutputEvent {
            stream: String::from("stdout"),
            test: None,
            text: String::from("é"),
        });

        let chunks = pending.drain();
        assert_eq!(chunks.len(), 1, "the half character is not a chunk");
        assert!(chunks.iter().all(|chunk| chunk.text.is_char_boundary(0)));
    }

    #[test]
    fn a_test_that_keeps_writing_past_the_budget_does_not_grow_the_vector() {
        // One byte of room and a multi-byte character: every write truncates
        // to nothing, `bytes` never grows, and the budget never closes. Keeping
        // those empty chunks is a way to run the runner out of memory from
        // inside a test.
        let mut pending = PendingOutput::default();
        pending.push(OutputEvent {
            stream: String::from("stdout"),
            test: None,
            text: "x".repeat(MAX_OUTPUT_BYTES_PER_FILE - 1),
        });
        for _ in 0..10_000 {
            pending.push(OutputEvent {
                stream: String::from("stdout"),
                test: None,
                text: String::from("é"),
            });
        }

        assert_eq!(pending.drain().len(), 1);
    }

    #[test]
    fn skip_reasons_survive_the_round_trip() {
        for (reason, expected) in [
            ("explicit", SkipReason::Explicit),
            ("not-only", SkipReason::NotOnly),
            ("filtered", SkipReason::Filtered),
        ] {
            let record = record_of(
                "a.js",
                TestEvent {
                    name: String::from("t"),
                    line: 1,
                    column: 1,
                    duration_micros: 0,
                    status: String::from("skipped"),
                    reason: Some(String::from(reason)),
                    message: None,
                    stack: None,
                    expected: None,
                    received: None,
                    site: None,
                },
            );
            assert_eq!(record.status, TestStatus::Skipped { reason: expected });
        }
    }
}
