//! Longest-first ordering, and the cold heuristic behind it.

use crate::{
    COLD_NANOS_PER_BYTE, ScheduleBasis, TestFilter, TestRunner, TestTimings, auto_workers,
    cold_weight_micros, makespan_micros, schedule_files,
};

fn files(sizes: &[(&'static str, usize)]) -> Vec<(&'static str, String)> {
    sizes
        .iter()
        .map(|(name, size)| (*name, "x".repeat(*size)))
        .collect()
}

fn as_pairs<'a>(files: &'a [(&'a str, String)]) -> Vec<(&'a str, &'a str)> {
    files
        .iter()
        .map(|(name, source)| (*name, source.as_str()))
        .collect()
}

#[test]
fn a_cold_run_orders_by_file_size_descending() {
    let owned = files(&[("small.js", 10), ("large.js", 10_000), ("mid.js", 1_000)]);
    let pairs = as_pairs(&owned);

    let schedule = schedule_files(&pairs, &TestTimings::new());
    let order: Vec<&str> = schedule.iter().map(|entry| entry.file.as_str()).collect();

    similar_asserts::assert_eq!(order, vec!["large.js", "mid.js", "small.js"]);
    assert!(
        schedule
            .iter()
            .all(|entry| entry.basis == ScheduleBasis::Size)
    );
}

#[test]
fn a_recorded_duration_beats_the_size_heuristic() {
    let owned = files(&[("small.js", 10), ("large.js", 10_000)]);
    let pairs = as_pairs(&owned);
    let mut timings = TestTimings::new();
    timings.record("small.js", 900_000);

    let schedule = schedule_files(&pairs, &timings);
    let order: Vec<&str> = schedule.iter().map(|entry| entry.file.as_str()).collect();

    similar_asserts::assert_eq!(order, vec!["small.js", "large.js"]);
    assert_eq!(schedule[0].basis, ScheduleBasis::Recorded);
    assert_eq!(schedule[1].basis, ScheduleBasis::Size);
}

#[test]
fn a_partially_warm_cache_still_produces_one_ordering() {
    let owned = files(&[("a.js", 100), ("b.js", 100), ("c.js", 100)]);
    let pairs = as_pairs(&owned);
    let mut timings = TestTimings::new();
    timings.record("b.js", 5_000_000);

    let schedule = schedule_files(&pairs, &timings);
    assert_eq!(schedule[0].file, "b.js");
    assert_eq!(schedule.len(), 3);
}

#[test]
fn ties_break_on_the_path_so_the_schedule_is_a_pure_function() {
    let owned = files(&[("c.js", 100), ("a.js", 100), ("b.js", 100)]);
    let pairs = as_pairs(&owned);

    let first = schedule_files(&pairs, &TestTimings::new());
    let second = schedule_files(&pairs, &TestTimings::new());

    assert_eq!(first, second);
    let order: Vec<&str> = first.iter().map(|entry| entry.file.as_str()).collect();
    similar_asserts::assert_eq!(order, vec!["a.js", "b.js", "c.js"]);
}

#[test]
fn every_file_keeps_its_index_into_the_caller_slice() {
    let owned = files(&[("small.js", 10), ("large.js", 10_000)]);
    let pairs = as_pairs(&owned);

    let schedule = schedule_files(&pairs, &TestTimings::new());
    assert_eq!(pairs[schedule[0].index].0, "large.js");
    assert_eq!(pairs[schedule[1].index].0, "small.js");
}

#[test]
fn the_cold_weight_is_proportional_to_size() {
    assert!(cold_weight_micros(1_000_000) > cold_weight_micros(1_000));
    assert_eq!(
        cold_weight_micros(1_000_000),
        1_000_000 * COLD_NANOS_PER_BYTE / 1_000
    );
}

#[test]
fn an_empty_file_still_has_a_weight() {
    assert_eq!(cold_weight_micros(0), 1);
}

#[test]
fn the_cold_weight_saturates_rather_than_overflowing() {
    assert!(cold_weight_micros(usize::MAX) > 0);
}

#[test]
fn scheduling_nothing_produces_nothing() {
    assert!(schedule_files(&[], &TestTimings::new()).is_empty());
}

#[test]
fn running_nothing_needs_no_javascript_host() {
    let report = TestRunner::new()
        .run(&[])
        .expect("an empty schedule does not need a worker");

    assert!(report.files.is_empty());
    assert!(report.plan.is_empty());
    assert_eq!(report.summary.files, 0);
    assert_eq!(report.summary.scheduled_warm, 0);
    assert_eq!(report.summary.scheduled_cold, 0);
}

#[test]
fn filtering_every_file_out_needs_no_javascript_host() {
    let files = [crate::TestFile::new(
        "src/example.test.js",
        "/p/src/example.test.js",
        "import { it } from '@uniflowed/test'; it('runs', () => {});",
    )];

    let report = TestRunner::new()
        .with_filter(TestFilter::new().with_path("other"))
        .run(&files)
        .expect("a path filter can make the schedule empty before host startup");

    assert!(report.files.is_empty());
    assert!(report.plan.is_empty());
    assert_eq!(report.summary.files, 0);
}

#[test]
fn longest_first_shortens_the_critical_path() {
    // Four workers, one very long file and a tail of short ones. Running the
    // long file last leaves three workers idle while it finishes.
    let mut owned: Vec<(String, String)> = vec![("long.js".to_string(), "x".repeat(4_000_000))];
    for index in 0..12 {
        owned.push((format!("short{index}.js"), "x".repeat(100_000)));
    }
    let pairs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();

    let scheduled = schedule_files(&pairs, &TestTimings::new());
    let mut source_order = scheduled.clone();
    source_order.sort_by_key(|entry| entry.index);
    // Put the long file last, which is what a source-ordered run risks.
    source_order.rotate_left(1);

    let lpt = makespan_micros(&scheduled, 4);
    let naive = makespan_micros(&source_order, 4);
    assert!(
        lpt < naive,
        "longest-first must not be worse: lpt={lpt} naive={naive}"
    );
}

#[test]
fn the_makespan_of_one_worker_is_the_total_work() {
    let owned = files(&[("a.js", 1_000), ("b.js", 2_000)]);
    let pairs = as_pairs(&owned);
    let schedule = schedule_files(&pairs, &TestTimings::new());

    let total: u64 = schedule.iter().map(|entry| entry.weight_micros).sum();
    assert_eq!(makespan_micros(&schedule, 1), total);
}

#[test]
fn the_makespan_of_zero_workers_is_treated_as_one() {
    let owned = files(&[("a.js", 1_000)]);
    let pairs = as_pairs(&owned);
    let schedule = schedule_files(&pairs, &TestTimings::new());

    assert_eq!(makespan_micros(&schedule, 0), makespan_micros(&schedule, 1));
}

#[test]
fn the_makespan_of_an_empty_schedule_is_zero() {
    assert_eq!(makespan_micros(&[], 4), 0);
}

#[test]
fn a_schedule_says_which_files_were_ordered_from_a_recording() {
    let mut timings = TestTimings::new();
    timings.record("a.test.js", 1_000);
    let owned = files(&[("a.test.js", 10), ("b.test.js", 10)]);
    let pairs = as_pairs(&owned);

    let schedule = schedule_files(&pairs, &timings);

    let warm = schedule
        .iter()
        .filter(|entry| entry.basis == ScheduleBasis::Recorded)
        .count();
    assert_eq!(warm, 1);
    assert_eq!(schedule.len() - warm, 1);
}

#[test]
fn a_runner_exposes_the_schedule_it_would_use() {
    let files = [
        crate::TestFile::new("small.js", "/p/small.js", "x".repeat(10)),
        crate::TestFile::new("large.js", "/p/large.js", "x".repeat(10_000)),
    ];

    let schedule = TestRunner::new().schedule(&files);
    assert_eq!(schedule[0].file, "large.js");
}

#[test]
fn a_schedule_entry_round_trips_through_json() {
    let owned = files(&[("a.js", 10)]);
    let pairs = as_pairs(&owned);
    let schedule = schedule_files(&pairs, &TestTimings::new());

    let json = serde_json::to_string(&schedule[0]).unwrap();
    assert!(json.contains("\"basis\":\"size\""));
    let back: crate::ScheduleEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(back, schedule[0]);
}

/// A warm schedule of `durations`, one file per entry, as the next run sees it.
fn recorded(durations: &[u64]) -> Vec<crate::ScheduleEntry> {
    let owned: Vec<(String, String)> = (0..durations.len())
        .map(|index| (format!("f{index:04}.test.js"), String::from("x")))
        .collect();
    let pairs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();
    let mut timings = TestTimings::new();
    for (index, micros) in durations.iter().enumerate() {
        timings.record(&format!("f{index:04}.test.js"), *micros);
    }
    schedule_files(&pairs, &timings)
}

#[test]
fn a_suite_of_short_files_does_not_start_a_worker_per_core() {
    // The suite `docs/app/guide/testing` measures: fifty files of about two
    // milliseconds, and a worker that costs forty to start. Eight workers
    // would each boot, take six files, and finish long after two would have.
    let schedule = recorded(&[2_000; 50]);

    let workers = auto_workers(&schedule, Some(40_000), 8);

    assert!(
        (2..=3).contains(&workers),
        "fifty two-millisecond files are two or three workers' work, not {workers}"
    );
}

#[test]
fn a_long_suite_keeps_every_core_it_can_use() {
    // A thousand twenty-millisecond files: twenty seconds of work, where a
    // worker that costs fifty milliseconds to start is handed seconds of it.
    let schedule = recorded(&[20_000; 1_000]);

    assert_eq!(auto_workers(&schedule, Some(50_000), 8), 8);

    // On sixty-four cores the last few workers would each shave a few
    // milliseconds off a run of three hundred, so the pool stops within one
    // start-up of the fastest pool rather than at the core count — and never
    // at the twenty or so a rule weighing each worker's start-up against the
    // time it saves would stop at, when those start-ups all happen at once.
    let wide = auto_workers(&schedule, Some(50_000), 64);
    assert!((50..=64).contains(&wide), "{wide}");
    assert!(
        makespan_micros(&schedule, wide) <= makespan_micros(&schedule, 64) + 50_000,
        "{wide} workers finish within a start-up of sixty-four"
    );
}

#[test]
fn a_file_that_dominates_the_suite_is_not_waited_on_by_idle_workers() {
    // One five-second file and forty-nine short ones. The run ends when the
    // long file does, so one worker for it and one for the rest is the whole
    // of what more workers can buy.
    let mut durations = vec![5_000_000];
    durations.extend([2_000; 49]);
    let schedule = recorded(&durations);

    assert_eq!(auto_workers(&schedule, Some(40_000), 8), 2);
}

#[test]
fn a_cold_schedule_starts_a_worker_per_core_as_it_always_did() {
    // Size-based weights rank files; they are not times, and dividing them by
    // a start-up measured in real microseconds would put every cold suite on
    // one worker.
    let owned = files(&[("a.js", 100), ("b.js", 100), ("c.js", 100)]);
    let pairs = as_pairs(&owned);
    let schedule = schedule_files(&pairs, &TestTimings::new());

    assert_eq!(auto_workers(&schedule, Some(40_000), 8), 3);
}

#[test]
fn without_a_recorded_start_up_every_core_is_started() {
    let schedule = recorded(&[2_000; 50]);

    assert_eq!(auto_workers(&schedule, None, 8), 8);
}

#[test]
fn a_pool_is_never_empty_and_never_wider_than_the_suite() {
    assert_eq!(auto_workers(&[], Some(40_000), 8), 1);
    assert_eq!(auto_workers(&recorded(&[5_000_000; 3]), Some(1_000), 8), 3);
    assert_eq!(auto_workers(&recorded(&[2_000; 50]), Some(40_000), 0), 1);
}

#[test]
fn a_start_up_of_nothing_is_not_believed() {
    // A recorded zero would say a worker is free, and every suite would be
    // handed every core again. It is floored instead, so a suite of very short
    // files still stops short of the core count.
    let schedule = recorded(&[100; 50]);

    assert!(
        auto_workers(&schedule, Some(0), 8) < 8,
        "{}",
        auto_workers(&schedule, Some(0), 8)
    );
    assert_eq!(
        auto_workers(&schedule, Some(0), 8),
        auto_workers(&schedule, Some(crate::MIN_WORKER_START_MICROS), 8)
    );
}

#[test]
fn a_new_file_is_costed_like_the_files_that_were_recorded() {
    // One file added since the last run. It has no duration, and costing it at
    // its size would make the suite look cold; costing it at the mean keeps the
    // answer the warm suite gets.
    let mut schedule = recorded(&[2_000; 49]);
    let owned = files(&[("new.test.js", 10)]);
    let pairs = as_pairs(&owned);
    let mut added = schedule_files(&pairs, &TestTimings::new());
    added[0].index = schedule.len();
    schedule.append(&mut added);

    let workers = auto_workers(&schedule, Some(40_000), 8);

    assert!((2..=3).contains(&workers), "{workers}");
}
