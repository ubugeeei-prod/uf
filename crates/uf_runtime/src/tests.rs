use super::*;

#[test]
fn default_contract_is_wintertc_flow_over_capability_js_hosts() {
    let contract = RuntimeContract::default();

    assert_eq!(contract.standard, RuntimeStandard::WinterTc);
    assert_eq!(contract.language, RuntimeLanguage::Flow);
    assert_eq!(
        contract.javascript_engine,
        JavaScriptEngine::CapabilityJsHost
    );
    assert_eq!(contract.event_loop, EventLoopModel::HostProvided);
    assert_eq!(contract.io, NativeIoModel::HostCapabilityBindings);
}

#[test]
fn default_contract_targets_capability_js_hosts() {
    let contract = RuntimeContract::default();

    assert!(contract.supports_host(RuntimeHost::Node));
    assert!(contract.supports_host(RuntimeHost::Deno));
    assert!(contract.supports_host(RuntimeHost::Bun));
    assert!(!contract.supports_host(RuntimeHost::Uf));
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
    assert!(contract.has_capability(RuntimeCapability::TerminalUi));
}

#[test]
fn postponed_hermes_contract_remains_explicit() {
    let contract = RuntimeContract::wintertc_hermes_native();

    assert_eq!(contract.javascript_engine, JavaScriptEngine::Hermes);
    assert_eq!(contract.event_loop, EventLoopModel::RustNativeLibuvParity);
    assert_eq!(contract.io, NativeIoModel::ZeroCopyStreaming);
    assert!(contract.supports_host(RuntimeHost::Uf));
    assert!(contract.has_capability(RuntimeCapability::NativePackages));
}
