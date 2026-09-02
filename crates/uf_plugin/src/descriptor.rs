//! What the container knows about a plugin before it runs any of it.

use camino::Utf8PathBuf;
use compact_str::CompactString;
use serde::Serialize;
use uf_config::{ApplyCondition, HookOrder, PipelineMode};

use crate::hook::{HookSet, PluginHook};

/// Who put a plugin in the pipeline.
///
/// `uf inspect --json` reports this so a developer can tell their own plugin
/// apart from the ones uf adds, without either being a special case anywhere
/// else in the container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginOrigin {
    /// Part of uf's own pipeline.
    Builtin,
    /// Declared in `plugins: [...]` in `uf.config.js`.
    Project,
}

impl PluginOrigin {
    /// Stable id, used in `uf inspect --json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Project => "project",
        }
    }
}

/// Where a project plugin's code comes from.
///
/// The distinction matters because only one of these two is a filesystem
/// request. A [`PluginSource::Package`] is a specifier the resolver looks up; a
/// [`PluginSource::ProjectFile`] is a path that has already been checked to sit
/// inside the project root, and it is stored in that checked form so nothing
/// downstream re-derives it from the user's original text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum PluginSource {
    /// uf's own pipeline; nothing is loaded from disk.
    Builtin,
    /// A package specifier such as `@uniflowed/plugin-mdx`.
    Package {
        /// The specifier, exactly as declared.
        specifier: CompactString,
    },
    /// A file inside the project, as a checked project-relative path.
    ProjectFile {
        /// Normalized, verified to stay under the project root.
        path: Utf8PathBuf,
    },
}

impl PluginSource {
    /// Stable id for the variant, used in `uf inspect --json`.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Package { .. } => "package",
            Self::ProjectFile { .. } => "project-file",
        }
    }
}

/// Everything the container needs to place and dispatch one plugin.
///
/// A descriptor is data: building one runs no plugin code, so the whole
/// pipeline can be resolved, ordered, checked for duplicates, and printed by
/// `uf inspect --json` before anything is executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDescriptor {
    /// Unique within one container. Built-in names use `CompactString`'s inline
    /// representation, so naming a built-in allocates nothing.
    pub name: CompactString,
    /// Who put this plugin in the pipeline.
    pub origin: PluginOrigin,
    /// Where the code comes from.
    pub source: PluginSource,
    /// Which band the plugin runs in.
    pub order: HookOrder,
    /// Which pipelines the plugin takes part in.
    pub apply: ApplyCondition,
    /// Which hooks the plugin implements.
    pub hooks: HookSet,
}

impl PluginDescriptor {
    /// A built-in descriptor. `name` is a `&'static str` so no allocation
    /// happens on the path that builds uf's own pipeline.
    pub fn builtin(name: &'static str, order: HookOrder, hooks: HookSet) -> Self {
        Self {
            name: CompactString::const_new(name),
            origin: PluginOrigin::Builtin,
            source: PluginSource::Builtin,
            order,
            apply: ApplyCondition::Always,
            hooks,
        }
    }

    /// A descriptor for a plugin declared in `uf.config.js`.
    pub fn project(
        name: impl Into<CompactString>,
        source: PluginSource,
        order: HookOrder,
        apply: ApplyCondition,
        hooks: HookSet,
    ) -> Self {
        Self {
            name: name.into(),
            origin: PluginOrigin::Project,
            source,
            order,
            apply,
            hooks,
        }
    }

    /// Replace the declared hook set.
    #[must_use]
    pub fn with_hooks(mut self, hooks: HookSet) -> Self {
        self.hooks = hooks;
        self
    }

    /// Replace the declared apply condition.
    #[must_use]
    pub fn with_apply(mut self, apply: ApplyCondition) -> Self {
        self.apply = apply;
        self
    }

    /// Whether this plugin implements `hook`.
    pub const fn implements(&self, hook: PluginHook) -> bool {
        self.hooks.contains(hook)
    }

    /// Whether this plugin runs at all in `mode`.
    pub const fn runs_in(&self, mode: PipelineMode) -> bool {
        self.apply.admits(mode)
    }

    /// Whether this plugin runs `hook` in `mode`.
    pub const fn dispatches(&self, hook: PluginHook, mode: PipelineMode) -> bool {
        self.runs_in(mode) && self.implements(hook)
    }
}
