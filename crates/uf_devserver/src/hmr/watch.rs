//! Poll-based file watching, with the interval said out loud and bounded.
//!
//! # Why polling
//!
//! `uf` has no third-party notification dependency, and the platform APIs
//! (`FSEvents`, `inotify`, `ReadDirectoryChangesW`) disagree about coalescing,
//! rename semantics and the number of events an editor's atomic save produces.
//! A poll is boring, portable, and — importantly for the property this feature
//! is judged on — it cannot report a change that did not happen. Invalidation
//! is where exactness is required; detection only has to be prompt.
//!
//! So this is a stat loop, and the cost is stated rather than hidden: the
//! interval is clamped into [`MIN_POLL_INTERVAL`]..=[`MAX_POLL_INTERVAL`] and
//! defaults to [`DEFAULT_POLL_INTERVAL`], the tree walk is bounded by
//! [`MAX_WATCHED_FILES`] and [`MAX_WATCH_DEPTH`], and both bounds are typed
//! errors rather than a truncated result — a watcher that silently stops
//! watching half a project is worse than one that refuses to start.
//!
//! # What is watched
//!
//! `.js` files only. That is the whole user-visible source surface of the
//! product; there are no `.ts`, `.jsx` or `.flow` files to consider.
//!
//! # What is not watched
//!
//! Symbolic links are skipped without following them, and any path the
//! [`FsPolicy`] deny list matches is skipped too. Both are the same rule: the
//! watcher must never turn a file the request pipeline would refuse to serve
//! into an event naming it. Editing `.env` produces no HMR event at all.

use std::fs;
use std::time::{Duration, SystemTime};

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use thiserror::Error;
use uf_infra::FxHashMap;

use crate::policy::FsPolicy;

use super::invalidate::ChangeKind;

/// Shortest accepted poll interval.
pub const MIN_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The poll interval `uf dev` uses unless configured otherwise.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Longest accepted poll interval.
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Most files one watcher will track.
pub const MAX_WATCHED_FILES: usize = 20_000;

/// Deepest directory the walk descends into.
pub const MAX_WATCH_DEPTH: usize = 32;

/// The only file extension the dev server watches.
pub const WATCHED_EXTENSION: &str = "js";

/// Directory names the walk never descends into.
///
/// A fixed `&'static str` table, not a configurable list: rule 3 of
/// `docs/security.md` — the walk's shape does not come from project text.
pub const SKIPPED_DIRECTORIES: &[&str] =
    &[".git", ".uf", "coverage", "dist", "node_modules", "target"];

/// Why a poll could not complete.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WatchError {
    /// The tree holds more than [`MAX_WATCHED_FILES`] watched files.
    #[error("the project holds more than {MAX_WATCHED_FILES} watched files")]
    TooManyFiles,
    /// A directory is deeper than [`MAX_WATCH_DEPTH`].
    #[error("{path} is deeper than the {MAX_WATCH_DEPTH} directory watch limit")]
    TooDeep {
        /// The directory that was too deep.
        path: Utf8PathBuf,
    },
    /// A directory could not be read.
    #[error("failed to read {path}: {message}")]
    Io {
        /// The directory that failed.
        path: Utf8PathBuf,
        /// The underlying failure.
        message: CompactString,
    },
    /// A path under the root is not valid UTF-8.
    #[error("a watched path is not valid UTF-8")]
    NonUtf8Path,
}

/// One change the watcher observed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Project-relative path, with forward slashes.
    pub path: Utf8PathBuf,
    /// What happened to it.
    pub change: ChangeKind,
}

/// What a poll remembers about one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// A stat-loop watcher over one project root.
#[derive(Debug)]
pub struct PollWatcher {
    root: Utf8PathBuf,
    interval: Duration,
    policy: Option<FsPolicy>,
    stamps: FxHashMap<Utf8PathBuf, FileStamp>,
    seeded: bool,
}

impl PollWatcher {
    /// Watch `root`, polling every `interval` clamped into the accepted range.
    pub fn new(root: &Utf8Path, interval: Duration) -> Self {
        Self {
            root: root.to_owned(),
            interval: interval.clamp(MIN_POLL_INTERVAL, MAX_POLL_INTERVAL),
            policy: None,
            stamps: FxHashMap::default(),
            seeded: false,
        }
    }

    /// Watch `root` at [`DEFAULT_POLL_INTERVAL`].
    pub fn with_default_interval(root: &Utf8Path) -> Self {
        Self::new(root, DEFAULT_POLL_INTERVAL)
    }

    /// Skip every path the policy's deny list matches.
    ///
    /// The dev server's deny list and the watcher's blind spots must be the
    /// same set, or an edit to `.env` would announce itself over the update
    /// channel even though the file can never be served.
    pub fn with_policy(mut self, policy: FsPolicy) -> Self {
        self.policy = Some(policy);
        self
    }

    /// The root being watched.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The clamped interval between polls.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// How many files the watcher is tracking.
    pub fn tracked(&self) -> usize {
        self.stamps.len()
    }

    /// Whether the first poll has run.
    ///
    /// The first poll records the tree without reporting every file as created;
    /// a dev server that announces five thousand updates at start-up is a dev
    /// server nobody reads the output of.
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// Walk the tree once and report what changed since the previous walk.
    ///
    /// # Errors
    ///
    /// Returns [`WatchError`] when a bound is exceeded or a directory cannot be
    /// read. A file that vanishes mid-walk is not an error: it is reported as
    /// [`ChangeKind::Deleted`] on this poll or the next one.
    pub fn poll(&mut self) -> Result<Vec<FileChange>, WatchError> {
        let scanned = self.scan()?;
        let mut changes = Vec::new();

        if !self.seeded {
            self.seeded = true;
            self.stamps = scanned;
            return Ok(changes);
        }

        for (path, stamp) in &scanned {
            match self.stamps.get(path) {
                None => changes.push(FileChange {
                    path: path.clone(),
                    change: ChangeKind::Created,
                }),
                Some(previous) if previous != stamp => changes.push(FileChange {
                    path: path.clone(),
                    change: ChangeKind::Modified,
                }),
                Some(_) => {}
            }
        }
        for path in self.stamps.keys() {
            if !scanned.contains_key(path) {
                changes.push(FileChange {
                    path: path.clone(),
                    change: ChangeKind::Deleted,
                });
            }
        }

        // Deterministic order, so two runs of the same edit produce the same
        // terminal output and the same event payload.
        changes.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.change.cmp(&right.change))
        });
        self.stamps = scanned;
        Ok(changes)
    }

    /// Walk the tree with an explicit stack.
    ///
    /// No recursion anywhere: a project can nest directories as deeply as the
    /// filesystem allows, and a recursive walk would meet that with a stack
    /// overflow instead of [`WatchError::TooDeep`].
    fn scan(&self) -> Result<FxHashMap<Utf8PathBuf, FileStamp>, WatchError> {
        let mut stamps = FxHashMap::default();
        let mut stack: Vec<(Utf8PathBuf, usize)> = vec![(Utf8PathBuf::new(), 0)];

        while let Some((relative, depth)) = stack.pop() {
            if depth > MAX_WATCH_DEPTH {
                return Err(WatchError::TooDeep { path: relative });
            }
            let absolute = if relative.as_str().is_empty() {
                self.root.clone()
            } else {
                self.root.join(&relative)
            };
            let entries = fs::read_dir(&absolute).map_err(|error| WatchError::Io {
                path: relative.clone(),
                message: CompactString::new(error.to_string()),
            })?;

            for entry in entries {
                let entry = entry.map_err(|error| WatchError::Io {
                    path: relative.clone(),
                    message: CompactString::new(error.to_string()),
                })?;
                let Ok(name) = entry.file_name().into_string() else {
                    return Err(WatchError::NonUtf8Path);
                };
                let child = if relative.as_str().is_empty() {
                    Utf8PathBuf::from(&name)
                } else {
                    relative.join(&name)
                };

                // `symlink_metadata` does not follow the link, so a link out of
                // the project is classified as a link and skipped rather than
                // walked. Following one would let a symlink drag an arbitrary
                // directory into the watched set.
                let metadata = match fs::symlink_metadata(entry.path()) {
                    Ok(metadata) => metadata,
                    // Vanished between `read_dir` and the stat. The next poll
                    // reports it; the walk does not fail.
                    Err(_) => continue,
                };
                if metadata.is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    if SKIPPED_DIRECTORIES.contains(&name.as_str()) || name.starts_with('.') {
                        continue;
                    }
                    stack.push((child, depth + 1));
                    continue;
                }
                if !metadata.is_file() || child.extension() != Some(WATCHED_EXTENSION) {
                    continue;
                }
                if self.is_denied(&child) {
                    continue;
                }
                if stamps.len() >= MAX_WATCHED_FILES {
                    return Err(WatchError::TooManyFiles);
                }
                stamps.insert(
                    child,
                    FileStamp {
                        modified: metadata.modified().ok(),
                        len: metadata.len(),
                    },
                );
            }
        }

        Ok(stamps)
    }

    fn is_denied(&self, relative: &Utf8Path) -> bool {
        self.policy
            .as_ref()
            .is_some_and(|policy| policy.deny_pattern_for(relative).is_some())
    }
}

/// The `.js` files under `root`, as the watcher would see them.
///
/// Used to seed a graph before the first poll, so `uf dev` starts with a graph
/// rather than discovering the project one edit at a time.
///
/// # Errors
///
/// Returns [`WatchError`] for the same reasons [`PollWatcher::poll`] does.
pub fn watched_files(watcher: &PollWatcher) -> Result<Vec<Utf8PathBuf>, WatchError> {
    let mut files: Vec<Utf8PathBuf> = watcher.scan()?.into_keys().collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests;
