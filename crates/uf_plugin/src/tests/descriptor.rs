//! The descriptor, and the `apply` condition it carries.

use uf_config::{ApplyCondition, HookOrder, PipelineMode};

use crate::descriptor::{PluginDescriptor, PluginOrigin, PluginSource};
use crate::hook::{HookSet, PluginHook};
use crate::outcome::HookOutcome;

use super::descriptor;

#[test]
fn a_builtin_descriptor_applies_to_both_pipelines() {
    let plugin = PluginDescriptor::builtin(
        "uf:example",
        HookOrder::Pre,
        HookSet::of(PluginHook::Transform),
    );

    assert_eq!(plugin.origin, PluginOrigin::Builtin);
    assert_eq!(plugin.source, PluginSource::Builtin);
    assert_eq!(plugin.apply, ApplyCondition::Always);
    assert_eq!(plugin.order, HookOrder::Pre);
}

#[test]
fn a_project_descriptor_keeps_the_source_it_was_given() {
    let plugin = descriptor("mdx", HookOrder::Post, HookSet::of(PluginHook::Load));

    assert_eq!(plugin.origin, PluginOrigin::Project);
    assert_eq!(
        plugin.source,
        PluginSource::Package {
            specifier: "mdx".into()
        }
    );
    assert_eq!(plugin.source.kind(), "package");
}

#[test]
fn with_hooks_replaces_the_declared_set() {
    let plugin = descriptor("a", HookOrder::Normal, HookSet::of(PluginHook::Load))
        .with_hooks(HookSet::of(PluginHook::Transform));

    assert!(plugin.implements(PluginHook::Transform));
    assert!(!plugin.implements(PluginHook::Load));
}

#[test]
fn with_apply_replaces_the_condition() {
    let plugin =
        descriptor("a", HookOrder::Normal, HookSet::EMPTY).with_apply(ApplyCondition::Build);

    assert_eq!(plugin.apply, ApplyCondition::Build);
}

#[test]
fn implements_matches_the_declared_set() {
    let plugin = descriptor(
        "a",
        HookOrder::Normal,
        HookSet::of(PluginHook::Load).with(PluginHook::Transform),
    );

    assert!(plugin.implements(PluginHook::Load));
    assert!(plugin.implements(PluginHook::Transform));
    assert!(!plugin.implements(PluginHook::ResolveId));
}

#[test]
fn apply_admits_exactly_the_matching_mode() {
    let table = [
        (ApplyCondition::Always, PipelineMode::Build, true),
        (ApplyCondition::Always, PipelineMode::Serve, true),
        (ApplyCondition::Build, PipelineMode::Build, true),
        (ApplyCondition::Build, PipelineMode::Serve, false),
        (ApplyCondition::Serve, PipelineMode::Build, false),
        (ApplyCondition::Serve, PipelineMode::Serve, true),
    ];

    for (apply, mode, expected) in table {
        assert_eq!(apply.admits(mode), expected, "{apply:?} in {mode:?}");
    }
    assert_eq!(
        table.len(),
        ApplyCondition::ALL.len() * PipelineMode::ALL.len(),
        "the table has to stay exhaustive"
    );
}

#[test]
fn runs_in_follows_the_apply_condition() {
    let build_only =
        descriptor("a", HookOrder::Normal, HookSet::EMPTY).with_apply(ApplyCondition::Build);

    assert!(build_only.runs_in(PipelineMode::Build));
    assert!(!build_only.runs_in(PipelineMode::Serve));
}

#[test]
fn dispatches_needs_both_the_hook_and_the_mode() {
    let plugin = descriptor("a", HookOrder::Normal, HookSet::of(PluginHook::Transform))
        .with_apply(ApplyCondition::Serve);

    assert!(plugin.dispatches(PluginHook::Transform, PipelineMode::Serve));
    assert!(!plugin.dispatches(PluginHook::Transform, PipelineMode::Build));
    assert!(!plugin.dispatches(PluginHook::Load, PipelineMode::Serve));
}

#[test]
fn a_descriptor_serializes_its_hooks_by_name() {
    let plugin = PluginDescriptor::builtin(
        "uf:example",
        HookOrder::Pre,
        HookSet::of(PluginHook::Transform),
    );

    assert_eq!(
        serde_json::to_value(&plugin).expect("serializes"),
        serde_json::json!({
            "name": "uf:example",
            "origin": "builtin",
            "source": { "kind": "builtin" },
            "order": "pre",
            "apply": "always",
            "hooks": ["transform"],
        })
    );
}

#[test]
fn plugin_origin_ids_are_stable() {
    assert_eq!(PluginOrigin::Builtin.as_str(), "builtin");
    assert_eq!(PluginOrigin::Project.as_str(), "project");
}

#[test]
fn plugin_source_kinds_are_stable() {
    assert_eq!(PluginSource::Builtin.kind(), "builtin");
    assert_eq!(
        PluginSource::Package {
            specifier: "a".into()
        }
        .kind(),
        "package"
    );
    assert_eq!(
        PluginSource::ProjectFile {
            path: "/tmp/a.js".into()
        }
        .kind(),
        "project-file"
    );
}

#[test]
fn a_source_serializes_under_the_same_kind_it_reports() {
    for source in [
        PluginSource::Builtin,
        PluginSource::Package {
            specifier: "a".into(),
        },
        PluginSource::ProjectFile {
            path: "/tmp/a.js".into(),
        },
    ] {
        let value = serde_json::to_value(&source).expect("serializes");

        assert_eq!(
            value.get("kind").and_then(serde_json::Value::as_str),
            Some(source.kind()),
            "{source:?}"
        );
    }
}

#[test]
fn a_project_file_source_serializes_its_checked_path() {
    assert_eq!(
        serde_json::to_value(PluginSource::ProjectFile {
            path: "/workspace/app/plugins/metrics.js".into(),
        })
        .expect("serializes"),
        serde_json::json!({
            "kind": "project-file",
            "path": "/workspace/app/plugins/metrics.js",
        })
    );
}

#[test]
fn order_and_apply_ids_are_stable() {
    assert_eq!(HookOrder::Pre.as_str(), "pre");
    assert_eq!(HookOrder::Normal.as_str(), "normal");
    assert_eq!(HookOrder::Post.as_str(), "post");
    assert_eq!(ApplyCondition::Build.as_str(), "build");
    assert_eq!(ApplyCondition::Serve.as_str(), "serve");
    assert_eq!(ApplyCondition::Always.as_str(), "always");
    assert_eq!(PipelineMode::Build.as_str(), "build");
    assert_eq!(PipelineMode::Serve.as_str(), "serve");
}

#[test]
fn order_defaults_to_normal_and_apply_to_always() {
    assert_eq!(HookOrder::default(), HookOrder::Normal);
    assert_eq!(ApplyCondition::default(), ApplyCondition::Always);
}

#[test]
fn an_outcome_reports_whether_it_produced_a_value() {
    let handled = HookOutcome::Handled(7u8);
    let declined: HookOutcome<u8> = HookOutcome::Passthrough;

    assert!(handled.is_handled());
    assert!(!handled.is_passthrough());
    assert!(declined.is_passthrough());
    assert!(!declined.is_handled());
    assert_eq!(handled.handled(), Some(7));
    assert_eq!(declined.handled(), None);
    assert_eq!(declined.unwrap_or(3), 3);
    assert_eq!(handled.unwrap_or(3), 7);
}

#[test]
fn mapping_an_outcome_keeps_passthrough() {
    assert_eq!(
        HookOutcome::Handled(2u8).map(|v| v * 2),
        HookOutcome::Handled(4)
    );
    assert_eq!(
        HookOutcome::<u8>::Passthrough.map(|v| v * 2),
        HookOutcome::Passthrough
    );
}

#[test]
fn an_option_converts_into_an_outcome() {
    assert_eq!(HookOutcome::from(Some(1u8)), HookOutcome::Handled(1));
    assert_eq!(HookOutcome::from(None::<u8>), HookOutcome::Passthrough);
}

#[test]
fn as_ref_borrows_without_consuming() {
    let outcome = HookOutcome::Handled(String::from("kept"));

    assert_eq!(outcome.as_ref().handled().map(String::as_str), Some("kept"));
    assert!(outcome.is_handled());
}
