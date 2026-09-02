//! uf's own pipeline, expressed as plugins.
//!
//! Everything the toolchain does to a module goes through the container, so
//! there is exactly one mechanism and one run order. What each stage *does*
//! stays in the crate that owns it — `uf_flow` parses, `uf_router` discovers
//! routes, `uf_rsc` splits the client boundary — and this module only says
//! where in the pipeline that work belongs. Wiring the work itself is
//! [`FnPlugin`](crate::FnPlugin)'s job, which is why no transform is
//! reimplemented here and why this crate does not depend on any of them.

use serde::Serialize;
use uf_config::{HookOrder, ReactCompilerMode, StyleEngine, UniflowedConfig};

use crate::descriptor::PluginDescriptor;
use crate::hook::{HookSet, PluginHook};

/// A stage of uf's own pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltinPlugin {
    /// Turns `.mdx` documents into React modules before Flow sees them.
    Mdx,
    /// Strips Flow types and normalizes the module for everything downstream.
    Flow,
    /// Turns the `app/` tree into route modules and their Flow types.
    Router,
    /// Splits the server graph from the client bundle at `"use client"`.
    Rsc,
    /// Extracts StyleX styles into a static stylesheet.
    Style,
    /// Runs the React compiler over components and hooks.
    ReactCompiler,
    /// Lowers JSX to the React runtime's own calls.
    Jsx,
    /// Resolves, fingerprints, and emits non-JavaScript imports.
    Asset,
}

impl BuiltinPlugin {
    /// Every built-in, in pipeline order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Mdx,
        Self::Flow,
        Self::Router,
        Self::Rsc,
        Self::Style,
        Self::ReactCompiler,
        Self::Jsx,
        Self::Asset,
    ];

    /// How many built-ins exist. [`BuiltinSet`] needs at least this many bits.
    pub const COUNT: usize = 8;

    /// The plugin's name in the pipeline, and in `uf inspect --json`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mdx => "uf:mdx",
            Self::Flow => "uf:flow",
            Self::Router => "uf:router",
            Self::Rsc => "uf:rsc",
            Self::Style => "uf:style",
            Self::ReactCompiler => "uf:react-compiler",
            Self::Jsx => "uf:jsx",
            Self::Asset => "uf:asset",
        }
    }

    /// Position in [`BuiltinPlugin::ALL`], and the bit index in a [`BuiltinSet`].
    pub const fn index(self) -> usize {
        match self {
            Self::Mdx => 0,
            Self::Flow => 1,
            Self::Router => 2,
            Self::Rsc => 3,
            Self::Style => 4,
            Self::ReactCompiler => 5,
            Self::Jsx => 6,
            Self::Asset => 7,
        }
    }

    /// This built-in as a one-bit mask.
    pub const fn bit(self) -> u8 {
        1u8 << self.index()
    }

    /// Which band the stage runs in.
    ///
    /// Flow stripping and route generation are `Pre` because every other plugin
    /// expects to see plain JavaScript and to be able to import a generated
    /// route module.
    ///
    /// The last two are both `Post`, in that order, and the order is the one
    /// Babel uses. The React compiler reads components as their author wrote
    /// them, JSX included, so nothing may lower JSX before it runs; and
    /// lowering `<div/>` to `_jsx("div", {})` afterwards changes no data flow
    /// it reasoned about, so nothing it proved is invalidated by running
    /// second.
    pub const fn order(self) -> HookOrder {
        match self {
            Self::Mdx | Self::Flow | Self::Router => HookOrder::Pre,
            Self::Rsc | Self::Style | Self::Asset => HookOrder::Normal,
            Self::ReactCompiler | Self::Jsx => HookOrder::Post,
        }
    }

    /// Which hooks the stage implements.
    pub const fn hooks(self) -> HookSet {
        match self {
            Self::Mdx => HookSet::of(PluginHook::Transform),
            Self::Flow => HookSet::of(PluginHook::Transform).with(PluginHook::ModuleParsed),
            Self::Router => HookSet::of(PluginHook::BuildStart)
                .with(PluginHook::ResolveId)
                .with(PluginHook::Load)
                .with(PluginHook::ConfigureServer)
                .with(PluginHook::HandleHotUpdate),
            Self::Rsc => HookSet::of(PluginHook::ModuleParsed)
                .with(PluginHook::Transform)
                .with(PluginHook::BuildEnd)
                .with(PluginHook::GenerateBundle)
                .with(PluginHook::WriteBundle),
            Self::Style => HookSet::of(PluginHook::Transform)
                .with(PluginHook::RenderChunk)
                .with(PluginHook::GenerateBundle),
            Self::ReactCompiler => HookSet::of(PluginHook::Transform),
            Self::Jsx => HookSet::of(PluginHook::Transform),
            Self::Asset => HookSet::of(PluginHook::ResolveId)
                .with(PluginHook::Load)
                .with(PluginHook::GenerateBundle)
                .with(PluginHook::WriteBundle)
                .with(PluginHook::TransformIndexHtml),
        }
    }

    /// The descriptor the container places.
    ///
    /// Every built-in applies to both `uf build` and `uf dev`: the two differ in
    /// what a stage emits, not in whether it runs.
    pub fn descriptor(self) -> PluginDescriptor {
        PluginDescriptor::builtin(self.name(), self.order(), self.hooks())
    }
}

/// Which built-ins a project's config switches on.
///
/// A bitset rather than a struct of flags, so "is the router in this pipeline?"
/// is one AND and the whole selection stays compact.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuiltinSet(u16);

impl BuiltinSet {
    /// No built-ins.
    pub const EMPTY: Self = Self(0);

    /// Every built-in.
    pub const ALL: Self = Self((1u16 << BuiltinPlugin::COUNT) - 1);

    /// A set holding exactly `plugin`.
    pub const fn of(plugin: BuiltinPlugin) -> Self {
        Self(plugin.bit() as u16)
    }

    /// This set plus `plugin`.
    pub const fn with(self, plugin: BuiltinPlugin) -> Self {
        Self(self.0 | plugin.bit() as u16)
    }

    /// This set plus `plugin` when `enabled`, unchanged otherwise.
    pub const fn with_if(self, plugin: BuiltinPlugin, enabled: bool) -> Self {
        if enabled { self.with(plugin) } else { self }
    }

    /// This set without `plugin`.
    pub const fn without(self, plugin: BuiltinPlugin) -> Self {
        Self(self.0 & !(plugin.bit() as u16))
    }

    /// Whether `plugin` is in the set.
    pub const fn contains(self, plugin: BuiltinPlugin) -> bool {
        self.0 & plugin.bit() as u16 != 0
    }

    /// Whether the set holds nothing.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many built-ins are in the set.
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// The raw mask.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// The built-ins in the set, in [`BuiltinPlugin::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = BuiltinPlugin> {
        BuiltinPlugin::ALL
            .into_iter()
            .filter(move |plugin| self.contains(*plugin))
    }

    /// The built-ins a project's `uf.config.js` switches on.
    ///
    /// Flow stripping, JSX lowering and asset handling are unconditional: a uf
    /// project is Flow source with JSX that imports assets, so switching any of
    /// them off would not produce a working build. The rest follow the config fields users
    /// already have, which is why there is no second set of plugin toggles.
    pub fn from_config(config: &UniflowedConfig) -> Self {
        Self::of(BuiltinPlugin::Flow)
            .with_if(BuiltinPlugin::Mdx, config.app.builtins.markdown.mdx.enabled)
            .with(BuiltinPlugin::Jsx)
            .with(BuiltinPlugin::Asset)
            .with_if(BuiltinPlugin::Router, config.app.router.enabled)
            .with_if(BuiltinPlugin::Rsc, config.app.rsc)
            .with_if(
                BuiltinPlugin::Style,
                matches!(config.app.builtins.style, StyleEngine::StyleX),
            )
            .with_if(
                BuiltinPlugin::ReactCompiler,
                config.app.builtins.react_compiler.enabled
                    && matches!(
                        config.app.builtins.react_compiler.mode,
                        ReactCompilerMode::Syntax
                    ),
            )
    }

    /// The descriptors for every built-in in the set, in pipeline order.
    pub fn descriptors(self) -> Vec<PluginDescriptor> {
        self.iter().map(BuiltinPlugin::descriptor).collect()
    }
}

impl FromIterator<BuiltinPlugin> for BuiltinSet {
    fn from_iter<I: IntoIterator<Item = BuiltinPlugin>>(iter: I) -> Self {
        iter.into_iter().fold(Self::EMPTY, Self::with)
    }
}

impl Serialize for BuiltinSet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(BuiltinPlugin::name))
    }
}
