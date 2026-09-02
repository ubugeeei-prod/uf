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
use uf_config::UniflowedConfig;
use uf_project::{ProjectFile, collect_source_files};
use uf_term::{PhaseTimer, Status};
use uf_test::{ImportGraph, TestFilter, Watcher};

use super::render::render_report;
use super::{TestArgs, read_timings, record_timings, run_once};
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
    args: TestArgs,
) -> Result<()> {
    let mut files = collect_source_files(root, &config)?;
    let mut graph = build_graph(&files);
    let filter = args.filter();

    let mut watcher = Watcher::new(root, args.watch_options());
    prime(&mut watcher, &files);

    let interval = watcher.interval();
    run_and_report(ui, root, &files, &files, &args, None);
    announce(ui, files.len(), interval);

    let mut ticks: u32 = 0;
    loop {
        std::thread::sleep(interval);
        ticks = ticks.wrapping_add(1);

        let triggered = !poll(&mut watcher, &files).is_empty();
        if !triggered && !ticks.is_multiple_of(RESCAN_EVERY) {
            continue;
        }

        let refreshed = collect_source_files(root, &config)?;
        let moved = changed_paths(&files, &refreshed);
        files = refreshed;
        prime(&mut watcher, &files);
        if moved.is_empty() {
            continue;
        }

        refresh_graph(&mut graph, &files, &moved);
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
        run_and_report(ui, root, &files, &subset, &args, Some(&moved));
    }
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
fn run_and_report(
    ui: &mut Ui,
    root: &Utf8Path,
    all_files: &[ProjectFile],
    subset: &[ProjectFile],
    args: &TestArgs,
    moved: Option<&[String]>,
) {
    if let Some(moved) = moved {
        let message = format!(
            "{} changed, re-running {}",
            plural(moved.len(), "file"),
            plural(subset.len(), "test file")
        );
        ui.render(|renderer, out| {
            renderer.blank(out);
            renderer.status(out, Status::Info, &message);
        });
    }

    let mut timer = PhaseTimer::start();
    let (timings, timing_note) = read_timings(root);
    let report = timer.measure("run", || run_once(ui, subset, args, timings.clone()));
    let duration = timer.total();
    let record_note = record_timings(root, timings, &report, all_files);

    render_report(
        ui,
        root,
        subset,
        &report,
        timer.phases(),
        duration,
        args,
        timing_note.as_deref(),
        record_note.as_deref(),
    );
}

fn announce(ui: &mut Ui, files: usize, interval: std::time::Duration) {
    let message = format!(
        "watching {} every {}",
        plural(files, "file"),
        uf_term::format_duration(interval)
    );
    ui.render(|renderer, out| {
        renderer.blank(out);
        renderer.status(out, Status::Info, &message);
    });
}

fn report_no_op(ui: &mut Ui, moved: &[String]) {
    let message = format!("{} changed, nothing to re-run", plural(moved.len(), "file"));
    ui.render(|renderer, out| {
        renderer.blank(out);
        renderer.status(out, Status::Skip, &message);
    });
}

#[cfg(test)]
mod tests;
