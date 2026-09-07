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

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::env_files::ProjectEnv;
use uf_config::load_config;
use uf_project::{ProjectFile, scan_source_files};
use uf_term::PhaseTimer;
use uf_test::{
    Bail, Concurrency, FileStatus, HostCommand, HostKind, LockedObserver, NativeTestRunnerPlan,
    RetryPolicy, RunOptions, TestFile, TestFilter, TestRunReport, TestRunner, TestTimings,
    WatchOptions, load_timings, save_timings,
};

use crate::cli::{CoverageReporterArg, ResultReporterArg};
use crate::commands::builder::uniflowed_package;
use crate::commands::vite::resolve_host;

use crate::support::{TEST, plural, project_env, quoted_list, selects, unreadable_lines};
use crate::ui::Ui;

mod coverage;
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
    /// Run in this mode instead of `test`.
    pub(crate) mode: Option<String>,
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
    /// Rewrite any snapshot that did not match.
    pub(crate) update_snapshots: bool,
    /// Run at most this many files at once.
    pub(crate) threads: Option<usize>,
    /// How often watch mode looks for changes, in milliseconds.
    pub(crate) watch_interval: Option<u64>,
    /// Measure which Flow lines the suite executed.
    pub(crate) coverage: bool,
    /// Which coverage reports to write, overriding the project's config.
    pub(crate) coverage_reporters: Vec<CoverageReporterArg>,
    /// Where coverage reports go, overriding the project's config.
    pub(crate) coverage_dir: Option<String>,
    /// A machine-readable shape for the run's results.
    pub(crate) reporter: Option<ResultReporterArg>,
    /// Where `reporter` writes.
    pub(crate) reporter_outfile: Option<String>,
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
    // A coverage report is a statement about a whole suite, and watch mode does
    // not run one: it runs whatever the last edit invalidated. Reporting "68%"
    // after re-running three files would be a number about three files wearing
    // the project's name, and a threshold checked against it would fail a
    // developer's loop for a reason that has nothing to do with their edit.
    if args.watch && args.coverage {
        bail!(
            "--watch and --coverage cannot be combined: watch mode re-runs only the files an \
             edit affected, so its coverage would not be the project's"
        );
    }

    let resolved = load_config(cwd)?;
    let root = resolved.root.clone();
    let scan = scan_source_files(&root, &resolved.config)?;
    let unreadable = unreadable_lines(&scan.unreadable);
    let files = scan.files;
    // Before anything is run. A file uf could not read might have been a test,
    // and a test that silently did not run is the worst thing a runner can do.
    if !unreadable.is_empty() {
        crate::commands::lint::render_unreadable(ui, &unreadable);
        bail!("{} could not be read", plural(unreadable.len(), "file"));
    }

    // A path argument that names nothing in the project is a typo, and a typo
    // that answers "0 failures" is the one mistake a test runner must never
    // make: `uf test pacakges/ui` was a green run over nothing at all. `uf
    // lint`, `uf fmt` and `uf check` already refuse the same argument, in the
    // same words.
    //
    // Naming a real directory that happens to hold no tests is not a typo, so
    // it stays a green run of zero tests — that is what `uf test` on a project
    // with no tests yet has always answered. Watch mode is excluded for the
    // same reason: it is a loop over files that do not exist yet.
    if !args.watch
        && !args.paths.is_empty()
        && !files
            .iter()
            .any(|file| selects(&args.paths, &file.relative_path))
    {
        bail!("no file matched {}", quoted_list(&args.paths));
    }

    if args.list {
        return render_list(ui, &root, &files, &args.filter());
    }
    // `test` rather than `development`, so `.env.test` is a file that means
    // something — the mode Vitest runs in, for the same reason: a suite that
    // talks to the development database is a suite that can destroy it.
    let env = project_env(&resolved, args.mode.as_deref(), TEST)?;
    if args.watch {
        return watch::watch(ui, &root, resolved.config, &env, args);
    }

    let mut host =
        test_host(&root, &resolved.config, &env)?.with_snapshot_updates(args.update_snapshots);

    // Every JavaScript file the project has, before discovery narrows it to the
    // ones that declare tests: a file no test imports never becomes a script,
    // so measuring alone cannot see it and the coverage report has to be told.
    //
    // JavaScript only. A `package.json` or a stylesheet is a project file that
    // is never executed, so "the suite never loaded it" is true of every one of
    // them and says nothing.
    let project_paths: Vec<String> = files
        .iter()
        .filter(|file| file.kind == uf_project::SourceKind::JavaScript)
        .map(|file| file.relative_path.clone())
        .collect();

    let settings = &resolved.config.test.coverage;
    let raw = if args.coverage || settings.enabled {
        if !host.can_collect_coverage() {
            bail!(
                "`uf test --coverage` needs Node.js: coverage is V8's own count, written out \
                 through `NODE_V8_COVERAGE` and mapped back through the source map the Node \
                 loader attaches. {} provides neither, and reporting zeroes would be worse than \
                 saying so.",
                host.kind.program()
            );
        }
        let raw = coverage::RawCoverage::create(&root)?;
        host = host.with_coverage_dir(raw.directory().to_path_buf());
        Some(raw)
    } else {
        None
    };

    let files = test_bearing(files);
    let mut timer = PhaseTimer::start();
    let (timings, timing_note) = read_timings(&root);
    let report = timer.measure("run", || {
        run_once(ui, &root, &host, &files, &args, timings.clone())
    })?;

    let collected = match &raw {
        Some(raw) => Some(timer.measure("coverage", || {
            coverage::collect(
                &root,
                settings,
                raw,
                &coverage::directory_for(&root, settings, args.coverage_dir.as_deref()),
                &args.coverage_reporters,
                &project_paths,
            )
        })?),
        None => None,
    };
    let duration = timer.total();

    if let Some(ResultReporterArg::Junit) = args.reporter {
        write_results_report(&root, &args, &report)?;
    }

    let recorded = record_timings(&root, timings, &report, &files);
    if args.json {
        ui.json(&test_payload(
            &report,
            collected.as_ref().map(|(coverage, _)| coverage),
        ))?;
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
            collected.as_ref().map(|(_, section)| section),
        );
    }

    finish(
        &report,
        collected
            .as_ref()
            .map_or(&[][..], |(_, section)| &section.violations),
    )
}

/// Write the run's results in the shape a CI system already parses.
///
/// An outfile rather than stdout, and `--reporter` requires one. `uf test
/// --json` already owns stdout for a machine, and putting a second document
/// there would mean deciding which of the two a caller meant; a CI system, on
/// the other hand, is configured with a *path* — `junit.xml`, collected after
/// the step — so the file is what it wants anyway.
fn write_results_report(root: &Utf8Path, args: &TestArgs, report: &TestRunReport) -> Result<()> {
    let Some(outfile) = args.reporter_outfile.as_deref() else {
        bail!("--reporter needs --reporter-outfile");
    };
    let path = Utf8Path::new(outfile);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if let Some(parent) = path.parent()
        && !parent.as_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("could not create {parent}"))?;
    }
    std::fs::write(&path, uf_test::junit(report))
        .with_context(|| format!("could not write {path}"))?;
    Ok(())
}

/// The host `uf test` runs its workers on.
///
/// The worker and the loader both live in the project's `node_modules`, so a
/// project that has not installed its dependencies is told that rather than
/// being handed a module-not-found from inside a worker.
pub(crate) fn test_host(
    root: &Utf8Path,
    config: &uf_config::UniflowedConfig,
    env: &ProjectEnv,
) -> Result<HostCommand> {
    let host = resolve_host(config)?;
    // The loader, not the bundler. `uf test` transforms through `uf transform`
    // and runs on a Capability JS Host; nothing in that path is Vite's, and
    // asking for `@uniflowed/vite` made a test run depend on a bundler it never
    // loads.
    let loader = uniflowed_package(root, "host", "register.js")?;
    let worker = loader
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
            Utf8Path::new("@uniflowed/host/register"),
            &loader.join("bun-preload.js"),
        )
        // Every worker gets the project's `.env` values, so a test reads
        // `process.env.DATABASE_URL` and finds what `uf dev` would have found.
        .with_env(env.exported());
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
            // A file with a declaration discovery could not read is still a
            // test file: the worker imports it and finds whatever registers.
            // Requiring a *readable* case meant `it(name, …)` in a loop made a
            // file vanish, and the run said "0 passed" and exited 0.
            let plan = uf_test::discover_tests(&file.relative_path, &file.source);
            plan.runnable_count() > 0 || !plan.unsupported.is_empty()
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
///
/// A coverage threshold fails the run exactly as a failing test does, and it is
/// checked *after* the tests: a suite that is red has a reason to be red
/// already, and adding "and coverage is 61%" to it would bury the failure that
/// caused the number.
fn finish(report: &TestRunReport, violations: &[uf_test::ThresholdViolation]) -> Result<()> {
    let summary = &report.summary;
    if summary.is_success() {
        if violations.is_empty() {
            return Ok(());
        }
        let mut message = format!(
            "uf test did not reach {}",
            plural(violations.len(), "coverage threshold")
        );
        for violation in violations {
            message.push_str("\n  ");
            message.push_str(&violation.describe());
        }
        bail!(message);
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

#[cfg(test)]
mod tests {
    use super::*;
    use uf_project::SourceKind;

    fn file(path: &str, source: &str) -> ProjectFile {
        ProjectFile {
            relative_path: path.to_owned(),
            absolute_path: Utf8PathBuf::from(path),
            source: source.to_owned(),
            kind: SourceKind::JavaScript,
        }
    }

    #[test]
    fn a_file_whose_names_are_not_literals_is_still_a_test_file() {
        // Discovery reads names; a name built in a loop cannot be read. The
        // file still holds tests, and the worker is what finds them — so
        // filtering on a *readable* case reported "0 passed" and exited 0 for
        // a file with two failing tests in it.
        let files = test_bearing(vec![
            file(
                "loop.test.js",
                "for (const name of ['a']) {\n  it(name, () => {});\n}\n",
            ),
            file("plain.test.js", "it('runs', () => {});\n"),
            file("component.js", "export const Button = () => null;\n"),
        ]);

        let kept: Vec<&str> = files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(kept, vec!["loop.test.js", "plain.test.js"]);
    }
}
