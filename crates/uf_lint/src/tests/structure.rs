//! The scope-sensitive rules: nested `component` and `hook` declarations, and the
//! rules of hooks -- which is where a lexer-free scan has to be most careful.

use super::*;

#[test]
fn nested_component_declarations_are_rejected() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\ncomponent Outer() {\n  component Inner() { return null; }\n  return <Inner />;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (3, 3));
}

#[test]
fn sibling_component_declarations_are_accepted() {
    let diagnostics = lint_js(
        "flow/nested-component",
        "// @flow\ncomponent Inner() { return null; }\ncomponent Outer() { return <Inner />; }\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn nested_hook_declarations_are_rejected() {
    let diagnostics = lint_js(
        "flow/nested-hook",
        "// @flow\ncomponent Outer() {\n  hook useInner(): number { return 1; }\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn top_level_hook_declarations_are_accepted() {
    let diagnostics = lint_js(
        "flow/nested-hook",
        "// @flow\nhook useInner(): number { return 1; }\ncomponent Outer() { return null; }\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_reject_conditional_hook_calls() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page(flag: boolean) {\n  if (flag) {\n    const [a] = useState(0);\n  }\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
    assert!(diagnostics[0].message.contains("top level"));
}

#[test]
fn hooks_rules_reject_hook_calls_in_callbacks() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent List(items: Array<string>) {\n  items.forEach(() => {\n    useEffect(() => {});\n  });\n  return null;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
}

#[test]
fn hooks_rules_reject_hook_calls_outside_any_component() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nconst value = useState(0);\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("component"));
}

#[test]
fn hooks_rules_reject_hook_calls_in_plain_functions() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nfunction helper() {\n  return useState(0);\n}\n",
    );

    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn hooks_rules_accept_top_level_calls_in_a_component() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page() {\n  const [a, setA] = useState(0);\n  useEffect(() => {});\n  return null;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_accept_top_level_calls_in_a_hook_declaration() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nhook useThing(): number {\n  const [a] = useState(0);\n  return a;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_accept_top_level_calls_in_a_use_prefixed_function() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nexport const useThing = (): number => {\n  const [a] = useState(0);\n  return a;\n};\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_do_not_treat_a_declaration_as_a_call() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\nfunction useThing(): number {\n  return 1;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_ignore_property_reads_that_look_like_hooks() {
    let diagnostics = lint_js("// @flow", "// @flow\n");
    assert!(diagnostics.is_empty());

    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page(api: Api) {\n  api.useThing();\n  return null;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_tolerate_jsx_expression_containers() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page() {\n  return <main>{useThing()}</main>;\n}\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn hooks_rules_are_not_confused_by_braces_inside_strings() {
    let diagnostics = lint_js(
        "react/hooks-rules",
        "// @flow\ncomponent Page() {\n  const s = \"}\";\n  const [a] = useState(0);\n  return s;\n}\n",
    );

    assert!(diagnostics.is_empty());
}
