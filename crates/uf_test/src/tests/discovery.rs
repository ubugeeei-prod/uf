//! What discovery finds, and what it refuses to find.

use crate::{
    MAX_CASES_PER_FILE, MAX_SOURCE_BYTES, NativeTestRunnerPlan, TestKind, TestPerformanceTarget,
    TestRuntime, TestScheduler, discover_tests, merge_plans,
};

fn names(source: &str) -> Vec<String> {
    discover_tests("a.test.js", source)
        .cases
        .into_iter()
        .map(|case| case.name)
        .collect()
}

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

    similar_asserts::assert_eq!(names(source), vec!["real test".to_string()]);
}

#[test]
fn ignores_a_member_call_on_another_object() {
    similar_asserts::assert_eq!(
        names("page.it('not ours', () => {});"),
        Vec::<String>::new()
    );
}

#[test]
fn ignores_declarations_inside_line_comments() {
    similar_asserts::assert_eq!(
        names("// it('commented', () => {});\n"),
        Vec::<String>::new()
    );
}

#[test]
fn ignores_declarations_inside_block_comments() {
    similar_asserts::assert_eq!(
        names("/* it('commented', () => {}); */\n"),
        Vec::<String>::new()
    );
}

#[test]
fn ignores_declarations_inside_template_literals() {
    similar_asserts::assert_eq!(
        names("const doc = `it('documented', () => {})`;\n"),
        Vec::<String>::new()
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
fn merging_no_plans_produces_an_empty_plan() {
    let merged = merge_plans([]);
    assert!(merged.is_empty());
    assert_eq!(merged.runnable_count(), 0);
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
    assert!(!plan.accepts_import("jest"));
    assert!(plan.react_testing_library_native);
    assert!(plan.official_flow_parser);
}

#[test]
fn an_unexpandable_form_is_recorded_by_name_rather_than_dropped() {
    let plan = discover_tests("a.test.js", "it.each([1, 2])('case %i', () => {});");

    assert!(plan.cases.is_empty());
    assert_eq!(plan.unsupported.len(), 1);
    assert_eq!(plan.unsupported[0].call, "it.each");
    assert_eq!(plan.unsupported[0].line, 1);
}

#[test]
fn several_unexpandable_forms_are_all_recorded() {
    let source = "describe.concurrent('a', () => {});\ntest.failing('b', () => {});\n";
    let plan = discover_tests("a.test.js", source);

    let calls: Vec<&str> = plan
        .unsupported
        .iter()
        .map(|entry| entry.call.as_str())
        .collect();
    similar_asserts::assert_eq!(calls, vec!["describe.concurrent", "test.failing"]);
}

#[test]
fn empty_input_discovers_nothing() {
    let plan = discover_tests("a.test.js", "");
    assert!(plan.is_empty());
    assert!(plan.resolve().is_empty());
}

#[test]
fn crlf_line_endings_report_the_right_lines() {
    let plan = discover_tests("a.test.js", "it('a', () => {});\r\nit('b', () => {});\r\n");
    assert_eq!(plan.cases[0].line, 1);
    assert_eq!(plan.cases[1].line, 2);
}

#[test]
fn a_byte_order_mark_does_not_shift_the_first_line() {
    let plan = discover_tests("a.test.js", "\u{feff}it('a', () => {});\n");
    assert_eq!(plan.cases.len(), 1);
    assert_eq!(plan.cases[0].line, 1);
}

#[test]
fn non_ascii_names_survive_discovery() {
    let plan = discover_tests("a.test.js", "it('日本語のテスト', () => {});");
    assert_eq!(plan.cases[0].name, "日本語のテスト");
}

#[test]
fn escaped_quotes_inside_a_name_are_unescaped() {
    let plan = discover_tests("a.test.js", r#"it('it\'s fine', () => {});"#);
    assert_eq!(plan.cases[0].name, "it's fine");
}

#[test]
fn a_template_literal_name_is_not_a_declaration_this_runner_reads() {
    let plan = discover_tests("a.test.js", "it(`dynamic ${x}`, () => {});");
    assert!(plan.cases.is_empty());
}

#[test]
fn a_source_past_the_size_limit_is_not_scanned() {
    let mut source = String::with_capacity(MAX_SOURCE_BYTES + 64);
    source.push_str("it('a', () => {});\n");
    while source.len() <= MAX_SOURCE_BYTES {
        source.push_str("// filler filler filler filler filler filler filler\n");
    }

    let plan = discover_tests("huge.test.js", &source);
    assert!(plan.is_empty());
}

#[test]
fn the_declaration_count_is_bounded() {
    let mut source = String::new();
    for index in 0..(MAX_CASES_PER_FILE + 25) {
        source.push_str("it('c");
        source.push_str(&index.to_string());
        source.push_str("', () => {});\n");
    }

    let plan = discover_tests("many.test.js", &source);
    assert_eq!(plan.cases.len(), MAX_CASES_PER_FILE);
}

#[test]
fn a_case_is_ordered_by_position_not_by_registration_identifier() {
    let source = "test('z', () => {});\nit('a', () => {});\ndescribe('m', () => {});\n";
    let plan = discover_tests("a.test.js", source);

    let names: Vec<&str> = plan.cases.iter().map(|case| case.name.as_str()).collect();
    similar_asserts::assert_eq!(names, vec!["z", "a", "m"]);
}

#[test]
fn a_declaration_without_a_string_name_is_not_recorded() {
    let plan = discover_tests("a.test.js", "it(name, () => {});");
    assert!(plan.cases.is_empty());
}
