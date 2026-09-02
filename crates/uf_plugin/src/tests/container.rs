//! Run order, duplicate names, and the ceilings.

use uf_config::{ApplyCondition, HookOrder, PipelineMode};

use crate::container::{ContainerError, MAX_PLUGINS, PluginContainer};
use crate::descriptor::PluginDescriptor;
use crate::hook::{HookSet, PluginHook};
use crate::plugin::{FnPlugin, Plugin};

use super::{descriptor, inert};

fn build(plugins: Vec<Box<dyn Plugin>>) -> PluginContainer {
    PluginContainer::build(PipelineMode::Build, plugins).expect("container")
}

#[test]
fn pre_runs_first_then_normal_then_post() {
    let container = build(vec![
        inert("post", HookOrder::Post),
        inert("normal", HookOrder::Normal),
        inert("pre", HookOrder::Pre),
    ]);

    assert_eq!(
        container.names().collect::<Vec<_>>(),
        vec!["pre", "normal", "post"]
    );
}

#[test]
fn declaration_order_is_kept_inside_a_band() {
    let container = build(vec![
        inert("pre-a", HookOrder::Pre),
        inert("normal-a", HookOrder::Normal),
        inert("pre-b", HookOrder::Pre),
        inert("normal-b", HookOrder::Normal),
        inert("pre-c", HookOrder::Pre),
    ]);

    assert_eq!(
        container.names().collect::<Vec<_>>(),
        vec!["pre-a", "pre-b", "pre-c", "normal-a", "normal-b"]
    );
}

#[test]
fn the_run_order_does_not_depend_on_the_declaration_order_of_bands() {
    let forwards = build(vec![
        inert("a", HookOrder::Pre),
        inert("b", HookOrder::Normal),
        inert("c", HookOrder::Post),
    ]);
    let backwards = build(vec![
        inert("c", HookOrder::Post),
        inert("b", HookOrder::Normal),
        inert("a", HookOrder::Pre),
    ]);

    assert_eq!(
        forwards.names().collect::<Vec<_>>(),
        backwards.names().collect::<Vec<_>>()
    );
}

#[test]
fn an_empty_container_runs_nothing() {
    let container = build(Vec::new());

    assert!(container.is_empty());
    assert_eq!(container.len(), 0);
    assert_eq!(container.active_hooks(), HookSet::EMPTY);
    for hook in PluginHook::ALL {
        assert!(!container.implements(hook), "{hook}");
    }
}

#[test]
fn two_plugins_with_one_name_is_an_error() {
    let error = PluginContainer::build(
        PipelineMode::Build,
        vec![
            inert("first", HookOrder::Normal),
            inert("other", HookOrder::Normal),
            inert("first", HookOrder::Normal),
        ],
    )
    .expect_err("a duplicate name is refused");

    assert_eq!(
        error,
        ContainerError::DuplicateName {
            name: "first".into(),
            first: 0,
            second: 2,
        }
    );
}

#[test]
fn a_duplicate_is_reported_at_its_declared_position_not_its_sorted_one() {
    let error = PluginContainer::build(
        PipelineMode::Build,
        vec![
            inert("post", HookOrder::Post),
            inert("dupe", HookOrder::Pre),
            inert("dupe", HookOrder::Post),
        ],
    )
    .expect_err("a duplicate name is refused");

    // Sorting would have moved `dupe`/Pre to the front; the message points at
    // the lines the user actually wrote.
    assert_eq!(
        error,
        ContainerError::DuplicateName {
            name: "dupe".into(),
            first: 1,
            second: 2,
        }
    );
}

#[test]
fn a_project_plugin_cannot_take_a_builtin_name() {
    let error = PluginContainer::from_descriptors(
        PipelineMode::Build,
        vec![
            PluginDescriptor::builtin("uf:flow", HookOrder::Pre, HookSet::EMPTY),
            descriptor("uf:flow", HookOrder::Post, HookSet::EMPTY),
        ],
    )
    .expect_err("a shadowed built-in is refused");

    assert!(matches!(error, ContainerError::DuplicateName { .. }));
}

#[test]
fn a_duplicate_error_names_the_plugin() {
    let error = ContainerError::DuplicateName {
        name: "mdx".into(),
        first: 1,
        second: 4,
    };

    assert_eq!(
        error.to_string(),
        "two plugins are named mdx: entries 1 and 4"
    );
}

#[test]
fn more_plugins_than_the_ceiling_is_an_error() {
    let plugins = (0..=MAX_PLUGINS)
        .map(|index| inert(&format!("p{index}"), HookOrder::Normal))
        .collect::<Vec<_>>();
    let count = plugins.len();

    let error =
        PluginContainer::build(PipelineMode::Build, plugins).expect_err("the ceiling is enforced");

    assert_eq!(
        error,
        ContainerError::TooManyPlugins {
            count,
            limit: MAX_PLUGINS,
        }
    );
}

#[test]
fn exactly_the_ceiling_is_accepted() {
    let plugins = (0..MAX_PLUGINS)
        .map(|index| inert(&format!("p{index}"), HookOrder::Normal))
        .collect::<Vec<_>>();

    let container = build(plugins);

    assert_eq!(container.len(), MAX_PLUGINS);
}

#[test]
fn the_plan_lists_plugins_that_this_mode_filters_out() {
    let container = PluginContainer::from_descriptors(
        PipelineMode::Serve,
        vec![
            descriptor(
                "build-only",
                HookOrder::Normal,
                HookSet::of(PluginHook::Transform),
            )
            .with_apply(ApplyCondition::Build),
        ],
    )
    .expect("container");

    assert_eq!(container.len(), 1);
    assert_eq!(container.names().collect::<Vec<_>>(), vec!["build-only"]);
    assert!(!container.implements(PluginHook::Transform));
}

#[test]
fn active_hooks_is_the_union_of_what_will_run() {
    let container = PluginContainer::from_descriptors(
        PipelineMode::Build,
        vec![
            descriptor("a", HookOrder::Normal, HookSet::of(PluginHook::Load)),
            descriptor("b", HookOrder::Normal, HookSet::of(PluginHook::Transform)),
            descriptor("c", HookOrder::Normal, HookSet::of(PluginHook::Load)),
        ],
    )
    .expect("container");

    assert_eq!(
        container.active_hooks(),
        HookSet::of(PluginHook::Load).with(PluginHook::Transform)
    );
}

#[test]
fn active_hooks_skips_a_plugin_this_mode_filters_out() {
    let container = PluginContainer::from_descriptors(
        PipelineMode::Build,
        vec![
            descriptor(
                "serve",
                HookOrder::Normal,
                HookSet::of(PluginHook::ConfigureServer),
            )
            .with_apply(ApplyCondition::Serve),
        ],
    )
    .expect("container");

    assert_eq!(container.active_hooks(), HookSet::EMPTY);
}

#[test]
fn plugins_for_lists_only_the_plugins_in_that_lane() {
    let container = PluginContainer::from_descriptors(
        PipelineMode::Build,
        vec![
            descriptor("loader", HookOrder::Normal, HookSet::of(PluginHook::Load)),
            descriptor(
                "transformer",
                HookOrder::Normal,
                HookSet::of(PluginHook::Transform),
            ),
            descriptor(
                "both",
                HookOrder::Post,
                HookSet::of(PluginHook::Load).with(PluginHook::Transform),
            ),
        ],
    )
    .expect("container");

    assert_eq!(
        container
            .plugins_for(PluginHook::Load)
            .map(|plugin| plugin.name.as_str())
            .collect::<Vec<_>>(),
        vec!["loader", "both"]
    );
    assert_eq!(container.plugins_for(PluginHook::BuildEnd).count(), 0);
}

#[test]
fn the_container_remembers_which_mode_it_resolved_for() {
    let container =
        PluginContainer::from_descriptors(PipelineMode::Serve, Vec::new()).expect("container");

    assert_eq!(container.mode(), PipelineMode::Serve);
}

#[test]
fn wiring_a_hook_also_declares_it() {
    let plugin =
        FnPlugin::new(descriptor("a", HookOrder::Normal, HookSet::EMPTY)).on_transform(|_| {
            Ok(crate::outcome::HookOutcome::Handled(
                crate::outcome::ModuleCode::new(""),
            ))
        });

    assert!(plugin.descriptor().implements(PluginHook::Transform));
}

#[test]
fn the_report_describes_the_resolved_pipeline() {
    let container = PluginContainer::from_descriptors(
        PipelineMode::Build,
        vec![
            descriptor("late", HookOrder::Post, HookSet::of(PluginHook::Transform)),
            descriptor("early", HookOrder::Pre, HookSet::of(PluginHook::Transform)),
        ],
    )
    .expect("container");

    let report = container.report();

    assert_eq!(report.count, 2);
    assert_eq!(report.mode, PipelineMode::Build);
    assert_eq!(report.plugins[0].name, "early");
    assert_eq!(report.plugins[0].position, 0);
    assert!(report.plugins[0].active);
    assert_eq!(report.hooks.len(), 1);
    assert_eq!(report.hooks[0].hook, PluginHook::Transform);
    assert_eq!(report.hooks[0].plugins, vec!["early", "late"]);
}

#[test]
fn the_report_marks_a_plugin_this_mode_filters_out_as_inactive() {
    let container = PluginContainer::from_descriptors(
        PipelineMode::Build,
        vec![
            descriptor(
                "serve-only",
                HookOrder::Normal,
                HookSet::of(PluginHook::ConfigureServer),
            )
            .with_apply(ApplyCondition::Serve),
        ],
    )
    .expect("container");

    let report = container.report();

    assert!(!report.plugins[0].active);
    assert!(report.hooks.is_empty());
}
