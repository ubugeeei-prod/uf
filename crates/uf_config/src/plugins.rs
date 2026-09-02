//! The `plugins: [...]` surface of `uf.config.js`.
//!
//! A uf project declares plugins here and nowhere else. There is no second
//! config file and no second plugin format, so this module owns the whole
//! vocabulary a user ever types: a name, and the two ordering knobs that decide
//! when the plugin runs relative to everything else.
//!
//! The declarations are deliberately inert data. Turning one into something the
//! build actually runs belongs to `uf_plugin`, and that is also where the
//! project-root containment check lives: a plugin name that names a file is a
//! request to execute code from disk, so it is untrusted input, not a label.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// One entry of `plugins: [...]` in `uf.config.js`.
///
/// Both spellings mean the same thing; the short one is the common case.
///
/// ```js
/// plugins: [
///   "@uniflowed/plugin-mdx",
///   { name: "./plugins/metrics.js", order: "post", apply: "build" },
/// ]
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginEntry {
    /// A bare name, taking the default order and apply condition.
    Name(CompactString),
    /// A name with an explicit order and/or apply condition.
    Spec(PluginSpec),
}

impl PluginEntry {
    /// The declared name, exactly as the user typed it.
    ///
    /// This is untrusted text: it has not been checked for path escapes yet.
    pub fn name(&self) -> &str {
        match self {
            Self::Name(name) => name.as_str(),
            Self::Spec(spec) => spec.name.as_str(),
        }
    }

    /// Where the plugin sits in the run order.
    pub fn order(&self) -> HookOrder {
        match self {
            Self::Name(_) => HookOrder::Normal,
            Self::Spec(spec) => spec.order,
        }
    }

    /// Which pipelines the plugin takes part in.
    pub fn apply(&self) -> ApplyCondition {
        match self {
            Self::Name(_) => ApplyCondition::Always,
            Self::Spec(spec) => spec.apply,
        }
    }
}

impl From<PluginSpec> for PluginEntry {
    fn from(spec: PluginSpec) -> Self {
        Self::Spec(spec)
    }
}

/// The long form of a [`PluginEntry`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PluginSpec {
    /// Package specifier or project-relative path naming the plugin.
    pub name: CompactString,
    /// Where the plugin sits in the run order.
    pub order: HookOrder,
    /// Which pipelines the plugin takes part in.
    pub apply: ApplyCondition,
}

impl PluginSpec {
    /// A plugin declared by name, taking the default order and apply condition.
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Place the plugin in a band other than [`HookOrder::Normal`].
    #[must_use]
    pub fn with_order(mut self, order: HookOrder) -> Self {
        self.order = order;
        self
    }

    /// Restrict the plugin to one pipeline.
    #[must_use]
    pub fn with_apply(mut self, apply: ApplyCondition) -> Self {
        self.apply = apply;
        self
    }
}

/// Where a plugin sits relative to the plugins that expressed no preference.
///
/// Within one band, declaration order in `uf.config.js` decides. That makes the
/// resolved pipeline a pure function of the config file, so two machines that
/// read the same config build in the same order.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum HookOrder {
    /// Runs before every plugin that did not ask for a position.
    Pre,
    /// The default band.
    #[default]
    Normal,
    /// Runs after every plugin that did not ask for a position.
    Post,
}

impl HookOrder {
    /// Every band, in the order the container runs them.
    pub const ALL: [Self; 3] = [Self::Pre, Self::Normal, Self::Post];

    /// The stable id used in `uf inspect --json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pre => "pre",
            Self::Normal => "normal",
            Self::Post => "post",
        }
    }
}

/// Which pipelines a plugin takes part in.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyCondition {
    /// Only when producing a build.
    Build,
    /// Only while the dev server is serving.
    Serve,
    /// Both.
    #[default]
    Always,
}

impl ApplyCondition {
    /// Every condition, for exhaustive tests and tables.
    pub const ALL: [Self; 3] = [Self::Build, Self::Serve, Self::Always];

    /// Whether a plugin with this condition runs in `mode`.
    pub const fn admits(self, mode: PipelineMode) -> bool {
        matches!(
            (self, mode),
            (Self::Always, _)
                | (Self::Build, PipelineMode::Build)
                | (Self::Serve, PipelineMode::Serve)
        )
    }

    /// The stable id used in `uf inspect --json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Serve => "serve",
            Self::Always => "always",
        }
    }
}

/// Which pipeline the plugin container is currently driving.
///
/// This is runtime state rather than a config field, but it is the other half
/// of [`ApplyCondition`], so the two live together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipelineMode {
    /// `uf build`.
    Build,
    /// `uf dev`.
    Serve,
}

impl PipelineMode {
    /// Both modes, for exhaustive tests and tables.
    pub const ALL: [Self; 2] = [Self::Build, Self::Serve];

    /// The stable id used in `uf inspect --json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Serve => "serve",
        }
    }
}
