use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::kind::{
    EventLoopModel, JavaScriptEngine, NativeIoModel, RuntimeCapability, RuntimeHost,
    RuntimeLanguage, RuntimeStandard,
};

/// Capability list storage optimized for the default contract size.
pub type CapabilityList = SmallVec<[RuntimeCapability; 16]>;

/// Supported host storage optimized for the default host set.
pub type HostList = SmallVec<[RuntimeHost; 8]>;

/// The execution contract a generated application expects from its JavaScript host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeContract {
    /// Web/runtime standard the contract follows.
    pub standard: RuntimeStandard,
    /// User-authored language expected by the toolchain.
    pub language: RuntimeLanguage,
    /// JavaScript engine family.
    pub javascript_engine: JavaScriptEngine,
    /// Event-loop ownership model.
    pub event_loop: EventLoopModel,
    /// Native IO integration model.
    pub io: NativeIoModel,
    /// Capabilities the app may rely on.
    pub capabilities: CapabilityList,
    /// Hosts that satisfy this contract.
    pub hosts: HostList,
}

impl Default for RuntimeContract {
    fn default() -> Self {
        Self {
            standard: RuntimeStandard::WinterTc,
            language: RuntimeLanguage::Flow,
            javascript_engine: JavaScriptEngine::CapabilityJsHost,
            event_loop: EventLoopModel::HostProvided,
            io: NativeIoModel::HostCapabilityBindings,
            capabilities: smallvec::smallvec![
                RuntimeCapability::Fetch,
                RuntimeCapability::Streams,
                RuntimeCapability::RequestResponse,
                RuntimeCapability::Url,
                RuntimeCapability::Headers,
                RuntimeCapability::Cookies,
                RuntimeCapability::Timers,
                RuntimeCapability::FileSystem,
                RuntimeCapability::Tcp,
                RuntimeCapability::Udp,
                RuntimeCapability::Tls,
                RuntimeCapability::Dns,
                RuntimeCapability::Cron,
                RuntimeCapability::S3,
                RuntimeCapability::SigV4,
                RuntimeCapability::Functions,
                RuntimeCapability::WebAssembly,
                RuntimeCapability::Workers,
                RuntimeCapability::ServerActions,
                RuntimeCapability::ReactServerComponents,
                RuntimeCapability::TerminalUi,
            ],
            hosts: smallvec::smallvec![RuntimeHost::Node, RuntimeHost::Deno, RuntimeHost::Bun,],
        }
    }
}

impl RuntimeContract {
    /// Whether a host satisfies the contract.
    pub fn supports_host(&self, host: RuntimeHost) -> bool {
        self.hosts.contains(&host)
    }

    /// Whether a capability is included in the contract.
    pub fn has_capability(&self, capability: RuntimeCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Default runtime-agnostic JavaScript host contract.
    pub fn capability_js_hosts() -> Self {
        Self::default()
    }

    /// Future native runtime contract, kept explicit while runtime work is deferred.
    pub fn wintertc_hermes_native() -> Self {
        Self {
            javascript_engine: JavaScriptEngine::Hermes,
            event_loop: EventLoopModel::RustNativeLibuvParity,
            io: NativeIoModel::ZeroCopyStreaming,
            capabilities: {
                let mut capabilities = Self::default().capabilities;
                capabilities.push(RuntimeCapability::NativePackages);
                capabilities
            },
            hosts: smallvec::smallvec![
                RuntimeHost::Uf,
                RuntimeHost::Node,
                RuntimeHost::Deno,
                RuntimeHost::Bun,
                RuntimeHost::Edge,
                RuntimeHost::Serverless,
                RuntimeHost::Container,
            ],
            ..Self::default()
        }
    }
}
