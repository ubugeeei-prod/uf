use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type CapabilityList = SmallVec<[RuntimeCapability; 16]>;
pub type HostList = SmallVec<[RuntimeHost; 8]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeContract {
    pub standard: RuntimeStandard,
    pub language: RuntimeLanguage,
    pub javascript_engine: JavaScriptEngine,
    pub event_loop: EventLoopModel,
    pub io: NativeIoModel,
    pub capabilities: CapabilityList,
    pub hosts: HostList,
}

impl Default for RuntimeContract {
    fn default() -> Self {
        Self {
            standard: RuntimeStandard::WinterTc,
            language: RuntimeLanguage::Flow,
            javascript_engine: JavaScriptEngine::Hermes,
            event_loop: EventLoopModel::RustNativeLibuvParity,
            io: NativeIoModel::ZeroCopyStreaming,
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
                RuntimeCapability::NativePackages,
            ],
            hosts: smallvec::smallvec![
                RuntimeHost::Uf,
                RuntimeHost::Node,
                RuntimeHost::Bun,
                RuntimeHost::Deno,
                RuntimeHost::Edge,
                RuntimeHost::Serverless,
                RuntimeHost::Container,
            ],
        }
    }
}

impl RuntimeContract {
    pub fn supports_host(&self, host: RuntimeHost) -> bool {
        self.hosts.contains(&host)
    }

    pub fn has_capability(&self, capability: RuntimeCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn wintertc_hermes_native() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeStandard {
    WinterTc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeLanguage {
    Flow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JavaScriptEngine {
    Hermes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventLoopModel {
    RustNativeLibuvParity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeIoModel {
    ZeroCopyStreaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeHost {
    Uf,
    Node,
    Bun,
    Deno,
    Edge,
    Serverless,
    Container,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeCapability {
    Fetch,
    Streams,
    RequestResponse,
    Url,
    Headers,
    Cookies,
    Timers,
    FileSystem,
    Tcp,
    Udp,
    Tls,
    Dns,
    Cron,
    S3,
    #[serde(rename = "sigv4")]
    SigV4,
    Functions,
    WebAssembly,
    Workers,
    ServerActions,
    ReactServerComponents,
    NativePackages,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contract_is_wintertc_hermes_and_rust_native() {
        let contract = RuntimeContract::default();

        assert_eq!(contract.standard, RuntimeStandard::WinterTc);
        assert_eq!(contract.language, RuntimeLanguage::Flow);
        assert_eq!(contract.javascript_engine, JavaScriptEngine::Hermes);
        assert_eq!(contract.event_loop, EventLoopModel::RustNativeLibuvParity);
        assert_eq!(contract.io, NativeIoModel::ZeroCopyStreaming);
    }

    #[test]
    fn default_contract_targets_deploy_anywhere_hosts() {
        let contract = RuntimeContract::default();

        assert!(contract.supports_host(RuntimeHost::Uf));
        assert!(contract.supports_host(RuntimeHost::Node));
        assert!(contract.supports_host(RuntimeHost::Bun));
        assert!(contract.supports_host(RuntimeHost::Deno));
        assert!(contract.supports_host(RuntimeHost::Edge));
        assert!(contract.supports_host(RuntimeHost::Serverless));
        assert!(contract.supports_host(RuntimeHost::Container));
    }

    #[test]
    fn default_contract_exposes_server_and_io_capabilities() {
        let contract = RuntimeContract::default();

        assert!(contract.has_capability(RuntimeCapability::Fetch));
        assert!(contract.has_capability(RuntimeCapability::Streams));
        assert!(contract.has_capability(RuntimeCapability::Tcp));
        assert!(contract.has_capability(RuntimeCapability::Tls));
        assert!(contract.has_capability(RuntimeCapability::Dns));
        assert!(contract.has_capability(RuntimeCapability::Cron));
        assert!(contract.has_capability(RuntimeCapability::S3));
        assert!(contract.has_capability(RuntimeCapability::SigV4));
        assert!(contract.has_capability(RuntimeCapability::Functions));
        assert!(contract.has_capability(RuntimeCapability::WebAssembly));
        assert!(contract.has_capability(RuntimeCapability::ServerActions));
        assert!(contract.has_capability(RuntimeCapability::ReactServerComponents));
        assert!(contract.has_capability(RuntimeCapability::NativePackages));
    }
}
