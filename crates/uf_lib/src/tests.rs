use super::*;

#[test]
fn exposes_test_api_from_root_uf_module() {
    let root = module_by_specifier("@uniflowed/core").expect("root module");

    assert_eq!(root.kind, NativeModuleKind::Runtime);
    assert!(root.flow_exports.iter().any(|export| export == "describe"));
    assert!(root.flow_exports.iter().any(|export| export == "it"));
}

#[test]
fn includes_react_flow_app_builtins() {
    let modules = builtin_modules();
    let specs = modules
        .iter()
        .map(|module| module.specifier.as_str())
        .collect::<Vec<_>>();

    assert!(specs.contains(&"@uniflowed/router"));
    assert!(specs.contains(&"@uniflowed/react"));
    assert!(specs.contains(&"@uniflowed/react-native"));
    assert!(specs.contains(&"@uniflowed/brand"));
    assert!(specs.contains(&"@uniflowed/testing"));
    assert!(specs.contains(&"@uniflowed/lib"));
    assert!(specs.contains(&"@uniflowed/lint"));
    assert!(specs.contains(&"@uniflowed/server"));
    assert!(specs.contains(&"@uniflowed/hooks"));
    assert!(specs.contains(&"@uniflowed/query"));
    assert!(specs.contains(&"@uniflowed/fetch"));
    assert!(specs.contains(&"@uniflowed/loader"));
    assert!(specs.contains(&"@uniflowed/effect"));
    assert!(specs.contains(&"@uniflowed/relay"));
    assert!(specs.contains(&"@uniflowed/graphql"));
    assert!(specs.contains(&"@uniflowed/web"));
    assert!(specs.contains(&"@uniflowed/markdown"));
    assert!(specs.contains(&"@uniflowed/temporal"));
    assert!(specs.contains(&"@uniflowed/pwa"));
    assert!(specs.contains(&"@uniflowed/prepare"));
    assert!(specs.contains(&"@uniflowed/stylex"));
    assert!(specs.contains(&"@uniflowed/ui"));
    assert!(specs.contains(&"@uniflowed/react-compiler"));
    assert!(specs.contains(&"@uniflowed/cell"));
    assert!(specs.contains(&"@uniflowed/state"));
    assert!(specs.contains(&"@uniflowed/validator"));
    assert!(specs.contains(&"@uniflowed/mock"));
    assert!(specs.contains(&"@uniflowed/browser"));
    assert!(specs.contains(&"@uniflowed/story"));
    assert!(specs.contains(&"@uniflowed/vrt"));
    assert!(specs.contains(&"@uniflowed/motion"));
    assert!(specs.contains(&"@uniflowed/tui"));
    assert!(specs.contains(&"@uniflowed/cli"));
}

#[test]
fn tui_contract_targets_opentui_and_react_ink_replacement() {
    let module = module_by_specifier("@uniflowed/tui").expect("tui module");
    let contract = tui_contract();

    assert_eq!(module.kind, NativeModuleKind::Ui);
    assert!(
        module
            .flow_exports
            .iter()
            .any(|export| export == "renderTui")
    );
    assert!(contract.react_ink_target.replacement_ready);
    assert!(contract.has_component("FrameBuffer"));
    assert!(contract.has_component("EmbeddedTerminal"));
}

#[test]
fn ui_registry_uses_compound_parts_for_complex_components() {
    let dialog = ui_components()
        .into_iter()
        .find(|component| component.name == "Dialog")
        .expect("Dialog");

    assert_eq!(dialog.runtime, UiRuntime::Split);
    assert!(dialog.preset_style);
    assert!(dialog.has_part("Body"));
    assert!(dialog.has_part("Trigger"));
}

#[test]
fn ui_registry_covers_shadcn_style_catalog() {
    let components = ui_components();
    let names = components
        .iter()
        .map(|component| component.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&"Accordion"));
    assert!(names.contains(&"Command"));
    assert!(names.contains(&"DataTable"));
    assert!(names.contains(&"Sheet"));
    assert!(names.contains(&"Tooltip"));
    assert!(components.len() >= 40);
}

#[test]
fn form_registry_is_validator_backed_and_react_compiler_safe() {
    let form = ui_components()
        .into_iter()
        .find(|component| component.name == "Form")
        .expect("Form");
    let contract = form.form.expect("form contract");

    assert_eq!(form.runtime, UiRuntime::Split);
    assert_eq!(contract.validator_module, "@uniflowed/validator");
    assert_eq!(contract.schema_kind, SchemaKind::Object);
    assert!(contract.compiler_safe);
    assert!(contract.render_idempotent);
    assert_eq!(
        contract.mutation_phase,
        FormMutationPhase::EventOrServerAction
    );
}

#[test]
fn hooks_registry_prefers_react_idempotency() {
    let hooks = hook_descriptors();

    assert!(hooks.iter().all(|hook| hook.idempotent_render));
    assert!(hooks.iter().any(|hook| hook.name == "useStableCallback"));
    assert!(hooks.iter().any(|hook| hook.server_component_safe));
}
