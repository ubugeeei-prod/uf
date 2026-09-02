//! `react/no-props-mutation` and `react/no-mutation-after-hook`.

use super::{accepts, check, findings};
use crate::rule::Finding;

#[test]
fn assigning_to_a_prop_is_rejected() {
    let diagnostics =
        check("component Page(title: string) {\n  title = \"x\";\n  return null;\n}\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::PropsMutated);
    assert_eq!(diagnostics[0].symbol.as_deref(), Some("title"));
}

#[test]
fn assigning_to_a_property_of_a_prop_is_rejected() {
    assert_eq!(
        findings("component Page(style: Style) {\n  style.color = \"red\";\n  return null;\n}\n"),
        [Finding::PropsMutated]
    );
}

#[test]
fn assigning_through_a_nested_property_is_rejected() {
    assert_eq!(
        findings(
            "component Page(theme: Theme) {\n  theme.colors.ink = \"red\";\n  return null;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn assigning_through_a_subscript_is_rejected() {
    assert_eq!(
        findings(
            "component Page(items: Array<string>) {\n  items[0] = \"x\";\n  return null;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn a_compound_assignment_to_a_prop_is_rejected() {
    assert_eq!(
        findings("component Page(counts: Counts) {\n  counts.total += 1;\n  return null;\n}\n"),
        [Finding::PropsMutated]
    );
}

#[test]
fn incrementing_a_prop_is_rejected() {
    assert_eq!(
        findings("component Page(count: number) {\n  count++;\n  return null;\n}\n"),
        [Finding::PropsMutated]
    );
}

#[test]
fn deleting_from_a_prop_is_rejected() {
    assert_eq!(
        findings("component Page(config: Config) {\n  delete config.width;\n  return null;\n}\n"),
        [Finding::PropsMutated]
    );
}

#[test]
fn a_mutating_method_call_on_a_prop_is_rejected() {
    assert_eq!(
        findings(
            "component Page(items: Array<string>) {\n  items.push(\"x\");\n  return null;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn a_mutating_method_call_through_a_property_is_rejected() {
    assert_eq!(
        findings("component Page(data: Data) {\n  data.rows.sort();\n  return null;\n}\n"),
        [Finding::PropsMutated]
    );
}

#[test]
fn a_non_mutating_method_call_on_a_prop_is_accepted() {
    accepts(
        "component Page(items: Array<string>) {\n  const next = items.map((x) => x);\n  return next;\n}\n",
    );
}

#[test]
fn reading_a_prop_is_accepted() {
    accepts("component Page(title: string) {\n  return <h1>{title}</h1>;\n}\n");
}

#[test]
fn an_alias_of_a_prop_is_still_a_prop() {
    assert_eq!(
        findings(
            "component Page(data: Data) {\n  const rows = data.rows;\n  rows.push(1);\n  return null;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn an_alias_two_steps_from_a_prop_is_still_a_prop() {
    assert_eq!(
        findings(
            "component Page(data: Data) {\n  const rows = data.rows;\n  const first = rows.first;\n  first.name = \"x\";\n  return null;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn a_copy_of_a_prop_may_be_written_to() {
    // `[...items]` builds a new array, and writing to a new array is what an
    // author is supposed to do instead of writing to the prop.
    accepts(
        "component Page(items: Array<string>) {\n  const copy = [...items];\n  copy.push(\"x\");\n  return copy;\n}\n",
    );
}

#[test]
fn an_object_built_from_a_prop_may_be_written_to() {
    accepts(
        "component Page(style: Style) {\n  const next = { ...style };\n  next.color = \"red\";\n  return next;\n}\n",
    );
}

#[test]
fn a_value_from_a_call_on_a_prop_may_be_written_to() {
    accepts(
        "component Page(items: Array<string>) {\n  const next = items.slice();\n  next.push(\"x\");\n  return next;\n}\n",
    );
}

#[test]
fn a_destructured_prop_is_still_a_prop() {
    assert_eq!(
        findings(
            "component Page(data: Data) {\n  const { rows } = data;\n  rows.push(1);\n  return null;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn a_prop_of_one_component_is_not_a_prop_of_the_next() {
    accepts(
        "component First(items: Array<string>) {\n  return items;\n}\n\
         component Second() {\n  const items = [];\n  items.push(\"x\");\n  return items;\n}\n",
    );
}

#[test]
fn a_local_binding_that_shadows_a_prop_name_may_be_written_to() {
    accepts(
        "component Page(items: Array<string>) {\n  const draft = [];\n  draft.push(items.length);\n  return draft;\n}\n",
    );
}

#[test]
fn a_hook_parameter_is_not_a_prop() {
    accepts("hook useThing(list: Array<string>) {\n  list.push(\"x\");\n  return list;\n}\n");
}

#[test]
fn a_mutation_inside_a_callback_is_still_a_prop_mutation() {
    // Writing to props is wrong wherever it happens, so this is reported even
    // though the callback is not render position.
    assert_eq!(
        findings(
            "component Page(items: Array<string>) {\n  const onClick = () => { items.push(\"x\"); };\n  return onClick;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn writing_to_a_value_after_handing_it_to_a_hook_is_rejected() {
    let diagnostics = check(
        "component Page() {\n  const config = { width: 1 };\n  const value = useDeferredValue(config);\n  config.width = 2;\n  return value;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::MutationAfterHook);
    assert_eq!(diagnostics[0].line, 4);
}

#[test]
fn writing_to_a_dependency_after_the_hook_is_rejected() {
    assert_eq!(
        findings(
            "component Page() {\n  const config = { width: 1 };\n  const value = useMemo(() => compute(config), [config]);\n  config.width = 2;\n  return value;\n}\n"
        ),
        [Finding::MutationAfterHook]
    );
}

#[test]
fn a_mutating_method_after_the_hook_is_rejected() {
    assert_eq!(
        findings(
            "component Page() {\n  const rows = [];\n  const value = useMemo(() => rows.length, [rows]);\n  rows.push(1);\n  return value;\n}\n"
        ),
        [Finding::MutationAfterHook]
    );
}

#[test]
fn writing_before_the_hook_sees_the_value_is_accepted() {
    accepts(
        "component Page() {\n  const config = { width: 1 };\n  config.width = 2;\n  const value = useDeferredValue(config);\n  return value;\n}\n",
    );
}

#[test]
fn a_property_name_that_matches_a_binding_is_not_an_argument() {
    accepts(
        "component Page() {\n  const config = { width: 1 };\n  const value = useDeferredValue(other.config);\n  config.width = 2;\n  return value;\n}\n",
    );
}

#[test]
fn a_callee_is_not_an_argument_handed_to_the_hook() {
    accepts(
        "component Page() {\n  const compute = () => 1;\n  const value = useMemo(() => compute(), []);\n  compute.cache = 1;\n  return value;\n}\n",
    );
}

#[test]
fn a_prop_written_to_after_a_hook_is_reported_as_a_prop_mutation() {
    // One mistake, one diagnostic: the more specific rule wins.
    assert_eq!(
        findings(
            "component Page(items: Array<string>) {\n  const value = useDeferredValue(items);\n  items.push(\"x\");\n  return value;\n}\n"
        ),
        [Finding::PropsMutated]
    );
}

#[test]
fn a_local_value_a_hook_never_saw_may_be_written_to() {
    accepts("component Page() {\n  const draft = [];\n  draft.push(1);\n  return draft;\n}\n");
}
