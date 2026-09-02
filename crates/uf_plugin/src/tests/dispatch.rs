//! First-wins dispatch: `ResolveId` and `Load`.

use uf_config::{HookOrder, PipelineMode};

use crate::container::PluginContainer;
use crate::hook::HookSet;
use crate::outcome::{HookOutcome, ModuleCode, ResolvedId, ResolvedKind};
use crate::plugin::{FnPlugin, Plugin};

use super::{appender, descriptor, passthrough, resolver};

fn build(plugins: Vec<Box<dyn Plugin>>) -> PluginContainer {
    PluginContainer::build(PipelineMode::Build, plugins).expect("container")
}

#[test]
fn resolve_id_takes_the_first_answer() {
    let container = build(vec![
        resolver("first", HookOrder::Normal, "first-id"),
        resolver("second", HookOrder::Normal, "second-id"),
    ]);

    let resolved = container
        .resolve_id("./a.js", None)
        .expect("resolves")
        .handled()
        .expect("someone answered");

    assert_eq!(resolved.id, "first-id");
    assert_eq!(resolved.kind, ResolvedKind::Bundled);
}

#[test]
fn resolve_id_never_asks_a_plugin_after_one_answered() {
    let container = build(vec![
        resolver("winner", HookOrder::Normal, "won"),
        Box::new(
            FnPlugin::new(descriptor("never", HookOrder::Post, HookSet::EMPTY))
                .on_resolve_id(|_| panic!("a later plugin must not be asked")),
        ),
    ]);

    assert!(
        container
            .resolve_id("x", None)
            .expect("resolves")
            .is_handled()
    );
}

#[test]
fn the_pre_band_wins_resolve_id_whatever_the_declaration_order() {
    let container = build(vec![
        resolver("normal", HookOrder::Normal, "normal-id"),
        resolver("pre", HookOrder::Pre, "pre-id"),
    ]);

    assert_eq!(
        container
            .resolve_id("x", None)
            .expect("resolves")
            .handled()
            .expect("answered")
            .id,
        "pre-id"
    );
}

#[test]
fn resolve_id_passes_through_when_nobody_answers() {
    let container = build(vec![
        passthrough("a", HookOrder::Normal),
        passthrough("b", HookOrder::Normal),
    ]);

    assert!(
        container
            .resolve_id("./a.js", Some("./b.js"))
            .expect("resolves")
            .is_passthrough()
    );
}

#[test]
fn resolve_id_passes_through_when_no_plugin_implements_it() {
    let container = build(vec![appender("a", HookOrder::Normal, "!")]);

    assert!(
        container
            .resolve_id("x", None)
            .expect("resolves")
            .is_passthrough()
    );
}

#[test]
fn resolve_id_hands_the_importer_to_the_plugin() {
    let container = build(vec![Box::new(
        FnPlugin::new(descriptor("echo", HookOrder::Normal, HookSet::EMPTY)).on_resolve_id(
            |input| {
                Ok(HookOutcome::Handled(ResolvedId::bundled(format!(
                    "{}<-{}",
                    input.specifier,
                    input.importer.unwrap_or("entry")
                ))))
            },
        ),
    )]);

    assert_eq!(
        container
            .resolve_id("./a.js", Some("./b.js"))
            .expect("resolves")
            .handled()
            .expect("answered")
            .id,
        "./a.js<-./b.js"
    );
    assert_eq!(
        container
            .resolve_id("./a.js", None)
            .expect("resolves")
            .handled()
            .expect("answered")
            .id,
        "./a.js<-entry"
    );
}

#[test]
fn a_resolved_id_records_what_it_names() {
    assert_eq!(ResolvedId::bundled("a").kind, ResolvedKind::Bundled);
    assert_eq!(ResolvedId::external("react").kind, ResolvedKind::External);
    assert_eq!(
        ResolvedId::virtual_module("\0uf:router").kind,
        ResolvedKind::Virtual
    );
}

#[test]
fn load_takes_the_first_answer() {
    let container = build(vec![
        Box::new(
            FnPlugin::new(descriptor("first", HookOrder::Normal, HookSet::EMPTY))
                .on_load(|_| Ok(HookOutcome::Handled(ModuleCode::new("first")))),
        ),
        Box::new(
            FnPlugin::new(descriptor("second", HookOrder::Normal, HookSet::EMPTY))
                .on_load(|_| Ok(HookOutcome::Handled(ModuleCode::new("second")))),
        ),
    ]);

    assert_eq!(
        container
            .load("x")
            .expect("loads")
            .handled()
            .expect("answered")
            .code,
        "first"
    );
}

#[test]
fn load_passes_through_when_nobody_answers() {
    let container = build(vec![passthrough("a", HookOrder::Normal)]);

    assert!(container.load("x").expect("loads").is_passthrough());
}

#[test]
fn load_carries_a_source_map_when_the_plugin_makes_one() {
    let container = build(vec![Box::new(
        FnPlugin::new(descriptor("a", HookOrder::Normal, HookSet::EMPTY)).on_load(|_| {
            Ok(HookOutcome::Handled(
                ModuleCode::new("code").with_source_map("{\"version\":3}"),
            ))
        }),
    )]);

    assert_eq!(
        container
            .load("x")
            .expect("loads")
            .handled()
            .expect("answered")
            .source_map
            .as_deref(),
        Some("{\"version\":3}")
    );
}
