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

use std::fmt;
use std::fs;
use std::io::Read;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smallvec::SmallVec;
use thiserror::Error;
use uf_config::{PackageManagerPreference, UniflowedConfig};

use crate::is_polluting_json_key;

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

/// Bytes read from `yarn.lock` when classifying the Yarn edition.
const YARN_LOCK_PROBE_BYTES: usize = 8 * 1024;

/// Inline candidate list; real projects carry at most a handful of lockfiles.
pub type DetectionCandidates = SmallVec<[DetectionCandidate; 4]>;

/// Inline diagnostic list emitted while reading untrusted project files.
pub type DetectionIssues = SmallVec<[DetectionIssue; 2]>;

/// Inline lockfile list for a single directory.
pub type LockfileList = SmallVec<[Lockfile; 4]>;

/// Package manager that can drive a JavaScript project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "CompactString", try_from = "CompactString")]
pub enum PackageManager {
    /// uf's own resolver, backed by `uf.lock` and the content-addressed `.uf/store`.
    Uf,
    /// npm.
    Npm,
    /// pnpm.
    Pnpm,
    /// Yarn, in the edition the project pins.
    Yarn(YarnEdition),
    /// Bun.
    Bun,
}

/// Yarn major-line, which changes both the lockfile format and the CLI flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum YarnEdition {
    /// Yarn 1.x, "classic".
    Classic,
    /// Yarn 2 and newer, "berry".
    Berry,
}

impl PackageManager {
    /// Every package manager uf can drive, in detection precedence order.
    pub const ALL: [Self; 6] = [
        Self::Uf,
        Self::Bun,
        Self::Pnpm,
        Self::Yarn(YarnEdition::Berry),
        Self::Yarn(YarnEdition::Classic),
        Self::Npm,
    ];

    /// Stable identifier used in JSON output, CLI text, and config values.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uf => "uf",
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn(YarnEdition::Classic) => "yarn-classic",
            Self::Yarn(YarnEdition::Berry) => "yarn-berry",
            Self::Bun => "bun",
        }
    }

    /// Parse a manager identifier.
    ///
    /// Bare `yarn` selects [`YarnEdition::Berry`], matching modern Yarn releases;
    /// `yarn-classic` pins Yarn 1.x.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "uf" => Some(Self::Uf),
            "npm" => Some(Self::Npm),
            "pnpm" => Some(Self::Pnpm),
            "yarn" | "yarn-berry" => Some(Self::Yarn(YarnEdition::Berry)),
            "yarn-classic" => Some(Self::Yarn(YarnEdition::Classic)),
            "bun" => Some(Self::Bun),
            _ => None,
        }
    }

    /// Map a `uf.config.js` preference onto a manager, or `None` for `auto`.
    #[must_use]
    pub const fn from_preference(preference: PackageManagerPreference) -> Option<Self> {
        match preference {
            PackageManagerPreference::Auto => None,
            PackageManagerPreference::Uf => Some(Self::Uf),
            PackageManagerPreference::Npm => Some(Self::Npm),
            PackageManagerPreference::Pnpm => Some(Self::Pnpm),
            PackageManagerPreference::Yarn | PackageManagerPreference::YarnBerry => {
                Some(Self::Yarn(YarnEdition::Berry))
            }
            PackageManagerPreference::YarnClassic => Some(Self::Yarn(YarnEdition::Classic)),
            PackageManagerPreference::Bun => Some(Self::Bun),
        }
    }

    /// Lockfile this manager writes.
    #[must_use]
    pub const fn lockfile(self) -> Lockfile {
        match self {
            Self::Uf => Lockfile::UfLock,
            Self::Npm => Lockfile::PackageLock,
            Self::Pnpm => Lockfile::PnpmLock,
            Self::Yarn(_) => Lockfile::YarnLock,
            Self::Bun => Lockfile::BunLock,
        }
    }
}

impl fmt::Display for PackageManager {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<PackageManager> for CompactString {
    fn from(manager: PackageManager) -> Self {
        Self::const_new(manager.as_str())
    }
}

/// Rejection returned when a string does not name a package manager uf can drive.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("`{name}` is not one of uf, npm, pnpm, yarn, yarn-classic, yarn-berry, bun")]
pub struct UnknownPackageManager {
    /// The rejected identifier.
    pub name: CompactString,
}

impl TryFrom<CompactString> for PackageManager {
    type Error = UnknownPackageManager;

    fn try_from(value: CompactString) -> Result<Self, Self::Error> {
        Self::parse(value.as_str()).ok_or(UnknownPackageManager { name: value })
    }
}

/// Lockfile uf recognises, in detection precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lockfile {
    /// `uf.lock`.
    UfLock,
    /// `bun.lock`, Bun's textual lockfile.
    BunLock,
    /// `bun.lockb`, Bun's binary lockfile.
    BunLockb,
    /// `pnpm-lock.yaml`.
    PnpmLock,
    /// `yarn.lock`, written by both Yarn editions.
    YarnLock,
    /// `package-lock.json`.
    PackageLock,
    /// `npm-shrinkwrap.json`.
    NpmShrinkwrap,
}

impl Lockfile {
    /// Every recognised lockfile, in precedence order.
    pub const ALL: [Self; 7] = [
        Self::UfLock,
        Self::BunLock,
        Self::BunLockb,
        Self::PnpmLock,
        Self::YarnLock,
        Self::PackageLock,
        Self::NpmShrinkwrap,
    ];

    /// File name on disk.
    #[must_use]
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::UfLock => "uf.lock",
            Self::BunLock => "bun.lock",
            Self::BunLockb => "bun.lockb",
            Self::PnpmLock => "pnpm-lock.yaml",
            Self::YarnLock => "yarn.lock",
            Self::PackageLock => "package-lock.json",
            Self::NpmShrinkwrap => "npm-shrinkwrap.json",
        }
    }

    /// Package manager that writes this lockfile.
    ///
    /// `yarn.lock` reports [`YarnEdition::Classic`] because the edition is only
    /// known after probing the lockfile header and `.yarnrc.yml`; detection
    /// refines it through [`yarn_edition_in`].
    #[must_use]
    pub const fn manager(self) -> PackageManager {
        match self {
            Self::UfLock => PackageManager::Uf,
            Self::BunLock | Self::BunLockb => PackageManager::Bun,
            Self::PnpmLock => PackageManager::Pnpm,
            Self::YarnLock => PackageManager::Yarn(YarnEdition::Classic),
            Self::PackageLock | Self::NpmShrinkwrap => PackageManager::Npm,
        }
    }
}

impl fmt::Display for Lockfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.file_name())
    }
}

/// Marker that identifies a directory as a workspace root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceMarker {
    /// A lockfile lived in the ancestor directory.
    Lockfile(Lockfile),
    /// The ancestor directory held `pnpm-workspace.yaml`.
    PnpmWorkspaceYaml,
    /// The ancestor's `package.json` declared a `"workspaces"` field.
    PackageJsonWorkspaces,
}

/// Semantic version parsed out of a `"packageManager"` field.
///
/// Deliberately not ordered: comparing prerelease segments correctly is a semver
/// concern uf does not need here, and a derived ordering would be wrong.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    /// Major component.
    pub major: u32,
    /// Minor component.
    pub minor: u32,
    /// Patch component.
    pub patch: u32,
    /// Prerelease segment without its leading `-`, when present.
    pub prerelease: Option<CompactString>,
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(prerelease) = &self.prerelease {
            write!(formatter, "-{prerelease}")?;
        }
        Ok(())
    }
}

/// Validated `"packageManager"` field, in corepack's `name@version[+integrity]` shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerSpec {
    /// Manager named by the field.
    pub manager: PackageManager,
    /// Version pinned by the field.
    pub version: Version,
    /// Corepack integrity suffix without its leading `+`, when present.
    pub integrity: Option<CompactString>,
}

/// Typed rejection for an invalid `"packageManager"` manifest field.
///
/// The field is attacker-controlled, so every rejection is structured: nothing is
/// echoed into a shell and nothing unvalidated ever reaches an [`crate::Invocation`].
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum PackageManagerFieldError {
    /// The field was present but empty.
    #[error("packageManager field is empty")]
    Empty,
    /// The field was longer than [`MAX_PACKAGE_MANAGER_FIELD_BYTES`].
    #[error("packageManager field is {length} bytes; the limit is {limit}")]
    TooLong {
        /// Length of the rejected field.
        length: usize,
        /// Accepted limit.
        limit: usize,
    },
    /// The field carried no `@` separating the name from the version.
    #[error("packageManager field is missing the `name@version` separator")]
    MissingSeparator,
    /// The name was not one of `npm`, `pnpm`, `yarn`, `bun`.
    #[error("packageManager name `{name}` is not one of npm, pnpm, yarn, bun")]
    UnknownManager {
        /// Rejected manager name.
        name: CompactString,
    },
    /// The version was not `major.minor.patch`.
    #[error("packageManager version `{version}` is not a major.minor.patch version")]
    MalformedVersion {
        /// Rejected version text.
        version: CompactString,
    },
    /// A numeric version component did not fit in a `u32`.
    #[error("packageManager version component `{component}` overflows a 32-bit integer")]
    VersionOverflow {
        /// Rejected component text.
        component: CompactString,
    },
    /// A byte outside the accepted alphabet appeared in the field.
    #[error("packageManager field has the forbidden character `{character}` at byte {offset}")]
    ForbiddenCharacter {
        /// The offending character.
        character: char,
        /// Byte offset of the character within the field.
        offset: usize,
    },
}

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
    /// The ancestor walk hit [`DetectionOptions::max_ancestors`] before finishing.
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

/// Parse and validate a corepack-style `"packageManager"` field.
///
/// The accepted shape is
/// `^(npm|pnpm|yarn|bun)@\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$`,
/// implemented as a single forward pass with no backtracking. A regex engine here
/// would be a ReDoS foothold on attacker-supplied manifest text (the CVE class of
/// `ansi-regex` CVE-2021-3807 and `semver` CVE-2022-25883), so the grammar is
/// hand-written and every byte is visited at most once.
///
/// # Errors
///
/// Returns [`PackageManagerFieldError`] describing exactly why the field was
/// refused. `uf` is intentionally not accepted: the field is corepack's, and uf
/// projects pin their manager through `uf.config.js`.
pub fn parse_package_manager_field(
    value: &str,
) -> Result<PackageManagerSpec, PackageManagerFieldError> {
    if value.is_empty() {
        return Err(PackageManagerFieldError::Empty);
    }
    if value.len() > MAX_PACKAGE_MANAGER_FIELD_BYTES {
        return Err(PackageManagerFieldError::TooLong {
            length: value.len(),
            limit: MAX_PACKAGE_MANAGER_FIELD_BYTES,
        });
    }

    let bytes = value.as_bytes();
    let Some(separator) = bytes.iter().position(|byte| *byte == b'@') else {
        return Err(PackageManagerFieldError::MissingSeparator);
    };

    let name = &value[..separator];
    let manager = match name {
        "npm" => PackageManager::Npm,
        "pnpm" => PackageManager::Pnpm,
        "bun" => PackageManager::Bun,
        // The edition is decided by the pinned major once the version parses.
        "yarn" => PackageManager::Yarn(YarnEdition::Classic),
        _ => {
            return Err(PackageManagerFieldError::UnknownManager {
                name: name.to_compact_string(),
            });
        }
    };

    let mut cursor = separator + 1;
    let major = take_number(value, &mut cursor)?;
    expect_dot(value, &mut cursor)?;
    let minor = take_number(value, &mut cursor)?;
    expect_dot(value, &mut cursor)?;
    let patch = take_number(value, &mut cursor)?;

    let prerelease = if bytes.get(cursor) == Some(&b'-') {
        cursor += 1;
        Some(take_tagged_segment(value, &mut cursor)?)
    } else {
        None
    };

    let integrity = if bytes.get(cursor) == Some(&b'+') {
        cursor += 1;
        Some(take_tagged_segment(value, &mut cursor)?)
    } else {
        None
    };

    if cursor != bytes.len() {
        return Err(forbidden_character(value, cursor));
    }

    let manager = match manager {
        PackageManager::Yarn(_) if major >= 2 => PackageManager::Yarn(YarnEdition::Berry),
        other => other,
    };

    Ok(PackageManagerSpec {
        manager,
        version: Version {
            major,
            minor,
            patch,
            prerelease,
        },
        integrity,
    })
}

/// Classify the Yarn edition used by `dir`.
///
/// Berry writes a `__metadata:` header into `yarn.lock` and is the only edition
/// that reads `.yarnrc.yml`; anything else is treated as Yarn 1.x.
#[must_use]
pub fn yarn_edition_in(dir: &Utf8Path) -> YarnEdition {
    if is_regular_file(&dir.join(".yarnrc.yml")) {
        return YarnEdition::Berry;
    }

    let head = read_file_head(&dir.join("yarn.lock"), YARN_LOCK_PROBE_BYTES);
    let head = String::from_utf8_lossy(&head);
    if head.contains("__metadata:") {
        YarnEdition::Berry
    } else {
        YarnEdition::Classic
    }
}

/// Every lockfile present directly in `dir`, in precedence order.
#[must_use]
pub fn scan_lockfiles(dir: &Utf8Path) -> LockfileList {
    Lockfile::ALL
        .into_iter()
        .filter(|lockfile| is_regular_file(&dir.join(lockfile.file_name())))
        .collect()
}

#[derive(Debug)]
struct WorkspaceRootFind {
    root: Utf8PathBuf,
    marker: WorkspaceMarker,
    manager: Option<PackageManager>,
    lockfiles: LockfileList,
    managers: SmallVec<[PackageManager; 4]>,
}

fn find_workspace_root(
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

fn managers_for(dir: &Utf8Path, lockfiles: &LockfileList) -> SmallVec<[PackageManager; 4]> {
    let yarn = lockfiles
        .contains(&Lockfile::YarnLock)
        .then(|| yarn_edition_in(dir));

    lockfiles
        .iter()
        .map(|lockfile| match lockfile {
            Lockfile::YarnLock => PackageManager::Yarn(yarn.unwrap_or(YarnEdition::Classic)),
            other => other.manager(),
        })
        .collect()
}

fn ambiguity(lockfiles: &LockfileList, managers: &[PackageManager]) -> DetectionOutcome {
    let distinct = managers.first().is_some_and(|first| {
        managers
            .iter()
            .any(|manager| !same_family(*manager, *first))
    });

    if distinct {
        DetectionOutcome::Ambiguous {
            lockfiles: lockfiles.clone(),
        }
    } else {
        DetectionOutcome::Unambiguous
    }
}

/// `bun.lock` next to `bun.lockb` is one manager, not a conflict; so is a Yarn
/// lockfile whose edition probe is the only thing that differs.
fn same_family(left: PackageManager, right: PackageManager) -> bool {
    matches!(
        (left, right),
        (PackageManager::Yarn(_), PackageManager::Yarn(_))
    ) || left == right
}

fn read_manifest(
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

fn manifest_package_manager(
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

fn is_regular_file(path: &Utf8Path) -> bool {
    // Never follow a symlink: a lockfile symlinked out of the tree must not vote.
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn read_file_head(path: &Utf8Path, limit: usize) -> Vec<u8> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Vec::new();
    };
    if !metadata.is_file() {
        return Vec::new();
    }
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };

    let mut head = Vec::with_capacity(limit.min(metadata.len() as usize + 1));
    if file.take(limit as u64).read_to_end(&mut head).is_err() {
        return Vec::new();
    }
    head
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

fn take_number(value: &str, cursor: &mut usize) -> Result<u32, PackageManagerFieldError> {
    let bytes = value.as_bytes();
    let start = *cursor;
    while bytes.get(*cursor).is_some_and(u8::is_ascii_digit) {
        *cursor += 1;
    }

    if start == *cursor {
        return Err(PackageManagerFieldError::MalformedVersion {
            version: truncated(&value[start.min(value.len())..]),
        });
    }

    value[start..*cursor]
        .parse::<u32>()
        .map_err(|_| PackageManagerFieldError::VersionOverflow {
            component: truncated(&value[start..*cursor]),
        })
}

fn expect_dot(value: &str, cursor: &mut usize) -> Result<(), PackageManagerFieldError> {
    if value.as_bytes().get(*cursor) == Some(&b'.') {
        *cursor += 1;
        return Ok(());
    }
    Err(PackageManagerFieldError::MalformedVersion {
        version: truncated(&value[(*cursor).min(value.len())..]),
    })
}

/// Consume a `[0-9A-Za-z.-]+` run, stopping before `+` or at the end of input.
fn take_tagged_segment(
    value: &str,
    cursor: &mut usize,
) -> Result<CompactString, PackageManagerFieldError> {
    let bytes = value.as_bytes();
    let start = *cursor;

    while let Some(byte) = bytes.get(*cursor) {
        if byte.is_ascii_alphanumeric() || *byte == b'.' || *byte == b'-' {
            *cursor += 1;
        } else {
            break;
        }
    }

    if start == *cursor {
        return Err(forbidden_character(value, start));
    }
    Ok(value[start..*cursor].to_compact_string())
}

fn forbidden_character(value: &str, offset: usize) -> PackageManagerFieldError {
    let character = value
        .get(offset..)
        .and_then(|rest| rest.chars().next())
        .unwrap_or(char::REPLACEMENT_CHARACTER);
    PackageManagerFieldError::ForbiddenCharacter { character, offset }
}

/// Keep untrusted text out of error messages beyond a fixed budget.
fn truncated(value: &str) -> CompactString {
    const BUDGET: usize = 32;

    let end = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= BUDGET)
        .last()
        .unwrap_or(0);
    value[..end].to_compact_string()
}

#[cfg(test)]
mod tests;
