//! `uf test --shard` and `uf test --merge-shards`, where they meet the project.
//!
//! [`uf_test::shard_files`] cuts a partition and [`uf_test::merge_shards`] puts
//! one back together, and neither touches a file. This is the half that does:
//! which of the project's test files a shard runs, where its record goes, where
//! a merge finds the records, and how the merged report reaches the terminal,
//! `--json`, JUnit and the coverage reports. It goes through the functions an
//! ordinary run reports with, so a merged suite is reported the way a whole one
//! is.

use std::collections::BTreeSet;
use std::io::Read;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::load_config;
use uf_project::{ProjectFile, scan_selected_source_files};
use uf_term::{KeyValue, PhaseTimer};
use uf_test::{Coverage, SHARD_RECORD_VERSION, Shard, ShardRecord, TestRunReport, TestTimings};

use super::payload::test_payload;
use super::render::render_report;
use super::{TestArgs, coverage, finish, read_timings, record_timings, write_results_report};
use crate::cli::ResultReporterArg;
use crate::support::plural;
use crate::ui::Ui;

/// Where a shard writes its record, and where `--merge-shards` looks when it is
/// not told.
pub(crate) const RECORDS_DIRECTORY: &str = ".uf/test-shards";

/// Largest record `--merge-shards` reads.
///
/// A record is a run's report, captured output included, of which the worker
/// keeps at most [`uf_test::MAX_OUTPUT_BYTES_PER_FILE`] per file. A file past
/// this is not something a shard wrote.
const MAX_RECORD_BYTES: u64 = 256 * 1024 * 1024;

/// The part of the suite one shard runs.
#[derive(Debug)]
pub(crate) struct Cut {
    shard: Shard,
    suite_files: usize,
    fingerprint: String,
    /// The test files this shard runs.
    pub(crate) files: Vec<ProjectFile>,
}

/// Cut `shard` from `tests` — the test files this run would run without it —
/// and say how much of the suite that is.
///
/// The schedule is the one [`uf_test::TestRunner`] makes for the same files:
/// the same path filter, and `timings`, which have to be the timings the run is
/// given. A shard that cut from one set of durations and scheduled from another
/// would still run a part of the suite, but not the part the fingerprint in
/// its record describes.
pub(crate) fn cut(
    ui: &mut Ui,
    tests: &[ProjectFile],
    args: &TestArgs,
    timings: &TestTimings,
    shard: Shard,
) -> Cut {
    let filter = args.filter();
    let sources: Vec<(&str, &str)> = tests
        .iter()
        .filter(|file| filter.matches_path(&file.relative_path))
        .map(|file| (file.relative_path.as_str(), file.source.as_str()))
        .collect();
    let schedule = uf_test::schedule_files(&sources, timings);
    let chosen: BTreeSet<&str> = uf_test::shard_files(&schedule, shard)
        .into_iter()
        .map(|entry| entry.file.as_str())
        .collect();
    let files: Vec<ProjectFile> = tests
        .iter()
        .filter(|file| chosen.contains(file.relative_path.as_str()))
        .cloned()
        .collect();
    let summary = uf_infra::cstr!(
        "{shard} · {} of {}",
        files.len(),
        plural(schedule.len(), "test file")
    )
    .into_string();
    ui.render(|renderer, out| {
        renderer.key_values(out, 2, &[KeyValue::new("shard", &summary)]);
    });
    Cut {
        shard,
        suite_files: schedule.len(),
        fingerprint: uf_test::partition_fingerprint(&schedule, shard.count()),
        files,
    }
}

/// Write `report`, and what the shard measured when it measured, as this
/// shard's record. Returns where it went, relative to `root` when it is inside.
pub(crate) fn write_record(
    root: &Utf8Path,
    cut: &Cut,
    report: &TestRunReport,
    measured: Option<&Coverage>,
) -> Result<String> {
    let record = ShardRecord {
        version: SHARD_RECORD_VERSION,
        shard: cut.shard,
        suite_files: cut.suite_files,
        fingerprint: cut.fingerprint.clone(),
        report: report.clone(),
        coverage: measured.cloned(),
    };
    let directory = root.join(RECORDS_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .with_context(|| uf_infra::cstr!("could not create {directory}"))?;
    let path = directory.join(record_name(cut.shard));
    let body = serde_json::to_vec(&record).context("could not serialise the shard record")?;
    std::fs::write(&path, body).with_context(|| uf_infra::cstr!("could not write {path}"))?;
    Ok(path
        .strip_prefix(root)
        .map_or_else(|_| path.to_string(), ToString::to_string))
}

/// Say where a shard's record went.
pub(crate) fn announce(ui: &mut Ui, shown: &str) {
    ui.render(|renderer, out| {
        renderer.key_values(out, 2, &[KeyValue::new("record", shown)]);
    });
}

/// The file a shard's record is written to: `2-of-3.json`.
fn record_name(shard: Shard) -> String {
    uf_infra::into_string(uf_infra::cstr!(
        "{}-of-{}.json",
        shard.index(),
        shard.count()
    ))
}

/// Whether `name` is a name [`record_name`] writes.
fn is_record_name(name: &str) -> bool {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    name.strip_suffix(".json")
        .and_then(|stem| stem.split_once("-of-"))
        .is_some_and(|(index, count)| digits(index) && digits(count))
}

/// `uf test --merge-shards DIR`: the shard records in `directory`, reported as
/// one run over the whole suite.
///
/// Runs nothing. It writes what that run would have written: the summary or
/// `--json`, `--reporter junit`, and the coverage reports and thresholds when
/// the shards measured. It records the merged durations, so a timings cache
/// saved after the merge gives the next shards the whole suite to cut from.
pub(crate) fn merge(cwd: &Utf8Path, ui: &mut Ui, directory: &str, args: &TestArgs) -> Result<()> {
    let refused = merge_refused_flags(args);
    if !refused.is_empty() {
        bail!(uf_infra::cstr!(
            "`uf test --merge-shards` runs nothing, so it cannot take {}: {}",
            plural(refused.len(), "flag"),
            refused.join(", ")
        ));
    }
    let resolved = load_config(cwd)?;
    let root = &resolved.root;
    let mut timer = PhaseTimer::start();
    let merged = timer.measure("merge", || {
        let records = read_records(root, directory)?;
        uf_test::merge_shards(records).map_err(|error| anyhow!(uf_infra::cstr!("{error}")))
    })?;

    // Every file the project has, for the code frames under a failure and for
    // keeping the recorded durations to files that still exist.
    let files = scan_selected_source_files(root, &resolved.config, &Vec::<String>::new())?.files;
    let settings = &resolved.config.test.coverage;
    let section = merged
        .coverage
        .as_ref()
        .map(|measured| {
            coverage::report(
                settings,
                measured,
                &coverage::directory_for(root, settings, args.coverage_dir.as_deref()),
                &args.coverage_reporters,
            )
        })
        .transpose()?;
    let duration = timer.total();

    if let Some(ResultReporterArg::Junit) = args.reporter {
        write_results_report(root, args, &merged.report)?;
    }
    let (timings, timing_note) = read_timings(root);
    let recorded = record_timings(root, timings, &merged.report, &files, &|_| true);
    if args.json {
        let mut document = test_payload(None, &merged.report, merged.coverage.as_ref());
        document["shards"] = serde_json::json!({
            "count": merged.count,
            "fingerprint": merged.fingerprint,
        });
        ui.json(&document)?;
    } else {
        let summary = uf_infra::cstr!(
            "{} from {} · fingerprint {}",
            plural(merged.report.summary.files, "test file"),
            plural(usize::try_from(merged.count).unwrap_or(usize::MAX), "shard"),
            merged.fingerprint
        )
        .into_string();
        ui.render(|renderer, out| {
            renderer.key_values(out, 2, &[KeyValue::new("merged", &summary)]);
        });
        render_report(
            ui,
            root,
            &files,
            &merged.report,
            timer.phases(),
            duration,
            args,
            None,
            timing_note.as_deref(),
            recorded.as_deref(),
            section.as_ref(),
            // A merge ran nothing, so nothing was drawn as it went: the files
            // are drawn here, in path order, from the shards' records.
            false,
        );
    }
    finish(
        &merged.report,
        section
            .as_ref()
            .map_or(&[][..], |section| &section.violations),
    )
}

/// Every flag in `args` that shapes a run, which a merge does not make.
///
/// `--coverage` among them: whether a merge reports coverage is decided by
/// whether the shards measured it.
fn merge_refused_flags(args: &TestArgs) -> Vec<&'static str> {
    [
        (args.list, "--list"),
        (args.watch, "--watch"),
        (args.changed.is_some(), "--changed"),
        (args.filter.is_some(), "-t"),
        (args.bail.is_some(), "--bail"),
        (args.retry > 0, "--retry"),
        (args.update_snapshots, "--update-snapshots"),
        (args.browser, "--browser"),
        (args.threads.is_some(), "-j"),
        (args.watch_interval.is_some(), "--watch-interval"),
        (args.coverage, "--coverage"),
        (args.bench, "--bench"),
        (args.mode.is_some(), "--mode"),
        (!args.paths.is_empty(), "PATH"),
    ]
    .into_iter()
    .filter_map(|(present, flag)| present.then_some(flag))
    .collect()
}

/// Every shard record in `directory`, and in the directories directly inside
/// it, in path order.
///
/// One level down as well, because that is how a CI system hands back the
/// artefacts several jobs uploaded: each in a directory of its own.
fn read_records(root: &Utf8Path, directory: &str) -> Result<Vec<ShardRecord>> {
    let directory = if Utf8Path::new(directory).is_absolute() {
        Utf8PathBuf::from(directory)
    } else {
        root.join(directory)
    };
    let mut paths = Vec::new();
    record_paths(&directory, 0, &mut paths).with_context(|| {
        uf_infra::into_string(uf_infra::cstr!(
            "could not read the shard records in {directory}"
        ))
    })?;
    if paths.is_empty() {
        bail!(uf_infra::cstr!(
            "there are no shard records in {directory}: each `uf test --shard <index>/<count>` \
             writes one into {RECORDS_DIRECTORY}; put every shard's record in one directory and \
             name it"
        ));
    }
    if paths.len() > usize::try_from(uf_test::MAX_SHARDS).unwrap_or(usize::MAX) {
        bail!(uf_infra::cstr!(
            "{directory} holds {} shard records, more than the {} shards a suite can be split into",
            paths.len(),
            uf_test::MAX_SHARDS
        ));
    }
    paths.sort();
    paths.iter().map(|path| read_record(path)).collect()
}

/// The paths of the records in `directory`, looking one directory further down
/// from the top.
fn record_paths(
    directory: &Utf8Path,
    depth: usize,
    out: &mut Vec<Utf8PathBuf>,
) -> std::io::Result<()> {
    for entry in directory.read_dir_utf8()? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() && depth == 0 {
            record_paths(entry.path(), 1, out)?;
        } else if kind.is_file() && is_record_name(entry.file_name()) {
            out.push(entry.path().to_path_buf());
        }
    }
    Ok(())
}

/// One record, read no further than [`MAX_RECORD_BYTES`].
fn read_record(path: &Utf8Path) -> Result<ShardRecord> {
    let file =
        std::fs::File::open(path).with_context(|| uf_infra::cstr!("could not open {path}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| uf_infra::cstr!("could not read {path}"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_RECORD_BYTES {
        bail!(uf_infra::cstr!(
            "{path} is larger than the {MAX_RECORD_BYTES} bytes a shard record can be"
        ));
    }
    serde_json::from_slice(&bytes)
        .with_context(|| uf_infra::cstr!("{path} is not a shard record uf reads"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shard(index: u32, count: u32) -> Shard {
        Shard::new(index, count).expect("a shard")
    }

    #[test]
    fn a_record_is_named_for_its_shard_and_nothing_else_is_read_as_one() {
        assert_eq!(record_name(shard(2, 3)), "2-of-3.json");
        assert!(is_record_name("2-of-3.json"));
        assert!(is_record_name("10-of-12.json"));
        for name in [
            "junit.xml",
            "2-of-3.json.tmp",
            "a-of-3.json",
            "2-of-.json",
            "-of-3.json",
            "2of3.json",
            "test-timings.json",
        ] {
            assert!(!is_record_name(name), "{name}");
        }
    }

    #[test]
    fn a_merge_refuses_every_flag_that_shapes_a_run_by_name() {
        let shaping = TestArgs {
            threads: Some(2),
            filter: Some(String::from("adds")),
            paths: vec![String::from("src")],
            ..TestArgs::default()
        };
        assert_eq!(merge_refused_flags(&shaping), vec!["-t", "-j", "PATH"]);

        let reporting = TestArgs {
            json: true,
            reporter: Some(ResultReporterArg::Junit),
            reporter_outfile: Some(String::from("junit.xml")),
            coverage_dir: Some(String::from("coverage")),
            ..TestArgs::default()
        };
        assert!(merge_refused_flags(&reporting).is_empty());
    }

    #[test]
    fn records_are_read_from_the_directory_and_the_directories_directly_inside_it() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let root = Utf8Path::from_path(directory.path()).expect("a UTF-8 path");
        let write = |relative: &str, part: Shard| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
            let record = ShardRecord {
                version: SHARD_RECORD_VERSION,
                shard: part,
                suite_files: 0,
                fingerprint: String::from("f"),
                report: TestRunReport::default(),
                coverage: None,
            };
            std::fs::write(&path, serde_json::to_vec(&record).expect("serialises"))
                .expect("write the record");
        };
        write("shards/test-shard-1/1-of-2.json", shard(1, 2));
        write("shards/2-of-2.json", shard(2, 2));
        // Two directories down: not where a CI system puts an artefact.
        write("shards/a/b/9-of-9.json", shard(9, 9));
        std::fs::write(root.join("shards/notes.json"), "{}").expect("an unrelated file");

        let records = read_records(root, "shards").expect("the records are read");

        let mut shards: Vec<String> = records
            .iter()
            .map(|record| record.shard.to_string())
            .collect();
        shards.sort();
        assert_eq!(shards, ["1/2", "2/2"]);

        let missing = read_records(root, "nowhere").expect_err("no such directory");
        assert!(
            missing
                .to_string()
                .contains("could not read the shard records"),
            "{missing:#}"
        );
        std::fs::create_dir_all(root.join("empty")).expect("an empty directory");
        let empty = read_records(root, "empty").expect_err("no records");
        assert!(
            empty.to_string().contains("there are no shard records"),
            "{empty:#}"
        );
    }

    #[test]
    fn a_record_that_is_not_one_names_its_file() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let root = Utf8Path::from_path(directory.path()).expect("a UTF-8 path");
        std::fs::create_dir_all(root.join("shards")).expect("a directory");
        std::fs::write(root.join("shards/1-of-1.json"), "{\"version\": 1}").expect("write");

        let error = read_records(root, "shards").expect_err("not a record");

        assert!(
            error
                .to_string()
                .contains("1-of-1.json is not a shard record"),
            "{error:#}"
        );
    }
}
