use serde::{Deserialize, Serialize};

/// App runtime and deployment defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RuntimeConfig {
    /// Default JavaScript host.
    pub default: RuntimeEngine,
    /// Compatible runtime/deployment targets.
    pub compatibility: Vec<RuntimeEngine>,
    /// Capability JS Host configuration for Node.js, Deno, and Bun.
    pub capability_js_host: CapabilityJsHostConfig,
    /// Deploy-anywhere adapter defaults.
    pub deploy: DeployAnywhereConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            default: RuntimeEngine::Node,
            compatibility: vec![
                RuntimeEngine::Node,
                RuntimeEngine::Deno,
                RuntimeEngine::Bun,
                RuntimeEngine::Edge,
                RuntimeEngine::Serverless,
                RuntimeEngine::Container,
            ],
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
    /// One adapter, because there is one implementation.
    ///
    /// This listed all seven, and every one of them was a name. Grepping the
    /// workspace for `DeployAdapter` outside this crate found nothing, so the
    /// list described a feature rather than reporting one — which is precisely
    /// what `docs/red-lines.md` closes with and what ubugeeei-prod/uf#250 and
    /// ubugeeei-prod/uf#335 are about. The default is now what
    /// [`DeployAdapter::is_implemented`] says is true, and naming any of the
    /// other six is an error that says which issue tracks it rather than a
    /// build that quietly produces nothing.
    fn default() -> Self {
        Self {
            enabled: true,
            adapter: None,
            adapters: vec![DeployAdapter::Node],
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
    /// makes `uf build --adapter edge` an error naming an issue instead of a
    /// command that appears to work. Keep it in step with
    /// `docs/app/reference/cli/_uf.page.mdx`, which is where a reader looks
    /// first.
    #[must_use]
    pub const fn is_implemented(self) -> bool {
        matches!(self, Self::Node)
    }

    /// The issue that tracks an adapter nobody has written yet.
    ///
    /// One issue rather than one per target, because they are one piece of
    /// work: the application half is already shared (`@uniflowed/server/fetch`),
    /// and what each of the six still needs is its own entry file and its own
    /// answer for where the static half lives.
    #[must_use]
    pub const fn tracking_issue(self) -> Option<u32> {
        if self.is_implemented() {
            None
        } else {
            Some(391)
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
