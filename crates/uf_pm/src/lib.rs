#![deny(missing_docs)]
//! Native package manager for `uf install`, `uf upgrade`, and `@uniflowed/pm`.
//!
//! The crate has two halves. [`install_workspace`] and [`PackageManagerPlan`]
//! describe uf's own resolver: `uf.lock` plus the content-addressed `.uf/store`.
//! [`detect_package_manager`] and [`command_for`] let uf *interoperate* with a
//! repository that already uses npm, pnpm, yarn, or bun, driving that manager
//! instead of forcing a migration.

pub mod command;
pub mod detect;
pub mod run;

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smallvec::SmallVec;
use thiserror::Error;
use uf_config::UniflowedConfig;

pub use crate::command::{Invocation, InvocationArgs, Operation, PROGRAMS, command_for};
pub use crate::detect::{
    Detection, DetectionCandidate, DetectionCandidates, DetectionIssue, DetectionIssues,
    DetectionOptions, DetectionOutcome, DetectionSource, Lockfile, LockfileList,
    MAX_ANCESTOR_DEPTH, MAX_MANIFEST_BYTES, MAX_PACKAGE_MANAGER_FIELD_BYTES, ManifestFault,
    PackageManager, PackageManagerFieldError, PackageManagerSpec, UnknownPackageManager, Version,
    WorkspaceMarker, YarnEdition, detect_package_manager, detect_package_manager_with,
    parse_package_manager_field, scan_lockfiles, yarn_edition_in,
};
pub use crate::run::{InstallRun, InstallRunError, run_install};

/// JSON object keys that must never be treated as data.
///
/// A manifest that ships `__proto__`, `constructor`, or `prototype` is aiming at
/// a JavaScript consumer's prototype chain (the `lodash.merge` CVE-2019-10744
/// class); uf drops them everywhere it walks a JSON map.
pub const POLLUTING_JSON_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

/// Return whether `key` is a prototype-pollution key that must be ignored.
#[must_use]
pub fn is_polluting_json_key(key: &str) -> bool {
    POLLUTING_JSON_KEYS.contains(&key)
}

/// Inline package list used for small workspaces without heap-heavy metadata.
pub type PackageList = SmallVec<[WorkspacePackage; 8]>;

/// Inline step list for deterministic install and upgrade planning.
pub type PackageManagerSteps = SmallVec<[PackageManagerStep; 8]>;

/// Native package manager plan inferred from `uf.config.js`.
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

/// Applied install report for a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerApplyReport {
    /// Lockfile written by `uf install`.
    pub lockfile: Utf8PathBuf,
    /// Store manifest written by `uf install`.
    pub store_manifest: Utf8PathBuf,
    /// Content-addressed package entries written by `uf install`.
    pub store_entries: Vec<Utf8PathBuf>,
    /// Locked workspace packages.
    pub packages: Vec<LockedPackage>,
}

/// Package entry persisted in `uf.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedPackage {
    /// Published package name.
    pub name: CompactString,
    /// Package version, or `0.0.0` when the manifest intentionally omits one.
    pub version: CompactString,
    /// Path relative to the workspace root.
    pub path: CompactString,
    /// Stable content address for the package manifest.
    pub integrity: CompactString,
    /// Store entry path relative to the package store root.
    pub store_path: CompactString,
    /// Production dependencies from `package.json`.
    pub dependencies: BTreeMap<CompactString, CompactString>,
    /// Development dependencies from `package.json`.
    pub dev_dependencies: BTreeMap<CompactString, CompactString>,
}

/// Native package-manager errors.
#[derive(Debug, Error)]
pub enum PackageManagerError {
    /// A filesystem read failed.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Path that failed to read.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A filesystem write failed.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that failed to write.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A package manifest was invalid JSON.
    #[error("failed to parse {path}: {source}")]
    Parse {
        /// Manifest path.
        path: Utf8PathBuf,
        /// JSON parser error.
        #[source]
        source: serde_json::Error,
    },
    /// A package manifest omitted its name.
    #[error("package manifest {path} is missing a string name")]
    MissingName {
        /// Manifest path.
        path: Utf8PathBuf,
    },
    /// A package manifest declared npm scripts while they are forbidden.
    #[error("package manifest {path} declares scripts; use uf tasks in uf.config.js")]
    ScriptsForbidden {
        /// Manifest path.
        path: Utf8PathBuf,
    },
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

/// Install the workspace deterministically without running npm lifecycle scripts.
pub fn install_workspace(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<PackageManagerApplyReport, PackageManagerError> {
    let plan = PackageManagerPlan::infer_from_config(config);
    let manifests = discover_package_manifests(root)?;
    let mut packages = Vec::with_capacity(manifests.len());

    for manifest in manifests {
        let source = fs::read_to_string(&manifest).map_err(|source| PackageManagerError::Read {
            path: manifest.to_path_buf(),
            source,
        })?;
        let value = serde_json::from_str::<Value>(&source).map_err(|source| {
            PackageManagerError::Parse {
                path: manifest.to_path_buf(),
                source,
            }
        })?;

        if plan.forbids_npm_scripts() && has_scripts(&value) {
            return Err(PackageManagerError::ScriptsForbidden { path: manifest });
        }

        packages.push(lock_manifest(root, &manifest, &source, &value)?);
    }

    packages.sort_by(|a, b| a.path.cmp(&b.path).then(a.name.cmp(&b.name)));

    let store_dir = root.join(plan.store.directory.as_str());
    fs::create_dir_all(&store_dir).map_err(|source| PackageManagerError::Write {
        path: store_dir.clone(),
        source,
    })?;
    let store_package_dir = store_dir.join("packages");
    fs::create_dir_all(&store_package_dir).map_err(|source| PackageManagerError::Write {
        path: store_package_dir.clone(),
        source,
    })?;

    let lockfile = root.join(plan.lockfile.as_str());
    let lock = PackageLockfile {
        lockfile_version: 1,
        resolver: plan.resolver,
        registry: plan.registry,
        scripts: plan.scripts,
        link_mode: plan.link_mode,
        packages: &packages,
    };
    write_json(&lockfile, &lock)?;

    let mut store_entries = Vec::with_capacity(packages.len());
    for package in &packages {
        let store_entry = store_dir.join(package.store_path.as_str());
        write_json(
            &store_entry,
            &PackageStoreEntry {
                version: 1,
                package,
            },
        )?;
        store_entries.push(store_entry);
    }

    let store_manifest = store_dir.join("manifest.json");
    let manifest = StoreManifest {
        version: 1,
        strategy: plan.store.strategy,
        packages: &packages,
    };
    write_json(&store_manifest, &manifest)?;

    Ok(PackageManagerApplyReport {
        lockfile,
        store_manifest,
        store_entries,
        packages,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageLockfile<'a> {
    lockfile_version: u8,
    resolver: PackageResolver,
    registry: CompactString,
    scripts: PackageScriptPolicy,
    link_mode: PackageLinkMode,
    packages: &'a [LockedPackage],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoreManifest<'a> {
    version: u8,
    strategy: PackageStoreStrategy,
    packages: &'a [LockedPackage],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageStoreEntry<'a> {
    version: u8,
    package: &'a LockedPackage,
}

fn discover_package_manifests(root: &Utf8Path) -> Result<Vec<Utf8PathBuf>, PackageManagerError> {
    let mut manifests = Vec::new();
    let submodules = submodule_paths(root);
    visit_package_dirs(root, root, &submodules, &mut manifests)?;
    manifests.sort();
    Ok(manifests)
}

/// The paths `.gitmodules` lists, relative to `root`.
///
/// A submodule is somebody else's repository that happens to be checked out
/// inside this one, and its `package.json` is not one of this project's
/// packages: it must not be locked, and its `scripts` are not this project
/// breaking the no-scripts rule.
///
/// This was already wrong before anything made it visible. `upstream/flow`
/// is Meta's repository and its manifest was being locked as a workspace
/// package; it went unnoticed only because that manifest happens to declare
/// no scripts. Checking out a fixture that does declare some — React Native,
/// Metro — turned it into `uf install` refusing to run at all.
///
/// Parsed by hand rather than with a git library, because it is four lines
/// of `key = value` and a dependency on libgit2 to read them would be the
/// larger cost. An unreadable or absent file means no submodules, which is
/// the right answer for a checkout that has none.
fn submodule_paths(root: &Utf8Path) -> Vec<Utf8PathBuf> {
    let Ok(source) = fs::read_to_string(root.join(".gitmodules")) else {
        return Vec::new();
    };
    source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("path"))
        .filter_map(|rest| rest.trim_start().strip_prefix('='))
        .map(|value| Utf8PathBuf::from(value.trim()))
        .collect()
}

fn visit_package_dirs(
    root: &Utf8Path,
    dir: &Utf8Path,
    submodules: &[Utf8PathBuf],
    manifests: &mut Vec<Utf8PathBuf>,
) -> Result<(), PackageManagerError> {
    let entries = fs::read_dir(dir).map_err(|source| PackageManagerError::Read {
        path: dir.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| PackageManagerError::Read {
            path: dir.to_path_buf(),
            source,
        })?;
        let path =
            Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| PackageManagerError::Read {
                path: Utf8PathBuf::from(path.display().to_string()),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, "path is not UTF-8"),
            })?;
        let file_type = entry
            .file_type()
            .map_err(|source| PackageManagerError::Read {
                path: path.clone(),
                source,
            })?;

        if file_type.is_dir() {
            if should_skip_dir(root, &path, submodules) {
                continue;
            }
            visit_package_dirs(root, &path, submodules, manifests)?;
        } else if file_type.is_file() && path.file_name() == Some("package.json") {
            manifests.push(path);
        }
    }

    Ok(())
}

fn should_skip_dir(root: &Utf8Path, path: &Utf8Path, submodules: &[Utf8PathBuf]) -> bool {
    let name = path.file_name().unwrap_or_default();
    if matches!(
        name,
        ".git" | ".uf" | "dist" | "node_modules" | "target" | "__uf_vrt__"
    ) {
        return true;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    relative.as_str().starts_with("tools/fuzz/target")
        || submodules.iter().any(|submodule| relative == submodule)
}

fn lock_manifest(
    root: &Utf8Path,
    manifest: &Utf8Path,
    source: &str,
    value: &Value,
) -> Result<LockedPackage, PackageManagerError> {
    let name = value.get("name").and_then(Value::as_str).ok_or_else(|| {
        PackageManagerError::MissingName {
            path: manifest.to_path_buf(),
        }
    })?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("0.0.0");
    let package_dir = manifest.parent().unwrap_or(root);
    let relative = package_dir
        .strip_prefix(root)
        .map(Utf8Path::as_str)
        .unwrap_or(package_dir.as_str());
    let path = if relative.is_empty() { "." } else { relative };
    let integrity = stable_manifest_integrity(source.as_bytes());
    let store_path = format!("packages/{integrity}.json");

    Ok(LockedPackage {
        name: name.to_compact_string(),
        version: version.to_compact_string(),
        path: path.to_compact_string(),
        integrity: integrity.to_compact_string(),
        store_path: store_path.to_compact_string(),
        dependencies: read_dependency_map(value, "dependencies"),
        dev_dependencies: read_dependency_map(value, "devDependencies"),
    })
}

fn stable_manifest_integrity(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }

    format!("uf-fnv1a64-{hash:016x}")
}

fn read_dependency_map(value: &Value, field: &str) -> BTreeMap<CompactString, CompactString> {
    let mut dependencies = BTreeMap::new();
    let Some(object) = value.get(field).and_then(Value::as_object) else {
        return dependencies;
    };

    for (name, version) in object {
        // A dependency map is untrusted manifest content; prototype-pollution keys
        // never become dependency names.
        if is_polluting_json_key(name) {
            continue;
        }
        if let Some(version) = version.as_str() {
            dependencies.insert(
                name.as_str().to_compact_string(),
                version.to_compact_string(),
            );
        }
    }

    dependencies
}

fn has_scripts(value: &Value) -> bool {
    value
        .get("scripts")
        .and_then(Value::as_object)
        .is_some_and(|scripts| !scripts.is_empty())
}

fn write_json<T: Serialize>(path: &Utf8Path, value: &T) -> Result<(), PackageManagerError> {
    let mut contents =
        serde_json::to_string_pretty(value).map_err(|source| PackageManagerError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    contents.push('\n');

    fs::write(path, contents).map_err(|source| PackageManagerError::Write {
        path: path.to_path_buf(),
        source,
    })
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
    /// Read `uf.config.js`.
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
mod tests;
