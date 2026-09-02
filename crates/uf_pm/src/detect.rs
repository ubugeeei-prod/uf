//! Package manager auto-inference for repositories that already use npm, pnpm, yarn, or bun.
//!
//! `uf` ships its own resolver ([`crate::PackageResolver::UfNative`]), but it must
//! *interoperate* with existing projects instead of forcing a migration.
//! [`detect_package_manager`] inspects a directory and reports which manager drives
//! it, the evidence that decided it, and every competing candidate so the CLI can
//! warn instead of silently guessing.
//!
//! # Precedence
//!
//! Highest wins; every loser is kept in [`Detection::alternatives`].
//!
//! | Rank | [`DetectionSource`] | Evidence |
//! | ---- | ------------------- | -------- |
//! | 1 | [`DetectionSource::ConfigOverride`] | `pm.packageManager` in `uf.config.js` |
//! | 2 | [`DetectionSource::PackageManagerField`] | `"packageManager"` in `<root>/package.json` |
//! | 3 | [`DetectionSource::Lockfile`] | a lockfile in `<root>` itself |
//! | 4 | [`DetectionSource::WorkspaceRoot`] | evidence in the nearest ancestor workspace root |
//! | 5 | [`DetectionSource::Default`] | [`PackageManager::Uf`] |
//!
//! Lockfiles are ranked against each other by [`Lockfile`] declaration order
//! (`uf.lock` > `bun.lock` > `bun.lockb` > `pnpm-lock.yaml` > `yarn.lock` >
//! `package-lock.json` > `npm-shrinkwrap.json`). When lockfiles naming *different*
//! managers sit side by side the tie-break is still deterministic, but
//! [`Detection::outcome`] reports [`DetectionOutcome::Ambiguous`] with every
//! conflicting lockfile so the choice is never silent.
//!
//! # Security
//!
//! Everything read here is attacker-controlled repository content:
//!
//! * The `"packageManager"` field is validated by a hand-written single-pass
//!   scanner rather than a backtracking regex, because regex engines over manifest
//!   text are a well-known ReDoS CVE class.
//! * Rejected fields never reach a process: [`crate::Invocation::program`] is a
//!   `&'static str` from a fixed table, so a hostile manifest cannot inject a
//!   program name or an argument.
//! * `__proto__`, `constructor`, and `prototype` keys are ignored wherever a JSON
//!   map is walked.
//! * Manifests larger than [`DetectionOptions::max_manifest_bytes`] are refused
//!   instead of being buffered.
//! * The ancestor walk is depth-bounded, stops at a `.git` directory, never
//!   follows a symlink, and never leaves [`DetectionOptions::boundary`].

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use smallvec::SmallVec;
use uf_config::UniflowedConfig;

mod field;
mod lockfile;
mod manager;
mod outcome;
mod workspace;

pub use field::{
    PackageManagerFieldError, PackageManagerSpec, Version, parse_package_manager_field,
};
pub use lockfile::{scan_lockfiles, yarn_edition_in};
pub use manager::{Lockfile, PackageManager, UnknownPackageManager, WorkspaceMarker, YarnEdition};
pub use outcome::{
    Detection, DetectionCandidate, DetectionIssue, DetectionOutcome, DetectionSource, ManifestFault,
};

use lockfile::{ambiguity, managers_for};
use workspace::{find_workspace_root, manifest_package_manager, read_manifest};

/// Largest `package.json` uf will parse while detecting a package manager.
///
/// Manifests above this bound are reported as [`DetectionIssue::ManifestTooLarge`]
/// instead of being buffered, so a hostile repository cannot force an unbounded
/// allocation.
pub const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;

/// Largest number of ancestor directories inspected while looking for a workspace root.
pub const MAX_ANCESTOR_DEPTH: usize = 64;

/// Largest `"packageManager"` field uf will parse.
pub const MAX_PACKAGE_MANAGER_FIELD_BYTES: usize = 128;

/// Inline candidate list; real projects carry at most a handful of lockfiles.
pub type DetectionCandidates = SmallVec<[DetectionCandidate; 4]>;

/// Inline diagnostic list emitted while reading untrusted project files.
pub type DetectionIssues = SmallVec<[DetectionIssue; 2]>;

/// Inline lockfile list for a single directory.
pub type LockfileList = SmallVec<[Lockfile; 4]>;

/// Knobs for [`detect_package_manager_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectionOptions<'a> {
    /// Explicit `pm.packageManager` override from `uf.config.js`.
    pub config_override: Option<PackageManager>,
    /// Directory the walk may never leave; no path outside it is ever read.
    pub boundary: Option<&'a Utf8Path>,
    /// Maximum number of ancestor directories inspected.
    pub max_ancestors: usize,
    /// Maximum manifest size parsed, in bytes.
    pub max_manifest_bytes: u64,
}

impl Default for DetectionOptions<'_> {
    fn default() -> Self {
        Self {
            config_override: None,
            boundary: None,
            max_ancestors: MAX_ANCESTOR_DEPTH,
            max_manifest_bytes: MAX_MANIFEST_BYTES,
        }
    }
}

impl<'a> DetectionOptions<'a> {
    /// Build options with uf's defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build options from `uf.config.js`, honouring `pm.packageManager`.
    #[must_use]
    pub fn from_config(config: &UniflowedConfig) -> DetectionOptions<'static> {
        DetectionOptions {
            config_override: PackageManager::from_preference(config.pm.package_manager),
            ..DetectionOptions::default()
        }
    }

    /// Refuse to read anything outside `boundary`.
    #[must_use]
    pub fn with_boundary(mut self, boundary: &'a Utf8Path) -> Self {
        self.boundary = Some(boundary);
        self
    }

    /// Pin an explicit manager, as `pm.packageManager` does.
    #[must_use]
    pub fn with_config_override(mut self, manager: PackageManager) -> Self {
        self.config_override = Some(manager);
        self
    }

    /// Bound the ancestor walk.
    #[must_use]
    pub fn with_max_ancestors(mut self, max_ancestors: usize) -> Self {
        self.max_ancestors = max_ancestors;
        self
    }

    /// Bound the manifest size that will be parsed.
    #[must_use]
    pub fn with_max_manifest_bytes(mut self, max_manifest_bytes: u64) -> Self {
        self.max_manifest_bytes = max_manifest_bytes;
        self
    }
}

/// Infer which package manager drives the project rooted at `root`.
///
/// Detection never fails: with no evidence it falls back to
/// [`PackageManager::Uf`], recording anything suspicious in
/// [`Detection::issues`]. See the [module docs](self) for the precedence table.
#[must_use]
pub fn detect_package_manager(root: &Utf8Path) -> Detection {
    detect_package_manager_with(root, &DetectionOptions::default())
}

/// Infer which package manager drives `root` with explicit options.
#[must_use]
pub fn detect_package_manager_with(root: &Utf8Path, options: &DetectionOptions<'_>) -> Detection {
    let start = lexically_normalized(root);
    let mut issues = DetectionIssues::new();

    if let Some(boundary) = options.boundary {
        let boundary = lexically_normalized(boundary);
        if !start.starts_with(&boundary) {
            issues.push(DetectionIssue::OutsideBoundary {
                path: start.clone(),
                boundary,
            });
            return Detection {
                root: start,
                package_manager: PackageManager::Uf,
                source: DetectionSource::Default,
                outcome: DetectionOutcome::Unambiguous,
                alternatives: DetectionCandidates::new(),
                issues,
            };
        }
    }

    let mut candidates = DetectionCandidates::new();

    if let Some(manager) = options.config_override {
        candidates.push(DetectionCandidate {
            package_manager: manager,
            source: DetectionSource::ConfigOverride,
        });
    }

    let manifest_path = start.join("package.json");
    let manifest = read_manifest(&manifest_path, options, &mut issues);
    if let Some(spec) = manifest
        .as_ref()
        .and_then(|value| manifest_package_manager(&manifest_path, value, &mut issues))
    {
        candidates.push(DetectionCandidate {
            package_manager: spec.manager,
            source: DetectionSource::PackageManagerField {
                manifest: manifest_path,
                spec,
            },
        });
    }

    let local_lockfiles = scan_lockfiles(&start);
    let local_managers = managers_for(&start, &local_lockfiles);
    for (lockfile, manager) in local_lockfiles.iter().zip(local_managers.iter()) {
        candidates.push(DetectionCandidate {
            package_manager: *manager,
            source: DetectionSource::Lockfile {
                lockfile: *lockfile,
                path: start.join(lockfile.file_name()),
            },
        });
    }

    let ancestor = find_workspace_root(&start, options, &mut issues);
    if let Some(found) = &ancestor
        && let Some(manager) = found.manager
    {
        candidates.push(DetectionCandidate {
            package_manager: manager,
            source: DetectionSource::WorkspaceRoot {
                root: found.root.clone(),
                marker: found.marker,
            },
        });
    }

    let mut candidates = candidates.into_iter();
    let winner = candidates.next().unwrap_or(DetectionCandidate {
        package_manager: PackageManager::Uf,
        source: DetectionSource::Default,
    });
    let alternatives = candidates.collect::<DetectionCandidates>();

    let outcome = match &winner.source {
        DetectionSource::Lockfile { .. } => ambiguity(&local_lockfiles, &local_managers),
        DetectionSource::WorkspaceRoot {
            marker: WorkspaceMarker::Lockfile(_),
            ..
        } => ancestor
            .as_ref()
            .map_or(DetectionOutcome::Unambiguous, |found| {
                ambiguity(&found.lockfiles, &found.managers)
            }),
        _ => DetectionOutcome::Unambiguous,
    };

    Detection {
        root: start,
        package_manager: winner.package_manager,
        source: winner.source,
        outcome,
        alternatives,
        issues,
    }
}

/// Resolve `.` and `..` without touching the filesystem.
///
/// Purely lexical on purpose: resolving through symlinks would let a crafted
/// checkout point the walk at a directory outside the boundary.
fn lexically_normalized(path: &Utf8Path) -> Utf8PathBuf {
    let mut normalized = Utf8PathBuf::new();

    for component in path.components() {
        match component {
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Utf8Component::Normal(_))
                ) {
                    normalized.pop();
                } else if normalized.as_str().is_empty() {
                    normalized.push("..");
                }
                // At a root, `..` is the root itself, matching POSIX resolution.
            }
            other => normalized.push(other.as_str()),
        }
    }

    normalized
}

#[cfg(test)]
mod tests;
