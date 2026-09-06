//! What an install changed, read off the lockfile it wrote.
//!
//! A package manager's own summary is a count — `added 16 packages` — and a
//! count is not an answer to "what changed". The lockfile is: it names every
//! package in the tree, its version, and where in `node_modules` it sits, both
//! before the install and after it. Reading it twice and subtracting is the
//! only way `uf install` can say *which* package moved from 4.17.20 to 4.17.21
//! without asking the manager to tell it.
//!
//! # Only npm's lockfile is read in detail
//!
//! `package-lock.json` and `npm-shrinkwrap.json` are JSON with a documented
//! shape. `pnpm-lock.yaml` is YAML, `yarn.lock` is Yarn's own syntax, and
//! `bun.lockb` is a binary format with no specification — parsing any of them
//! from a hand-written scanner would be three more things to be wrong about,
//! and a delta that is quietly wrong is worse than no delta. For those, a
//! [`LockfileSnapshot`] still records whether the file exists and how large it
//! is, so the summary can say the lockfile changed without inventing a list.
//!
//! # Untrusted input
//!
//! A lockfile is repository content, and this reads it before anything has
//! validated it. It is size-capped rather than buffered blindly, prototype
//! pollution keys are dropped the way they are everywhere else uf walks a JSON
//! map, and nothing read here ever reaches a command line.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use serde::Serialize;

use crate::detect::{Lockfile, PackageManager};
use crate::is_polluting_json_key;

/// Largest lockfile uf will parse for a delta.
///
/// A four-thousand-package `package-lock.json` is a few megabytes. Past this
/// the file is left alone and the summary reports sizes instead of names,
/// because a report is not worth an unbounded allocation.
pub const MAX_LOCKFILE_BYTES: u64 = 32 * 1024 * 1024;

/// The `node_modules` path segment npm keys its tree on.
const TREE_SEGMENT: &str = "node_modules/";

/// One package as a lockfile records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedEntry {
    /// Published package name.
    pub name: CompactString,
    /// The version installed at this path.
    pub version: CompactString,
}

/// A lockfile as it stood at one moment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LockfileSnapshot {
    /// The file this describes, whether or not it exists.
    pub path: Utf8PathBuf,
    /// Whether the file was there.
    pub present: bool,
    /// Its size in bytes.
    pub bytes: u64,
    /// Every package in the tree, keyed by its path under the project root.
    ///
    /// Empty when the format is one uf does not parse, which is not the same
    /// as a tree with nothing in it — see [`LockfileSnapshot::detailed`].
    pub entries: BTreeMap<CompactString, LockedEntry>,
    /// Whether [`LockfileSnapshot::entries`] was actually read.
    pub detailed: bool,
}

impl LockfileSnapshot {
    /// How many packages the tree holds, or `None` when uf did not read it.
    #[must_use]
    pub fn package_count(&self) -> Option<usize> {
        self.detailed.then_some(self.entries.len())
    }
}

/// How one package differs between two lockfiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    /// It was not in the tree before.
    Added,
    /// It is not in the tree any more.
    Removed,
    /// It is at a different version.
    Updated,
    /// Same version, different place in the tree.
    ///
    /// What hoisting does. Nothing was downloaded and nothing was deleted, but
    /// a `node_modules` that moved a package is a `node_modules` that resolves
    /// differently, which is worth a line when a build starts behaving oddly.
    Moved,
}

impl ChangeKind {
    /// Every kind, in the order a summary lists them.
    pub const ALL: [Self; 4] = [Self::Added, Self::Removed, Self::Updated, Self::Moved];

    /// Stable identifier, matching the emitted JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Removed => "removed",
            Self::Updated => "updated",
            Self::Moved => "moved",
        }
    }

    /// The one-character mark a summary puts beside the package.
    #[must_use]
    pub const fn mark(self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Removed => "-",
            Self::Updated => "~",
            Self::Moved => ">",
        }
    }
}

/// One package that is not what it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageChange {
    /// Published package name.
    pub name: CompactString,
    /// What happened to it.
    pub kind: ChangeKind,
    /// Version, or versions, before the install.
    pub before: CompactString,
    /// Version, or versions, after it.
    pub after: CompactString,
}

/// Everything an install changed in the lockfile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LockfileDelta {
    /// Every changed package, sorted by kind and then by name.
    pub changes: Vec<PackageChange>,
    /// Packages in the tree before, when uf could read the format.
    pub packages_before: Option<usize>,
    /// Packages in the tree after.
    pub packages_after: Option<usize>,
    /// Lockfile size before, in bytes.
    pub bytes_before: u64,
    /// Lockfile size after, in bytes.
    pub bytes_after: u64,
    /// Whether the lockfile existed before the install.
    pub existed_before: bool,
    /// Whether it exists now.
    pub exists_now: bool,
    /// Whether the per-package list is real, rather than merely empty.
    pub detailed: bool,
}

impl LockfileDelta {
    /// How many packages changed in this way.
    #[must_use]
    pub fn count(&self, kind: ChangeKind) -> usize {
        self.changes
            .iter()
            .filter(|change| change.kind == kind)
            .count()
    }

    /// Whether the install left the dependency tree exactly as it found it.
    ///
    /// A lockfile uf cannot parse falls back to its size and its existence,
    /// which is the honest answer available: a byte-identical lockfile is
    /// strong evidence nothing moved, and a different one is proof something
    /// did.
    #[must_use]
    pub fn is_unchanged(&self) -> bool {
        if self.detailed {
            return self.changes.is_empty();
        }
        self.existed_before == self.exists_now && self.bytes_before == self.bytes_after
    }
}

/// Read the lockfile `manager` writes under `root`.
///
/// Never fails: a lockfile that is missing, too large, or in a format uf does
/// not parse gives a snapshot that says so.
#[must_use]
pub fn snapshot(root: &Utf8Path, manager: PackageManager) -> LockfileSnapshot {
    let path = lockfile_path(root, manager);
    let Ok(metadata) = fs::metadata(&path) else {
        return LockfileSnapshot {
            path,
            ..LockfileSnapshot::default()
        };
    };
    if !metadata.is_file() {
        // Never follow a lockfile that is a directory or a device; the same
        // rule detection applies to a manifest.
        return LockfileSnapshot {
            path,
            ..LockfileSnapshot::default()
        };
    }
    let bytes = metadata.len();
    let mut snapshot = LockfileSnapshot {
        path,
        present: true,
        bytes,
        entries: BTreeMap::new(),
        detailed: false,
    };
    if !parses_in_detail(manager) || bytes > MAX_LOCKFILE_BYTES {
        return snapshot;
    }
    let Ok(text) = fs::read_to_string(&snapshot.path) else {
        return snapshot;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return snapshot;
    };
    let Some(packages) = value.get("packages").and_then(serde_json::Value::as_object) else {
        // Lockfile version 1 nests its tree under `dependencies` instead, and
        // npm has not written one since npm 6. Reporting sizes is better than
        // reporting a tree read out of a shape this does not understand.
        return snapshot;
    };
    for (path, entry) in packages {
        if is_polluting_json_key(path) {
            continue;
        }
        // The root entry describes the project, which is not one of its
        // dependencies.
        if path.is_empty() {
            continue;
        }
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let Some(version) = entry.get("version").and_then(serde_json::Value::as_str) else {
            // A workspace link carries no version of its own; it is a pointer
            // to a directory this repository already has.
            continue;
        };
        let Some(name) = tree_name(path) else {
            continue;
        };
        snapshot.entries.insert(
            path.to_compact_string(),
            LockedEntry {
                name,
                version: version.to_compact_string(),
            },
        );
    }
    snapshot.detailed = true;
    snapshot
}

/// Subtract `before` from `after`.
#[must_use]
pub fn diff(before: &LockfileSnapshot, after: &LockfileSnapshot) -> LockfileDelta {
    let detailed = before.detailed || after.detailed;
    let mut delta = LockfileDelta {
        changes: Vec::new(),
        packages_before: before.package_count(),
        packages_after: after.package_count(),
        bytes_before: before.bytes,
        bytes_after: after.bytes,
        existed_before: before.present,
        exists_now: after.present,
        // A lockfile that did not exist before is not an unparsed one: an
        // absent tree is an empty tree, and every package in the new lockfile
        // is genuinely new.
        detailed: detailed && (before.detailed || !before.present) && after.detailed,
    };
    if !delta.detailed {
        return delta;
    }

    let before = by_name(before);
    let after = by_name(after);
    let mut names: BTreeSet<&CompactString> = BTreeSet::new();
    names.extend(before.keys());
    names.extend(after.keys());

    for name in names {
        let was = before.get(name);
        let now = after.get(name);
        let kind = match (was, now) {
            (None, Some(_)) => ChangeKind::Added,
            (Some(_), None) => ChangeKind::Removed,
            (Some(was), Some(now)) if was.versions != now.versions => ChangeKind::Updated,
            (Some(was), Some(now)) if was.paths != now.paths => ChangeKind::Moved,
            _ => continue,
        };
        delta.changes.push(PackageChange {
            name: name.clone(),
            kind,
            before: was.map(Placement::version_label).unwrap_or_default(),
            after: now.map(Placement::version_label).unwrap_or_default(),
        });
    }
    delta.changes.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.cmp(&right.name))
    });
    delta
}

/// Where one package sits in a tree, and at which versions.
#[derive(Debug, Default)]
struct Placement {
    versions: BTreeSet<CompactString>,
    paths: BTreeSet<CompactString>,
}

impl Placement {
    /// Every version this package is installed at, for display.
    ///
    /// Usually one. A tree that nests two versions of the same package says
    /// both, because reporting one of them would be picking a winner uf has no
    /// basis to pick.
    fn version_label(&self) -> CompactString {
        let mut label = CompactString::default();
        for version in &self.versions {
            if !label.is_empty() {
                label.push_str(", ");
            }
            label.push_str(version);
        }
        label
    }
}

fn by_name(snapshot: &LockfileSnapshot) -> BTreeMap<CompactString, Placement> {
    let mut out: BTreeMap<CompactString, Placement> = BTreeMap::new();
    for (path, entry) in &snapshot.entries {
        let placement = out.entry(entry.name.clone()).or_default();
        placement.versions.insert(entry.version.clone());
        placement.paths.insert(path.clone());
    }
    out
}

/// The package a `node_modules` path names.
///
/// `node_modules/a/node_modules/@scope/b` is `@scope/b`: what matters is the
/// last installation on the path, because that is the package the entry
/// describes.
fn tree_name(path: &str) -> Option<CompactString> {
    let last = path.rsplit(TREE_SEGMENT).next()?;
    (!last.is_empty() && last != path).then(|| last.to_compact_string())
}

/// Whether uf reads this manager's lockfile as a tree rather than as bytes.
const fn parses_in_detail(manager: PackageManager) -> bool {
    matches!(manager, PackageManager::Npm)
}

/// The lockfile `manager` writes under `root`.
///
/// Two managers write one of two files rather than always the same one, and
/// which one is not something uf can decide from the manager's name. npm
/// honours an existing `npm-shrinkwrap.json` over a `package-lock.json`; Bun
/// wrote the binary `bun.lockb` until it started writing the textual
/// `bun.lock`, and a machine may have either version. So the file is chosen by
/// looking, and only the last resort is an assumption.
fn lockfile_path(root: &Utf8Path, manager: PackageManager) -> Utf8PathBuf {
    let candidates: &[Lockfile] = match manager {
        PackageManager::Npm => &[Lockfile::NpmShrinkwrap, Lockfile::PackageLock],
        PackageManager::Bun => &[Lockfile::BunLock, Lockfile::BunLockb],
        _ => &[],
    };
    for candidate in candidates {
        let path = root.join(candidate.file_name());
        if path.is_file() {
            return path;
        }
    }
    root.join(manager.lockfile().file_name())
}

#[cfg(test)]
mod tests;
