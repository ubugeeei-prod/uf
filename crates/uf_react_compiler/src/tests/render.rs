//! `react/no-render-side-effects`.

use super::{accepts, check, findings};
use crate::rule::Finding;

#[test]
fn writing_to_a_module_binding_during_render_is_rejected() {
    let diagnostics = check(
        "let renders = 0;\ncomponent Page() {\n  renders = renders + 1;\n  return renders;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::ModuleBindingAssigned);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn incrementing_a_module_binding_during_render_is_rejected() {
    assert_eq!(
        findings("let renders = 0;\ncomponent Page() {\n  renders++;\n  return renders;\n}\n"),
        [Finding::ModuleBindingAssigned]
    );
}

#[test]
fn writing_to_a_property_of_a_module_binding_during_render_is_rejected() {
    assert_eq!(
        findings(
            "const cache = {};\ncomponent Page(id: string) {\n  cache[id] = 1;\n  return null;\n}\n"
        ),
        [Finding::ModuleBindingAssigned]
    );
}

#[test]
fn writing_to_an_imported_binding_during_render_is_rejected() {
    assert_eq!(
        findings(
            "import { registry } from \"./registry.js\";\ncomponent Page() {\n  registry.count = 1;\n  return null;\n}\n"
        ),
        [Finding::ModuleBindingAssigned]
    );
}

#[test]
fn writing_to_a_module_binding_from_an_event_handler_is_accepted() {
    accepts(
        "let renders = 0;\ncomponent Page() {\n  const onClick = () => { renders = renders + 1; };\n  return onClick;\n}\n",
    );
}

#[test]
fn writing_to_a_module_binding_outside_a_component_is_accepted() {
    accepts("let renders = 0;\nexport function bump() {\n  renders = renders + 1;\n}\n");
}

#[test]
fn writing_to_a_local_binding_during_render_is_accepted() {
    accepts("component Page() {\n  let total = 0;\n  total = total + 1;\n  return total;\n}\n");
}

#[test]
fn logging_during_render_is_rejected() {
    let diagnostics = check("component Page() {\n  console.log(\"render\");\n  return null;\n}\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::ConsoleDuringRender);
}

#[test]
fn logging_from_an_effect_is_accepted() {
    accepts(
        "component Page() {\n  useEffect(() => {\n    console.log(\"mounted\");\n  });\n  return null;\n}\n",
    );
}

#[test]
fn logging_outside_a_component_is_accepted() {
    accepts("console.log(\"boot\");\nexport const x = 1;\n");
}

#[test]
fn reading_the_dom_during_render_is_rejected() {
    let diagnostics =
        check("component Page() {\n  const title = document.title;\n  return title;\n}\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::DomAccessDuringRender);
    assert_eq!(diagnostics[0].symbol.as_deref(), Some("document"));
}

#[test]
fn reading_local_storage_during_render_is_rejected() {
    assert_eq!(
        findings(
            "component Page() {\n  const seen = localStorage.getItem(\"seen\");\n  return seen;\n}\n"
        ),
        [Finding::DomAccessDuringRender]
    );
}

#[test]
fn reading_the_window_during_render_is_rejected() {
    assert_eq!(
        findings("component Page() {\n  return window.innerWidth;\n}\n"),
        [Finding::DomAccessDuringRender]
    );
}

#[test]
fn a_typeof_window_guard_is_accepted() {
    accepts(
        "component Page() {\n  const client = typeof window !== \"undefined\";\n  return client;\n}\n",
    );
}

#[test]
fn reading_the_dom_from_an_effect_is_accepted() {
    accepts(
        "component Page() {\n  useEffect(() => {\n    document.title = \"uf\";\n  });\n  return null;\n}\n",
    );
}

#[test]
fn a_local_binding_named_like_a_browser_global_is_accepted() {
    accepts("component Page(document: Doc) {\n  return document.title;\n}\n");
}

#[test]
fn reading_the_clock_during_render_is_rejected() {
    let diagnostics = check("component Clock() {\n  return <p>{Date.now()}</p>;\n}\n");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::UnstableReadDuringRender);
    assert_eq!(diagnostics[0].rule(), "react/no-render-side-effects");
}

#[test]
fn reading_randomness_during_render_is_rejected() {
    assert_eq!(
        findings("component Page() {\n  const id = Math.random();\n  return id;\n}\n"),
        [Finding::UnstableReadDuringRender]
    );
}

#[test]
fn other_date_and_math_members_are_accepted() {
    accepts(
        "component Page(at: number) {\n  const rounded = Math.round(at);\n  const day = Date.parse(\"2026-01-01\");\n  return rounded + day;\n}\n",
    );
}

#[test]
fn a_custom_hook_body_is_render_position() {
    assert_eq!(
        findings("hook useNow(): number {\n  return Date.now();\n}\n"),
        [Finding::UnstableReadDuringRender]
    );
}

#[test]
fn a_use_prefixed_function_body_is_render_position() {
    assert_eq!(
        findings("function useNow(): number {\n  return Date.now();\n}\n"),
        [Finding::UnstableReadDuringRender]
    );
}

#[test]
fn a_plain_helper_body_is_not_render_position() {
    accepts("function stamp(): number {\n  return Date.now();\n}\n");
}

#[test]
fn several_side_effects_are_all_reported_in_position_order() {
    let diagnostics = check(
        "let renders = 0;\ncomponent Page() {\n  console.log(\"a\");\n  renders = 1;\n  return Math.random();\n}\n",
    );
    assert_eq!(
        diagnostics
            .iter()
            .map(|entry| (entry.line, entry.finding))
            .collect::<Vec<_>>(),
        [
            (3, Finding::ConsoleDuringRender),
            (4, Finding::ModuleBindingAssigned),
            (5, Finding::UnstableReadDuringRender),
        ]
    );
}
