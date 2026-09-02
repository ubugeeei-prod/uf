//! Turning a project's `plugins: [...]` into a pipeline.

use camino::Utf8PathBuf;
use uf_config::{
    ApplyCondition, HookOrder, PipelineMode, PluginEntry, PluginSpec, UniflowedConfig,
};

use crate::builtin::BuiltinPlugin;
use crate::descriptor::PluginOrigin;
use crate::hook::HookSet;
use crate::resolve::{PluginPathError, ResolveError, resolve_pipeline, resolve_project_plugins};

const ROOT: &str = "/workspace/app";

fn config_with(entries: Vec<PluginEntry>) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.plugins = entries;
    config
}

#[test]
fn plugins_default_to_none() {
    assert!(UniflowedConfig::default().plugins.is_empty());
    assert!(
        resolve_project_plugins(
            &UniflowedConfig::default(),
            Utf8PathBuf::from(ROOT).as_path()
        )
        .expect("resolves")
        .is_empty()
    );
}

#[test]
fn resolving_keeps_declaration_order() {
    let config = config_with(vec![
        PluginEntry::Name("a".into()),
        PluginEntry::Name("b".into()),
        PluginEntry::Name("c".into()),
    ]);

    let resolved =
        resolve_project_plugins(&config, Utf8PathBuf::from(ROOT).as_path()).expect("resolves");

    assert_eq!(
        resolved
            .iter()
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[test]
fn a_resolved_entry_carries_its_declared_order_and_apply() {
    let config = config_with(vec![PluginEntry::Spec(
        PluginSpec::new("metrics")
            .with_order(HookOrder::Post)
            .with_apply(ApplyCondition::Build),
    )]);

    let resolved =
        resolve_project_plugins(&config, Utf8PathBuf::from(ROOT).as_path()).expect("resolves");

    assert_eq!(resolved[0].order, HookOrder::Post);
    assert_eq!(resolved[0].apply, ApplyCondition::Build);
    assert_eq!(resolved[0].origin, PluginOrigin::Project);
}

#[test]
fn a_resolved_entry_starts_with_no_hooks() {
    let config = config_with(vec![PluginEntry::Name("metrics".into())]);

    let resolved =
        resolve_project_plugins(&config, Utf8PathBuf::from(ROOT).as_path()).expect("resolves");

    assert_eq!(
        resolved[0].hooks,
        HookSet::EMPTY,
        "the resolver places plugins, it does not read them"
    );
}

#[test]
fn a_bad_entry_is_reported_with_its_index() {
    let config = config_with(vec![
        PluginEntry::Name("fine".into()),
        PluginEntry::Name("../../etc/passwd".into()),
    ]);

    let error = resolve_project_plugins(&config, Utf8PathBuf::from(ROOT).as_path())
        .expect_err("the escape is refused");

    assert!(matches!(
        error,
        ResolveError::Entry {
            index: 1,
            source: PluginPathError::ParentSegment { .. },
        }
    ));
    assert_eq!(error.to_string(), "plugins[1] is not a usable plugin name");
}

#[test]
fn one_bad_entry_refuses_the_whole_pipeline() {
    let config = config_with(vec![PluginEntry::Name("/etc/passwd".into())]);

    assert!(
        resolve_pipeline(
            &config,
            Utf8PathBuf::from(ROOT).as_path(),
            PipelineMode::Build
        )
        .is_err(),
        "a config that names code outside the project must not build at all"
    );
}

#[test]
fn the_pipeline_puts_the_builtins_before_a_normal_project_plugin() {
    let config = config_with(vec![PluginEntry::Name("mdx".into())]);

    let container = resolve_pipeline(
        &config,
        Utf8PathBuf::from(ROOT).as_path(),
        PipelineMode::Build,
    )
    .expect("resolves");
    let names = container.names().collect::<Vec<_>>();
    let mdx = names
        .iter()
        .position(|name| *name == "mdx")
        .expect("placed");

    assert_eq!(names.len(), BuiltinPlugin::COUNT + 1);
    for plugin in BuiltinPlugin::ALL {
        let builtin = names
            .iter()
            .position(|name| *name == plugin.name())
            .expect("placed");
        match plugin.order() {
            // A default project plugin is Normal, so it runs after every
            // built-in that asked for no position and before the ones that
            // asked to be last.
            HookOrder::Pre | HookOrder::Normal => assert!(builtin < mdx, "{plugin:?}"),
            HookOrder::Post => assert!(builtin > mdx, "{plugin:?}"),
        }
    }
}

#[test]
fn a_project_plugin_can_ask_to_run_before_the_builtins_in_its_band() {
    let config = config_with(vec![PluginEntry::Spec(
        PluginSpec::new("mdx").with_order(HookOrder::Pre),
    )]);

    let container = resolve_pipeline(
        &config,
        Utf8PathBuf::from(ROOT).as_path(),
        PipelineMode::Build,
    )
    .expect("resolves");
    let names = container.names().collect::<Vec<_>>();

    // Still after the built-in `Pre` plugins, because built-ins are placed
    // first and the sort inside a band is stable.
    assert_eq!(
        names.iter().position(|name| *name == "mdx"),
        Some(2),
        "{names:?}"
    );
}

#[test]
fn a_project_plugin_that_shadows_a_builtin_name_is_refused_before_it_can_run() {
    // The reserved prefix stops this at classification, so the container never
    // has to choose between two plugins called `uf:flow`.
    let config = config_with(vec![PluginEntry::Name("uf:flow".into())]);

    assert!(matches!(
        resolve_pipeline(
            &config,
            Utf8PathBuf::from(ROOT).as_path(),
            PipelineMode::Build
        ),
        Err(ResolveError::Entry {
            index: 0,
            source: PluginPathError::ReservedPrefix { .. },
        })
    ));
}

#[test]
fn two_project_plugins_with_one_name_are_refused() {
    let config = config_with(vec![
        PluginEntry::Name("mdx".into()),
        PluginEntry::Name("mdx".into()),
    ]);

    assert!(matches!(
        resolve_pipeline(
            &config,
            Utf8PathBuf::from(ROOT).as_path(),
            PipelineMode::Build
        ),
        Err(ResolveError::Container(_))
    ));
}
