//! `uf test`: schedule the suite, run it wide, and report what happened.
//!
//! The command owns three decisions and delegates everything else:
//!
//! * **What to run** — path and name filters, `.only` / `.skip` / `.todo`, and
//!   in watch mode the set the import graph says an edit invalidated.
//! * **In what order** — longest-first, from durations the previous run wrote to
//!   `.uf/test-timings.json`. A cold suite falls back to file size.
//! * **How to say it** — a line per file as each one finishes, with code frames
//!   under its failures, a live progress line on stderr, and a summary; or,
//!   under `--json`, one machine-readable document on stdout and nothing else.
//!
//! Executing a test body is not one of them. That happens on the project's
//! JavaScript host, in `@uniflowed/test`'s worker, which imports each file
//! through the same `uf transform` a build uses — see [`uf_test::host`].

use std::num::NonZeroUsize;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::env_files::ProjectEnv;
use uf_config::{
    FrameworkPreset, NativeTestApplicationTarget, Permissions, ToolchainAccess, UniflowedConfig,
    load_config,
};
use uf_project::{ProjectFile, scan_existing_selected_source_files, scan_selected_source_files};
use uf_runtime::RuntimeHost;
use uf_term::PhaseTimer;
use uf_test::{
    Bail, Concurrency, FileStatus, HostCommand, HostKind, NativeTestRunnerPlan, PlannedTestFile,
    RetryPolicy, RunOptions, TestApplicationTarget, TestFile, TestFilter, TestPlan, TestRunReport,
    TestRunner, TestTimings, WatchOptions, load_timings, save_timings,
};

use crate::cli::{CoverageReporterArg, ResultReporterArg};
use crate::commands::builder::uniflowed_package;
use crate::commands::runtimes;
use crate::commands::vite::{Host, find_program};

use crate::support::{
    TEST, ignore_deprecation, plural, project_env, quoted_list, render_ignore_deprecation, selects,
    unreadable_lines,
};
use crate::ui::Ui;

mod bench;
pub(crate) mod bun;
mod changed;
mod coverage;
mod payload;
mod render;
mod shards;
mod stream;
mod watch;

use payload::test_payload;
use render::{render_list, render_report};

/// How many of the slowest files are named in the summary.
const SLOWEST_SHOWN: usize = 5;

#[derive(Debug, Clone)]
struct PlannedProjectFile {
    file: ProjectFile,
    plan: TestPlan,
}

/// Everything `uf test` was asked to do.
#[derive(Debug, Clone, Default)]
pub(crate) struct TestArgs {
    /// Override the configured runtime with an installed host.
    pub(crate) host: Option<crate::cli::TestHostArg>,
    /// List what would run instead of running it.
    pub(crate) list: bool,
    /// Run in this mode instead of `test`.
    pub(crate) mode: Option<String>,
    /// Re-run affected tests when a file changes.
    pub(crate) watch: bool,
    /// Run only the test files a change since this ref reaches.
    pub(crate) changed: Option<String>,
    /// Run only this part of a suite split across machines.
    pub(crate) shard: Option<uf_test::Shard>,
    /// Report the shard records in this directory as one run, running nothing.
    pub(crate) merge_shards: Option<String>,
    /// Run the benchmarks in place of the tests.
    pub(crate) bench: bool,
    /// The baseline benchmarks are compared with, instead of the default one.
    pub(crate) baseline: Option<String>,
    /// Save this run's benchmark medians as the baseline.
    pub(crate) save_baseline: bool,
    /// How far past its baseline a median may go, in per cent.
    pub(crate) bench_threshold: Option<u32>,
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
    /// Run every file in a real browser instead of on Node's DOM shim.
    pub(crate) browser: bool,
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
                // A benchmark timed while other workers run other files is
                // timed against them; one file at a time is what makes two runs'
                // numbers comparable. `-j` still says otherwise.
                None if self.bench => Concurrency::Fixed(NonZeroUsize::MIN),
                None => Concurrency::Auto,
            },
            bail: self.bail.map(Bail::after).unwrap_or_default(),
            retry: if self.retry == 0 {
                RetryPolicy::none()
            } else {
                RetryPolicy::retries(self.retry)
            },
            // The run's first file pays for the browser's start-up, because
            // `uf` starts its stopwatch when it writes the request and the
            // driver is still opening a page. See [`BROWSER_FILE_TIMEOUT`].
            file_timeout: if self.browser {
                uf_test::BROWSER_FILE_TIMEOUT
            } else {
                uf_test::DEFAULT_FILE_TIMEOUT
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
    // `--changed` is one selection, made from git before the run; watch mode
    // makes its own after every edit. Asked for both, neither can keep the
    // promise the other makes.
    if args.watch && args.changed.is_some() {
        bail!(
            "--watch and --changed cannot be combined: watch mode already re-runs what each edit \
             affects"
        );
    }
    // Watch mode's reason, in the same words: a report over the files a change
    // reached is not the project's coverage, and a threshold held against it
    // would fail a pull request over files it never touched.
    if args.changed.is_some() && args.coverage {
        bail!(
            "--changed and --coverage cannot be combined: a run over the files a change reaches \
             would report coverage that is not the project's"
        );
    }
    // A shard is one part of a run CI splits across machines, and watch mode is
    // a loop on this one: there is no part of a loop to hand out.
    if args.watch && args.shard.is_some() {
        bail!(
            "--watch and --shard cannot be combined: watch mode re-runs what each edit affects \
             on this machine, and a shard is one part of a run split across several"
        );
    }
    // A benchmark's numbers are only worth comparing when nothing else is
    // changing them: a watch loop re-times whatever an edit touched, a shard
    // times part of the suite beside other machines, coverage instruments the
    // code being timed, and a page is not the host the baseline was timed on.
    if args.bench {
        let clashing = [
            (args.watch, "--watch"),
            (args.shard.is_some(), "--shard"),
            (args.coverage, "--coverage"),
            (args.browser, "--browser"),
        ];
        if let Some((_, flag)) = clashing.iter().find(|(present, _)| *present) {
            bail!(
                "{flag} and --bench cannot be combined: a benchmark is timed one file at a time, \
                 on the project's host, with nothing else changing what it measures"
            );
        }
    }
    // Before anything is scanned or started: a merge runs nothing.
    if let Some(directory) = args.merge_shards.as_deref() {
        return shards::merge(cwd, ui, directory, &args);
    }

    let mut resolved = load_config(cwd)?;
    if let Some(host) = &args.host {
        resolved.config.test.runtime = Some(uf_config::Written::new(host.as_str()));
        uf_config::validate_config(
            resolved.config_path.as_deref().unwrap_or(&resolved.root),
            &resolved.config,
        )?;
    }
    let root = resolved.root.clone();
    if test_application_target(&resolved.config) == TestApplicationTarget::ReactNative {
        let tables = uf_router::native::discover_native_route_tables(&root, &resolved.config)?;
        uf_router::native::write_native_route_tables(&root, &resolved.config, &tables)?;
    }
    // Named paths override `.gitignore`, as they do for `uf lint` and
    // `uf fmt`: a suite that writes its fixture into an ignored directory —
    // `packages/test/module-mock.test.js` does, so a killed run leaves nothing
    // behind — still has to be runnable by name.
    //
    // A one-shot, non-coverage run over an existing path can stay inside that
    // path. Watch mode and `--changed` still need the whole import graph, so a
    // change to a shared dependency reaches the selected tests, and coverage
    // still needs every JavaScript file so "not covered" means something.
    //
    // Every host is one of those now. Deno used to need the whole project even
    // for a narrowed run, because its loader was an ahead-of-time pass over
    // the scan and a module the scan left out was a module Deno met as Flow.
    // Its loader is a hook, asked about each import as it happens, so a run
    // narrowed to one directory needs only that directory on every host.
    let needs_project_scan = args.watch
        || args.changed.is_some()
        || args.coverage
        || resolved.config.test.coverage.enabled;
    let scan = if needs_project_scan {
        scan_selected_source_files(&root, &resolved.config, &args.paths)?
    } else {
        match scan_existing_selected_source_files(&root, &resolved.config, &args.paths)? {
            Some(scan) => scan,
            None => scan_selected_source_files(&root, &resolved.config, &args.paths)?,
        }
    };
    render_ignore_deprecation(ui, ignore_deprecation(&resolved.config));
    crate::support::render_deprecations(ui, resolved.config.test_runner_deprecation());
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

    // Asked before `--list` and before a Bun runner takes the suite, so both
    // show and run what this selection chose. The graph is built over every
    // file the scan found; the selection only ever narrows which test files run.
    let changed = args
        .changed
        .as_deref()
        .map(|reference| changed::select(&root, reference, &files))
        .transpose()?;

    // `test.runner` naming Bun hands the suite to `bun test`, and uf's own
    // runner never starts: not for `--list`, which Bun cannot answer without
    // running the files, and not for anything after it. What reaches Bun, and
    // why a report is always read back, is [`bun`].
    let runner = resolved.config.test_runner_tool();
    if let uf_config::TestRunnerSpec::Bun(_) = &runner.spec {
        let refused = bun::refused_flags(&args);
        if !refused.is_empty() {
            let reasons: Vec<String> = refused
                .iter()
                .map(|refusal| format!("{} — {}", refusal.flag, refusal.reason))
                .collect();
            bail!(
                "`test.runner` is `{}`, and `bun test` cannot honour {}:\n  {}",
                runner.spec,
                plural(refused.len(), "flag"),
                reasons.join("\n  ")
            );
        }
        if args.reporter.is_some() && args.reporter_outfile.is_none() {
            bail!("--reporter needs --reporter-outfile");
        }
        let env = project_env(&resolved, args.mode.as_deref(), TEST)?;
        // Coverage switched on in `uf.config.js` is asked of Bun the way
        // `--coverage` is; the thresholds Bun cannot be held to are refused in
        // [`bun::run`]. A `--changed` run asks for none, for the reason uf's
        // own runner collects none on one.
        let coverage_configured = resolved.config.test.coverage.enabled;
        let args = TestArgs {
            coverage: changed.is_none() && (args.coverage || coverage_configured),
            ..args
        };
        let selected: Vec<ProjectFile> = test_bearing(files)
            .into_iter()
            .filter(|file| args.paths.is_empty() || selects(&args.paths, &file.relative_path))
            .collect();
        // `--changed` is uf's to answer, not Bun's: Bun is handed the files the
        // selection reached, and runs nothing else.
        let selected = match &changed {
            Some(selection) => changed::narrow(ui, selection, selected, coverage_configured),
            None => selected,
        };
        return bun::run(ui, &resolved, env, &selected, &args);
    }

    if args.list {
        if changed.is_none() && args.shard.is_none() {
            return render_list(ui, &root, &files, &args.filter(), args.bench);
        }
        let mut tests = test_bearing(files);
        if let Some(selection) = &changed {
            // `false`: `--list` collects nothing, so there is no coverage to
            // say is missing.
            tests = changed::narrow(ui, selection, tests, false);
        }
        if let Some(shard) = args.shard {
            let (timings, _) = read_timings(&root);
            tests = shards::cut(ui, &tests, &args, &timings, shard).files;
        }
        return render_list(ui, &root, &tests, &args.filter(), args.bench);
    }
    let application_target = test_application_target(&resolved.config);
    if application_target == TestApplicationTarget::ReactNative
        && (args.browser
            || args.bench
            || args.coverage
            || resolved.config.test.coverage.enabled
            || args.update_snapshots)
    {
        bail!(
            "`uf test` native component tests support Node execution and interaction assertions; browser mode, benchmarks, coverage and snapshot updates are not supported in the native module environment"
        );
    }
    // `test` rather than `development`, so `.env.test` is a file that means
    // something — the mode Vitest runs in, for the same reason: a suite that
    // talks to the development database is a suite that can destroy it.
    let env = project_env(&resolved, args.mode.as_deref(), TEST)?;
    // The runtime the suite runs on: `test.runtime`, then the runtime the
    // runner brings, then `runtime`, then the host uf has always found.
    // Resolved once, before the watch loop and the one-shot run part ways, so
    // a first run on a version downloads it in one place and says so once.
    let runtime = runtimes::resolve(&resolved, runtimes::Role::Test, &mut |message| {
        ui.render_err(|renderer, out| renderer.status(out, uf_term::Status::Info, message));
    })?;
    let env = runtime.environment(env);
    if args.watch {
        return watch::watch(ui, &root, resolved.config, &env, runtime.host, args);
    }

    // A snapshot is a file beside the test that took it, and a page has no
    // filesystem to write one to. Said here rather than let through to
    // `internal/browser/node.js`'s refusal, because `-u` is a request to
    // *change files on disk* and answering it with a per-test failure would
    // leave a reader wondering which snapshots did get rewritten. None did.
    // Before the browser is looked for, deliberately. Installing a browser
    // would not make this combination work, so "there is no browser on this
    // machine" would be a true sentence that sent a reader somewhere useless.
    // The `can_collect_coverage` check below still stands for Bun and Deno,
    // where the question is about the host that was already chosen.
    // A `--changed` run collects no coverage, even when `uf.config.js` turns it
    // on, for the reason `--changed --coverage` is refused above.
    // Nor a run of the benchmarks, whose timings coverage would inflate; a
    // `--bench --coverage` is refused above.
    let coverage_on = !args.bench
        && changed.is_none()
        && (args.coverage || resolved.config.test.coverage.enabled);
    if args.browser && coverage_on {
        bail!(
            "`uf test --browser --coverage` cannot measure anything: V8 is counting in the \
             renderer exactly as it counts in Node, but `NODE_V8_COVERAGE` is Node's own switch \
             and the only way to ask a page for its profile is the DevTools protocol, which this \
             mode does not speak. Run `uf test --coverage` on Node for the numbers, and \
             `uf test --browser` for what a browser answers."
        );
    }

    let resolved_host = runtime.host;
    let host_kind = test_host_kind(resolved_host.kind, args.browser);
    let settings = &resolved.config.test.coverage;
    if coverage_on && !host_kind_can_collect_coverage(host_kind) {
        bail!(
            "`uf test --coverage` is Node-only today: uf reads V8 counts written through \
             `NODE_V8_COVERAGE` and maps them back through the source-map cache the Node loader \
             writes. {} cannot provide that same Flow-source report, and reporting zeroes would \
             be worse than saying so.",
            host_kind.name()
        );
    }
    let mut host = test_host(&root, &resolved.config, &env, args.browser, resolved_host)?
        .with_snapshot_updates(args.update_snapshots)
        .with_benchmarks(args.bench)
        .with_axe(resolved.config.accessibility.axe.as_json());
    host.env.push((
        "UF_VRT_CONFIG".to_owned(),
        serde_json::to_string(&resolved.config.vrt)?,
    ));

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

    let raw = if coverage_on {
        let raw = coverage::RawCoverage::create(&root)?;
        host = host.with_coverage_dir(raw.directory().to_path_buf());
        Some(raw)
    } else {
        None
    };

    let (planned, files) = if changed.is_none() && args.shard.is_none() {
        let planned = planned_test_bearing(files);
        let files = planned.iter().map(|planned| planned.file.clone()).collect();
        (Some(planned), files)
    } else {
        (None, test_bearing(files))
    };
    let files = match &changed {
        Some(selection) => changed::narrow(ui, selection, files, settings.enabled),
        None => files,
    };
    let mut timer = PhaseTimer::start();
    let (timings, timing_note) = read_timings(&root);
    // Cut from the timings the run schedules with, so the part this shard runs
    // is the part every other shard left for it.
    let cut = args
        .shard
        .map(|shard| shards::cut(ui, &files, &args, &timings, shard));
    let run_files = cut.as_ref().map_or(&files[..], |cut| &cut.files[..]);
    let report = timer.measure("run", || match &planned {
        Some(planned) => run_once_planned(ui, &root, &host, planned, &args, timings.clone()),
        None => run_once(ui, &root, &host, run_files, &args, timings.clone(), None),
    })?;

    // A shard measures and records. The reports and the thresholds are
    // statements about the whole suite, so they are `--merge-shards`' to make.
    let mut measured = None;
    let collected = match (&raw, &cut) {
        (Some(raw), None) => Some(timer.measure("coverage", || {
            coverage::collect(
                &root,
                settings,
                raw,
                &coverage::directory_for(&root, settings, args.coverage_dir.as_deref()),
                &args.coverage_reporters,
                &project_paths,
            )
        })?),
        (Some(raw), Some(_)) => {
            measured = Some(timer.measure("coverage", || {
                coverage::measure(&root, settings, raw, &project_paths)
            })?);
            None
        }
        (None, _) => None,
    };
    let duration = timer.total();

    if let Some(ResultReporterArg::Junit) = args.reporter {
        write_results_report(&root, &args, &report)?;
    }

    // A shard leaves the timings as it found them: every shard in this
    // checkout has to cut its part from the same durations, and the merge
    // records the whole suite's.
    let (recorded, shard_record) = match &cut {
        Some(cut) => (
            None,
            Some(shards::write_record(
                &root,
                cut,
                &report,
                measured.as_ref(),
            )?),
        ),
        // What a file costs in a run of the benchmarks is its iterations, and
        // the next run of the tests would be scheduled by them.
        None if args.bench => (None, None),
        // A run narrowed by a path or by `--changed` measured part of the
        // suite, and speaks only for that part: what it recorded before for
        // every other file is still the best estimate the next full run has.
        None => (
            record_timings(&root, timings, &report, &files, &|recorded| {
                changed.is_none() && selects(&args.paths, recorded)
            }),
            None,
        ),
    };
    let benchmarks = args
        .bench
        .then(|| bench::compare(&root, &args, &report))
        .transpose()?;
    if args.json {
        let mut document = test_payload(
            Some(&host),
            &report,
            collected.as_ref().map(|(coverage, _)| coverage),
        );
        if let Some(comparison) = &benchmarks {
            document["benchmarks"] = comparison.payload();
        }
        ui.json(&document)?;
    } else {
        render_report(
            ui,
            &root,
            run_files,
            &report,
            timer.phases(),
            duration,
            &args,
            Some(&host),
            timing_note.as_deref(),
            recorded.as_deref(),
            collected.as_ref().map(|(_, section)| section),
            true,
        );
        if let Some(shown) = &shard_record {
            shards::announce(ui, shown);
        }
        if let Some(comparison) = &benchmarks {
            bench::render(ui, comparison);
        }
    }

    finish(
        &report,
        collected
            .as_ref()
            .map_or(&[][..], |(_, section)| &section.violations),
    )?;
    benchmarks.map_or(Ok(()), |comparison| comparison.verdict())
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
///
/// `browser` swaps the host rather than adding a flag to one: the process uf
/// starts is a different module, the thing that runs the test body is a page,
/// and everything above this function is unchanged. See [`HostKind::Browser`]
/// and [`uf_test::browser`] for what that costs and what it depends on.
///
/// No host needs the project's file list any more. Deno did, while its Flow
/// loader was an ahead-of-time pass that could only compile what it was told
/// about; every host now transforms each module as it is imported, so the
/// command is the same whatever the run selected.
///
/// `host` is the runtime [`runtimes::resolve`] settled for the suite, resolved
/// by the caller rather than here so a watch session and a one-shot run share
/// one answer, and a first run on a version downloads it once.
pub(crate) fn test_host(
    root: &Utf8Path,
    config: &uf_config::UniflowedConfig,
    env: &ProjectEnv,
    browser: bool,
    host: Host,
) -> Result<HostCommand> {
    // The loader, not the bundler. `uf test` transforms through `uf transform`
    // and runs on a Capability JS Host; nothing in that path is Vite's, and
    // asking for `@uniflowed/vite` made a test run depend on a bundler it never
    // loads.
    let loader = uniflowed_package(root, "host", "register.js")?;
    let scope = loader.parent().map(Utf8Path::to_path_buf);
    let worker_module = if browser {
        "test/browser-worker.js"
    } else {
        "test/worker.js"
    };
    let worker = scope
        .as_ref()
        .map(|scope| scope.join(worker_module))
        .filter(|worker| worker.is_file())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`@uniflowed/test` is not installed for {root}; add it to the project's \
                 dependencies and run the package manager (`uf install`)"
            )
        })?;

    let kind = test_host_kind(host.kind, browser);
    let native = test_application_target(config) == TestApplicationTarget::ReactNative;
    if native && kind != HostKind::Node {
        bail!(
            "`uf test` native component tests require Node and React Native's official Jest module environment; select test.runtime = 'node'"
        );
    }
    // The driver of a browser run is Node whatever the project's Capability JS
    // Host is, because the runtime under test is the browser and the driver
    // only shuttles JSON between a pipe and a socket. Found here rather than
    // assumed, so a machine with no `node` is told that instead of being handed
    // a "no such file or directory" from `spawn`.
    let host_name = if browser { "the browser" } else { host.name() };
    let program = if browser {
        find_program(HostKind::Browser.program()).ok_or_else(|| {
            anyhow::anyhow!(
                "`uf test --browser` drives a browser from a Node process, and there is no \
                 `node` on PATH. The page is where the tests run; Node is what serves it their \
                 modules and holds the browser's process handle."
            )
        })?
    } else {
        host.program
    };
    // Read once: it is the values the workers get *and* the names the
    // permission set has to grant, and the two must be the same list or a test
    // would be handed a variable it may not read.
    let mut exported = env.exported();
    if native {
        exported.push((String::from("UF_TEST_TARGET"), String::from("react-native")));
        exported.push((String::from("NODE_ENV"), String::from("test")));
    }
    // Deno's hook is `node:module`'s `registerHooks`, and a Deno older than
    // the release that implemented it would start every worker only for the
    // preload to refuse inside each one. Asked once, here, so the refusal is
    // one sentence about the host rather than a failed file per test file.
    if kind == HostKind::Deno {
        require_deno_hooks(&program)?;
    }
    let mut command = HostCommand::new(kind, program, worker, root.to_path_buf())
        .with_flow_loader(
            Utf8Path::new("@uniflowed/host/register"),
            &loader.join("bun-preload.js"),
            &loader.join("deno-preload.js"),
        )
        // Every worker gets the project's `.env` values, so a test reads
        // `process.env.DATABASE_URL` and finds what `uf dev` would have found.
        .with_env(exported.clone());
    // The worker transforms through the binary that started it, never a
    // different `uf` that happens to be on PATH.
    let uf_binary = uf_binary()?;
    command = command.with_uf_binary(uf_binary.clone());
    // After the binary is known, because the permission set has to grant the
    // transform service the right to exist: a worker that may not start `uf
    // transform` cannot load a line of Flow.
    if browser && config.permissions.is_some() {
        // Refused rather than translated, for the reason Bun's set is refused:
        // a set that the other hosts enforce and one silently ignores is worse
        // than no set at all. Every category a project can declare describes a
        // *process* — which files it may read, which programs it may start —
        // and the process a browser run sandboxes would be the driver, not the
        // page the tests are in. Putting `--allow-read` on the driver would
        // produce a run that reads as sandboxed in `uf explain` and a renderer
        // with the whole machine's network.
        //
        // Only a *declared* set refuses. The undeclared-Deno default above is a
        // statement about Deno's own toolchain rather than about the project,
        // and `--browser` is not on Deno: the driver is Node, so there is no
        // default to inherit and nothing for this to be silent about.
        bail!(
            "`uf test --browser` cannot enforce this project's `permissions`: they describe \
             what a process may reach, and the process the tests run in is a browser uf \
             started rather than a host uf configured. A set uf cannot enforce stops the run \
             rather than being partly applied — run the suite without `--browser`, where \
             Node and Deno enforce it."
        );
    }
    // On Deno the set goes on whether or not the project declared one, and that
    // is the one place `uf test` cannot give a host what it gives the others.
    // Node and Bun run a process that may reach anything until a project says
    // otherwise; Deno's default is the opposite, so "no permission set" cannot
    // be passed through — the choice is between the toolchain's own access and
    // `-A`, and ubugeeei-prod/uf#246 is about what `-A` costs. So an undeclared
    // Deno run gets the project root, its packages and nothing else, and
    // `uf explain test` prints exactly that.
    let nothing_declared = Permissions::default();
    let declared = config
        .permissions
        .as_ref()
        .or_else(|| (kind == HostKind::Deno).then_some(&nothing_declared));
    if let Some(permissions) = declared {
        command = command.with_permissions(worker_permissions(
            kind,
            root,
            &loader,
            Some(uf_binary.as_path()),
            permissions,
            &exported
                .iter()
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>(),
        )?);
    }
    if browser {
        // `UF_BROWSER` from this process's environment, and deliberately not
        // from the project's `.env`: which browser a suite is measured in is a
        // property of the machine, and a cloned repository's `.env` must not be
        // able to answer it. `docs/security.md` makes the same claim about
        // `UF_BINARY` for the same reason.
        let named = std::env::var(uf_test::browser::BROWSER_VARIABLE).ok();
        let found = uf_test::find_browser(named.as_deref(), &find_program, &Utf8Path::is_file)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        command = command.with_browser(found.program);
    }
    if !command.loads_flow() {
        // No host reaches this today: Node registers hooks, Bun preloads a
        // plugin, Deno preloads `registerHooks`, and a browser run's driver
        // serves the page every module through `uf transform`. It stays
        // because the question it asks belongs to `uf_runtime::HOSTS` rather
        // than to this function — a fifth `HostKind` whose row has no
        // `flow_loader` must be refused here rather than allowed to meet a
        // syntax error in somebody's own test file.
        //
        // The reason comes from the table rather than from a sentence written
        // here, so that the message a person meets and the table
        // `docs/hosts.md` is generated from cannot drift apart. The two used to
        // be separate sentences and the enum's said nothing at all.
        let support = uf_runtime::HostSupport::for_host(runtime_host(kind));
        let tracking = support.tracking_issue.map_or_else(String::new, |issue| {
            format!(" Tracked by https://github.com/ubugeeei-prod/uf/issues/{issue}.")
        });
        bail!(
            "`uf test` cannot run on {host_name} yet: it has no Flow loader, so a test file \
             written in Flow could not be imported. What it needs is {}.{tracking} Node.js and \
             Bun both run the suite today; install one, or name it in \
             `app.runtime.capabilityJsHost.default`.",
            support.missing.unwrap_or("a Flow loader"),
        );
    }
    Ok(command)
}

fn test_host_kind(host: uf_config::CapabilityJsHost, browser: bool) -> HostKind {
    if browser {
        return HostKind::Browser;
    }
    match host {
        uf_config::CapabilityJsHost::Node => HostKind::Node,
        uf_config::CapabilityJsHost::Bun => HostKind::Bun,
        uf_config::CapabilityJsHost::Deno => HostKind::Deno,
    }
}

const fn host_kind_can_collect_coverage(kind: HostKind) -> bool {
    matches!(kind, HostKind::Node)
}

/// The first Deno release with `node:module`'s `registerHooks`.
///
/// `@uniflowed/host/deno-preload` installs the Flow loader through that hook
/// and nothing else, so this is the floor for running a uf project on Deno at
/// all. The preload checks for the hook itself, from inside the process; this
/// is the same line drawn before any process starts.
const DENO_WITH_HOOKS: (u64, u64) = (2, 8);

/// Refuse a Deno that predates the hook the Flow loader is built on.
///
/// A version that cannot be read is let through rather than refused. The
/// preload asks the runtime whether the hook exists and says the same sentence
/// from inside the worker, so an unreadable `--version` costs a later message
/// and never a run that silently cannot load Flow — while refusing it would
/// turn a Deno build that prints its version differently into a host uf will
/// not start for no reason it can name.
fn require_deno_hooks(program: &Utf8Path) -> Result<()> {
    let Ok(output) = std::process::Command::new(program.as_std_path())
        .arg("--version")
        .output()
    else {
        return Ok(());
    };
    match deno_version(&String::from_utf8_lossy(&output.stdout)) {
        Some(found) if found < DENO_WITH_HOOKS => bail!("{}", deno_too_old(program, found)),
        _ => Ok(()),
    }
}

/// What a person is told about a Deno older than [`DENO_WITH_HOOKS`].
fn deno_too_old(program: &Utf8Path, (major, minor): (u64, u64)) -> String {
    format!(
        "`uf test` cannot run on the Deno at {program}: it is {major}.{minor}, and uf loads Flow \
         on Deno through `node:module`'s `registerHooks`, which Deno implemented in {}.{}. Run \
         `deno upgrade`, or name Node.js or Bun in `app.runtime.capabilityJsHost.default`.",
        DENO_WITH_HOOKS.0, DENO_WITH_HOOKS.1
    )
}

/// The `major.minor` in what `deno --version` prints.
///
/// The first line is `deno 2.9.6 (stable, release, aarch64-apple-darwin)`; a
/// pre-release carries a suffix on the patch (`2.10.0-rc.1`), which is past
/// the two numbers this reads. Anything that does not start with `deno` is not
/// a Deno this knows how to read, and answers `None`.
fn deno_version(text: &str) -> Option<(u64, u64)> {
    let mut words = text.split_whitespace();
    if words.next()? != "deno" {
        return None;
    }
    let mut numbers = words.next()?.split(['.', '-', '+']);
    let major = numbers.next()?.parse().ok()?;
    let minor = numbers.next()?.parse().ok()?;
    Some((major, minor))
}

/// The `uf` every worker in this run transforms its modules through.
///
/// # Why this is not "whatever `uf` is installed"
///
/// A worker imports each test file through the host's Flow loader, and that
/// loader shells out to `uf transform`. Which `uf` it reaches decides what the
/// modules under test *are*. `packages/host/transform.js` falls back to a bare
/// `uf` on `PATH` when nothing says otherwise, so a run that cannot name its
/// own binary silently answers a different question — "what does the installed
/// uf make of this project" rather than "what does this one" — while the
/// report carries the name of the binary the user typed. That is not a slower
/// run, it is a different compiler; see ubugeeei-prod/uf#217.
///
/// # The routes, in order, and why `current_exe` is first
///
/// 1. [`std::env::current_exe`], which is the exact answer wherever the
///    platform gives one, and is therefore never overridable. An inherited
///    `UF_BINARY` — from an outer run, from a shell profile, from a `.env`
///    should one ever reach this process — would otherwise redirect a run to
///    a compiler nobody chose while the report carried this binary's name,
///    which is the defect itself rather than a fix for it. `docs/security.md`
///    makes the same claim about the project's environment for the same
///    reason.
/// 2. `UF_BINARY` from this process's environment. The escape hatch, and it
///    is placed exactly where an escape hatch is needed: a platform or
///    filesystem where the route above will not answer, or answers with a
///    path that is not UTF-8.
/// 3. `argv[0]`, resolved the way the shell that launched us resolved it: a
///    path against the working directory, a bare name along `PATH`. This is
///    still *the binary that was invoked* rather than "an uf" — the difference
///    from the old fallback is that it is resolved here, now, and checked to
///    exist, instead of being left to a `uf` lookup inside a worker minutes
///    later.
///
/// Refusing when all three fail is the point. The alternative considered was a
/// warning in the run header, and a warning in a passing run is read by
/// nobody: the run would still report on a compiler nobody chose. A refusal
/// names the one thing that fixes it.
pub(crate) fn uf_binary() -> Result<Utf8PathBuf> {
    let current_exe = std::env::current_exe()
        .ok()
        .and_then(|binary| Utf8PathBuf::from_path_buf(binary).ok());
    let declared = std::env::var("UF_BINARY").ok();
    let argv0 = std::env::args_os()
        .next()
        .and_then(|argument| argument.into_string().ok());
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|cwd| Utf8PathBuf::from_path_buf(cwd).ok());
    resolve_uf_binary(
        current_exe,
        declared.as_deref(),
        argv0.as_deref(),
        cwd.as_deref(),
        &find_program,
    )
    .ok_or_else(|| {
        anyhow::anyhow!(
            "`uf test` cannot tell which `uf` binary is running, so it cannot promise that the \
             workers transform this project through it: the operating system would not say \
             (`current_exe`), and argv[0] named nothing that exists. Running the suite anyway \
             would compile it with whatever `uf` is on PATH and report the result under this \
             one's name. Set `UF_BINARY` to the path of the binary to use."
        )
    })
}

/// The decision behind [`uf_binary`], with everything it reads passed in.
///
/// Split out because the interesting cases are the ones a process cannot be
/// put into from a test: `current_exe` refusing to answer, or answering with a
/// path that is not UTF-8. Both are arguments here.
///
/// `on_path` is the `PATH` lookup, injected for the same reason.
fn resolve_uf_binary(
    current_exe: Option<Utf8PathBuf>,
    declared: Option<&str>,
    argv0: Option<&str>,
    cwd: Option<&Utf8Path>,
    on_path: &dyn Fn(&str) -> Option<Utf8PathBuf>,
) -> Option<Utf8PathBuf> {
    if let Some(binary) = current_exe {
        return Some(binary);
    }
    if let Some(declared) = declared.filter(|declared| !declared.is_empty()) {
        return Some(Utf8PathBuf::from(declared));
    }
    let argv0 = argv0.filter(|argv0| !argv0.is_empty())?;
    // A bare name is a `PATH` lookup and anything else is a path, which is the
    // rule every shell applies to the command it was given. `uf` written with
    // no separator was found on `PATH`, so looking it up there finds the same
    // file; `./target/release/uf` was not, and must not be.
    if !argv0.contains('/') && !(cfg!(windows) && argv0.contains('\\')) {
        return on_path(argv0);
    }
    let path = Utf8Path::new(argv0);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd?.join(path)
    };
    // Checked rather than trusted: argv[0] is whatever the parent process
    // chose, and a login shell's `-uf` or a name from a since-deleted
    // directory would otherwise be handed to a worker as a compiler.
    resolved.is_file().then_some(resolved)
}

/// What uf itself must reach on the host, whatever the project declared.
///
/// Three of the toolchain's own grants, and each of them is the difference
/// between a permission set and a run that cannot start:
///
/// * **the project root, readable.** A worker imports the test file and
///   everything it imports. A declared `read` list is *added to* this rather
///   than replacing it, which is the part worth saying out loud: a permission
///   set does not narrow a run's reach into the project, it denies the rest of
///   the machine — `~/.ssh`, `~/.aws`, `/etc`.
/// * **the directory the packages resolve from, readable.** `@uniflowed/test`
///   and `@uniflowed/host` are reached through `node_modules`, which in a
///   workspace sits above the project and would otherwise be outside every
///   grant.
/// * **the `uf` binary, readable and runnable.** Every Flow module is
///   transformed by a `uf transform` child, and `packages/host/transform.js`
///   stats the binary before spawning it to key its cache. Both are denied by
///   default under a permission model, and the failure would surface inside
///   uf's own loader rather than in anything the project wrote.
///
/// `.uf` is the only writable path uf adds: the transform and check caches, the
/// snapshots a `--update-snapshots` run rewrites, and the coverage documents
/// all live under it.
///
/// `loader` is `None` for `uf explain`, which describes a run rather than
/// starting one and has not resolved the project's packages. The difference is
/// one entry in the read list, and saying "the packages directory as well"
/// costs a reader less than a second implementation of this would.
pub(crate) fn toolchain_access(
    root: &Utf8Path,
    loader: Option<&Utf8Path>,
    uf_binary: Option<&Utf8Path>,
) -> ToolchainAccess {
    let mut toolchain = ToolchainAccess {
        read: vec![root.to_string()],
        write: vec![root.join(".uf").to_string()],
        run: Vec::new(),
        env: WORKER_ENVIRONMENT
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        // Node's Flow loader compiles a cache miss on a thread — the transform
        // thread its in-thread hooks start, or the loader thread `register()`
        // starts on a Node without `registerHooks` — and Node's permission
        // model calls either one a worker.
        loader_thread: true,
    };
    if let Some(loader) = loader {
        // `loader` is `<node_modules>/@uniflowed/host`; two levels up is the
        // directory every bare specifier in the worker resolves through.
        if let Some(modules) = loader.parent().and_then(Utf8Path::parent) {
            toolchain.read.push(modules.to_string());
        }
        // And where the packages *really* are. A workspace links
        // `node_modules/@uniflowed/host` at its own `packages/host`, and Node
        // resolves a symlink before it checks the path against the grant — so
        // a run in a workspace was denied the loader it had just been given
        // permission to read. Granting the resolved scope directory covers
        // every `@uniflowed/*` the worker imports and nothing else.
        if let Ok(real) = loader.canonicalize_utf8()
            && let Some(scope) = real.parent()
        {
            toolchain.read.push(scope.to_string());
        }
    }
    if let Some(binary) = uf_binary {
        toolchain.read.push(binary.to_string());
        toolchain.run.push(binary.to_string());
    }
    toolchain
}

/// The host, as `uf_runtime` names it.
///
/// [`HostKind::Browser`] maps to [`RuntimeHost::Node`], and the reason it is
/// not a fourth row is worth stating: `uf_runtime::HOSTS` grades runtimes that
/// **run a uf project** — a server, a build, an application — and a browser
/// already runs one, as the target rather than as the host. What this function
/// answers for is the *process* uf starts, and for a browser run that process
/// is the Node driver. The two callers ask exactly that: which permission
/// arguments to translate into (a browser run refuses before reaching them),
/// and which host's missing-loader row to quote (a browser run loads Flow).
pub(crate) const fn runtime_host(kind: HostKind) -> RuntimeHost {
    match kind {
        HostKind::Node | HostKind::Browser => RuntimeHost::Node,
        HostKind::Bun => RuntimeHost::Bun,
        HostKind::Deno => RuntimeHost::Deno,
    }
}

/// The variables uf itself puts in a worker's environment, by name.
///
/// A grant list rather than documentation: on Deno, a variable uf set and did
/// not name is a variable the worker cannot read, which is a `PermissionDenied`
/// from inside `@uniflowed/host` rather than from anything a project wrote.
/// `PATH` is here because `packages/host/transform.js` searches it to identify
/// the `uf` it will run.
///
/// `NODE_V8_COVERAGE` is the one name uf only *sometimes* sets — on a Node
/// worker that collects coverage — and it is here for Deno, which never gets
/// it: Deno's `node:child_process` reads it whenever a child is started with an
/// explicit environment, to decide whether to pass Node's switch on, and the
/// Deno loader starts `uf transform` exactly that way. Ungranted, that read is
/// `NotCapable`, and the first Flow import fails with it.
///
/// The project's own `.env` names are added beside these per run; they are not
/// constant and are not uf's.
pub(crate) const WORKER_ENVIRONMENT: [&str; 9] = [
    "NODE_V8_COVERAGE",
    "PATH",
    "UF_AXE",
    "UF_BINARY",
    "UF_IN_SOURCE_TESTS",
    "UF_PROJECT_ROOT",
    "UF_TEST_BENCH",
    "UF_TEST_TARGET",
    "UF_UPDATE_SNAPSHOTS",
];

fn worker_permissions(
    kind: HostKind,
    root: &Utf8Path,
    loader: &Utf8Path,
    uf_binary: Option<&Utf8Path>,
    permissions: &Permissions,
    env: &[String],
) -> Result<Vec<String>> {
    let mut toolchain = toolchain_access(root, Some(loader), uf_binary);
    // A test reads configuration through `process.env`, and uf is the process
    // that put it there. Naming the variables uf set is not widening the
    // project's set: it is the same disclosure the read and write lists make,
    // for the category Deno is the only host to have.
    toolchain.env.extend(env.iter().cloned());
    // Deno keeps `run` like the other two. It used to be cleared here, while
    // Deno's modules were compiled before it started and nothing needed
    // starting; its loader transforms each module as it loads now, through a
    // `uf transform` child, so it is granted that one program — by name,
    // `--allow-run=<this uf>`, which is the scope Node's
    // `--allow-child-process` cannot express.
    // The error is the feature: a set this host cannot enforce stops the run
    // and names the host that can, rather than being partly applied.
    uf_runtime::permissions::host_arguments(runtime_host(kind), permissions, &toolchain)
        .map_err(|error| anyhow::anyhow!("{error}"))
}

/// Run the suite once, drawing each file as it finishes and a progress line
/// while it goes. See [`stream`].
pub(crate) fn run_once(
    ui: &mut Ui,
    root: &Utf8Path,
    host: &HostCommand,
    files: &[ProjectFile],
    args: &TestArgs,
    timings: TestTimings,
    pool: Option<&uf_test::WorkerPool>,
) -> Result<TestRunReport> {
    let sources = test_files(root, files);
    let runner = TestRunner::new()
        .with_options(args.options())
        .with_filter(args.filter())
        .with_timings(timings)
        .with_host(host.clone());

    let stream = stream::Stream::start(
        ui,
        files
            .iter()
            .map(|file| (file.relative_path.as_str(), file.source.as_str())),
        crate::support::project_label(root),
    );
    Ok(stream.drive(|| match pool {
        Some(pool) => runner.run_observed_in(&sources, &stream, pool),
        None => runner.run_observed(&sources, &stream),
    })?)
}

fn run_once_planned(
    ui: &mut Ui,
    root: &Utf8Path,
    host: &HostCommand,
    files: &[PlannedProjectFile],
    args: &TestArgs,
    timings: TestTimings,
) -> Result<TestRunReport> {
    let sources = planned_test_files(root, files);
    let runner = TestRunner::new()
        .with_options(args.options())
        .with_filter(args.filter())
        .with_timings(timings)
        .with_host(host.clone());

    let stream = stream::Stream::start(
        ui,
        files.iter().map(|planned| {
            (
                planned.file.relative_path.as_str(),
                planned.file.source.as_str(),
            )
        }),
        crate::support::project_label(root),
    );
    Ok(stream.drive(|| runner.run_planned_observed(&sources, &stream))?)
}

/// The files that declare at least one test.
///
/// Every module in a project is not a test: importing one to find out would
/// run its side effects and cost a process, and a config file or a component
/// has nothing to report. Discovery answers the question by reading, which is
/// the same answer `uf test --list` shows.
pub(crate) fn test_bearing(files: Vec<ProjectFile>) -> Vec<ProjectFile> {
    planned_test_bearing(files)
        .into_iter()
        .map(|planned| planned.file)
        .collect()
}

fn planned_test_bearing(files: Vec<ProjectFile>) -> Vec<PlannedProjectFile> {
    files
        .into_iter()
        .filter_map(|file| {
            // A file with a declaration discovery could not read is still a
            // test file: the worker imports it and finds whatever registers.
            // Requiring a *readable* case meant `it(name, …)` in a loop made a
            // file vanish, and the run said "0 passed" and exited 0.
            let plan = uf_test::discover_tests(&file.relative_path, &file.source);
            plan_declares_tests(&plan).then_some(PlannedProjectFile { file, plan })
        })
        .collect()
}

fn plan_declares_tests(plan: &TestPlan) -> bool {
    plan.runnable_count() > 0 || plan.bench_count() > 0 || !plan.unsupported.is_empty()
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

fn planned_test_files(root: &Utf8Path, files: &[PlannedProjectFile]) -> Vec<PlannedTestFile> {
    files
        .iter()
        .map(|planned| {
            PlannedTestFile::new(
                TestFile::new(
                    planned.file.relative_path.clone(),
                    root.join(&planned.file.relative_path),
                    planned.file.source.clone(),
                ),
                planned.plan.clone(),
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
/// `files` are the test files this run looked at, and `in_scope` says which
/// recorded paths it looked for: an entry in scope that is not among `files`
/// was deleted or no longer declares a test, and is dropped. An entry out of
/// scope is kept while its file exists. Before that distinction, the durations
/// kept were only the ones this run measured, so `uf test packages/ui` threw
/// away what the last full run had learned about the other two hundred files,
/// and the next full run was scheduled blind — its slowest files started last.
///
/// Returns a note when the cache could not be written; failing to write a cache
/// is never a reason to fail a test run.
pub(crate) fn record_timings(
    root: &Utf8Path,
    mut timings: TestTimings,
    report: &TestRunReport,
    files: &[ProjectFile],
    in_scope: &dyn Fn(&str) -> bool,
) -> Option<String> {
    for file in &report.files {
        if file.status == FileStatus::Completed {
            timings.record(&file.file, file.duration_micros);
        }
    }
    // What a worker cost to start in this run, for the next run to size its
    // pool from. A run that timed no worker keeps the previous estimate rather
    // than forgetting it: one run of a file that crashed its host says nothing
    // about how long a host takes to start.
    if let Some(micros) = report.summary.worker_start_micros {
        timings.record_worker_start(micros);
    }
    timings.retain_files(|recorded| {
        files
            .iter()
            .any(|file| file.relative_path.as_str() == recorded)
            || (!in_scope(recorded) && root.join(recorded).is_file())
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

/// Resolve the concrete application runtime a test run targets.
pub(crate) fn test_application_target(config: &UniflowedConfig) -> TestApplicationTarget {
    let target = config
        .test
        .target
        .unwrap_or_else(|| config.test.native_runner().application_target);
    match target {
        NativeTestApplicationTarget::Web => TestApplicationTarget::Web,
        NativeTestApplicationTarget::ReactNative => TestApplicationTarget::ReactNative,
        NativeTestApplicationTarget::Auto => match config.app.framework {
            FrameworkPreset::ReactNative => TestApplicationTarget::ReactNative,
            FrameworkPreset::Uniflowed | FrameworkPreset::React => TestApplicationTarget::Web,
        },
    }
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
    if summary.foreign_declarations > 0 {
        // Named rather than counted with the rest: `it.each` is a form uf runs
        // and cannot list, and this is a declaration uf cannot run at all. The
        // second is why the run is red, so it is what the message says.
        bail!(
            "uf test found {}, which this runner cannot execute: they register \
             with another runner, so none of them ran",
            plural(
                summary.foreign_declarations,
                "test declaration from another runner"
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
    fn a_deno_version_is_read_from_what_deno_prints() {
        assert_eq!(
            deno_version("deno 2.9.6 (stable, release, aarch64-apple-darwin)\nv8 15.0.245.2\n"),
            Some((2, 9))
        );
        assert_eq!(
            deno_version("deno 1.46.3 (stable, release, x86_64-unknown-linux-gnu)"),
            Some((1, 46))
        );
        assert_eq!(deno_version("deno 2.10.0-rc.1 (canary)"), Some((2, 10)));
        assert_eq!(deno_version("node v26.8.1"), None);
        assert_eq!(deno_version(""), None);
    }

    /// The floor is the release that implemented `registerHooks`, and the
    /// refusal says so — which release it found, which one it needs, and what
    /// to do — rather than leaving the first Flow import to fail per file.
    #[test]
    fn a_deno_without_the_hook_is_refused_by_name() {
        assert!((1, 46) < DENO_WITH_HOOKS);
        assert!((2, 7) < DENO_WITH_HOOKS);
        assert!((2, 8) >= DENO_WITH_HOOKS);
        assert!((2, 10) >= DENO_WITH_HOOKS);
        assert!((3, 0) >= DENO_WITH_HOOKS);

        let message = deno_too_old(Utf8Path::new("/usr/local/bin/deno"), (2, 7));
        for expected in [
            "/usr/local/bin/deno",
            "2.7",
            "2.8",
            "registerHooks",
            "deno upgrade",
        ] {
            assert!(message.contains(expected), "{expected}: {message}");
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

    #[test]
    fn planned_test_files_carry_the_discovered_plan() {
        let planned = planned_test_bearing(vec![
            file(
                "loop.test.js",
                "for (const name of ['a']) {\n  it(name, () => {});\n}\n",
            ),
            file("component.js", "export const Button = () => null;\n"),
        ]);

        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].file.relative_path, "loop.test.js");
        assert_eq!(planned[0].plan.unsupported.len(), 1);

        let runner_files = planned_test_files(camino::Utf8Path::new("/project"), &planned);
        assert_eq!(runner_files.len(), 1);
        assert_eq!(runner_files[0].file.relative, "loop.test.js");
        assert_eq!(runner_files[0].plan, planned[0].plan);
    }

    #[test]
    fn auto_test_application_target_follows_the_framework() {
        let mut config = UniflowedConfig::default();

        assert_eq!(test_application_target(&config), TestApplicationTarget::Web);

        config.app.framework = FrameworkPreset::ReactNative;
        assert_eq!(
            test_application_target(&config),
            TestApplicationTarget::ReactNative
        );
    }

    /// `test.runner` written as the object, where `applicationTarget` is kept
    /// as a legacy fallback for configs that have not moved to `test.target`.
    fn runner_object(target: NativeTestApplicationTarget) -> uf_config::TestRunnerConfig {
        let mut runner = uf_config::NativeTestRunnerConfig::default();
        runner.application_target = target;
        uf_config::TestRunnerConfig::Object(runner)
    }

    #[test]
    fn test_target_wins_over_the_framework_and_the_legacy_object() {
        let mut config = UniflowedConfig::default();
        config.app.framework = FrameworkPreset::ReactNative;
        config.test.target = Some(NativeTestApplicationTarget::Web);
        config.test.runner = Some(runner_object(NativeTestApplicationTarget::ReactNative));

        assert_eq!(test_application_target(&config), TestApplicationTarget::Web);

        config.test.target = Some(NativeTestApplicationTarget::ReactNative);

        assert_eq!(
            test_application_target(&config),
            TestApplicationTarget::ReactNative
        );
    }

    #[test]
    fn legacy_runner_object_target_still_answers_when_test_target_is_absent() {
        let mut config = UniflowedConfig::default();
        config.app.framework = FrameworkPreset::ReactNative;
        config.test.runner = Some(runner_object(NativeTestApplicationTarget::Web));

        assert_eq!(test_application_target(&config), TestApplicationTarget::Web);

        config.test.runner = Some(runner_object(NativeTestApplicationTarget::ReactNative));
        assert_eq!(
            test_application_target(&config),
            TestApplicationTarget::ReactNative
        );
    }

    /// A `PATH` that holds exactly one answer, for the argv[0] route.
    fn path_holding(name: &str, at: &str) -> impl Fn(&str) -> Option<Utf8PathBuf> {
        let name = name.to_owned();
        let at = Utf8PathBuf::from(at);
        move |asked: &str| (asked == name).then(|| at.clone())
    }

    #[test]
    fn the_running_binary_is_what_the_workers_transform_through() {
        let found = resolve_uf_binary(
            Some(Utf8PathBuf::from("/opt/uf/bin/uf")),
            None,
            Some("uf"),
            Some(Utf8Path::new("/work")),
            &path_holding("uf", "/usr/local/bin/uf"),
        );

        // Not the `uf` on PATH, which is a different build.
        assert_eq!(found.as_deref(), Some(Utf8Path::new("/opt/uf/bin/uf")));
    }

    #[test]
    fn an_inherited_uf_binary_does_not_redirect_a_run_that_knows_its_own_path() {
        // The precedence that matters. `UF_BINARY` is set by every `uf` that
        // spawns a child, exported by shells, and named in `docs/security.md`
        // as the variable a cloned repository's `.env` must not be able to
        // answer — so letting it beat the running binary would be the defect
        // this function exists to fix, wearing a different hat: a run
        // compiled by a binary nobody chose, reported under this one's name.
        let found = resolve_uf_binary(
            Some(Utf8PathBuf::from("/opt/uf/bin/uf")),
            Some("/tmp/candidate/uf"),
            None,
            None,
            &path_holding("uf", "/usr/local/bin/uf"),
        );

        assert_eq!(found.as_deref(), Some(Utf8Path::new("/opt/uf/bin/uf")));
    }

    #[test]
    fn an_explicit_uf_binary_answers_when_the_platform_will_not() {
        // The escape hatch, at the one place an escape hatch is needed: a
        // machine where `current_exe` refuses, or answers with a path that is
        // not UTF-8. Both arrive here as `None`.
        let found = resolve_uf_binary(
            None,
            Some("/tmp/candidate/uf"),
            Some("uf"),
            Some(Utf8Path::new("/work")),
            &path_holding("uf", "/usr/local/bin/uf"),
        );

        assert_eq!(found.as_deref(), Some(Utf8Path::new("/tmp/candidate/uf")));
        // Empty is not an answer: `UF_BINARY=` exported by a shell means the
        // variable is unset, not that the binary is called "".
        assert_eq!(
            resolve_uf_binary(
                None,
                Some(""),
                Some("uf"),
                Some(Utf8Path::new("/work")),
                &path_holding("uf", "/usr/local/bin/uf"),
            )
            .as_deref(),
            Some(Utf8Path::new("/usr/local/bin/uf"))
        );
    }

    #[test]
    fn a_current_exe_that_will_not_answer_falls_back_to_how_uf_was_invoked() {
        // `current_exe` failing, and `current_exe` answering with a path that
        // is not UTF-8, arrive here as the same `None` — and both used to set
        // nothing at all, which left the worker resolving a bare `uf` along
        // PATH minutes later. Resolved here instead: same lookup, one process,
        // and the answer is checked.
        let bare = resolve_uf_binary(
            None,
            None,
            Some("uf"),
            Some(Utf8Path::new("/work")),
            &path_holding("uf", "/usr/local/bin/uf"),
        );
        assert_eq!(bare.as_deref(), Some(Utf8Path::new("/usr/local/bin/uf")));

        // A path is a path, and is never looked up on PATH: `./target/release/uf`
        // is emphatically not the installed one.
        let running = std::env::current_exe().expect("this test process has a path");
        let running = Utf8PathBuf::from_path_buf(running).expect("and it is UTF-8");
        let directory = running.parent().expect("with a parent");
        let relative = format!("./{}", running.file_name().expect("and a file name"));
        let by_path = resolve_uf_binary(
            None,
            None,
            Some(&relative),
            Some(directory),
            &path_holding("uf", "/usr/local/bin/uf"),
        );
        assert_eq!(by_path, Some(directory.join(&relative)));
    }

    #[test]
    fn a_run_that_cannot_name_its_own_binary_is_refused() {
        // Every route exhausted: no explicit answer, no `current_exe`, and an
        // argv[0] that names nothing. Returning `None` is what makes `uf test`
        // stop — running would compile the project with whatever `uf` is
        // installed and report it under this binary's name.
        assert_eq!(
            resolve_uf_binary(
                None,
                None,
                Some("uf"),
                Some(Utf8Path::new("/work")),
                &|_| None,
            ),
            None
        );
        assert_eq!(
            resolve_uf_binary(
                None,
                None,
                Some("/no/such/directory/uf"),
                Some(Utf8Path::new("/work")),
                &|_| None,
            ),
            None
        );
        // A login shell passes `-uf`, and a process can be started with no
        // argv[0] at all. Neither is a binary.
        assert_eq!(
            resolve_uf_binary(
                None,
                None,
                Some("-uf"),
                Some(Utf8Path::new("/work")),
                &|_| { None }
            ),
            None
        );
        assert_eq!(
            resolve_uf_binary(None, None, None, Some(Utf8Path::new("/work")), &|_| None),
            None
        );
    }
}
