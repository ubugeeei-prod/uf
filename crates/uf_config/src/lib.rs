#![cfg_attr(test, allow(clippy::disallowed_macros))]

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
pub use uf_assets::{FontsConfig, IconsConfig, ImagesConfig, OgConfig, RemotePattern};
pub use uf_bundle::{BudgetMetric, BundleBudgets, ByteSize, SizeBudget};
pub use uf_runtime::{Permission, PermissionError, Permissions, ToolchainAccess};

mod app;
pub mod env_files;
mod library;
mod lint;
mod native_links;
mod package_scripts;
mod pin;
pub use package_scripts::INSTALL_LIFECYCLE_SCRIPTS;
pub use pin::pinned_uf;
pub mod plugins;
mod rendering;
mod router_rules;
mod runtime;
pub mod schema;
pub mod tools;

pub use app::{
    AppConfig, BuiltinConfig, CacheConfig, FrameworkPreset, HeaderRule, HighlightConfig,
    HighlightThemes, MarkdownConfig, MdxConfig, NativeLinksConfig, Navigation, ReactCompilerConfig,
    ReactCompilerMode, ReactConfig, RedirectRule, RenderingConfig, RenderingMode, RewriteRule,
    RouterConfig, RuntimeTarget, StyleEngine, TrailingSlash,
};
pub use library::{LibraryConfig, LibraryFormat, LibraryPlan};
pub use lint::{LintConfig, RuleLevel};
pub use plugins::{ApplyCondition, HookOrder, PipelineMode, PluginEntry, PluginSpec};
pub use rendering::{PlanSource, Prerender, RenderingPlan};
pub use runtime::{
    CapabilityJsHost, CapabilityJsHostConfig, DeployAdapter, DeployAnywhereConfig, RuntimeConfig,
    RuntimeEngine,
};
pub use tools::{
    BUN_TEST_RUNNER_ISSUE, BuilderSpec, DeclaredTool, PackageManagerName, PackageManagerSpec,
    ParseSpec, RuntimeSpec, SpecError, TestRunnerConfig, TestRunnerSpec, ToolDeclaration, ToolName,
    ToolRole, ToolSource, ToolSpec, ToolVersion, VITE_BUILDER_MODULE, Written,
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
    /// Paths every command that walks the project stays out of, or `None` for
    /// the list uf ships.
    ///
    /// Top level because it is read at the top level: `uf fmt`, `uf lint`,
    /// `uf check`, `uf test` and `uf doc` all walk the project through
    /// [`uf_project::scan_source_files`], and all five read this. It lived
    /// under `lint` until ubugeeei-prod/uf#575, which is a name that answers
    /// "which command?" with one of the five — so a person excluding a
    /// directory from *formatting* had to write it under `lint`, and a person
    /// reading `lint.ignore` had no reason to think it did anything to
    /// `uf fmt`.
    ///
    /// `None` — the key absent — is not the same as `Some(vec![])`, which is
    /// why this is an `Option` and not an empty default. Absent means uf's own
    /// list; empty means a project that has looked at that list and wants none
    /// of it. [`LintConfig::ignore`] is the deprecated spelling and is read
    /// when this is absent; see [`UniflowedConfig::project_ignore`].
    pub ignore: Option<Vec<CompactString>>,
    pub lint: LintConfig,
    /// The package manager `uf install`, `uf add`, `uf update` and the rest
    /// drive, and optionally which release of it: `"pnpm@12.0.0"`.
    ///
    /// `None` — the key absent — is the detection uf has always done:
    /// `pm.packageManager`, then `package.json#packageManager`, then the
    /// lockfile. Present, it comes before all three, and `pm.packageManager`
    /// is its deprecated spelling. See [`tools`] for the grammar and
    /// [`UniflowedConfig::package_manager_tool`] for the lookup.
    pub package_manager: Option<Written<PackageManagerSpec>>,
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
    /// The runtime every command runs on unless a section names its own, and
    /// optionally which release of it: `"node@26"`.
    ///
    /// `uf start`, `uf run` and `uf exec` read it directly; `uf dev`,
    /// `uf build` and `uf preview` read it after `build.runtime`; `uf test`
    /// after `test.runtime` and the runtime its runner brings. `None` is the
    /// host selection uf has always made — `app.runtime.capabilityJsHost`, on
    /// `PATH` — so a project that writes none of these runs on what it ran on.
    /// See [`tools`].
    pub runtime: Option<Written<RuntimeSpec>>,
    pub site: SiteConfig,
    /// Tasks `uf prepare` runs before a commit, keyed by a glob over the
    /// staged files.
    ///
    /// A glob with no `/` matches a file's name wherever it is; one with a `/`
    /// matches its path from the project root. Each task named runs once, with
    /// every staged file its glob matches appended to its command, over what
    /// is staged rather than what is on disk. See `uf_prepare`.
    pub staged: BTreeMap<CompactString, StagedTasks>,
    pub task_runner: TaskRunnerConfig,
    pub tasks: BTreeMap<CompactString, TaskDefinition>,
    pub test: TestConfig,
    /// Where `uf ui add` writes the components a project owns, and where
    /// `uf ui list` and `uf ui diff` look for them.
    pub ui: UiConfig,
    /// The release of uf this project runs on, or `None` for whichever uf
    /// started the command: `"0.3.0"`.
    ///
    /// An exact release, and read before anything else: a uf of any other
    /// release started inside the project hands its command line to this one,
    /// from the installer's store, installing it first. See [`pin`].
    pub uf: Option<CompactString>,
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
/// The builder is named by `build.builder` now — [`BuildConfig::builder`] —
/// beside the build it describes, and this section is the spelling that came
/// first. Either way the name is a module specifier resolved the way any other
/// provider is: a package name found by walking up `node_modules`, or a path
/// starting with `.` or `/` that must stay inside the project. What is found
/// has to satisfy the contract in `docs/architecture.md` — a driver executable
/// by the project's Capability JS Host, speaking one JSON event per line — and
/// nothing about that contract is Vite's.
///
/// This is not an `eject`. Red line 4 forbids one, and this is its opposite:
/// the seam a project reaches for when the default is wrong is a *provider*
/// swap, and it is reversible by deleting one line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct BuilderConfig {
    /// The module that implements the builder contract, in the spelling that
    /// came before `build.builder`.
    ///
    /// **Deprecated**, and read only when `build.builder` is absent; see
    /// [`UniflowedConfig::builder_tool`]. `None` rather than
    /// `"@uniflowed/vite"` when absent, because the deprecation is for a
    /// project that wrote the key, and a filled-in default cannot tell a line
    /// a project wrote from one it did not.
    ///
    /// A relative path is resolved from the project root and may not climb out
    /// of it, which is the same rule `uf_plugin` applies to a plugin: a config
    /// file is untrusted input, and "run this file as the toolchain" is the
    /// most dangerous thing it can say.
    pub module: Option<CompactString>,
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

    /// The paths every command that walks the project stays out of.
    ///
    /// One list, read by `uf fmt`, `uf lint`, `uf check`, `uf test` and
    /// `uf doc` alike, which is why it is [`ignore`](Self::ignore) at the top
    /// level and no longer `lint.ignore`. The old spelling is still read —
    /// there is no release in which a project that wrote it stops being
    /// honoured — and the reader is told which key it is, once, the way
    /// [`Self::read_registry`] tells a project still resolving through
    /// `publish.registry`.
    ///
    /// Precedence rather than union: two lists that both apply is a rule
    /// nobody can predict from either file. The new key wins outright when it
    /// is present, so a project migrating can move the list in one edit and
    /// see exactly what it moved.
    #[must_use]
    pub fn project_ignore(&self) -> ProjectIgnore<'_> {
        if let Some(ignore) = self.ignore.as_deref() {
            return ProjectIgnore {
                entries: ignore,
                source: IgnoreSource::Project,
            };
        }
        if let Some(ignore) = self.lint.ignore.as_deref() {
            return ProjectIgnore {
                entries: ignore,
                source: IgnoreSource::LintFallback,
            };
        }
        ProjectIgnore {
            entries: &DEFAULT_IGNORE,
            source: IgnoreSource::Default,
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

/// What uf stays out of when a project says nothing.
///
/// Three directories nobody's editor, formatter or linter has an opinion about
/// worth acting on: a dependency tree, a build output and cargo's. `.uf` and
/// `.git` are *not* here — `uf_project` keeps those in a list of its own,
/// because a project cannot opt back into them and this one it may replace
/// entirely.
pub static DEFAULT_IGNORE: [CompactString; 3] = [
    CompactString::const_new("node_modules"),
    CompactString::const_new("dist"),
    CompactString::const_new("target"),
];

/// Which key supplied the project's ignore list.
///
/// Worth distinguishing because one of the three is deprecated and the reader
/// has to be told which line of their config to move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreSource {
    /// `ignore`, which is the setting that means "no command walks these".
    Project,
    /// `lint.ignore`, because `ignore` is absent and this project set it.
    ///
    /// Deprecated: it still works, and it is named after one of the five
    /// commands that read it. See ubugeeei-prod/uf#575.
    LintFallback,
    /// Neither was set, so it is [`DEFAULT_IGNORE`].
    Default,
}

impl IgnoreSource {
    /// Whether this project is relying on the deprecated spelling.
    #[must_use]
    pub const fn is_deprecated(self) -> bool {
        matches!(self, Self::LintFallback)
    }

    /// The sentence to print when it is.
    ///
    /// One sentence, naming both keys and why the old one is wrong, because
    /// "deprecated" without the replacement is a message that costs a search —
    /// and because the reason is the whole point here: the reader is being told
    /// that a key they wrote under `lint` has been deciding what `uf fmt`
    /// looks at all along.
    #[must_use]
    pub const fn deprecation(self) -> Option<&'static str> {
        match self {
            Self::LintFallback => Some(
                "lint.ignore is read by uf fmt, uf lint, uf check, uf test and uf doc alike; \
                 move it to the top-level `ignore`, which is what it has always meant",
            ),
            Self::Project | Self::Default => None,
        }
    }
}

/// The paths no command walks into, and which key they came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectIgnore<'a> {
    /// The entries themselves, in the order the project wrote them.
    pub entries: &'a [CompactString],
    /// Which setting supplied them.
    pub source: IgnoreSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct BuildConfig {
    pub budgets: BundleBudgets,
    /// Which builder `uf dev`, `uf build`, `uf preview` and `uf start` drive:
    /// `"vite"`, or a module specifier.
    ///
    /// `None` falls back to `builder.module`, this key's deprecated spelling,
    /// and then to `@uniflowed/vite`. See [`UniflowedConfig::builder_tool`].
    pub builder: Option<Written<BuilderSpec>>,
    pub entries: Vec<CompactString>,
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
    /// What `uf dev`, `uf build` and `uf preview` run on, when it is not the
    /// top-level `runtime`: `"node@26"`.
    ///
    /// Read before [`UniflowedConfig::runtime`], which is what lets a project
    /// build on Node and test on Bun. See
    /// [`UniflowedConfig::build_runtime_tool`].
    pub runtime: Option<Written<RuntimeSpec>>,
    pub static_build: bool,
    pub sourcemap: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            // Budgets stay unset by default: failing a build nobody asked us to
            // police is worse than reporting and moving on.
            budgets: BundleBudgets::default(),
            builder: None,
            entries: vec![CompactString::const_new("app.js")],
            lib: None,
            out_dir: CompactString::const_new("dist"),
            runtime: None,
            static_build: false,
            sourcemap: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DocsConfig {
    /// Where a documentation build is written, which `uf clean` removes.
    pub out_dir: CompactString,
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            out_dir: CompactString::const_new("dist/docs"),
        }
    }
}

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
}

impl Default for DevConfig {
    fn default() -> Self {
        Self {
            host: CompactString::const_new("127.0.0.1"),
            port: 5173,
            strict_port: false,
            fs: DevFsConfig::default(),
            allowed_hosts: Vec::new(),
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
    /// **Deprecated.** It says which tools a project has and not what each is
    /// for, so a project that builds on Node and tests on Bun could not write
    /// that down — and no command read it: a pinned Node was used under
    /// `uf env exec` and nowhere else. Each tool is declared where it is used
    /// now, as `name@version`: [`UniflowedConfig::runtime`],
    /// [`BuildConfig::runtime`], [`TestConfig::runtime`] and
    /// [`UniflowedConfig::package_manager`]. See ubugeeei-prod/uf#940.
    ///
    /// It keeps working for `uf env install` and `uf env exec`, which say so
    /// once, and a pin here that disagrees with one of those keys is an error
    /// naming both rather than a precedence rule.
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
    pub non_flow: NonFlowFormatConfig,
    pub quotes: QuoteStyle,
    pub semicolons: bool,
}

impl Default for FmtConfig {
    fn default() -> Self {
        Self {
            indent_width: 2,
            line_width: 100,
            non_flow: NonFlowFormatConfig::default(),
            quotes: QuoteStyle::Double,
            semicolons: true,
        }
    }
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
            return Err(serde::de::Error::custom(
                compact_str::format_compact!(
                    "fmt.nonFlow.arguments may not contain `{argument}`: it decides whether \
                 `uf fmt --check` writes, and that is the command's own contract"
                )
                .into_string(),
            ));
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

/// The registry uf reads from, and publishes to, when a project names neither.
pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PackageManagerConfig {
    pub lockfile: CompactString,
    pub store_dir: CompactString,
    pub allow_lifecycle_scripts: bool,
    /// Which package manager drives the project, in the spelling that came
    /// before the top-level `packageManager`.
    ///
    /// **Deprecated** for the four managers [`UniflowedConfig::package_manager`]
    /// can name, and read only when that key is absent; the two naming
    /// different managers is an error. `auto` is the default and `uf` — uf's
    /// own resolver, which has no release to pin — has no other spelling, so
    /// neither is deprecated. See
    /// [`UniflowedConfig::package_manager_deprecation`].
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

/// Where `uf ui add` writes the components a project owns.
///
/// A section rather than a flag on the command, because `uf ui list` and
/// `uf ui diff` read the same directory: a component written somewhere the next
/// command does not look reads as missing there.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct UiConfig {
    /// The directory, relative to the project root, or empty for
    /// `app/components/ui`. The CLI refuses a path that leaves the project.
    pub directory: CompactString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct VrtConfig {
    /// Where `page.screenshot(name)` keeps its baselines, inside the project.
    pub baselines: CompactString,
    /// How many pixels may differ and still pass.
    pub threshold: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TestConfig {
    pub module: CompactString,
    /// What `uf test` runs on, when it is neither the runtime the runner
    /// brings nor the top-level `runtime`: `"node@26"`.
    ///
    /// See [`UniflowedConfig::test_runtime_tool`] for the order, and
    /// [`ConfigError::TestRuntimeContradictsRunner`] for the one combination
    /// that is refused.
    pub runtime: Option<Written<RuntimeSpec>>,
    /// Which application host `uf test` targets. `None` preserves the legacy
    /// `test.runner.applicationTarget` fallback until the object form goes
    /// away.
    pub target: Option<NativeTestApplicationTarget>,
    /// What runs the suite: `"uf"` or `"bun[@version]"` — or, deprecated, the
    /// object that described uf's own runner field by field.
    ///
    /// `None` is uf's own runner. See [`UniflowedConfig::test_runner_tool`],
    /// and [`TestConfig::native_runner`] for the object's fields.
    pub runner: Option<TestRunnerConfig>,
    /// Whether uf's runner may split a long test file into shares that run
    /// on several workers at once: `uf_test::Part`.
    ///
    /// Off unless a project turns it on, because it is a promise about the
    /// project's tests rather than a setting of the runner: every share
    /// imports the file and runs its own run of cases, so a case that only
    /// passes after the one written above it has run in the same process —
    /// a `describe` reading what the previous one's `afterAll` left — would
    /// fail in a share of its own.
    pub split_files: bool,
    pub coverage: CoverageConfig,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/test"),
            runtime: None,
            target: None,
            runner: None,
            split_files: false,
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
    /// The one field of the deprecated object anything reads: the fallback
    /// for [`TestConfig::target`]. `runtime`, `scheduler`,
    /// `performanceTarget`, `jsHosts` and `officialFlowParser` described uf's
    /// own runner and changed nothing about it; ubugeeei-prod/uf#1387 removed
    /// them.
    pub application_target: NativeTestApplicationTarget,
}

impl Default for NativeTestRunnerConfig {
    fn default() -> Self {
        Self {
            application_target: NativeTestApplicationTarget::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeTestApplicationTarget {
    #[default]
    Auto,
    Web,
    ReactNative,
}

impl Default for VrtConfig {
    fn default() -> Self {
        Self {
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
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            registry: CompactString::const_new(DEFAULT_REGISTRY),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReleaseConfig {
    pub tag_prefix: CompactString,
}

impl Default for ReleaseConfig {
    fn default() -> Self {
        Self {
            tag_prefix: CompactString::const_new("uf@"),
        }
    }
}

/// How `uf run` treats a task it does not run itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TaskRunnerConfig {
    /// Whether a task with no `command` of its own may be handed to Vite+'s
    /// task runner, `vp run <name>`, which runs the `package.json` script of
    /// that name.
    ///
    /// A task that names a command is run by uf — `uf_task` builds the
    /// dependency graph, runs independent nodes up to a concurrency limit, and
    /// answers from `.uf/cache/task` when the files a task declares it reads
    /// have not changed — and this key has nothing to say about it. The
    /// hand-over is only for a task with no command, which is Vite+'s to
    /// define; see ubugeeei-prod/uf#272.
    ///
    /// On by default, because the hand-over is what a command-less task has
    /// always done. `false` refuses such a task by name rather than starting
    /// a script nobody reviewed in `uf.config.js`. Until ubugeeei-prod/uf#1387
    /// the key defaulted to `false`, was printed by `uf inspect` and
    /// `uf explain` as "forbidden", and was enforced nowhere; `engine`, its
    /// neighbour, had one value and chose nothing, and is gone.
    pub allow_package_scripts: bool,
}

impl Default for TaskRunnerConfig {
    fn default() -> Self {
        Self {
            allow_package_scripts: true,
        }
    }
}

/// The tasks one entry of `staged` names: one, written as a string, or several
/// in the order they run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StagedTasks {
    One(CompactString),
    Many(Vec<CompactString>),
}

impl StagedTasks {
    /// The task names, in order.
    #[must_use]
    pub fn names(&self) -> &[CompactString] {
        match self {
            Self::One(name) => std::slice::from_ref(name),
            Self::Many(names) => names,
        }
    }
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

    /// The arguments this task declares, in the order `uf run` fills them.
    #[must_use]
    pub fn args(&self) -> &[TaskArgument] {
        match self {
            Self::Command(_) => &[],
            Self::Detailed(task) => &task.args,
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
    /// The arguments the task takes, in the order they are appended to
    /// `command`. See [`TaskArgument`].
    pub args: Vec<TaskArgument>,
}

/// One argument a task declares.
///
/// A declaration rather than a template: the value is appended to the task's
/// command, in the order the arguments are declared, exactly where
/// `uf run task value` put it before there was anything to declare. What the
/// declaration adds is a name to pass it by (`--name value`), the values it
/// may take, what to use when it is left out, and — at a terminal — a list to
/// pick it from instead of an error. `uf_task::arguments` is the reader.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct TaskArgument {
    /// What `--name` and the picker call it.
    pub name: CompactString,
    /// One line saying what it is for.
    pub description: Option<CompactString>,
    /// The only values it may take. Empty means any value.
    pub choices: Vec<CompactString>,
    /// The value used when it is not given. An argument with a default is
    /// never asked for.
    pub default: Option<CompactString>,
    /// Whether leaving it out is an error. [`None`] means "unless it has a
    /// `default`".
    pub required: Option<bool>,
}

impl TaskArgument {
    /// An argument called `name`, with nothing else declared.
    #[must_use]
    pub fn named(name: impl Into<CompactString>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Whether it has to be given — on the command line, by its default, or
    /// at a terminal by picking one.
    #[must_use]
    pub fn is_required(&self) -> bool {
        self.required.unwrap_or(self.default.is_none())
    }
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
            args: Vec::new(),
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
    /// The config file is read without being run, and something in it is not a
    /// literal.
    ///
    /// The message names the expression and its line, because the export shape
    /// is nearly always already right and saying so sends the reader looking in
    /// the wrong place. See ubugeeei-prod/uf#698, where the reported fix was
    /// the thing the file already did.
    #[error("{path}:{line}: {reason}\n  found: {snippet}")]
    UnsupportedExpression {
        path: Utf8PathBuf,
        /// 1-based line of the expression that was refused.
        line: usize,
        /// The refused text, trimmed and bounded.
        snippet: String,
        reason: &'static str,
    },
    /// A config file uf has no reader for, which is not the same problem as a
    /// config file it cannot evaluate — and used to share its message.
    #[error("cannot read {path}: uf reads `.js`, `.mjs`, `.cjs` and `.flow` config files")]
    UnreadableConfigFile { path: Utf8PathBuf },
    /// `rendering.cache.actions: true`, which asks for a cache uf does not
    /// have.
    ///
    /// The key is gone — ubugeeei-prod/uf#1387 removed it from
    /// `@uniflowed/config` — and `true` is still refused rather than dropped,
    /// for the reason it was refused while it was declared: a project that
    /// sets it is asking for caching it will not get, and the failure belongs
    /// where the request was made rather than in production, where the
    /// symptom is a mutation that invalidates nothing. See
    /// ubugeeei-prod/uf#277.
    #[error(
        "{path}: rendering.cache.{key} is true, and uf has no {key} cache. \
         The key was removed in uf 0.10 because it could not change anything; \
         delete it (`uf codemod` does). `route`, `fetch` and `data` are the \
         caches uf has."
    )]
    UnimplementedCache {
        path: Utf8PathBuf,
        key: &'static str,
    },
    /// `app.builtins.markdown.mdx.jsxImportSource` naming
    /// `@uniflowed/jsx-runtime`.
    ///
    /// That was the key's default, and the only value `@uniflowed/config`
    /// accepted, for as long as nothing read it: MDX was compiled against
    /// React whatever it said. It is read now, and `@uniflowed/jsx-runtime` is
    /// a package of Flow declarations with no `jsx-runtime` for compiled MDX to
    /// import, so honouring the value would break every `.mdx` page of a
    /// project that only ever wrote the default down. See ubugeeei-prod/uf#1387.
    #[error(
        "{path}: app.builtins.markdown.mdx.jsxImportSource is \"@uniflowed/jsx-runtime\", \
         which has no JSX runtime for MDX to import. It was never read before uf 0.10; \
         delete it to compile MDX against React, as it always has been (`uf codemod` \
         does), or name a package that exports `jsx-runtime`."
    )]
    MdxJsxImportSource { path: Utf8PathBuf },
    /// A runtime named as this project's, with no host behind the name.
    ///
    /// `uf`, `edge`, `serverless` and `container` parse here and have no Flow
    /// loader in `uf_runtime::HOSTS`, so a project that names one cannot
    /// import its own first file there. The loader is the test, not the
    /// grade — `edge` is graded *experimental* for a built worker that runs
    /// under Wrangler, and is refused here all the same. Naming one changed
    /// nothing a command does and two things a reader sees — `uf explain`
    /// printed it as the JavaScript host, and `.uf/install.json` recorded it
    /// among the hosts that must be available — which is the failure
    /// ubugeeei-prod/uf#246 is named after.
    #[error(
        "{path}: {key} names `{engine}`, and uf has no host for it — \
         `uf_runtime::HOSTS` grades `{engine}` {level} with no Flow loader, so a project that \
         names it still runs on whichever of {hosts} the machine has. \
         Which host a command starts is `app.runtime.capabilityJsHost.default`; \
         which target `uf build` writes an artefact for is `app.runtime.deploy.adapter`. \
         Name one of {hosts} here, or drop the key. See {tracking}."
    )]
    RuntimeEngineWithoutHost {
        path: Utf8PathBuf,
        /// The key that named it, so a reader knows which of the two to edit.
        key: &'static str,
        /// The refused name, as it is written.
        engine: &'static str,
        /// How `uf_runtime::HOSTS` grades it.
        level: &'static str,
        /// The names that are hosts, from the same table.
        hosts: String,
        /// Where the rest of the answer is.
        tracking: String,
    },
    /// `uf` names something other than an exact release of uf.
    ///
    /// Its own variant for the reason [`ConfigError::ToolSpec`] is one: the
    /// value was read, and the refusal is about what it says. See [`pin`].
    #[error("{path}: uf is `{written}`, {reason}")]
    UfVersion {
        path: Utf8PathBuf,
        /// What was written there.
        written: String,
        /// Why it was refused, and what to write instead.
        reason: String,
    },
    /// A tool spec uf cannot read: a name the key's role does not take, a
    /// range or a tag where a version belongs, or nothing after an `@`.
    ///
    /// Its own variant rather than [`ConfigError::Parse`], and that matters
    /// beyond the message: `uf dev` and `uf build` answer a parse failure by
    /// starting a JavaScript host to evaluate the config instead, which for a
    /// typo in a version would start a process for nothing and then report a
    /// config that could not be evaluated. See [`tools`].
    #[error("{path}: {key} is `{written}`, {reason}")]
    ToolSpec {
        path: Utf8PathBuf,
        /// The key, as a project writes it: `build.runtime`.
        key: &'static str,
        /// What was written there.
        written: String,
        /// Why it was refused, and what to write instead.
        reason: String,
    },
    /// `test.runtime` beside a runner that brings a different runtime.
    ///
    /// `test: { runtime: "node@26", runner: "bun@1.4" }`: a Bun runner runs on
    /// the Bun it names, so one of the two lines is false, and which one is not
    /// a question uf gets to answer by precedence.
    #[error(
        "{path}: test.runtime is `{runtime}` and test.runner is `{runner}`, and those cannot both \
         be true: a Bun runner runs on the Bun it names. Drop `test.runtime`, which the runner \
         already decides, or write `test.runtime: \"{runner}\"`."
    )]
    TestRuntimeContradictsRunner {
        path: Utf8PathBuf,
        /// What `test.runtime` says.
        runtime: String,
        /// What `test.runner` says.
        runner: String,
    },
    /// A deprecated tool key beside the key that replaced it, saying something
    /// else.
    ///
    /// `builder.module`, `pm.packageManager` and `env.toolchain` keep working,
    /// and a project half way through moving has both spellings at once, which
    /// is fine while they agree. When they do not, whichever were read second
    /// would silently win — the failure ubugeeei-prod/uf#385 is about, with
    /// different keys.
    ///
    /// What to do about it is worked out when the message is written rather
    /// than carried, because every `Result` in this crate is as large as its
    /// largest error and the sentence is a function of the fields.
    #[error(
        "{path}: {key} is `{written}` and {legacy_key} is `{legacy_written}` — two spellings of \
         one tool that disagree, and uf does not pick one. {legacy_key} is the deprecated \
         spelling: {fix}.",
        fix = tools::disagreement_fix(.key, .legacy_key, .legacy_written)
    )]
    ToolKeysDisagree {
        path: Utf8PathBuf,
        /// The key that replaced the deprecated one.
        key: &'static str,
        /// What it says.
        written: String,
        /// The deprecated key: `builder.module`, `pm.packageManager`,
        /// `env.toolchain.node`.
        legacy_key: String,
        /// What it says.
        legacy_written: String,
    },
    /// A `rendering.modes` that leaves the build with nothing it can do.
    ///
    /// The list is an allowlist, so naming a strategy no route ends up needing
    /// is not itself an error. A list that permits nothing — `[]`, or only
    /// strategies uf has not written, should one be declared before it is —
    /// is different: there is no build behind it, and the two honest readings
    /// of it — "prerender anyway" and "produce nothing" — are both the silent
    /// semantic change the guide forbids.
    #[error(
        "{path}: app.rendering.modes is [{modes}], and uf implements none of them. \
         `ssg` prerenders a route, `isr` prerenders it and regenerates it once its \
         lifetime passes, `ppr` prerenders its static shell and leaves the parts that read \
         the request to a server, and `ssr` renders it per request. Allow at least one of \
         them."
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
    /// `csr` in a list with anything else.
    ///
    /// Every other mode answers "where does this route's document come from",
    /// which is what makes the list a set the build picks from per route. `csr`
    /// answers it for the whole application with one document that is no
    /// route's, so a list holding it and something else has two readings —
    /// "prerender what you can and fall back to the shell", and "the shell, and
    /// never mind the rest" — that are different applications. Whichever were
    /// read second would silently win, which is the failure
    /// ubugeeei-prod/uf#385 is about with different keys.
    #[error(
        "{path}: app.rendering.modes is [{modes}], and `csr` cannot share the list. It renders \
         every route in the browser from one shell, where the others write a document per \
         route — so a build cannot pick between them per route, which is what this list is \
         for. Write `[\"csr\"]` for a single-page application, or drop it."
    )]
    CsrIsNotOneOfSeveral { path: Utf8PathBuf, modes: String },
    /// `build.staticBuild` in a `csr` project.
    ///
    /// `staticBuild` is "prerender every route and emit no server bundle", and
    /// `csr` prerenders no route at all. The second half of the two agrees and
    /// the first half cannot, so this is a project that has asked for every
    /// route to be prerendered by a build that writes one document.
    #[error(
        "{path}: build.staticBuild prerenders every route, and app.rendering.modes is \
         [\"csr\"], which prerenders none — the build writes one shell and the browser renders \
         the routes. A `csr` build already emits no server, so drop `staticBuild`."
    )]
    CsrWithStaticBuild { path: Utf8PathBuf },
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
    /// An `app.router.redirects`, `rewrites` or `headers` entry uf cannot read.
    ///
    /// One variant with a sentence rather than one per spelling, because the
    /// spellings are many and each sentence names the fix; `router_rules`
    /// writes them. The index is the entry's position in its list, so the
    /// reader goes straight to the object rather than searching for a source
    /// that may appear twice.
    #[error("{path}: `app.router.{key}[{index}]` {reason}")]
    RouterRule {
        path: Utf8PathBuf,
        key: &'static str,
        index: usize,
        reason: String,
    },
    /// An `app.router.basePath` every host would compare as characters and
    /// match nothing with. `router_rules` writes the reason.
    #[error("{path}: `app.router.basePath` is {written:?}, {reason}")]
    RouterBasePath {
        path: Utf8PathBuf,
        written: String,
        reason: String,
    },
    /// An `app.builtins.images` setting the request-time endpoint would read
    /// wider, or narrower, than it is written.
    ///
    /// `key` is the whole path under `images`, index included, so the reader
    /// goes straight to the entry. `uf_assets::check_remote_pattern` writes
    /// the reason for a pattern.
    #[error("{path}: `app.builtins.images.{key}` {reason}")]
    ImagesRemote {
        path: Utf8PathBuf,
        key: String,
        reason: String,
    },
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
            let json5 = match extract_config_object(&source) {
                Some(json5) => json5,
                None => {
                    let refusal = diagnose_config_expression(&source);
                    return Err(ConfigError::UnsupportedExpression {
                        path: path.to_path_buf(),
                        line: refusal.line,
                        snippet: refusal.snippet,
                        reason: refusal.reason,
                    });
                }
            };
            parse_config_object(path, &json5)
        }
        _ => Err(ConfigError::UnreadableConfigFile {
            path: path.to_path_buf(),
        }),
    }
}

/// Parse the object literal extracted from a static `uf.config.js`.
///
/// This is the bootstrap path: Rust can read enough config to choose a host and
/// builder without running user code. Once a command has a JavaScript host, the
/// evaluated module should enter through [`parse_config_projection`] instead.
pub fn parse_config_object(
    path: &Utf8Path,
    json5_object: &str,
) -> Result<UniflowedConfig, ConfigError> {
    let config: UniflowedConfig =
        json5::from_str(json5_object).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            message: source.to_string(),
        })?;
    validate_config(path, &config)?;
    Ok(config)
}

/// Parse the JSON projection of an evaluated `uf.config.js`.
///
/// `@uniflowed/vite` already evaluates the module for commands that start a
/// builder driver and emits plain JSON. Keeping this entry point in `uf_config`
/// gives that evaluated path the same serde defaults and semantic validation
/// as the static bootstrap loader.
pub fn parse_config_projection(
    path: &Utf8Path,
    projection: serde_json::Value,
) -> Result<UniflowedConfig, ConfigError> {
    let config: UniflowedConfig =
        serde_json::from_value(projection).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            message: source.to_string(),
        })?;
    validate_config(path, &config)?;
    Ok(config)
}

/// Validate semantic config combinations that serde alone cannot express.
pub fn validate_config(path: &Utf8Path, config: &UniflowedConfig) -> Result<(), ConfigError> {
    check_cache_switches(path, &config.app.rendering.cache)?;
    check_mdx(path, &config.app.builtins.markdown.mdx)?;
    // Which runtime this project says it is written for, checked against the
    // table that says which runtimes have a host. Before the rendering and
    // library checks only because it is the cheapest of the three; the three
    // are independent. See ubugeeei-prod/uf#246.
    runtime::check(path, config)?;
    // What the project says a build may produce, checked where it was written.
    // `rendering::check` refuses the two combinations that have no build behind
    // them; `RenderingPlan::resolve` is infallible after it, which is why every
    // caller downstream can ask for the plan without handling an error.
    rendering::check(path, config)?;
    // And what a build *is*, which is the question one level above that:
    // `app.router.enabled: false` makes the project a library, and `build.lib`
    // describes a build only a library has. See ubugeeei-prod/uf#268.
    library::check(path, config)?;
    // And which tool each command runs: every spec in a form uf can read, no
    // test runtime its runner contradicts, and no deprecated tool key saying
    // something other than the key that replaced it. See ubugeeei-prod/uf#940.
    tools::check(path, config)?;
    // And which uf: an exact release, which is the only thing a uf started
    // elsewhere can hand the command line to. See [`pin`].
    pin::check(path, config.uf.as_deref())?;
    // And `app.router`'s redirects, rewrites and headers, in the grammar every
    // host matches them with. See ubugeeei-prod/uf#959.
    router_rules::check(path, config)?;
    native_links::check(path, &config.app.router)?;
    // And the request-time image endpoint's allow-list, which is a security
    // boundary: a pattern read wider than it is written admits a host nobody
    // listed. See ubugeeei-prod/uf#958.
    check_remote_images(path, &config.app.builtins.images)?;
    Ok(())
}

/// Refuse a remote image setting the endpoint would not read as written.
fn check_remote_images(path: &Utf8Path, images: &ImagesConfig) -> Result<(), ConfigError> {
    let refuse = |key: String, reason: String| ConfigError::ImagesRemote {
        path: path.to_path_buf(),
        key,
        reason,
    };
    for (index, pattern) in images.remote_patterns.iter().enumerate() {
        uf_assets::check_remote_pattern(pattern).map_err(|reason| {
            refuse(
                compact_str::format_compact!("remotePatterns[{index}]").into_string(),
                reason,
            )
        })?;
    }
    for (index, quality) in images.qualities.iter().enumerate() {
        if !(1..=100).contains(quality) {
            return Err(refuse(
                compact_str::format_compact!("qualities[{index}]").into_string(),
                compact_str::format_compact!("is {quality}, and a quality is 1 to 100")
                    .into_string(),
            ));
        }
    }
    if images.transformer.as_deref() == Some("") {
        return Err(refuse(
            String::from("transformer"),
            String::from(
                "is empty; name a module exporting `createImageTransformer`, or drop the key",
            ),
        ));
    }
    Ok(())
}

/// `rendering.cache.actions: true`, which is refused though the key is gone.
fn check_cache_switches(path: &Utf8Path, cache: &CacheConfig) -> Result<(), ConfigError> {
    if cache.retired_actions == Some(true) {
        return Err(ConfigError::UnimplementedCache {
            path: path.to_path_buf(),
            key: "actions",
        });
    }
    Ok(())
}

/// The one `mdx.jsxImportSource` that cannot be honoured; see
/// [`ConfigError::MdxJsxImportSource`].
fn check_mdx(path: &Utf8Path, mdx: &MdxConfig) -> Result<(), ConfigError> {
    if mdx.jsx_import_source == "@uniflowed/jsx-runtime" {
        return Err(ConfigError::MdxJsxImportSource {
            path: path.to_path_buf(),
        });
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

/// Why [`extract_config_object`] refused, with somewhere to look.
///
/// `uf.config.js` is read rather than run, so only literals survive. That is a
/// real constraint and this type does not argue with it — it says *which*
/// expression fell outside it and on what line, which is what the single
/// message it replaces did not. See ubugeeei-prod/uf#698: the old message
/// named `export default defineConfig({ ... })` as the fix, and the file it
/// was reporting on already did exactly that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedConfig {
    /// 1-based line of the refused expression.
    pub line: usize,
    /// The refused text, trimmed and bounded so a minified file cannot make
    /// the diagnostic unreadable.
    pub snippet: String,
    pub reason: &'static str,
}

/// Longest refused expression the message will quote.
const SNIPPET_BYTES: usize = 120;

/// Explain a refusal, for the file [`extract_config_object`] returned [`None`]
/// for.
///
/// Separate from the extractor rather than folded into it: the extractor runs
/// on every successful load too, and it should not carry the cost of
/// describing a failure that is not going to happen.
#[must_use]
pub fn diagnose_config_expression(source: &str) -> UnsupportedConfig {
    let mut in_block_comment = false;
    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        let mut text = raw.trim();

        if in_block_comment {
            match text.find("*/") {
                Some(end) => {
                    in_block_comment = false;
                    text = text[end + 2..].trim();
                }
                None => continue,
            }
        }
        while let Some(rest) = text.strip_prefix("/*") {
            match rest.find("*/") {
                Some(end) => text = rest[end + 2..].trim(),
                None => {
                    in_block_comment = true;
                    text = "";
                    break;
                }
            }
        }

        if text.is_empty() || text.starts_with("//") || text.starts_with("import ") {
            continue;
        }

        // The first thing that is neither an import nor a comment. Whatever is
        // wrong with this file, this is where a reader should start.
        let Some(exported) = text.strip_prefix("export default") else {
            return UnsupportedConfig {
                line,
                snippet: snippet(text),
                reason: "uf reads this file without running it, so a statement before the \
                         default export is not evaluated — inline the value at its use",
            };
        };

        let exported = exported.trim();
        if let Some(call) = exported.strip_prefix("defineConfig") {
            let argument = call.trim_start().strip_prefix('(').unwrap_or(call).trim();
            if !argument.starts_with('{') {
                return UnsupportedConfig {
                    line,
                    snippet: snippet(argument),
                    reason: "the argument to `defineConfig` has to be an object literal, \
                             because uf reads this file without running it",
                };
            }
            return UnsupportedConfig {
                line,
                snippet: snippet(text),
                reason: "the object passed to `defineConfig` is not closed",
            };
        }

        if exported.starts_with('{') {
            return UnsupportedConfig {
                line,
                snippet: snippet(text),
                reason: "the exported object literal is not closed",
            };
        }

        return UnsupportedConfig {
            line,
            snippet: snippet(exported),
            reason: "the default export has to be an object literal or \
                     `defineConfig({ ... })`, because uf reads this file without running it",
        };
    }

    UnsupportedConfig {
        line: 1,
        snippet: String::new(),
        reason: "this file has no default export",
    }
}

/// The refused text, on one line and bounded.
fn snippet(text: &str) -> String {
    let text = text.trim();
    let mut cut = text.len().min(SNIPPET_BYTES);
    while cut < text.len() && !text.is_char_boundary(cut) {
        cut += 1;
    }
    let mut shown = text[..cut].replace(['\n', '\r'], " ");
    if cut < text.len() {
        shown.push('…');
    }
    shown
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
