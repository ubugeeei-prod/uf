//! The same suite must produce the same result whatever the schedule does.

use std::num::NonZeroUsize;

use crate::{Concurrency, RunOptions, TestRunner, TestTimings};

/// A suite with wildly different file costs, so the schedule really does
/// reorder it.
fn suite() -> Vec<(String, String)> {
    (0..40)
        .map(|index| {
            let mut source = String::new();
            for case in 0..(1 + index % 7) {
                source.push_str("describe('suite ");
                source.push_str(&index.to_string());
                source.push_str("', () => { it('case ");
                source.push_str(&case.to_string());
                source.push_str("', () => { expect(");
                source.push_str(&case.to_string());
                source.push_str(").toBe(");
                let expected = if case % 3 == 0 { 0 } else { case };
                source.push_str(&expected.to_string());
                source.push_str("); }); });\n");
            }
            source.push_str("it.skip('off', () => {});\nit.todo('later');\n");
            (format!("src/f{index:02}.test.js"), source)
        })
        .collect()
}

fn pairs(owned: &[(String, String)]) -> Vec<(&str, &str)> {
    owned
        .iter()
        .map(|(file, source)| (file.as_str(), source.as_str()))
        .collect()
}

fn run(concurrency: Concurrency, timings: TestTimings) -> crate::TestRunReport {
    let owned = suite();
    let sources = pairs(&owned);
    TestRunner::new()
        .with_timings(timings)
        .with_options(RunOptions {
            concurrency,
            ..RunOptions::default()
        })
        .run(&sources)
        .without_timings()
}

#[test]
fn a_serial_run_and_a_parallel_run_agree_exactly() {
    let serial = run(Concurrency::Serial, TestTimings::new());
    let parallel = run(
        Concurrency::Fixed(NonZeroUsize::new(8).unwrap()),
        TestTimings::new(),
    );

    similar_asserts::assert_eq!(serial.summary, parallel.summary);
    similar_asserts::assert_eq!(serial.files, parallel.files);
    similar_asserts::assert_eq!(serial.plan, parallel.plan);
}

#[test]
fn a_run_at_two_widths_agrees_exactly() {
    let narrow = run(
        Concurrency::Fixed(NonZeroUsize::new(2).unwrap()),
        TestTimings::new(),
    );
    let wide = run(
        Concurrency::Fixed(NonZeroUsize::new(16).unwrap()),
        TestTimings::new(),
    );

    similar_asserts::assert_eq!(narrow.files, wide.files);
}

#[test]
fn recorded_timings_change_the_order_but_not_the_result() {
    let cold = run(Concurrency::Serial, TestTimings::new());

    let mut timings = TestTimings::new();
    for (index, (file, _)) in suite().iter().enumerate() {
        // Deliberately inverted: the cheapest file claims to be the slowest.
        timings.record(file, (100 - index as u64) * 1_000);
    }
    let warm = run(Concurrency::Serial, timings);

    similar_asserts::assert_eq!(cold.files, warm.files);
    assert_eq!(cold.summary.passed, warm.summary.passed);
    assert_eq!(cold.summary.failed, warm.summary.failed);
}

#[test]
fn json_output_is_byte_identical_across_runs_modulo_timings() {
    let first = serde_json::to_string(&run(Concurrency::Serial, TestTimings::new())).unwrap();
    let second = serde_json::to_string(&run(
        Concurrency::Fixed(NonZeroUsize::new(8).unwrap()),
        TestTimings::new(),
    ))
    .unwrap();

    similar_asserts::assert_eq!(first, second);
}

#[test]
fn repeating_one_run_produces_the_same_json() {
    let owned = suite();
    let sources = pairs(&owned);
    let runner = TestRunner::new();

    let first = serde_json::to_string(&runner.run(&sources).without_timings()).unwrap();
    let second = serde_json::to_string(&runner.run(&sources).without_timings()).unwrap();

    similar_asserts::assert_eq!(first, second);
}

#[test]
fn files_are_reported_in_path_order_whatever_the_schedule() {
    let report = run(
        Concurrency::Fixed(NonZeroUsize::new(8).unwrap()),
        TestTimings::new(),
    );
    let paths: Vec<&str> = report.files.iter().map(|file| file.file.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();

    similar_asserts::assert_eq!(paths, sorted);
}

#[test]
fn records_inside_a_file_are_reported_in_source_order() {
    let report = TestRunner::new().run(&[(
        "a.test.js",
        "it('third', () => {});\nit('first', () => {});\nit('second', () => {});\n",
    )]);
    let names: Vec<&str> = report.files[0]
        .records
        .iter()
        .map(|record| record.name.as_str())
        .collect();

    similar_asserts::assert_eq!(names, vec!["third", "first", "second"]);
}

#[test]
fn scrubbing_timings_zeroes_every_measured_duration() {
    let owned = suite();
    let sources = pairs(&owned);
    let report = TestRunner::new().run(&sources).without_timings();

    assert_eq!(report.summary.duration_micros, 0);
    assert!(report.files.iter().all(|file| file.duration_micros == 0));
}

#[test]
fn the_discovered_plan_does_not_depend_on_the_schedule() {
    let serial = run(Concurrency::Serial, TestTimings::new());
    let parallel = run(Concurrency::Auto, TestTimings::new());

    similar_asserts::assert_eq!(serial.plan.cases.len(), parallel.plan.cases.len());
    similar_asserts::assert_eq!(serial.plan, parallel.plan);
}

#[test]
fn a_wide_run_over_many_small_files_stays_correct() {
    let owned: Vec<(String, String)> = (0..200)
        .map(|index| {
            (
                format!("src/t{index:03}.test.js"),
                format!("it('case {index}', () => {{ expect({index}).toBe({index}); }});"),
            )
        })
        .collect();
    let sources = pairs(&owned);

    let report = TestRunner::new().run(&sources);
    assert_eq!(report.summary.passed, 200);
    assert_eq!(report.summary.failed, 0);
    assert!(report.is_success());
}
