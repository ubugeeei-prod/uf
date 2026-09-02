//! Reading the lockfile evidence out of one directory.
//!
//! Nothing here follows a symlink: a lockfile pointing outside the checkout must
//! not get a vote. The Yarn edition is decided from a bounded prefix of
//! `yarn.lock` rather than the whole file, and lockfiles naming genuinely
//! different managers are reported as ambiguous instead of being resolved
//! silently.

use std::fs;
use std::io::Read;

use camino::Utf8Path;
use smallvec::SmallVec;

use super::LockfileList;
use super::manager::{Lockfile, PackageManager, YarnEdition};
use super::outcome::DetectionOutcome;

/// Bytes read from `yarn.lock` when classifying the Yarn edition.
const YARN_LOCK_PROBE_BYTES: usize = 8 * 1024;

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

pub(crate) fn managers_for(
    dir: &Utf8Path,
    lockfiles: &LockfileList,
) -> SmallVec<[PackageManager; 4]> {
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

pub(crate) fn ambiguity(lockfiles: &LockfileList, managers: &[PackageManager]) -> DetectionOutcome {
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

pub(crate) fn is_regular_file(path: &Utf8Path) -> bool {
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
