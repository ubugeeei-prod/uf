//! The `uf:react-compiler` stage inside a container.

use uf_plugin::{
    BuiltinPlugin, HookFailure, PipelineMode, PluginContainer, PluginHook, PluginOrigin,
};

use crate::plugin::{FindingsSink, OnFinding, plugin};

/// A container holding only the React compiler stage, and its findings sink.
fn container(on_finding: OnFinding) -> (PluginContainer, FindingsSink) {
    let (compiler, sink) = plugin(on_finding);
    let container = PluginContainer::build(PipelineMode::Build, vec![Box::new(compiler)])
        .expect("one plugin resolves");
    (container, sink)
}

#[test]
fn the_plugin_carries_the_builtin_descriptor() {
    let (container, _sink) = container(OnFinding::Refuse);
    let descriptor = &container.descriptors()[0];
    assert_eq!(descriptor.name, BuiltinPlugin::ReactCompiler.name());
    assert_eq!(descriptor.origin, PluginOrigin::Builtin);
    assert_eq!(descriptor.order, BuiltinPlugin::ReactCompiler.order());
}

#[test]
fn the_plugin_declares_the_transform_hook() {
    let (container, _sink) = container(OnFinding::Refuse);
    assert!(container.implements(PluginHook::Transform));
}

#[test]
fn syntax_mode_never_rewrites_a_sound_module() {
    let (container, sink) = container(OnFinding::Refuse);
    let outcome = container
        .transform("app/page.js", "component Page() { return null; }\n")
        .expect("a sound module");
    assert!(outcome.is_passthrough());
    assert!(sink.drain().is_empty());
}

#[test]
fn a_module_that_does_not_validate_is_refused() {
    let (container, _sink) = container(OnFinding::Refuse);
    let error = container
        .transform(
            "app/page.js",
            "component Page(flag: boolean) { if (flag) { useState(0); } return null; }\n",
        )
        .expect_err("a module that does not validate");
    assert!(
        format!("{error}").contains("react-compiler"),
        "the container names the plugin: {error}"
    );
}

#[test]
fn the_refusal_names_the_rule_that_failed() {
    let (container, _sink) = container(OnFinding::Refuse);
    let error = container
        .transform("app/page.js", "component Page(t: string) { t = \"x\"; }\n")
        .expect_err("a module that mutates props");
    let source: &HookFailure = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref())
        .expect("a typed hook failure");
    assert_eq!(
        *source,
        HookFailure::Rejected {
            rule: "react/no-props-mutation"
        }
    );
}

#[test]
fn a_refused_module_still_sends_its_findings() {
    let (container, sink) = container(OnFinding::Refuse);
    let _ = container.transform("app/page.js", "component Page(t: string) { t = \"x\"; }\n");
    let findings = sink.drain();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].id, "app/page.js");
}

#[test]
fn reporting_mode_collects_findings_and_lets_the_module_through() {
    let (container, sink) = container(OnFinding::Report);
    let outcome = container
        .transform(
            "app/page.js",
            "component Page(flag: boolean) { if (flag) { useState(0); } return null; }\n",
        )
        .expect("reporting mode does not fail");
    assert!(outcome.is_passthrough());

    let findings = sink.drain();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].id, "app/page.js");
    assert_eq!(findings[0].diagnostics.len(), 1);
}

#[test]
fn draining_twice_does_not_repeat_a_module() {
    let (container, sink) = container(OnFinding::Report);
    container
        .transform("app/page.js", "component Page(t: string) { t = \"x\"; }\n")
        .expect("reporting mode does not fail");
    assert_eq!(sink.drain().len(), 1);
    assert!(sink.drain().is_empty());
}

#[test]
fn every_module_arrives_in_the_order_it_was_validated() {
    let (container, sink) = container(OnFinding::Report);
    for name in ["a", "b", "c"] {
        container
            .transform(
                &format!("app/{name}.js"),
                "component Page(t: string) { t = \"x\"; }\n",
            )
            .expect("reporting mode does not fail");
    }
    let ids: Vec<String> = sink
        .drain()
        .into_iter()
        .map(|module| module.id.to_string())
        .collect();
    assert_eq!(ids, ["app/a.js", "app/b.js", "app/c.js"]);
}

#[test]
fn a_module_over_the_size_ceiling_is_reported_as_too_large() {
    let (container, _sink) = container(OnFinding::Refuse);
    let source = "x".repeat(crate::MAX_SOURCE_BYTES + 1);
    let error = container
        .transform("app/huge.js", &source)
        .expect_err("an over-large module");
    let failure: &HookFailure = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref())
        .expect("a typed hook failure");
    assert!(matches!(failure, HookFailure::InputTooLarge { .. }));
}

#[test]
fn the_stage_runs_after_flow_stripping() {
    let order = [BuiltinPlugin::Flow, BuiltinPlugin::ReactCompiler].map(|builtin| builtin.order());
    assert!(
        order[0] < order[1],
        "the compiler must see plain JavaScript"
    );
}
