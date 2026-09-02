use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::runtime::RuntimeConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct AppConfig {
    pub component_default: ComponentBoundary,
    pub framework: FrameworkPreset,
    pub react: ReactConfig,
    pub rendering: RenderingConfig,
    pub router: RouterConfig,
    pub runtime: RuntimeConfig,
    pub rsc: bool,
    pub server_actions: bool,
    pub orm: OrmConfig,
    pub builtins: BuiltinConfig,
    pub targets: Vec<RuntimeTarget>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            component_default: ComponentBoundary::Server,
            framework: FrameworkPreset::Uniflowed,
            react: ReactConfig::default(),
            rendering: RenderingConfig::default(),
            router: RouterConfig::default(),
            runtime: RuntimeConfig::default(),
            rsc: true,
            server_actions: true,
            orm: OrmConfig::default(),
            builtins: BuiltinConfig::default(),
            targets: vec![
                RuntimeTarget::Web,
                RuntimeTarget::ReactNative,
                RuntimeTarget::Server,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrameworkPreset {
    Uniflowed,
    React,
    ReactNative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RouterConfig {
    pub enabled: bool,
    pub entry: CompactString,
    pub manifest: CompactString,
    pub root: CompactString,
    pub convention: RouterConvention,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            entry: CompactString::const_new("app.js"),
            manifest: CompactString::const_new("router.js"),
            root: CompactString::const_new("app"),
            convention: RouterConvention::FileSystem,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouterConvention {
    FileSystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct OrmConfig {
    pub enabled: bool,
    pub module: CompactString,
    pub native: bool,
    pub generated_flow_types: bool,
    pub prepared_by_default: bool,
}

impl Default for OrmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            module: CompactString::const_new("@uniflowed/orm"),
            native: true,
            generated_flow_types: true,
            prepared_by_default: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct BuiltinConfig {
    pub data: DataEngine,
    pub effect: EffectEngine,
    pub fetch: FetchConfig,
    pub cell: bool,
    pub framework_lints: bool,
    pub graphql: GraphQlConfig,
    pub loader: LoaderConfig,
    pub markdown: MarkdownConfig,
    pub motion: MotionConfig,
    pub native_test_runner: bool,
    pub pwa: PwaConfig,
    pub react_compiler: ReactCompilerConfig,
    pub react_testing_library: bool,
    pub relay: bool,
    pub style: StyleEngine,
    pub temporal: TemporalConfig,
    pub tui: TuiConfig,
    pub web: WebConfig,
}

impl Default for BuiltinConfig {
    fn default() -> Self {
        Self {
            data: DataEngine::UniflowedQuery,
            effect: EffectEngine::UniflowedEffect,
            fetch: FetchConfig::default(),
            cell: true,
            framework_lints: true,
            graphql: GraphQlConfig::default(),
            loader: LoaderConfig::default(),
            markdown: MarkdownConfig::default(),
            motion: MotionConfig::default(),
            native_test_runner: true,
            pwa: PwaConfig::default(),
            react_compiler: ReactCompilerConfig::default(),
            react_testing_library: true,
            relay: true,
            style: StyleEngine::StyleX,
            temporal: TemporalConfig::default(),
            tui: TuiConfig::default(),
            web: WebConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StyleEngine {
    StyleX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DataEngine {
    UniflowedQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectEngine {
    UniflowedEffect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FetchConfig {
    pub module: CompactString,
    pub override_global_fetch: bool,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/fetch"),
            override_global_fetch: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct GraphQlConfig {
    pub module: CompactString,
    pub relay_base: bool,
}

impl Default for GraphQlConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/graphql"),
            relay_base: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct LoaderConfig {
    pub module: CompactString,
    pub state_module: CompactString,
    pub cache: CacheModeConfig,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/loader"),
            state_module: CompactString::const_new("@uniflowed/state"),
            cache: CacheModeConfig::OptIn,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct WebConfig {
    pub module: CompactString,
    pub typed_routes: bool,
    pub link_prefetch: LinkPrefetchMode,
    pub cache: CacheModeConfig,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/web"),
            typed_routes: true,
            link_prefetch: LinkPrefetchMode::Intent,
            cache: CacheModeConfig::OptIn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkPrefetchMode {
    Off,
    Intent,
    Render,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct MarkdownConfig {
    pub module: CompactString,
    pub engine: MarkdownEngineConfig,
    pub mdx: MdxConfig,
    pub cache: CacheModeConfig,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/markdown"),
            engine: MarkdownEngineConfig::OxContentWasm,
            mdx: MdxConfig::default(),
            cache: CacheModeConfig::OptIn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarkdownEngineConfig {
    OxContentWasm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct MdxConfig {
    pub enabled: bool,
    pub extensions: Vec<CompactString>,
    pub jsx_import_source: CompactString,
    pub pipeline_plugin: MdxPipelinePluginConfig,
}

impl Default for MdxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extensions: vec![CompactString::const_new(".mdx")],
            jsx_import_source: CompactString::const_new("@uniflowed/jsx-runtime"),
            pipeline_plugin: MdxPipelinePluginConfig::BuiltIn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MdxPipelinePluginConfig {
    BuiltIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct MotionConfig {
    pub module: CompactString,
    pub engine: MotionEngineConfig,
    pub compiler_safe: bool,
    pub server_component_safe: bool,
    pub reduced_motion_default: bool,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/motion"),
            engine: MotionEngineConfig::UfNative,
            compiler_safe: true,
            server_component_safe: true,
            reduced_motion_default: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MotionEngineConfig {
    #[default]
    UfNative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TuiConfig {
    pub module: CompactString,
    pub std_module: CompactString,
    pub standard: TuiStandardConfig,
    pub native_renderer: bool,
    pub beat_react_ink: bool,
    pub rich_media: bool,
    pub in_memory_tests: bool,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/tui"),
            std_module: CompactString::const_new("@uniflowed/std/tui"),
            standard: TuiStandardConfig::OpenTui,
            native_renderer: true,
            beat_react_ink: true,
            rich_media: true,
            in_memory_tests: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiStandardConfig {
    #[default]
    OpenTui,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TemporalConfig {
    pub module: CompactString,
    pub lite: bool,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/temporal"),
            lite: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PwaConfig {
    pub module: CompactString,
    pub enabled_by_default: bool,
    pub cache: CacheModeConfig,
}

impl Default for PwaConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/pwa"),
            enabled_by_default: false,
            cache: CacheModeConfig::OptIn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheModeConfig {
    OptIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReactCompilerConfig {
    pub enabled: bool,
    pub implementation: ReactCompilerImplementation,
    pub mode: ReactCompilerMode,
}

impl Default for ReactCompilerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            implementation: ReactCompilerImplementation::OfficialRust,
            mode: ReactCompilerMode::Syntax,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactCompilerImplementation {
    OfficialRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactCompilerMode {
    Syntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeTarget {
    Web,
    ReactNative,
    Server,
    Hermes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ComponentBoundary {
    Server,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReactConfig {
    pub version: CompactString,
    pub async_react: bool,
    pub suspense: bool,
    pub use_hook: bool,
}

impl Default for ReactConfig {
    fn default() -> Self {
        Self {
            version: CompactString::const_new("19"),
            async_react: true,
            suspense: true,
            use_hook: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RenderingConfig {
    pub modes: Vec<RenderingMode>,
    pub cache: CacheConfig,
}

impl Default for RenderingConfig {
    fn default() -> Self {
        Self {
            modes: vec![
                RenderingMode::Ppr,
                RenderingMode::Ssr,
                RenderingMode::Ssg,
                RenderingMode::Isr,
            ],
            cache: CacheConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingMode {
    Ppr,
    Ssr,
    Ssg,
    Isr,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CacheConfig {
    pub actions: bool,
    pub data: bool,
    pub fetch: bool,
    pub route: bool,
}
