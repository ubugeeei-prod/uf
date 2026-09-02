//! uf's own pipeline stages.

use uf_config::{HookOrder, PipelineMode, StyleEngine, UniflowedConfig};

use crate::builtin::{BuiltinPlugin, BuiltinSet};
use crate::container::PluginContainer;
use crate::descriptor::{PluginOrigin, PluginSource};
use crate::hook::{HookSet, PluginHook};
use crate::resolve::BUILTIN_PREFIX;

fn pipeline(config: &UniflowedConfig) -> PluginContainer {
    PluginContainer::from_descriptors(
        PipelineMode::Build,
        BuiltinSet::from_config(config).descriptors(),
    )
    .expect("uf's own pipeline always resolves")
}

#[test]
fn all_holds_every_builtin_exactly_once() {
    let mut seen = BuiltinSet::EMPTY;
    for plugin in BuiltinPlugin::ALL {
        assert!(!seen.contains(plugin), "{plugin:?} appears twice");
        seen = seen.with(plugin);
    }
    assert_eq!(seen, BuiltinSet::ALL);
    assert_eq!(BuiltinPlugin::ALL.len(), BuiltinPlugin::COUNT);
}

#[test]
fn every_builtin_has_a_unique_index_and_bit() {
    let mut mask = 0u8;
    for (position, plugin) in BuiltinPlugin::ALL.into_iter().enumerate() {
        assert_eq!(plugin.index(), position, "{plugin:?}");
        assert_eq!(plugin.bit().count_ones(), 1, "{plugin:?}");
        assert_eq!(mask & plugin.bit(), 0, "{plugin:?} reuses a bit");
        mask |= plugin.bit();
    }
    assert_eq!(mask, BuiltinSet::ALL.bits());
}

#[test]
fn every_builtin_has_a_unique_reserved_name() {
    let mut names = BuiltinPlugin::ALL.map(BuiltinPlugin::name).to_vec();
    for name in &names {
        assert!(name.starts_with(BUILTIN_PREFIX), "{name}");
        assert!(name.len() > BUILTIN_PREFIX.len(), "{name}");
    }
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "two built-ins share a name");
}

#[test]
fn every_builtin_implements_at_least_one_hook() {
    for plugin in BuiltinPlugin::ALL {
        assert!(!plugin.hooks().is_empty(), "{plugin:?}");
    }
}

#[test]
fn flow_stripping_and_route_generation_run_first() {
    assert_eq!(BuiltinPlugin::Flow.order(), HookOrder::Pre);
    assert_eq!(BuiltinPlugin::Router.order(), HookOrder::Pre);
}

#[test]
fn the_react_compiler_runs_last() {
    assert_eq!(BuiltinPlugin::ReactCompiler.order(), HookOrder::Post);

    let names = pipeline(&UniflowedConfig::default())
        .names()
        .map(str::to_string)
        .collect::<Vec<_>>();

    assert_eq!(
        names.last().map(String::as_str),
        Some(BuiltinPlugin::ReactCompiler.name())
    );
}

#[test]
fn flow_stripping_runs_before_every_other_transform() {
    let container = pipeline(&UniflowedConfig::default());
    let transformers = container
        .plugins_for(PluginHook::Transform)
        .map(|plugin| plugin.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(transformers.first(), Some(&BuiltinPlugin::Flow.name()));
    assert!(transformers.len() > 1);
}

#[test]
fn a_builtin_descriptor_is_marked_builtin() {
    for plugin in BuiltinPlugin::ALL {
        let descriptor = plugin.descriptor();
        assert_eq!(descriptor.origin, PluginOrigin::Builtin);
        assert_eq!(descriptor.source, PluginSource::Builtin);
        assert_eq!(descriptor.name, plugin.name());
        assert_eq!(descriptor.hooks, plugin.hooks());
    }
}

#[test]
fn a_default_project_switches_on_every_builtin() {
    let set = BuiltinSet::from_config(&UniflowedConfig::default());

    assert_eq!(set, BuiltinSet::ALL);
    assert_eq!(set.len() as usize, BuiltinPlugin::COUNT);
}

#[test]
fn turning_off_the_router_drops_its_plugin() {
    let mut config = UniflowedConfig::default();
    config.app.router.enabled = false;

    let set = BuiltinSet::from_config(&config);

    assert!(!set.contains(BuiltinPlugin::Router));
    assert_eq!(set, BuiltinSet::ALL.without(BuiltinPlugin::Router));
}

#[test]
fn turning_off_rsc_drops_its_plugin() {
    let mut config = UniflowedConfig::default();
    config.app.rsc = false;

    assert!(!BuiltinSet::from_config(&config).contains(BuiltinPlugin::Rsc));
}

#[test]
fn turning_off_the_react_compiler_drops_its_plugin() {
    let mut config = UniflowedConfig::default();
    config.app.builtins.react_compiler.enabled = false;

    let set = BuiltinSet::from_config(&config);

    assert!(!set.contains(BuiltinPlugin::ReactCompiler));
    assert!(set.contains(BuiltinPlugin::Flow), "the rest stay");
}

#[test]
fn the_style_engine_selects_the_style_plugin() {
    let mut config = UniflowedConfig::default();
    config.app.builtins.style = StyleEngine::StyleX;

    assert!(BuiltinSet::from_config(&config).contains(BuiltinPlugin::Style));
}

#[test]
fn flow_and_asset_handling_cannot_be_switched_off() {
    let mut config = UniflowedConfig::default();
    config.app.router.enabled = false;
    config.app.rsc = false;
    config.app.builtins.react_compiler.enabled = false;

    let set = BuiltinSet::from_config(&config);

    assert!(set.contains(BuiltinPlugin::Flow));
    assert!(set.contains(BuiltinPlugin::Asset));
}

#[test]
fn an_empty_set_produces_no_descriptors() {
    assert!(BuiltinSet::EMPTY.is_empty());
    assert_eq!(BuiltinSet::EMPTY.len(), 0);
    assert!(BuiltinSet::EMPTY.descriptors().is_empty());
}

#[test]
fn a_set_iterates_in_pipeline_order() {
    let set = BuiltinSet::of(BuiltinPlugin::Asset).with(BuiltinPlugin::Flow);

    assert_eq!(
        set.iter().collect::<Vec<_>>(),
        vec![BuiltinPlugin::Flow, BuiltinPlugin::Asset]
    );
}

#[test]
fn with_if_only_adds_when_asked() {
    let set = BuiltinSet::EMPTY
        .with_if(BuiltinPlugin::Flow, true)
        .with_if(BuiltinPlugin::Rsc, false);

    assert_eq!(set, BuiltinSet::of(BuiltinPlugin::Flow));
}

#[test]
fn collecting_builtins_deduplicates_them() {
    let set: BuiltinSet = [BuiltinPlugin::Flow, BuiltinPlugin::Flow, BuiltinPlugin::Rsc]
        .into_iter()
        .collect();

    assert_eq!(set.len(), 2);
}

#[test]
fn a_set_serializes_as_plugin_names() {
    let set = BuiltinSet::of(BuiltinPlugin::Flow).with(BuiltinPlugin::Rsc);

    assert_eq!(
        serde_json::to_value(set).expect("serializes"),
        serde_json::json!(["uf:flow", "uf:rsc"])
    );
}

#[test]
fn uf_s_own_pipeline_has_no_duplicate_names() {
    let container = pipeline(&UniflowedConfig::default());

    assert_eq!(container.len(), BuiltinPlugin::COUNT);
}

#[test]
fn every_builtin_takes_part_in_both_pipelines() {
    for mode in PipelineMode::ALL {
        let container = PluginContainer::from_descriptors(mode, BuiltinSet::ALL.descriptors())
            .expect("container");

        assert!(
            container
                .report()
                .plugins
                .iter()
                .all(|plugin| plugin.active),
            "{mode:?}"
        );
    }
}

#[test]
fn the_pipeline_covers_the_hooks_a_build_needs() {
    let container = pipeline(&UniflowedConfig::default());

    for hook in [
        PluginHook::ResolveId,
        PluginHook::Load,
        PluginHook::Transform,
        PluginHook::GenerateBundle,
        PluginHook::WriteBundle,
    ] {
        assert!(container.implements(hook), "{hook}");
    }
    assert!(
        container.active_hooks().intersection(HookSet::ALL) == container.active_hooks(),
        "every active hook is a real hook"
    );
}
