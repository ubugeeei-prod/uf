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
fn annotating_a_declaration_with_an_imported_type_is_accepted() {
    // `const mode: Mode = …` ends in a name and an `=`, which is the shape of
    // a write to `Mode`. It is an annotation, and `Mode` is a type: there is
    // no value behind it to write to.
    accepts(
        "import type { Mode } from \"./mode.js\";\nhook useMode(given: ?Mode): Mode {\n  const mode: Mode = given ?? \"onSubmit\";\n  return mode;\n}\n",
    );
}

#[test]
fn annotating_a_declaration_with_an_imported_class_is_accepted() {
    // A class is a value and a type at once, so this one is module state and
    // still not written to.
    accepts(
        "import { Model } from \"./model.js\";\ncomponent Page() {\n  const model: Model = new Model();\n  return model.id;\n}\n",
    );
}

#[test]
fn a_type_named_after_a_dom_global_is_not_a_dom_read() {
    // The other half of not recording a type import as module state: it is
    // still a name the module declares, and `Selection` is a browser global.
    accepts(
        "import type { Selection } from \"./selection.js\";\ncomponent Page(chosen: Selection) {\n  const current: Selection = chosen;\n  return current.id;\n}\n",
    );
}

#[test]
fn an_inline_type_specifier_does_not_make_the_rest_of_the_clause_types() {
    // `import { type Mode, registry }` binds one type and one value, and the
    // value is still module state.
    assert_eq!(
        findings(
            "import { type Mode, registry } from \"./registry.js\";\ncomponent Page() {\n  const mode: Mode = \"onSubmit\";\n  registry.count = 1;\n  return mode;\n}\n"
        ),
        [Finding::ModuleBindingAssigned]
    );
}

#[test]
fn a_second_declarator_with_an_annotation_is_accepted() {
    accepts(
        "import type { Mode } from \"./mode.js\";\ncomponent Page() {\n  let count = 0, mode: Mode = \"onSubmit\";\n  return count + mode.length;\n}\n",
    );
}

#[test]
fn a_label_before_a_write_to_module_state_is_still_a_write() {
    // A `:` in front of the target is not always an annotation, and the scan
    // that steps over annotations must not step over this.
    assert_eq!(
        findings(
            "let renders = 0;\ncomponent Page() {\n  bump: renders = renders + 1;\n  return renders;\n}\n"
        ),
        [Finding::ModuleBindingAssigned]
    );
}

#[test]
fn a_jsx_attribute_named_after_an_import_is_not_a_write() {
    // `render={…}` in a tag reads backwards exactly like `render = …`, and
    // `render` is an imported name here. It is an attribute.
    accepts(
        "import { Controller, render } from \"./ui.js\";\ncomponent Page() {\n  return <Controller name=\"a\" control={null} render={({ field }) => <input {...field} />} />;\n}\n",
    );
}

#[test]
fn a_write_after_a_comparison_is_still_a_write() {
    // The scan that steps over a tag must not step over this: `<` here is a
    // comparison, written with a space after it, and `total` is module state.
    assert_eq!(
        findings(
            "let total = 0;\ncomponent Page(limit: number) {\n  if (total < limit) total = limit;\n  return total;\n}\n"
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
