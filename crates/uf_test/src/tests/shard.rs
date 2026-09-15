//! Splitting a suite across shards, and merging it back.

use crate::coverage::Coverage;
use crate::discovery::{discover_tests, merge_plans};
use crate::report::{FileReport, FileStatus, TestRecord, TestRunReport, TestStatus, TestSummary};
use crate::schedule::{ScheduleBasis, ScheduleEntry};
use crate::shard::{
    MAX_SHARDS, SHARD_RECORD_VERSION, Shard, ShardError, ShardMergeError, ShardRecord,
    merge_shards, partition_fingerprint, shard_files,
};

/// A schedule of one file per weight, in `schedule_files` order: heaviest
/// first, then by path.
fn entries(weights: &[u64]) -> Vec<ScheduleEntry> {
    let mut entries: Vec<ScheduleEntry> = weights
        .iter()
        .enumerate()
        .map(|(index, weight)| ScheduleEntry {
            index,
            file: format!("src/f{index:02}.test.js").into(),
            weight_micros: *weight,
            basis: ScheduleBasis::Recorded,
        })
        .collect();
    entries.sort_by(|a, b| {
        b.weight_micros
            .cmp(&a.weight_micros)
            .then(a.file.cmp(&b.file))
    });
    entries
}

fn shard(index: u32, count: u32) -> Shard {
    Shard::new(index, count).expect("a shard")
}

#[test]
fn a_shard_is_written_as_which_one_and_how_many() {
    let parsed: Shard = "2/3".parse().expect("a shard");

    assert_eq!((parsed.index(), parsed.count()), (2, 3));
    assert_eq!(parsed.to_string(), "2/3");
    assert_eq!("1/1".parse::<Shard>(), Ok(shard(1, 1)));
}

#[test]
fn a_shard_that_is_not_one_is_refused_for_what_is_wrong_with_it() {
    for text in [
        "", "3", "1/", "/3", "a/3", "1/3/5", "+1/3", " 1/3", "1 /3", "-1/3",
    ] {
        assert!(
            matches!(text.parse::<Shard>(), Err(ShardError::Malformed(_))),
            "{text:?}"
        );
    }
    assert_eq!(
        "0/3".parse::<Shard>(),
        Err(ShardError::Index { index: 0, count: 3 })
    );
    assert_eq!(
        "4/3".parse::<Shard>(),
        Err(ShardError::Index { index: 4, count: 3 })
    );
    assert_eq!("1/0".parse::<Shard>(), Err(ShardError::Count(0)));
    assert_eq!(
        format!("1/{}", MAX_SHARDS + 1).parse::<Shard>(),
        Err(ShardError::Count(MAX_SHARDS + 1))
    );
    assert_eq!(
        "1/99999999999".parse::<Shard>(),
        Err(ShardError::Count(u32::MAX))
    );
    assert!(
        format!("{MAX_SHARDS}/{MAX_SHARDS}")
            .parse::<Shard>()
            .is_ok()
    );
}

#[test]
fn a_shard_read_back_from_a_record_is_held_to_the_same_rules() {
    assert_eq!(
        serde_json::to_string(&shard(2, 3)).expect("serialises"),
        r#"{"index":2,"count":3}"#
    );
    assert!(serde_json::from_str::<Shard>(r#"{"index":4,"count":3}"#).is_err());
}

#[test]
fn every_file_lands_in_exactly_one_shard() {
    let weights = [
        0, 5, 5, 900, 12, 0, 77, 3, 3, 3, 250, 1, 0, 64, 64, 8, 2_000, 9, 10, 11, 0, 40, 41,
    ];
    let schedule = entries(&weights);

    for count in 1..=8 {
        let mut seen = vec![0usize; weights.len()];
        for index in 1..=count {
            for entry in shard_files(&schedule, shard(index, count)) {
                seen[entry.index] += 1;
            }
        }
        assert!(
            seen.iter().all(|times| *times == 1),
            "{count} shards: {seen:?}"
        );
    }
}

#[test]
fn more_shards_than_files_leaves_the_extra_shards_empty() {
    let schedule = entries(&[3, 2, 1]);

    let sizes: Vec<usize> = (1..=5)
        .map(|index| shard_files(&schedule, shard(index, 5)).len())
        .collect();

    assert_eq!(sizes, [1, 1, 1, 0, 0]);
}

#[test]
fn shards_are_balanced_by_expected_cost_rather_than_by_file_count() {
    // One file as heavy as the other five together.
    let weights = [500, 100, 100, 100, 100, 100];
    let schedule = entries(&weights);

    let indices = |part: Shard| -> Vec<usize> {
        shard_files(&schedule, part)
            .iter()
            .map(|entry| entry.index)
            .collect()
    };
    let first = indices(shard(1, 2));
    let second = indices(shard(2, 2));
    let cost = |files: &[usize]| files.iter().map(|at| weights[*at]).sum::<u64>();

    assert_eq!((first.len(), second.len()), (1, 5));
    assert_eq!((cost(&first), cost(&second)), (500, 500));
}

#[test]
fn files_recorded_at_zero_still_spread_across_the_shards() {
    let schedule = entries(&[0; 6]);

    let sizes: Vec<usize> = (1..=3)
        .map(|index| shard_files(&schedule, shard(index, 3)).len())
        .collect();

    assert_eq!(sizes, [2, 2, 2]);
}

#[test]
fn the_partition_and_its_fingerprint_depend_on_the_schedule_and_the_count_alone() {
    let schedule = entries(&[40, 30, 20, 10, 10]);
    let again = entries(&[40, 30, 20, 10, 10]);

    for index in 1..=3 {
        assert_eq!(
            shard_files(&schedule, shard(index, 3)),
            shard_files(&again, shard(index, 3))
        );
    }
    let fingerprint = partition_fingerprint(&schedule, 3);
    assert_eq!(fingerprint, partition_fingerprint(&again, 3));
    assert_eq!(fingerprint.len(), 16);
    assert_ne!(fingerprint, partition_fingerprint(&schedule, 4));
    assert_ne!(
        fingerprint,
        partition_fingerprint(&entries(&[40, 30, 20, 10, 11]), 3)
    );
    let mut renamed = entries(&[40, 30, 20, 10, 10]);
    renamed[0].file = "src/other.test.js".into();
    assert_ne!(fingerprint, partition_fingerprint(&renamed, 3));
}

/// A completed file with one record per status.
fn file(path: &str, statuses: &[TestStatus]) -> FileReport {
    FileReport {
        file: path.to_owned(),
        status: FileStatus::Completed,
        duration_micros: 1_000,
        records: statuses
            .iter()
            .enumerate()
            .map(|(at, status)| TestRecord {
                file: path.to_owned(),
                name: format!("case {at}"),
                line: at + 1,
                column: 1,
                status: status.clone(),
                attempts: 1,
                duration_micros: 100,
                output: Vec::new(),
                bench: None,
            })
            .collect(),
        output: Vec::new(),
    }
}

/// Five files with a pass, a failure, a todo and a file that did not load.
fn suite() -> Vec<FileReport> {
    vec![
        file("src/a.test.js", &[TestStatus::Passed, TestStatus::Passed]),
        file(
            "src/b.test.js",
            &[TestStatus::Failed {
                failures: Vec::new(),
            }],
        ),
        file("src/c.test.js", &[TestStatus::Todo]),
        FileReport {
            status: FileStatus::LoadFailed {
                message: String::from("boom"),
                stack: None,
            },
            ..file("src/d.test.js", &[])
        },
        file("src/e.test.js", &[TestStatus::Passed]),
    ]
}

/// The report a run over `files` makes, with every duration zeroed.
fn report(mut files: Vec<FileReport>) -> TestRunReport {
    files.sort_by(|a, b| a.file.cmp(&b.file));
    let plan = merge_plans(
        files
            .iter()
            .map(|file| discover_tests(&file.file, "it('one', () => {});\nit.todo('two');\n")),
    );
    let mut summary = TestSummary {
        files: files.len(),
        unsupported_declarations: plan.unsupported.len(),
        foreign_declarations: plan.foreign_count(),
        ..TestSummary::default()
    };
    summary.count_files(&files);
    TestRunReport {
        plan,
        files,
        summary,
    }
}

/// One record per shard of `split`, each listing positions in `files`.
fn records(split: &[&[usize]], files: &[FileReport], fingerprint: &str) -> Vec<ShardRecord> {
    let count = u32::try_from(split.len()).expect("a handful of shards");
    split
        .iter()
        .zip(1..)
        .map(|(members, index)| {
            let mut report = report(members.iter().map(|at| files[*at].clone()).collect());
            report.summary.duration_micros = 1_000 * u64::from(index);
            ShardRecord {
                version: SHARD_RECORD_VERSION,
                shard: shard(index, count),
                suite_files: files.len(),
                fingerprint: fingerprint.to_owned(),
                report,
                coverage: None,
            }
        })
        .collect()
}

#[test]
fn merged_shards_report_what_one_run_over_the_whole_suite_reports() {
    let files = suite();

    let merged =
        merge_shards(records(&[&[0, 3], &[1], &[2, 4]], &files, "f")).expect("the shards merge");

    let mut whole = report(files);
    // The shards' durations, summed.
    whole.summary.duration_micros = 6_000;
    assert_eq!(merged.report, whole);
    let summary = &merged.report.summary;
    assert_eq!(
        (
            summary.files,
            summary.passed,
            summary.failed,
            summary.todo,
            summary.failed_files
        ),
        (5, 3, 1, 1, 1)
    );
    assert_eq!((merged.count, merged.fingerprint.as_str()), (3, "f"));
    assert_eq!(merged.coverage, None);
}

#[test]
fn records_merge_the_same_in_any_order() {
    let files = suite();
    let mut shards = records(&[&[0, 3], &[1], &[2, 4]], &files, "f");
    let forward = merge_shards(shards.clone()).expect("the shards merge");

    shards.reverse();

    assert_eq!(merge_shards(shards), Ok(forward));
}

#[test]
fn a_merge_that_would_misreport_the_suite_is_refused() {
    let files = suite();
    let shards = || records(&[&[0, 3], &[1], &[2, 4]], &files, "f");

    assert_eq!(merge_shards(Vec::new()), Err(ShardMergeError::Empty));

    let mut missing = shards();
    missing.remove(1);
    assert_eq!(
        merge_shards(missing),
        Err(ShardMergeError::Missing {
            missing: String::from("2"),
            count: 3
        })
    );

    let mut twice = shards();
    twice[1] = twice[0].clone();
    assert_eq!(
        merge_shards(twice),
        Err(ShardMergeError::Duplicate { shard: shard(1, 3) })
    );

    let mut counts = shards();
    counts[2].shard = shard(3, 4);
    assert_eq!(
        merge_shards(counts),
        Err(ShardMergeError::Counts {
            shards: String::from("1/3, 2/3, 3/4")
        })
    );

    let mut cut = shards();
    cut[1].fingerprint = String::from("g");
    assert_eq!(
        merge_shards(cut),
        Err(ShardMergeError::Fingerprints {
            fingerprints: String::from("f, g")
        })
    );

    let mut version = shards();
    version[2].version = SHARD_RECORD_VERSION + 1;
    assert_eq!(
        merge_shards(version),
        Err(ShardMergeError::Version {
            shard: shard(3, 3),
            found: SHARD_RECORD_VERSION + 1
        })
    );

    assert_eq!(
        merge_shards(records(&[&[0, 3], &[1, 3], &[2, 4]], &files, "f")),
        Err(ShardMergeError::Overlap {
            file: String::from("src/d.test.js"),
            first: shard(1, 3),
            second: shard(2, 3),
        })
    );

    assert_eq!(
        merge_shards(records(&[&[0, 3], &[1], &[2]], &files, "f")),
        Err(ShardMergeError::FileCount { ran: 4, suite: 5 })
    );

    let mut mixed = shards();
    mixed[1].coverage = Some(Coverage::new());
    assert_eq!(
        merge_shards(mixed),
        Err(ShardMergeError::MixedCoverage {
            with: shard(2, 3),
            without: shard(1, 3)
        })
    );
}

fn coverage(json: &str) -> Coverage {
    serde_json::from_str(json).expect("a coverage record")
}

#[test]
fn a_shard_record_carries_its_coverage_through_json_unchanged() {
    let record = ShardRecord {
        version: SHARD_RECORD_VERSION,
        shard: shard(1, 1),
        suite_files: 0,
        fingerprint: String::from("f"),
        report: TestRunReport::default(),
        coverage: Some(coverage(
            r#"{"files":[{"path":"src/a.js","lines":[[1,2],[3,0]],"functions":[{"at":{"line":1,"column":0},"name":"total","hits":2}],"branches":[{"at":{"line":3,"column":2},"hits":0}]}],"neverLoaded":["src/b.js"],"unmapped":["src/plain.js"]}"#,
        )),
    };

    let text = serde_json::to_string(&record).expect("the record serialises");
    let back: ShardRecord = serde_json::from_str(&text).expect("and reads back");

    assert_eq!(back, record);
    assert_eq!(serde_json::to_string(&back).expect("serialises"), text);
}

#[test]
fn merged_coverage_sums_hits_and_names_only_the_files_no_shard_loaded() {
    let files = suite();
    let mut shards = records(&[&[0, 1, 2], &[3, 4]], &files, "f");
    shards[0].coverage = Some(coverage(
        r#"{"files":[{"path":"src/a.js","lines":[[1,1],[2,0]]}],"neverLoaded":["src/b.js","src/c.js"]}"#,
    ));
    shards[1].coverage = Some(coverage(
        r#"{"files":[{"path":"src/a.js","lines":[[1,1],[2,1]]},{"path":"src/b.js","lines":[[1,1]]}],"neverLoaded":["src/c.js"]}"#,
    ));

    let merged = merge_shards(shards)
        .expect("the shards merge")
        .coverage
        .expect("both shards measured");

    let lines: Vec<(u32, u64)> = merged.file("src/a.js").expect("src/a.js").lines().collect();
    assert_eq!(lines, [(1, 2), (2, 1)]);
    assert!(merged.file("src/b.js").is_some());
    assert_eq!(merged.never_loaded(), [String::from("src/c.js")]);
}

#[test]
fn a_coverage_record_that_names_a_path_outside_the_project_is_refused() {
    let error = serde_json::from_str::<Coverage>(
        r#"{"files":[{"path":"../../etc/passwd","lines":[[1,1]]}]}"#,
    )
    .expect_err("a path outside the project");

    assert!(
        error.to_string().contains("not a path inside the project"),
        "{error}"
    );
}
