//! `.only`, `.skip`, `.todo`, nesting, and the precedence between them.

use crate::plan::{Selection, SkipReason};
use crate::{TestKind, TestModifier, discover_tests, merge_plans};

#[test]
fn records_the_only_modifier() {
    let plan = discover_tests("a.test.js", "it.only('focused', () => {});");
    assert_eq!(plan.cases[0].modifier, TestModifier::Only);
}

#[test]
fn records_the_skip_modifier() {
    let plan = discover_tests("a.test.js", "it.skip('off', () => {});");
    assert_eq!(plan.cases[0].modifier, TestModifier::Skip);
}

#[test]
fn records_the_todo_modifier_without_a_body() {
    let plan = discover_tests("a.test.js", "it.todo('write me');");
    assert_eq!(plan.cases[0].modifier, TestModifier::Todo);
    assert_eq!(plan.cases[0].name, "write me");
}

#[test]
fn records_a_modifier_written_across_lines() {
    let plan = discover_tests("a.test.js", "it\n  .only\n  ('focused', () => {});");
    assert_eq!(plan.cases[0].modifier, TestModifier::Only);
}

#[test]
fn a_describe_can_carry_a_modifier() {
    let plan = discover_tests(
        "a.test.js",
        "describe.skip('suite', () => { it('a', () => {}) });",
    );
    assert_eq!(plan.cases[0].modifier, TestModifier::Skip);
    assert_eq!(plan.cases[0].kind, TestKind::Describe);
}

#[test]
fn nesting_is_recovered_from_byte_ranges() {
    let source = r#"
        describe('outer', () => {
          describe('inner', () => {
            it('leaf', () => {});
          });
        });
    "#;
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    let leaf = plan.cases.len() - 1;
    let ancestors = resolution.ancestors(leaf);
    assert_eq!(ancestors.len(), 2);
    assert_eq!(plan.cases[ancestors[0]].name, "outer");
    assert_eq!(plan.cases[ancestors[1]].name, "inner");
    assert_eq!(resolution.full_name(&plan, leaf), "outer > inner > leaf");
}

#[test]
fn sibling_describes_do_not_nest() {
    let source = "describe('a', () => { it('x', () => {}) });\ndescribe('b', () => { it('y', () => {}) });\n";
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    let names: Vec<String> = (0..plan.cases.len())
        .filter(|index| plan.cases[*index].kind == TestKind::Test)
        .map(|index| resolution.full_name(&plan, index))
        .collect();
    similar_asserts::assert_eq!(names, vec!["a > x".to_string(), "b > y".to_string()]);
}

#[test]
fn a_top_level_test_has_no_ancestors() {
    let plan = discover_tests("a.test.js", "it('alone', () => {});");
    let resolution = plan.resolve();

    assert!(resolution.ancestors(0).is_empty());
    assert_eq!(resolution.full_name(&plan, 0), "alone");
    assert_eq!(resolution.parent(0), None);
}

#[test]
fn only_restricts_the_file_it_appears_in() {
    let source = "it('kept', () => {});\nit.only('focus', () => {});\n";
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    assert_eq!(
        resolution.selection(0),
        Selection::Skipped(SkipReason::NotOnly)
    );
    assert_eq!(resolution.selection(1), Selection::Run);
}

#[test]
fn only_in_one_file_does_not_restrict_another() {
    let plan = merge_plans([
        discover_tests("a.test.js", "it.only('focus', () => {});"),
        discover_tests("b.test.js", "it('other', () => {});"),
    ]);
    let resolution = plan.resolve();

    assert_eq!(resolution.selection(0), Selection::Run);
    assert_eq!(resolution.selection(1), Selection::Run);
}

#[test]
fn an_only_describe_covers_the_tests_inside_it() {
    let source = r#"
        describe.only('focus', () => {
          it('inside', () => {});
        });
        it('outside', () => {});
    "#;
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    let inside = plan
        .cases
        .iter()
        .position(|case| case.name == "inside")
        .unwrap();
    let outside = plan
        .cases
        .iter()
        .position(|case| case.name == "outside")
        .unwrap();
    assert_eq!(resolution.selection(inside), Selection::Run);
    assert_eq!(
        resolution.selection(outside),
        Selection::Skipped(SkipReason::NotOnly)
    );
}

#[test]
fn skip_beats_only_inside_a_focused_describe() {
    let source = r#"
        describe.only('focus', () => {
          it.skip('off', () => {});
          it('on', () => {});
        });
    "#;
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    let off = plan.cases.iter().position(|c| c.name == "off").unwrap();
    let on = plan.cases.iter().position(|c| c.name == "on").unwrap();
    assert_eq!(
        resolution.selection(off),
        Selection::Skipped(SkipReason::Explicit)
    );
    assert_eq!(resolution.selection(on), Selection::Run);
}

#[test]
fn todo_beats_skip() {
    let source = "describe.skip('suite', () => { it.todo('later'); });";
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    let later = plan.cases.iter().position(|c| c.name == "later").unwrap();
    assert_eq!(resolution.selection(later), Selection::Todo);
}

#[test]
fn a_skipped_describe_skips_everything_inside_it() {
    let source = "describe.skip('suite', () => { it('a', () => {}); it('b', () => {}); });";
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    for index in 0..plan.cases.len() {
        assert_eq!(
            resolution.selection(index),
            Selection::Skipped(SkipReason::Explicit),
            "case {index} should be skipped"
        );
    }
}

#[test]
fn an_unbalanced_describe_does_not_swallow_the_rest_of_the_file() {
    let source = "describe('broken', () => {\nit('after', () => {});\n";
    let plan = discover_tests("a.test.js", source);
    let resolution = plan.resolve();

    let after = plan.cases.iter().position(|c| c.name == "after").unwrap();
    assert_eq!(resolution.parent(after), None);
}

#[test]
fn a_plan_round_trips_through_json() {
    let plan = discover_tests(
        "a.test.js",
        "describe('s', () => { it.only('t', () => {}) });",
    );
    let json = serde_json::to_string(&plan).unwrap();
    let back: crate::TestPlan = serde_json::from_str(&json).unwrap();

    assert_eq!(back.cases.len(), plan.cases.len());
    assert_eq!(back.cases[1].modifier, TestModifier::Only);
}

#[test]
fn the_modifier_suffix_renders_as_written() {
    assert_eq!(TestModifier::None.suffix(), "");
    assert_eq!(TestModifier::Only.suffix(), ".only");
    assert_eq!(TestModifier::Skip.suffix(), ".skip");
    assert_eq!(TestModifier::Todo.suffix(), ".todo");
}
