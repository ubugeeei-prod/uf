//! The `plugins` section of `uf inspect --json`.
//!
//! A pipeline nobody can see is a pipeline nobody can debug, so the resolved
//! order and each plugin's hooks are reportable without running anything. The
//! report names uf's stages and the project's own plugins and nothing else: the
//! engines underneath are an implementation detail, and naming them here would
//! put them in front of users who only ever wrote `uf.config.js`.

use serde::Serialize;
use uf_config::{ApplyCondition, HookOrder, PipelineMode};

use crate::container::PluginContainer;
use crate::descriptor::{PluginOrigin, PluginSource};
use crate::hook::{HookDispatch, HookSet, PluginHook};

/// The resolved pipeline, ready to serialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineReport<'a> {
    /// Which pipeline was resolved.
    pub mode: PipelineMode,
    /// How many plugins are in it.
    pub count: usize,
    /// The plugins, in the order they run.
    pub plugins: Vec<PluginReport<'a>>,
    /// The hooks that any plugin runs, and who runs them.
    pub hooks: Vec<HookReport<'a>>,
}

/// One plugin's place in the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginReport<'a> {
    /// Zero-based position in the run order.
    pub position: usize,
    /// The plugin's name.
    pub name: &'a str,
    /// Whether uf or the project put it here.
    pub origin: PluginOrigin,
    /// Where its code comes from.
    pub source: &'a PluginSource,
    /// Which band it runs in.
    pub order: HookOrder,
    /// Which pipelines it takes part in.
    pub apply: ApplyCondition,
    /// Which hooks it implements.
    ///
    /// Empty for a project plugin whose module has not been read yet: the
    /// resolver places plugins, it does not load them.
    pub hooks: HookSet,
    /// Whether it runs at all in this mode.
    pub active: bool,
}

/// One hook, and the plugins that run it here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookReport<'a> {
    /// The hook.
    pub hook: PluginHook,
    /// How the container combines what the plugins return.
    pub dispatch: HookDispatch,
    /// The plugins that run it, in run order.
    pub plugins: Vec<&'a str>,
}

impl PluginContainer {
    /// The resolved pipeline as a serializable report.
    ///
    /// Only hooks that at least one plugin runs are listed, so the section
    /// describes this project rather than restating the hook enum.
    pub fn report(&self) -> PipelineReport<'_> {
        let plugins = self
            .descriptors()
            .iter()
            .enumerate()
            .map(|(position, descriptor)| PluginReport {
                position,
                name: descriptor.name.as_str(),
                origin: descriptor.origin,
                source: &descriptor.source,
                order: descriptor.order,
                apply: descriptor.apply,
                hooks: descriptor.hooks,
                active: descriptor.runs_in(self.mode()),
            })
            .collect();

        let hooks = self
            .active_hooks()
            .iter()
            .map(|hook| HookReport {
                hook,
                dispatch: hook.dispatch(),
                plugins: self
                    .plugins_for(hook)
                    .map(|descriptor| descriptor.name.as_str())
                    .collect(),
            })
            .collect();

        PipelineReport {
            mode: self.mode(),
            count: self.len(),
            plugins,
            hooks,
        }
    }
}
