//! Longest-first ordering, and the cold heuristic behind it.

use crate::{
    COLD_NANOS_PER_BYTE, Concurrency, RunOptions, ScheduleBasis, TestRunner, TestTimings,
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
fn a_run_reports_how_many_files_were_scheduled_warm() {
    let mut timings = TestTimings::new();
    timings.record("a.test.js", 1_000);

    let runner = TestRunner::new()
        .with_timings(timings)
        .with_options(RunOptions::serial());
    let report = runner.run(&[
        ("a.test.js", "it('a', () => {});"),
        ("b.test.js", "it('b', () => {});"),
    ]);

    assert_eq!(report.summary.scheduled_warm, 1);
    assert_eq!(report.summary.scheduled_cold, 1);
}

#[test]
fn a_runner_exposes_the_schedule_it_would_use() {
    let owned = files(&[("small.js", 10), ("large.js", 10_000)]);
    let pairs = as_pairs(&owned);

    let schedule = TestRunner::new().schedule(&pairs);
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

#[test]
fn a_fixed_worker_count_still_runs_every_file() {
    let runner = TestRunner::new().with_options(RunOptions {
        concurrency: Concurrency::Fixed(std::num::NonZeroUsize::new(3).unwrap()),
        ..RunOptions::default()
    });
    let sources: Vec<(String, String)> = (0..20)
        .map(|index| {
            (
                format!("t{index}.test.js"),
                format!("it('case {index}', () => {{ expect(1).toBe(1); }});"),
            )
        })
        .collect();
    let pairs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();

    let report = runner.run(&pairs);
    assert_eq!(report.summary.passed, 20);
    assert_eq!(report.files.len(), 20);
}
