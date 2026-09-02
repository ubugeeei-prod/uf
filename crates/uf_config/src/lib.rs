use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
pub use uf_bundle::{BudgetMetric, BundleBudgets, ByteSize, SizeBudget};

mod lint;
pub mod plugins;
mod runtime;

pub use lint::{
    FlowBuiltinLintMode, FlowLintConfig, FlowLintParser, LintConfig, LintEngine, RuleLevel,
};
pub use plugins::{ApplyCondition, HookOrder, PipelineMode, PluginEntry, PluginSpec};
pub use runtime::{
    CapabilityJsHost, CapabilityJsHostConfig, DeployAdapter, DeployAnywhereConfig,
    NativeServerAdapter, NativeServerConfig, RuntimeConfig, RuntimeEngine, ServerConfig,
    ServerEngine,
};

pub const CONFIG_FILES: &[&str] = &["uf.config.js"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct UniflowedConfig {
    pub app: AppConfig,
    pub build: BuildConfig,
    pub dev: DevConfig,
    pub docs: DocsConfig,
    pub env: EnvConfig,
    pub fmt: FmtConfig,
    pub lint: LintConfig,
    pub package: PackageConfig,
    /// Plugins the project adds, in declaration order.
    ///
    /// Entries are raw, untrusted declarations; `uf_plugin` resolves them into
    /// a run order and rejects any that reach outside the project root.
    pub plugins: Vec<PluginEntry>,
    pub pm: PackageManagerConfig,
    pub publish: PublishConfig,
    pub release: ReleaseConfig,
    pub rm: RuntimeManagerConfig,
    pub server: ServerConfig,
    pub std: StdConfig,
    pub story: StoryConfig,
    pub task_runner: TaskRunnerConfig,
    pub tasks: BTreeMap<CompactString, TaskDefinition>,
    pub test: TestConfig,
    pub vrt: VrtConfig,
}

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
    pub mode: ReactCompilerMode,
}

impl Default for ReactCompilerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: ReactCompilerMode::Syntax,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct BuildConfig {
    pub budgets: BundleBudgets,
    pub entries: Vec<CompactString>,
    pub hooks: BTreeMap<CompactString, TaskDefinition>,
    pub out_dir: CompactString,
    pub static_build: bool,
    pub sourcemap: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            // Budgets stay unset by default: failing a build nobody asked us to
            // police is worse than reporting and moving on.
            budgets: BundleBudgets::default(),
            entries: vec![CompactString::const_new("app.js")],
            hooks: BTreeMap::new(),
            out_dir: CompactString::const_new("dist"),
            static_build: false,
            sourcemap: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DocsConfig {
    pub enabled: bool,
    pub app: CompactString,
    pub source: CompactString,
    pub out_dir: CompactString,
    pub static_build: bool,
    pub deploy: DeployTarget,
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            app: CompactString::const_new("docs/app.js"),
            source: CompactString::const_new("docs"),
            out_dir: CompactString::const_new("dist/docs"),
            static_build: true,
            deploy: DeployTarget::Void,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeployTarget {
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DevConfig {
    pub host: CompactString,
    pub port: u16,
    pub strict_port: bool,
    pub fs: DevFsConfig,
    pub allowed_hosts: Vec<CompactString>,
    pub allowed_origins: Vec<CompactString>,
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            host: CompactString::const_new("127.0.0.1"),
            port: 5173,
            strict_port: false,
            fs: DevFsConfig::default(),
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
        }
    }
}

/// Which files the dev server may serve.
///
/// `allow` names extra roots beyond the project root, which is always allowed.
/// `deny` is a glob list evaluated on the canonical path, and deny wins over
/// allow. Entries are *added* to `uf_devserver`'s built-in deny list — which
/// already covers `.env*`, `**/.git/**`, `*.pem`, `*.key`, `*.crt`, and
/// `**/.uf/**` — and cannot remove one. See `docs/security.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DevFsConfig {
    pub allow: Vec<CompactString>,
    pub deny: Vec<CompactString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct EnvConfig {
    pub active: CompactString,
    pub files: Vec<CompactString>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            active: CompactString::const_new("development"),
            files: vec![
                CompactString::const_new(".env"),
                CompactString::const_new(".env.local"),
                CompactString::const_new(".env.development"),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FmtConfig {
    pub indent_width: u8,
    pub line_width: u16,
    pub max_blank_lines: u8,
    pub flow: FlowFormatConfig,
    pub non_flow: NonFlowFormatConfig,
    pub quotes: QuoteStyle,
    pub semicolons: bool,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            line_width: 100,
            max_blank_lines: 1,
            flow: FlowFormatConfig::default(),
            non_flow: NonFlowFormatConfig::default(),
            quotes: QuoteStyle::Double,
            semicolons: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FlowFormatConfig {
    pub parser: FlowFormatParser,
    pub printer: FlowFormatPrinter,
}

impl Default for FlowFormatConfig {
    fn default() -> Self {
        Self {
            parser: FlowFormatParser::OfficialFlowRust,
            printer: FlowFormatPrinter::UfRust,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFormatParser {
    #[default]
    OfficialFlowRust,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowFormatPrinter {
    #[default]
    UfRust,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NonFlowFormatConfig {
    pub formatter: NonFlowFormatter,
}

impl Default for NonFlowFormatConfig {
    fn default() -> Self {
        Self {
            formatter: NonFlowFormatter::Biome,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NonFlowFormatter {
    #[default]
    Biome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuoteStyle {
    Single,
    Double,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PackageConfig {
    pub generator: PackageGenerator,
    pub targets: Vec<PackageTarget>,
    pub typescript_declarations_to_flow: bool,
}

impl Default for PackageConfig {
    fn default() -> Self {
        Self {
            generator: PackageGenerator::NapiRs,
            targets: vec![
                PackageTarget::NodeNapi,
                PackageTarget::BunNapi,
                PackageTarget::DenoNapi,
                PackageTarget::EdgeWasm,
                PackageTarget::ServerlessNapi,
            ],
            typescript_declarations_to_flow: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageGenerator {
    #[default]
    NapiRs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageTarget {
    NodeNapi,
    BunNapi,
    DenoNapi,
    EdgeWasm,
    ServerlessNapi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PackageManagerConfig {
    pub module: CompactString,
    pub resolver: PackageManagerResolver,
    pub lockfile: CompactString,
    pub store_dir: CompactString,
    pub allow_lifecycle_scripts: bool,
    pub package_manager: PackageManagerPreference,
}

impl Default for PackageManagerConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/pm"),
            resolver: PackageManagerResolver::UfNative,
            lockfile: CompactString::const_new("uf.lock"),
            store_dir: CompactString::const_new(".uf/store"),
            allow_lifecycle_scripts: false,
            package_manager: PackageManagerPreference::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManagerResolver {
    #[default]
    UfNative,
}

/// Which package manager drives the project, overriding auto-inference.
///
/// `Auto` infers the manager from the project itself: an explicit
/// `"packageManager"` field, then a lockfile, then the nearest workspace root,
/// then uf's own resolver. `Yarn` means the modern Berry line; pin `YarnClassic`
/// for Yarn 1.x.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManagerPreference {
    #[default]
    Auto,
    Uf,
    Npm,
    Pnpm,
    Yarn,
    YarnClassic,
    YarnBerry,
    Bun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RuntimeManagerConfig {
    pub module: CompactString,
    pub infer_from_config: bool,
    pub version: CompactString,
    pub auto_switch: bool,
    pub acquisition: RuntimeManagerAcquisition,
    pub apply: RuntimeManagerApply,
    pub doctor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct StdConfig {
    pub module: CompactString,
    pub wintertc_aligned: bool,
    pub native_bindings: bool,
    pub modules: Vec<StdModuleConfig>,
}

impl Default for StdConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/std"),
            wintertc_aligned: true,
            native_bindings: true,
            modules: vec![
                StdModuleConfig::Vfs,
                StdModuleConfig::Fs,
                StdModuleConfig::Types,
                StdModuleConfig::Pipeline,
                StdModuleConfig::Effect,
                StdModuleConfig::Env,
                StdModuleConfig::Format,
                StdModuleConfig::Stdio,
                StdModuleConfig::Hash,
                StdModuleConfig::Debug,
                StdModuleConfig::Defs,
                StdModuleConfig::Lock,
                StdModuleConfig::Colors,
                StdModuleConfig::Qs,
                StdModuleConfig::Equality,
                StdModuleConfig::Http,
                StdModuleConfig::Buffer,
                StdModuleConfig::Ws,
                StdModuleConfig::Sql,
                StdModuleConfig::Json,
                StdModuleConfig::Yaml,
                StdModuleConfig::Toml,
                StdModuleConfig::Collections,
                StdModuleConfig::Crypto,
                StdModuleConfig::Dotenv,
                StdModuleConfig::Math,
                StdModuleConfig::Os,
                StdModuleConfig::Net,
                StdModuleConfig::Dns,
                StdModuleConfig::Path,
                StdModuleConfig::Stream,
                StdModuleConfig::Url,
                StdModuleConfig::Wasm,
                StdModuleConfig::Glob,
                StdModuleConfig::Motion,
                StdModuleConfig::Tui,
                StdModuleConfig::Cron,
                StdModuleConfig::S3,
                StdModuleConfig::Sigv4,
                StdModuleConfig::Functions,
                StdModuleConfig::Uuid,
                StdModuleConfig::Zip,
                StdModuleConfig::ImportMeta,
                StdModuleConfig::Defer,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StdModuleConfig {
    Vfs,
    Fs,
    Types,
    Pipeline,
    Effect,
    Env,
    Format,
    Stdio,
    Hash,
    Debug,
    Defs,
    Lock,
    Colors,
    Qs,
    Equality,
    Http,
    Buffer,
    Ws,
    Sql,
    Json,
    Yaml,
    Toml,
    Collections,
    Crypto,
    Dotenv,
    Math,
    Os,
    Net,
    Dns,
    Path,
    Stream,
    Url,
    Wasm,
    Glob,
    Motion,
    Tui,
    Cron,
    S3,
    Sigv4,
    Functions,
    Uuid,
    Zip,
    ImportMeta,
    Defer,
}

impl Default for RuntimeManagerConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/rm"),
            infer_from_config: true,
            version: CompactString::const_new("node@system"),
            auto_switch: true,
            acquisition: RuntimeManagerAcquisition::Auto,
            apply: RuntimeManagerApply::ConfigAndHost,
            doctor: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeManagerAcquisition {
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeManagerApply {
    #[default]
    ConfigAndHost,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct StoryConfig {
    pub enabled: bool,
    pub module: CompactString,
    pub mocks: MockConfig,
    pub browser: BrowserAutomationConfig,
}

impl Default for StoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            module: CompactString::const_new("@uniflowed/story"),
            mocks: MockConfig::default(),
            browser: BrowserAutomationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct MockConfig {
    pub module: CompactString,
    pub msw_compatible: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/mock"),
            msw_compatible: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct BrowserAutomationConfig {
    pub module: CompactString,
    pub playwright_compatible: bool,
}

impl Default for BrowserAutomationConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/browser"),
            playwright_compatible: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct VrtConfig {
    pub enabled: bool,
    pub module: CompactString,
    pub baselines: CompactString,
    pub threshold: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TestConfig {
    pub module: CompactString,
    pub runner: NativeTestRunnerConfig,
    pub react_testing_library_native: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/test"),
            runner: NativeTestRunnerConfig::default(),
            react_testing_library_native: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NativeTestRunnerConfig {
    pub runtime: NativeTestRuntimeConfig,
    pub scheduler: NativeTestSchedulerConfig,
    pub performance_target: NativeTestPerformanceTarget,
    pub js_hosts: Vec<CapabilityJsHost>,
    pub official_flow_parser: bool,
}

impl Default for NativeTestRunnerConfig {
    fn default() -> Self {
        Self {
            runtime: NativeTestRuntimeConfig::CapabilityJsHost,
            scheduler: NativeTestSchedulerConfig::NativeWorkStealing,
            performance_target: NativeTestPerformanceTarget::FasterThanBun,
            js_hosts: vec![
                CapabilityJsHost::Node,
                CapabilityJsHost::Deno,
                CapabilityJsHost::Bun,
            ],
            official_flow_parser: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeTestRuntimeConfig {
    #[default]
    CapabilityJsHost,
    UfSelfHosted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeTestSchedulerConfig {
    #[default]
    NativeWorkStealing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeTestPerformanceTarget {
    #[default]
    FasterThanBun,
}

impl Default for VrtConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            module: CompactString::const_new("@uniflowed/vrt"),
            baselines: CompactString::const_new("__uf_vrt__"),
            threshold: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PublishConfig {
    pub registry: CompactString,
    pub dry_run: bool,
    pub first_publish: FirstPublishConfig,
    pub trusted_publish: TrustedPublishConfig,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            registry: CompactString::const_new("https://registry.npmjs.org"),
            dry_run: true,
            first_publish: FirstPublishConfig::default(),
            trusted_publish: TrustedPublishConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FirstPublishConfig {
    pub mode: FirstPublishMode,
    pub local_bootstrap: bool,
}

impl Default for FirstPublishConfig {
    fn default() -> Self {
        Self {
            mode: FirstPublishMode::Local,
            local_bootstrap: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FirstPublishMode {
    #[default]
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TrustedPublishConfig {
    pub enabled: bool,
    pub provider: TrustedPublishProvider,
    pub tokenless: bool,
    pub trigger: TrustedPublishTrigger,
}

impl Default for TrustedPublishConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: TrustedPublishProvider::GitHubActionsOidc,
            tokenless: true,
            trigger: TrustedPublishTrigger::TagPush,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustedPublishProvider {
    #[default]
    #[serde(rename = "github-actions-oidc")]
    GitHubActionsOidc,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustedPublishTrigger {
    #[default]
    TagPush,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReleaseConfig {
    pub tag_prefix: CompactString,
    pub command: CompactString,
    pub publish: bool,
}

impl Default for ReleaseConfig {
    fn default() -> Self {
        Self {
            tag_prefix: CompactString::const_new("uf@"),
            command: CompactString::const_new("uf release alpha"),
            publish: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TaskRunnerConfig {
    pub engine: TaskRunnerEngine,
    pub allow_package_scripts: bool,
}

impl Default for TaskRunnerConfig {
    fn default() -> Self {
        Self {
            engine: TaskRunnerEngine::UfTask,
            allow_package_scripts: false,
        }
    }
}

/// Which runner executes `uf.config.js` tasks.
///
/// `uf` matches the task semantics of the wider ecosystem so existing task
/// definitions keep working, but a developer using `uf` never chose an
/// underlying runner and should not have to reason about one. No alias is kept
/// for the old spelling: a name a user can still write is still a name they can
/// see, which is the thing being removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TaskRunnerEngine {
    /// uf's own task runner.
    #[default]
    UfTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskDefinition {
    Command(CompactString),
    Detailed(TaskCommand),
}

impl TaskDefinition {
    pub fn command(&self) -> &str {
        match self {
            Self::Command(command) => command.as_str(),
            Self::Detailed(task) => task.command.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TaskCommand {
    pub command: CompactString,
    pub cwd: Option<CompactString>,
    pub depends_on: Vec<CompactString>,
    pub env: BTreeMap<CompactString, CompactString>,
}

impl Default for TaskCommand {
    fn default() -> Self {
        Self {
            command: CompactString::new(""),
            cwd: None,
            depends_on: Vec::new(),
            env: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedConfig {
    pub root: Utf8PathBuf,
    pub config_path: Option<Utf8PathBuf>,
    pub config: UniflowedConfig,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {message}")]
    Parse { path: Utf8PathBuf, message: String },
    #[error(
        "unsupported config expression in {path}; use `export default defineConfig({{ ... }})`"
    )]
    UnsupportedExpression { path: Utf8PathBuf },
}

pub fn load_config(start: impl AsRef<Utf8Path>) -> Result<ResolvedConfig, ConfigError> {
    let start = start.as_ref();
    let root = discover_root(start);
    let config_path = discover_config(&root);

    let config = match &config_path {
        Some(path) => load_config_file(path)?,
        None => UniflowedConfig::default(),
    };

    Ok(ResolvedConfig {
        root,
        config_path,
        config,
    })
}

pub fn discover_root(start: &Utf8Path) -> Utf8PathBuf {
    let mut current = if start.is_file() {
        start.parent().unwrap_or(start).to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        if CONFIG_FILES.iter().any(|name| current.join(name).exists())
            || current.join("package.json").exists()
            || current.join(".git").exists()
        {
            return current;
        }

        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return start.to_path_buf(),
        }
    }
}

pub fn discover_config(root: &Utf8Path) -> Option<Utf8PathBuf> {
    CONFIG_FILES
        .iter()
        .map(|file| root.join(file))
        .find(|path| path.exists())
}

pub fn load_config_file(path: &Utf8Path) -> Result<UniflowedConfig, ConfigError> {
    let source = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    match path.extension() {
        Some("js" | "mjs" | "cjs" | "flow") => {
            let json5 = extract_config_object(&source).ok_or_else(|| {
                ConfigError::UnsupportedExpression {
                    path: path.to_path_buf(),
                }
            })?;
            json5::from_str(&json5).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                message: source.to_string(),
            })
        }
        _ => Err(ConfigError::UnsupportedExpression {
            path: path.to_path_buf(),
        }),
    }
}

pub fn extract_config_object(source: &str) -> Option<String> {
    let without_imports = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("import "))
        .collect::<Vec<_>>()
        .join("\n");
    let expression = strip_leading_comments(without_imports.trim())
        .trim_end_matches(';')
        .trim();
    let expression = expression
        .strip_prefix("export default")
        .map(str::trim)
        .unwrap_or(expression);

    if expression.starts_with("defineConfig") {
        let open = expression.find('(')?;
        let call = extract_balanced(&expression[open..], '(', ')')?;
        let inner = &call[1..call.len() - 1];
        let inner = inner.trim();
        if inner.starts_with('{') {
            return extract_balanced(inner, '{', '}');
        }
        return None;
    }

    if expression.starts_with('{') {
        return extract_balanced(expression, '{', '}');
    }

    None
}

fn strip_leading_comments(mut source: &str) -> &str {
    loop {
        source = source.trim_start();
        if let Some(rest) = source.strip_prefix("//") {
            source = rest
                .split_once('\n')
                .map(|(_, rest)| rest)
                .unwrap_or_default();
            continue;
        }
        if let Some(rest) = source.strip_prefix("/*") {
            let Some((_, rest)) = rest.split_once("*/") else {
                return "";
            };
            source = rest;
            continue;
        }
        return source;
    }
}

fn extract_balanced(source: &str, open: char, close: char) -> Option<String> {
    let mut chars = source.char_indices();
    let (_, first) = chars.next()?;
    if first != open {
        return None;
    }

    let mut depth = 1usize;
    let mut string_quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut previous = '\0';

    for (index, ch) in chars {
        if line_comment {
            if ch == '\n' {
                line_comment = false;
            }
            previous = ch;
            continue;
        }

        if block_comment {
            if previous == '*' && ch == '/' {
                block_comment = false;
            }
            previous = ch;
            continue;
        }

        if let Some(quote) = string_quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                string_quote = None;
            }
            previous = ch;
            continue;
        }

        match ch {
            '"' | '\'' | '`' => string_quote = Some(ch),
            '/' if previous == '/' => line_comment = true,
            '*' if previous == '/' => block_comment = true,
            current if current == open => depth += 1,
            current if current == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(source[..=index].to_string());
                }
            }
            _ => {}
        }
        previous = ch;
    }

    None
}

pub fn define_config(config: UniflowedConfig) -> UniflowedConfig {
    config
}

#[cfg(test)]
mod tests;
