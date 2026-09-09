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
    /// What the project declares about the fonts it imports.
    ///
    /// Read by `uf assets`, which self-hosts each one and computes the
    /// metric-matched fallback; see `crates/uf_assets`.
    pub fonts: crate::FontsConfig,
    pub framework_lints: bool,
    pub graphql: GraphQlConfig,
    /// What the project declares about the icons it imports.
    ///
    /// The directory `uf:icon/…` resolves against. Read by `uf assets`, which
    /// turns each one into a `<symbol>` and assembles one sprite from the set
    /// a build reached; see `crates/uf_assets/src/icon.rs`.
    pub icons: crate::IconsConfig,
    /// What the project declares about the images it imports.
    ///
    /// The widths a layout asks for and the quality they are encoded at. Read
    /// by `uf assets`; see `crates/uf_assets`.
    pub images: crate::ImagesConfig,
    pub loader: LoaderConfig,
    pub markdown: MarkdownConfig,
    pub motion: MotionConfig,
    pub native_test_runner: bool,
    /// What the project declares about its Open Graph cards.
    ///
    /// A `*.og.json` template is drawn by `uf assets` from a font this names.
    /// uf embeds no typeface, so a project that draws cards has to point at
    /// one; see `crates/uf_assets/src/og.rs`.
    pub og: crate::OgConfig,
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
            fonts: crate::FontsConfig::default(),
            framework_lints: true,
            graphql: GraphQlConfig::default(),
            icons: crate::IconsConfig::default(),
            images: crate::ImagesConfig::default(),
            loader: LoaderConfig::default(),
            markdown: MarkdownConfig::default(),
            motion: MotionConfig::default(),
            native_test_runner: true,
            og: crate::OgConfig::default(),
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
    /// Syntax highlighting for fenced code.
    ///
    /// A field here rather than nowhere. `HighlightConfig` was declared,
    /// exported and documented in the configuration reference — and was a
    /// field of no struct, so nothing deserialized those keys and nothing
    /// read them. `packages/vite` has read `mdxConfig.highlight` the whole
    /// time; what it got was `undefined`, and a project that set a theme got
    /// no error and no effect. See ubugeeei-prod/uf#646.
    pub highlight: HighlightConfig,
}

impl Default for MdxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extensions: vec![CompactString::const_new(".mdx")],
            jsx_import_source: CompactString::const_new("@uniflowed/jsx-runtime"),
            pipeline_plugin: MdxPipelinePluginConfig::BuiltIn,
            highlight: HighlightConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MdxPipelinePluginConfig {
    BuiltIn,
}

/// Build-time syntax highlighting for fenced code in Markdown and MDX.
///
/// On by default: a documentation page whose samples are undifferentiated grey
/// is not "MDX works out of the box". The colours are computed during the build
/// and written into the HTML, so nothing is shipped to the browser to do it,
/// and both themes are emitted together as CSS variables because a build
/// cannot know whether the reader prefers light or dark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct HighlightConfig {
    pub enabled: bool,
    pub themes: HighlightThemes,
    /// Grammars to load beyond the ones a uf project uses by default.
    pub langs: Vec<CompactString>,
}

impl Default for HighlightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            themes: HighlightThemes::default(),
            langs: Vec::new(),
        }
    }
}

/// The theme used for each of the reader's two preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct HighlightThemes {
    pub light: CompactString,
    pub dark: CompactString,
}

impl Default for HighlightThemes {
    fn default() -> Self {
        Self {
            light: CompactString::const_new("github-light"),
            dark: CompactString::const_new("github-dark-dimmed"),
        }
    }
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
    /// Whether `uf dev` hydrates the application inside `<StrictMode>`.
    ///
    /// On by default, and a development-only default: `uf build` never emits
    /// it, so what a visitor runs is unaffected by this field whatever it says.
    /// What Strict Mode buys is that React runs a render, a state initialiser
    /// and a `useMemo` factory twice and mounts every effect, unmounts it and
    /// mounts it again — so a component that is not pure, and an effect whose
    /// cleanup does not undo its setup, fail while they are being written
    /// rather than in the one production render that happens to interleave.
    ///
    /// It is a default rather than a feature because the argument against it is
    /// always the same one — double invocation surprises people — and the
    /// answer is always the same too: it surprises them in development, once,
    /// about a bug they already have. A project that disagrees writes
    /// `app: { react: { strictMode: false } }` and gets a dev server that
    /// renders the way the deployment does.
    ///
    /// See ubugeeei-prod/uf#516. `@uniflowed/vite` reads this out of the loaded
    /// `uf.config.js` and generates the flag into `virtual:uf/client`; nothing
    /// on the Rust side acts on it, which is why it is declared here rather
    /// than plumbed — a field `uf.config.js` may set and `uf inspect` may
    /// report has to exist in the schema that describes the file.
    pub strict_mode: bool,
}

impl Default for ReactConfig {
    fn default() -> Self {
        Self {
            version: CompactString::const_new("19"),
            async_react: true,
            suspense: true,
            use_hook: true,
            strict_mode: true,
        }
    }
}

/// What a project permits `uf build` to produce.
///
/// [`modes`](Self::modes) is an **allowlist**, not a request. It does not ask
/// for a rendering strategy; it says which ones this project is willing to
/// deploy, and the build picks between them per route — a route with no
/// parameters is prerendered, a route with parameters is prerendered if its
/// page exports `generateStaticParams`, and anything left over is rendered per
/// request.
///
/// The distinction matters because those two answers are deployed to different
/// places. A prerendered document is a file a CDN serves; a route rendered per
/// request needs a process. A project that has already chosen a static host
/// says `["ssg"]`, and the build's job is then to **refuse** a route it could
/// only serve with a server rather than to write a `dist/` with a hole in it.
/// That refusal is the whole reason this list is read: until
/// ubugeeei-prod/uf#336 nothing read it, `["ssr"]` was accepted and silently
/// meant SSG, and `["ssg"]` on a project with a per-request route produced a
/// build that 404s once it is deployed and nowhere before.
///
/// See [`crate::RenderingPlan`] for what the build makes of it, together with
/// [`crate::BuildConfig::static_build`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RenderingConfig {
    /// The rendering strategies this project will deploy, in no order.
    ///
    /// The default is the four that decide per route, which together mean
    /// "decide per route and deploy a server" — the behaviour every uf project
    /// had before anything read this. [`RenderingMode::Csr`] is deliberately
    /// not among them and says why on itself.
    pub modes: Vec<RenderingMode>,
    /// What happens when a visitor follows a link.
    ///
    /// A second question from [`modes`](Self::modes), and deliberately not a
    /// fifth value in that list; see [`Navigation`].
    pub navigation: Navigation,
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
            navigation: Navigation::default(),
            cache: CacheConfig::default(),
        }
    }
}

/// What happens when a visitor follows a link.
///
/// The other half of "what does this project deploy", and a **different
/// question** from [`RenderingMode`] rather than a fifth value in it. That
/// list says where a document comes from and is decided per route; this says
/// what the browser does with the document once it has one, and is one answer
/// for the whole application.
///
/// The two compose, which is the proof they are two axes and not one:
///
/// | | `client` | `document` |
/// | --- | --- | --- |
/// | `ssg` | a prerendered site the client router takes over | a static site of documents |
/// | `ssr` | a server-rendered app the client router takes over | a server-rendered application in the older sense |
///
/// Every cell is a deployment somebody wants, and none of the four is a
/// rendering strategy the build could pick *per route*: a route is prerendered
/// or it is not, and the answer does not change because of what the browser
/// does afterwards. Spelling this as `modes: ["mpa", …]` would have put a
/// whole-application decision into a per-route allowlist, where
/// `["mpa", "ssr"]` permits two things that are not alternatives — and an
/// allowlist entry the build can never select is the defect
/// `RenderingMode::Ppr` already is.
///
/// It is also deliberately not spelled the way the neighbours spell it.
/// Next.js says `output: "export"` and Astro says `output: "static"`, and each
/// conflates "no server" with "no client router" because each has one answer
/// for both. uf already has a name for "no server" — `build.staticBuild` — so
/// borrowing either spelling would give one question two names, which is
/// `docs/red-lines.md`'s line 2 from the inside.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Navigation {
    /// The client router takes the link over: the next route is resolved and
    /// rendered in the page that is already open.
    ///
    /// The default, and what every uf application did before this key existed.
    #[default]
    Client,
    /// The browser follows the link: a full document request, the way a link
    /// works with no JavaScript at all.
    ///
    /// The application still hydrates — a `"use client"` component is still a
    /// `"use client"` component — and what it does not do is take navigation
    /// over. See the guide for what that costs and what it does not.
    Document,
}

impl Navigation {
    /// The spelling a `uf.config.js` uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Document => "document",
        }
    }
}

/// One rendering strategy a project may allow.
///
/// Three of the five are implemented, and the enum keeps the other two because
/// the list is an allowlist: naming a strategy uf cannot do yet permits
/// something that never happens, which costs nothing, where *removing* the name
/// would make today's `uf.config.js` files fail to parse. What is refused is a
/// list that allows **only** unimplemented strategies — see
/// [`crate::ConfigError::NoImplementedRenderingMode`] — because that is a
/// project asking for a build uf cannot produce at all.
///
/// [`Csr`](Self::Csr) is the value that does not behave like the others, and
/// its own documentation says why: it is not a per-route answer, so it is the
/// one value that cannot share a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderingMode {
    /// Partial prerendering. **Planned**; allowed and never selected.
    Ppr,
    /// Rendered per request, by `uf start`, `uf preview` or a deploy adapter.
    /// **Implemented.**
    Ssr,
    /// Prerendered to a document at build time. **Implemented.**
    Ssg,
    /// Incremental static regeneration. **Planned**; allowed and never
    /// selected. `rendering.cache` and ubugeeei-prod/uf#277 are the half of it
    /// that exists.
    Isr,
    /// Rendered in the browser, from one shell prerendered at build time.
    /// **Implemented.**
    ///
    /// A single-page application. The build writes one document — an empty root
    /// and the script and stylesheet tags — and the client router resolves and
    /// renders every route from there. No route gets a document of its own, and
    /// no server renders one.
    ///
    /// # Why it cannot share a list
    ///
    /// Every other value answers "where does *this route's* document come
    /// from", which is what makes a list of them a set the build picks from per
    /// route. This one answers "where does every route come from" with a single
    /// document that belongs to no route, and once it is chosen there is no
    /// per-route decision left. `["csr", "ssg"]` is therefore a project asking
    /// for a build that both writes a document per route and does not, and its
    /// two honest readings — "prerender what you can and fall back to the
    /// shell" and "the shell, and never mind the rest of the list" — are
    /// different applications. So it is refused rather than resolved by
    /// precedence; see [`crate::ConfigError::CsrIsNotOneOfSeveral`].
    ///
    /// It is also **not in the default list**, which every other value is. The
    /// default means "decide per route and deploy a server", and a build that
    /// quietly decided to be a single-page application because nothing forbade
    /// it would be the largest semantic change a default has ever made.
    Csr,
}

impl RenderingMode {
    /// The spelling a `uf.config.js` uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ppr => "ppr",
            Self::Ssr => "ssr",
            Self::Ssg => "ssg",
            Self::Isr => "isr",
            Self::Csr => "csr",
        }
    }

    /// Whether `uf build` can select this strategy for a route today.
    ///
    /// `ppr` and `isr` are declared and unwritten. They are not refused on
    /// their own — see the type's documentation — but a project that allows
    /// nothing else is refused, and this is the predicate that decides it.
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        matches!(self, Self::Ssr | Self::Ssg | Self::Csr)
    }

    /// Whether this value is the whole application's answer rather than one
    /// route's.
    ///
    /// One value, and a predicate rather than a comparison, because the
    /// refusal, the plan and the message that explains them all have to agree
    /// about which value it is.
    #[must_use]
    pub const fn is_exclusive(self) -> bool {
        matches!(self, Self::Csr)
    }
}

/// Which of uf's caches a project has turned on.
///
/// Four switches, and for a long time all four of them were read once, copied
/// into `dist/uf-build-manifest.json` and read by nothing — so setting any of
/// them to `true` changed one field of one JSON file and no behaviour anywhere.
/// ubugeeei-prod/uf#277 is about that, and about how a config key that accepts
/// `true` and means nothing is indistinguishable from a cache that is off.
///
/// Two of them mean something now:
///
/// * `route` — a rendered document, kept under the URL that produced it. Wired
///   through `@uniflowed/vite`'s generated server entry and through
///   `uf preview` and `uf start` to `createFetchHandler`'s `cache` option.
/// * `fetch` — a request's answer, kept under the client's name and the URL.
///   The same wiring reaches `@uniflowed/server/cache`'s `createCachedFetch`.
///
/// Two of them do not, and are **refused** rather than ignored:
/// [`crate::ConfigError::UnimplementedCache`] fails the load when `data` or
/// `actions` is `true`. Being told is the point — a project that asks for a
/// cache uf does not have should find out at the config file rather than in
/// production, where the symptom is a mutation that invalidates nothing.
///
/// All four still default to `false`. `docs/roadmap.md` says "opt-in-only cache
/// controls" and that has not changed; what has changed is that opting in now
/// does something.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CacheConfig {
    /// Not implemented. `true` is refused; see the type's documentation.
    pub actions: bool,
    /// Not implemented. `true` is refused; see the type's documentation.
    pub data: bool,
    /// A request's answer, through `@uniflowed/server/cache`.
    pub fetch: bool,
    /// A rendered document, through `@uniflowed/server/fetch`.
    pub route: bool,
}
