//! The container: one deterministic run order, and dispatch that costs a slice
//! walk per module rather than a search.

use compact_str::CompactString;
use thiserror::Error;
use uf_config::{HookOrder, PipelineMode};
use uf_infra::{FxHashMap, InlineVec};

use crate::descriptor::PluginDescriptor;
use crate::hook::{HookSet, PluginHook};
use crate::outcome::{
    HookFailure, HookOutcome, LoadInput, ModuleCode, ResolveInput, ResolvedId, TransformInput,
};
use crate::plugin::{FnPlugin, Plugin};

/// How many plugins one container will hold.
///
/// Config is untrusted input and the container indexes plugins with a `u16`, so
/// the ceiling is explicit rather than "whatever fits in memory".
pub const MAX_PLUGINS: usize = 1024;

/// Indices into the plugin list for one hook. Eight covers every realistic
/// pipeline without touching the allocator.
type HookLane = InlineVec<u16, 8>;

/// Everything that can go wrong before or during dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContainerError {
    /// Two plugins claimed the same name.
    ///
    /// Silently letting the second win is how a pipeline ends up not running
    /// the plugin its config names, so this is an error with both positions.
    #[error("two plugins are named {name}: entries {first} and {second}")]
    DuplicateName {
        /// The contested name.
        name: CompactString,
        /// Declaration index of the first plugin with that name.
        first: usize,
        /// Declaration index of the second.
        second: usize,
    },
    /// More plugins than the container will hold.
    #[error("{count} plugins declared, over the ceiling of {limit}")]
    TooManyPlugins {
        /// How many were declared.
        count: usize,
        /// The ceiling, always [`MAX_PLUGINS`].
        limit: usize,
    },
    /// A plugin failed while running a hook.
    #[error("plugin {plugin} failed the {hook} hook for {module}")]
    Hook {
        /// The plugin that failed.
        plugin: CompactString,
        /// The hook it was running.
        hook: PluginHook,
        /// The module or specifier it was handed.
        module: CompactString,
        /// The plugin's own typed reason.
        #[source]
        source: HookFailure,
    },
}

/// The resolved pipeline.
///
/// Construction does all the work that can be done without running plugin code:
/// it rejects duplicate names, fixes the run order, and turns each plugin's
/// [`HookSet`] into a per-hook list of indices. After that a hook dispatch is a
/// walk over a short slice, with no name comparison, no filtering, and no
/// allocation unless a plugin actually produces something.
pub struct PluginContainer {
    /// In resolved run order.
    plugins: Vec<Box<dyn Plugin>>,
    /// Parallel to `plugins`, so the plan can be reported without running it.
    descriptors: Vec<PluginDescriptor>,
    mode: PipelineMode,
    /// Union of the hooks that will actually run in `mode`.
    active: HookSet,
    /// For each hook, the plugins that implement it and pass `apply`.
    lanes: [HookLane; PluginHook::COUNT],
}

impl PluginContainer {
    /// Resolve `plugins` into a pipeline for `mode`.
    ///
    /// The run order is every [`HookOrder::Pre`] plugin in declaration order,
    /// then every [`HookOrder::Normal`], then every [`HookOrder::Post`]. The
    /// sort is stable, so the result is a pure function of the declaration
    /// list: two machines reading the same `uf.config.js` build in the same
    /// order.
    pub fn build(
        mode: PipelineMode,
        plugins: Vec<Box<dyn Plugin>>,
    ) -> Result<Self, ContainerError> {
        if plugins.len() > MAX_PLUGINS {
            return Err(ContainerError::TooManyPlugins {
                count: plugins.len(),
                limit: MAX_PLUGINS,
            });
        }

        let mut seen: FxHashMap<&str, usize> =
            FxHashMap::with_capacity_and_hasher(plugins.len(), Default::default());
        for (index, plugin) in plugins.iter().enumerate() {
            let name = plugin.descriptor().name.as_str();
            if let Some(&first) = seen.get(name) {
                return Err(ContainerError::DuplicateName {
                    name: CompactString::new(name),
                    first,
                    second: index,
                });
            }
            seen.insert(name, index);
        }
        drop(seen);

        let mut plugins = plugins;
        plugins.sort_by_key(|plugin| order_rank(plugin.descriptor().order));

        let descriptors = plugins
            .iter()
            .map(|plugin| plugin.descriptor().clone())
            .collect::<Vec<_>>();

        let mut lanes: [HookLane; PluginHook::COUNT] = std::array::from_fn(|_| HookLane::new());
        let mut active = HookSet::EMPTY;
        for (index, descriptor) in descriptors.iter().enumerate() {
            if !descriptor.runs_in(mode) {
                continue;
            }
            for hook in descriptor.hooks.iter() {
                lanes[hook.index()].push(index as u16);
                active = active.with(hook);
            }
        }

        Ok(Self {
            plugins,
            descriptors,
            mode,
            active,
            lanes,
        })
    }

    /// Resolve descriptors alone into a pipeline that runs nothing.
    ///
    /// `uf inspect --json` reports the plan without executing it, and the same
    /// ordering and duplicate rules have to apply to that report or it would be
    /// describing a different pipeline than the one that builds.
    pub fn from_descriptors(
        mode: PipelineMode,
        descriptors: Vec<PluginDescriptor>,
    ) -> Result<Self, ContainerError> {
        let plugins = descriptors
            .into_iter()
            .map(|descriptor| Box::new(FnPlugin::new(descriptor)) as Box<dyn Plugin>)
            .collect();
        Self::build(mode, plugins)
    }

    /// The resolved plan, in run order.
    pub fn descriptors(&self) -> &[PluginDescriptor] {
        &self.descriptors
    }

    /// Plugin names, in run order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.descriptors.iter().map(|entry| entry.name.as_str())
    }

    /// Which pipeline this container was resolved for.
    pub const fn mode(&self) -> PipelineMode {
        self.mode
    }

    /// How many plugins are in the pipeline, including ones filtered out of
    /// every hook by their apply condition.
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    /// Whether the pipeline is empty.
    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }

    /// Every hook some plugin will actually run in this mode. One AND.
    pub const fn active_hooks(&self) -> HookSet {
        self.active
    }

    /// Whether any plugin runs `hook` in this mode.
    pub const fn implements(&self, hook: PluginHook) -> bool {
        self.active.contains(hook)
    }

    /// The plugins that run `hook`, in run order.
    pub fn plugins_for(&self, hook: PluginHook) -> impl Iterator<Item = &PluginDescriptor> {
        self.lanes[hook.index()]
            .iter()
            .map(|&index| &self.descriptors[index as usize])
    }

    /// Resolve an import specifier. First-wins: the first plugin that answers
    /// owns the id and no later plugin is asked.
    pub fn resolve_id(
        &self,
        specifier: &str,
        importer: Option<&str>,
    ) -> Result<HookOutcome<ResolvedId>, ContainerError> {
        let input = ResolveInput {
            specifier,
            importer,
        };
        for &index in &self.lanes[PluginHook::ResolveId.index()] {
            match self.plugins[index as usize].resolve_id(input) {
                Ok(HookOutcome::Handled(resolved)) => return Ok(HookOutcome::Handled(resolved)),
                Ok(HookOutcome::Passthrough) => {}
                Err(source) => {
                    return Err(self.hook_error(index, PluginHook::ResolveId, specifier, source));
                }
            }
        }
        Ok(HookOutcome::Passthrough)
    }

    /// Load a module's source. First-wins, like [`Self::resolve_id`].
    pub fn load(&self, id: &str) -> Result<HookOutcome<ModuleCode>, ContainerError> {
        let input = LoadInput { id };
        for &index in &self.lanes[PluginHook::Load.index()] {
            match self.plugins[index as usize].load(input) {
                Ok(HookOutcome::Handled(code)) => return Ok(HookOutcome::Handled(code)),
                Ok(HookOutcome::Passthrough) => {}
                Err(source) => {
                    return Err(self.hook_error(index, PluginHook::Load, id, source));
                }
            }
        }
        Ok(HookOutcome::Passthrough)
    }

    /// Run the transform chain over one module.
    ///
    /// Each plugin sees what the previous one produced. When no plugin touches
    /// the module the result is [`HookOutcome::Passthrough`] and nothing was
    /// allocated, which is the common case for most modules in a build.
    ///
    /// The surviving source map is the last one produced. A plugin that
    /// rewrites code without producing a map drops the chain's map on purpose:
    /// a map from an earlier stage describes text that no longer exists.
    pub fn transform(
        &self,
        id: &str,
        code: &str,
    ) -> Result<HookOutcome<ModuleCode>, ContainerError> {
        let mut current: Option<ModuleCode> = None;
        for &index in &self.lanes[PluginHook::Transform.index()] {
            let input = TransformInput {
                id,
                code: current.as_ref().map_or(code, |held| held.code.as_str()),
            };
            match self.plugins[index as usize].transform(input) {
                Ok(HookOutcome::Handled(produced)) => current = Some(produced),
                Ok(HookOutcome::Passthrough) => {}
                Err(source) => {
                    return Err(self.hook_error(index, PluginHook::Transform, id, source));
                }
            }
        }
        Ok(current.into())
    }

    /// Run a broadcast hook. Every plugin that declared it is notified, in run
    /// order, and the first failure stops the pipeline.
    pub fn notify(&self, hook: PluginHook) -> Result<(), ContainerError> {
        for &index in &self.lanes[hook.index()] {
            if let Err(source) = self.plugins[index as usize].notify(hook) {
                return Err(self.hook_error(index, hook, hook.as_str(), source));
            }
        }
        Ok(())
    }

    fn hook_error(
        &self,
        index: u16,
        hook: PluginHook,
        module: &str,
        source: HookFailure,
    ) -> ContainerError {
        ContainerError::Hook {
            plugin: self.descriptors[index as usize].name.clone(),
            hook,
            module: CompactString::new(module),
            source,
        }
    }
}

impl std::fmt::Debug for PluginContainer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginContainer")
            .field("mode", &self.mode)
            .field("descriptors", &self.descriptors)
            .finish_non_exhaustive()
    }
}

/// Sort key for the three bands. Stable sorting on this keeps declaration
/// order within a band.
const fn order_rank(order: HookOrder) -> u8 {
    match order {
        HookOrder::Pre => 0,
        HookOrder::Normal => 1,
        HookOrder::Post => 2,
    }
}
