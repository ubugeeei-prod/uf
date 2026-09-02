//! The bounded walk up to the nearest workspace root, and the manifests it reads.
//!
//! Every step of this walk touches attacker-controlled repository content, so it
//! is depth-limited, stops at a `.git` directory, never leaves the configured
//! boundary, and refuses a `package.json` that is oversized, not a regular file,
//! or carrying prototype-pollution keys.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::ToCompactString;
use serde_json::Value;
use smallvec::SmallVec;

use crate::is_polluting_json_key;

use super::field::{PackageManagerSpec, parse_package_manager_field};
use super::lockfile::{is_regular_file, managers_for, scan_lockfiles};
use super::manager::{PackageManager, WorkspaceMarker};
use super::outcome::{DetectionIssue, ManifestFault};
use super::{DetectionIssues, DetectionOptions, LockfileList, lexically_normalized};

#[derive(Debug)]
pub(crate) struct WorkspaceRootFind {
    pub(crate) root: Utf8PathBuf,
    pub(crate) marker: WorkspaceMarker,
    pub(crate) manager: Option<PackageManager>,
    pub(crate) lockfiles: LockfileList,
    pub(crate) managers: SmallVec<[PackageManager; 4]>,
}

pub(crate) fn find_workspace_root(
    start: &Utf8Path,
    options: &DetectionOptions<'_>,
    issues: &mut DetectionIssues,
) -> Option<WorkspaceRootFind> {
    let mut current = start.parent()?.to_path_buf();
    let mut visited = 0usize;

    loop {
        if visited >= options.max_ancestors {
            issues.push(DetectionIssue::AncestorLimitReached {
                limit: options.max_ancestors,
            });
            return None;
        }
        visited += 1;

        if let Some(boundary) = options.boundary
            && !current.starts_with(lexically_normalized(boundary))
        {
            return None;
        }

        let lockfiles = scan_lockfiles(&current);
        if let Some(lockfile) = lockfiles.first() {
            let managers = managers_for(&current, &lockfiles);
            let manager = managers.first().copied();
            return Some(WorkspaceRootFind {
                root: current,
                marker: WorkspaceMarker::Lockfile(*lockfile),
                manager,
                lockfiles,
                managers,
            });
        }

        if is_regular_file(&current.join("pnpm-workspace.yaml")) {
            return Some(WorkspaceRootFind {
                root: current,
                marker: WorkspaceMarker::PnpmWorkspaceYaml,
                manager: Some(PackageManager::Pnpm),
                lockfiles,
                managers: SmallVec::new(),
            });
        }

        let manifest_path = current.join("package.json");
        if let Some(manifest) = read_manifest(&manifest_path, options, issues)
            && manifest_has_workspaces(&manifest_path, &manifest, issues)
        {
            let manager = manifest_package_manager(&manifest_path, &manifest, issues)
                .map(|spec| spec.manager);
            return Some(WorkspaceRootFind {
                root: current,
                marker: WorkspaceMarker::PackageJsonWorkspaces,
                manager,
                lockfiles,
                managers: SmallVec::new(),
            });
        }

        // A `.git` directory ends the repository; a workspace never spans past it.
        if current.join(".git").exists() {
            return None;
        }

        current = current.parent()?.to_path_buf();
    }
}

pub(crate) fn read_manifest(
    path: &Utf8Path,
    options: &DetectionOptions<'_>,
    issues: &mut DetectionIssues,
) -> Option<Value> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            issues.push(DetectionIssue::ManifestUnusable {
                manifest: path.to_path_buf(),
                fault: ManifestFault::Unreadable,
            });
            return None;
        }
    };

    // `symlink_metadata` does not follow the final component, so a manifest that
    // points outside the tree is refused rather than read (symlink-escape guard).
    if !metadata.is_file() {
        issues.push(DetectionIssue::ManifestUnusable {
            manifest: path.to_path_buf(),
            fault: ManifestFault::NotARegularFile,
        });
        return None;
    }

    if metadata.len() > options.max_manifest_bytes {
        issues.push(DetectionIssue::ManifestTooLarge {
            manifest: path.to_path_buf(),
            bytes: metadata.len(),
            limit: options.max_manifest_bytes,
        });
        return None;
    }

    let Ok(source) = fs::read_to_string(path) else {
        issues.push(DetectionIssue::ManifestUnusable {
            manifest: path.to_path_buf(),
            fault: ManifestFault::Unreadable,
        });
        return None;
    };

    match serde_json::from_str::<Value>(&source) {
        Ok(value) if value.is_object() => Some(value),
        _ => {
            issues.push(DetectionIssue::ManifestUnusable {
                manifest: path.to_path_buf(),
                fault: ManifestFault::InvalidJson,
            });
            None
        }
    }
}

pub(crate) fn manifest_package_manager(
    path: &Utf8Path,
    manifest: &Value,
    issues: &mut DetectionIssues,
) -> Option<PackageManagerSpec> {
    report_polluting_keys(path, manifest, issues);

    let raw = manifest.get("packageManager")?.as_str()?;
    match parse_package_manager_field(raw) {
        Ok(spec) => Some(spec),
        Err(error) => {
            issues.push(DetectionIssue::InvalidPackageManagerField {
                manifest: path.to_path_buf(),
                error,
            });
            None
        }
    }
}

fn manifest_has_workspaces(
    path: &Utf8Path,
    manifest: &Value,
    issues: &mut DetectionIssues,
) -> bool {
    report_polluting_keys(path, manifest, issues);

    match manifest.get("workspaces") {
        Some(Value::Array(globs)) => !globs.is_empty(),
        // Yarn 1 also accepts `{ "packages": [...], "nohoist": [...] }`.
        Some(Value::Object(object)) => object
            .iter()
            .filter(|(key, _)| !is_polluting_json_key(key))
            .any(|(key, value)| {
                key == "packages" && value.as_array().is_some_and(|globs| !globs.is_empty())
            }),
        _ => false,
    }
}

/// Record and ignore prototype-pollution keys declared at the manifest root.
///
/// A manifest shipping `__proto__`, `constructor`, or `prototype` is aiming at a
/// JavaScript consumer (the `lodash.merge` CVE-2019-10744 class); uf never treats
/// those keys as data.
fn report_polluting_keys(path: &Utf8Path, manifest: &Value, issues: &mut DetectionIssues) {
    let Some(object) = manifest.as_object() else {
        return;
    };

    for (key, _) in object.iter().filter(|(key, _)| is_polluting_json_key(key)) {
        let issue = DetectionIssue::PollutingManifestKey {
            manifest: path.to_path_buf(),
            key: key.as_str().to_compact_string(),
        };
        if !issues.contains(&issue) {
            issues.push(issue);
        }
    }
}
