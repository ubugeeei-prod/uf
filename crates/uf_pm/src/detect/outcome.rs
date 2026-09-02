//! What a detection reports back: the evidence, the verdict, the complaints.
//!
//! [`DetectionSource`] records why a manager won and [`DetectionOutcome`] whether
//! the deciding evidence was unique. Detection never fails, so anything
//! suspicious found while reading untrusted repository files travels back as a
//! [`DetectionIssue`] instead of an error.

use camino::Utf8PathBuf;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use super::field::{PackageManagerFieldError, PackageManagerSpec};
use super::manager::{Lockfile, PackageManager, WorkspaceMarker};
use super::{DetectionCandidates, DetectionIssues, LockfileList};

/// Evidence that decided which package manager drives a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DetectionSource {
    /// `pm.packageManager` in `uf.config.js` named the manager explicitly.
    ConfigOverride,
    /// A `"packageManager"` field in `package.json` named the manager.
    PackageManagerField {
        /// Manifest that declared the field.
        manifest: Utf8PathBuf,
        /// Parsed and validated field value.
        spec: PackageManagerSpec,
    },
    /// A lockfile in the starting directory named the manager.
    Lockfile {
        /// Lockfile that decided the manager.
        lockfile: Lockfile,
        /// Path of that lockfile.
        path: Utf8PathBuf,
    },
    /// Evidence was inherited from the nearest ancestor workspace root.
    WorkspaceRoot {
        /// Ancestor directory that owns the workspace.
        root: Utf8PathBuf,
        /// Marker that identified the ancestor as a workspace root.
        marker: WorkspaceMarker,
    },
    /// No evidence at all; uf's native manager is the default.
    Default,
}

impl DetectionSource {
    /// Stable identifier for the source kind, matching the emitted JSON tag.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ConfigOverride => "config-override",
            Self::PackageManagerField { .. } => "package-manager-field",
            Self::Lockfile { .. } => "lockfile",
            Self::WorkspaceRoot { .. } => "workspace-root",
            Self::Default => "default",
        }
    }
}

/// A package manager candidate backed by one piece of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionCandidate {
    /// Manager the evidence points at.
    pub package_manager: PackageManager,
    /// The evidence itself.
    pub source: DetectionSource,
}

/// Whether the winning evidence was unique.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DetectionOutcome {
    /// Exactly one manager was evidenced at the deciding precedence level.
    Unambiguous,
    /// Lockfiles naming different managers sat side by side in one directory.
    ///
    /// [`Detection::manager`] still holds the deterministic tie-break so callers
    /// keep working, but the conflict is reported rather than hidden.
    Ambiguous {
        /// Every conflicting lockfile, in precedence order.
        lockfiles: LockfileList,
    },
}

/// Non-fatal problem found while reading untrusted project files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DetectionIssue {
    /// A `"packageManager"` field was present but rejected.
    InvalidPackageManagerField {
        /// Manifest that declared the field.
        manifest: Utf8PathBuf,
        /// Typed rejection reason.
        error: PackageManagerFieldError,
    },
    /// A manifest was larger than the parse cap and was skipped.
    ManifestTooLarge {
        /// Manifest that was skipped.
        manifest: Utf8PathBuf,
        /// Size of the manifest in bytes.
        bytes: u64,
        /// Accepted limit in bytes.
        limit: u64,
    },
    /// A manifest existed but could not be used.
    ManifestUnusable {
        /// Manifest that was skipped.
        manifest: Utf8PathBuf,
        /// Why the manifest was skipped.
        fault: ManifestFault,
    },
    /// A manifest declared a prototype-pollution key, which was ignored.
    PollutingManifestKey {
        /// Manifest that declared the key.
        manifest: Utf8PathBuf,
        /// The ignored key.
        key: CompactString,
    },
    /// The ancestor walk hit
    /// [`DetectionOptions::max_ancestors`](super::DetectionOptions::max_ancestors) before
    /// finishing.
    AncestorLimitReached {
        /// The limit that was reached.
        limit: usize,
    },
    /// The starting directory was outside the supplied boundary, so nothing was read.
    OutsideBoundary {
        /// Starting directory, lexically normalised.
        path: Utf8PathBuf,
        /// Boundary the walk may never leave.
        boundary: Utf8PathBuf,
    },
}

/// Why a manifest could not be consulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestFault {
    /// The file could not be read.
    Unreadable,
    /// The file was not valid JSON, or was not a JSON object.
    InvalidJson,
    /// The path was not a regular file; uf never follows a symlinked manifest.
    NotARegularFile,
}

/// Result of inferring which package manager drives a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// Directory detection started from, lexically normalised.
    pub root: Utf8PathBuf,
    /// Package manager that drives the project.
    pub package_manager: PackageManager,
    /// Evidence that decided [`Detection::package_manager`].
    pub source: DetectionSource,
    /// Whether the deciding evidence was unique.
    pub outcome: DetectionOutcome,
    /// Every candidate that lost, in precedence order.
    pub alternatives: DetectionCandidates,
    /// Problems found while reading untrusted project files.
    pub issues: DetectionIssues,
}

impl Detection {
    /// Return whether lockfiles naming different managers sat side by side.
    #[must_use]
    pub const fn is_ambiguous(&self) -> bool {
        matches!(self.outcome, DetectionOutcome::Ambiguous { .. })
    }

    /// Return whether uf's own resolver drives the project.
    #[must_use]
    pub const fn is_uf_native(&self) -> bool {
        matches!(self.package_manager, PackageManager::Uf)
    }
}
