use serde::{Deserialize, Serialize};

/// Runtime standard a contract follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeStandard {
    /// WinterTC-compatible web runtime APIs.
    WinterTc,
}

/// Source language expected by the runtime contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLanguage {
    /// Flow-typed JavaScript.
    Flow,
}

/// JavaScript engine family used for user-code execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JavaScriptEngine {
    /// Host-provided JavaScript engine with declared capabilities.
    CapabilityJsHost,
    /// Hermes embedded by a future self-hosted runtime.
    Hermes,
}

/// Event-loop ownership model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventLoopModel {
    /// Node.js, Deno, or Bun owns the event loop.
    HostProvided,
    /// A deferred native runtime owns a libuv-compatible loop.
    RustNativeLibuvParity,
}

/// Native IO integration model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeIoModel {
    /// IO is reached through Capability JS Host adapters.
    HostCapabilityBindings,
    /// Future native streaming IO model.
    ZeroCopyStreaming,
}

/// Runtime host a project may target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeHost {
    /// Deferred self-hosted `uf` runtime.
    Uf,
    /// Node.js.
    Node,
    /// Bun.
    Bun,
    /// Deno.
    Deno,
    /// Edge runtime.
    Edge,
    /// Serverless runtime.
    Serverless,
    /// Container runtime.
    Container,
}

/// Runtime capability exposed to application code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeCapability {
    /// Fetch API.
    Fetch,
    /// Web streams.
    Streams,
    /// Request and response objects.
    RequestResponse,
    /// URL parsing and formatting.
    Url,
    /// Headers API.
    Headers,
    /// Cookie helpers.
    Cookies,
    /// Timers.
    Timers,
    /// File-system access.
    FileSystem,
    /// TCP sockets.
    Tcp,
    /// UDP sockets.
    Udp,
    /// TLS sockets.
    Tls,
    /// DNS lookups.
    Dns,
    /// Cron scheduling.
    Cron,
    /// S3-compatible object storage.
    S3,
    /// SigV4 request signing.
    #[serde(rename = "sigv4")]
    SigV4,
    /// Function deployment/runtime support.
    Functions,
    /// WebAssembly.
    WebAssembly,
    /// Workers.
    Workers,
    /// Server actions.
    ServerActions,
    /// React Server Components.
    ReactServerComponents,
    /// Native package execution.
    NativePackages,
    /// Native terminal UI rendering.
    TerminalUi,
}
