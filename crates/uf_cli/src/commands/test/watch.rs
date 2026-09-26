//! `uf test --watch`: run once, then run exactly what each edit invalidated.
//!
//! The loop is deliberately dull. Notice a change, ask the import graph what the
//! changed files reach, re-run that set and nothing else. The interesting
//! property is what it does *not* do: an edit to a module no test imports
//! produces no run at all, and an edit to a module two tests share produces a
//! run of exactly those two.
//!
//! # Two layers, on purpose
//!
//! Stat-based polling is the cheap **trigger**: it costs one `stat` per source
//! file per interval and notices nothing but a moved length or modification
//! time. Re-collecting the project is the **truth**: it compares actual file
//! contents, so it cannot be fooled by a file system whose modification times
//! have one-second granularity, and it is the only thing that sees files
//! created or deleted.
//!
//! The trigger runs every interval; the truth runs when the trigger fires, and
//! unconditionally every [`RESCAN_EVERY`] intervals so that a newly created test
//! file is picked up within a bounded time even though nothing existing moved.

use anyhow::Result;
use camino::Utf8Path;
use compact_str::CompactString;
use uf_config::UniflowedConfig;
use uf_config::env_files::ProjectEnv;
use uf_project::{ProjectFile, scan_selected_source_files};
use uf_term::{PhaseTimer, Status};
use uf_test::{ImportGraph, TestFilter, Watcher, WorkerPool};

use super::render::render_report;
use super::{TestArgs, read_timings, record_timings, run_once};
use crate::commands::vite::Host;
use crate::support::plural;
use crate::ui::Ui;

/// Re-read the project unconditionally every this many polls.
///
/// At the default interval that is roughly two seconds, which bounds how long a
/// brand new test file can sit unnoticed.
const RESCAN_EVERY: u32 = 8;

/// Run once, then keep running whatever each change invalidates.
///
/// Never returns on its own: watch mode ends when the developer stops it, which
/// is why the exit status of the last run is not this function's business.
pub(super) fn watch(
    ui: &mut Ui,
    root: &Utf8Path,
    config: UniflowedConfig,
    env: &ProjectEnv,
    runtime: Host,
    args: TestArgs,
) -> Result<()> {
    // The one-shot `uf test` has already refused to start on a file it could
    // not read, so a watch session either began without any or is about to be
    // told about one it did not have before. Either way it keeps watching:
    // exiting a watch loop because somebody saved a binary is hostile.
    // The paths this session was started with, so a watch narrowed to a
    // directory `.gitignore` names sees the same files the one-shot run does.
    let mut files = scan_selected_source_files(root, &config, &args.paths)?.files;
    // Resolved once: a watch session that lost its host between runs would be
    // reporting a different failure than the one the user is editing towards.
    // Resolved once, with the environment the session started with: a watch
    // that reloaded `.env` mid-session would change what the suite means
    // between two runs of the same file.
    // Kept between runs, so a rerun costs the files it runs and what the edit
    // touched rather than a host start and the whole dependency graph each
    // time; see `uf_test::WorkerPool` for what keeps a kept worker from
    // running the code as it was before the edit. A browser's driver reloads
    // its page for every file already and has nothing to keep.
    let keep = !args.browser;
    let host = super::test_host(root, &config, env, args.browser, runtime)?
        .with_axe(config.accessibility.axe.as_json())
        .with_kept_workers(keep);
    let pool = keep.then(WorkerPool::new);
    // Every host, Deno included. Deno used to be refused here, because its Flow
    // loader was an ahead-of-time pass and a watch session would have kept
    // re-running the tree the first pass wrote. Its loader is a hook now
    // (`@uniflowed/host/deno-preload`), and the worker re-imports a file with a
    // fresh `?uf-run=` query on every run, which the hook is asked about like
    // any other import — so an edit reaches the next run on Deno exactly as it
    // does on Node. `crates/uf_cli/tests/deno_host.rs` edits a file under a
    // running watch to hold that. See ubugeeei-prod/uf#246.
    let mut graph = build_graph(&files);
    let filter = args.filter();

    let mut watcher = Watcher::new(root, args.watch_options());
    prime(&mut watcher, &files);

    // A chosen `--watch-interval` is taken as asked. Otherwise the watcher
    // waits as long as polling costs about two per cent of a core — 50 ms on a
    // selection of a few hundred files, up to the old 250 ms on a large one —
    // because a save noticed a quarter of a second late was most of the time
    // between it and a rerun that now takes tens of milliseconds.
    let chosen = args.watch_interval.is_some();
    let mut interval = watcher.interval();
    // Kernel file events when nobody chose an interval and this machine can
    // give them, so a save is heard when it happens rather than at the next
    // look; see `uf_test::EventWatcher`. A chosen interval is a request to
    // poll, and a file system that sends no events is polled as before.
    let (mut events, unavailable) = if chosen {
        (None, None)
    } else {
        match start_events(root, &files) {
            Ok(source) => (Some(source), None),
            Err(error) => (None, Some(error.to_string())),
        }
    };
    run_and_report(ui, root, &host, &files, &files, &args, None, pool.as_ref());
    announce(
        ui,
        files.len(),
        events.is_none().then_some(interval),
        unavailable.as_deref(),
    );

    let mut ticks: u32 = 0;
    loop {
        if let Some(source) = events.as_mut() {
            let moved = match source.wait(EVENT_RESCAN_EVERY) {
                Some(batch) if !batch.rescan => match reread_heard(&mut files, &batch.changed) {
                    Heard::Nothing => continue,
                    Heard::Read(moved) => moved,
                    Heard::Rescan => rescan(root, &config, &args, &mut files)?,
                },
                // Nothing for a while, or the backend lost events: the walk is
                // what sees a file created where no watched file was, and what
                // catches whatever a dropped event would have said.
                _ => rescan(root, &config, &args, &mut files)?,
            };
            // A directory that gained its first watched file is heard from from
            // now on. One the backend refuses is left to the periodic rescan.
            let _ = source.watch_directories(files.iter().map(|file| file.relative_path.as_str()));
            if moved.is_empty() {
                continue;
            }
            refresh_graph(&mut graph, &files, &moved);
            if let Some(pool) = &pool {
                pool.invalidate(&loaded_paths(root, &moved));
            }
            let rerun = affected(&graph, &moved, &files, &filter);
            if rerun.is_empty() {
                report_no_op(ui, &moved);
                continue;
            }
            let subset: Vec<ProjectFile> = files
                .iter()
                .filter(|file| rerun.iter().any(|path| path == &file.relative_path))
                .cloned()
                .collect();
            run_and_report(
                ui,
                root,
                &host,
                &files,
                &subset,
                &args,
                Some(&moved),
                pool.as_ref(),
            );
            continue;
        }

        std::thread::sleep(interval);
        ticks = ticks.wrapping_add(1);

        let polled = std::time::Instant::now();
        let changes = poll(&mut watcher, &files);
        if !chosen {
            interval = uf_test::adaptive_interval(polled.elapsed());
        }
        let triggered = !changes.is_empty();
        if !triggered && !ticks.is_multiple_of(RESCAN_EVERY) {
            continue;
        }

        // The files the trigger named, read again and nothing else, when that
        // is all that happened: an edit to a file the session already knows.
        // A file that appeared or went, or the periodic rescan, still reads the
        // whole selection, because only the walk can see a new file.
        let reread = (triggered
            && !ticks.is_multiple_of(RESCAN_EVERY)
            && changes.added.is_empty()
            && changes.removed.is_empty())
        .then(|| reread_modified(&mut files, &changes.modified))
        .flatten();
        let moved = match reread {
            Some(moved) => moved,
            None => {
                let refreshed = scan_selected_source_files(root, &config, &args.paths)?.files;
                let moved = changed_paths(&files, &refreshed);
                files = refreshed;
                prime(&mut watcher, &files);
                moved
            }
        };
        if moved.is_empty() {
            continue;
        }

        refresh_graph(&mut graph, &files, &moved);
        if let Some(pool) = &pool {
            pool.invalidate(&loaded_paths(root, &moved));
        }
        let rerun = affected(&graph, &moved, &files, &filter);
        if rerun.is_empty() {
            report_no_op(ui, &moved);
            continue;
        }

        let subset: Vec<ProjectFile> = files
            .iter()
            .filter(|file| rerun.iter().any(|path| path == &file.relative_path))
            .cloned()
            .collect();
        run_and_report(
            ui,
            root,
            &host,
            &files,
            &subset,
            &args,
            Some(&moved),
            pool.as_ref(),
        );
    }
}

/// The files that moved, as the paths a worker's loader resolved them to.
///
/// Canonical, because the loader's paths are: a project under a symlinked
/// directory (`/tmp` on macOS is `/private/tmp`) would otherwise name one
/// file two ways and the worker would find nothing to reload. A file that has
/// gone cannot be canonicalised and is named as it was.
fn loaded_paths(root: &Utf8Path, moved: &[String]) -> Vec<String> {
    let base = std::fs::canonicalize(root.as_std_path())
        .ok()
        .and_then(|path| camino::Utf8PathBuf::from_path_buf(path).ok())
        .unwrap_or_else(|| root.to_path_buf());
    moved
        .iter()
        .map(|relative| {
            let path = base.join(relative);
            std::fs::canonicalize(path.as_std_path())
                .ok()
                .and_then(|canonical| canonical.into_os_string().into_string().ok())
                .unwrap_or_else(|| path.into_string())
        })
        .collect()
}

fn prime(watcher: &mut Watcher, files: &[ProjectFile]) {
    let paths: Vec<&str> = files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    watcher.prime(paths);
}

fn poll(watcher: &mut Watcher, files: &[ProjectFile]) -> uf_test::ChangeSet {
    let paths: Vec<&str> = files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    watcher.poll(paths)
}

fn build_graph(files: &[ProjectFile]) -> ImportGraph {
    ImportGraph::build(
        files
            .iter()
            .map(|file| (file.relative_path.as_str(), file.source.as_str())),
    )
}

/// How long an event-driven session goes without an event before it walks the
/// selection anyway.
///
/// The walk is what finds a file created in a directory that held no watched
/// file — no subscription covered it — and what catches whatever a dropped
/// event would have said. Five seconds rather than the polling loop's two,
/// because here it is the only cost an idle session has.
const EVENT_RESCAN_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// How long a session waits to hear about its own probe file before it decides
/// events are not arriving and polls instead.
const EVENT_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Subscribe to kernel events for the project and every directory holding a
/// watched file, and prove they arrive.
fn start_events(
    root: &Utf8Path,
    files: &[ProjectFile],
) -> Result<uf_test::EventWatcher, uf_test::EventError> {
    let mut source = uf_test::EventWatcher::new(root)?;
    source.watch_directories(files.iter().map(|file| file.relative_path.as_str()))?;
    // Heard, not only subscribed: a sandbox that denies the file-event service
    // lets the watch start and then says nothing, and a session that trusted
    // that would never see a save.
    if !source.verify(&root.join(".uf"), EVENT_PROBE_TIMEOUT)? {
        return Err(uf_test::EventError::generic(
            "the file system accepted the watch and reported nothing",
        ));
    }
    Ok(source)
}

/// What a burst of events means for the session.
#[derive(Debug, PartialEq, Eq)]
enum Heard {
    /// Only paths the session has no use for: its own cache, a dependency, an
    /// editor's swap file.
    Nothing,
    /// Files the session already holds, read again; the ones whose text moved.
    Read(Vec<String>),
    /// Something only the walk can answer: a new file, one that went, one that
    /// could not be read.
    Rescan,
}

/// Decide what the paths in one burst of events call for, reading again the
/// ones the session already holds.
fn reread_heard(files: &mut [ProjectFile], heard: &[CompactString]) -> Heard {
    let relevant: Vec<&CompactString> = heard.iter().filter(|path| !never_watched(path)).collect();
    if relevant.is_empty() {
        return Heard::Nothing;
    }
    let (known, unknown): (Vec<CompactString>, Vec<&CompactString>) = {
        let mut known = Vec::new();
        let mut unknown = Vec::new();
        for path in relevant {
            if files.iter().any(|file| file.relative_path == path.as_str()) {
                known.push(path.clone());
            } else {
                unknown.push(path);
            }
        }
        (known, unknown)
    };
    if unknown.iter().any(|path| might_be_source(path)) {
        return Heard::Rescan;
    }
    if known.is_empty() {
        return Heard::Nothing;
    }
    match reread_modified(files, &known) {
        Some(moved) => Heard::Read(moved),
        None => Heard::Rescan,
    }
}

/// Paths no selection ever includes: what uf and the package manager write.
///
/// Heard because FSEvents reports a directory's whole subtree however it was
/// asked, and `uf test` itself writes `.uf/test-timings.json` after every run;
/// a session that rescanned on its own writes would walk the project after
/// every run for nothing.
fn never_watched(path: &str) -> bool {
    path.split('/')
        .any(|part| matches!(part, ".uf" | ".git" | "node_modules" | "target" | "dist"))
}

/// Whether a path the session does not hold could be a new source file, and
/// so worth a walk. An editor's swap file, a lock file, a build log are not.
fn might_be_source(path: &str) -> bool {
    matches!(
        path.rsplit_once('.').map(|(_, extension)| extension),
        Some("js" | "jsx" | "mjs" | "cjs" | "json")
    )
}

/// Walk the selection again and say which paths moved, keeping the new list.
fn rescan(
    root: &Utf8Path,
    config: &UniflowedConfig,
    args: &TestArgs,
    files: &mut Vec<ProjectFile>,
) -> Result<Vec<String>> {
    let refreshed = scan_selected_source_files(root, config, &args.paths)?.files;
    let moved = changed_paths(files, &refreshed);
    *files = refreshed;
    Ok(moved)
}

/// Read again the files the watcher saw move, and say which of them changed.
///
/// The fast path of the loop: an edit to a file the session already holds is
/// one `stat` that moved and one read, rather than a walk and a read of every
/// file in the selection — which on a repository of a few thousand files is
/// most of the time between a save and a rerun. The truth is still contents,
/// as in [`changed_paths`]: a file whose stamp moved and whose text did not
/// (a `touch`, a save of the same bytes) is not a change.
///
/// `None` when any of them cannot be read or is not one the session holds —
/// it went between the poll and the read, or it is new — and the caller then
/// rescans the whole selection, which is what sees those.
fn reread_modified(files: &mut [ProjectFile], modified: &[CompactString]) -> Option<Vec<String>> {
    let mut read = Vec::with_capacity(modified.len());
    for path in modified {
        let index = files
            .iter()
            .position(|file| file.relative_path == path.as_str())?;
        let source = std::fs::read_to_string(files[index].absolute_path.as_std_path()).ok()?;
        read.push((index, source));
    }
    let mut moved = Vec::new();
    for (index, source) in read {
        if files[index].source != source {
            files[index].source = source;
            moved.push(files[index].relative_path.clone());
        }
    }
    Some(moved)
}

/// Every path that was added, removed, or whose contents differ.
///
/// Both lists arrive sorted by path, so this is one merge rather than a
/// quadratic scan. Comparing contents rather than timestamps is what makes the
/// answer exact.
fn changed_paths(previous: &[ProjectFile], current: &[ProjectFile]) -> Vec<String> {
    let mut changed = Vec::new();
    let (mut left, mut right) = (0usize, 0usize);

    while left < previous.len() && right < current.len() {
        let before = &previous[left];
        let after = &current[right];
        match before.relative_path.cmp(&after.relative_path) {
            std::cmp::Ordering::Less => {
                changed.push(before.relative_path.clone());
                left += 1;
            }
            std::cmp::Ordering::Greater => {
                changed.push(after.relative_path.clone());
                right += 1;
            }
            std::cmp::Ordering::Equal => {
                if before.source != after.source {
                    changed.push(after.relative_path.clone());
                }
                left += 1;
                right += 1;
            }
        }
    }
    changed.extend(
        previous[left..]
            .iter()
            .map(|file| file.relative_path.clone()),
    );
    changed.extend(
        current[right..]
            .iter()
            .map(|file| file.relative_path.clone()),
    );
    changed
}

/// Bring the graph back in step with the project.
///
/// A module's outgoing edges depend on its own source alone, so only the files
/// that moved are rescanned. A file that has gone leaves the graph entirely, so
/// a deleted module cannot keep invalidating its old importers.
fn refresh_graph(graph: &mut ImportGraph, files: &[ProjectFile], moved: &[String]) {
    for path in moved {
        match files.iter().find(|file| &file.relative_path == path) {
            Some(file) => graph.insert(&file.relative_path, &file.source),
            None => graph.remove(path),
        }
    }
}

/// The test files a change invalidated.
///
/// A file counts as a test file when it declares at least one runnable test,
/// which is exact and needs no naming convention. The path filter still
/// applies, so `uf test --watch src/checkout` stays inside that directory.
fn affected(
    graph: &ImportGraph,
    moved: &[String],
    files: &[ProjectFile],
    filter: &TestFilter,
) -> Vec<String> {
    graph
        .affected_tests(moved.iter().map(String::as_str), |path| {
            filter.matches_path(path)
                && files.iter().any(|file| {
                    file.relative_path == path
                        && uf_test::discover_tests(path, &file.source).runnable_count() > 0
                })
        })
        .into_iter()
        .map(|path| path.to_string())
        .collect()
}

/// Run `subset`, recording timings against the whole project so that files this
/// cycle did not touch keep the durations they already had.
#[allow(clippy::too_many_arguments)]
fn run_and_report(
    ui: &mut Ui,
    root: &Utf8Path,
    host: &uf_test::HostCommand,
    all_files: &[ProjectFile],
    subset: &[ProjectFile],
    args: &TestArgs,
    moved: Option<&[String]>,
    pool: Option<&WorkerPool>,
) {
    if let Some(moved) = moved {
        let message = uf_infra::cstr!(
            "{} changed, re-running {}",
            plural(moved.len(), "file"),
            plural(subset.len(), "test file")
        )
        .into_string();
        ui.render(|renderer, out| {
            renderer.blank(out);
            renderer.status(out, Status::Info, &message);
        });
    }

    let mut timer = PhaseTimer::start();
    let (timings, timing_note) = read_timings(root);
    let report = timer.measure("run", || {
        run_once(ui, root, host, subset, args, timings.clone(), pool)
    });
    let duration = timer.total();
    // A run that could not start at all is reported and the watch continues:
    // a host that went away is a thing to fix, not a reason to lose the
    // session.
    let report = match report {
        Ok(report) => report,
        Err(error) => {
            let message = error.to_string();
            ui.render(|renderer, out| {
                renderer.blank(out);
                renderer.status(out, Status::Error, &message);
            });
            return;
        }
    };
    let record_note = record_timings(root, timings, &report, all_files, &|_| true);

    render_report(
        ui,
        root,
        subset,
        &report,
        timer.phases(),
        duration,
        args,
        Some(host),
        timing_note.as_deref(),
        record_note.as_deref(),
        // Watch mode collects none: `uf test --watch --coverage` is refused,
        // because a report over the files one edit invalidated is not the
        // project's coverage.
        None,
        true,
    );
}

/// Say what the session is watching, and how.
///
/// With kernel events there is no interval to print. A chosen interval is
/// printed; an adaptive one is not, because it follows what a poll costs and
/// moves from one poll to the next. When events were wanted and could not be
/// had, the reason is printed, so a session that is slower than it should be
/// says why.
fn announce(
    ui: &mut Ui,
    files: usize,
    interval: Option<std::time::Duration>,
    unavailable: Option<&str>,
) {
    let mut message = match interval {
        Some(interval) if unavailable.is_none() => uf_infra::cstr!(
            "watching {} every {}",
            plural(files, "file"),
            uf_term::format_duration(interval)
        )
        .into_string(),
        _ => uf_infra::cstr!("watching {}", plural(files, "file")).into_string(),
    };
    if let Some(reason) = unavailable {
        uf_infra::append!(
            message,
            " by polling; file events are unavailable: {reason}"
        );
    }
    ui.render(|renderer, out| {
        renderer.blank(out);
        renderer.status(out, Status::Info, &message);
    });
}

fn report_no_op(ui: &mut Ui, moved: &[String]) {
    let message =
        uf_infra::cstr!("{} changed, nothing to re-run", plural(moved.len(), "file")).into_string();
    ui.render(|renderer, out| {
        renderer.blank(out);
        renderer.status(out, Status::Skip, &message);
    });
}

#[cfg(test)]
mod tests;
