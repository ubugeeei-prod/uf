use camino::Utf8Path;
use serde::{Deserialize, Serialize};
use uf_runtime::{HostSupport, RuntimeHost};

use crate::{ConfigError, UniflowedConfig};

/// App runtime and deployment defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RuntimeConfig {
    /// The runtime this project is written for.
    ///
    /// Not the host a command starts — that is
    /// [`CapabilityJsHostConfig::default`], which is the key `uf dev`,
    /// `uf test` and `uf build` resolve through. This one is the project's own
    /// statement of what it targets, and [`check`] holds it to the runtimes uf
    /// has a host for.
    pub default: RuntimeEngine,
    /// The other runtimes this project states it runs on.
    pub compatibility: Vec<RuntimeEngine>,
    /// Capability JS Host configuration for Node.js, Deno, and Bun.
    pub capability_js_host: CapabilityJsHostConfig,
    /// Deploy-anywhere adapter defaults.
    pub deploy: DeployAnywhereConfig,
}

impl Default for RuntimeConfig {
    /// The runtimes a uf project runs on, and no others.
    ///
    /// This listed six, and three of them — `edge`, `serverless`,
    /// `container` — are rows `uf_runtime::HOSTS` grades **planned** with no
    /// Flow loader at all. The list was written into `.uf/install.json` under
    /// `runtimeManager.hosts`, which is documented as the hosts that must be
    /// available, and printed by `uf inspect --json`: every project uf
    /// installed recorded that it ran on three runtimes that cannot import a
    /// line of its source. That is ubugeeei-prod/uf#246's own sentence — a
    /// name in an enum is not compatibility — written by uf itself.
    ///
    /// So the default is what [`RuntimeEngine::is_a_host`] says is true, the
    /// same way [`DeployAnywhereConfig::default`] is what
    /// [`DeployAdapter::is_implemented`] says is true. A host that earns a
    /// Flow loader joins this list by earning it.
    fn default() -> Self {
        Self {
            default: RuntimeEngine::Node,
            compatibility: RuntimeEngine::ALL
                .iter()
                .copied()
                .filter(|engine| engine.is_a_host())
                .collect(),
            capability_js_host: CapabilityJsHostConfig::default(),
            deploy: DeployAnywhereConfig::default(),
        }
    }
}

/// Runtime engine target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeEngine {
    /// Deferred self-hosted `uf` runtime.
    Uf,
    /// Node.js.
    #[default]
    Node,
    /// Deno.
    Deno,
    /// Bun.
    Bun,
    /// Edge runtime.
    Edge,
    /// Serverless runtime.
    Serverless,
    /// Container runtime.
    Container,
}

impl RuntimeEngine {
    /// Every engine, host or not, in the order `uf_runtime::HOSTS` lists them.
    ///
    /// The variants stay even though [`check`] refuses four of them, and that
    /// is the point: a name uf accepted for a year is refused with a sentence
    /// naming the key its writer meant, rather than with serde's "unknown
    /// variant", which reads like a typo in a word that was correct.
    pub const ALL: &'static [Self] = &[
        Self::Node,
        Self::Bun,
        Self::Deno,
        Self::Edge,
        Self::Serverless,
        Self::Container,
        Self::Uf,
    ];

    /// The name a person writes in `uf.config.js`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uf => "uf",
            Self::Node => "node",
            Self::Deno => "deno",
            Self::Bun => "bun",
            Self::Edge => "edge",
            Self::Serverless => "serverless",
            Self::Container => "container",
        }
    }

    /// The row in `uf_runtime::HOSTS` this engine is a name for.
    #[must_use]
    pub const fn host(self) -> RuntimeHost {
        match self {
            Self::Uf => RuntimeHost::Uf,
            Self::Node => RuntimeHost::Node,
            Self::Deno => RuntimeHost::Deno,
            Self::Bun => RuntimeHost::Bun,
            Self::Edge => RuntimeHost::Edge,
            Self::Serverless => RuntimeHost::Serverless,
            Self::Container => RuntimeHost::Container,
        }
    }

    /// Whether uf can run a project on it.
    ///
    /// Read off the table rather than written down twice: an engine is a host
    /// when its row has a Flow loader, because a runtime that cannot import a
    /// Flow module cannot import a uf project's first file. `uf_config`'s own
    /// tests assert the two agree, so a host that gains a loader opens its
    /// name here and one that loses it closes it.
    #[must_use]
    pub fn is_a_host(self) -> bool {
        HostSupport::for_host(self.host()).loads_flow()
    }
}

/// Refuse a runtime this project could not run on.
///
/// The keys are a statement about the project, and until now they were a
/// statement nothing could be wrong: all seven names parsed, and four of them
/// named runtimes with no host. Naming one changed nothing a command does —
/// which host runs is [`CapabilityJsHostConfig::default`] — and it changed two
/// things a reader sees: `uf explain` printed it as the JavaScript host, and
/// `.uf/install.json` recorded it among the hosts that must be available.
///
/// Refused where it is written, for the reason `check_cache_switches` refuses
/// an unimplemented cache: the request cannot be honoured, and the failure
/// belongs at the place the request was made rather than in a manifest
/// somebody reads later. See ubugeeei-prod/uf#246.
pub(crate) fn check(path: &Utf8Path, config: &UniflowedConfig) -> Result<(), ConfigError> {
    let runtime = &config.app.runtime;
    for (key, engine) in std::iter::once(("app.runtime.default", runtime.default)).chain(
        runtime
            .compatibility
            .iter()
            .map(|engine| ("app.runtime.compatibility", *engine)),
    ) {
        if engine.is_a_host() {
            continue;
        }
        let support = HostSupport::for_host(engine.host());
        return Err(ConfigError::RuntimeEngineWithoutHost {
            path: path.to_path_buf(),
            key,
            engine: engine.as_str(),
            level: support.level.as_str(),
            hosts: host_names(),
            tracking: support.tracking_issue.map_or_else(
                || String::from("docs/hosts.md"),
                |issue| format!("docs/hosts.md and ubugeeei-prod/uf#{issue}"),
            ),
        });
    }
    Ok(())
}

/// The engines a project may name, as the message lists them.
fn host_names() -> String {
    RuntimeEngine::ALL
        .iter()
        .filter(|engine| engine.is_a_host())
        .map(|engine| format!("`{}`", engine.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Host-provided JavaScript engine selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct CapabilityJsHostConfig {
    /// Default host for local execution.
    pub default: CapabilityJsHost,
    /// Accepted host set.
    pub hosts: Vec<CapabilityJsHost>,
    /// Whether `uf` should infer an installed host.
    pub auto_detect: bool,
}

impl Default for CapabilityJsHostConfig {
    fn default() -> Self {
        Self {
            default: CapabilityJsHost::Node,
            hosts: vec![
                CapabilityJsHost::Node,
                CapabilityJsHost::Deno,
                CapabilityJsHost::Bun,
            ],
            auto_detect: true,
        }
    }
}

/// Builtin host-provided JavaScript engine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityJsHost {
    /// Node.js.
    #[default]
    Node,
    /// Deno.
    Deno,
    /// Bun.
    Bun,
}

impl CapabilityJsHost {
    /// The name a person writes, which is also the executable's name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Deno => "deno",
            Self::Bun => "bun",
        }
    }
}

/// Deploy-anywhere adapter selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DeployAnywhereConfig {
    /// Whether deploy adapter planning is enabled.
    pub enabled: bool,
    /// The adapter `uf build` writes an artefact for when none is named on the
    /// command line. `None` means `uf build` writes what it always wrote.
    pub adapter: Option<DeployAdapter>,
    /// Adapters this toolchain can produce an artefact for.
    pub adapters: Vec<DeployAdapter>,
}

impl Default for DeployAnywhereConfig {
    /// The adapters that write something, and no others.
    ///
    /// This listed all seven, and every one of them was a name. Grepping the
    /// workspace for `DeployAdapter` outside this crate found nothing, so the
    /// list described a feature rather than reporting one — which is precisely
    /// what `docs/red-lines.md` closes with and what ubugeeei-prod/uf#250 and
    /// ubugeeei-prod/uf#335 are about. The default is what
    /// [`DeployAdapter::is_implemented`] says is true, and naming one of the
    /// rest is an error that says which issue tracks it rather than a build
    /// that quietly produces nothing.
    fn default() -> Self {
        Self {
            enabled: true,
            adapter: None,
            adapters: DeployAdapter::ALL
                .iter()
                .copied()
                .filter(|adapter| adapter.is_implemented())
                .collect(),
        }
    }
}

/// Deployment adapter target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeployAdapter {
    /// Node.js deployment.
    Node,
    /// Bun deployment.
    Bun,
    /// Deno deployment.
    Deno,
    /// Edge deployment.
    Edge,
    /// Serverless deployment.
    Serverless,
    /// Static deployment.
    Static,
    /// Container deployment.
    Container,
}

impl DeployAdapter {
    /// Every adapter, implemented or not, in the order they are documented.
    pub const ALL: &'static [Self] = &[
        Self::Node,
        Self::Bun,
        Self::Deno,
        Self::Edge,
        Self::Serverless,
        Self::Static,
        Self::Container,
    ];

    /// The name a person writes, in `uf.config.js` and after `--adapter`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Bun => "bun",
            Self::Deno => "deno",
            Self::Edge => "edge",
            Self::Serverless => "serverless",
            Self::Static => "static",
            Self::Container => "container",
        }
    }

    /// Whether `uf build --adapter` can actually write this one.
    ///
    /// The distinction is load-bearing rather than documentary: it is what
    /// makes `uf build --adapter bun` an error naming an issue instead of a
    /// command that appears to work. Keep it in step with
    /// `docs/app/reference/cli/_uf.page.mdx`, which is where a reader looks
    /// first.
    ///
    /// "Writes something the platform accepts, in that platform's documented
    /// shape, driven by a test" — and not "has been deployed". None of the
    /// five server targets has ever run on the platform it targets; the
    /// sandbox uf is developed in has no credentials for any cloud and cannot
    /// bind a socket. What the tests establish is the emitted file set, the
    /// emitted handler's answers in process, and that those answers match
    /// `uf start`'s. The documentation says the same thing in the same words.
    ///
    /// `static` is the exception to that caveat rather than to the rule: its
    /// output is `dist/`, which every other command already serves, and the
    /// thing it had to grow was the refusal — so what its tests establish is
    /// that a project needing a server is named and refused, and that a
    /// project that needs none is copied. Neither of those needs a platform.
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        matches!(
            self,
            Self::Node | Self::Bun | Self::Edge | Self::Serverless | Self::Static | Self::Container
        )
    }

    /// The issue that tracks an adapter nobody has written yet.
    ///
    /// One issue rather than one per target, because what is left is one
    /// target: `deno`. It needs the benchmark `bun` has now had — the `node`
    /// output already runs unchanged on both, so an adapter that is not
    /// measurably faster is a directory with a different name on it — and the
    /// machine uf is developed on has no Deno to run it with.
    #[must_use]
    pub const fn tracking_issue(self) -> Option<u32> {
        if self.is_implemented() {
            None
        } else {
            Some(391)
        }
    }

    /// What each unwritten adapter is still waiting for, in one clause.
    ///
    /// Beside [`Self::tracking_issue`] because a reader who has just been told
    /// "not yet" wants to know whether that means "nobody got to it" or "the
    /// design says not to" — and for these three it is the second. `None` for
    /// an adapter that is implemented, which is what makes the two functions
    /// answer the same question.
    #[must_use]
    pub const fn unimplemented_because(self) -> Option<&'static str> {
        match self {
            Self::Deno => Some(
                "the `node` output already runs unchanged on Deno, so this is worth writing \
                 only once a benchmark shows the native server beating `node:http` under the \
                 same handler — as one now has for Bun, on a machine that had no Deno to \
                 measure",
            ),
            _ => None,
        }
    }
}

/// Development/server runtime settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerConfig {
    /// Server implementation.
    pub engine: ServerEngine,
    /// Native Rust server defaults.
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

/// Server implementation kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerEngine {
    /// Native Rust server implementation.
    #[default]
    NativeRust,
}

/// Native server adapter settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct NativeServerConfig {
    /// Whether streaming responses are enabled.
    pub streaming: bool,
    /// Whether zero-copy HTTP paths are enabled.
    pub zero_copy_http: bool,
    /// Supported runtime adapters.
    pub adapters: Vec<NativeServerAdapter>,
}

impl Default for NativeServerConfig {
    fn default() -> Self {
        Self {
            streaming: true,
            zero_copy_http: true,
            adapters: vec![
                NativeServerAdapter::Node,
                NativeServerAdapter::Deno,
                NativeServerAdapter::Bun,
                NativeServerAdapter::Edge,
                NativeServerAdapter::Serverless,
                NativeServerAdapter::Container,
            ],
        }
    }
}

/// Native server adapter target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeServerAdapter {
    /// Deferred self-hosted `uf` runtime adapter.
    Uf,
    /// Node.js adapter.
    Node,
    /// Bun adapter.
    Bun,
    /// Deno adapter.
    Deno,
    /// Edge adapter.
    Edge,
    /// Serverless adapter.
    Serverless,
    /// Container adapter.
    Container,
}
