//! Tests for the plugin container.
//!
//! Split by concern so each file stays readable: the hook vocabulary, the
//! descriptor, the container's ordering rules, its dispatch rules, the
//! built-in pipeline, config resolution and its path guard, and the check that
//! no engine name reaches a user.

mod builtin;
mod chain;
mod config;
mod container;
mod descriptor;
mod dispatch;
mod hook;
mod naming;
mod path_guard;
mod resolve;

use camino::Utf8PathBuf;
use uf_config::{ApplyCondition, HookOrder};

use crate::descriptor::{PluginDescriptor, PluginSource};
use crate::hook::{HookSet, PluginHook};
use crate::outcome::{HookOutcome, HookResult, ModuleCode, ResolvedId, TransformInput};
use crate::plugin::{FnPlugin, Plugin};

/// A project plugin that implements nothing, for ordering tests.
fn inert(name: &str, order: HookOrder) -> Box<dyn Plugin> {
    Box::new(FnPlugin::new(descriptor(name, order, HookSet::EMPTY)))
}

/// A descriptor for a project plugin with no filesystem source.
fn descriptor(name: &str, order: HookOrder, hooks: HookSet) -> PluginDescriptor {
    PluginDescriptor::project(
        name,
        PluginSource::Package {
            specifier: name.into(),
        },
        order,
        ApplyCondition::Always,
        hooks,
    )
}

/// A plugin whose `Transform` hook appends `mark` to whatever it is handed.
fn appender(name: &str, order: HookOrder, mark: &'static str) -> Box<dyn Plugin> {
    Box::new(
        FnPlugin::new(descriptor(name, order, HookSet::EMPTY)).on_transform(
            move |input: TransformInput<'_>| {
                Ok(HookOutcome::Handled(ModuleCode::new(format!(
                    "{}{mark}",
                    input.code
                ))))
            },
        ),
    )
}

/// A plugin that claims every specifier, answering with `id`.
fn resolver(name: &str, order: HookOrder, id: &'static str) -> Box<dyn Plugin> {
    Box::new(
        FnPlugin::new(descriptor(name, order, HookSet::EMPTY))
            .on_resolve_id(move |_| Ok(HookOutcome::Handled(ResolvedId::bundled(id)))),
    )
}

/// A plugin that declines every hook it is asked.
fn passthrough(name: &str, order: HookOrder) -> Box<dyn Plugin> {
    Box::new(
        FnPlugin::new(descriptor(name, order, HookSet::EMPTY))
            .on_resolve_id(|_| Ok(HookOutcome::Passthrough))
            .on_load(|_| Ok(HookOutcome::Passthrough))
            .on_transform(|_| Ok(HookOutcome::Passthrough)),
    )
}

/// A plugin whose `Transform` hook always fails with `failure`.
fn failing(name: &str, order: HookOrder, failure: crate::outcome::HookFailure) -> Box<dyn Plugin> {
    Box::new(
        FnPlugin::new(descriptor(name, order, HookSet::EMPTY))
            .on_transform(move |_| -> HookResult<ModuleCode> { Err(failure) }),
    )
}

/// A descriptor that declares `hook` without wiring anything to it.
fn declaring(name: &str, hook: PluginHook) -> PluginDescriptor {
    descriptor(name, HookOrder::Normal, HookSet::of(hook))
}

/// A temporary directory as a UTF-8 path.
fn temp_root(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("temp dir is UTF-8")
}
