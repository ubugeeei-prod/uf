use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
pub use uf_assets::{FontsConfig, IconsConfig, ImagesConfig, OgConfig};
pub use uf_bundle::{BudgetMetric, BundleBudgets, ByteSize, SizeBudget};
pub use uf_runtime::{Permission, PermissionError, Permissions, ToolchainAccess};

mod app;
pub mod env_files;
mod library;
mod lint;
pub mod plugins;
mod rendering;
mod runtime;

pub use app::{
    AppConfig, BuiltinConfig, CacheConfig, CacheModeConfig, ComponentBoundary, DataEngine,
    EffectEngine, FetchConfig, FrameworkPreset, GraphQlConfig, HighlightConfig, HighlightThemes,
    LinkPrefetchMode, LoaderConfig, MarkdownConfig, MarkdownEngineConfig, MdxConfig,
    MdxPipelinePluginConfig, MotionConfig, MotionEngineConfig, OrmConfig, PwaConfig,
    ReactCompilerConfig, ReactCompilerImplementation, ReactCompilerMode, ReactConfig,
    RenderingConfig, RenderingMode, RouterConfig, RouterConvention, RuntimeTarget, StyleEngine,
    TemporalConfig, TuiConfig, TuiStandardConfig, WebConfig,
};
pub use library::{LibraryConfig, LibraryFormat, LibraryPlan};
pub use lint::{
    FlowBuiltinLintMode, FlowLintConfig, FlowLintParser, LintConfig, LintEngine, RuleLevel,
};
pub use plugins::{ApplyCondition, HookOrder, PipelineMode, PluginEntry, PluginSpec};
pub use rendering::{PlanSource, Prerender, RenderingPlan};
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
    /// What a runtime accessibility audit runs, and where it runs.
    pub accessibility: AccessibilityConfig,
    pub app: AppConfig,
    pub build: BuildConfig,
    /// Which builder `uf dev`, `uf build`, `uf preview` and `uf start` drive.
    pub builder: BuilderConfig,
    pub dev: DevConfig,
    pub docs: DocsConfig,
    pub env: EnvConfig,
    pub fmt: FmtConfig,
    pub lint: LintConfig,
    pub package: PackageConfig,
    /// What the project's own code may reach, or `None` for no limit.
    ///
    /// `None` — the key absent — is the toolchain uf has always been: a test,
    /// a plugin and a config file run with whatever the host would have given
    /// them. Present is the opposite default, **deny except what is listed**,
    /// and `permissions: {}` is a legitimate and meaningful thing to write: it
    /// denies everything uf does not itself need. The two cases are an
    /// `Option` rather than an `is_empty()` check for exactly that reason —
    /// "no block" and "an empty block" are opposite instructions, and a
    /// section struct with a `Default` cannot tell them apart.
    ///
    /// The set is uf's, not a particular host's; `uf_runtime::permissions`
    /// translates it per host and refuses where a host cannot enforce it.
    pub permissions: Option<Permissions>,
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
    pub site: SiteConfig,
    pub std: StdConfig,
    pub story: StoryConfig,
    pub task_runner: TaskRunnerConfig,
    pub tasks: BTreeMap<CompactString, TaskDefinition>,
    pub test: TestConfig,
    /// The project's own Vite configuration, passed through untouched.
    ///
    /// uf does not read this and does not need to. Re-declaring an upstream
    /// tool's options — as `dev` and `build` still do — makes every option uf
    /// has not enumerated unreachable until uf ships a release naming it,
    /// which is the structure that made `react-scripts` the bottleneck every
    /// ecosystem upgrade had to pass through. See `docs/red-lines.md`.
    pub vite: Option<serde_json::Value>,
    pub vrt: VrtConfig,
}

/// The runtime accessibility audit: `expect(el).toHaveNoAxeViolations()` in a
/// test, and the page `uf dev` is serving.
///
/// One block for both, and that is the point of it. A rule a project has
/// decided cannot be judged here — `color-contrast` against a DOM with no
/// layout is the standing example — must be the same rule in the suite and in
/// the developer's loop, or the audit that runs while somebody is writing the
/// component disagrees with the one that will block their pull request. That
/// is worse than having only one of them.
///
/// Distinct from `lint.rules`' `a11y/*`, which are static rules over JSX that
/// was never rendered. The two do not overlap: the linter can see that an
/// `<img>` has no `alt` in the source, and only a rendered tree can say that
/// the heading levels the component actually produced skip a step.
///
/// Everything here is inert without axe-core, which uf does not install: see
/// `packages/test/internal/axe.js` for why the engine is an optional
/// dependency rather than a vendored reimplementation of four hundred rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct AccessibilityConfig {
    /// Whether `uf dev` audits the page it renders and reports through the
    /// diagnostic channel.
    ///
    /// On by default, and it costs nothing in a project that has not installed
    /// axe-core: the dev server looks the engine up before it injects anything,
    /// and injects nothing when there is none. Turning it off is for a project
    /// that has the engine for its tests and does not want the report while it
    /// works.
    pub dev_audit: bool,
    /// Which rules run, in both places.
    pub axe: AxeConfig,
}

/// How much of axe-core an audit runs.
///
/// Data, not modules: the engine is named in `packages/test/internal/axe.js`
/// as a constant, because `uf.config.js` arrives with a cloned repository and
/// "which module does the runner import" is not a question it may answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct AxeConfig {
    /// Run only rules carrying one of these axe tags — `"wcag2a"`,
    /// `"wcag2aa"`, `"best-practice"` — or every rule when empty.
    pub tags: Vec<CompactString>,
    /// Rule ids to turn off, by axe's own id.
    pub disabled_rules: Vec<CompactString>,
    /// The weakest impact that counts as a violation, or every impact when
    /// absent.
    ///
    /// A floor rather than a filter on ids, because "we are not fixing minor
    /// findings this quarter" is a different decision from "this rule is wrong
    /// about our markup", and writing the first as a list of the second goes
    /// stale the moment axe adds a rule.
    pub min_impact: Option<AxeImpact>,
}

/// How serious axe considers a violation, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AxeImpact {
    /// Worth fixing, not worth blocking.
    Minor,
    /// A real obstacle for some readers.
    Moderate,
    /// An obstacle for many readers.
    Serious,
    /// Makes the content unusable.
    Critical,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            dev_audit: true,
            axe: AxeConfig::default(),
        }
    }
}

impl AxeConfig {
    /// This block as the JSON `packages/test/internal/axe.js` reads, or `None`
    /// when it says nothing.
    ///
    /// `None` rather than `"{}"` for the empty case so the variable is absent
    /// rather than present and meaningless, which is the difference between a
    /// worker that was told nothing and one that was told to narrow to
    /// nothing.
    #[must_use]
    pub fn as_json(&self) -> Option<String> {
        if self.tags.is_empty() && self.disabled_rules.is_empty() && self.min_impact.is_none() {
            return None;
        }
        serde_json::to_string(self).ok()
    }
}

/// Which builder uf orchestrates.
///
/// Vite is the **default**, and `docs/red-lines.md` line 3 is the reason this
/// key exists: every built-in provider must be replaceable, and until
/// ubugeeei-prod/uf#549 this was the largest one in the toolchain with no seam
/// at all — `@uniflowed/vite` was not one implementation of a contract, it was
/// reached by name from four commands.
///
/// [`module`](Self::module) is a module specifier resolved the way any other
/// provider is: a package name found by walking up `node_modules`, or a path
/// starting with `.` or `/` that must stay inside the project. What is found
/// has to satisfy the contract in `docs/architecture.md` — a driver executable
/// by the project's Capability JS Host, speaking one JSON event per line — and
/// nothing about that contract is Vite's.
///
/// This is not an `eject`. Red line 4 forbids one, and this is its opposite:
/// the seam a project reaches for when the default is wrong is a *provider*
/// swap, and it is reversible by deleting one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct BuilderConfig {
    /// The module that implements the builder contract.
    ///
    /// `"@uniflowed/vite"` unless a project says otherwise. A relative path is
    /// resolved from the project root and may not climb out of it, which is
    /// the same rule `uf_plugin` applies to a plugin: a config file is
    /// untrusted input, and "run this file as the toolchain" is the most
    /// dangerous thing it can say.
    pub module: CompactString,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/vite"),
        }
    }
}

impl UniflowedConfig {
    /// The registry uf reads packuments and attestations from.
    ///
    /// Reading and publishing are two different questions and, until
    /// ubugeeei-prod/uf#540, uf had one answer to both: `publish.registry`. For
    /// npmjs and for a private registry a project both publishes to and
    /// installs from, that is right by accident; it stops being right the
    /// moment the two differ, which is every project that publishes to a
    /// company registry and installs through a read-through mirror.
    ///
    /// So `pm.registry` is the one that means "resolve against this", and it
    /// falls back to `publish.registry` rather than to npmjs — a project that
    /// has only ever set one keeps the behaviour it had. The fallback is
    /// reported rather than silent: [`RegistrySource::is_deprecated`] is what
    /// a command prints a deprecation from.
    #[must_use]
    pub fn read_registry(&self) -> ReadRegistry<'_> {
        if let Some(registry) = self.pm.registry.as_deref() {
            return ReadRegistry {
                url: registry,
                source: RegistrySource::Pm,
            };
        }
        // Only a project that *moved* `publish.registry` is relying on the old
        // meaning. One that left it at npmjs is not being warned about a key it
        // never set — the value is the same either way, and a deprecation
        // nobody can act on is noise.
        let published = self.publish.registry.as_str();
        if published == DEFAULT_REGISTRY {
            return ReadRegistry {
                url: published,
                source: RegistrySource::Default,
            };
        }
        ReadRegistry {
            url: published,
            source: RegistrySource::PublishFallback,
        }
    }

    /// The scope bindings, as `("@scope", registry)` pairs in scope order.
    ///
    /// Returned as a borrow of the config rather than copied: the caller is
    /// building a router out of it and there is nothing to own.
    pub fn scope_registries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.pm
            .scopes
            .iter()
            .map(|(scope, registry)| (scope.as_str(), registry.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct BuildConfig {
    pub budgets: BundleBudgets,
    pub entries: Vec<CompactString>,
    pub hooks: BTreeMap<CompactString, TaskDefinition>,
    /// What a library build writes, or `None` for a project that said nothing.
    ///
    /// An `Option` rather than a struct with defaults, and for the same reason
    /// [`UniflowedConfig::permissions`] is one: "absent" and "present and
    /// empty" are different instructions here. A library needs none of these
    /// keys — `app.router.enabled: false` is the whole declaration and
    /// [`LibraryPlan`] fills the rest in — so the only thing presence can
    /// mean is that the project wrote them, which is what lets
    /// `library::check` refuse a library build declared inside an
    /// application instead of resolving the contradiction by precedence.
    pub lib: Option<LibraryConfig>,
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
            lib: None,
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

/// Where the built application is served from, and what it tells crawlers.
///
/// This is the one fact a build cannot work out for itself. A route table says
/// `/guide/install`; a `sitemap.xml` has to say
/// `https://docs.uniflowed.dev/guide/install`, and no part of a bundle knows
/// the host it will be deployed to. So [`url`](Self::url) is the switch: unset,
/// `uf build` writes no metadata files at all, because a `<loc>` that is wrong
/// is worse for a site than a sitemap that does not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SiteConfig {
    /// The origin the site is served from, with no trailing slash —
    /// `"https://docs.uniflowed.dev"`.
    ///
    /// Validated when it is used rather than when it is parsed: `uf_config`
    /// reads a `uf.config.js` for every command, and a command that never
    /// writes a sitemap has no business refusing to run over one.
    pub url: Option<CompactString>,
    /// Whether to write `sitemap.xml`. On by default, and still conditional on
    /// [`url`](Self::url).
    pub sitemap: bool,
    pub robots: RobotsConfig,
}

/// What `robots.txt` says, for the crawlers that read it.
///
/// Deliberately small. `robots.txt` is a request, not a control — a path named
/// here is a path published to everyone who fetches the file, so the rules a
/// project wants are almost never the rules it has. What it is genuinely good
/// for is the `Sitemap:` line, which is how a crawler that was not told about
/// the sitemap finds it, and that is why the file is written at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RobotsConfig {
    /// Whether to write `robots.txt`. On by default, and still conditional on
    /// there being something true to put in it.
    pub enabled: bool,
    /// Paths to allow, as `Allow:` lines. Only ever useful as an exception
    /// carved out of a [`disallow`](Self::disallow) below it.
    pub allow: Vec<CompactString>,
    /// Paths to ask crawlers not to request, as `Disallow:` lines.
    pub disallow: Vec<CompactString>,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            url: None,
            sitemap: true,
            robots: RobotsConfig::default(),
        }
    }
}

impl Default for RobotsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow: Vec::new(),
            disallow: Vec::new(),
        }
    }
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
    /// The mode a command runs in when nothing else says.
    ///
    /// The mode picks `.env.<mode>` and `.env.<mode>.local` out of the cascade
    /// and is what `import.meta.env.MODE` reads. Empty by default, which means
    /// the command decides — `development` for `uf dev`, `production` for `uf
    /// build`, `uf preview` and `uf start`, `test` for `uf test`.
    ///
    /// `--mode` beats this, and so does the profile `uf env use` wrote; see
    /// [`env_files::resolve_mode`].
    pub active: CompactString,
    /// The environment files to read, instead of the conventional cascade.
    ///
    /// Empty by default, which selects `.env`, `.env.local`, `.env.<mode>` and
    /// `.env.<mode>.local`, in that order, the later file winning. A project
    /// that sets this gets exactly the files it names, in the order it names
    /// them, and nothing else — a file that does not exist is skipped.
    ///
    /// Paths are relative to the project root. See [`env_files`] for the
    /// precedence, the parser, and which values reach the browser.
    pub files: Vec<CompactString>,
    /// The JavaScript runtimes and package managers this project uses, by
    /// name and exact version — `{ node: "24.14.0", pnpm: "9.15.0" }`.
    ///
    /// Exact, because a range is not an environment: a lockfile that can
    /// resolve differently tomorrow does not answer "what is this built
    /// with". `uf env install` puts each into a store shared by every
    /// repository on the machine and links it into this one, and nothing is
    /// installed globally.
    ///
    /// Empty by default: a project that does not say gets whatever is on
    /// `PATH`, which is what every project does today and is not a thing to
    /// change without being asked.
    pub toolchain: BTreeMap<CompactString, CompactString>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            // Both empty, and both mean "the command decides". They used to be
            // `"development"` and the three files a development run reads,
            // which was a default that could only ever be right for `uf dev`:
            // a `uf build` honouring it would have read `.env.development` into
            // a production bundle. See ubugeeei-prod/uf#259.
            active: CompactString::const_new(""),
            files: Vec::new(),
            toolchain: BTreeMap::new(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NonFlowFormatConfig {
    pub formatter: NonFlowFormatter,
    /// Whether the project named that formatter, or uf did.
    ///
    /// Not a setting: nothing in `uf.config.js` writes it, and it is skipped
    /// on the way out so `uf inspect --json` keeps answering "which formatter
    /// will run" with one field. It records where the answer came from, which
    /// is a different question and the one that decides whether a missing
    /// binary is a warning or an error. uf's default is a convenience and must
    /// never fail a project that did not ask for it; `fmt.nonFlow.formatter`
    /// written out by hand is a requirement the project stated, and a stated
    /// requirement that is not met is an error. See ubugeeei-prod/uf#441.
    #[serde(skip)]
    pub chosen_by_project: bool,
    /// Extra arguments, passed to the formatter verbatim.
    ///
    /// # Why this is a list of strings rather than a shape
    ///
    /// Red line 2 forbids uf mirroring an upstream tool's configuration
    /// schema: every option uf re-declares is an option that needs a uf
    /// release before anybody can use it. Red line 8 requires the opposite
    /// direction — if the formatter can do it, a uf project can do it, without
    /// waiting for uf.
    ///
    /// A list of arguments satisfies both, because uf declares nothing about
    /// what is in it. A Tailwind 4 project writes
    /// `["--css-parse-tailwind-directives=true"]` and needs no second
    /// configuration file; the same field reaches every option biome or
    /// prettier has now or adds later, and uf never has to know their names.
    ///
    /// Appended after uf's own arguments, so a project that wants a different
    /// line width than `fmt.lineWidth` can say so — defaults are conveniences
    /// (red line 9). The one thing it may not do is turn a check into a write:
    /// see [`crate::forbidden_formatter_argument`].
    pub arguments: Vec<CompactString>,
}

impl Default for NonFlowFormatConfig {
    fn default() -> Self {
        Self {
            formatter: NonFlowFormatter::Biome,
            chosen_by_project: false,
            arguments: Vec::new(),
        }
    }
}

/// The arguments a project may not put in `fmt.nonFlow.arguments`.
///
/// `uf fmt --check` is what runs in CI, and its whole contract is that it
/// changes nothing. An argument that makes the formatter write anyway would
/// turn a green check into a working tree nobody asked to have rewritten —
/// silently, in a job whose output nobody reads when it passes.
///
/// Nothing else is refused. A project that passes a nonsensical option gets
/// the formatter's own error, which is a better message than one uf could
/// write about a flag it has never heard of.
#[must_use]
pub fn forbidden_formatter_argument(argument: &str) -> bool {
    // `--write=false` is not a write, but it is also not something anybody
    // types; the flag is matched on its name so that `--write` and
    // `--write=true` are both caught, and a project meaning the third thing
    // can say it in a way that does not read as the first.
    let name = argument.split('=').next().unwrap_or(argument);
    matches!(name, "--write" | "-w" | "--fix" | "--unsafe" | "--check")
}

/// Read by hand rather than derived, because the derive cannot tell "the
/// project wrote `biome`" from "the project wrote nothing and got uf's
/// default" — `#[serde(default)]` fills the field in either way and the two
/// are indistinguishable afterwards. Reading the key as an `Option` keeps that
/// difference, and collapsing it here means every reader still sees a plain
/// `NonFlowFormatter` rather than an option it has to unwrap.
impl<'de> Deserialize<'de> for NonFlowFormatConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Fields {
            #[serde(default)]
            formatter: Option<NonFlowFormatter>,
            #[serde(default)]
            arguments: Vec<CompactString>,
        }

        let fields = Fields::deserialize(deserializer)?;
        if let Some(argument) = fields
            .arguments
            .iter()
            .find(|argument| forbidden_formatter_argument(argument))
        {
            return Err(serde::de::Error::custom(format!(
                "fmt.nonFlow.arguments may not contain `{argument}`: it decides whether \
                 `uf fmt --check` writes, and that is the command's own contract"
            )));
        }
        Ok(Self {
            formatter: fields.formatter.unwrap_or_default(),
            chosen_by_project: fields.formatter.is_some(),
            arguments: fields.arguments,
        })
    }
}

/// Who formats the files uf's Flow printer has no business touching.
///
/// `uf fmt` prints Flow from the official Flow parser's syntax tree; a project
/// also holds JSON, CSS and TypeScript, and a Flow printer over any of them
/// produces a file that no longer parses. uf runs a formatter that understands
/// them rather than writing one, and which formatter is the project's choice.
///
/// A real choice, not a shape: this enumeration had one variant and nothing
/// read it, which is what red line 3 warns about — "the shape of
/// replaceability with none of the substance". The default is a convenience.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NonFlowFormatter {
    /// Biome, which is fast and formats all three.
    #[default]
    Biome,
    /// Prettier, which more projects already have.
    Prettier,
    /// Nobody. Non-Flow files are left exactly as they are.
    None,
}

impl NonFlowFormatter {
    /// The name to print when saying who will format these files.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Biome => "biome",
            Self::Prettier => "prettier",
            Self::None => "none",
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

/// The registry uf reads from, and publishes to, when a project names neither.
pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org";

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
    /// The registry uf *reads* from: packuments, provenance attestations, and
    /// the versions `uf update` reports against.
    ///
    /// Unset means `publish.registry`, which is where this value lived until
    /// ubugeeei-prod/uf#540 — so a project that has only ever set one keeps
    /// working, and one that publishes to a company registry while installing
    /// through a read-through mirror can finally say so.
    pub registry: Option<CompactString>,
    /// Which registry answers for which scope, as `"@scope" -> registry URL`.
    ///
    /// A scope named here is resolved from that registry **and nowhere else**.
    /// There is deliberately no fallback to [`PackageManagerConfig::registry`]:
    /// a fallback is the dependency-confusion vulnerability, not a mitigation
    /// of it. See [`crate::UniflowedConfig::scope_registries`].
    pub scopes: BTreeMap<CompactString, CompactString>,
    /// How hard `uf install` looks at npm provenance attestations.
    pub provenance: ProvenanceMode,
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
            registry: None,
            scopes: BTreeMap::new(),
            provenance: ProvenanceMode::default(),
        }
    }
}

/// What `uf install` does about npm provenance attestations.
///
/// Most of npm has no attestation, so `Report` is the default: an attestation
/// that is *present and wrong* is always a hard failure, and one that is absent
/// is a line in the summary. `Off` is for a machine with no route to the
/// registry at all, where the reads would only ever time out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProvenanceMode {
    /// Read every attestation that exists, refuse a mismatch, report the rest.
    #[default]
    Report,
    /// Read nothing. The lockfile's integrity hashes are the only check left.
    Off,
}

impl ProvenanceMode {
    /// Whether uf reads attestations at all under this mode.
    #[must_use]
    pub const fn reads_attestations(self) -> bool {
        matches!(self, Self::Report)
    }
}

/// Where the registry uf reads from came from.
///
/// Worth distinguishing because one of the three is deprecated and the reader
/// has to be told which project text to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrySource {
    /// `pm.registry`, which is the setting that means "read from here".
    Pm,
    /// `publish.registry`, because `pm.registry` is unset and this project set
    /// the publish one to something other than the default.
    ///
    /// Deprecated: it still works, and it is the wrong key. See
    /// ubugeeei-prod/uf#540.
    PublishFallback,
    /// Neither was set, so it is npmjs.
    Default,
}

impl RegistrySource {
    /// Whether this project is relying on the deprecated spelling.
    #[must_use]
    pub const fn is_deprecated(self) -> bool {
        matches!(self, Self::PublishFallback)
    }

    /// The sentence to print when it is.
    ///
    /// One sentence, naming both keys, because "deprecated" without the
    /// replacement is a message that costs a search.
    #[must_use]
    pub const fn deprecation(self) -> Option<&'static str> {
        match self {
            Self::PublishFallback => Some(
                "publish.registry is being read from as well as published to; \
                 set pm.registry to the one uf should resolve against",
            ),
            Self::Pm | Self::Default => None,
        }
    }
}

/// The registry uf resolves against, and which key it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadRegistry<'a> {
    /// The URL itself.
    pub url: &'a str,
    /// Which setting supplied it.
    pub source: RegistrySource,
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
    pub coverage: CoverageConfig,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/test"),
            runner: NativeTestRunnerConfig::default(),
            react_testing_library_native: true,
            coverage: CoverageConfig::default(),
        }
    }
}

/// How `uf test --coverage` measures, reports and gates.
///
/// It lives here rather than in a file of its own because a coverage gate is a
/// property of the project, not of the invocation: the number CI fails on has
/// to be the number a laptop fails on, and the only way to guarantee that is
/// for both to read it from `uf.config.js`.
///
/// `enabled` turns coverage on for every run; `--coverage` turns it on for one.
/// The thresholds apply whenever coverage was collected, however it was asked
/// for, so a project cannot pass by leaving the flag off — that is what makes
/// it a gate rather than a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CoverageConfig {
    /// Collect coverage on every run, without `--coverage`.
    pub enabled: bool,
    /// Where the reports are written, relative to the project root.
    pub directory: CompactString,
    /// Which reports to write. `text` is the terminal summary.
    pub reporters: Vec<CoverageReporterConfig>,
    /// Keep only files whose path contains one of these; all of them when empty.
    pub include: Vec<CompactString>,
    /// Drop files whose path contains one of these.
    pub exclude: Vec<CompactString>,
    /// Percentages the project as a whole must reach.
    pub thresholds: CoverageThresholdConfig,
    /// Percentages every single file must reach.
    pub per_file_thresholds: CoverageThresholdConfig,
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: CompactString::const_new("coverage"),
            reporters: vec![CoverageReporterConfig::Text, CoverageReporterConfig::Lcov],
            include: Vec::new(),
            // A test file's own coverage is not information: it is a hundred
            // per cent by construction, because running it is what coverage
            // measures. Counting it would raise every project's number by
            // however many tests it has, which is the opposite of what the
            // number is for.
            exclude: vec![
                CompactString::const_new(".test."),
                CompactString::const_new(".spec."),
            ],
            thresholds: CoverageThresholdConfig::default(),
            per_file_thresholds: CoverageThresholdConfig::default(),
        }
    }
}

/// A percentage each metric must reach, or nothing when it is not checked.
///
/// Whole numbers: a coverage gate is a policy somebody chose, and `84.73` is
/// not a number anybody chooses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CoverageThresholdConfig {
    /// Least line coverage that passes.
    pub lines: Option<u8>,
    /// Least function coverage that passes.
    pub functions: Option<u8>,
    /// Least branch coverage that passes.
    pub branches: Option<u8>,
}

/// One report `uf test --coverage` can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageReporterConfig {
    /// A table on the terminal, and nothing on disk.
    Text,
    /// `lcov.info`, which every code-host coverage integration reads.
    Lcov,
    /// `cobertura-coverage.xml`, which the JVM-shaped half of CI reads.
    Cobertura,
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
    /// Where `uf publish` pushes a package.
    ///
    /// Not where uf reads from: that is `pm.registry`, and it defaults to this
    /// one. See [`UniflowedConfig::read_registry`].
    pub registry: CompactString,
    pub dry_run: bool,
    pub first_publish: FirstPublishConfig,
    pub trusted_publish: TrustedPublishConfig,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            registry: CompactString::const_new(DEFAULT_REGISTRY),
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

    /// The tasks this one runs after, in the order they were written.
    #[must_use]
    pub fn depends_on(&self) -> &[CompactString] {
        match self {
            Self::Command(_) => &[],
            Self::Detailed(task) => &task.depends_on,
        }
    }

    /// The detailed form, for the fields only it has.
    #[must_use]
    pub fn details(&self) -> Option<&TaskCommand> {
        match self {
            Self::Command(_) => None,
            Self::Detailed(task) => Some(task),
        }
    }

    /// Whether uf may answer this task from `.uf/cache/task` instead of
    /// running it.
    ///
    /// **A task that declares no `inputs` always runs.** That is the whole of
    /// the default, and it is deliberate: a cache key over an input set
    /// somebody has not written down is a guess, and a wrong guess here is a
    /// check that reports success without looking — the one failure mode a
    /// task runner must not have. Declaring `inputs` is a person saying "this
    /// is everything it reads", and only then is there anything to key on.
    ///
    /// `cache: false` turns it off for a task that does declare them, which is
    /// how a task whose inputs are listed for some other reason — or one with
    /// an effect uf cannot see — opts out.
    #[must_use]
    pub fn is_cacheable(&self) -> bool {
        self.details()
            .is_some_and(|task| !task.inputs.is_empty() && task.cache != Some(false))
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
    /// Every file this task reads, as paths or globs relative to the project
    /// root. A pattern beginning `!` excludes what it matches.
    ///
    /// This is the whole of what uf keys the task's cached result on, so it
    /// has to be the whole of what the task reads. A task that lists none is
    /// never cached; see [`TaskDefinition::is_cacheable`].
    pub inputs: Vec<CompactString>,
    /// Every file this task writes, in the same syntax.
    ///
    /// uf does not restore these on a hit — it *checks* them: a result is
    /// replayed only while the files it produced are still on disk with the
    /// contents it produced. Deleting a build directory therefore rebuilds it
    /// rather than being reported as already done.
    pub outputs: Vec<CompactString>,
    /// `false` to keep a task with declared `inputs` out of the cache.
    ///
    /// [`None`] is the default and means "decide from `inputs`". `true` is
    /// that default said out loud, and it is an error on a task that declares
    /// no inputs rather than a silently ignored request.
    pub cache: Option<bool>,
}

impl Default for TaskCommand {
    fn default() -> Self {
        Self {
            command: CompactString::new(""),
            cwd: None,
            depends_on: Vec::new(),
            env: BTreeMap::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            cache: None,
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
    /// A cache switch that is `true` and means nothing.
    ///
    /// `rendering.cache` has four keys and uf implements two of them. Reading
    /// `data: true` and carrying on would put the switch in
    /// `dist/uf-build-manifest.json` and change no behaviour anywhere — which
    /// is precisely the state ubugeeei-prod/uf#277 objects to, and it was that
    /// state for all four keys. Refusing is the only answer that cannot be
    /// mistaken for a cache: a project that sets it is asking for caching it
    /// will not get, and the failure has to happen where the request was made
    /// rather than in production where it was not honoured.
    #[error(
        "{path}: rendering.cache.{key} is true, and uf has no {key} cache. \
         It would reach the build manifest and change nothing. \
         `route` and `fetch` are the two that are implemented; \
         see ubugeeei-prod/uf#277 for what the other two need."
    )]
    UnimplementedCache {
        path: Utf8PathBuf,
        key: &'static str,
    },
    /// A `rendering.modes` that leaves the build with nothing it can do.
    ///
    /// The list is an allowlist, so naming a strategy uf has not written is
    /// not itself an error — `["ssg", "isr"]` permits one thing that never
    /// happens and one that does. A list that permits *only* strategies uf
    /// has not written is different: there is no build behind it, and the two
    /// honest readings of it — "prerender anyway" and "produce nothing" — are
    /// both the silent semantic change the guide forbids.
    #[error(
        "{path}: app.rendering.modes is [{modes}], and uf implements none of them. \
         `ssg` prerenders a route and `ssr` renders it per request; \
         `ppr` and `isr` are planned and are never selected. \
         Allow at least one of `ssg` and `ssr`."
    )]
    NoImplementedRenderingMode { path: Utf8PathBuf, modes: String },
    /// `build.staticBuild` beside a `rendering.modes` that forbids `ssg`.
    ///
    /// Two declarations that cannot both be true: one says every route is
    /// prerendered and no server is emitted, the other says a prerendered
    /// route is not something this project deploys. Whichever were read second
    /// would silently win, which is the failure ubugeeei-prod/uf#385 is about
    /// one level down.
    #[error(
        "{path}: build.staticBuild prerenders every route, and app.rendering.modes does not \
         allow `ssg`. Add `\"ssg\"` to the list, or drop `staticBuild` and deploy the server \
         this project's routes need."
    )]
    StaticBuildWithoutSsg { path: Utf8PathBuf },
    /// `build.lib` in a project whose file-system router is on.
    ///
    /// One project is one kind of build. `app.router.enabled` is what says
    /// which, and `build.lib` describes a build only a library has, so the two
    /// together are a project that has asked for both — and whichever were
    /// read second would silently win, which is the failure
    /// ubugeeei-prod/uf#385 is about with different keys.
    #[error(
        "{path}: build.lib describes a library build, and app.router.enabled is true, so this \
         project is an application. Set `app: {{ router: {{ enabled: false }} }}` to make it a \
         library, or drop `build.lib`."
    )]
    LibraryBuildInAnApplication { path: Utf8PathBuf },
    /// `build.lib.entries: []`.
    #[error(
        "{path}: build.lib.entries is empty, so this library build has nothing to build. Name \
         the modules a consumer imports — `entries: [\"index.js\"]` is the default."
    )]
    LibraryWithoutEntries { path: Utf8PathBuf },
    /// `build.lib.formats: []`.
    #[error(
        "{path}: build.lib.formats is empty, so this library build would write no module at \
         all. Name at least one — `formats: [\"es\"]` is the default, and `\"cjs\"` is what a \
         consumer calling `require` needs."
    )]
    LibraryWithoutFormats { path: Utf8PathBuf },
    /// `build.lib.formats` naming a format uf does not write.
    ///
    /// Refused rather than skipped: a build that quietly writes two of the
    /// three formats a project asked for is a build whose gap is found by a
    /// consumer.
    #[error(
        "{path}: build.lib.formats names {formats}, which uf does not write. Each needs a \
         global name per entry, and what a Flow library's global should be is not a decision uf \
         has made. The formats uf writes are `es` — what a bundler and a modern Node consume — \
         and `cjs`, for a consumer that calls `require`."
    )]
    LibraryFormatNotImplemented { path: Utf8PathBuf, formats: String },
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
            let config: UniflowedConfig =
                json5::from_str(&json5).map_err(|source| ConfigError::Parse {
                    path: path.to_path_buf(),
                    message: source.to_string(),
                })?;
            check_cache_switches(path, &config.app.rendering.cache)?;
            // What the project says a build may produce, checked where it was
            // written. `rendering::check` refuses the two combinations that
            // have no build behind them; `RenderingPlan::resolve` is
            // infallible after it, which is why every caller downstream can
            // ask for the plan without handling an error.
            rendering::check(path, &config)?;
            // And what a build *is*, which is the question one level above
            // that: `app.router.enabled: false` makes the project a library,
            // and `build.lib` describes a build only a library has. See
            // ubugeeei-prod/uf#268.
            library::check(path, &config)?;
            Ok(config)
        }
        _ => Err(ConfigError::UnsupportedExpression {
            path: path.to_path_buf(),
        }),
    }
}

/// Refuse a cache switch uf would read and not honour.
///
/// Only the two that are unimplemented, and only when a project turned one
/// *on*: `false` is the default and says the same thing whether or not there is
/// an implementation behind it. `route` and `fetch` reach
/// `@uniflowed/server/cache` through the generated server entry and through
/// `uf preview`/`uf start`, so they are checked by the suite rather than here.
fn check_cache_switches(path: &Utf8Path, cache: &CacheConfig) -> Result<(), ConfigError> {
    for (on, key) in [(cache.data, "data"), (cache.actions, "actions")] {
        if on {
            return Err(ConfigError::UnimplementedCache {
                path: path.to_path_buf(),
                key,
            });
        }
    }
    Ok(())
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
