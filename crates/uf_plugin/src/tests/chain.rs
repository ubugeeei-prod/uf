//! Chained dispatch, `apply` filtering, broadcast, and failure mid-chain.

use uf_config::{ApplyCondition, HookOrder, PipelineMode};

use crate::container::{ContainerError, PluginContainer};
use crate::hook::{HookSet, PluginHook};
use crate::outcome::{HookFailure, HookOutcome, ModuleCode};
use crate::plugin::{FnPlugin, Plugin};

use super::{appender, descriptor, failing, passthrough, resolver};

fn build(plugins: Vec<Box<dyn Plugin>>) -> PluginContainer {
    PluginContainer::build(PipelineMode::Build, plugins).expect("container")
}

#[test]
fn transform_runs_every_plugin_in_order() {
    let container = build(vec![
        appender("post", HookOrder::Post, "3"),
        appender("normal", HookOrder::Normal, "2"),
        appender("pre", HookOrder::Pre, "1"),
    ]);

    assert_eq!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .handled()
            .expect("handled")
            .code,
        "src123"
    );
}

#[test]
fn transform_feeds_each_plugin_what_the_previous_one_produced() {
    let container = build(vec![
        appender("a", HookOrder::Pre, "-a"),
        Box::new(
            FnPlugin::new(descriptor("assert", HookOrder::Normal, HookSet::EMPTY)).on_transform(
                |input| {
                    assert_eq!(input.code, "src-a");
                    assert_eq!(input.id, "a.js");
                    Ok(HookOutcome::Handled(ModuleCode::new("final")))
                },
            ),
        ),
    ]);

    assert_eq!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .handled()
            .expect("handled")
            .code,
        "final"
    );
}

#[test]
fn transform_passes_through_when_no_plugin_touches_the_module() {
    let container = build(vec![
        passthrough("a", HookOrder::Pre),
        passthrough("b", HookOrder::Normal),
        passthrough("c", HookOrder::Post),
    ]);

    assert!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .is_passthrough(),
        "a module nobody rewrites must not be copied"
    );
}

#[test]
fn transform_passes_through_when_no_plugin_implements_it() {
    let container = build(vec![resolver("a", HookOrder::Normal, "x")]);

    assert!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .is_passthrough()
    );
}

#[test]
fn a_declining_plugin_in_the_middle_does_not_break_the_chain() {
    let container = build(vec![
        appender("a", HookOrder::Pre, "1"),
        passthrough("skip", HookOrder::Normal),
        appender("b", HookOrder::Post, "2"),
    ]);

    assert_eq!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .handled()
            .expect("handled")
            .code,
        "src12"
    );
}

#[test]
fn the_surviving_source_map_is_the_last_one_produced() {
    let container = build(vec![
        Box::new(
            FnPlugin::new(descriptor("first", HookOrder::Pre, HookSet::EMPTY)).on_transform(|_| {
                Ok(HookOutcome::Handled(
                    ModuleCode::new("one").with_source_map("first-map"),
                ))
            }),
        ),
        Box::new(
            FnPlugin::new(descriptor("second", HookOrder::Post, HookSet::EMPTY)).on_transform(
                |_| {
                    Ok(HookOutcome::Handled(
                        ModuleCode::new("two").with_source_map("second-map"),
                    ))
                },
            ),
        ),
    ]);

    assert_eq!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .handled()
            .expect("handled")
            .source_map
            .as_deref(),
        Some("second-map")
    );
}

#[test]
fn a_transform_without_a_map_drops_the_stale_one() {
    let container = build(vec![
        Box::new(
            FnPlugin::new(descriptor("mapped", HookOrder::Pre, HookSet::EMPTY)).on_transform(
                |_| {
                    Ok(HookOutcome::Handled(
                        ModuleCode::new("one").with_source_map("stale"),
                    ))
                },
            ),
        ),
        appender("unmapped", HookOrder::Post, "!"),
    ]);

    assert_eq!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .handled()
            .expect("handled")
            .source_map,
        None,
        "a map pointing at text that no longer exists is worse than none"
    );
}

#[test]
fn a_plugin_that_fails_mid_chain_stops_the_pipeline() {
    let container = build(vec![
        appender("first", HookOrder::Pre, "1"),
        failing(
            "broken",
            HookOrder::Normal,
            HookFailure::UnsupportedSyntax { offset: 12 },
        ),
        Box::new(
            FnPlugin::new(descriptor("never", HookOrder::Post, HookSet::EMPTY))
                .on_transform(|_| panic!("the chain must stop at the failure")),
        ),
    ]);

    let error = container
        .transform("app/page.js", "src")
        .expect_err("the failure propagates");

    assert_eq!(
        error,
        ContainerError::Hook {
            plugin: "broken".into(),
            hook: PluginHook::Transform,
            module: "app/page.js".into(),
            source: HookFailure::UnsupportedSyntax { offset: 12 },
        }
    );
}

#[test]
fn a_hook_error_names_the_plugin_the_hook_and_the_module() {
    let container = build(vec![failing(
        "broken",
        HookOrder::Normal,
        HookFailure::Rejected {
            rule: "server/no-client-secret",
        },
    )]);

    let error = container
        .transform("app/page.js", "src")
        .expect_err("the failure propagates");

    assert_eq!(
        error.to_string(),
        "plugin broken failed the transform hook for app/page.js"
    );
    assert_eq!(
        std::error::Error::source(&error).map(ToString::to_string),
        Some("rejected by rule server/no-client-secret".to_string())
    );
}

#[test]
fn a_resolve_failure_names_the_specifier_it_was_handed() {
    let container = build(vec![Box::new(
        FnPlugin::new(descriptor("broken", HookOrder::Normal, HookSet::EMPTY))
            .on_resolve_id(|_| Err(HookFailure::InputTooLarge { bytes: 9, limit: 4 })),
    )]);

    let error = container
        .resolve_id("@scope/pkg", None)
        .expect_err("the failure propagates");

    assert_eq!(
        error,
        ContainerError::Hook {
            plugin: "broken".into(),
            hook: PluginHook::ResolveId,
            module: "@scope/pkg".into(),
            source: HookFailure::InputTooLarge { bytes: 9, limit: 4 },
        }
    );
}

#[test]
fn a_load_failure_propagates() {
    let container = build(vec![Box::new(
        FnPlugin::new(descriptor("broken", HookOrder::Normal, HookSet::EMPTY)).on_load(|_| {
            Err(HookFailure::MissingPrerequisite {
                required: PluginHook::ResolveId,
                hook: PluginHook::Load,
            })
        }),
    )]);

    let error = container.load("x").expect_err("the failure propagates");

    assert!(matches!(
        error,
        ContainerError::Hook {
            hook: PluginHook::Load,
            ..
        }
    ));
}

#[test]
fn hook_failures_render_their_structured_fields() {
    assert_eq!(
        HookFailure::UnsupportedSyntax { offset: 3 }.to_string(),
        "unsupported syntax at byte offset 3"
    );
    assert_eq!(
        HookFailure::MissingPrerequisite {
            required: PluginHook::Load,
            hook: PluginHook::Transform,
        }
        .to_string(),
        "the load hook must run before transform"
    );
    assert_eq!(
        HookFailure::InputTooLarge {
            bytes: 10,
            limit: 4
        }
        .to_string(),
        "input is 10 bytes, over the 4 byte ceiling"
    );
}

#[test]
fn a_build_only_plugin_never_runs_while_serving() {
    let plugin = Box::new(
        FnPlugin::new(
            descriptor("build-only", HookOrder::Normal, HookSet::EMPTY)
                .with_apply(ApplyCondition::Build),
        )
        .on_transform(|_| panic!("a build-only plugin must not run while serving")),
    ) as Box<dyn Plugin>;
    let container = PluginContainer::build(PipelineMode::Serve, vec![plugin]).expect("container");

    assert!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .is_passthrough()
    );
}

#[test]
fn a_serve_only_plugin_never_runs_in_a_build() {
    let plugin = Box::new(
        FnPlugin::new(
            descriptor("serve-only", HookOrder::Normal, HookSet::EMPTY)
                .with_apply(ApplyCondition::Serve),
        )
        .on_transform(|_| panic!("a serve-only plugin must not run in a build")),
    ) as Box<dyn Plugin>;
    let container = build(vec![plugin]);

    assert!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .is_passthrough()
    );
}

#[test]
fn apply_filtering_leaves_the_rest_of_the_chain_intact() {
    let filtered = Box::new(
        FnPlugin::new(
            descriptor("serve-only", HookOrder::Pre, HookSet::EMPTY)
                .with_apply(ApplyCondition::Serve),
        )
        .on_transform(|_| Ok(HookOutcome::Handled(ModuleCode::new("wrong")))),
    ) as Box<dyn Plugin>;
    let container = build(vec![filtered, appender("kept", HookOrder::Normal, "!")]);

    assert_eq!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .handled()
            .expect("handled")
            .code,
        "src!"
    );
}

#[test]
fn notify_reaches_only_the_plugins_that_declared_the_hook() {
    static STARTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let container = PluginContainer::build(
        PipelineMode::Build,
        vec![
            Box::new(
                FnPlugin::new(super::declaring("a", PluginHook::BuildStart)).on_notify(|hook| {
                    assert_eq!(hook, PluginHook::BuildStart);
                    STARTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Ok(())
                }),
            ),
            Box::new(
                FnPlugin::new(super::declaring("b", PluginHook::BuildEnd)).on_notify(|_| {
                    panic!("a plugin that declared BuildEnd must not see BuildStart")
                }),
            ),
        ],
    )
    .expect("container");

    container.notify(PluginHook::BuildStart).expect("notifies");

    assert_eq!(STARTS.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(
        container
            .plugins_for(PluginHook::BuildStart)
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a"]
    );
}

#[test]
fn notify_stops_at_the_first_failure() {
    let container = PluginContainer::build(
        PipelineMode::Build,
        vec![
            Box::new(
                FnPlugin::new(super::declaring("broken", PluginHook::BuildStart))
                    .on_notify(|_| Err(HookFailure::Rejected { rule: "uf/example" })),
            ),
            Box::new(
                FnPlugin::new(super::declaring("never", PluginHook::BuildStart))
                    .on_notify(|_| panic!("notification must stop at the failure")),
            ),
        ],
    )
    .expect("container");

    let error = container
        .notify(PluginHook::BuildStart)
        .expect_err("the failure propagates");

    assert!(matches!(
        error,
        ContainerError::Hook {
            hook: PluginHook::BuildStart,
            ..
        }
    ));
}

#[test]
fn a_hook_a_plugin_did_not_declare_is_never_called() {
    let plugin = Box::new(
        FnPlugin::new(descriptor("a", HookOrder::Normal, HookSet::EMPTY))
            .on_transform(|_| Ok(HookOutcome::Handled(ModuleCode::new("x")))),
    ) as Box<dyn Plugin>;
    let container = build(vec![plugin]);

    // `on_transform` declared `Transform` and nothing else, so `Load` has no lane.
    assert!(container.implements(PluginHook::Transform));
    assert!(!container.implements(PluginHook::Load));
    assert!(container.load("a.js").expect("loads").is_passthrough());
}

#[test]
fn a_declared_hook_with_nothing_wired_passes_through() {
    let container = PluginContainer::build(
        PipelineMode::Build,
        vec![Box::new(FnPlugin::new(descriptor(
            "inert",
            HookOrder::Normal,
            HookSet::of(PluginHook::Transform),
        )))],
    )
    .expect("container");

    assert!(container.implements(PluginHook::Transform));
    assert!(
        container
            .transform("a.js", "src")
            .expect("transforms")
            .is_passthrough()
    );
}
