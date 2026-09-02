//! Noticing that a file changed.
//!
//! # Why polling
//!
//! Watching by poll is a deliberate trade. The alternative is a per-platform
//! kernel notification API (`FSEvents`, `inotify`, `ReadDirectoryChangesW`),
//! which means a third-party dependency, three code paths, and a class of
//! platform-specific bugs — for a component whose job is to notice an edit a
//! human just made. A stat of every source file is a few hundred microseconds
//! on a project of a few thousand files, and the interval is bounded at both
//! ends: [`MIN_POLL_INTERVAL`] so the watcher cannot become a busy loop, and
//! [`MAX_POLL_INTERVAL`] so it cannot become unresponsive.
//!
//! Detection is (length, modification time). On a file system with one-second
//! modification-time granularity, two edits inside the same second that leave
//! the file the same length are indistinguishable — the standard limitation of
//! stat-based watching, and the reason the poll interval defaults above a
//! second's worth of margin rather than below it.
//!
//! What is *not* approximate is the invalidation: what reruns after a change is
//! decided by [`crate::graph::ImportGraph`], exactly.

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use uf_infra::FxHashMap;

use crate::path::is_safe_relative;

/// Shortest poll interval accepted.
pub const MIN_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Default poll interval: fast enough to feel immediate, slow enough that a
/// large project costs nothing measurable.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Longest poll interval accepted.
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// How often the watcher looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    interval: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            interval: DEFAULT_POLL_INTERVAL,
        }
    }
}

impl WatchOptions {
    /// Poll every `interval`, clamped into
    /// [`MIN_POLL_INTERVAL`]..=[`MAX_POLL_INTERVAL`].
    pub fn with_interval(interval: Duration) -> Self {
        Self {
            interval: interval.clamp(MIN_POLL_INTERVAL, MAX_POLL_INTERVAL),
        }
    }

    /// The clamped interval.
    pub fn interval(self) -> Duration {
        self.interval
    }
}

/// What one stat call recorded about a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified_nanos: u128,
}

impl FileStamp {
    fn read(path: &Utf8Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        if !metadata.is_file() {
            return None;
        }
        Some(Self {
            len: metadata.len(),
            modified_nanos: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|since| since.as_nanos())
                .unwrap_or_default(),
        })
    }
}

/// What changed between two polls.
///
/// Every list is sorted, so a change set is a value the tests can compare
/// directly rather than a bag whose order depends on the file system.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// Files that did not exist at the previous poll.
    pub added: Vec<CompactString>,
    /// Files whose length or modification time moved.
    pub modified: Vec<CompactString>,
    /// Files that have gone.
    pub removed: Vec<CompactString>,
}

impl ChangeSet {
    /// Whether nothing moved.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
    }

    /// How many files moved.
    pub fn len(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len()
    }

    /// Every path that moved, in sorted order.
    ///
    /// This is what the import graph is asked to invalidate: a removed file
    /// invalidates its importers exactly as an edited one does.
    pub fn paths(&self) -> Vec<CompactString> {
        let mut paths = Vec::with_capacity(self.len());
        paths.extend(self.added.iter().cloned());
        paths.extend(self.modified.iter().cloned());
        paths.extend(self.removed.iter().cloned());
        paths.sort_unstable();
        paths.dedup();
        paths
    }
}

/// A poll-based file watcher rooted at a project directory.
#[derive(Debug, Clone)]
pub struct Watcher {
    root: Utf8PathBuf,
    options: WatchOptions,
    stamps: FxHashMap<CompactString, FileStamp>,
}

impl Watcher {
    /// Watch the project at `root`, with nothing recorded yet.
    pub fn new(root: &Utf8Path, options: WatchOptions) -> Self {
        Self {
            root: root.to_path_buf(),
            options,
            stamps: FxHashMap::default(),
        }
    }

    /// The clamped poll interval a caller should sleep for.
    pub fn interval(&self) -> Duration {
        self.options.interval()
    }

    /// How many files are being tracked.
    pub fn len(&self) -> usize {
        self.stamps.len()
    }

    /// Whether nothing is being tracked.
    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }

    /// Record the current state of `files` without reporting anything.
    ///
    /// Called once after the first run, so that the first real poll reports
    /// edits rather than reporting the whole project as new.
    pub fn prime<'a>(&mut self, files: impl IntoIterator<Item = &'a str>) {
        for file in files {
            let Some(path) = self.resolve(file) else {
                continue;
            };
            if let Some(stamp) = FileStamp::read(&path) {
                self.stamps.insert(CompactString::from(file), stamp);
            }
        }
    }

    /// Stat `files` and report what moved since the previous poll.
    pub fn poll<'a>(&mut self, files: impl IntoIterator<Item = &'a str>) -> ChangeSet {
        let mut changes = ChangeSet::default();
        let mut seen: FxHashMap<CompactString, FileStamp> = FxHashMap::default();

        for file in files {
            let Some(path) = self.resolve(file) else {
                continue;
            };
            let Some(stamp) = FileStamp::read(&path) else {
                continue;
            };
            let key = CompactString::from(file);
            match self.stamps.get(&key) {
                None => changes.added.push(key.clone()),
                Some(previous) if *previous != stamp => changes.modified.push(key.clone()),
                Some(_) => {}
            }
            seen.insert(key, stamp);
        }

        for known in self.stamps.keys() {
            if !seen.contains_key(known) {
                changes.removed.push(known.clone());
            }
        }

        self.stamps = seen;
        changes.added.sort_unstable();
        changes.modified.sort_unstable();
        changes.removed.sort_unstable();
        changes
    }

    /// Join a project-relative path onto the root, refusing anything that is
    /// not project-relative.
    ///
    /// The watcher stats whatever it is handed, so this is the boundary that
    /// keeps a hostile path out of `/etc`.
    fn resolve(&self, file: &str) -> Option<Utf8PathBuf> {
        is_safe_relative(file).then(|| self.root.join(file))
    }
}

/// The wall-clock instant a caller should next poll at, for a loop that wants
/// to sleep in small slices so it stays interruptible.
pub fn next_poll_at(now: SystemTime, options: WatchOptions) -> SystemTime {
    now + options.interval()
}
