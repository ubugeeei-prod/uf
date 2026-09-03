//! `uf test`: schedule the suite, run it wide, and report what happened.
//!
//! The command owns three decisions and delegates everything else:
//!
//! * **What to run** — path and name filters, `.only` / `.skip` / `.todo`, and
//!   in watch mode the set the import graph says an edit invalidated.
//! * **In what order** — longest-first, from durations the previous run wrote to
//!   `.uf/test-timings.json`. A cold suite falls back to file size.
//! * **How to say it** — a live progress line on stderr, code frames under the
//!   failures, and a summary; or, under `--json`, one machine-readable document
//!   on stdout and nothing else.
//!
//! Executing a test body is not one of them. That happens on the project's
//! JavaScript host, in `@uniflowed/test`'s worker, which imports each file
//! through the same `uf transform` a build uses — see [`uf_test::host`].

use std::num::NonZeroUsize;
use std::time::Duration;

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::load_config;
use uf_project::{ProjectFile, collect_source_files};
use uf_term::PhaseTimer;
use uf_test::{
    Bail, Concurrency, FileStatus, HostCommand, HostKind, LockedObserver, NativeTestRunnerPlan,
    RetryPolicy, RunOptions, TestFile, TestFilter, TestRunReport, TestRunner, TestTimings,
    WatchOptions, load_timings, save_timings,
};

use crate::commands::vite::{package_dir, resolve_host};

use crate::support::plural;
use crate::ui::Ui;

mod payload;
mod render;
mod watch;

use payload::test_payload;
use render::{render_list, render_report};

/// How many of the slowest files are named in the summary.
const SLOWEST_SHOWN: usize = 5;

/// Everything `uf test` was asked to do.
#[derive(Debug, Clone, Default)]
pub(crate) struct TestArgs {
    /// List what would run instead of running it.
    pub(crate) list: bool,
    /// Re-run affected tests when a file changes.
    pub(crate) watch: bool,
    /// Emit machine-readable JSON on stdout.
    pub(crate) json: bool,
    /// Keep only tests whose fully qualified name contains this pattern.
    pub(crate) filter: Option<String>,
    /// Stop once this many tests have failed.
    pub(crate) bail: Option<usize>,
    /// Re-run a failing test up to this many more times.
    pub(crate) retry: u32,
    /// Run at most this many files at once.
    pub(crate) threads: Option<usize>,
    /// How often watch mode looks for changes, in milliseconds.
    pub(crate) watch_interval: Option<u64>,
    /// Only run files whose path contains one of these patterns.
    pub(crate) paths: Vec<String>,
}

impl TestArgs {
    /// The run options this invocation asks for.
    fn options(&self) -> RunOptions {
        RunOptions {
            concurrency: match self.threads.and_then(NonZeroUsize::new) {
                Some(threads) => Concurrency::Fixed(threads),
                None => Concurrency::Auto,
            },
            bail: self.bail.map(Bail::after).unwrap_or_default(),
            retry: if self.retry == 0 {
                RetryPolicy::none()
            } else {
                RetryPolicy::retries(self.retry)
            },
            ..RunOptions::default()
        }
    }

    /// The filter this invocation asks for.
    fn filter(&self) -> TestFilter {
        let filter = TestFilter::new().with_paths(self.paths.iter().map(String::as_str));
        match &self.filter {
            Some(pattern) => filter.with_name(pattern),
            None => filter,
        }
    }

    /// The watch settings this invocation asks for.
    fn watch_options(&self) -> WatchOptions {
        match self.watch_interval {
            Some(millis) => WatchOptions::with_interval(Duration::from_millis(millis)),
            None => WatchOptions::default(),
        }
    }
}

pub(crate) fn test(cwd: &Utf8Path, ui: &mut Ui, args: TestArgs) -> Result<()> {
    // Watch mode prints a report per change; `--json` promises one document and
    // nothing else. Rather than quietly picking one, say so.
    if args.watch && args.json {
        bail!("--watch and --json cannot be combined: watch mode reports once per change");
    }

    let resolved = load_config(cwd)?;
    let root = resolved.root.clone();
    let files = collect_source_files(&root, &resolved.config)?;

    if args.list {
        return render_list(ui, &root, &files, &args.filter());
    }
    if args.watch {
        return watch::watch(ui, &root, resolved.config, args);
    }

    let host = test_host(&root, &resolved.config)?;
    let files = test_bearing(files);
    let mut timer = PhaseTimer::start();
    let (timings, timing_note) = read_timings(&root);
    let report = timer.measure("run", || {
        run_once(ui, &root, &host, &files, &args, timings.clone())
    })?;
    let duration = timer.total();

    let recorded = record_timings(&root, timings, &report, &files);
    if args.json {
        ui.json(&test_payload(&report))?;
    } else {
        render_report(
            ui,
            &root,
            &files,
            &report,
            timer.phases(),
            duration,
            &args,
            &host,
            timing_note.as_deref(),
            recorded.as_deref(),
        );
    }

    finish(&report)
}

/// The host `uf test` runs its workers on.
///
/// The worker and the loader both live in the project's `node_modules`, so a
/// project that has not installed its dependencies is told that rather than
/// being handed a module-not-found from inside a worker.
pub(crate) fn test_host(
    root: &Utf8Path,
    config: &uf_config::UniflowedConfig,
) -> Result<HostCommand> {
    let host = resolve_host(config)?;
    let vite = package_dir(root)?;
    let worker = vite
        .parent()
        .map(|scope| scope.join("test/worker.js"))
        .filter(|worker| worker.is_file())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`@uniflowed/test` is not installed for {root}; add it to the project's \
                 dependencies and run the package manager (`uf install`)"
            )
        })?;

    let kind = match host.kind {
        uf_config::CapabilityJsHost::Node => HostKind::Node,
        uf_config::CapabilityJsHost::Bun => HostKind::Bun,
        uf_config::CapabilityJsHost::Deno => HostKind::Deno,
    };
    let host_name = host.name();
    let mut command = HostCommand::new(kind, host.program, worker, root.to_path_buf())
        .with_flow_loader(
            Utf8Path::new("@uniflowed/vite/register"),
            &vite.join("bun-preload.js"),
        );
    // The worker transforms through the binary that started it, never a
    // different `uf` that happens to be on PATH.
    if let Ok(binary) = std::env::current_exe()
        && let Ok(binary) = Utf8PathBuf::from_path_buf(binary)
    {
        command = command.with_uf_binary(binary);
    }
    if !command.loads_flow() {
        bail!(
            "`uf test` cannot run on {} yet: it has no Flow loader, so a test file written in \
             Flow could not be imported. Install Node.js or Bun.",
            host_name
        );
    }
    Ok(command)
}

/// Run the suite once, drawing a progress line while it goes.
pub(crate) fn run_once(
    ui: &Ui,
    root: &Utf8Path,
    host: &HostCommand,
    files: &[ProjectFile],
    args: &TestArgs,
    timings: TestTimings,
) -> Result<TestRunReport> {
    let sources = test_files(root, files);
    let runner = TestRunner::new()
        .with_options(args.options())
        .with_filter(args.filter())
        .with_timings(timings)
        .with_host(host.clone());

    let mut progress = ui.progress();
    if !progress.is_enabled() {
        return Ok(runner.run(&sources)?);
    }

    let mut line = String::new();
    let observer = LockedObserver::new(move |completed: usize, total: usize, report: &_| {
        let report: &uf_test::FileReport = report;
        line.clear();
        line.push_str(&completed.to_string());
        line.push('/');
        line.push_str(&total.to_string());
        line.push(' ');
        line.push_str(&report.file);
        progress.tick(&line);
    });
    Ok(runner.run_observed(&sources, &observer)?)
}

/// The files that declare at least one test.
///
/// Every module in a project is not a test: importing one to find out would
/// run its side effects and cost a process, and a config file or a component
/// has nothing to report. Discovery answers the question by reading, which is
/// the same answer `uf test --list` shows.
pub(crate) fn test_bearing(files: Vec<ProjectFile>) -> Vec<ProjectFile> {
    files
        .into_iter()
        .filter(|file| {
            uf_test::discover_tests(&file.relative_path, &file.source).runnable_count() > 0
        })
        .collect()
}

/// Every collected file, as the runner wants them.
///
/// The worker imports by absolute path — a relative one would resolve against
/// the worker's own directory — while the report keeps the relative path a
/// person reads.
pub(crate) fn test_files(root: &Utf8Path, files: &[ProjectFile]) -> Vec<TestFile> {
    files
        .iter()
        .map(|file| {
            TestFile::new(
                file.relative_path.clone(),
                root.join(&file.relative_path),
                file.source.clone(),
            )
        })
        .collect()
}

/// Read recorded timings, falling back to a cold schedule on anything the
/// validator rejects.
///
/// The document is untrusted input, so a bad one is never fatal: the run just
/// schedules by size and says why.
pub(crate) fn read_timings(root: &Utf8Path) -> (TestTimings, Option<String>) {
    match load_timings(root) {
        Ok((timings, audit)) if audit.is_clean() => (timings, None),
        Ok((timings, audit)) => (
            timings,
            Some(format!(
                "ignored {} in .uf/test-timings.json",
                plural(audit.rejected(), "unusable entry")
            )),
        ),
        Err(error) => (
            TestTimings::new(),
            Some(format!("scheduling cold: {error}")),
        ),
    }
}

/// Record this run's durations for the next one.
///
/// Returns a note when the cache could not be written; failing to write a cache
/// is never a reason to fail a test run.
pub(crate) fn record_timings(
    root: &Utf8Path,
    mut timings: TestTimings,
    report: &TestRunReport,
    files: &[ProjectFile],
) -> Option<String> {
    for file in &report.files {
        if file.status == FileStatus::Completed {
            timings.record(&file.file, file.duration_micros);
        }
    }
    timings.retain_files(|recorded| {
        files
            .iter()
            .any(|file| file.relative_path.as_str() == recorded)
    });

    save_timings(root, &timings)
        .err()
        .map(|error| format!("could not record timings: {error}"))
}

/// The path recorded timings live at, for the summary block.
pub(crate) fn timings_label(root: &Utf8Path) -> Utf8PathBuf {
    uf_test::timings_path(root)
}

/// The runner contract reported alongside every run.
pub(crate) fn runner_plan() -> NativeTestRunnerPlan {
    NativeTestRunnerPlan::self_hosted()
}

/// Turn a report into the command's exit status.
fn finish(report: &TestRunReport) -> Result<()> {
    let summary = &report.summary;
    if summary.is_success() {
        return Ok(());
    }
    if summary.failed > 0 {
        bail!("uf test failed with {}", plural(summary.failed, "failure"));
    }
    if summary.failed_files > 0 {
        bail!(
            "uf test could not run {}",
            plural(summary.failed_files, "file")
        );
    }
    if summary.unsupported_declarations > 0 {
        bail!(
            "uf test found {}",
            plural(
                summary.unsupported_declarations,
                "unsupported test declaration"
            )
        );
    }
    bail!("uf test stopped early because --bail was reached");
}
