use super::*;

#[test]
fn defaults_to_opentui_compatible_native_contract() {
    let contract = contract();

    assert_eq!(contract.engine, TuiEngine::UfNativeOpenTuiCompatible);
    assert_eq!(contract.standard, TuiStandard::OpenTui);
    assert_eq!(contract.renderer, TuiRenderer::CellDiffNative);
    assert_eq!(contract.layout, TuiLayoutEngine::FlexboxYogaCompatible);
    assert!(contract.supports(TuiFeature::Flexbox));
    assert!(contract.supports(TuiFeature::CellDiff));
    assert!(contract.supports(TuiFeature::InMemoryTesting));
}

#[test]
fn targets_react_ink_replacement_with_rich_features() {
    let target = contract().react_ink_target;

    assert!(target.replacement_ready);
    assert!(target.native_renderer);
    assert!(target.typed_components);
    assert!(target.rich_media);
    assert!(target.in_memory_tests);
    assert_eq!(
        target.performance_target,
        TuiPerformanceTarget::FasterThanReactInk
    );
}

#[test]
fn exposes_opentui_style_component_catalog() {
    let contract = contract();

    assert!(contract.has_component("Box"));
    assert!(contract.has_component("Text"));
    assert!(contract.has_component("Input"));
    assert!(contract.has_component("ScrollBox"));
    assert!(contract.has_component("FrameBuffer"));
    assert!(contract.has_component("EmbeddedTerminal"));
    assert!(contract.component("Select").unwrap().has_part("Item"));
    assert!(
        contract
            .component("TestRenderer")
            .unwrap()
            .server_component_safe
    );
    assert!(contract.component("Input").unwrap().interactive);
}
