#![deny(missing_docs)]
//! Native package manager planning for `uf install`, `uf upgrade`, and `@uniflowed/pm`.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_config::UniflowedConfig;

/// Inline package list used for small workspaces without heap-heavy metadata.
pub type PackageList = SmallVec<[WorkspacePackage; 8]>;

/// Inline step list for deterministic install and upgrade planning.
pub type PackageManagerSteps = SmallVec<[PackageManagerStep; 8]>;

/// Native package manager plan inferred from `uf.config.flow`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerPlan {
    /// Resolver backend used by the package manager.
    pub resolver: PackageResolver,
    /// Lockfile name written by `uf install`.
    pub lockfile: CompactString,
    /// Registry used when a package has no explicit source override.
    pub registry: CompactString,
    /// Script execution policy for package manifests.
    pub scripts: PackageScriptPolicy,
    /// Store and cache strategy.
    pub store: PackageStore,
    /// Package link strategy used when applying the resolved graph.
    pub link_mode: PackageLinkMode,
    /// Planned packages for the current workspace.
    pub workspace_packages: PackageList,
    /// Deterministic install steps.
    pub steps: PackageManagerSteps,
}

impl Default for PackageManagerPlan {
    fn default() -> Self {
        Self {
            resolver: PackageResolver::UfNative,
            lockfile: CompactString::const_new("uf.lock"),
            registry: CompactString::const_new("https://registry.npmjs.org"),
            scripts: PackageScriptPolicy::Forbid,
            store: PackageStore {
                strategy: PackageStoreStrategy::ContentAddressed,
                directory: CompactString::const_new(".uf/store"),
            },
            link_mode: PackageLinkMode::HardlinkThenCopy,
            workspace_packages: SmallVec::new(),
            steps: smallvec::smallvec![
                PackageManagerStep::ReadConfig,
                PackageManagerStep::ResolveGraph,
                PackageManagerStep::VerifyIntegrity,
                PackageManagerStep::WriteLockfile,
                PackageManagerStep::ApplyStore,
                PackageManagerStep::LinkWorkspace,
            ],
        }
    }
}

impl PackageManagerPlan {
    /// Infer the native package manager contract from the unified config.
    pub fn infer_from_config(config: &UniflowedConfig) -> Self {
        Self {
            lockfile: CompactString::from(config.pm.lockfile.as_str()),
            registry: CompactString::from(config.publish.registry.as_str()),
            scripts: PackageScriptPolicy::from_config(config.pm.allow_lifecycle_scripts),
            store: PackageStore {
                strategy: PackageStoreStrategy::ContentAddressed,
                directory: CompactString::from(config.pm.store_dir.as_str()),
            },
            ..Self::default()
        }
    }

    /// Return whether npm-style manifest scripts are blocked.
    pub fn forbids_npm_scripts(&self) -> bool {
        self.scripts == PackageScriptPolicy::Forbid
    }

    /// Add a workspace package to the plan without changing the resolver contract.
    pub fn with_workspace_package(mut self, name: &str, path: &str) -> Self {
        self.workspace_packages.push(WorkspacePackage {
            name: name.to_compact_string(),
            path: path.to_compact_string(),
        });
        self
    }
}

/// Resolver backend used by the native package manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageResolver {
    /// The self-hosted Rust resolver owned by uf.
    UfNative,
}

/// Manifest script execution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageScriptPolicy {
    /// Reject npm scripts and lifecycle hooks.
    Forbid,
    /// Allow lifecycle hooks only when explicitly opted in.
    ExplicitOptIn,
}

impl PackageScriptPolicy {
    fn from_config(allow_lifecycle_scripts: bool) -> Self {
        if allow_lifecycle_scripts {
            Self::ExplicitOptIn
        } else {
            Self::Forbid
        }
    }
}

/// Content store description for package tarballs and native artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageStore {
    /// Cache and integrity strategy.
    pub strategy: PackageStoreStrategy,
    /// Directory used for the local store.
    pub directory: CompactString,
}

/// Store strategy used by `@uniflowed/pm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageStoreStrategy {
    /// Address every package by integrity hash.
    ContentAddressed,
}

/// Linking strategy used when applying packages into a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageLinkMode {
    /// Prefer hardlinks and copy only when the filesystem requires it.
    HardlinkThenCopy,
}

/// Workspace package known to the package manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePackage {
    /// Published package name.
    pub name: CompactString,
    /// Path relative to the workspace root.
    pub path: CompactString,
}

/// Deterministic package manager step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackageManagerStep {
    /// Read `uf.config.flow`.
    ReadConfig,
    /// Resolve the package graph.
    ResolveGraph,
    /// Verify tarball, lockfile, and native artifact integrity.
    VerifyIntegrity,
    /// Write `uf.lock`.
    WriteLockfile,
    /// Populate the content-addressed store.
    ApplyStore,
    /// Link packages into the workspace.
    LinkWorkspace,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_plan_is_self_hosted_and_script_free() {
        let plan = PackageManagerPlan::default();

        assert_eq!(plan.resolver, PackageResolver::UfNative);
        assert_eq!(plan.lockfile, "uf.lock");
        assert_eq!(plan.store.strategy, PackageStoreStrategy::ContentAddressed);
        assert!(plan.forbids_npm_scripts());
        assert!(plan.steps.contains(&PackageManagerStep::ResolveGraph));
        assert!(plan.steps.contains(&PackageManagerStep::VerifyIntegrity));
    }

    #[test]
    fn infers_registry_store_and_script_policy_from_config() {
        let config = UniflowedConfig::default();
        let plan = PackageManagerPlan::infer_from_config(&config);

        assert_eq!(plan.registry, "https://registry.npmjs.org");
        assert_eq!(plan.store.directory, ".uf/store");
        assert_eq!(plan.scripts, PackageScriptPolicy::Forbid);
    }

    #[test]
    fn records_workspace_packages_without_npm_scripts() {
        let plan = PackageManagerPlan::default()
            .with_workspace_package("@uniflowed/core", "crates/uf_lib/lib/core");

        assert_eq!(plan.workspace_packages[0].name, "@uniflowed/core");
        assert!(plan.forbids_npm_scripts());
    }
}
