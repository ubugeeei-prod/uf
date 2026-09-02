//! The closed hook vocabulary and the bitset that makes dispatch a mask test.

use serde::{Deserialize, Serialize};

/// Every hook the uf pipeline drives.
///
/// The set is closed on purpose. A plugin cannot invent a hook name, the
/// container cannot be asked for one it does not run, and adding a hook is a
/// deliberate change to this enum rather than a new string appearing in a
/// config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginHook {
    /// Reshape the config before it is resolved.
    Config,
    /// Observe the fully resolved config.
    ConfigResolved,
    /// A build or dev session is starting.
    BuildStart,
    /// Turn an import specifier into a module id.
    ResolveId,
    /// Produce the source text for a module id.
    Load,
    /// Rewrite a module's source text.
    Transform,
    /// Observe a module once its imports and exports are known.
    ModuleParsed,
    /// Every module has been processed.
    BuildEnd,
    /// Rewrite the code of one output chunk.
    RenderChunk,
    /// Inspect or add to the whole output before it is written.
    GenerateBundle,
    /// The output has been written to disk.
    WriteBundle,
    /// Attach handlers to the dev server.
    ConfigureServer,
    /// Decide what a changed file invalidates.
    HandleHotUpdate,
    /// Rewrite the HTML document shell.
    TransformIndexHtml,
}

impl PluginHook {
    /// Every hook, in pipeline order.
    ///
    /// The order is the order a build drives them, so a table built from this
    /// array reads top-to-bottom like the build itself.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Config,
        Self::ConfigResolved,
        Self::BuildStart,
        Self::ResolveId,
        Self::Load,
        Self::Transform,
        Self::ModuleParsed,
        Self::BuildEnd,
        Self::RenderChunk,
        Self::GenerateBundle,
        Self::WriteBundle,
        Self::ConfigureServer,
        Self::HandleHotUpdate,
        Self::TransformIndexHtml,
    ];

    /// How many hooks exist. [`HookSet`] needs at least this many bits.
    pub const COUNT: usize = 14;

    /// Stable id, used in `uf inspect --json` and in diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::ConfigResolved => "config-resolved",
            Self::BuildStart => "build-start",
            Self::ResolveId => "resolve-id",
            Self::Load => "load",
            Self::Transform => "transform",
            Self::ModuleParsed => "module-parsed",
            Self::BuildEnd => "build-end",
            Self::RenderChunk => "render-chunk",
            Self::GenerateBundle => "generate-bundle",
            Self::WriteBundle => "write-bundle",
            Self::ConfigureServer => "configure-server",
            Self::HandleHotUpdate => "handle-hot-update",
            Self::TransformIndexHtml => "transform-index-html",
        }
    }

    /// Position in [`PluginHook::ALL`], and the bit index in a [`HookSet`].
    pub const fn index(self) -> usize {
        match self {
            Self::Config => 0,
            Self::ConfigResolved => 1,
            Self::BuildStart => 2,
            Self::ResolveId => 3,
            Self::Load => 4,
            Self::Transform => 5,
            Self::ModuleParsed => 6,
            Self::BuildEnd => 7,
            Self::RenderChunk => 8,
            Self::GenerateBundle => 9,
            Self::WriteBundle => 10,
            Self::ConfigureServer => 11,
            Self::HandleHotUpdate => 12,
            Self::TransformIndexHtml => 13,
        }
    }

    /// This hook as a one-bit mask.
    pub const fn bit(self) -> u16 {
        1u16 << self.index()
    }

    /// How the container combines what the plugins return.
    ///
    /// This is a property of the hook, not of any plugin, which is why the
    /// container can guarantee it: a `FirstWins` hook stops at the first
    /// plugin that answers, and a `Chained` hook feeds each answer to the
    /// next plugin. See [`crate::HookOutcome`].
    pub const fn dispatch(self) -> HookDispatch {
        match self {
            // Whoever claims a specifier or a module id owns it; asking the
            // rest afterwards could only produce a second, conflicting answer.
            Self::ResolveId | Self::Load => HookDispatch::FirstWins,
            // Each plugin rewrites what the previous one produced.
            Self::Transform
            | Self::RenderChunk
            | Self::TransformIndexHtml
            | Self::HandleHotUpdate => HookDispatch::Chained,
            // Notifications: every plugin sees the event, nobody consumes it.
            Self::Config
            | Self::ConfigResolved
            | Self::BuildStart
            | Self::ModuleParsed
            | Self::BuildEnd
            | Self::GenerateBundle
            | Self::WriteBundle
            | Self::ConfigureServer => HookDispatch::Broadcast,
        }
    }
}

/// How the container combines the plugins that implement one hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookDispatch {
    /// Every plugin runs; nothing is returned.
    Broadcast,
    /// The first plugin that answers wins and the rest are skipped.
    FirstWins,
    /// Every plugin runs and sees the previous plugin's output.
    Chained,
}

impl HookDispatch {
    /// Stable id, used in `uf inspect --json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Broadcast => "broadcast",
            Self::FirstWins => "first-wins",
            Self::Chained => "chained",
        }
    }
}

/// The set of hooks one plugin implements, as a bitset.
///
/// A `Vec<String>` here would put a string compare in the middle of the
/// per-module loop. A `u16` makes "does this plugin implement `Transform`?" a
/// single AND, which is what lets the container precompute its dispatch lists
/// without walking names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HookSet(u16);

impl HookSet {
    /// No hooks.
    pub const EMPTY: Self = Self(0);

    /// Every hook in [`PluginHook::ALL`].
    pub const ALL: Self = Self((1u16 << PluginHook::COUNT) - 1);

    /// A set holding exactly `hook`.
    pub const fn of(hook: PluginHook) -> Self {
        Self(hook.bit())
    }

    /// This set plus `hook`.
    pub const fn with(self, hook: PluginHook) -> Self {
        Self(self.0 | hook.bit())
    }

    /// This set without `hook`.
    pub const fn without(self, hook: PluginHook) -> Self {
        Self(self.0 & !hook.bit())
    }

    /// Everything in either set.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Everything in both sets.
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Whether `hook` is in the set. One AND, no branch.
    pub const fn contains(self, hook: PluginHook) -> bool {
        self.0 & hook.bit() != 0
    }

    /// Whether the set holds nothing.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many hooks are in the set.
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// The raw mask, for callers that want to store or compare it directly.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// The hooks in the set, in [`PluginHook::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = PluginHook> {
        PluginHook::ALL
            .into_iter()
            .filter(move |hook| self.contains(*hook))
    }
}

impl FromIterator<PluginHook> for HookSet {
    fn from_iter<I: IntoIterator<Item = PluginHook>>(iter: I) -> Self {
        iter.into_iter().fold(Self::EMPTY, Self::with)
    }
}

impl IntoIterator for HookSet {
    type Item = PluginHook;
    type IntoIter = std::vec::IntoIter<PluginHook>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter().collect::<Vec<_>>().into_iter()
    }
}

impl Serialize for HookSet {
    /// Emitted as the list of hook ids, so `uf inspect --json` shows names
    /// rather than a number nobody can read.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(PluginHook::as_str))
    }
}

impl<'de> Deserialize<'de> for HookSet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Vec::<PluginHook>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}
