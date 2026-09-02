//! Bounds, retries, bail, and the counters a run reports.
//!
//! Everything here is about what a run is *allowed* to do rather than what an
//! assertion decides: how many times a case may execute, how long a file may
//! take, when the run stops early, and how the outcome is counted.

use std::num::NonZeroUsize;
use std::time::Duration;

use crate::{
    Attempt, Bail, Concurrency, FileStatus, RetryPolicy, RunOptions, Schedule, SkipReason,
    TestRunner, TestStatus, UnsupportedReason, run_tests,
};

fn run(source: &str) -> crate::TestRunReport {
    run_tests([("math.test.js", source)])
}

#[test]
fn a_skipped_test_records_no_attempts() {
    let report = run("it.skip('off', () => {});");
    assert_eq!(report.files[0].records[0].attempts, 0);
}

#[test]
fn a_passing_test_records_exactly_one_attempt() {
    let report = run("it('a', () => { expect(1).toBe(1); });");
    assert_eq!(report.files[0].records[0].attempts, 1);
}

#[test]
fn a_retry_policy_drives_the_attempt_count() {
    let runner = TestRunner::new().with_options(RunOptions {
        retry: RetryPolicy::retries(3),
        ..RunOptions::serial()
    });
    let report = runner.run(&[("a.test.js", "it('a', () => { expect(1).toBe(2); });")]);

    assert_eq!(report.files[0].records[0].attempts, 4);
    assert_eq!(report.summary.failed, 1);
}

#[test]
fn a_retry_policy_does_not_re_run_a_passing_test() {
    let runner = TestRunner::new().with_options(RunOptions {
        retry: RetryPolicy::retries(5),
        ..RunOptions::serial()
    });
    let report = runner.run(&[("a.test.js", "it('a', () => { expect(1).toBe(1); });")]);

    assert_eq!(report.files[0].records[0].attempts, 1);
}

#[test]
fn a_retry_schedule_is_bounded_by_the_attempt_cap() {
    let policy = RetryPolicy::from_schedule(Schedule::Forever);
    let mut retries = Attempt::first();
    // One execution has already happened by the time the first decision is made.
    let mut executions = 1u32;
    while policy.next_delay(retries).is_some() {
        retries = retries.advance(Duration::ZERO);
        executions += 1;
        assert!(
            executions <= crate::MAX_ATTEMPTS,
            "a `Forever` schedule must still be capped"
        );
    }
    assert_eq!(executions, crate::MAX_ATTEMPTS);
}

#[test]
fn a_retry_delay_is_capped() {
    let policy = RetryPolicy::from_schedule(Schedule::spaced(Duration::from_secs(3_600)));
    assert_eq!(
        policy.next_delay(Attempt::first()),
        Some(crate::MAX_RETRY_DELAY)
    );
}

#[test]
fn no_retry_policy_means_one_attempt() {
    assert_eq!(RetryPolicy::none().max_attempts(), 1);
    assert_eq!(RetryPolicy::none().next_delay(Attempt::first()), None);
}

#[test]
fn a_file_past_the_size_limit_is_reported_by_name() {
    let runner = TestRunner::new().with_options(RunOptions {
        max_source_bytes: 16,
        ..RunOptions::serial()
    });
    let report = runner.run(&[("big.test.js", "it('a', () => { expect(1).toBe(1); });")]);

    assert_eq!(report.summary.failed_files, 1);
    assert_eq!(report.files[0].file, "big.test.js");
    assert!(matches!(
        report.files[0].status,
        FileStatus::TooLarge { .. }
    ));
    assert!(!report.is_success());
}

#[test]
fn a_file_that_exhausts_its_budget_is_reported_as_a_timeout() {
    let mut source = String::new();
    for index in 0..400 {
        source.push_str("it('case ");
        source.push_str(&index.to_string());
        source.push_str("', () => { expect(1).toBe(1); });\n");
    }
    let runner = TestRunner::new().with_options(RunOptions {
        file_timeout: Duration::from_nanos(1),
        ..RunOptions::serial()
    });
    let report = runner.run(&[("slow.test.js", &source)]);

    assert!(matches!(
        report.files[0].status,
        FileStatus::TimedOut { .. }
    ));
    assert_eq!(report.files[0].file, "slow.test.js");
    assert_eq!(report.summary.failed_files, 1);
    assert!(!report.is_success());
}

#[test]
fn a_timeout_in_one_file_does_not_take_the_run_down() {
    let mut slow = String::new();
    for index in 0..400 {
        slow.push_str("it('c");
        slow.push_str(&index.to_string());
        slow.push_str("', () => { expect(1).toBe(1); });\n");
    }
    let runner = TestRunner::new().with_options(RunOptions {
        file_timeout: Duration::from_nanos(1),
        ..RunOptions::serial()
    });
    let report = runner.run(&[
        ("slow.test.js", slow.as_str()),
        ("fast.test.js", "it('a', () => { expect(1).toBe(1); });"),
    ]);

    assert_eq!(report.files.len(), 2);
    let fast = report
        .files
        .iter()
        .find(|file| file.file == "fast.test.js")
        .unwrap();
    assert_eq!(fast.status, FileStatus::Completed);
}

#[test]
fn the_file_budget_is_clamped_into_the_supported_range() {
    let options = RunOptions {
        file_timeout: Duration::from_secs(60 * 60 * 24),
        ..RunOptions::default()
    };
    assert_eq!(options.effective_file_timeout(), crate::MAX_FILE_TIMEOUT);

    let options = RunOptions {
        file_timeout: Duration::ZERO,
        ..RunOptions::default()
    };
    assert_eq!(options.effective_file_timeout(), crate::MIN_FILE_TIMEOUT);
}

#[test]
fn the_source_limit_cannot_be_raised_past_the_crate_bound() {
    let options = RunOptions {
        max_source_bytes: usize::MAX,
        ..RunOptions::default()
    };
    assert_eq!(
        options.effective_max_source_bytes(),
        crate::MAX_SOURCE_BYTES
    );
}

#[test]
fn the_assertion_cap_is_never_zero() {
    let options = RunOptions {
        max_assertions_per_test: 0,
        ..RunOptions::default()
    };
    assert_eq!(options.effective_max_assertions(), 1);
}

#[test]
fn assertions_recorded_from_one_body_are_bounded() {
    let mut body = String::from("it('a', () => {\n");
    for _ in 0..50 {
        body.push_str("  expect(x).toContain(1);\n");
    }
    body.push_str("});\n");

    let runner = TestRunner::new().with_options(RunOptions {
        max_assertions_per_test: 4,
        ..RunOptions::serial()
    });
    let report = runner.run(&[("a.test.js", &body)]);

    assert_eq!(report.summary.unsupported_assertions, 4);
}

#[test]
fn bail_stops_scheduling_further_files() {
    let failing = "it('a', () => { expect(1).toBe(2); });";
    let sources: Vec<(&str, &str)> = vec![
        ("a.test.js", failing),
        ("b.test.js", failing),
        ("c.test.js", failing),
        ("d.test.js", failing),
    ];
    let runner = TestRunner::new().with_options(RunOptions {
        bail: Bail::After(NonZeroUsize::new(1).unwrap()),
        ..RunOptions::serial()
    });
    let report = runner.run(&sources);

    assert!(report.summary.bailed);
    assert!(
        report
            .files
            .iter()
            .any(|file| file.status == FileStatus::NotRun)
    );
    assert!(!report.is_success());
}

#[test]
fn bail_off_runs_everything() {
    let failing = "it('a', () => { expect(1).toBe(2); });";
    let report = run_tests([("a.test.js", failing), ("b.test.js", failing)]);

    assert!(!report.summary.bailed);
    assert_eq!(report.summary.failed, 2);
    assert!(
        report
            .files
            .iter()
            .all(|file| file.status == FileStatus::Completed)
    );
}

#[test]
fn a_zero_bail_threshold_is_no_bail() {
    assert_eq!(Bail::after(0), Bail::Off);
    assert!(!Bail::Off.is_reached(usize::MAX));
}

#[test]
fn a_bail_threshold_triggers_at_the_count() {
    let bail = Bail::after(2);
    assert!(!bail.is_reached(1));
    assert!(bail.is_reached(2));
    assert!(bail.is_reached(3));
}

#[test]
fn an_empty_suite_is_a_successful_run() {
    let report = run_tests([]);
    assert!(report.is_success());
    assert_eq!(report.summary.files, 0);
}

#[test]
fn an_unexpandable_declaration_fails_the_run() {
    let report = run("it.each([1])('c', () => {});");
    assert_eq!(report.summary.unsupported_declarations, 1);
    assert!(!report.is_success());
}

#[test]
fn a_record_name_is_fully_qualified() {
    let report = run("describe('suite', () => { it('case', () => { expect(1).toBe(1); }); });");
    assert_eq!(report.files[0].records[0].name, "suite > case");
}

#[test]
fn the_slowest_files_list_is_stable() {
    let report = run_tests([
        ("a.test.js", "it('a', () => {});"),
        ("b.test.js", "it('b', () => {});"),
        ("c.test.js", "it('c', () => {});"),
    ]);

    let slowest = report.slowest_files(2);
    assert_eq!(slowest.len(), 2);
    assert!(slowest[0].duration_micros >= slowest[1].duration_micros);
}

#[test]
fn asking_for_more_slow_files_than_exist_is_fine() {
    let report = run("it('a', () => {});");
    assert_eq!(report.slowest_files(100).len(), 1);
}

#[test]
fn concurrency_resolves_to_at_least_one_worker() {
    assert_eq!(Concurrency::Serial.threads(), 1);
    assert!(Concurrency::Auto.threads() >= 1);
    assert_eq!(
        Concurrency::Fixed(NonZeroUsize::new(4).unwrap()).threads(),
        4
    );
    assert!(Concurrency::Serial.is_serial());
    assert!(!Concurrency::Fixed(NonZeroUsize::new(4).unwrap()).is_serial());
}

#[test]
fn a_skipped_record_names_the_reason() {
    let report = run("it.skip('off', () => {});");
    assert!(matches!(
        report.files[0].records[0].status,
        TestStatus::Skipped {
            reason: SkipReason::Explicit
        }
    ));
}

#[test]
fn a_file_report_exposes_its_duration_as_a_duration() {
    let report = run("it('a', () => {});");
    assert_eq!(
        report.files[0].duration(),
        Duration::from_micros(report.files[0].duration_micros)
    );
    assert_eq!(
        report.summary.duration(),
        Duration::from_micros(report.summary.duration_micros)
    );
}

#[test]
fn a_file_status_explains_itself() {
    assert!(
        FileStatus::TimedOut {
            budget_micros: 5_000
        }
        .describe()
        .contains("5000")
    );
    assert!(
        FileStatus::TooLarge { bytes: 9, limit: 8 }
            .describe()
            .contains('9')
    );
    assert!(
        FileStatus::Panicked {
            message: "boom".to_string()
        }
        .describe()
        .contains("boom")
    );
    assert_eq!(FileStatus::Completed.describe(), "completed");
    assert!(FileStatus::NotRun.describe().contains("bail"));
}

#[test]
fn an_unsupported_reason_explains_itself() {
    assert!(
        UnsupportedReason::Matcher {
            matcher: "toContain".to_string()
        }
        .describe()
        .contains("toContain")
    );
    assert!(
        UnsupportedReason::Expression
            .describe()
            .contains("constant")
    );
    assert!(UnsupportedReason::Malformed.describe().contains("balanced"));
}

#[test]
fn a_run_report_round_trips_through_json() {
    let report = run("describe('s', () => { it('a', () => { expect(1).toBe(2); }); });");
    let json = serde_json::to_string(&report).unwrap();
    let back: crate::TestRunReport = serde_json::from_str(&json).unwrap();

    assert_eq!(back.summary, report.summary);
    assert_eq!(back.files, report.files);
}

#[test]
fn a_very_long_expression_is_truncated_in_the_report() {
    let mut source = String::from("it('a', () => { expect(");
    source.push_str(&"x".repeat(crate::MAX_EXPRESSION_BYTES * 2));
    source.push_str(").toContain(1); });");

    let report = run_tests([("a.test.js", source.as_str())]);
    let TestStatus::Unsupported { assertions } = &report.files[0].records[0].status else {
        panic!("expected an unsupported status");
    };
    assert!(assertions[0].expression.len() <= crate::MAX_EXPRESSION_BYTES + 4);
}
