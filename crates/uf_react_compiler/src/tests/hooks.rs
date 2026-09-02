//! `react/hooks-rules`: hooks are called unconditionally, at the top level.

use super::{accepts, check, findings};
use crate::rule::Finding;

#[test]
fn a_top_level_call_in_a_component_is_accepted() {
    accepts(
        "component Page() {\n  const [a, setA] = useState(0);\n  useEffect(() => {});\n  return null;\n}\n",
    );
}

#[test]
fn a_top_level_call_in_a_hook_declaration_is_accepted() {
    accepts("hook useThing(): number {\n  const [a] = useState(0);\n  return a;\n}\n");
}

#[test]
fn a_top_level_call_in_a_use_prefixed_function_is_accepted() {
    accepts(
        "export const useThing = (): number => {\n  const [a] = useState(0);\n  return a;\n};\n",
    );
}

#[test]
fn a_top_level_call_in_a_use_prefixed_declaration_is_accepted() {
    accepts("export function useThing(): number {\n  const [a] = useState(0);\n  return a;\n}\n");
}

#[test]
fn a_conditional_call_is_rejected() {
    let diagnostics = check(
        "component Page(flag: boolean) {\n  if (flag) {\n    const [a] = useState(0);\n  }\n  return null;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::HookNotAtTopLevel);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn a_call_inside_a_loop_is_rejected() {
    assert_eq!(
        findings(
            "component Page() {\n  for (const x of xs) {\n    useEffect(() => {});\n  }\n  return null;\n}\n"
        ),
        [Finding::HookNotAtTopLevel]
    );
}

#[test]
fn a_call_inside_a_callback_is_rejected() {
    let diagnostics = check(
        "component List(items: Array<string>) {\n  items.forEach(() => {\n    useEffect(() => {});\n  });\n  return null;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn a_call_inside_a_try_block_is_rejected() {
    assert_eq!(
        findings(
            "component Page() {\n  try {\n    useEffect(() => {});\n  } catch (error) {}\n  return null;\n}\n"
        ),
        [Finding::HookNotAtTopLevel]
    );
}

#[test]
fn a_call_after_an_early_return_is_rejected() {
    let diagnostics = check(
        "component Page(flag: boolean) {\n  if (!flag) {\n    return null;\n  }\n  const [a] = useState(0);\n  return a;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::HookAfterEarlyReturn);
    assert_eq!(diagnostics[0].line, 5);
}

#[test]
fn a_call_after_a_brace_less_early_return_is_rejected() {
    assert_eq!(
        findings(
            "component Page(flag: boolean) {\n  if (!flag) return null;\n  const [a] = useState(0);\n  return a;\n}\n"
        ),
        [Finding::HookAfterEarlyReturn]
    );
}

#[test]
fn a_call_inside_the_returned_expression_is_accepted() {
    accepts("component Page() {\n  return <main>{useThing()}</main>;\n}\n");
}

#[test]
fn a_call_before_the_only_return_is_accepted() {
    accepts("component Page() {\n  const [a] = useState(0);\n  return a;\n}\n");
}

#[test]
fn a_return_inside_a_callback_does_not_block_later_hooks() {
    accepts(
        "component Page(items: Array<string>) {\n  const names = items.map((item) => { return item; });\n  const [a] = useState(0);\n  return a;\n}\n",
    );
}

#[test]
fn a_call_outside_any_component_is_rejected() {
    let diagnostics = check("const value = useState(0);\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::HookOutsideComponent);
}

#[test]
fn a_call_in_a_plain_function_is_rejected() {
    assert_eq!(
        findings("function helper() {\n  return useState(0);\n}\n"),
        [Finding::HookOutsideComponent]
    );
}

#[test]
fn a_call_in_a_plain_arrow_is_rejected() {
    assert_eq!(
        findings("const helper = () => {\n  return useState(0);\n};\n"),
        [Finding::HookOutsideComponent]
    );
}

#[test]
fn a_declaration_is_not_a_call() {
    accepts("function useThing(): number {\n  return 1;\n}\n");
}

#[test]
fn a_hook_declaration_is_not_a_call() {
    accepts("hook useThing(): number {\n  return 1;\n}\n");
}

#[test]
fn a_property_read_that_looks_like_a_hook_is_ignored() {
    accepts("component Page(api: Api) {\n  api.useThing();\n  return null;\n}\n");
}

#[test]
fn braces_inside_a_string_do_not_move_the_scope() {
    accepts("component Page() {\n  const s = \"}\";\n  const [a] = useState(0);\n  return s;\n}\n");
}

#[test]
fn braces_inside_a_template_do_not_move_the_scope() {
    accepts("component Page() {\n  const s = `}`;\n  const [a] = useState(0);\n  return s;\n}\n");
}

#[test]
fn braces_inside_a_comment_do_not_move_the_scope() {
    accepts("component Page() {\n  // }\n  const [a] = useState(0);\n  return a;\n}\n");
}

#[test]
fn a_brace_inside_a_regular_expression_does_not_move_the_scope() {
    accepts(
        "component Page() {\n  const re = /[}]/;\n  const [a] = useState(0);\n  return re;\n}\n",
    );
}

#[test]
fn an_object_literal_does_not_count_as_a_nested_scope() {
    accepts(
        "component Page() {\n  const config = { size: 1 };\n  const [a] = useState(config);\n  return a;\n}\n",
    );
}

#[test]
fn a_nested_component_gets_its_own_hook_scope() {
    accepts(
        "component Outer() {\n  component Inner() {\n    const [a] = useState(0);\n    return a;\n  }\n  return <Inner />;\n}\n",
    );
}

#[test]
fn a_use_named_function_nested_in_a_component_may_call_hooks() {
    accepts(
        "component Page() {\n  function useLocal(): number {\n    const [a] = useState(0);\n    return a;\n  }\n  return useLocal();\n}\n",
    );
}

#[test]
fn a_flow_return_type_does_not_hide_the_body() {
    accepts("component Page(): React.Node {\n  const [a] = useState(0);\n  return a;\n}\n");
}

#[test]
fn a_typed_arrow_hook_does_not_lose_its_scope() {
    accepts(
        "const useThing = (n: number): number => {\n  const [a] = useState(n);\n  return a;\n};\n",
    );
}

#[test]
fn every_conditional_call_is_reported() {
    assert_eq!(
        findings(
            "component Page(flag: boolean) {\n  if (flag) {\n    useEffect(() => {});\n    const [a] = useState(0);\n  }\n  return null;\n}\n"
        ),
        [Finding::HookNotAtTopLevel, Finding::HookNotAtTopLevel]
    );
}

#[test]
fn the_reported_column_points_at_the_hook() {
    let diagnostics =
        check("component Page(flag: boolean) {\n  if (flag) { useState(0); }\n  return null;\n}\n");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 15));
    assert_eq!(diagnostics[0].span, "useState".len());
    assert_eq!(diagnostics[0].symbol.as_deref(), Some("useState"));
}

#[test]
fn a_hook_rule_reports_under_the_lint_rule_id() {
    let diagnostics = check("const value = useState(0);\n");
    assert_eq!(diagnostics[0].rule(), "react/hooks-rules");
    assert!(diagnostics[0].message().contains("component"));
}

#[test]
fn a_top_level_message_says_top_level() {
    let diagnostics =
        check("component Page(flag: boolean) {\n  if (flag) { useState(0); }\n  return null;\n}\n");
    assert!(diagnostics[0].message().contains("top level"));
}
