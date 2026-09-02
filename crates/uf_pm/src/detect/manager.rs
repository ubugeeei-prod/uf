//! The managers uf can drive, and the on-disk evidence that names one.
//!
//! [`PackageManager`] is the vocabulary the rest of detection speaks, while
//! [`Lockfile`] and [`WorkspaceMarker`] are the artefacts that vote for a
//! manager. Yarn carries its [`YarnEdition`] inside the variant rather than
//! being a bare name, because the two editions take different command lines and
//! a detection that forgot which one it found would be useless.

use std::fmt;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uf_config::PackageManagerPreference;

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
    /// refines it through [`yarn_edition_in`](super::yarn_edition_in).
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
