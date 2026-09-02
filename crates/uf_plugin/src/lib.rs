#![deny(missing_docs)]
//! The plugin container that drives every stage of a uf build.
//!
//! uf has one plugin mechanism, and its own pipeline is built out of it: Flow
//! stripping, router codegen, the RSC boundary split, StyleX, the React
//! compiler, and asset handling are all [`BuiltinPlugin`] descriptors placed in
//! the same container a project's own plugins go into. There is no second path
//! and no hardcoded stage, so the run order a developer reads in
//! `uf inspect --json` is the order that actually runs.
//!
//! What the pieces are:
//!
//! * [`PluginHook`] is the closed set of hooks uf drives, and [`HookSet`] is
//!   that set as a bitset, so asking whether a plugin implements a hook is one
//!   AND rather than a string compare in the per-module loop.
//! * [`PluginDescriptor`] is what the container knows before running anything:
//!   a name, an [`origin`](PluginOrigin), a [`source`](PluginSource), a band,
//!   an apply condition, and the hooks.
//! * [`PluginContainer`] fixes the run order — every `Pre` in declaration
//!   order, then every `Normal`, then every `Post` — and owns the dispatch
//!   loop, so first-wins and chaining are properties of the hook rather than
//!   something a caller can get wrong. [`HookOutcome`] is how a plugin says it
//!   produced something without deciding what that means.
//! * [`resolve_pipeline`] turns a `uf.config.js` into a container, refusing any
//!   plugin entry that names a path outside the project root.
//!
//! Stages delegate rather than duplicate: the transform logic lives in the
//! crate that owns it, and [`FnPlugin`] is how that crate hands a closure to the
//! container without this crate depending on it.
//!
//! ```
//! use uf_plugin::{BuiltinSet, PipelineMode, PluginContainer, PluginHook};
//! use uf_config::UniflowedConfig;
//!
//! let config = UniflowedConfig::default();
//! let container = PluginContainer::from_descriptors(
//!     PipelineMode::Build,
//!     BuiltinSet::from_config(&config).descriptors(),
//! )
//! .expect("a default project resolves");
//!
//! // Flow stripping runs before the React compiler, whatever order the
//! // descriptors arrived in.
//! let order = container.names().collect::<Vec<_>>();
//! assert!(
//!     order.iter().position(|name| *name == "uf:flow")
//!         < order.iter().position(|name| *name == "uf:react-compiler")
//! );
//! assert!(container.implements(PluginHook::Transform));
//! ```

pub mod builtin;
pub mod container;
pub mod descriptor;
pub mod hook;
pub mod inspect;
pub mod outcome;
pub mod plugin;
pub mod resolve;

pub use uf_config::{ApplyCondition, HookOrder, PipelineMode, PluginEntry, PluginSpec};

pub use crate::builtin::{BuiltinPlugin, BuiltinSet};
pub use crate::container::{ContainerError, MAX_PLUGINS, PluginContainer};
pub use crate::descriptor::{PluginDescriptor, PluginOrigin, PluginSource};
pub use crate::hook::{HookDispatch, HookSet, PluginHook};
pub use crate::inspect::{HookReport, PipelineReport, PluginReport};
pub use crate::outcome::{
    HookFailure, HookOutcome, HookResult, LoadInput, ModuleCode, ResolveInput, ResolvedId,
    ResolvedKind, TransformInput,
};
pub use crate::plugin::{FnPlugin, Plugin};
pub use crate::resolve::{
    BUILTIN_PREFIX, MAX_PLUGIN_NAME_BYTES, PluginPathError, ResolveError, classify_plugin_name,
    resolve_entry, resolve_pipeline, resolve_project_plugins,
};

#[cfg(test)]
mod tests;
