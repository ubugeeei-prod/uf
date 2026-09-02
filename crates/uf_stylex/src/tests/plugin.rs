//! The `uf:style` stage inside a container.

use uf_plugin::{
    BuiltinPlugin, HookFailure, PipelineMode, PluginContainer, PluginHook, PluginOrigin,
};

use super::module;
use crate::plugin::{FORBIDDEN_KEY_RULE, SheetSink, UNSAFE_VALUE_RULE, plugin};

/// A container holding only the style stage, and its sheet sink.
fn container() -> (PluginContainer, SheetSink) {
    let (style, sheet) = plugin();
    let container = PluginContainer::build(PipelineMode::Build, vec![Box::new(style)])
        .expect("one plugin resolves");
    (container, sheet)
}

#[test]
fn the_plugin_carries_the_builtin_descriptor() {
    let (container, _sheet) = container();
    let descriptor = &container.descriptors()[0];
    assert_eq!(descriptor.name, BuiltinPlugin::Style.name());
    assert_eq!(descriptor.origin, PluginOrigin::Builtin);
    assert_eq!(descriptor.order, BuiltinPlugin::Style.order());
}

#[test]
fn the_plugin_declares_the_hooks_the_descriptor_does() {
    let (container, _sheet) = container();
    for hook in BuiltinPlugin::Style.hooks().iter() {
        assert!(container.implements(hook), "{hook:?} is missing");
    }
    assert!(container.implements(PluginHook::Transform));
}

#[test]
fn a_module_with_styles_is_rewritten() {
    let (container, _sheet) = container();
    let outcome = container
        .transform(
            "app/page.js",
            &module("const s = stylex.create({ a: { color: \"red\" } });\n"),
        )
        .expect("a module that compiles");
    let code = outcome.handled().expect("the module was rewritten").code;
    assert!(!code.contains("stylex.create"));
}

#[test]
fn a_module_without_styles_passes_through_untouched() {
    let (container, sheet) = container();
    let outcome = container
        .transform("app/page.js", "// @flow\nexport const x = 1;\n")
        .expect("a module with no styles");
    assert!(outcome.is_passthrough());
    assert!(sheet.drain().is_empty());
}

#[test]
fn every_module_folds_into_one_sheet() {
    let (container, sheet) = container();
    container
        .transform(
            "app/a.js",
            &module("const s = stylex.create({ a: { color: \"red\" } });\n"),
        )
        .expect("a module that compiles");
    container
        .transform(
            "app/b.js",
            &module("const s = stylex.create({ b: { marginTop: 8 } });\n"),
        )
        .expect("a module that compiles");
    assert_eq!(sheet.drain().len(), 2);
}

#[test]
fn the_folded_sheet_does_not_depend_on_module_order() {
    let first = module("const s = stylex.create({ a: { marginTop: 8 } });\n");
    let second = module("const s = stylex.create({ b: { margin: 0 } });\n");

    let (forwards, forwards_sheet) = container();
    forwards.transform("app/a.js", &first).expect("compiles");
    forwards.transform("app/b.js", &second).expect("compiles");

    let (backwards, backwards_sheet) = container();
    backwards.transform("app/b.js", &second).expect("compiles");
    backwards.transform("app/a.js", &first).expect("compiles");

    similar_asserts::assert_eq!(
        forwards_sheet.drain().to_css(),
        backwards_sheet.drain().to_css()
    );
}

#[test]
fn draining_twice_does_not_repeat_a_rule() {
    let (container, sheet) = container();
    container
        .transform(
            "app/a.js",
            &module("const s = stylex.create({ a: { color: \"red\" } });\n"),
        )
        .expect("a module that compiles");
    assert_eq!(sheet.drain().len(), 1);
    assert!(sheet.drain().is_empty());
}

#[test]
fn a_value_that_escapes_its_rule_refuses_the_module() {
    let (container, _sheet) = container();
    let error = container
        .transform(
            "app/evil.js",
            &module(
                "const s = stylex.create({ a: { color: \"red} .victim { display: none\" } });\n",
            ),
        )
        .expect_err("a hostile value");
    let failure: &HookFailure = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref())
        .expect("a typed hook failure");
    assert_eq!(
        *failure,
        HookFailure::Rejected {
            rule: UNSAFE_VALUE_RULE
        }
    );
}

#[test]
fn a_prototype_key_refuses_the_module() {
    let (container, _sheet) = container();
    let error = container
        .transform(
            "app/evil.js",
            &module("const s = stylex.create({ __proto__: { color: \"red\" } });\n"),
        )
        .expect_err("a prototype key");
    let failure: &HookFailure = std::error::Error::source(&error)
        .and_then(|source| source.downcast_ref())
        .expect("a typed hook failure");
    assert_eq!(
        *failure,
        HookFailure::Rejected {
            rule: FORBIDDEN_KEY_RULE
        }
    );
}

#[test]
fn a_module_over_the_size_ceiling_is_reported_as_too_large() {
    let (container, _sheet) = container();
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
fn running_a_module_through_twice_changes_it_once() {
    let (container, _sheet) = container();
    let source = module("const s = stylex.create({ a: { color: \"red\" } });\n");
    let once = container
        .transform("app/page.js", &source)
        .expect("a module that compiles")
        .handled()
        .expect("the module was rewritten")
        .code;
    let twice = container
        .transform("app/page.js", &once)
        .expect("the rewritten module");
    assert!(twice.is_passthrough());
}

#[test]
fn the_stage_runs_before_the_react_compiler() {
    let order = [BuiltinPlugin::Style, BuiltinPlugin::ReactCompiler].map(|builtin| builtin.order());
    assert!(
        order[0] < order[1],
        "the compiler must be the last pass over component code"
    );
}
