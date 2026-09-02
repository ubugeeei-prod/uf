//! The `uf:jsx` stage as the container sees it.

use uf_config::{PipelineMode, UniflowedConfig};
use uf_plugin::{
    BuiltinPlugin, BuiltinSet, HookFailure, HookOrder, Plugin, PluginContainer, PluginHook,
    TransformInput,
};

use crate::plugin::{CLASSIC_RUNTIME_RULE, plugin};
use crate::{JsxOptions, ReactRuntime};

fn transform(source: &str, options: JsxOptions) -> Result<Option<String>, HookFailure> {
    let stage = plugin(options);
    let outcome = stage.transform(TransformInput {
        id: "app.js",
        code: source,
    })?;
    Ok(outcome.handled().map(|code| code.code))
}

#[test]
fn the_stage_lowers_a_module_with_jsx() {
    let code = transform("const a = <div />;\n", JsxOptions::default())
        .expect("transform runs")
        .expect("handled");

    assert!(code.contains("_jsx(\"div\", {"), "{code}");
    assert!(!code.contains("<div"), "{code}");
}

#[test]
fn the_stage_passes_a_module_without_jsx_through() {
    let outcome = transform("const a = 1;\n", JsxOptions::default()).expect("transform runs");

    assert_eq!(outcome, None);
}

#[test]
fn the_stage_refuses_a_project_that_needs_the_classic_runtime() {
    let options = JsxOptions {
        runtime: ReactRuntime::Classic,
        ..JsxOptions::default()
    };

    let failure = transform("const a = <div />;\n", options).expect_err("refused");

    assert_eq!(
        failure,
        HookFailure::Rejected {
            rule: CLASSIC_RUNTIME_RULE
        }
    );
}

#[test]
fn the_stage_declares_the_transform_hook() {
    let descriptor = plugin(JsxOptions::default()).descriptor().clone();

    assert_eq!(descriptor.name.as_str(), "uf:jsx");
    assert!(descriptor.implements(PluginHook::Transform));
}

#[test]
fn the_stage_is_in_the_default_pipeline() {
    let set = BuiltinSet::from_config(&UniflowedConfig::default());

    assert!(set.contains(BuiltinPlugin::Jsx));
}

#[test]
fn the_stage_runs_after_the_react_compiler() {
    let container =
        PluginContainer::from_descriptors(PipelineMode::Build, BuiltinSet::ALL.descriptors())
            .expect("a pipeline resolves");
    let order: Vec<&str> = container.names().collect();

    let compiler = order
        .iter()
        .position(|name| *name == "uf:react-compiler")
        .expect("the react compiler");
    let jsx = order
        .iter()
        .position(|name| *name == "uf:jsx")
        .expect("the jsx stage");

    assert!(compiler < jsx, "{order:?}");
}

#[test]
fn the_stage_runs_after_flow_stripping() {
    let container =
        PluginContainer::from_descriptors(PipelineMode::Build, BuiltinSet::ALL.descriptors())
            .expect("a pipeline resolves");
    let order: Vec<&str> = container.names().collect();

    assert!(
        order.iter().position(|name| *name == "uf:flow")
            < order.iter().position(|name| *name == "uf:jsx"),
        "{order:?}"
    );
}

#[test]
fn the_stage_is_in_the_post_band() {
    assert_eq!(BuiltinPlugin::Jsx.order(), HookOrder::Post);
}

#[test]
fn the_stage_has_a_stable_name_and_index() {
    assert_eq!(BuiltinPlugin::Jsx.name(), "uf:jsx");
    assert_eq!(
        BuiltinPlugin::ALL[BuiltinPlugin::Jsx.index()],
        BuiltinPlugin::Jsx
    );
}

#[test]
fn a_chain_of_flow_stripping_then_lowering_produces_javascript() {
    // What the pipeline really does to a component, in order.
    let source =
        "// @flow\ncomponent Card(title: string) renders Node {\n  return <h2>{title}</h2>;\n}\n";

    let stripped = uf_flow::strip_types(source).expect("strips").code;
    let lowered = crate::transform(&stripped, &JsxOptions::default())
        .expect("lowers")
        .code;

    assert_eq!(lowered.lines().count(), source.lines().count());
    assert!(lowered.contains("function"), "{lowered}");
    assert!(lowered.contains("_jsx(\"h2\", {children:"), "{lowered}");
    assert!(
        !uf_flow::scan::tokenize_jsx(&lowered)
            .iter()
            .any(|token| token.kind.is_jsx()),
        "{lowered}"
    );
}
