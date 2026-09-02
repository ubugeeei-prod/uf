#![deny(missing_docs)]
//! Runtime manager planning for `@uniflowed/rm`.

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_config::{RuntimeEngine, UniflowedConfig};
use uf_runtime::{RuntimeContract, RuntimeHost};

/// Inline host list used by the runtime manager.
pub type RuntimeHostList = SmallVec<[RuntimeHost; 8]>;

/// Inline acquisition and apply steps.
pub type RuntimeManagerSteps = SmallVec<[RuntimeManagerStep; 8]>;

/// Inline installer shell list.
pub type InstallerShellList = SmallVec<[InstallerShell; 8]>;

/// Inline platform list.
pub type InstallerPlatformList = SmallVec<[InstallerPlatform; 8]>;

/// Runtime manager plan inferred from `uf.config.js`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeManagerPlan {
    /// Default engine selected for this project.
    pub engine: RuntimeEngine,
    /// Runtime contract satisfied by the selected JavaScript host.
    pub contract: RuntimeContract,
    /// Hosts that must be available after adaptation.
    pub hosts: RuntimeHostList,
    /// Acquisition strategy for the default runtime.
    pub acquisition: RuntimeAcquisition,
    /// How the runtime manager applies the acquired runtime.
    pub application: RuntimeApplication,
    /// XDG-compliant layout for runtime storage and shims.
    pub xdg: XdgLayout,
    /// Installer surface for setup.uniflowed.dev.
    pub installer: InstallerPlan,
    /// Deterministic runtime manager steps.
    pub steps: RuntimeManagerSteps,
}

impl Default for RuntimeManagerPlan {
    fn default() -> Self {
        Self {
            engine: RuntimeEngine::Node,
            contract: RuntimeContract::capability_js_hosts(),
            hosts: smallvec::smallvec![RuntimeHost::Node, RuntimeHost::Deno, RuntimeHost::Bun,],
            acquisition: RuntimeAcquisition::Auto,
            application: RuntimeApplication::ConfigAndHost,
            xdg: XdgLayout::default(),
            installer: InstallerPlan::default(),
            steps: smallvec::smallvec![
                RuntimeManagerStep::ReadConfig,
                RuntimeManagerStep::InferRuntime,
                RuntimeManagerStep::DetectCapabilityHost,
                RuntimeManagerStep::ApplyAdapters,
                RuntimeManagerStep::VerifyDoctor,
            ],
        }
    }
}

impl RuntimeManagerPlan {
    /// Infer the runtime manager plan from the unified config.
    pub fn infer_from_config(config: &UniflowedConfig) -> Self {
        let mut plan = Self {
            engine: config.app.runtime.default,
            ..Self::default()
        };
        plan.hosts.clear();
        plan.hosts
            .push(runtime_engine_to_host(config.app.runtime.default));
        for engine in &config.app.runtime.compatibility {
            let host = runtime_engine_to_host(*engine);
            if !plan.hosts.contains(&host) {
                plan.hosts.push(host);
            }
        }
        plan
    }

    /// Return whether the default plan includes the uf runtime.
    pub fn includes_uf_runtime(&self) -> bool {
        self.hosts.contains(&RuntimeHost::Uf)
    }

    /// Return whether Hermes is the JavaScript engine for the contract.
    pub fn uses_hermes(&self) -> bool {
        self.contract.javascript_engine == uf_runtime::JavaScriptEngine::Hermes
    }

    /// Return whether the plan delegates JavaScript execution to a host runtime.
    pub fn uses_capability_js_host(&self) -> bool {
        self.contract.javascript_engine == uf_runtime::JavaScriptEngine::CapabilityJsHost
    }
}

/// Runtime version reference such as `uf@0.1.0`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReference {
    /// Runtime name.
    pub name: compact_str::CompactString,
    /// Runtime version.
    pub version: compact_str::CompactString,
}

impl RuntimeReference {
    /// Parse a runtime reference.
    pub fn parse(specifier: &str) -> Option<Self> {
        let (name, version) = specifier.split_once('@')?;
        if name.is_empty() || version.is_empty() {
            return None;
        }
        Some(Self {
            name: name.into(),
            version: version.into(),
        })
    }
}

/// XDG-compliant path layout for uf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XdgLayout {
    /// User config directory.
    pub config_dir: compact_str::CompactString,
    /// User data directory.
    pub data_dir: compact_str::CompactString,
    /// User cache directory.
    pub cache_dir: compact_str::CompactString,
    /// User state directory.
    pub state_dir: compact_str::CompactString,
    /// User runtime directory, when available.
    pub runtime_dir: Option<compact_str::CompactString>,
    /// User local executable directory.
    pub bin_dir: compact_str::CompactString,
    /// `uf` shim path.
    pub shim_path: compact_str::CompactString,
    /// Runtime versions directory.
    pub versions_dir: compact_str::CompactString,
}

impl Default for XdgLayout {
    fn default() -> Self {
        Self::from_home("$HOME")
    }
}

impl XdgLayout {
    /// Build the default XDG layout for a home directory.
    pub fn from_home(home: &str) -> Self {
        Self {
            config_dir: format_xdg(home, ".config/uniflowed"),
            data_dir: format_xdg(home, ".local/share/uniflowed"),
            cache_dir: format_xdg(home, ".cache/uniflowed"),
            state_dir: format_xdg(home, ".local/state/uniflowed"),
            runtime_dir: None,
            bin_dir: format_xdg(home, ".local/bin"),
            shim_path: format_xdg(home, ".local/bin/uf"),
            versions_dir: format_xdg(home, ".local/share/uniflowed/runtimes"),
        }
    }

    /// Build an XDG layout from explicit environment values.
    pub fn from_env(env: XdgEnv<'_>) -> Self {
        let config_base = absolute_or_default(env.config_home, env.home, ".config");
        let data_base = absolute_or_default(env.data_home, env.home, ".local/share");
        let cache_base = absolute_or_default(env.cache_home, env.home, ".cache");
        let state_base = absolute_or_default(env.state_home, env.home, ".local/state");
        let runtime_dir = env
            .runtime_dir
            .filter(|path| path.starts_with('/'))
            .map(|path| append_path(path, "uniflowed"));

        Self {
            config_dir: append_path(config_base.as_str(), "uniflowed"),
            data_dir: append_path(data_base.as_str(), "uniflowed"),
            cache_dir: append_path(cache_base.as_str(), "uniflowed"),
            state_dir: append_path(state_base.as_str(), "uniflowed"),
            runtime_dir,
            bin_dir: format_xdg(env.home, ".local/bin"),
            shim_path: format_xdg(env.home, ".local/bin/uf"),
            versions_dir: append_path(data_base.as_str(), "uniflowed/runtimes"),
        }
    }
}

fn format_xdg(home: &str, suffix: &str) -> compact_str::CompactString {
    let mut path = compact_str::CompactString::from(home);
    if !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(suffix);
    path
}

fn absolute_or_default(
    candidate: Option<&str>,
    home: &str,
    fallback: &str,
) -> compact_str::CompactString {
    candidate
        .filter(|path| path.starts_with('/'))
        .map_or_else(|| format_xdg(home, fallback), Into::into)
}

fn append_path(base: &str, suffix: &str) -> compact_str::CompactString {
    let mut path = compact_str::CompactString::from(base);
    if !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(suffix.trim_start_matches('/'));
    path
}

/// Raw XDG environment values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XdgEnv<'a> {
    /// Home directory.
    pub home: &'a str,
    /// XDG config base directory.
    pub config_home: Option<&'a str>,
    /// XDG data base directory.
    pub data_home: Option<&'a str>,
    /// XDG cache base directory.
    pub cache_home: Option<&'a str>,
    /// XDG state base directory.
    pub state_home: Option<&'a str>,
    /// XDG runtime base directory.
    pub runtime_dir: Option<&'a str>,
}

/// Installer plan for setup.uniflowed.dev.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerPlan {
    /// Installer endpoint.
    pub endpoint: compact_str::CompactString,
    /// Supported POSIX shells.
    pub shells: InstallerShellList,
    /// Supported platforms.
    pub platforms: InstallerPlatformList,
    /// Default install command.
    pub posix_command: compact_str::CompactString,
    /// Windows install command.
    pub windows_command: compact_str::CompactString,
}

impl Default for InstallerPlan {
    fn default() -> Self {
        Self {
            endpoint: compact_str::CompactString::const_new("https://setup.uniflowed.dev"),
            shells: smallvec::smallvec![
                InstallerShell::Sh,
                InstallerShell::Bash,
                InstallerShell::Zsh,
                InstallerShell::Ush,
            ],
            platforms: smallvec::smallvec![
                InstallerPlatform::Windows,
                InstallerPlatform::MacOs,
                InstallerPlatform::Linux,
            ],
            posix_command: compact_str::CompactString::const_new(
                "curl -fsSL https://setup.uniflowed.dev | sh",
            ),
            windows_command: compact_str::CompactString::const_new(
                "irm https://setup.uniflowed.dev/install.ps1 | iex",
            ),
        }
    }
}

/// Supported installer shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallerShell {
    /// POSIX sh.
    Sh,
    /// Bash.
    Bash,
    /// Zsh.
    Zsh,
    /// Universal shell.
    Ush,
}

/// Supported installer platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallerPlatform {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux.
    Linux,
}

/// Plan produced by `uf use`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeUsePlan {
    /// Requested runtime reference.
    pub requested: RuntimeReference,
    /// XDG-compliant layout.
    pub layout: XdgLayout,
    /// Whether normal `uf` invocations may auto-switch from config.
    pub auto_switch: bool,
    /// Steps needed to activate the runtime.
    pub steps: SmallVec<[RuntimeUseStep; 8]>,
}

impl RuntimeUsePlan {
    /// Build a use plan for a runtime reference.
    pub fn new(requested: RuntimeReference, layout: XdgLayout, auto_switch: bool) -> Self {
        Self {
            requested,
            layout,
            auto_switch,
            steps: smallvec::smallvec![
                RuntimeUseStep::ResolveVersion,
                RuntimeUseStep::DownloadRuntime,
                RuntimeUseStep::VerifyChecksum,
                RuntimeUseStep::InstallVersion,
                RuntimeUseStep::WriteShim,
                RuntimeUseStep::ActivateVersion,
            ],
        }
    }
}

/// Runtime activation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeUseStep {
    /// Resolve the requested version.
    ResolveVersion,
    /// Download the runtime archive.
    DownloadRuntime,
    /// Verify checksum and signature metadata.
    VerifyChecksum,
    /// Install the runtime under the XDG data directory.
    InstallVersion,
    /// Write or update the user-local `uf` shim.
    WriteShim,
    /// Mark the version active in the XDG state directory.
    ActivateVersion,
}

fn runtime_engine_to_host(engine: RuntimeEngine) -> RuntimeHost {
    match engine {
        RuntimeEngine::Uf => RuntimeHost::Uf,
        RuntimeEngine::Node => RuntimeHost::Node,
        RuntimeEngine::Bun => RuntimeHost::Bun,
        RuntimeEngine::Deno => RuntimeHost::Deno,
        RuntimeEngine::Edge => RuntimeHost::Edge,
        RuntimeEngine::Serverless => RuntimeHost::Serverless,
        RuntimeEngine::Container => RuntimeHost::Container,
    }
}

/// Runtime acquisition strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeAcquisition {
    /// Infer, fetch, and verify the runtime automatically.
    Auto,
}

/// Runtime application strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeApplication {
    /// Apply runtime settings to config-derived tasks and the current host.
    ConfigAndHost,
}

/// Deterministic runtime manager step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeManagerStep {
    /// Read `uf.config.js`.
    ReadConfig,
    /// Infer runtime requirements from app, server, and deploy config.
    InferRuntime,
    /// Detect the selected Node.js, Deno, or Bun host and its capabilities.
    DetectCapabilityHost,
    /// Acquire the selected runtime and native adapters.
    AcquireRuntime,
    /// Apply runtime adapters for every supported host.
    ApplyAdapters,
    /// Verify the applied runtime through doctor checks.
    VerifyDoctor,
}

#[cfg(test)]
mod tests;
