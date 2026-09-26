//! Which tool each command runs, declared at the key of the command that runs
//! it.
//!
//! # Why a tool is declared where it is used
//!
//! `env.toolchain` said which tools a repository has — `{ node: "24.14.0",
//! pnpm: "9.15.0" }` — and nothing about what each one is *for*. A project
//! whose build runs on Node and whose tests run on Bun could not write that
//! down, and nothing uf ran read the pins anyway: `uf dev`, `uf build` and
//! `uf test` found their host on `PATH`, so a pinned Node was used under
//! `uf env exec` and nowhere else. See ubugeeei-prod/uf#940.
//!
//! So a tool is named at the key of the command that runs it:
//!
//! ```js
//! export default defineConfig({
//!   runtime: "node@26",
//!   packageManager: "pnpm@12.0.0",
//!   build: { runtime: "node@26", builder: "vite" },
//!   test: { runtime: "bun@1.4", runner: "bun@1.4" },
//! });
//! ```
//!
//! # The grammar
//!
//! A spec is `name[@version]`, and what follows the `@` is one of three things:
//!
//! | Written | Means |
//! | --- | --- |
//! | `node` | whatever `node` is on `PATH` — what every project got before |
//! | `node@26`, `bun@1.4` | the newest release with that prefix, resolved once and locked in `uf.lock` |
//! | `pnpm@12.0.0` | exactly that release |
//!
//! Anything shaped like a range — `^`, `~`, `>`, `<`, `*`, `x`, `||`, a space —
//! is refused where it is read. A range is not an environment: it can resolve
//! to a different release tomorrow, which is the one thing a statement of what
//! a project runs on must not do. A prefix is the honest spelling of "the
//! newest 26": it names one release, and it moves only when somebody moves it.
//!
//! # Where each command looks
//!
//! | Command | Keys, in order |
//! | --- | --- |
//! | `uf dev`, `uf build`, `uf preview` | `build.runtime` → `runtime` |
//! | `uf test` | `test.runtime` → the runner's own runtime → `runtime` |
//! | `uf start`, `uf run`, `uf exec` | `runtime` |
//! | `uf install`, `uf add`, `uf update`, … | `packageManager` → `pm.packageManager` |
//!
//! After those come the keys that answered before these existed, and then what
//! uf detects — so a project that writes none of this behaves exactly as it
//! did. What this module answers is what a project *declares*. Turning a
//! declaration into a binary on disk is `uf_env`'s job, and the command's.
//!
//! # Roles are checked where the config is read
//!
//! `runtime: "pnpm"` names a tool uf knows and is still wrong, so each key is
//! typed by its role: a runtime is `node`, `bun` or `deno`; a package manager
//! is `npm`, `pnpm`, `yarn` or `bun`; a test runner is `uf` or `bun`; a builder
//! is `vite` or a module specifier. The refusal names the key, what was
//! written and what to write instead, because the spec alone does not know
//! which key it was under.
//!
//! # Migration
//!
//! `env.toolchain`, `builder.module`, `pm.packageManager` and the object form
//! of `test.runner` keep working. Each says, in one sentence, which key
//! replaces it — the pattern `lint.ignore` → `ignore` set — and two spellings
//! of one tool that disagree are an error naming both keys rather than a
//! precedence rule nobody could predict from either line.

use std::fmt;
use std::sync::LazyLock;

use camino::Utf8Path;
use compact_str::CompactString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    CapabilityJsHost, ConfigError, FrameworkPreset, NativeTestApplicationTarget,
    NativeTestRunnerConfig, PackageManagerPreference, UniflowedConfig,
};

/// The module `build.builder: "vite"` names, and the builder a project that
/// names none gets.
pub const VITE_BUILDER_MODULE: &str = "@uniflowed/vite";

/// The issue that turned `test.runner: "bun"` into a run.
///
/// Nothing waits on it any more — [`TestRunnerSpec::tracking_issue`] answers
/// `None` for every runner — and it stays a public name because removing one
/// is a breaking change to this crate's API.
pub const BUN_TEST_RUNNER_ISSUE: u32 = 942;

/// The names one role accepts.
///
/// A trait over several small enumerations rather than one enumeration of
/// every tool, because the question a key answers is narrower than "is this a
/// tool uf knows": `runtime: "pnpm"` names a tool and is still wrong, and the
/// sentence that says so has to list what *this* key takes.
pub trait ToolName: Copy + Eq + fmt::Debug + 'static {
    /// The role, the way a sentence names one: `a runtime`.
    const ROLE: &'static str;
    /// Every name the role accepts, in the order a message lists them.
    const ALL: &'static [Self];
    /// An example of a spec for this role, for a message about a value that
    /// is not a string at all.
    const EXAMPLE: &'static str;

    /// The name as a project writes it.
    fn name(self) -> &'static str;
}

/// A runtime is one of the three Capability JS Hosts, and the name a project
/// writes is the executable's name.
impl ToolName for CapabilityJsHost {
    const ROLE: &'static str = "a runtime";
    const ALL: &'static [Self] = &[Self::Node, Self::Bun, Self::Deno];
    const EXAMPLE: &'static str = "node@26";

    fn name(self) -> &'static str {
        self.as_str()
    }
}

/// A package manager `packageManager` may name.
///
/// Four, and not [`PackageManagerPreference`]'s eight. `auto` is the absence
/// of a declaration rather than one; `uf` is uf's own resolver, which has no
/// release of its own to pin; and `yarn-classic` and `yarn-berry` are one tool
/// whose edition is its major version — `yarn@1` is Classic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackageManagerName {
    /// npm.
    Npm,
    /// pnpm.
    Pnpm,
    /// Yarn, either edition.
    Yarn,
    /// Bun, which installs as well as runs.
    Bun,
}

impl ToolName for PackageManagerName {
    const ROLE: &'static str = "a package manager";
    const ALL: &'static [Self] = &[Self::Npm, Self::Pnpm, Self::Yarn, Self::Bun];
    const EXAMPLE: &'static str = "pnpm@10";

    fn name(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }
}

/// Which release of a tool a spec asks for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolVersion {
    /// No version: the tool of that name on `PATH`, which is what every
    /// project got before it could say anything else.
    OnPath,
    /// A numeric prefix — `26`, `1.4` — naming the newest release that starts
    /// with it. It is resolved against the publisher's index once and locked
    /// in `uf.lock`, so every machine runs the same release until somebody
    /// moves it.
    Prefix(CompactString),
    /// A full version — `24.14.0`, `1.3.0-canary.2` — naming exactly that
    /// release.
    Exact(CompactString),
}

impl ToolVersion {
    /// What follows the `@`, or `None` for a spec with no `@`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::OnPath => None,
            Self::Prefix(version) | Self::Exact(version) => Some(version),
        }
    }

    /// The major version, when the spec names one.
    ///
    /// What a Yarn spec's edition is decided by: `yarn@1` is Classic, and
    /// every later major is Berry.
    #[must_use]
    pub fn major(&self) -> Option<u64> {
        self.as_str()?.split(['.', '-', '+']).next()?.parse().ok()
    }

    /// Read what follows the `@` in a spec for `name`.
    fn parse(name: &'static str, version: &str) -> Result<Self, SpecError> {
        if version.is_empty() {
            return Err(SpecError::MissingVersion { name });
        }
        if is_prefix(version) {
            return Ok(Self::Prefix(version.into()));
        }
        if is_exact(version) {
            return Ok(Self::Exact(version.into()));
        }
        if looks_like_a_range(version) {
            return Err(SpecError::Range {
                name,
                major: first_number(version),
            });
        }
        // `v26`, which is how Node spells its own tags and how half of the
        // internet writes a Node version. Not accepted — a second spelling of
        // one version is a second thing to compare — but told exactly what to
        // write, because the intent is not in doubt.
        let without_v = version
            .strip_prefix(['v', 'V'])
            .filter(|rest| is_prefix(rest) || is_exact(rest))
            .map(str::to_owned);
        Err(SpecError::NotAVersion {
            name,
            version: version.to_owned(),
            without_v,
        })
    }
}

/// A tool and which release of it: `name[@version]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolSpec<N> {
    /// Which tool.
    pub name: N,
    /// Which release of it.
    pub version: ToolVersion,
}

/// What `runtime`, `build.runtime` and `test.runtime` hold: `node`, `bun` or
/// `deno`, optionally at a version.
///
/// The Rust side of `@uniflowed/config`'s `RuntimeSpec`.
pub type RuntimeSpec = ToolSpec<CapabilityJsHost>;

/// What `packageManager` holds: `npm`, `pnpm`, `yarn` or `bun`, optionally at
/// a version.
///
/// The Rust side of `@uniflowed/config`'s `PackageManagerSpec`.
pub type PackageManagerSpec = ToolSpec<PackageManagerName>;

impl<N: ToolName> ToolSpec<N> {
    /// Read a spec written for this role.
    ///
    /// # Errors
    ///
    /// When the name is not one this role takes, or the version is a range,
    /// a tag, or missing after an `@`. [`SpecError`]'s `Display` says which,
    /// and what to write instead.
    pub fn parse(written: &str) -> Result<Self, SpecError> {
        let written = written.trim();
        if written.is_empty() {
            return Err(SpecError::Empty {
                role: N::ROLE,
                names: name_list::<N>(),
            });
        }
        let (name, version) = match written.split_once('@') {
            Some((name, version)) => (name, Some(version)),
            None => (written, None),
        };
        let Some(tool) = N::ALL.iter().copied().find(|tool| tool.name() == name) else {
            return Err(SpecError::UnknownName {
                name: name.to_owned(),
                role: N::ROLE,
                names: name_list::<N>(),
            });
        };
        let version = match version {
            None => ToolVersion::OnPath,
            Some(version) => ToolVersion::parse(tool.name(), version)?,
        };
        Ok(Self {
            name: tool,
            version,
        })
    }
}

impl<N: ToolName> fmt::Display for ToolSpec<N> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.name.name())?;
        match self.version.as_str() {
            Some(version) => write!(out, "@{version}"),
            None => Ok(()),
        }
    }
}

/// What `test.runner` names when it is a string.
///
/// The Rust side of `@uniflowed/config`'s `TestRunnerSpec`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum TestRunnerSpec {
    /// `"uf"`: the runner built into this binary, and the default.
    #[default]
    Uf,
    /// `"bun"` or `"bun@1.4"`: `bun test`, on the Bun it names.
    ///
    /// `uf test` hands the files its discovery found to `bun test`, with uf's
    /// Flow preload loaded and `@uniflowed/test` resolving to `bun:test`, and
    /// reads the run's JUnit report back to say whether the suite passed.
    Bun(ToolVersion),
}

impl TestRunnerSpec {
    /// Read `test.runner`'s string form.
    ///
    /// # Errors
    ///
    /// When the name is neither `uf` nor `bun`, when `uf` is given a version,
    /// or when Bun's version is a range, a tag, or missing after an `@`.
    pub fn parse(written: &str) -> Result<Self, SpecError> {
        let written = written.trim();
        if written.is_empty() {
            return Err(SpecError::Empty {
                role: "a test runner",
                names: TEST_RUNNER_NAMES.to_owned(),
            });
        }
        let (name, version) = match written.split_once('@') {
            Some((name, version)) => (name, Some(version)),
            None => (written, None),
        };
        match (name, version) {
            ("uf", None) => Ok(Self::Uf),
            ("uf", Some(version)) => Err(SpecError::VersionedUfRunner {
                version: version.to_owned(),
            }),
            ("bun", None) => Ok(Self::Bun(ToolVersion::OnPath)),
            ("bun", Some(version)) => Ok(Self::Bun(ToolVersion::parse("bun", version)?)),
            (name, _) => Err(SpecError::UnknownName {
                name: name.to_owned(),
                role: "a test runner",
                names: TEST_RUNNER_NAMES.to_owned(),
            }),
        }
    }

    /// The runtime this runner runs on, when it brings its own.
    ///
    /// `bun test` runs on the Bun it names, so `runner: "bun@1.4"` is also a
    /// statement about `uf test`'s runtime — which is why `test.runtime` may be
    /// left out beside it, and why a `test.runtime` naming anything else is a
    /// contradiction. uf's own runner runs on whatever runtime the project
    /// chose, so it implies nothing.
    #[must_use]
    pub fn implied_runtime(&self) -> Option<RuntimeSpec> {
        match self {
            Self::Uf => None,
            Self::Bun(version) => Some(RuntimeSpec {
                name: CapabilityJsHost::Bun,
                version: version.clone(),
            }),
        }
    }

    /// The issue a runner is waiting on before `uf test` can run it.
    ///
    /// None, for either runner: `bun test` runs behind `uf test` since
    /// [`BUN_TEST_RUNNER_ISSUE`]. Kept rather than removed, because removing a
    /// public method is a breaking change to this crate's API, and the next
    /// runner that parses before it runs will need it again.
    #[must_use]
    pub const fn tracking_issue(&self) -> Option<u32> {
        match self {
            Self::Uf | Self::Bun(_) => None,
        }
    }
}

/// How a message lists the test runners.
const TEST_RUNNER_NAMES: &str = "`uf` or `bun`";

impl fmt::Display for TestRunnerSpec {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uf => out.write_str("uf"),
            Self::Bun(version) => match version.as_str() {
                Some(version) => write!(out, "bun@{version}"),
                None => out.write_str("bun"),
            },
        }
    }
}

/// What `build.builder` names.
///
/// The Rust side of `@uniflowed/config`'s `BuilderSpec`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BuilderSpec {
    /// `"vite"`, which is `@uniflowed/vite`: the builder uf ships, and the one
    /// a project that names none gets.
    ///
    /// A word rather than the package name because the package is uf's
    /// adapter for Vite, not Vite, and `"vite"` read as a specifier would be
    /// the `vite` package — which is not a builder.
    Vite,
    /// Any other string: a module specifier, resolved under the rules
    /// `builder.module` has always had — a package found up `node_modules`, or
    /// a path inside the project.
    Module(CompactString),
}

impl BuilderSpec {
    /// Read `build.builder`.
    ///
    /// # Errors
    ///
    /// Only when nothing is written: any other string is a specifier, and
    /// whether it names a builder is a question for the directory it resolves
    /// to rather than for the string.
    pub fn parse(written: &str) -> Result<Self, SpecError> {
        match written.trim() {
            "" => Err(SpecError::Empty {
                role: "a builder",
                names: "`vite` or a module specifier".to_owned(),
            }),
            "vite" => Ok(Self::Vite),
            specifier => Ok(Self::Module(specifier.into())),
        }
    }

    /// The module specifier uf resolves.
    #[must_use]
    pub fn module(&self) -> &str {
        match self {
            Self::Vite => VITE_BUILDER_MODULE,
            Self::Module(specifier) => specifier,
        }
    }
}

impl fmt::Display for BuilderSpec {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vite => out.write_str("vite"),
            Self::Module(specifier) => out.write_str(specifier),
        }
    }
}

/// A spec type that can be read from what a project wrote.
///
/// What lets [`Written`] be one type for every role.
pub trait ParseSpec: Sized + fmt::Display {
    /// An example of a spec for this role, for a message about a value that
    /// is not a string.
    const EXAMPLE: &'static str;

    /// Read what was written.
    ///
    /// # Errors
    ///
    /// When it is not a spec this role takes; see [`SpecError`].
    fn parse_spec(written: &str) -> Result<Self, SpecError>;
}

impl<N: ToolName> ParseSpec for ToolSpec<N> {
    const EXAMPLE: &'static str = N::EXAMPLE;

    fn parse_spec(written: &str) -> Result<Self, SpecError> {
        Self::parse(written)
    }
}

impl ParseSpec for TestRunnerSpec {
    const EXAMPLE: &'static str = "uf";

    fn parse_spec(written: &str) -> Result<Self, SpecError> {
        Self::parse(written)
    }
}

impl ParseSpec for BuilderSpec {
    const EXAMPLE: &'static str = "vite";

    fn parse_spec(written: &str) -> Result<Self, SpecError> {
        Self::parse(written)
    }
}

/// A spec as a project wrote it, and what uf read it as.
///
/// Both halves are kept, and a refusal is carried rather than raised while
/// deserializing. The reason is the message, not the data: an error raised
/// inside serde cannot name the key it was under, and it surfaces as a failure
/// to *parse* the file — which `uf dev` and `uf build` answer by starting a
/// JavaScript host to evaluate the config instead, so a typo in a version would
/// be reported, after a process had been started for nothing, as a config uf
/// could not evaluate. [`crate::validate_config`] raises it instead, as
/// [`ConfigError::ToolSpec`], with the key in the sentence.
///
/// Serialized as the text, so `uf inspect --json` prints what the project
/// wrote and a projection round-trips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written<T> {
    text: CompactString,
    parsed: Result<T, SpecError>,
}

impl<T: ParseSpec> Written<T> {
    /// Read `text` for this role, keeping the refusal if it is one.
    #[must_use]
    pub fn new(text: impl Into<CompactString>) -> Self {
        let text = text.into();
        let parsed = T::parse_spec(&text);
        Self { text, parsed }
    }
}

impl<T> Written<T> {
    /// What the project wrote.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// What uf read it as, or `None` when it was refused.
    ///
    /// A config that went through [`crate::validate_config`] has no refused
    /// specs in it, so every loaded config answers `Some` here.
    #[must_use]
    pub fn spec(&self) -> Option<&T> {
        self.parsed.as_ref().ok()
    }

    /// Why it was refused, when it was.
    #[must_use]
    pub fn error(&self) -> Option<&SpecError> {
        self.parsed.as_ref().err()
    }

    /// A value that is not a string at all, kept so the refusal can name its
    /// key.
    fn not_a_string(shown: String, example: &'static str) -> Self {
        Self {
            text: shown.into(),
            parsed: Err(SpecError::NotAString { example }),
        }
    }
}

impl<T: ParseSpec> From<T> for Written<T> {
    fn from(spec: T) -> Self {
        Self {
            text: spec.to_string().into(),
            parsed: Ok(spec),
        }
    }
}

impl<T> Serialize for Written<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

impl<'de, T: ParseSpec> Deserialize<'de> for Written<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(text) => Self::new(text),
            other => Self::not_a_string(other.to_string(), T::EXAMPLE),
        })
    }
}

/// `test.runner`, in either shape it has had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestRunnerConfig {
    /// A spec: `"uf"` or `"bun@1.4"`.
    Spec(Written<TestRunnerSpec>),
    /// The object form, which describes uf's own runner field by field.
    ///
    /// **Deprecated.** It still parses and `applicationTarget` in it is still
    /// read when `test.target` is absent; `uf test` says once which spelling
    /// replaces it. See
    /// [`UniflowedConfig::test_runner_deprecation`].
    Object(NativeTestRunnerConfig),
}

impl Serialize for TestRunnerConfig {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Spec(spec) => spec.serialize(serializer),
            Self::Object(object) => object.serialize(serializer),
        }
    }
}

/// Read by hand, because the two shapes are told apart by JSON type — a string
/// is a spec and an object is the old description — and an untagged enum
/// would answer a typo inside the object with "did not match any variant",
/// which names neither shape.
impl<'de> Deserialize<'de> for TestRunnerConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::String(text) => Ok(Self::Spec(Written::new(text))),
            object @ serde_json::Value::Object(_) => serde_json::from_value(object)
                .map(Self::Object)
                .map_err(|error| {
                    serde::de::Error::custom(
                        compact_str::format_compact!("test.runner: {error}").into_string(),
                    )
                }),
            other => Ok(Self::Spec(Written::not_a_string(
                other.to_string(),
                TestRunnerSpec::EXAMPLE,
            ))),
        }
    }
}

/// The object form's defaults, for a project that wrote a spec or nothing.
static NATIVE_RUNNER_DEFAULTS: LazyLock<NativeTestRunnerConfig> =
    LazyLock::new(NativeTestRunnerConfig::default);

impl crate::TestConfig {
    /// The native runner's settings: the object form when a project still
    /// writes it, and uf's defaults otherwise.
    ///
    /// `applicationTarget` is read only as the legacy fallback for
    /// `test.target`.
    #[must_use]
    pub fn native_runner(&self) -> &NativeTestRunnerConfig {
        match &self.runner {
            Some(TestRunnerConfig::Object(object)) => object,
            Some(TestRunnerConfig::Spec(_)) | None => &NATIVE_RUNNER_DEFAULTS,
        }
    }
}

/// Why a spec was refused.
///
/// Its `Display` is the clause that follows the key and what was written —
/// `test.runtime is \`node@^26\`, which is a range, …` — because the key is
/// the one thing a spec does not know about itself, and a refusal without it
/// sends the reader searching the file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecError {
    /// Nothing was written.
    Empty {
        /// The role, as `a runtime`.
        role: &'static str,
        /// What the role accepts, listed.
        names: String,
    },
    /// A name this role does not take.
    UnknownName {
        /// The name as written.
        name: String,
        /// The role, as `a runtime`.
        role: &'static str,
        /// What the role accepts, listed.
        names: String,
    },
    /// An `@` with nothing after it.
    MissingVersion {
        /// The tool.
        name: &'static str,
    },
    /// A range where a version belongs.
    Range {
        /// The tool.
        name: &'static str,
        /// The first number in the range, which is the prefix to suggest.
        major: Option<String>,
    },
    /// Something after the `@` that is not a version: `lts`, `latest`, `v26`.
    NotAVersion {
        /// The tool.
        name: &'static str,
        /// What was written after the `@`.
        version: String,
        /// The version without its leading `v`, when that is all that is wrong.
        without_v: Option<String>,
    },
    /// `uf@…`, for a runner that is this binary.
    VersionedUfRunner {
        /// What was written after the `@`.
        version: String,
    },
    /// A number, a list or an object where a spec belongs.
    NotAString {
        /// A spec for this role.
        example: &'static str,
    },
}

impl fmt::Display for SpecError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { role, names } => {
                write!(
                    out,
                    "which names nothing; name {role} — {names} — or leave the key out"
                )
            }
            Self::UnknownName { name, role, names } if name.is_empty() => write!(
                out,
                "which names no tool before the `@`; name {role} — {names}"
            ),
            Self::UnknownName { name, role, names } => {
                write!(
                    out,
                    "and `{name}` is not {role} uf runs — write one of {names}"
                )
            }
            Self::MissingVersion { name } => write!(
                out,
                "and nothing follows the `@`. Write `{name}` for whichever `{name}` is on PATH, \
                 or `{name}@<major>` for the newest release of that major, resolved once and \
                 locked in uf.lock"
            ),
            Self::Range {
                name,
                major: Some(major),
            } => write!(
                out,
                "which is a range, and a range is not an environment: it can resolve to a \
                 different release tomorrow. Write a version prefix instead — `{name}@{major}` \
                 is the newest {major}.x, resolved once and locked in uf.lock"
            ),
            Self::Range { name, major: None } => write!(
                out,
                "which is a range, and a range is not an environment: it can resolve to a \
                 different release tomorrow. Write a version prefix instead — `{name}@<major>` \
                 is the newest release of that major, resolved once and locked in uf.lock"
            ),
            Self::NotAVersion {
                name,
                version,
                without_v: Some(bare),
            } => write!(
                out,
                "and `{version}` is not a version as uf reads one — write `{name}@{bare}`, \
                 without the `v`"
            ),
            Self::NotAVersion { name, version, .. } => write!(
                out,
                "and `{version}` is not a version. Write a version prefix — `{name}@<major>` is \
                 the newest release of that major, resolved once and locked in uf.lock — or an \
                 exact release, `{name}@<major>.<minor>.<patch>`"
            ),
            Self::VersionedUfRunner { .. } => write!(
                out,
                "and `uf` is the runner built into this binary, so there is no release of it to \
                 pin — write `\"uf\"`"
            ),
            Self::NotAString { example } => write!(
                out,
                "which is not a string. A tool is written `name[@version]` — `\"{example}\"`"
            ),
        }
    }
}

impl std::error::Error for SpecError {}

/// `` `node`, `bun` or `deno` ``.
fn name_list<N: ToolName>() -> String {
    let names: Vec<String> = N::ALL
        .iter()
        .map(|tool| compact_str::format_compact!("`{}`", tool.name()).into_string())
        .collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => {
            compact_str::format_compact!("{} or {last}", rest.join(", ")).into_string()
        }
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// A numeric identifier as semver has one: digits, and no leading zero.
fn is_numeric_identifier(part: &str) -> bool {
    !part.is_empty()
        && part.bytes().all(|byte| byte.is_ascii_digit())
        && (part.len() == 1 || !part.starts_with('0'))
}

/// `26` or `1.4`: one or two numeric parts, and nothing else.
pub(crate) fn is_prefix(version: &str) -> bool {
    let mut parts = 0;
    for part in version.split('.') {
        if !is_numeric_identifier(part) {
            return false;
        }
        parts += 1;
    }
    (1..=2).contains(&parts)
}

/// `24.14.0`, `1.3.0-canary.2`, `4.9.2+sha.20260910`.
pub(crate) fn is_exact(version: &str) -> bool {
    let (core, suffix) = match version.find(['-', '+']) {
        Some(index) => version.split_at(index),
        None => (version, ""),
    };
    let mut parts = 0;
    for part in core.split('.') {
        if !is_numeric_identifier(part) {
            return false;
        }
        parts += 1;
    }
    parts == 3 && is_version_suffix(suffix)
}

/// A prerelease and build suffix, or nothing.
fn is_version_suffix(suffix: &str) -> bool {
    if suffix.is_empty() {
        return true;
    }
    let (prerelease, build) = match suffix.strip_prefix('-') {
        Some(rest) => match rest.split_once('+') {
            Some((prerelease, build)) => (Some(prerelease), Some(build)),
            None => (Some(rest), None),
        },
        None => match suffix.strip_prefix('+') {
            Some(build) => (None, Some(build)),
            None => return false,
        },
    };
    prerelease.is_none_or(are_identifiers) && build.is_none_or(are_identifiers)
}

fn are_identifiers(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Whether `version` is written the way a range is.
///
/// Read after the prefix and exact checks have failed, so a prerelease that
/// happens to contain an `x` has already been accepted by then.
fn looks_like_a_range(version: &str) -> bool {
    version.contains(|character: char| {
        matches!(character, '^' | '~' | '<' | '>' | '=' | '*' | '|' | ',')
            || character.is_whitespace()
    }) || version.split('.').any(|part| matches!(part, "x" | "X"))
}

/// The first run of digits, which is the major a range was about.
fn first_number(version: &str) -> Option<String> {
    let start = version.find(|character: char| character.is_ascii_digit())?;
    let digits: String = version[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    is_numeric_identifier(&digits).then_some(digits)
}

/// Where a tool a command runs came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    /// The key that declares this role: `build.runtime`, `runtime`,
    /// `test.runtime`, `test.runner`, `packageManager`, `build.builder`.
    Key(&'static str),
    /// A key for another role that implies this one: `test.runner` naming Bun
    /// is a statement about the test runtime too.
    ImpliedBy(&'static str),
    /// A spelling that is deprecated and still honoured: `builder.module`,
    /// `pm.packageManager`, or `test.runner` written as an object.
    Deprecated(&'static str),
    /// Nothing declared it, so uf's own default: `vite` for the builder, `uf`
    /// for the test runner.
    Default,
}

impl ToolSource {
    /// The key, when a key supplied it.
    #[must_use]
    pub const fn key(self) -> Option<&'static str> {
        match self {
            Self::Key(key) | Self::ImpliedBy(key) | Self::Deprecated(key) => Some(key),
            Self::Default => None,
        }
    }

    /// How the key supplied it, as one word: `declared`, `implied`,
    /// `deprecated` or `default`.
    #[must_use]
    pub const fn via(self) -> &'static str {
        match self {
            Self::Key(_) => "declared",
            Self::ImpliedBy(_) => "implied",
            Self::Deprecated(_) => "deprecated",
            Self::Default => "default",
        }
    }
}

impl fmt::Display for ToolSource {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(key) => out.write_str(key),
            Self::ImpliedBy(key) => write!(out, "implied by {key}"),
            Self::Deprecated(key) => write!(out, "{key}, deprecated"),
            Self::Default => out.write_str("uf's default"),
        }
    }
}

/// A tool, and the key it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredTool<T> {
    /// The tool.
    pub spec: T,
    /// Where it came from.
    pub source: ToolSource,
}

/// The places a tool is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolRole {
    /// What `uf start`, `uf run` and `uf exec` run on: `runtime`.
    Runtime,
    /// What `uf dev`, `uf build` and `uf preview` run on: `build.runtime`,
    /// then `runtime`.
    BuildRuntime,
    /// What `uf test` runs on: `test.runtime`, then the runner's own, then
    /// `runtime`.
    TestRuntime,
    /// What runs the suite: `test.runner`.
    TestRunner,
    /// What installs: `packageManager`, then `pm.packageManager`.
    PackageManager,
    /// What `uf dev`, `uf build`, `uf preview` and `uf start` drive:
    /// `build.builder`, then `builder.module`.
    Builder,
}

impl ToolRole {
    /// Every role, in the order a report lists them: what runs, then what
    /// installs, then what builds.
    pub const ALL: [Self; 6] = [
        Self::Runtime,
        Self::BuildRuntime,
        Self::TestRuntime,
        Self::TestRunner,
        Self::PackageManager,
        Self::Builder,
    ];

    /// What the role is called on a line of `uf inspect`.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::BuildRuntime => "build runtime",
            Self::TestRuntime => "test runtime",
            Self::TestRunner => "test runner",
            Self::PackageManager => "package manager",
            Self::Builder => "builder",
        }
    }

    /// The commands whose tool this is.
    #[must_use]
    pub const fn commands(self) -> &'static [&'static str] {
        match self {
            Self::Runtime => &["start", "run", "exec"],
            Self::BuildRuntime => &["dev", "build", "preview"],
            Self::TestRuntime | Self::TestRunner => &["test"],
            Self::PackageManager => &[
                "install",
                "add",
                "remove",
                "uninstall",
                "update",
                "patch",
                "pm",
                "catalog",
                "why",
                "ls",
                "audit",
                "search",
            ],
            Self::Builder => &["dev", "build", "preview", "start"],
        }
    }

    /// What a command does for this role when nothing declares a tool for it.
    ///
    /// Today's behaviour, named, because "not declared" on its own leaves a
    /// reader to guess what runs instead.
    #[must_use]
    pub const fn undeclared(self) -> &'static str {
        match self {
            Self::Runtime | Self::BuildRuntime | Self::TestRuntime => {
                "app.runtime.capabilityJsHost, looked up on PATH"
            }
            Self::PackageManager => "detected: package.json#packageManager, then the lockfile",
            // Both have a default, so neither is ever undeclared.
            Self::TestRunner | Self::Builder => "uf's default",
        }
    }

    /// The roles `uf <command>` reads, in [`Self::ALL`]'s order.
    pub fn for_command(command: &str) -> impl Iterator<Item = Self> + '_ {
        Self::ALL
            .into_iter()
            .filter(move |role| role.commands().contains(&command))
    }
}

/// One role, as `uf inspect` and `uf explain` report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDeclaration {
    /// Which role.
    pub role: ToolRole,
    /// What the role is called on a line of text.
    pub label: &'static str,
    /// The commands that run it.
    pub commands: &'static [&'static str],
    /// The spec as a project writes it — `bun@1.4`, `uf`, `vite` — or `None`
    /// when nothing declares one.
    pub spec: Option<String>,
    /// The key it came from, when a key supplied it.
    pub key: Option<&'static str>,
    /// `declared`, `implied`, `deprecated`, `default`, or `undeclared`.
    pub via: &'static str,
    /// What happens instead, when nothing declares one.
    pub undeclared: Option<&'static str>,
}

impl ToolDeclaration {
    fn new(role: ToolRole, found: Option<(String, ToolSource)>) -> Self {
        match found {
            Some((spec, source)) => Self {
                role,
                label: role.label(),
                commands: role.commands(),
                spec: Some(spec),
                key: source.key(),
                via: source.via(),
                undeclared: None,
            },
            None => Self {
                role,
                label: role.label(),
                commands: role.commands(),
                spec: None,
                key: None,
                via: "undeclared",
                undeclared: Some(role.undeclared()),
            },
        }
    }

    /// The value column of a report line: `node@26 (build.runtime)`.
    #[must_use]
    pub fn summary(&self) -> String {
        match (&self.spec, self.key) {
            (Some(spec), Some(key)) => match self.via {
                "implied" => {
                    compact_str::format_compact!("{spec} (implied by {key})").into_string()
                }
                "deprecated" => {
                    compact_str::format_compact!("{spec} ({key}, deprecated)").into_string()
                }
                _ => compact_str::format_compact!("{spec} ({key})").into_string(),
            },
            (Some(spec), None) => {
                compact_str::format_compact!("{spec} (uf's default)").into_string()
            }
            (None, _) => compact_str::format_compact!(
                "not declared — {}",
                self.undeclared.unwrap_or("uf's default")
            )
            .into_string(),
        }
    }
}

fn declared<T: Clone>(written: Option<&Written<T>>, source: ToolSource) -> Option<DeclaredTool<T>> {
    written.and_then(Written::spec).map(|spec| DeclaredTool {
        spec: spec.clone(),
        source,
    })
}

impl UniflowedConfig {
    /// What `uf start`, `uf run` and `uf exec` run on: `runtime`.
    #[must_use]
    pub fn runtime_tool(&self) -> Option<DeclaredTool<RuntimeSpec>> {
        declared(self.runtime.as_ref(), ToolSource::Key("runtime"))
    }

    /// What `uf dev`, `uf build` and `uf preview` run on: `build.runtime`,
    /// then `runtime`.
    #[must_use]
    pub fn build_runtime_tool(&self) -> Option<DeclaredTool<RuntimeSpec>> {
        declared(
            self.build.runtime.as_ref(),
            ToolSource::Key("build.runtime"),
        )
        .or_else(|| self.runtime_tool())
    }

    /// What `uf test` runs on: `test.runtime`, then the runtime the runner
    /// brings, then `runtime`.
    #[must_use]
    pub fn test_runtime_tool(&self) -> Option<DeclaredTool<RuntimeSpec>> {
        declared(self.test.runtime.as_ref(), ToolSource::Key("test.runtime"))
            .or_else(|| {
                let runner = self.test_runner_tool();
                runner.spec.implied_runtime().map(|spec| DeclaredTool {
                    spec,
                    source: ToolSource::ImpliedBy("test.runner"),
                })
            })
            .or_else(|| self.runtime_tool())
    }

    /// What runs the suite: `test.runner`, or uf's own when it is absent or
    /// written as the object that described uf's own.
    #[must_use]
    pub fn test_runner_tool(&self) -> DeclaredTool<TestRunnerSpec> {
        match &self.test.runner {
            Some(TestRunnerConfig::Spec(written)) => match written.spec() {
                Some(spec) => DeclaredTool {
                    spec: spec.clone(),
                    source: ToolSource::Key("test.runner"),
                },
                None => DeclaredTool {
                    spec: TestRunnerSpec::Uf,
                    source: ToolSource::Default,
                },
            },
            Some(TestRunnerConfig::Object(_)) => DeclaredTool {
                spec: TestRunnerSpec::Uf,
                source: ToolSource::Deprecated("test.runner"),
            },
            None => DeclaredTool {
                spec: TestRunnerSpec::Uf,
                source: ToolSource::Default,
            },
        }
    }

    /// What installs: `packageManager`, then `pm.packageManager` when it names
    /// a manager `packageManager` could name.
    ///
    /// `pm.packageManager: "uf"` is not one — uf's resolver has no release to
    /// pin — so it is not reported as a package manager here, and it goes on
    /// deciding detection exactly as it did.
    #[must_use]
    pub fn package_manager_tool(&self) -> Option<DeclaredTool<PackageManagerSpec>> {
        declared(
            self.package_manager.as_ref(),
            ToolSource::Key("packageManager"),
        )
        .or_else(|| {
            preference_name(self.pm.package_manager).map(|name| DeclaredTool {
                spec: PackageManagerSpec {
                    name,
                    version: ToolVersion::OnPath,
                },
                source: ToolSource::Deprecated("pm.packageManager"),
            })
        })
    }

    /// What `uf dev`, `uf build`, `uf preview` and `uf start` drive:
    /// `build.builder`, then `builder.module`, then `@uniflowed/vite`.
    #[must_use]
    pub fn builder_tool(&self) -> DeclaredTool<BuilderSpec> {
        if let Some(tool) = declared(
            self.build.builder.as_ref(),
            ToolSource::Key("build.builder"),
        ) {
            return tool;
        }
        match &self.builder.module {
            Some(module) => DeclaredTool {
                spec: BuilderSpec::Module(module.clone()),
                source: ToolSource::Deprecated("builder.module"),
            },
            None => DeclaredTool {
                spec: BuilderSpec::Vite,
                source: ToolSource::Default,
            },
        }
    }

    /// One role, for a report.
    #[must_use]
    pub fn tool_declaration(&self, role: ToolRole) -> ToolDeclaration {
        fn found<T: fmt::Display>(tool: DeclaredTool<T>) -> (String, ToolSource) {
            (tool.spec.to_string(), tool.source)
        }
        let found = match role {
            ToolRole::Runtime => self.runtime_tool().map(found),
            ToolRole::BuildRuntime => self.build_runtime_tool().map(found),
            ToolRole::TestRuntime => self.test_runtime_tool().map(found),
            ToolRole::TestRunner => Some(found(self.test_runner_tool())),
            ToolRole::PackageManager => self.package_manager_tool().map(found),
            ToolRole::Builder => Some(found(self.builder_tool())),
        };
        ToolDeclaration::new(role, found)
    }

    /// Every role, for `uf inspect`.
    #[must_use]
    pub fn tool_declarations(&self) -> Vec<ToolDeclaration> {
        ToolRole::ALL
            .into_iter()
            .map(|role| self.tool_declaration(role))
            .collect()
    }

    /// The sentence `uf env` prints for a project still writing
    /// `env.toolchain`.
    ///
    /// Concrete when it can be: a project that pins one runtime and one
    /// package manager is told the two lines to write. One that pins several
    /// runtimes is told the keys, because which of them builds and which tests
    /// is exactly what `env.toolchain` never said.
    #[must_use]
    pub fn toolchain_deprecation(&self) -> Option<String> {
        let toolchain = &self.env.toolchain;
        if toolchain.is_empty() {
            return None;
        }
        let spec = |name: &str| -> Option<String> {
            toolchain.get(name).map(|version| {
                compact_str::format_compact!("{name}@{}", version.trim()).into_string()
            })
        };
        let runtimes: Vec<String> = ["node", "bun", "deno"]
            .into_iter()
            .filter_map(spec)
            .collect();
        let managers: Vec<String> = ["npm", "pnpm", "yarn"]
            .into_iter()
            .filter_map(spec)
            .collect();
        let mut instead = Vec::new();
        match runtimes.as_slice() {
            [] => {}
            [one] => {
                instead.push(compact_str::format_compact!("`runtime: \"{one}\"`").into_string())
            }
            _ => instead.push(
                "`runtime`, `build.runtime` and `test.runtime`, each as `name@version`".to_owned(),
            ),
        }
        match managers.as_slice() {
            [] => {}
            [one] => instead
                .push(compact_str::format_compact!("`packageManager: \"{one}\"`").into_string()),
            _ => instead.push("`packageManager`, as `name@version`".to_owned()),
        }
        let instead = if instead.is_empty() {
            "`runtime` and `packageManager`, each as `name@version`".to_owned()
        } else {
            instead.join(" and ")
        };
        Some(
            compact_str::format_compact!(
                "env.toolchain says which tools this project has and not what each is for; declare \
             each where it is used instead — {instead}"
            )
            .into_string(),
        )
    }

    /// The sentence for a project still writing `builder.module`.
    #[must_use]
    pub fn builder_module_deprecation(&self) -> Option<String> {
        let module = self.builder.module.as_deref()?;
        let spec = if module == VITE_BUILDER_MODULE {
            "vite"
        } else {
            module
        };
        Some(
            compact_str::format_compact!(
                "builder.module is `build.builder` now, beside the build it describes — write \
             `build: {{ builder: \"{spec}\" }}`"
            )
            .into_string(),
        )
    }

    /// The sentence for a project still naming its manager in
    /// `pm.packageManager`.
    ///
    /// Only for a manager `packageManager` can name. `auto` is the default
    /// and says nothing; `uf` has no spelling in the new key, so a project
    /// writing it is not told to move something it cannot move.
    #[must_use]
    pub fn package_manager_deprecation(&self) -> Option<String> {
        let spec = match self.pm.package_manager {
            PackageManagerPreference::Auto | PackageManagerPreference::Uf => return None,
            // Classic is the 1.x line, which is what a prefix says.
            PackageManagerPreference::YarnClassic => "yarn@1",
            PackageManagerPreference::Yarn | PackageManagerPreference::YarnBerry => "yarn",
            PackageManagerPreference::Npm => "npm",
            PackageManagerPreference::Pnpm => "pnpm",
            PackageManagerPreference::Bun => "bun",
        };
        Some(compact_str::format_compact!(
            "pm.packageManager is the top-level `packageManager` now, which can pin a release as \
             well — write `packageManager: \"{spec}\"`"
        ).into_string())
    }

    /// The sentence for a project still writing `test.runner` as an object.
    ///
    /// Two sentences, because the object had one field a command reads.
    /// `applicationTarget` is inferred from `app.framework` when it is `auto`,
    /// so a project whose object says what the inference would say loses
    /// nothing by writing `runner: "uf"`. A project whose object overrides the
    /// inference would lose the override, and is told that rather than being
    /// told to delete a line that is doing something.
    #[must_use]
    pub fn test_runner_deprecation(&self) -> Option<String> {
        let Some(TestRunnerConfig::Object(object)) = &self.test.runner else {
            return None;
        };
        let inferred = match self.app.framework {
            FrameworkPreset::ReactNative => NativeTestApplicationTarget::ReactNative,
            FrameworkPreset::Uniflowed | FrameworkPreset::React => NativeTestApplicationTarget::Web,
        };
        let target = object.application_target;
        if target == NativeTestApplicationTarget::Auto || target == inferred {
            return Some(
                "test.runner as an object describes uf's own runner, which is `runner: \"uf\"` \
                 and the default — `test.target` follows `app.framework` — so the object \
                 can go"
                    .to_owned(),
            );
        }
        Some(compact_str::format_compact!(
            "test.runner as an object is deprecated in favour of `runner: \"uf\"`, and this one \
             sets `applicationTarget: \"{}\"` — write `test.target: \"{}\"` beside `runner: \"uf\"` \
             to keep that override",
            target_name(target),
            target_name(target)
        ).into_string())
    }

    /// Every tool deprecation this project would be told about, for
    /// `uf inspect`.
    #[must_use]
    pub fn tool_deprecations(&self) -> Vec<String> {
        [
            self.toolchain_deprecation(),
            self.builder_module_deprecation(),
            self.package_manager_deprecation(),
            self.test_runner_deprecation(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

fn target_name(target: NativeTestApplicationTarget) -> &'static str {
    match target {
        NativeTestApplicationTarget::Auto => "auto",
        NativeTestApplicationTarget::Web => "web",
        NativeTestApplicationTarget::ReactNative => "react-native",
    }
}

/// The manager a `pm.packageManager` value names, when `packageManager` could
/// name it.
fn preference_name(preference: PackageManagerPreference) -> Option<PackageManagerName> {
    match preference {
        PackageManagerPreference::Auto | PackageManagerPreference::Uf => None,
        PackageManagerPreference::Npm => Some(PackageManagerName::Npm),
        PackageManagerPreference::Pnpm => Some(PackageManagerName::Pnpm),
        PackageManagerPreference::Yarn
        | PackageManagerPreference::YarnClassic
        | PackageManagerPreference::YarnBerry => Some(PackageManagerName::Yarn),
        PackageManagerPreference::Bun => Some(PackageManagerName::Bun),
    }
}

/// `pm.packageManager`'s value, as a project writes it.
const fn preference_text(preference: PackageManagerPreference) -> &'static str {
    match preference {
        PackageManagerPreference::Auto => "auto",
        PackageManagerPreference::Uf => "uf",
        PackageManagerPreference::Npm => "npm",
        PackageManagerPreference::Pnpm => "pnpm",
        PackageManagerPreference::Yarn => "yarn",
        PackageManagerPreference::YarnClassic => "yarn-classic",
        PackageManagerPreference::YarnBerry => "yarn-berry",
        PackageManagerPreference::Bun => "bun",
    }
}

/// Whether `pm.packageManager` and `packageManager` say the same thing.
///
/// The same manager, and for Yarn the same edition when the spec's version
/// decides one: `yarn-classic` beside `yarn@4` is two answers.
fn preference_agrees(preference: PackageManagerPreference, spec: &PackageManagerSpec) -> bool {
    match (preference, spec.name) {
        (PackageManagerPreference::Auto, _)
        | (PackageManagerPreference::Npm, PackageManagerName::Npm)
        | (PackageManagerPreference::Pnpm, PackageManagerName::Pnpm)
        | (PackageManagerPreference::Bun, PackageManagerName::Bun) => true,
        (PackageManagerPreference::YarnClassic, PackageManagerName::Yarn) => {
            spec.version.major().is_none_or(|major| major == 1)
        }
        (
            PackageManagerPreference::Yarn | PackageManagerPreference::YarnBerry,
            PackageManagerName::Yarn,
        ) => spec.version.major().is_none_or(|major| major != 1),
        _ => false,
    }
}

/// Refuse a spec uf cannot read, a test runtime its runner contradicts, and a
/// deprecated key that disagrees with the key that replaced it.
///
/// Where the config is read, for the reason every check in
/// [`crate::validate_config`] gives: a declaration that cannot be honoured is
/// a failure at the line that made it, not in the command that would have
/// quietly done something else.
pub(crate) fn check(path: &Utf8Path, config: &UniflowedConfig) -> Result<(), ConfigError> {
    refuse(path, "runtime", config.runtime.as_ref())?;
    refuse(path, "packageManager", config.package_manager.as_ref())?;
    refuse(path, "build.runtime", config.build.runtime.as_ref())?;
    refuse(path, "build.builder", config.build.builder.as_ref())?;
    refuse(path, "test.runtime", config.test.runtime.as_ref())?;
    if let Some(TestRunnerConfig::Spec(written)) = &config.test.runner {
        refuse(path, "test.runner", Some(written))?;
    }

    // `test: { runtime: "node@26", runner: "bun@1.4" }`. A Bun runner runs on
    // the Bun it names, so one of the two lines is false, and which one is not
    // a guess uf gets to make.
    if let (Some(runtime), Some(implied)) = (
        config.test.runtime.as_ref().and_then(Written::spec),
        config.test_runner_tool().spec.implied_runtime(),
    ) && *runtime != implied
    {
        return Err(ConfigError::TestRuntimeContradictsRunner {
            path: path.to_path_buf(),
            runtime: runtime.to_string(),
            runner: config.test_runner_tool().spec.to_string(),
        });
    }

    if let (Some(builder), Some(module)) = (
        config.build.builder.as_ref().and_then(Written::spec),
        config.builder.module.as_deref(),
    ) && builder.module() != module
    {
        return Err(ConfigError::ToolKeysDisagree {
            path: path.to_path_buf(),
            key: "build.builder",
            written: builder.to_string(),
            legacy_key: "builder.module".to_owned(),
            legacy_written: module.to_owned(),
        });
    }

    if let Some(manager) = config.package_manager.as_ref().and_then(Written::spec)
        && !preference_agrees(config.pm.package_manager, manager)
    {
        return Err(ConfigError::ToolKeysDisagree {
            path: path.to_path_buf(),
            key: "packageManager",
            written: manager.to_string(),
            legacy_key: "pm.packageManager".to_owned(),
            legacy_written: preference_text(config.pm.package_manager).to_owned(),
        });
    }

    check_toolchain(path, config)
}

/// A key whose spec was refused, as the error that names it.
fn refuse<T>(
    path: &Utf8Path,
    key: &'static str,
    written: Option<&Written<T>>,
) -> Result<(), ConfigError> {
    match written.and_then(|written| written.error().map(|error| (written, error))) {
        Some((written, error)) => Err(ConfigError::ToolSpec {
            path: path.to_path_buf(),
            key,
            written: written.text().to_owned(),
            reason: error.to_string(),
        }),
        None => Ok(()),
    }
}

/// `env.toolchain` pins beside a key that names the same tool differently.
///
/// Equal as written, or an error. `runtime: "node@24"` beside
/// `env.toolchain.node: "24.14.0"` is not a pair that agrees: one asks for the
/// newest 24.x and the other for one release of it, and `uf env install` would
/// have to pick.
fn check_toolchain(path: &Utf8Path, config: &UniflowedConfig) -> Result<(), ConfigError> {
    if config.env.toolchain.is_empty() {
        return Ok(());
    }
    let runner_runtime = config.test_runner_tool().spec.implied_runtime();
    let runtimes = [
        ("runtime", config.runtime.as_ref().and_then(Written::spec)),
        (
            "build.runtime",
            config.build.runtime.as_ref().and_then(Written::spec),
        ),
        (
            "test.runtime",
            config.test.runtime.as_ref().and_then(Written::spec),
        ),
        ("test.runner", runner_runtime.as_ref()),
    ];
    let declared = runtimes
        .into_iter()
        .filter_map(|(key, spec)| spec.map(|spec| (key, spec.name.name(), spec.version.clone())))
        .chain(
            config
                .package_manager
                .as_ref()
                .and_then(Written::spec)
                .map(|spec| ("packageManager", spec.name.name(), spec.version.clone())),
        );
    for (key, name, version) in declared {
        let Some(pinned) = config.env.toolchain.get(name) else {
            continue;
        };
        let pinned = pinned.trim();
        if version == ToolVersion::Exact(pinned.into()) {
            continue;
        }
        let written = match version.as_str() {
            Some(version) => compact_str::format_compact!("{name}@{version}").into_string(),
            None => name.to_owned(),
        };
        return Err(ConfigError::ToolKeysDisagree {
            path: path.to_path_buf(),
            key,
            written,
            legacy_key: compact_str::format_compact!("env.toolchain.{name}").into_string(),
            legacy_written: pinned.to_owned(),
        });
    }
    Ok(())
}

/// What to do about a deprecated tool key that disagrees with the key that
/// replaced it: [`ConfigError::ToolKeysDisagree`]'s last sentence.
///
/// Delete the old line, always — it is the deprecated one — and for a pin, say
/// what the new key would have to say to mean that release, because a pin is
/// the one case where the reader may have meant the old value.
pub(crate) fn disagreement_fix(key: &str, legacy_key: &str, legacy_written: &str) -> String {
    match legacy_key {
        "builder.module" => {
            "delete `builder.module` — `build.builder` is the key that names the builder now"
                .to_owned()
        }
        "pm.packageManager" => "delete `pm.packageManager` — `packageManager` names the manager, \
                                and its release when you want one"
            .to_owned(),
        pin => {
            let name = pin.strip_prefix("env.toolchain.").unwrap_or(pin);
            compact_str::format_compact!(
                "delete `{pin}`, and write `{key}: \"{name}@{legacy_written}\"` if \
                 {legacy_written} is the release you mean"
            )
            .into_string()
        }
    }
}

#[cfg(test)]
mod tests;
