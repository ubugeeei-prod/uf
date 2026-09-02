use std::collections::BTreeMap;
use std::fmt;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
pub use uf_bundle::{BudgetMetric, BundleBudgets, ByteSize, SizeBudget};

pub mod plugins;

pub use plugins::{ApplyCondition, HookOrder, PipelineMode, PluginEntry, PluginSpec};

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
    pub flow_cell: bool,
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
            flow_cell: true,
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
    pub cache: CacheModeConfig,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/markdown"),
            engine: MarkdownEngineConfig::OxContentWasm,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RuntimeConfig {
    pub default: RuntimeEngine,
    pub compatibility: Vec<RuntimeEngine>,
    pub deploy: DeployAnywhereConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default: RuntimeEngine::Uf,
            compatibility: vec![
                RuntimeEngine::Node,
                RuntimeEngine::Bun,
                RuntimeEngine::Deno,
                RuntimeEngine::Edge,
                RuntimeEngine::Serverless,
                RuntimeEngine::Container,
            ],
            deploy: DeployAnywhereConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeEngine {
    #[default]
    Uf,
    Node,
    Bun,
    Deno,
    Edge,
    Serverless,
    Container,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DeployAnywhereConfig {
    pub enabled: bool,
    pub adapters: Vec<DeployAdapter>,
}

impl Default for DeployAnywhereConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            adapters: vec![
                DeployAdapter::Node,
                DeployAdapter::Bun,
                DeployAdapter::Deno,
                DeployAdapter::Edge,
                DeployAdapter::Serverless,
                DeployAdapter::Static,
                DeployAdapter::Container,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeployAdapter {
    Node,
    Bun,
    Deno,
    Edge,
    Serverless,
    Static,
    Container,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerConfig {
    pub engine: ServerEngine,
    pub native: NativeServerConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            engine: ServerEngine::NativeRust,
            native: NativeServerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerEngine {
    #[default]
    NativeRust,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NativeServerConfig {
    pub streaming: bool,
    pub zero_copy_http: bool,
    pub adapters: Vec<NativeServerAdapter>,
}

impl Default for NativeServerConfig {
    fn default() -> Self {
        Self {
            streaming: true,
            zero_copy_http: true,
            adapters: vec![
                NativeServerAdapter::Uf,
                NativeServerAdapter::Node,
                NativeServerAdapter::Bun,
                NativeServerAdapter::Deno,
                NativeServerAdapter::Edge,
                NativeServerAdapter::Serverless,
                NativeServerAdapter::Container,
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeServerAdapter {
    Uf,
    Node,
    Bun,
    Deno,
    Edge,
    Serverless,
    Container,
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
    pub quotes: QuoteStyle,
    pub semicolons: bool,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            line_width: 100,
            max_blank_lines: 1,
            quotes: QuoteStyle::Double,
            semicolons: true,
        }
    }
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
            version: CompactString::const_new("uf@0.1.0"),
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
    pub official_flow_parser: bool,
}

impl Default for NativeTestRunnerConfig {
    fn default() -> Self {
        Self {
            runtime: NativeTestRuntimeConfig::UfSelfHosted,
            scheduler: NativeTestSchedulerConfig::NativeWorkStealing,
            performance_target: NativeTestPerformanceTarget::FasterThanBun,
            official_flow_parser: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeTestRuntimeConfig {
    #[default]
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
pub struct LintConfig {
    pub files: Vec<CompactString>,
    pub ignore: Vec<CompactString>,
    pub rules: BTreeMap<CompactString, RuleLevel>,
}

/// Every lint rule `uf lint` ships, with the level uf applies out of the box.
///
/// `uf lint` is the union of Flow's built-in lint set (the `flow/` namespace) and
/// uf's own framework rules, so this table has to name both. Flow itself defaults
/// every built-in lint to `off`; uf does not, because a linter nobody switches on
/// catches nothing. The policy, applied per row below:
///
/// - `error` when the pattern is a bug, an unsound escape hatch, or a rule whose
///   fix is mechanical — the things a large Flow codebase cannot let accumulate.
/// - `warn` when the pattern is only suspicious, is legitimate in some code, or
///   when uf's current check covers a syntactic subset of what Flow checks.
/// - `off` only where leaving a rule on would report the same violation twice.
///
/// Each rule's full rationale, category, and one-line description live on its
/// `uf_lint::RuleDescriptor`; `uf_lint` has a test asserting this table and that
/// catalogue agree exactly, in both directions, so the two cannot drift apart.
const DEFAULT_LINT_RULES: [(&str, RuleLevel); 53] = [
    // --- Flow built-in lints ------------------------------------------------
    // Exactness must be stated, not inferred from a config flag.
    ("flow/ambiguous-object-type", RuleLevel::Error),
    // Reading named exports off a default import is a CommonJS interop bug.
    ("flow/default-import-access", RuleLevel::Error),
    // `bool` is a legacy alias for `boolean`; mechanical fix.
    ("flow/deprecated-type", RuleLevel::Error),
    // Legal, just confusing.
    ("flow/export-renamed-default", RuleLevel::Warn),
    // Flow's internal types are unstable across releases.
    ("flow/internal-type", RuleLevel::Error),
    // A namespace object is not a value.
    ("flow/invalid-import-star-use", RuleLevel::Error),
    // Rebinding a method to a foreign receiver is unsound.
    ("flow/invalid-this-arg", RuleLevel::Error),
    // Shadowing a builtin libdef breaks every consumer at once.
    ("flow/libdef-override", RuleLevel::Error),
    // uf ships ESM; mixing module systems defeats static analysis.
    ("flow/mixed-import-and-require", RuleLevel::Error),
    // A nested component remounts its whole subtree every render.
    ("flow/nested-component", RuleLevel::Error),
    // A nested hook gets a new identity every render.
    ("flow/nested-hook", RuleLevel::Error),
    // A mutable export is a live binding consumers cannot reason about.
    ("flow/non-const-var-export", RuleLevel::Error),
    // Only actionable mid-migration, so it must not block one.
    ("flow/nonstrict-import", RuleLevel::Warn),
    // Shadowing `div`/`span` silently changes what JSX means.
    ("flow/react-intrinsic-overlap", RuleLevel::Error),
    // The explicit form is verbose; the implicit form is not itself a bug.
    ("flow/require-explicit-enum-checks", RuleLevel::Warn),
    ("flow/require-explicit-enum-switch-cases", RuleLevel::Warn),
    // The most common Flow-catchable production bug: `if (count)` skipping 0.
    ("flow/sketchy-null", RuleLevel::Error),
    // The typed variants stay off so one violation is not reported twice.
    ("flow/sketchy-null-bigint", RuleLevel::Off),
    ("flow/sketchy-null-bool", RuleLevel::Off),
    ("flow/sketchy-null-mixed", RuleLevel::Off),
    ("flow/sketchy-null-number", RuleLevel::Off),
    ("flow/sketchy-null-string", RuleLevel::Off),
    // `{count && <List />}` renders a literal `0`; user-visible bug.
    ("flow/sketchy-number", RuleLevel::Error),
    // Legal in methods, so warn rather than block.
    ("flow/this-in-exported-function", RuleLevel::Warn),
    // `any`/`Object`/`Function` switch the type checker off.
    ("flow/unclear-type", RuleLevel::Error),
    // Reading a field before the constructor finishes yields `undefined`.
    ("flow/uninitialized-instance-property", RuleLevel::Error),
    // Dead code, not a bug.
    ("flow/unnecessary-invariant", RuleLevel::Warn),
    // uf only sees the syntactic subset today.
    ("flow/unnecessary-optional-chain", RuleLevel::Warn),
    // Accessors hide side effects, but are legitimate in some UI code.
    ("flow/unsafe-getters-setters", RuleLevel::Warn),
    // `Object.assign` mutates its target and is unsound in Flow.
    ("flow/unsafe-object-assign", RuleLevel::Error),
    // An untyped dependency turns everything it exports into `any`.
    ("flow/untyped-import", RuleLevel::Error),
    ("flow/untyped-type-import", RuleLevel::Error),
    // A floating promise swallows rejections and loses ordering.
    ("flow/unused-promise", RuleLevel::Error),
    // --- uf's own rules -----------------------------------------------------
    // A file that does not parse cannot be checked at all.
    ("flow/syntax", RuleLevel::Error),
    ("uniflowed/no-tabs", RuleLevel::Error),
    ("uniflowed/no-trailing-whitespace", RuleLevel::Error),
    // Tasks belong in uf.config.js, never in a shelled-out package manager.
    ("uniflowed/no-npm-script-invocation", RuleLevel::Error),
    // A typo'd suppression silently stops enforcing a rule.
    ("uniflowed/unknown-lint-suppression", RuleLevel::Error),
    // Style preferences during the migration to Flow component/hook syntax.
    ("react/component-syntax", RuleLevel::Warn),
    ("react/hook-syntax", RuleLevel::Warn),
    // Breaking the rules of hooks corrupts React's hook state.
    ("react/hooks-rules", RuleLevel::Error),
    // Framework routes are wired by name; `warn` while the scaffold migrates.
    ("react/no-default-export-component", RuleLevel::Warn),
    // Non-idempotent render breaks streaming SSR and hydration.
    ("react/no-render-side-effects", RuleLevel::Error),
    // Platform branches are a preference, not a correctness problem.
    ("react-native/platform-split", RuleLevel::Warn),
    // Leaking a secret into a client bundle is unrecoverable.
    ("server/no-client-secret", RuleLevel::Error),
    ("server/no-server-only-import-in-client", RuleLevel::Error),
    // A misplaced directive is silently ignored; Next.js has shipped this bug.
    ("server/use-client-directive-position", RuleLevel::Error),
    ("server/use-server-actions", RuleLevel::Error),
    ("router/reserved-files", RuleLevel::Error),
    ("package/no-npm-scripts", RuleLevel::Error),
    ("fetch/no-global-override", RuleLevel::Error),
    // XSS and arbitrary code execution: never a warning.
    ("security/no-dangerously-set-inner-html", RuleLevel::Error),
    ("security/no-eval", RuleLevel::Error),
];

impl Default for LintConfig {
    fn default() -> Self {
        let mut rules = BTreeMap::new();
        for (rule, level) in DEFAULT_LINT_RULES {
            rules.insert(CompactString::const_new(rule), level);
        }

        Self {
            files: vec![
                CompactString::const_new("app"),
                CompactString::const_new("npm"),
                CompactString::const_new("server"),
                CompactString::const_new("tests"),
            ],
            ignore: vec![
                CompactString::const_new("node_modules"),
                CompactString::const_new("dist"),
                CompactString::const_new("target"),
            ],
            rules,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
    Off,
    Warn,
    Error,
}

impl RuleLevel {
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    pub fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

impl<'de> Deserialize<'de> for RuleLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = RuleLevel;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("\"off\", \"warn\", \"error\", false, true, 0, 1, or 2")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(if value {
                    RuleLevel::Error
                } else {
                    RuleLevel::Off
                })
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    0 => Ok(RuleLevel::Off),
                    1 => Ok(RuleLevel::Warn),
                    2 => Ok(RuleLevel::Error),
                    _ => Err(E::custom(format!("unsupported rule level {value}"))),
                }
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "off" => Ok(RuleLevel::Off),
                    "warn" | "warning" => Ok(RuleLevel::Warn),
                    "error" => Ok(RuleLevel::Error),
                    _ => Err(E::custom(format!("unsupported rule level {value:?}"))),
                }
            }
        }

        deserializer.deserialize_any(Visitor)
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
            command: CompactString::const_new("uf release minor"),
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
