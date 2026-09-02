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
    /// Supported adapters.
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
