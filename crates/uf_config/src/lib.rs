use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
pub use uf_bundle::{BudgetMetric, BundleBudgets, ByteSize, SizeBudget};

mod app;
mod lint;
pub mod plugins;
mod runtime;

pub use app::{
    AppConfig, BuiltinConfig, CacheConfig, CacheModeConfig, ComponentBoundary, DataEngine,
    EffectEngine, FetchConfig, FrameworkPreset, GraphQlConfig, LinkPrefetchMode, LoaderConfig,
    MarkdownConfig, MarkdownEngineConfig, MdxConfig, MdxPipelinePluginConfig, MotionConfig,
    MotionEngineConfig, OrmConfig, PwaConfig, ReactCompilerConfig, ReactCompilerImplementation,
    ReactCompilerMode, ReactConfig, RenderingConfig, RenderingMode, RouterConfig, RouterConvention,
    RuntimeTarget, StyleEngine, TemporalConfig, TuiConfig, TuiStandardConfig, WebConfig,
};
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
/// allow. Both are handed to Vite's `server.fs` unchanged, on top of Vite's
/// own built-in deny list (`.env`, `.env.*`, `*.{crt,pem}`, `**/.git/**`).
/// See `docs/security.md`.
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
    ViteTask,
    #[default]
    CapabilityJsHost,
    UfSelfHosted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeTestSchedulerConfig {
    ViteTaskCache,
    #[default]
    NativeWorkStealing,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeTestPerformanceTarget {
    ViteTask,
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
            engine: TaskRunnerEngine::ViteTask,
            allow_package_scripts: false,
        }
    }
}

/// Which runner executes `uf.config.js` tasks.
///
/// `uf` delegates this surface to Vite Task so package scripts and task graphs
/// share the upstream Rust scheduler while the rest of uf stays runtime
/// agnostic. No alias is kept for the old spelling: a name a user can still
/// write is still a name they can see, which is the thing being removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum TaskRunnerEngine {
    /// Vite+'s Rust task runner, invoked through the public `vp run` interface.
    #[default]
    ViteTask,
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
