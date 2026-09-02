use super::*;

#[test]
fn discovers_describe_it_and_test_calls() {
    let source = r#"
        import { describe, expect, it, test } from '@uniflowed/testing';

        describe('math', () => {
          it('adds values', () => {});
          test("subtracts values", () => {});
        });
    "#;

    let plan = discover_tests("src/math.test.js", source);

    assert_eq!(plan.cases.len(), 3);
    assert_eq!(plan.runnable_count(), 2);
    assert_eq!(plan.cases[0].kind, TestKind::Describe);
    assert_eq!(plan.cases[1].name, "adds values");
    assert_eq!(plan.cases[2].name, "subtracts values");
}

#[test]
fn ignores_identifier_substrings() {
    let source = r#"
        const within = 'not a test';
        const title = "it('also not a test')";
        it('real test', () => {});
    "#;

    let plan = discover_tests("src/a.test.js", source);

    similar_asserts::assert_eq!(
        plan.cases
            .iter()
            .map(|case| case.name.as_str())
            .collect::<Vec<_>>(),
        vec!["real test"]
    );
}

#[test]
fn merges_plans_in_file_order() {
    let a = discover_tests("b.test.js", "it('b', () => {})");
    let b = discover_tests("a.test.js", "it('a', () => {})");

    let merged = merge_plans([a, b]);

    assert_eq!(merged.cases[0].file, "a.test.js");
    assert_eq!(merged.cases[1].file, "b.test.js");
}

#[test]
fn runner_plan_is_self_hosted_and_faster_than_bun_targeted() {
    let plan = NativeTestRunnerPlan::self_hosted();

    assert_eq!(plan.runtime, TestRuntime::UfSelfHosted);
    assert_eq!(plan.scheduler, TestScheduler::NativeWorkStealing);
    assert_eq!(
        plan.performance_target,
        TestPerformanceTarget::FasterThanBun
    );
    assert!(plan.accepts_import("@uniflowed/test"));
    assert!(plan.accepts_import("@uniflowed/testing"));
    assert!(plan.react_testing_library_native);
    assert!(plan.official_flow_parser);
}

#[test]
fn run_tests_executes_supported_assertions() {
    let report = run_tests([(
        "math.test.js",
        r#"
          import { it } from "@uniflowed/test";
          it("adds", () => {
            expect(1 + 1).toBe(2);
            expect(createId("flow")).toEqual("flow");
          });
        "#,
    )]);

    assert!(report.is_success());
    assert_eq!(report.passed, 1);
    assert_eq!(report.unsupported_assertions, 0);
}

#[test]
fn run_tests_reports_failures() {
    let report = run_tests([(
        "math.test.js",
        r#"
          it("fails", () => {
            expect("flow").toBe("typescript");
          });
        "#,
    )]);

    assert!(!report.is_success());
    assert_eq!(report.failed, 1);
    assert!(report.failures[0].message.contains("toBe assertion failed"));
}

#[test]
fn run_tests_rejects_unsupported_assertions() {
    let report = run_tests([(
        "math.test.js",
        r#"
          it("needs matcher support", () => {
            expect([1, 2, 3]).toContain(2);
          });
        "#,
    )]);

    assert!(!report.is_success());
    assert_eq!(report.unsupported_assertions, 1);
}

#[test]
fn run_tests_accepts_native_react_testing_visibility_contract() {
    let report = run_tests([(
        "page.test.js",
        r#"
          it("renders", async () => {
            render(<Page />);
            await expect(screen.findByText("Flow at native speed")).resolves.toBeVisible();
          });
        "#,
    )]);

    assert!(report.is_success());
    assert_eq!(report.passed, 1);
}
