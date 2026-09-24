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
//! Four properties the design is built around:
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
//! * **Every event says which file it belongs to.** "One file at a time" bounds
//!   what the worker *starts*, not what a finished file left running: a
//!   `setTimeout` nobody waited for still fires, and its events land in the
//!   middle of the next file's. Each request carries a generation number, every
//!   event is stamped with the generation it was written under, and an event
//!   stamped with a request this worker has already finished is dropped with a
//!   note instead of being handed to the file running now.

use std::collections::BTreeMap;
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
    /// Deno 2.8 or newer, with the hooks from `@uniflowed/host/deno-preload`.
    ///
    /// 2.8 is where Deno implemented `node:module`'s `registerHooks`, the
    /// synchronous hook the preload is built on. An older Deno has no hook to
    /// install anything in, and `uf test` refuses it by version rather than
    /// starting workers that cannot read a line of Flow.
    Deno,
    /// A real browser, driven by `@uniflowed/test/browser-worker.js`.
    ///
    /// # Why this is a host and not a mode
    ///
    /// A host, in this module, is *where a test body runs*. On the three above
    /// that is the process uf starts; here it is a page, and the process uf
    /// starts is the driver that serves it. Nothing else about the seam moves:
    /// the driver is spawned like any worker, is written to on stdin, answers
    /// with the same events on stdout, is bounded by the same deadline, and is
    /// killed and replaced when it stops answering. Scheduling — which files,
    /// in what order, how many at once, what a retry means — stays in Rust and
    /// never learns that this one is a browser.
    ///
    /// What is different is written down rather than hidden: [`Self::program`]
    /// on the driver, [`HostCommand::browser`] for the browser it drives, and
    /// [`HostCommand::can_collect_coverage`] for the one thing this host cannot
    /// be asked for.
    Browser,
}

impl HostKind {
    /// The executable's name on PATH.
    ///
    /// For [`Self::Browser`] this is the *driver's* program, not the browser's.
    /// A page cannot read a pipe, so the process uf starts for a browser run is
    /// still an ordinary JavaScript host running an ordinary worker module; the
    /// browser is a second binary that worker launches, and it is named
    /// separately in [`HostCommand::browser`] because it is a separate
    /// dependency with a separate way of being absent.
    ///
    /// Node rather than the project's Capability JS Host, deliberately and for
    /// now. The driver runs one HTTP server and one child process and shuttles
    /// JSON between them — nothing about it needs to be the runtime under test,
    /// because the runtime under test is the browser. Pinning it to one host
    /// removes an axis from a feature that already has enough of them, and
    /// `docs/hosts.md` says so on the browser's row.
    #[must_use]
    pub const fn program(self) -> &'static str {
        match self {
            Self::Node | Self::Browser => "node",
            Self::Bun => "bun",
            Self::Deno => "deno",
        }
    }

    /// How a message names this host to a person.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Node => "Node.js",
            Self::Bun => "Bun",
            Self::Deno => "Deno",
            Self::Browser => "the browser",
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
    /// Variables to set on every worker, from the project's `.env` files.
    ///
    /// A test reads configuration the way the application does — through
    /// `process.env` — so the runner has to put the same values there. See
    /// `uf_config::env_files` for where they come from and what wins.
    pub env: Vec<(String, String)>,
    /// Whether this run may rewrite a snapshot that did not match.
    pub update_snapshots: bool,
    /// Whether this is a run of the benchmarks, `uf test --bench`, rather than
    /// of the tests.
    pub bench: bool,
    /// The project's accessibility rule set, as the JSON `axe.js` reads.
    ///
    /// A property of the project rather than of the invocation, so it travels
    /// with the run the way `update_snapshots` does: two files in one run must
    /// not be able to disagree about what "accessible" means.
    pub axe: Option<String>,
    /// The browser a [`HostKind::Browser`] command drives.
    ///
    /// Passed to the driver in the environment rather than on the command line
    /// because it is a property of the run and not of any one file, exactly
    /// like `update_snapshots` and `axe`. Which binary this is, and what
    /// happens when there is none, is [`crate::browser`]; by the time it
    /// reaches here the question has already been answered or the run has
    /// already stopped.
    pub browser: Option<Utf8PathBuf>,
    /// Where each worker writes its V8 coverage document, when coverage is on.
    ///
    /// Set as `NODE_V8_COVERAGE`, which is Node's own switch: V8 counts
    /// execution whether anyone asks or not, and this is the variable that
    /// makes Node write those counts — and the source-map cache beside them —
    /// out when the process exits. Nothing is instrumented and the worker does
    /// not know it is being measured; see [`crate::coverage`] for why that is
    /// the property worth having.
    pub coverage_dir: Option<Utf8PathBuf>,
}

/// The variable that tells a worker it will be kept between runs; see
/// [`HostCommand::with_kept_workers`].
const KEEP_WORKERS: &str = "UF_TEST_KEEP_WORKERS";

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
            env: Vec::new(),
            update_snapshots: false,
            bench: false,
            axe: None,
            browser: None,
            coverage_dir: None,
        }
    }

    /// Drive this browser, for a [`HostKind::Browser`] command.
    #[must_use]
    pub fn with_browser(mut self, browser: Utf8PathBuf) -> Self {
        self.browser = Some(browser);
        self
    }

    /// Set these variables on every worker this command starts.
    #[must_use]
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    /// Register the host's Flow loader, so an imported module is transformed.
    ///
    /// One argument per host that needs a module named, because the three
    /// are reached three ways: Node imports `register` with `--import`, Bun
    /// preloads its plugin, and Deno preloads the hooks. Each command uses the
    /// one for its own kind.
    #[must_use]
    pub fn with_flow_loader(
        mut self,
        register: &Utf8Path,
        bun_preload: &Utf8Path,
        deno_preload: &Utf8Path,
    ) -> Self {
        self.leading_args = match self.kind {
            // `--enable-source-maps` is what makes a stack frame name the line
            // the author wrote rather than the line the transform produced:
            // the loader appends a source map to every module it transforms,
            // and without this Node ignores it.
            // The browser's driver is a Node process running an ordinary Flow
            // module, so it registers the loader the ordinary way. What the
            // *page* imports is transformed by the same `uf transform` through
            // a different door — the driver's module server — because a page
            // has no loader hook to install one in. Same compiler, same cache,
            // two ways in; see `packages/test/internal/browser/serve.js`.
            HostKind::Node | HostKind::Browser => vec![
                String::from("--enable-source-maps"),
                String::from("--import"),
                register.to_string(),
            ],
            HostKind::Bun => vec![String::from("--preload"), bun_preload.to_string()],
            // `run`, then the hooks: `registerHooks` from `node:module`, which
            // Deno implements from 2.8 and runs synchronously in the importing
            // thread, installed before the worker's first import. Then
            // whatever [`HostCommand::with_permissions`] adds.
            //
            // What is deliberately *not* here is `-A`. It used to be, on the
            // reasoning that Deno's default — no filesystem, no network, no
            // environment — is the opposite of every other host's, so an
            // all-access flag was parity rather than a grant. The reasoning
            // holds and the conclusion does not: `-A` is a grant nothing later
            // on the command line takes back, so every run on the one host
            // that can enforce the whole of `uf.config.js`'s permission model
            // started by turning it off. A Deno host that begins there can
            // only ever narrow by remembering to, and forgetting is silent.
            //
            // So the baseline is the toolchain's own access instead —
            // `uf_runtime::permissions::host_arguments` with an empty declared
            // set, which is the project root, its packages and nothing else.
            // `uf explain test` prints it. See ubugeeei-prod/uf#246.
            HostKind::Deno => vec![
                String::from("run"),
                String::from("--preload"),
                deno_preload.to_string(),
            ],
        };
        self
    }

    /// Put a translated permission set in force on every worker.
    ///
    /// The arguments come from `uf_runtime::permissions::host_arguments`, which
    /// is where the per-host translation and the refusals live; this only has
    /// to place them. They go after the loader registration and before the
    /// worker module, which is where every host wants its own flags — and on
    /// Deno *after* the `run` subcommand, which is why they are appended rather
    /// than prepended.
    ///
    /// On Deno this is called for every run, not only for a project that
    /// declared a set. Deno has no "no permission model" to fall back to — its
    /// default grants nothing at all — so the choice there is between the
    /// toolchain's own access and `-A`, and the second is not a choice uf
    /// makes any more. See [`HostCommand::with_flow_loader`].
    #[must_use]
    pub fn with_permissions(mut self, arguments: Vec<String>) -> Self {
        self.leading_args.extend(arguments);
        self
    }

    /// Whether this host transforms Flow on import.
    ///
    /// Node, Bun and Deno each have a hook installed by the arguments
    /// [`HostCommand::with_flow_loader`] wrote, and every module the worker
    /// imports goes through it. Deno's answered `false` until its hook existed:
    /// before 2.8 its loader was an ahead-of-time pass over the modules uf
    /// could enumerate, and a command that had not been handed the pass's
    /// output could not read Flow.
    ///
    /// A browser answers `true` for a different reason: the page is not what
    /// reads a module. The driver serves it every one through the same
    /// `uf transform` the Node loader calls, so there is nothing to install in
    /// the runtime — which is why this arm is spelled out rather than folded
    /// in with the other three.
    #[must_use]
    pub const fn loads_flow(&self) -> bool {
        match self.kind {
            HostKind::Node | HostKind::Bun | HostKind::Deno => true,
            HostKind::Browser => true,
        }
    }

    /// Point the worker's transform at a specific `uf` binary.
    #[must_use]
    pub fn with_uf_binary(mut self, binary: Utf8PathBuf) -> Self {
        self.uf_binary = Some(binary);
        self
    }

    /// Collect V8 coverage from every worker into `directory`.
    ///
    /// The directory must be this run's alone and must already exist: Node
    /// appends a document per process and reads nothing back, so a directory
    /// shared with a previous run would merge that run's counts into this one's.
    #[must_use]
    pub fn with_coverage_dir(mut self, directory: Utf8PathBuf) -> Self {
        self.coverage_dir = Some(directory);
        self
    }

    /// Whether this command collects coverage.
    #[must_use]
    pub const fn collects_coverage(&self) -> bool {
        self.coverage_dir.is_some()
    }

    /// Whether this host can collect coverage at all.
    ///
    /// Node only, and the reason is not a missing feature of uf's: Bun
    /// implements no `NODE_V8_COVERAGE`, and Deno writes its counts through
    /// `--coverage` in a profile format of its own, with no source-map cache
    /// beside them for [`crate::coverage`] to map back through. A caller is
    /// expected to say so rather than report a run of zeroes.
    ///
    /// The browser is the interesting `false`, because the counters are right
    /// there — V8 is counting in the renderer exactly as it counts in Node. The
    /// switch is not: `NODE_V8_COVERAGE` is Node's own, written from a Node
    /// exit handler, and the only way to ask a page for its profile is the
    /// DevTools protocol, which is a browser-shaped dependency this mode
    /// deliberately does not have (see [`crate::browser`]). Coverage from a
    /// browser run is worth having and is not this change.
    #[must_use]
    pub const fn can_collect_coverage(&self) -> bool {
        matches!(self.kind, HostKind::Node)
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

    /// Run the benchmarks in place of the tests: `uf test --bench`.
    ///
    /// In the environment, like snapshot updates, because it is a property of
    /// the run rather than of any one file.
    #[must_use]
    pub fn with_benchmarks(mut self, bench: bool) -> Self {
        self.bench = bench;
        self
    }

    /// Give every worker the project's accessibility rule set.
    ///
    /// `None` leaves the variable unset, which the matcher reads as "run every
    /// rule" — the widest answer, and the right one for a project that has not
    /// said anything.
    #[must_use]
    pub fn with_axe(mut self, axe: Option<String>) -> Self {
        self.axe = axe;
        self
    }

    /// Start workers that will be kept between runs, as `uf test --watch`
    /// keeps them.
    ///
    /// A kept worker is told which files changed before its next run, and its
    /// Flow loader records the graph it loads so that it can answer that; see
    /// [`Worker::invalidate`]. A worker that is not kept loads every module
    /// once, and records nothing it would never be asked about.
    ///
    /// Carried as `UF_TEST_KEEP_WORKERS=1` in [`HostCommand::env`] rather than
    /// as a field of its own, so every existing way of building this struct
    /// keeps compiling. A project whose `.env` sets the same name only makes
    /// its workers record a graph nobody asks about.
    #[must_use]
    pub fn with_kept_workers(mut self, keep: bool) -> Self {
        self.env.retain(|(name, _)| name != KEEP_WORKERS);
        if keep {
            self.env.push((KEEP_WORKERS.to_owned(), "1".to_owned()));
        }
        self
    }

    /// Whether [`HostCommand::with_kept_workers`] asked for kept workers.
    #[must_use]
    pub fn keeps_workers(&self) -> bool {
        self.env
            .iter()
            .any(|(name, value)| name == KEEP_WORKERS && value == "1")
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
    /// Run only this share of the file's cases; see [`Part`].
    #[serde(skip_serializing_if = "Option::is_none")]
    part: Option<Part>,
    /// Which request this is, counting from one within this worker.
    ///
    /// The worker stamps it on every event it writes while serving this
    /// request — including from a callback the file left behind, which is the
    /// whole point — and [`Worker::run_file`] refuses an event stamped with any
    /// other. It is the same number the worker already used to bust its import
    /// cache; making it part of the protocol is what lets the two sides agree
    /// on which file an event came from.
    ///
    /// Assigned here rather than counted in the worker because the side that
    /// has to check a number should be the side that chose it. The worker
    /// counts the requests it serves too, so the two agree by construction, and
    /// its count is only ever used as a fallback for a host too old to send
    /// this field.
    generation: u64,
}

/// One share of a file's cases, for a file run on several workers at once.
///
/// A file's cases are numbered in the order the file registers them, and share
/// `index` of `count` is the contiguous run of them from `index * n / count`
/// up to `(index + 1) * n / count`, `n` being how many there are. Every share
/// imports the whole file — a case cannot be reached without the module that
/// declares it — and runs, and reports, its own cases and no others, so the
/// shares of one file between them report every case exactly once.
///
/// Contiguous rather than dealt round-robin, so a `beforeAll` that sets up for
/// a `describe` is run by as few shares as the split allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Part {
    /// Which share, from zero.
    pub index: usize,
    /// How many shares the file is split into.
    pub count: usize,
}

/// One request for [`Worker::run`]: which file, and how to run it.
#[derive(Debug, Clone, Copy)]
pub struct FileRun<'a> {
    /// Absolute path the worker imports.
    pub file: &'a str,
    /// The path records are reported under.
    pub relative: &'a str,
    /// Keep only cases whose full name contains this.
    pub filter: Option<&'a str>,
    /// Run only this share of the cases; all of them when `None`.
    pub part: Option<Part>,
    /// The budget for one case.
    pub case_timeout: Duration,
    /// The budget for the whole file, after which the worker is killed.
    pub deadline: Duration,
}

/// The files that changed, sent to a kept worker between runs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvalidateRequest<'a> {
    /// Absolute paths, as the worker's loader resolves them.
    invalidate: &'a [String],
    /// Counted with the file requests, so an event a finished file left
    /// behind cannot be taken for the answer to this.
    generation: u64,
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
    /// The answer to an [`InvalidateRequest`].
    Invalidated(InvalidatedEvent),
}

impl Event {
    /// Which request the worker was serving when it wrote this.
    const fn generation(&self) -> u64 {
        match self {
            Self::Test(event) => event.generation,
            Self::File(event) => event.generation,
            Self::Output(event) => event.generation,
            Self::Invalidated(event) => event.generation,
        }
    }

    /// How a note names this event, when it arrived too late to be reported.
    ///
    /// Everything quoted here was chosen by the worker, so everything quoted
    /// here goes through [`excerpt`]: a note about untrusted output must not
    /// itself be a way to write a screen's worth of it.
    fn describe(&self) -> String {
        match self {
            Self::Test(event) => format!("the case \"{}\"", excerpt(&event.name)),
            Self::File(event) => match &event.message {
                Some(message) => format!(
                    "the file result \"{}\": {}",
                    excerpt(&event.status),
                    excerpt(message)
                ),
                None => format!("the file result \"{}\"", excerpt(&event.status)),
            },
            Self::Output(event) => format!("output \"{}\"", excerpt(&event.text)),
            Self::Invalidated(_) => String::from("an answer about changed files"),
        }
    }
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
    /// One timing per measured call, in whole microseconds, for a benchmark.
    #[serde(default)]
    samples: Option<Vec<u64>>,
    /// The request this was written under. See [`Request::generation`].
    #[serde(default)]
    generation: u64,
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
    /// How long the worker says the file took, from taking the request to
    /// answering it.
    ///
    /// The worker's own clock rather than `uf`'s, and that is the point: `uf`
    /// starts timing when it writes the request, so the first file a worker is
    /// given is also charged for the worker booting. See
    /// [`Worker::start_micros`].
    #[serde(default)]
    duration_micros: Option<u64>,
    /// The request this was written under. See [`Request::generation`].
    #[serde(default)]
    generation: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvalidatedEvent {
    /// Whether the worker can promise its next file sees every change.
    #[serde(default)]
    ok: bool,
    /// The request this answers. See [`Request::generation`].
    #[serde(default)]
    generation: u64,
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
    /// The request this was written under. See [`Request::generation`].
    #[serde(default)]
    generation: u64,
}

/// Whether `stamp` names a request other than the one being served.
///
/// Zero is not a request. It is what an event carries when nothing said which
/// one it belonged to: a worker older than this field, or a host whose
/// asynchronous storage did not reach the callback that wrote it — Deno 1.31
/// does not carry a store through `setTimeout`, and Bun does not carry one into
/// its unhandled-rejection hook. An unstamped event is taken as the file being
/// served, which is exactly what every event was taken as before generations
/// existed.
///
/// Treating a missing stamp as stale was the first answer and it was wrong
/// twice over. A `file` event is how a file *ends*, so dropping an unstamped
/// one leaves the host waiting for a reply that has already been given, until a
/// deadline sixty times the per-case budget; and an event that names no request
/// is not evidence that it came from another one, so refusing it would trade a
/// wrong attribution for a hang.
const fn is_stale(stamp: u64, serving: u64) -> bool {
    stamp != 0 && stamp != serving
}

/// Most finished requests one file's notes will name.
///
/// Everything read from a worker is untrusted, and a generation is a number the
/// worker chose. Without a cap, a stream of events each claiming a different
/// one would grow the ledger without limit from inside a test. Eight covers the
/// shape this exists for — a file or two whose timers outlived them — and the
/// rest are counted rather than named.
const MAX_STALE_REQUESTS_NAMED: usize = 8;

/// Longest excerpt a note quotes from an event it dropped.
const MAX_STALE_EXCERPT_CHARS: usize = 80;

/// A short, single-line rendering of text the worker chose.
///
/// Only the first line, because a note is one line and the interesting part of
/// a `console.log` is its beginning. Control characters are left as they are:
/// the renderer escapes them on the way to a terminal, and doing it twice would
/// show a reader `\\n` where the test wrote a newline.
fn excerpt(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut kept: String = line.chars().take(MAX_STALE_EXCERPT_CHARS).collect();
    if kept.chars().count() < line.chars().count() {
        kept.push('…');
    }
    kept
}

/// What one finished request sent after it finished.
#[derive(Debug)]
struct StaleTally {
    /// How many of its events were dropped.
    count: usize,
    /// [`Event::describe`] of the first, which is the one worth quoting: it is
    /// the earliest thing the file did after it was supposed to be over.
    first: String,
}

/// Events the worker wrote for a request that had already finished.
///
/// They are dropped — see [`Worker::run_file`] for why each kind cannot go
/// anywhere else — and this ledger is what stops the dropping from being
/// silent. It is a tally rather than a note per event because one abandoned
/// `setInterval` produces thousands, and a file whose report is mostly an
/// apology about another file is not a report.
#[derive(Debug, Default)]
struct StaleEvents {
    /// Tallies by the generation that sent them, in order.
    named: BTreeMap<u64, StaleTally>,
    /// Events from requests beyond [`MAX_STALE_REQUESTS_NAMED`].
    beyond: usize,
}

impl StaleEvents {
    /// Record one dropped event.
    fn record(&mut self, generation: u64, description: String) {
        if let Some(tally) = self.named.get_mut(&generation) {
            tally.count += 1;
        } else if self.named.len() < MAX_STALE_REQUESTS_NAMED {
            self.named.insert(
                generation,
                StaleTally {
                    count: 1,
                    first: description,
                },
            );
        } else {
            self.beyond += 1;
        }
    }

    /// The notes to put in the report, given the files this worker has served.
    ///
    /// Deliberately outside [`MAX_OUTPUT_BYTES_PER_FILE`]: a budget on what a
    /// test may print must not be able to silence the explanation of why
    /// something is missing from the report. What it costs is bounded by
    /// [`MAX_STALE_REQUESTS_NAMED`] notes of [`MAX_STALE_EXCERPT_CHARS`] each.
    fn notes(&self, served: &[String]) -> Vec<OutputChunk> {
        let mut notes = Vec::new();
        for (generation, tally) in &self.named {
            // A generation the worker invented names no file, and a generation
            // is one-based, so both ends of the lookup can fail.
            let origin = generation
                .checked_sub(1)
                .and_then(|at| usize::try_from(at).ok())
                .and_then(|at| served.get(at))
                .map_or_else(
                    || format!("a request this worker never served ({generation})"),
                    |file| format!("`{file}`"),
                );
            // "That run of it" rather than "that file": a retry re-runs the
            // same path, so the file a straggler came from can be the file
            // being reported, one attempt earlier.
            let text = if tally.count == 1 {
                format!(
                    "[uf] one event arrived from {origin} after that run of it had finished, and \
                     was dropped rather than reported here: {}\n",
                    tally.first
                )
            } else {
                format!(
                    "[uf] {} events arrived from {origin} after that run of it had finished, and \
                     were dropped rather than reported here; the first was {}\n",
                    tally.count, tally.first
                )
            };
            notes.push(OutputChunk {
                stream: OutputStream::Stderr,
                text,
            });
        }
        if self.beyond > 0 {
            notes.push(OutputChunk {
                stream: OutputStream::Stderr,
                text: format!(
                    "[uf] and {} more from further runs this worker had already finished\n",
                    self.beyond
                ),
            });
        }
        notes
    }
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

/// One file's output: the notes about what was refused, then what it printed.
///
/// The notes come first, out of the order things happened in, because the
/// terminal draws only the first twenty lines of this section (`uf_cli`'s
/// `OUTPUT_LINES_SHOWN`) and a note saying why something is missing from the
/// report must not be what a chatty file pushes out of view. `--json` carries
/// both either way.
fn file_output(
    pending: &mut PendingOutput,
    stale: &StaleEvents,
    served: &[String],
) -> Vec<OutputChunk> {
    let mut output = stale.notes(served);
    output.append(&mut pending.drain());
    output
}

/// A worker process, and the thread reading its output.
///
/// The reader is a thread because a blocking read cannot be given a deadline;
/// the driver waits on the channel instead, which can.
#[derive(Debug)]
pub struct Worker {
    child: Child,
    /// Taken when the worker is asked to stop: closing it is how a worker is
    /// told there is no more work, and the only way it reaches its own exit
    /// handlers. See [`Worker::shutdown`].
    stdin: Option<ChildStdin>,
    events: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    /// Every file this worker has been asked to run, in the order it was asked.
    ///
    /// The index is the request's generation minus one, which is how a note
    /// about an event that arrived too late can name the file it came from
    /// rather than only the number. One short path per file the worker ran,
    /// against a source text per file the run already holds.
    served: Vec<String>,
    /// When the process was started, for [`Worker::start_micros`].
    spawned: Instant,
    /// How long the worker said its most recent file took, when it said.
    ///
    /// Cleared as each request is sent, so a request that ended any way other
    /// than with the worker's own answer never reports the previous file's.
    reported_micros: Option<u64>,
    /// Whether this worker has been stopped and waited for.
    ///
    /// Stopping twice is ordinary — a file that times out kills its worker,
    /// and the run retires the same worker afterwards — and the second time
    /// must do nothing at all. A pid stops naming this process the moment it
    /// is waited for, and the kernel is free to hand it to somebody else; a
    /// second signal, to a *group* named by that pid, would then be aimed at a
    /// stranger. [`std::process::Child::kill`] guards itself for exactly this
    /// reason, and this is the same guard for the part it cannot see.
    stopped: bool,
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
        // The project's own variables first: uf's own below name the project
        // root, the binary the worker transforms through, whether snapshots
        // may be rewritten and whether benchmarks run, and a `.env` file in a
        // cloned repository must not be able to answer any of those.
        for (name, value) in &command.env {
            process.env(name, value);
        }
        process
            .args(&command.leading_args)
            .arg(command.worker.as_str())
            .current_dir(command.root.as_std_path())
            .env("UF_PROJECT_ROOT", command.root.as_str())
            .env(
                "UF_UPDATE_SNAPSHOTS",
                if command.update_snapshots { "1" } else { "" },
            )
            .env("UF_TEST_BENCH", if command.bench { "1" } else { "" })
            // What makes `import.meta.uf.test` compile to uf's test API rather
            // than to `void 0`. It is set here — on the worker, by the runner —
            // and by nothing else, so a module compiled for a build can never
            // acquire an in-source block and a module compiled for a test run
            // can never lose one. The loader keys its cache on it too; see
            // `packages/host/internal/node-hooks.js`.
            .env("UF_IN_SOURCE_TESTS", "1")
            .env(KEEP_WORKERS, if command.keeps_workers() { "1" } else { "" })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // The worker's stderr is the host's own noise — an unhandled
            // warning, a deprecation — and it is not part of the report. It is
            // inherited so a person debugging sees it, rather than swallowed.
            .stderr(Stdio::inherit());
        // Each worker leads a process group of its own, so [`Worker::kill`] can
        // stop everything it started rather than only the process uf spawned.
        // Why that distinction is not academic is written down there: `node` on
        // `PATH` is frequently a shim that forks the real runtime, and the
        // process uf started is not the process running the test file.
        //
        // It also takes the workers out of the terminal's foreground group,
        // which is what a job-control signal — a Ctrl-C — is delivered to.
        // Nothing is lost: uf's own stdin is the pipe each worker reads, so a
        // uf that dies for any reason closes it, and `packages/test/worker.js`
        // exits on that end of input.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt as _;
            process.process_group(0);
        }
        if let Some(binary) = &command.uf_binary {
            process.env("UF_BINARY", binary.as_str());
        }
        if let Some(axe) = &command.axe {
            process.env("UF_AXE", axe.as_str());
        }
        if let Some(browser) = &command.browser {
            process.env(crate::browser::BROWSER_VARIABLE, browser.as_str());
        }
        if let Some(directory) = &command.coverage_dir {
            process.env("NODE_V8_COVERAGE", directory.as_str());
        }
        // Deno will not let a process whose run permission names programs
        // start one while a dynamic-loader variable is in its environment —
        // `LD_LIBRARY_PATH`, `LD_PRELOAD`, `DYLD_*` — unless `--allow-run` is
        // unscoped, and uf's never is. Measured on Deno 2.9.6: `spawnSync` then
        // answers with no process at all, so every module the transform cache
        // does not hold fails to load. `cargo test` sets `LD_LIBRARY_PATH` on
        // Linux for the process it tests, which is how CI met it, and a shell
        // that exports one for a native toolchain meets it the same way. A
        // worker has no use for them, because Deno has already loaded its own
        // libraries, so none is passed on — whether uf's environment or the
        // project's supplied it.
        if matches!(command.kind, HostKind::Deno) {
            let inherited = std::env::vars_os().map(|(name, _)| name);
            let declared = command
                .env
                .iter()
                .map(|(name, _)| std::ffi::OsString::from(name));
            for name in inherited.chain(declared) {
                if is_loader_variable(&name) {
                    process.env_remove(name);
                }
            }
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
            stdin: Some(stdin),
            events,
            reader: Some(reader),
            served: Vec::new(),
            // After the pipes and the reader rather than before the spawn: a
            // few hundred microseconds against a start-up of tens of
            // milliseconds, and an instant taken before `spawn` would be one
            // for a process that might never have existed.
            spawned: Instant::now(),
            reported_micros: None,
            stopped: false,
        })
    }

    /// Run one file and collect what it reported.
    ///
    /// `deadline` bounds the whole file. Passing it kills the worker, which is
    /// why the caller must replace it afterwards — [`FileOutcome`] carrying a
    /// [`FileStatus::TimedOut`] means this worker is gone.
    ///
    /// # Events from a file that has already finished
    ///
    /// A worker's events are one stream, and a file's code can outlive the
    /// file: a `setTimeout` nobody awaited fires while the *next* file is
    /// running, and everything it does arrives here. Each event carries the
    /// generation of the request it was written under, and one from any other
    /// request is dropped and tallied in [`StaleEvents`] rather than reported.
    ///
    /// All three kinds are dropped, and the reason is the same for all three
    /// even though the damage is not. The file an event belongs to has already
    /// been returned from this method and handed to the observer — the report
    /// is streamed, a file is drawn as it finishes — so "attribute it to the
    /// file it came from" is not on the table by the time the event is read.
    /// That leaves reporting it under the wrong file, or not at all:
    ///
    /// * A `test` event would add a case to a file that does not declare it,
    ///   counted in that file's totals and stamped with that file's path by
    ///   [`record_of`], turning one file red for something another file did.
    /// * An `output` event would be filed under whichever case of *this* file
    ///   shares the name it carries, and the same case name in two files is
    ///   ordinary — `describe("adds")` is not a unique identifier. Failing to
    ///   match is no better: the chunk becomes this file's own printing.
    /// * A `file` event is the worst of the three, because it is how a file
    ///   *ends*: accepting one would cut this file's report short and stamp it
    ///   with another file's status. The worker's unhandled-rejection handler
    ///   is exactly this shape — it writes a `file` event and exits — so a
    ///   promise the previous file abandoned used to fail the next one with a
    ///   message from code it does not contain. Dropped, this file keeps
    ///   running; if the worker then exits under it, it is reported as a worker
    ///   that died, which is what happened.
    pub fn run_file(
        &mut self,
        file: &str,
        relative: &str,
        filter: Option<&str>,
        case_timeout: Duration,
        deadline: Duration,
    ) -> FileOutcome {
        self.run(
            FileRun {
                file,
                relative,
                filter,
                part: None,
                case_timeout,
                deadline,
            },
            &mut |_| {},
        )
    }

    /// [`Worker::run_file`], for a [`FileRun`] — which may be one share of a
    /// file — calling `on_test` with each case as the worker reports it.
    ///
    /// `on_test` is how a case reaches the screen while the rest of its file
    /// is still running. It sees the record as it arrived, before a retry
    /// could change it; the [`FileOutcome`] is what the report is made of.
    pub fn run(&mut self, run: FileRun<'_>, on_test: &mut dyn FnMut(&TestRecord)) -> FileOutcome {
        let FileRun {
            file,
            relative,
            filter,
            part,
            case_timeout,
            deadline,
        } = run;
        // Pushed before the request is sent, so the generation is the file's
        // place in this worker's history whether or not the send succeeds. A
        // send that fails ends the worker anyway.
        self.served.push(relative.to_string());
        self.reported_micros = None;
        let generation = u64::try_from(self.served.len()).unwrap_or(u64::MAX);
        let request = Request {
            file,
            filter,
            timeout_ms: case_timeout.as_millis().min(u128::from(u64::MAX)) as u64,
            part,
            generation,
        };
        let mut line = match serde_json::to_string(&request) {
            Ok(line) => line,
            Err(error) => {
                return Self::host_failed(relative, format!("unencodable request: {error}"));
            }
        };
        line.push('\n');
        let Some(stdin) = self.stdin.as_mut() else {
            return Self::host_failed(
                relative,
                String::from("the worker has already been stopped"),
            );
        };
        if let Err(error) = stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
        {
            return Self::host_failed(relative, format!("could not reach the worker: {error}"));
        }

        let started = Instant::now();
        let mut records = Vec::new();
        // Whatever the file printed travels with it whatever happens to it: a
        // file that hung after printing is the case where the printing is most
        // of the evidence there is.
        let mut pending = PendingOutput::default();
        let mut stale = StaleEvents::default();
        loop {
            let remaining = deadline.checked_sub(started.elapsed());
            let Some(remaining) = remaining else {
                self.kill();
                return FileOutcome {
                    status: FileStatus::TimedOut {
                        budget_micros: u64::try_from(deadline.as_micros()).unwrap_or(u64::MAX),
                    },
                    records,
                    output: file_output(&mut pending, &stale, &self.served),
                };
            };
            match self.events.recv_timeout(remaining) {
                Ok(line) => match serde_json::from_str::<Event>(&line) {
                    Ok(event) if is_stale(event.generation(), generation) => {
                        stale.record(event.generation(), event.describe());
                    }
                    Ok(Event::Test(event)) => {
                        let mut record = record_of(relative, event);
                        record.output = pending.take(&record.name);
                        on_test(&record);
                        records.push(record);
                    }
                    Ok(Event::Output(event)) => pending.push(event),
                    // Only ever the answer to a request this method did not
                    // send; nothing about this file.
                    Ok(Event::Invalidated(_)) => {}
                    Ok(Event::File(event)) => {
                        self.reported_micros = event.duration_micros;
                        return FileOutcome {
                            status: file_status(event),
                            records,
                            output: file_output(&mut pending, &stale, &self.served),
                        };
                    }
                    Err(error) => {
                        self.kill();
                        return FileOutcome {
                            status: FileStatus::HostFailed {
                                message: format!("unreadable worker output: {error}: {line}"),
                            },
                            records,
                            output: file_output(&mut pending, &stale, &self.served),
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
                        output: file_output(&mut pending, &stale, &self.served),
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
                        output: file_output(&mut pending, &stale, &self.served),
                    };
                }
            }
        }
    }

    /// Tell a kept worker which files changed since it last ran one.
    ///
    /// `true` when the worker promises that the next file it imports sees every
    /// one of them: the changed modules, and each module it has loaded that
    /// reaches one, are imported afresh from then on, and nothing else is.
    /// `false` when it cannot — it was not started to be kept, its host has no
    /// in-thread loader to answer with, or a module in the way was loaded with
    /// `require()` — and when it does not answer within `timeout` or at all. A
    /// worker that says `false` must not run another file; the caller replaces
    /// it with a fresh one, which cannot have seen the old code.
    pub fn invalidate(&mut self, changed: &[String], timeout: Duration) -> bool {
        if changed.is_empty() {
            return true;
        }
        self.served.push(String::from("(the files that changed)"));
        let generation = u64::try_from(self.served.len()).unwrap_or(u64::MAX);
        let request = InvalidateRequest {
            invalidate: changed,
            generation,
        };
        let Ok(mut line) = serde_json::to_string(&request) else {
            return false;
        };
        line.push('\n');
        let Some(stdin) = self.stdin.as_mut() else {
            return false;
        };
        if stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .is_err()
        {
            return false;
        }
        let started = Instant::now();
        loop {
            let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
                return false;
            };
            match self.events.recv_timeout(remaining) {
                Ok(line) => match serde_json::from_str::<Event>(&line) {
                    Ok(Event::Invalidated(event)) if !is_stale(event.generation, generation) => {
                        return event.ok;
                    }
                    // Whatever a finished file left behind, arriving late. It
                    // belongs to nobody now, as in [`Worker::run_file`].
                    Ok(_) => {}
                    Err(_) => return false,
                },
                Err(_) => return false,
            }
        }
    }

    /// How many requests this worker has been sent: every file, every retry
    /// of one, and every [`Worker::invalidate`].
    ///
    /// Counted whether or not the request was answered, because it is the
    /// generation the next request will carry that this is a measure of. A
    /// [`crate::WorkerPool`] reads it to retire a kept worker after
    /// [`crate::MAX_REQUESTS_PER_KEPT_WORKER`], which is a bound on the modules
    /// a long watch session has loaded afresh and never let go of.
    pub fn served(&self) -> usize {
        self.served.len()
    }

    /// How long the worker said its most recent file took, when it said.
    pub fn reported_micros(&self) -> Option<u64> {
        self.reported_micros
    }

    /// What this worker cost to start, in microseconds.
    ///
    /// Only answered for a worker that has served exactly one request and was
    /// told how long it took: the time from spawning the process to now, less
    /// that. It is meant to be asked as the first request returns, which is
    /// when "now" is the moment the answer arrived; asked any later it would
    /// count the wait, and once a second request has been sent the question
    /// has no answer at all.
    pub fn start_micros(&self) -> Option<u64> {
        if self.served.len() != 1 {
            return None;
        }
        let reported = self.reported_micros?;
        let since_spawn = u64::try_from(self.spawned.elapsed().as_micros()).unwrap_or(u64::MAX);
        Some(since_spawn.saturating_sub(reported))
    }

    fn host_failed(file: &str, message: String) -> FileOutcome {
        let _ = file;
        FileOutcome {
            status: FileStatus::HostFailed { message },
            records: Vec::new(),
            output: Vec::new(),
        }
    }

    /// Let the worker exit on its own, then stop it.
    ///
    /// Closing stdin is the worker's signal that there is no more work
    /// (`packages/test/worker.js`'s `serve`), and it answers by draining its
    /// queue and calling `process.exit(0)`. That exit is the only moment a
    /// coverage document is written: `NODE_V8_COVERAGE` is flushed from an exit
    /// handler, and a process that is killed runs none. So a run that collects
    /// coverage has to wait for the worker to go rather than take it — the
    /// difference between a merged report and a report missing whichever
    /// workers happened to still be alive at the end.
    ///
    /// Bounded by `grace`, because "wait for it" and "a runner that can hang
    /// CI" are the same sentence otherwise. Whatever is left is killed, which
    /// costs that worker's counts and nothing else.
    ///
    /// Stdout closing is the signal, not a timer: the reader thread sees the
    /// end of that pipe exactly when the last process holding its write end is
    /// gone — after the worker's exit handlers have run. "The last process"
    /// rather than "the worker", because a `node` that is a version manager's
    /// shim has forked the runtime that does the work and handed it those
    /// pipes; [`Worker::kill`] is where that stops being a detail.
    pub fn shutdown(&mut self, grace: Duration) {
        drop(self.stdin.take());
        let until = Instant::now() + grace;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                // Something a finished file left behind. There is no file to
                // report it under any more; the worker is on its way out.
                Ok(_) => continue,
                Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout) => break,
            }
        }
        self.kill();
    }

    /// Stop the worker, waiting for its reader so no thread outlives the run.
    ///
    /// # Why the signal goes to the group and not to the child
    ///
    /// `node` on `PATH` is often not node. A version manager installs a shim —
    /// Volta's `volta-shim` is one, and any wrapper that wants to see how the
    /// tool exited is another — which *forks* the real runtime rather than
    /// `exec`ing it. uf's child is then the shim, and the process importing
    /// the test file is the shim's child, holding the same stdin and stdout
    /// pipes by inheritance.
    ///
    /// A `SIGKILL` addressed to the shim ends the shim and nothing else. The
    /// real runtime is reparented to `init` and keeps the write end of this
    /// worker's stdout open, so the reader thread's `read` never returns end
    /// of file and the `join` below never returns either. Every file had run,
    /// every result had been reported, and `uf test` sat there: no output, no
    /// exit, on every run, on any machine whose `node` is a forking shim.
    ///
    /// So the signal goes to the process group [`Worker::spawn`] gave this
    /// worker, which is the shim and everything it started.
    ///
    /// # And why the reader is joined with a bound
    ///
    /// The group covers a child that stays in it, which is every host uf
    /// starts. It cannot cover one that leaves — a `setsid` in a wrapper
    /// script — and the cost of being wrong about that must not be a runner
    /// that hangs. So the reader is given a moment to see the end of the pipe
    /// and is abandoned if it does not: a thread parked in `read` costs
    /// nothing at exit, and a report that arrives is worth more than a thread
    /// that is accounted for.
    pub fn kill(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        kill_group(&self.child);
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take()
            && self.reader_ended(READER_GRACE)
        {
            let _ = reader.join();
        }
    }

    /// Whether the reader thread is done, waiting up to `grace` for it.
    ///
    /// Asked through the channel rather than of the thread, because a
    /// [`std::thread::JoinHandle`] cannot be joined with a deadline: the
    /// reader owns the sending half, so a [`RecvTimeoutError::Disconnected`]
    /// is the reader's loop having ended and its sender having dropped, which
    /// is exactly the moment a `join` becomes free.
    fn reader_ended(&self, grace: Duration) -> bool {
        let until = Instant::now() + grace;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                // Whatever a stopped worker had already written. Nothing is
                // waiting for it any more.
                Ok(_) => continue,
                Err(RecvTimeoutError::Disconnected) => return true,
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
        false
    }
}

/// How long [`Worker::kill`] waits for the reader to see the end of the pipe.
///
/// Generous next to what it bounds — the group is signalled first, so the
/// usual answer arrives in well under a millisecond — and short next to a run.
/// It is only ever spent when something outlived the signal, which is the case
/// this exists to survive rather than to wait out.
const READER_GRACE: Duration = Duration::from_millis(500);

/// `SIGKILL` to the process group `child` leads.
///
/// A no-op where there are no process groups. [`Worker::spawn`] puts each
/// worker in its own, so the group is the worker and its descendants and
/// never uf itself; a group that has already gone answers `ESRCH`, which is
/// the same nothing as killing a child that has already exited.
#[cfg(unix)]
fn kill_group(child: &Child) {
    let Ok(pid) = i32::try_from(child.id()) else {
        return;
    };
    // SAFETY: `killpg` takes a process group id and a signal and returns a
    // code; it touches nothing this process owns. The id is this child's own,
    // which `spawn` made a group leader, so the group named here is the one uf
    // started and cannot be uf's own.
    unsafe {
        libc::killpg(pid, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_group(_child: &Child) {}

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
                Some("bench") => SkipReason::Bench,
                Some("not-bench") => SkipReason::NotBench,
                _ => SkipReason::Explicit,
            },
            message: event.message.filter(|message| !message.is_empty()),
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
        bench: event
            .samples
            .as_deref()
            .and_then(crate::report::BenchStats::from_samples),
    }
}

/// Whether `name` is a variable the platform's dynamic loader reads.
///
/// The `LD_` and `DYLD_` families, which Deno will not pass to a child started
/// under a scoped `--allow-run`; see [`Worker::spawn`].
fn is_loader_variable(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name.starts_with("LD_") || name.starts_with("DYLD_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The variables a Deno worker is started without, and nothing else: a
    /// name that merely contains `LD` is somebody's own.
    #[test]
    fn loader_variables_are_the_ld_and_dyld_families() {
        for name in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "DYLD_INSERT_LIBRARIES",
        ] {
            assert!(is_loader_variable(std::ffi::OsStr::new(name)), "{name}");
        }
        for name in ["PATH", "OLDPWD", "LDFLAGS", "UF_BINARY", "BUILD_LD_PATH"] {
            assert!(!is_loader_variable(std::ffi::OsStr::new(name)), "{name}");
        }
    }

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
            Utf8Path::new("/p/deno-preload.js"),
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
            Utf8Path::new("/p/deno-preload.js"),
        );

        assert_eq!(command.leading_args, ["--preload", "/p/bun-preload.js"]);
        assert!(command.loads_flow());
    }

    /// The browser is a host, and everything else about the seam is unchanged.
    ///
    /// The three claims worth pinning: the process uf starts is the driver's
    /// and not the browser's, the driver registers the same Flow loader Node
    /// does, and the browser binary travels on the command rather than in the
    /// arguments — because it is a property of the run, not of a file.
    #[test]
    fn a_browser_command_drives_a_browser_from_a_node_driver() {
        let command = HostCommand::new(
            HostKind::Browser,
            Utf8PathBuf::from("/usr/bin/node"),
            Utf8PathBuf::from("/p/browser-worker.js"),
            Utf8PathBuf::from("/p"),
        )
        .with_flow_loader(
            Utf8Path::new("@uniflowed/host/register"),
            Utf8Path::new("/p/bun-preload.js"),
            Utf8Path::new("/p/deno-preload.js"),
        )
        .with_browser(Utf8PathBuf::from("/usr/bin/chromium"));

        assert_eq!(HostKind::Browser.program(), "node");
        assert_eq!(
            command.leading_args,
            [
                "--enable-source-maps",
                "--import",
                "@uniflowed/host/register"
            ]
        );
        assert!(command.loads_flow(), "the page's modules are transformed");
        assert_eq!(
            command.browser.as_deref(),
            Some(Utf8Path::new("/usr/bin/chromium"))
        );
        // The one capability the browser does not have, and the message a
        // caller writes from it names a host that does.
        assert!(!command.can_collect_coverage());
        assert_eq!(HostKind::Browser.name(), "the browser");
        // The driver is Node, so it is the Node loader it registers and not
        // either preload.
        assert!(
            !command
                .leading_args
                .iter()
                .any(|argument| argument.ends_with("preload.js")),
            "{:?}",
            command.leading_args
        );
    }

    /// Deno preloads the hooks, and loads Flow on the worker it was given.
    ///
    /// The worker is the installed `@uniflowed/test/worker.js`, the same file
    /// Node and Bun run, rather than a compiled copy under `.uf/deno` — which
    /// is the difference a hook makes over the ahead-of-time pass this host
    /// used to need. See ubugeeei-prod/uf#246.
    #[test]
    fn a_deno_command_preloads_the_hooks() {
        let command = HostCommand::new(
            HostKind::Deno,
            Utf8PathBuf::from("/usr/bin/deno"),
            Utf8PathBuf::from("/p/node_modules/@uniflowed/test/worker.js"),
            Utf8PathBuf::from("/p"),
        )
        .with_flow_loader(
            Utf8Path::new("@uniflowed/host/register"),
            Utf8Path::new("/p/bun-preload.js"),
            Utf8Path::new("/p/deno-preload.js"),
        );

        assert_eq!(
            command.leading_args,
            ["run", "--preload", "/p/deno-preload.js"]
        );
        assert!(command.loads_flow());
        assert!(
            !command.can_collect_coverage(),
            "Deno's coverage profile is not a format uf reads"
        );
    }

    /// No `-A`, on any path through the builder.
    ///
    /// The flag is the one ubugeeei-prod/uf#246 says a Deno host is worth not
    /// building from: it grants everything and nothing later on the command
    /// line takes any of it back, so a run that starts there is sandboxed only
    /// while somebody remembers to narrow it. What a Deno run gets instead is
    /// the toolchain's own access, computed the same way a declared set is —
    /// which is why this asserts the *absence* rather than a particular
    /// replacement.
    #[test]
    fn a_deno_run_never_starts_from_all_access() {
        let command = HostCommand::new(
            HostKind::Deno,
            Utf8PathBuf::from("/usr/bin/deno"),
            Utf8PathBuf::from("/p/worker.js"),
            Utf8PathBuf::from("/p"),
        )
        .with_flow_loader(
            Utf8Path::new("a"),
            Utf8Path::new("b"),
            Utf8Path::new("/p/deno-preload.js"),
        )
        .with_permissions(vec![String::from("--allow-read=/p")]);

        assert!(
            !command.leading_args.iter().any(|argument| argument == "-A"),
            "{:?}",
            command.leading_args
        );
        assert_eq!(
            command.leading_args.first().map(String::as_str),
            Some("run")
        );
        assert_eq!(
            command.leading_args.last().map(String::as_str),
            Some("--allow-read=/p")
        );
    }

    #[test]
    fn a_node_permission_set_follows_the_loader_registration() {
        let command = HostCommand::new(
            HostKind::Node,
            Utf8PathBuf::from("/usr/bin/node"),
            Utf8PathBuf::from("/p/worker.js"),
            Utf8PathBuf::from("/p"),
        )
        .with_flow_loader(
            Utf8Path::new("@uniflowed/host/register"),
            Utf8Path::new("/p/bun-preload.js"),
            Utf8Path::new("/p/deno-preload.js"),
        )
        .with_permissions(vec![
            String::from("--permission"),
            String::from("--allow-fs-read=/p"),
        ]);

        assert_eq!(
            command.leading_args,
            [
                "--enable-source-maps",
                "--import",
                "@uniflowed/host/register",
                "--permission",
                "--allow-fs-read=/p",
            ]
        );
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
                samples: None,
                generation: 1,
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
                generation: 1,
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
            generation: 1,
        });
        // Two bytes of one character, with one byte of room: neither byte is
        // kept, because half of a character is not a character.
        pending.push(OutputEvent {
            stream: String::from("stdout"),
            test: None,
            text: String::from("é"),
            generation: 1,
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
            generation: 1,
        });
        for _ in 0..10_000 {
            pending.push(OutputEvent {
                stream: String::from("stdout"),
                test: None,
                text: String::from("é"),
                generation: 1,
            });
        }

        assert_eq!(pending.drain().len(), 1);
    }

    #[test]
    fn a_file_event_carries_the_duration_the_worker_measured() {
        // The worker's clock is the one that does not include the worker
        // booting, so it is the one a first file's duration is corrected by.
        let Ok(Event::File(event)) = serde_json::from_str::<Event>(
            r#"{"event":"file","status":"completed","durationMicros":1234,"generation":1}"#,
        ) else {
            panic!("a file line must parse as a file event");
        };
        assert_eq!(event.duration_micros, Some(1234));

        // A worker older than the field says nothing, and nothing is corrected
        // on its behalf.
        let Ok(Event::File(event)) = serde_json::from_str::<Event>(
            r#"{"event":"file","status":"completed","generation":1}"#,
        ) else {
            panic!("a file line must parse as a file event");
        };
        assert_eq!(event.duration_micros, None);
    }

    #[test]
    fn every_kind_of_event_says_which_request_it_came_from() {
        // The stamp is what makes the stream self-describing, so all three
        // kinds have to carry it — not only `output`, which is the kind the
        // issue was reported against.
        for line in [
            r#"{"event":"test","name":"a > b","status":"passed","generation":4}"#,
            r#"{"event":"file","status":"completed","generation":4}"#,
            r#"{"event":"output","stream":"stdout","text":"hi\n","generation":4}"#,
        ] {
            let event = serde_json::from_str::<Event>(line).expect("an event must parse");
            assert_eq!(event.generation(), 4, "in {line}");
        }
    }

    #[test]
    fn a_kept_worker_is_told_what_changed_in_a_request_of_its_own() {
        // No `file`: the worker's `serve` reads a request without one as the
        // list of files that changed, and answers it without running anything.
        let changed = [String::from("/project/dep.js")];
        let line = serde_json::to_string(&InvalidateRequest {
            invalidate: &changed,
            generation: 3,
        })
        .expect("a request serialises");
        assert_eq!(line, r#"{"invalidate":["/project/dep.js"],"generation":3}"#);
    }

    #[test]
    fn a_worker_answers_whether_it_can_promise_the_next_file_sees_the_change() {
        for (line, promised) in [
            (r#"{"event":"invalidated","ok":true,"generation":3}"#, true),
            (
                r#"{"event":"invalidated","ok":false,"generation":3}"#,
                false,
            ),
            // An answer that does not say is not a promise.
            (r#"{"event":"invalidated","generation":3}"#, false),
        ] {
            let Ok(Event::Invalidated(event)) = serde_json::from_str::<Event>(line) else {
                panic!("an invalidation answer must parse: {line}");
            };
            assert_eq!(event.ok, promised, "in {line}");
            assert_eq!(event.generation, 3);
        }
    }

    #[test]
    fn a_worker_that_was_not_kept_is_told_so_and_records_no_graph() {
        let node = HostCommand::new(
            HostKind::Node,
            Utf8PathBuf::from("node"),
            Utf8PathBuf::from("worker.js"),
            Utf8PathBuf::from("."),
        );
        assert!(!node.keeps_workers());
        let kept = node.with_kept_workers(true);
        assert!(kept.keeps_workers());
        // Asked twice, said once; taken back, gone.
        let again = kept.with_kept_workers(true);
        assert_eq!(
            again
                .env
                .iter()
                .filter(|(name, _)| name == KEEP_WORKERS)
                .count(),
            1
        );
        assert!(!again.with_kept_workers(false).keeps_workers());
    }

    #[test]
    fn an_event_from_a_request_that_has_finished_is_stale() {
        assert!(is_stale(1, 2), "the file before this one");
        assert!(!is_stale(2, 2), "the file being served");
        // A worker cannot be ahead of the host, so this is a worker saying
        // something impossible; it is still not this file's, which is the only
        // question being asked.
        assert!(is_stale(9, 2), "a request that has not been sent");
    }

    #[test]
    fn an_event_that_names_no_request_is_the_file_being_served() {
        // Not stale, deliberately. `0` is what an unstamped event carries — a
        // worker older than the field, or a host whose storage did not reach
        // the callback — and dropping a `file` event on that basis would leave
        // the run waiting for an answer it had already been given.
        assert!(!is_stale(0, 1));
        assert!(!is_stale(0, 7));
    }

    #[test]
    fn a_dropped_event_is_counted_and_named_after_the_file_it_came_from() {
        let served = vec![String::from("src/a.test.js"), String::from("src/b.test.js")];
        let mut stale = StaleEvents::default();
        for text in ["from the previous file\n", "and again\n"] {
            let line = output_line(Some("a > b"), text.trim_end());
            let event = serde_json::from_str::<Event>(&line).expect("an event must parse");
            stale.record(1, event.describe());
        }

        let notes = stale.notes(&served);
        assert_eq!(notes.len(), 1, "one note per originating request");
        assert_eq!(notes[0].stream, OutputStream::Stderr);
        let text = &notes[0].text;
        assert!(
            text.starts_with("[uf] 2 events arrived from `src/a.test.js`"),
            "{text}"
        );
        // The first is quoted, because it is the earliest thing the file did
        // after it was supposed to be over; the second is only counted.
        assert!(text.contains("from the previous file"), "{text}");
        assert!(!text.contains("and again"), "{text}");
        assert!(text.ends_with('\n'), "a note is one line: {text}");
    }

    #[test]
    fn a_note_names_the_kind_of_event_it_dropped() {
        let served = vec![String::from("src/a.test.js")];
        for (line, expected) in [
            (
                r#"{"event":"test","name":"a > b","status":"failed","generation":1}"#,
                "the case \"a > b\"",
            ),
            (
                r#"{"event":"file","status":"run-failed","message":"unhandled rejection: boom","generation":1}"#,
                "the file result \"run-failed\": unhandled rejection: boom",
            ),
        ] {
            let event = serde_json::from_str::<Event>(line).expect("an event must parse");
            let mut stale = StaleEvents::default();
            stale.record(1, event.describe());
            let notes = stale.notes(&served);
            assert!(notes[0].text.contains(expected), "{}", notes[0].text);
        }
    }

    #[test]
    fn a_note_quotes_a_bounded_amount_of_what_a_test_wrote() {
        // The note is the runner's own line about untrusted text, so the
        // untrusted text must not be able to become the line. Both bounds are
        // exercised: one long write, and one that spans many lines.
        let long = "x".repeat(4096);
        assert_eq!(excerpt(&long).chars().count(), MAX_STALE_EXCERPT_CHARS + 1);
        assert!(excerpt(&long).ends_with('…'));
        assert_eq!(excerpt("first\nsecond\nthird\n"), "first");
        assert_eq!(excerpt(""), "");
    }

    #[test]
    fn the_number_of_requests_a_note_names_is_bounded() {
        // A generation is a number the worker chose, so a test that writes one
        // event per invented generation would otherwise grow this ledger for
        // as long as the file it is running beside lasts.
        let mut stale = StaleEvents::default();
        for generation in 100..10_000u64 {
            stale.record(generation, String::from("output \"x\""));
        }

        let notes = stale.notes(&[]);
        assert_eq!(notes.len(), MAX_STALE_REQUESTS_NAMED + 1, "and one summary");
        // A generation this worker never served names no file, and says so
        // rather than pointing at whichever file happens to be at that index.
        assert!(
            notes[0]
                .text
                .contains("a request this worker never served (100)"),
            "{}",
            notes[0].text
        );
        assert!(
            notes[MAX_STALE_REQUESTS_NAMED]
                .text
                .contains("and 9892 more from further runs"),
            "{}",
            notes[MAX_STALE_REQUESTS_NAMED].text
        );
    }

    #[test]
    fn the_notes_come_before_what_the_file_printed() {
        // The terminal draws only the first lines of the output section, so a
        // note explaining what is missing from the report has to be above the
        // printing rather than after it.
        let mut pending = PendingOutput::default();
        let Ok(Event::Output(event)) = serde_json::from_str::<Event>(&output_line(None, "hello"))
        else {
            panic!("an output line must parse as an output event");
        };
        pending.push(event);
        let mut stale = StaleEvents::default();
        stale.record(1, String::from("output \"gone\""));

        let output = file_output(&mut pending, &stale, &[String::from("src/a.test.js")]);
        assert_eq!(output.len(), 2);
        assert!(output[0].text.starts_with("[uf] "), "{}", output[0].text);
        assert_eq!(output[1].text, "hello\n");
    }

    #[test]
    fn a_file_that_dropped_nothing_says_nothing() {
        let mut pending = PendingOutput::default();
        assert!(file_output(&mut pending, &StaleEvents::default(), &[]).is_empty());
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
                    samples: None,
                    generation: 1,
                },
            );
            assert_eq!(
                record.status,
                TestStatus::Skipped {
                    reason: expected,
                    message: None
                }
            );
        }
    }

    #[test]
    fn explicit_skip_messages_survive_the_round_trip() {
        let record = record_of(
            "a.js",
            TestEvent {
                name: String::from("t"),
                line: 1,
                column: 1,
                duration_micros: 0,
                status: String::from("skipped"),
                reason: Some(String::from("explicit")),
                message: Some(String::from("Deno has no synchronous module hook")),
                stack: None,
                expected: None,
                received: None,
                site: None,
                samples: None,
                generation: 1,
            },
        );
        assert_eq!(
            record.status,
            TestStatus::Skipped {
                reason: SkipReason::Explicit,
                message: Some(String::from("Deno has no synchronous module hook"))
            }
        );
    }
}
