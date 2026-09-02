//! What the source-level subset decides, and what it refuses to decide.
//!
//! Every case here is about one test body: which assertions hold, which fail,
//! and which the subset must report as unsupported rather than claim as a pass.

use crate::{TestStatus, UnsupportedReason, run_tests};

fn run(source: &str) -> crate::TestRunReport {
    run_tests([("math.test.js", source)])
}

#[test]
fn run_tests_executes_supported_assertions() {
    let report = run(r#"
          import { it } from "@uniflowed/test";
          it("adds", () => {
            expect(1 + 1).toBe(2);
            expect(createId("flow")).toEqual("flow");
          });
        "#);

    assert!(report.is_success());
    assert_eq!(report.summary.passed, 1);
    assert_eq!(report.summary.unsupported_assertions, 0);
}

#[test]
fn run_tests_reports_failures() {
    let report = run(r#"
          it("fails", () => {
            expect("flow").toBe("typescript");
          });
        "#);

    assert!(!report.is_success());
    assert_eq!(report.summary.failed, 1);
    let failure = report.failures().next().unwrap();
    let TestStatus::Failed { failures, .. } = &failure.status else {
        panic!("expected a failure");
    };
    assert!(failures[0].message.contains("toBe assertion failed"));
}

#[test]
fn a_failure_carries_the_position_of_the_assertion() {
    let report = run("it('fails', () => {\n  expect(1).toBe(2);\n});\n");
    let TestStatus::Failed { failures, .. } = &report.files[0].records[0].status else {
        panic!("expected a failure");
    };

    assert_eq!(failures[0].line, 2);
    assert_eq!(failures[0].column, 3);
    assert!(failures[0].span >= "expect(1).toBe(2)".len());
}

#[test]
fn run_tests_rejects_unsupported_matchers_by_name() {
    let report = run(r#"
          it("needs matcher support", () => {
            expect([1, 2, 3]).toContain(2);
          });
        "#);

    assert!(!report.is_success());
    assert_eq!(report.summary.unsupported_assertions, 1);
    let TestStatus::Unsupported { assertions } = &report.files[0].records[0].status else {
        panic!("expected an unsupported status");
    };
    assert_eq!(
        assertions[0].reason,
        UnsupportedReason::Matcher {
            matcher: "toContain".to_string()
        }
    );
    assert!(assertions[0].expression.contains("toContain"));
}

#[test]
fn an_unsupported_test_is_never_reported_as_a_pass() {
    let report = run("it('a', () => { expect(x).toContain(2); });");
    assert_eq!(report.summary.passed, 0);
    assert_eq!(report.summary.unsupported, 1);
}

#[test]
fn non_constant_operands_are_unsupported_rather_than_failed() {
    let report = run("it('a', () => { expect(total).toBe(count); });");
    let TestStatus::Unsupported { assertions } = &report.files[0].records[0].status else {
        panic!("expected an unsupported status");
    };
    assert_eq!(assertions[0].reason, UnsupportedReason::Expression);
}

#[test]
fn a_thrown_error_fails_the_test_with_its_message() {
    let report = run("it('a', () => { throw new Error('boom'); });");
    let TestStatus::Failed { failures, .. } = &report.files[0].records[0].status else {
        panic!("expected a failure");
    };
    assert_eq!(failures[0].message, "boom");
}

#[test]
fn a_thrown_error_without_a_literal_message_still_fails() {
    let report = run("it('a', () => { throw new Error(reason); });");
    let TestStatus::Failed { failures, .. } = &report.files[0].records[0].status else {
        panic!("expected a failure");
    };
    assert_eq!(failures[0].message, "test threw Error");
}

#[test]
fn run_tests_accepts_native_react_testing_visibility_contract() {
    let report = run(r#"
          it("renders", async () => {
            render(<Page />);
            await expect(screen.findByText("Flow at native speed")).resolves.toBeVisible();
          });
        "#);

    assert!(report.is_success());
    assert_eq!(report.summary.passed, 1);
}

#[test]
fn a_visibility_assertion_without_a_render_is_unsupported() {
    let report =
        run("it('renders', async () => { await expect(other()).resolves.toBeVisible(); });");
    assert_eq!(report.summary.unsupported, 1);
}

#[test]
fn a_concise_arrow_body_is_evaluated() {
    let report = run("it('a', () => expect(1).toBe(1));");
    assert!(report.is_success());
    assert_eq!(report.summary.passed, 1);
}

#[test]
fn a_function_expression_body_is_evaluated() {
    let report = run("it('a', function () { expect(1).toBe(2); });");
    assert_eq!(report.summary.failed, 1);
}

#[test]
fn a_body_that_never_closes_does_not_reach_the_next_test() {
    let report = run("it('a', () => { expect(1).toBe(2)\nit('b', () => { expect(1).toBe(1); });");
    // `a` cannot be closed, so nothing is claimed for it; `b` still runs.
    assert!(report.summary.failed + report.summary.passed <= 2);
    assert!(report.files[0].records.iter().any(|r| r.name == "b"));
}

#[test]
fn every_declaration_is_reported_even_when_skipped() {
    let report = run("it.skip('off', () => {});\nit('on', () => {});");
    assert_eq!(report.files[0].records.len(), 2);
    assert_eq!(report.summary.skipped, 1);
    assert_eq!(report.summary.passed, 1);
}

#[test]
fn a_todo_declaration_is_counted_apart_from_a_skip() {
    let report = run("it.todo('later');");
    assert_eq!(report.summary.todo, 1);
    assert_eq!(report.summary.skipped, 0);
    assert!(matches!(
        report.files[0].records[0].status,
        TestStatus::Todo
    ));
}
