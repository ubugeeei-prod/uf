use super::*;

#[test]
fn describes_a_flow_react_renderer_that_follows_opentui() {
    let contract = contract();

    assert_eq!(contract.engine, TuiEngine::FlowReactOpenTuiCompatible);
    assert_eq!(contract.standard, TuiStandard::OpenTui);
    assert_eq!(contract.renderer, TuiRenderer::CellDiff);
    assert_eq!(contract.layout, TuiLayoutEngine::FlexboxCells);
    assert_eq!(contract.input, TuiInputModel::KeyboardMouseSelectionFocus);
    assert_eq!(contract.runtime_binding, TuiRuntimeBinding::FlowReact);
}

#[test]
fn claims_only_the_features_the_package_implements() {
    let contract = contract();

    for feature in [
        TuiFeature::Flexbox,
        TuiFeature::CellDiff,
        TuiFeature::Keyboard,
        TuiFeature::Mouse,
        TuiFeature::Selection,
        TuiFeature::Clipboard,
        TuiFeature::Focus,
        TuiFeature::RichText,
        TuiFeature::Scrollback,
        TuiFeature::InMemoryTesting,
        TuiFeature::SnapshotTesting,
    ] {
        assert!(contract.supports(feature), "{feature:?} is implemented");
    }

    // The other half of the same assertion, and the half that matters: this
    // contract listed every one of these while the package behind it was a
    // function that threw. Removing one from this list is how a feature gets
    // announced, so announcing one requires deleting a line here.
    for feature in [
        TuiFeature::Keymap,
        TuiFeature::TerminalAutomation,
        TuiFeature::CodeHighlight,
        TuiFeature::Markdown,
        TuiFeature::Images,
        TuiFeature::Audio,
        TuiFeature::ThreeD,
        TuiFeature::Ssh,
        TuiFeature::QrCode,
        TuiFeature::EmbeddedTerminal,
        TuiFeature::Notifications,
        TuiFeature::Animations,
    ] {
        assert!(
            !contract.supports(feature),
            "{feature:?} is not implemented and must not be claimed"
        );
    }
}

#[test]
fn exposes_the_components_that_exist_and_no_others() {
    let contract = contract();

    assert_eq!(contract.components.len(), 7);
    for name in [
        "Box",
        "Text",
        "Input",
        "ScrollBox",
        "Select",
        "TabSelect",
        "Textarea",
    ] {
        assert!(contract.has_component(name), "{name} is exported");
    }

    // The rest of OpenTUI's components, which the package does not have.
    // Adding one to the list above means deleting it from this one.
    for name in [
        "Slider",
        "ScrollBar",
        "Code",
        "Markdown",
        "LineNumbers",
        "Diff",
        "TextTable",
        "AsciiFont",
        "FrameBuffer",
        "Image",
        "QrCode",
        "EmbeddedTerminal",
    ] {
        assert!(
            !contract.has_component(name),
            "{name} is not implemented and must not be listed"
        );
    }

    // Item selection is a keyboard component, and not the text selection
    // `TuiFeature::Selection` names.
    for name in ["Select", "TabSelect"] {
        let component = contract.component(name).unwrap();
        assert_eq!(component.kind, TuiComponentKind::Selection);
        assert_eq!(component.feature, TuiFeature::Keyboard);
        assert!(component.interactive);
    }
    let textarea = contract.component("Textarea").unwrap();
    assert_eq!(textarea.kind, TuiComponentKind::Input);
    assert!(!textarea.server_component_safe);

    // `Box` and `Text` describe a frame and can be produced anywhere; `Input`
    // needs somebody at a keyboard.
    assert!(contract.component("Box").unwrap().server_component_safe);
    assert!(contract.component("Input").unwrap().interactive);
    assert!(!contract.component("Input").unwrap().server_component_safe);
    assert!(contract.component("Box").unwrap().has_part("Root"));
}

#[test]
fn states_the_gap_to_react_ink_rather_than_the_ambition() {
    let target = contract().react_ink_target;

    // Three of these read `true` while nothing rendered at all. They become
    // true again by somebody implementing them, not by editing this.
    assert!(!target.replacement_ready);
    assert!(!target.native_renderer);
    assert!(!target.rich_media);

    assert!(target.typed_components);
    assert!(target.in_memory_tests);
    assert_eq!(
        target.performance_target,
        TuiPerformanceTarget::WritesOnlyChangedCells
    );
}

#[test]
fn serialises_the_contract_as_the_names_the_cli_prints() {
    let json = serde_json::to_value(contract()).expect("serialise");

    assert_eq!(json["engine"], "flow-react-open-tui-compatible");
    assert_eq!(json["renderer"], "cell-diff");
    assert_eq!(json["layout"], "flexbox-cells");
    assert_eq!(json["input"], "keyboard-mouse-selection-focus");
    assert_eq!(
        json["reactInkTarget"]["performanceTarget"],
        "writes-only-changed-cells"
    );
}
