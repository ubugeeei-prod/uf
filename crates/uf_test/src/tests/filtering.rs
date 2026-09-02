//! `-t`, path filters, and what they do to the report.

use crate::{MAX_PATTERN_BYTES, RunOptions, SkipReason, TestFilter, TestRunner, TestStatus};

const SUITE: [(&str, &str); 2] = [
    (
        "src/math.test.js",
        "describe('math', () => { it('adds', () => {}); it('subtracts', () => {}); });",
    ),
    (
        "src/ui/button.test.js",
        "describe('button', () => { it('renders', () => {}); });",
    ),
];

fn run_with(filter: TestFilter) -> crate::TestRunReport {
    TestRunner::new()
        .with_filter(filter)
        .with_options(RunOptions::serial())
        .run(&SUITE)
}

#[test]
fn an_empty_filter_excludes_nothing() {
    let filter = TestFilter::new();
    assert!(filter.is_empty());
    assert!(filter.matches_path("anything"));
    assert!(filter.matches_name("anything"));
}

#[test]
fn a_name_filter_keeps_only_matching_tests() {
    let report = run_with(TestFilter::new().with_name("adds"));

    assert_eq!(report.summary.passed, 1);
    assert_eq!(report.summary.skipped, 2);
    let kept: Vec<&str> = report
        .records()
        .filter(|record| record.status.is_passed())
        .map(|record| record.name.as_str())
        .collect();
    similar_asserts::assert_eq!(kept, vec!["math > adds"]);
}

#[test]
fn a_name_filter_matches_the_fully_qualified_name() {
    let report = run_with(TestFilter::new().with_name("math > adds"));
    assert_eq!(report.summary.passed, 1);
}

#[test]
fn a_name_filter_can_match_the_describe_alone() {
    let report = run_with(TestFilter::new().with_name("button"));
    assert_eq!(report.summary.passed, 1);
    assert_eq!(report.summary.skipped, 2);
}

#[test]
fn a_filtered_out_test_records_the_filter_as_its_reason() {
    let report = run_with(TestFilter::new().with_name("adds"));
    let skipped = report
        .records()
        .find(|record| record.name == "math > subtracts")
        .unwrap();

    assert!(matches!(
        skipped.status,
        TestStatus::Skipped {
            reason: SkipReason::Filtered
        }
    ));
}

#[test]
fn a_name_filter_that_matches_nothing_runs_nothing() {
    let report = run_with(TestFilter::new().with_name("no such test"));
    assert_eq!(report.summary.passed, 0);
    assert_eq!(report.summary.skipped, 3);
}

#[test]
fn an_empty_name_pattern_is_ignored_rather_than_matching_nothing() {
    let filter = TestFilter::new().with_name("");
    assert!(filter.is_empty());
    assert_eq!(run_with(filter).summary.passed, 3);
}

#[test]
fn a_whitespace_only_pattern_is_ignored() {
    assert!(TestFilter::new().with_name("   ").is_empty());
    assert!(TestFilter::new().with_path("\t\n").is_empty());
}

#[test]
fn a_path_filter_keeps_only_matching_files() {
    let report = run_with(TestFilter::new().with_path("src/ui/"));

    assert_eq!(report.summary.files, 1);
    assert_eq!(report.files[0].file, "src/ui/button.test.js");
}

#[test]
fn several_path_filters_widen_the_selection() {
    let filter = TestFilter::new().with_path("math").with_path("button");
    assert_eq!(run_with(filter).summary.files, 2);
}

#[test]
fn path_filters_can_be_added_in_bulk() {
    let filter = TestFilter::new().with_paths(["math", "button"]);
    assert_eq!(filter.path_patterns().len(), 2);
}

#[test]
fn a_path_filter_that_matches_nothing_schedules_nothing() {
    let report = run_with(TestFilter::new().with_path("no/such/dir"));
    assert_eq!(report.summary.files, 0);
    assert!(report.is_success());
}

#[test]
fn a_path_filter_and_a_name_filter_compose() {
    let filter = TestFilter::new().with_path("math").with_name("subtracts");
    let report = run_with(filter);

    assert_eq!(report.summary.files, 1);
    assert_eq!(report.summary.passed, 1);
    assert_eq!(report.summary.skipped, 1);
}

#[test]
fn filtering_is_case_sensitive() {
    assert_eq!(
        run_with(TestFilter::new().with_name("ADDS")).summary.passed,
        0
    );
}

#[test]
fn a_pattern_is_reported_back_as_stored() {
    let filter = TestFilter::new().with_name("  adds  ");
    assert_eq!(filter.name_pattern(), Some("adds"));
}

#[test]
fn an_over_long_pattern_is_truncated_rather_than_rejected() {
    let pattern = "x".repeat(MAX_PATTERN_BYTES * 4);
    let filter = TestFilter::new().with_name(&pattern);

    assert_eq!(filter.name_pattern().unwrap().len(), MAX_PATTERN_BYTES);
}

#[test]
fn truncating_a_pattern_respects_character_boundaries() {
    let pattern = "日".repeat(MAX_PATTERN_BYTES);
    let filter = TestFilter::new().with_name(&pattern);
    // Would panic on a byte slice through the middle of a character.
    assert!(filter.name_pattern().unwrap().len() <= MAX_PATTERN_BYTES);
}

#[test]
fn a_filter_does_not_override_an_explicit_skip() {
    let runner = TestRunner::new()
        .with_filter(TestFilter::new().with_name("off"))
        .with_options(RunOptions::serial());
    let report = runner.run(&[("a.test.js", "it.skip('off', () => {});")]);

    assert!(matches!(
        report.files[0].records[0].status,
        TestStatus::Skipped {
            reason: SkipReason::Explicit
        }
    ));
}

#[test]
fn a_filter_does_not_override_a_todo() {
    let runner = TestRunner::new()
        .with_filter(TestFilter::new().with_name("nothing matches"))
        .with_options(RunOptions::serial());
    let report = runner.run(&[("a.test.js", "it.todo('later');")]);

    assert!(matches!(
        report.files[0].records[0].status,
        TestStatus::Todo
    ));
}

#[test]
fn only_still_wins_inside_a_filtered_file() {
    let runner = TestRunner::new()
        .with_filter(TestFilter::new().with_name("case"))
        .with_options(RunOptions::serial());
    let report = runner.run(&[(
        "a.test.js",
        "it('case one', () => {});\nit.only('case two', () => {});",
    )]);

    assert_eq!(report.summary.passed, 1);
    let passed = report.records().find(|r| r.status.is_passed()).unwrap();
    assert_eq!(passed.name, "case two");
}

#[test]
fn a_runner_exposes_the_filter_it_was_given() {
    let runner = TestRunner::new().with_filter(TestFilter::new().with_name("x"));
    assert_eq!(runner.filter().name_pattern(), Some("x"));
}

#[test]
fn a_runner_exposes_the_options_it_was_given() {
    let runner = TestRunner::new().with_options(RunOptions::serial());
    assert!(runner.options().concurrency.is_serial());
}
