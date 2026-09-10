//! Keeping a cache directory under a bound.
//!
//! uf keeps three answer caches under `.uf/cache/` — `transform/` written by
//! the Capability JS Host's loader, `check/` written by `uf check`, `task/`
//! written by `uf run` — and until this existed none of them ever gave a byte
//! back. Every one is content-addressed, so an entry is orphaned the moment
//! anything in its key changes: editing a file orphans one entry, and
//! rebuilding `uf` orphans an entire generation at once because the compiler's
//! identity is in every key. On this repository that is about 330 modules and
//! 9 MB of `transform/` per build, and `rm -rf .uf/cache` was the only reclaim.
//!
//! `.uf/cache/assets/` is deliberately not one of them, and the omission is a
//! decision rather than an oversight. Its entries are build *outputs* — the
//! resized images a build is about to copy into `dist/` — read back later in
//! the same run, so what may be evicted is a question about that pipeline's
//! lifecycle and not about bytes. It also does not have the growth this exists
//! to stop: an image is keyed by its own content and parameters, and
//! rebuilding `uf` does not orphan a generation of it.
//!
//! # The policy, and why it is this one
//!
//! **A byte cap per cache directory, evicting least recently used first.**
//!
//! Three were considered, and ubugeeei-prod/uf#218 has the argument in full:
//!
//! * *Keep only the current compiler identity, sweeping the rest.* The
//!   smallest cache, and wrong: it makes a bisect or a rebase that walks back
//!   over a compiler change recompile everything each way. Both the transform
//!   cache and the check cache have a test that pins the *opposite* guarantee
//!   on purpose — `packages/host/transform-cache.test.js`'s "still has a
//!   build's entries when that build comes back" and
//!   `uf_check::tests::cache::a_rebuilt_uf_is_not_served_what_the_previous_one_decided`
//!   — so this option means retiring a guarantee rather than adding one.
//! * *Age: drop entries untouched for N days.* Simple, and unrelated to the
//!   thing anybody complains about, which is disk. A week of heavy rebuilding
//!   is over the budget on day one and under it on day eight.
//! * *A byte cap, least recently used first.* Bounded in the unit the
//!   complaint is in, and it keeps the entries a bisect is about to want,
//!   because those are the ones that were read most recently.
//!
//! The last one is what this is. The bound is a number somebody had to pick;
//! [`MAX_CACHE_BYTES`] says which number and what it buys.
//!
//! # "Least recently used" on a filesystem that may not be keeping score
//!
//! An entry is written once and thereafter only read, so its modification time
//! is when it was created and its access time is when it was last hit. This
//! takes the later of the two, which is the honest reading of "last used".
//!
//! Access times are not guaranteed to move: `relatime` — the Linux default —
//! updates one at most once a day, and `noatime` never does. Where they do not
//! move, the later of the two is the modification time and the policy degrades
//! to evicting the *oldest* entries rather than the coldest ones. That is
//! coarser and it is still a bound, which is the property this exists for; it
//! is written down here so the degradation is a known one rather than a
//! surprise.
//!
//! # Being safe against the twelve workers of `uf test`
//!
//! Every one of these caches is written by an atomic rename precisely because
//! a reader must never see half a file, and a sweep has to be at least as
//! careful. Three rules make it so:
//!
//! * **Nothing is ever rewritten or truncated, only unlinked.** A reader that
//!   already opened an entry reads it to the end on every platform uf targets;
//!   a reader that has not yet opened it gets a miss, and a miss costs one
//!   recompilation and cannot be wrong.
//! * **A file used within [`CacheBound::grace`] is never removed.** That is
//!   what keeps the sweep away from a temporary another process is about to
//!   rename into place, and from an entry a run in flight has just written.
//! * **A removal that fails is not an error.** Another sweep got there first,
//!   or Windows has the file open. Either way the byte budget is a target and
//!   not an assertion.
//!
//! Two sweeps racing may each decide to remove the same entry, and may
//! together take the directory below the target. Both are harmless: the cost
//! is a recompilation, and the alternative — a lock file — would be a new way
//! for a crashed process to stop every later one.
//!
//! # What is not swept
//!
//! Only files directly in the directory. Subdirectories are left alone,
//! contents and all, because a cache keeps things beside its entries that are
//! not entries: `.uf/cache/task/last/` holds one note per task recording what
//! that task keyed on, which is what makes `uf run --why` able to name the
//! file that changed. It is bounded by the number of tasks and rewritten in
//! place, so it is not what grows — and evicting it by size would take away an
//! explanation to save a kilobyte.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How much one cache directory may hold before a sweep, in bytes.
///
/// 128 MiB, and the number is chosen against measured entries rather than
/// picked for roundness. On this repository `.uf/cache/transform` holds about
/// 330 modules per build at an average of 28 KB each — 9 MB a build — so the
/// cap is about fourteen builds of headroom, and `.uf/cache/check` at 6 KB a
/// record is many times that. Fourteen builds is the span a bisect or a rebase
/// walks over, which is the case the whole come-back-warm guarantee exists to
/// serve; a month of them is not, and that is what was accumulating.
///
/// It is deliberately per directory rather than over `.uf/cache` as a whole.
/// The three caches are written by three different processes at three
/// different times, and a shared budget would make `uf run` evict what
/// `uf check` was about to read — a coupling nobody could predict from either
/// command.
pub const MAX_CACHE_BYTES: u64 = 128 * 1024 * 1024;

/// What a sweep brings a directory down to once it is over the cap.
///
/// Three quarters of [`MAX_CACHE_BYTES`], and the gap is the point: sweeping
/// back to exactly the cap would put the next run over it again, so every run
/// from then on would pay for a full sort of the directory to evict one entry.
/// Leaving a quarter free means a sweep happens once per quarter-budget of new
/// entries, and every run in between stops after one `read_dir`.
pub const SWEEP_TARGET_BYTES: u64 = MAX_CACHE_BYTES / 4 * 3;

/// How recently a file must have been used to be safe from a sweep.
///
/// See this module's header: this is the rule that keeps a sweep away from a
/// temporary a concurrent writer is about to rename into place. A minute is
/// far longer than the window between a write and its rename and far shorter
/// than the age of anything a sweep is actually for.
pub const SWEEP_GRACE: Duration = Duration::from_secs(60);

/// What a cache directory is allowed to hold, and what a sweep leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheBound {
    /// Sweep when the directory holds more than this many bytes.
    pub max_bytes: u64,
    /// Stop evicting once it holds no more than this many.
    pub target_bytes: u64,
    /// Never remove a file used more recently than this.
    pub grace: Duration,
}

impl Default for CacheBound {
    /// The bound every uf cache uses: see [`MAX_CACHE_BYTES`].
    fn default() -> Self {
        Self {
            max_bytes: MAX_CACHE_BYTES,
            target_bytes: SWEEP_TARGET_BYTES,
            grace: SWEEP_GRACE,
        }
    }
}

/// What one sweep did.
///
/// Returned rather than logged: a sweep is not something a command should
/// announce — the reader asked for a build, not for housekeeping — but it is
/// something a test has to be able to assert about.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Swept {
    /// Bytes the directory held when the sweep started.
    pub bytes_before: u64,
    /// Bytes it holds now, as far as this sweep can tell.
    pub bytes_after: u64,
    /// Entries unlinked.
    pub removed: usize,
    /// Entries that survived, including any the grace period protected.
    pub kept: usize,
}

/// One file the sweep may remove.
struct Entry {
    path: PathBuf,
    bytes: u64,
    /// The later of the file's access and modification times.
    used_at: SystemTime,
}

/// Bring `directory` back under `bound`, evicting least recently used first.
///
/// Cheap and silent in the overwhelmingly common case: one `read_dir` and one
/// `stat` per entry, no sort and no writes, because the directory is under the
/// cap. A directory that does not exist yet is not an error — a project that
/// has never been built has no cache to bound.
///
/// Removal failures are ignored; see this module's header for why that is the
/// right answer rather than a swallowed error.
pub fn sweep(directory: &Path, bound: CacheBound) -> Swept {
    let Ok(listing) = fs::read_dir(directory) else {
        return Swept::default();
    };

    let mut entries = Vec::new();
    let mut bytes_before = 0u64;
    for entry in listing.flatten() {
        // `DirEntry::metadata` does not follow symlinks, so a link planted in
        // a cache directory is counted as the few bytes it is and unlinked as
        // itself. Following it would let the sweep delete whatever it pointed
        // at, which is not a thing a cache should ever be able to do.
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let bytes = metadata.len();
        bytes_before = bytes_before.saturating_add(bytes);
        entries.push(Entry {
            path: entry.path(),
            bytes,
            used_at: used_at(&metadata),
        });
    }

    if bytes_before <= bound.max_bytes {
        return Swept {
            bytes_before,
            bytes_after: bytes_before,
            removed: 0,
            kept: entries.len(),
        };
    }

    // Coldest first.
    entries.sort_unstable_by_key(|entry| entry.used_at);

    let now = SystemTime::now();
    let mut bytes_after = bytes_before;
    let mut removed = 0;
    for entry in &entries {
        if bytes_after <= bound.target_bytes {
            break;
        }
        let fresh = match now.duration_since(entry.used_at) {
            Ok(age) => age < bound.grace,
            // Used in the future: a clock that moved, or a tree copied with
            // its timestamps. Protected, which is the safe direction — the
            // worst case is a cache over its budget until the timestamp is
            // behind us, rather than a sweep that removes what a running
            // process is writing.
            Err(_) => true,
        };
        if fresh || fs::remove_file(&entry.path).is_err() {
            continue;
        }
        bytes_after = bytes_after.saturating_sub(entry.bytes);
        removed += 1;
    }

    Swept {
        bytes_before,
        bytes_after,
        removed,
        kept: entries.len() - removed,
    }
}

/// When a file was last used: the later of its access and modification times.
///
/// A filesystem that reports neither leaves the entry looking as old as it can
/// be, which puts it first in line. That is the right way round: an entry
/// whose age cannot be established is exactly the one there is no reason to
/// keep, and it is protected from a sweep that is running beside a writer by
/// the grace period rather than by its timestamp.
fn used_at(metadata: &fs::Metadata) -> SystemTime {
    let accessed = metadata.accessed().ok();
    let modified = metadata.modified().ok();
    match (accessed, modified) {
        (Some(accessed), Some(modified)) => accessed.max(modified),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => SystemTime::UNIX_EPOCH,
    }
}

#[cfg(test)]
mod tests;
