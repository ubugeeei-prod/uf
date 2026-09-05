//! Bounds, determinism, and input that was never valid JavaScript.

use super::{accepts, check, findings};
use crate::error::{MAX_SCOPE_DEPTH, MAX_SOURCE_BYTES};
use crate::rule::Finding;
use crate::{ReactCompilerError, validate};

/// A module exercising every check at once.
const FIXTURE: &str = "// @flow\n\
     import { registry } from \"./registry.js\";\n\
     let renders = 0;\n\
     component Page(items: Array<string>, flag: boolean) {\n\
     \x20 const box = useRef(null);\n\
     \x20 console.log(box.current);\n\
     \x20 if (flag) { const [a] = useState(0); }\n\
     \x20 items.push(\"x\");\n\
     \x20 renders = renders + 1;\n\
     \x20 registry.count = 1;\n\
     \x20 return <p>{Date.now()}</p>;\n\
     }\n";

#[test]
fn validating_the_same_module_twice_gives_the_same_answer() {
    assert_eq!(check(FIXTURE), check(FIXTURE));
}

#[test]
fn the_fixture_reports_every_rule_once() {
    let mut rules: Vec<&str> = check(FIXTURE).iter().map(|entry| entry.rule()).collect();
    rules.sort_unstable();
    rules.dedup();
    assert_eq!(
        rules,
        [
            "react/hooks-rules",
            "react/no-props-mutation",
            "react/no-ref-read-in-render",
            "react/no-render-side-effects",
        ]
    );
}

#[test]
fn diagnostics_come_back_in_position_order() {
    let diagnostics = check(FIXTURE);
    assert!(
        diagnostics
            .windows(2)
            .all(|pair| (pair[0].line, pair[0].column) <= (pair[1].line, pair[1].column))
    );
}

#[test]
fn crlf_and_lf_report_the_same_lines() {
    let lf = check(FIXTURE);
    let crlf = check(&FIXTURE.replace('\n', "\r\n"));
    assert_eq!(
        lf.iter().map(|entry| entry.line).collect::<Vec<_>>(),
        crlf.iter().map(|entry| entry.line).collect::<Vec<_>>()
    );
}

#[test]
fn a_byte_order_mark_does_not_move_the_first_line() {
    let plain = check("component Page() {\n  console.log(1);\n  return null;\n}\n");
    let marked = check("\u{feff}component Page() {\n  console.log(1);\n  return null;\n}\n");
    assert_eq!(plain[0].line, marked[0].line);
}

#[test]
fn a_non_ascii_line_keeps_byte_columns() {
    let diagnostics = check(
        "component Page() {\n  const 見出し = 1;\n  console.log(見出し);\n  return null;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].column, 3);
}

#[test]
fn a_module_over_the_size_ceiling_is_refused() {
    let source = "x".repeat(MAX_SOURCE_BYTES + 1);
    assert!(matches!(
        validate(&source),
        Err(ReactCompilerError::SourceTooLarge { .. })
    ));
}

#[test]
fn a_module_nested_past_the_scope_ceiling_is_refused() {
    let mut source = String::from("component Page() {\n");
    source.push_str(&"{".repeat(MAX_SCOPE_DEPTH + 8));
    source.push_str(&"}".repeat(MAX_SCOPE_DEPTH + 8));
    source.push_str("\n}\n");
    match validate(&source) {
        Err(ReactCompilerError::ScopeTooDeep { limit, .. }) => assert_eq!(limit, MAX_SCOPE_DEPTH),
        other => panic!("expected a depth refusal, got {other:?}"),
    }
}

#[test]
fn deep_nesting_does_not_exhaust_the_stack() {
    let depth = 100_000;
    let mut source = String::from("component Page() {\n");
    source.push_str(&"if (x) {".repeat(depth));
    source.push_str(&"}".repeat(depth));
    source.push_str("\nreturn null;\n}\n");
    assert!(validate(&source).is_err());
}

#[test]
fn an_empty_module_reports_nothing() {
    accepts("");
}

#[test]
fn a_module_of_only_a_byte_order_mark_reports_nothing() {
    accepts("\u{feff}");
}

#[test]
fn a_module_of_only_a_shebang_reports_nothing() {
    accepts("#!/usr/bin/env node\n");
}

#[test]
fn a_module_with_no_react_in_it_reports_nothing() {
    accepts("export const total = [1, 2, 3].reduce((a, b) => a + b, 0);\n");
}

#[test]
fn an_unterminated_string_does_not_panic() {
    let _ = validate("component Page() {\n  const s = \"open;\n  return s;\n}\n");
}

#[test]
fn an_unterminated_block_does_not_panic() {
    let _ = validate("component Page() {\n  const [a] = useState(0);\n");
}

#[test]
fn unbalanced_closing_braces_do_not_panic() {
    let _ = validate("}}}}\ncomponent Page() { return null; }\n");
}

#[test]
fn a_truncated_module_at_every_length_does_not_panic() {
    for end in 0..FIXTURE.len() {
        if FIXTURE.is_char_boundary(end) {
            let _ = validate(&FIXTURE[..end]);
        }
    }
}

#[test]
fn a_component_with_no_body_does_not_panic() {
    let _ = validate("component Page(");
    let _ = validate("component Page()");
    let _ = validate("component");
    let _ = validate("hook");
    let _ = validate("const");
    let _ = validate("import {");
}

#[test]
fn a_hook_call_with_no_arguments_list_does_not_panic() {
    accepts("component Page() {\n  const value = useState;\n  return value;\n}\n");
}

#[test]
fn many_findings_stop_at_the_ceiling() {
    let mut source = String::from("component Page() {\n");
    for _ in 0..crate::MAX_DIAGNOSTICS + 32 {
        source.push_str("  console.log(1);\n");
    }
    source.push_str("  return null;\n}\n");
    assert_eq!(check(&source).len(), crate::MAX_DIAGNOSTICS);
}

#[test]
fn a_module_with_thousands_of_bindings_stays_bounded() {
    let mut source = String::from("component Page() {\n");
    for index in 0..crate::MAX_TRACKED_BINDINGS + 64 {
        source.push_str(&format!("  const name{index} = {index};\n"));
    }
    source.push_str("  return null;\n}\n");
    assert!(findings(&source).is_empty());
}

#[test]
fn a_variable_named_hook_is_not_a_hook_declaration() {
    // `hook` is a contextual keyword. React's own Fast Refresh runtime holds
    // the DevTools global in a variable of that name and writes to it at the
    // start of a line — where the walk sees no preceding identifier and used
    // to read the member access as `hook <name>(…) {`, opening a hook body
    // that swallowed the rest of the function. Every module-state write after
    // it was then reported as a write during render.
    accepts(
        "const registry = new Map();\nexport function inject(hook) {\nhook.isDisabled = true;\nhook.inject = function (injected) {\n  registry.set(1, injected);\n};\n}\n",
    );
}

#[test]
fn a_variable_named_component_is_not_a_component_declaration() {
    accepts(
        "const seen = new Map();\nexport function record(component) {\ncomponent.id = 1;\nseen.set(component.id, component);\n}\n",
    );
}

#[test]
fn a_real_hook_declaration_still_declares_one() {
    // The guard must not cost the keyword its meaning.
    let diagnostics = check(
        "hook useThing(flag: boolean): number {\n  if (flag) {\n    const [a] = useState(0);\n  }\n  return 1;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::HookNotAtTopLevel);
}

#[test]
fn a_generic_declaration_is_still_a_declaration() {
    accepts("component Box<T>(value: T) renders React.Node {\n  return null;\n}\n");
}
