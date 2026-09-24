//! Hearing about a change from the kernel instead of looking for one.
//!
//! [`crate::Watcher`] notices an edit by `stat`ing every watched file on a
//! timer. That is portable and cheap per look, but a save waits for the next
//! look — up to a quarter of a second on a large project, which once a rerun
//! itself takes tens of milliseconds is most of the time between saving and
//! seeing the result — and every look costs a `stat` per file whether anything
//! changed or not.
//!
//! This asks the operating system to say when something changed: FSEvents on
//! macOS and inotify on Linux, through the `notify` crate. A save is heard
//! within milliseconds, and an idle session costs nothing.
//!
//! # What it watches
//!
//! Each directory that holds a watched file, and the project root — each one
//! on its own, not recursively. A recursive watch of the root would, on Linux,
//! place an inotify watch on every directory under `node_modules`, which on a
//! real project is thousands and can exhaust the per-user limit before it has
//! watched anything that matters. A file created in a directory no watched file
//! was in yet is found by the caller's periodic rescan instead, which is what
//! it was always found by.
//!
//! # What it is not
//!
//! The truth about what changed. An event says a path was touched; whether its
//! contents moved is still decided by reading them, exactly as for a poll (see
//! the `uf test --watch` loop). Events can also be dropped — a kernel queue
//! overflows, a backend gives up — and the watcher reports that as a request to
//! rescan rather than pretending nothing happened.
//!
//! # When it is not used
//!
//! Where the watcher cannot start — a file system that sends no events (many
//! network mounts, some container overlays), an inotify limit already reached —
//! [`EventWatcher::new`] or [`EventWatcher::watch_directories`] answers with the
//! error, and the caller polls with [`crate::Watcher`] as before.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use notify::{EventKind, RecursiveMode, Watcher as _};

use crate::path::is_safe_relative;

/// How long a burst of events is allowed to keep arriving before it is
/// reported as one change.
///
/// An editor's save is often several writes — truncate, write, rename a
/// temporary file over the original — and a run started on the first of them
/// would read a half-written file and run again on the next. Short enough to
/// be invisible next to the run it starts.
pub const EVENT_QUIET: Duration = Duration::from_millis(8);

/// The longest a burst is waited on before it is reported anyway, so a
/// process writing continuously cannot hold a rerun back forever.
pub const EVENT_BURST_LIMIT: Duration = Duration::from_millis(100);

/// What the kernel said since the last time it was asked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventBatch {
    /// Paths that were created, written, renamed or removed, relative to the
    /// project root, sorted and without duplicates. Only paths inside the
    /// root; anything else a backend reports is dropped.
    pub changed: Vec<CompactString>,
    /// Whether the backend lost events — a queue overflow, a watch that had to
    /// be dropped — so what `changed` lists is not everything that happened.
    /// The caller rescans when this is set.
    pub rescan: bool,
}

/// A running subscription to the kernel's file-change events for a project.
pub struct EventWatcher {
    watcher: notify::RecommendedWatcher,
    events: Receiver<notify::Result<notify::Event>>,
    /// The root as the kernel names it: canonical, because FSEvents reports
    /// `/private/tmp/…` for a project opened as `/tmp/…`.
    root: PathBuf,
    /// The directories already subscribed to.
    watched: BTreeSet<PathBuf>,
}

impl std::fmt::Debug for EventWatcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventWatcher")
            .field("root", &self.root)
            .field("watched", &self.watched.len())
            .finish_non_exhaustive()
    }
}

impl EventWatcher {
    /// Start listening for changes under `root`, watching the root itself.
    ///
    /// # Errors
    ///
    /// The backend's error when the platform has none, the root cannot be
    /// resolved, or the root cannot be watched. The caller falls back to
    /// polling.
    pub fn new(root: &Utf8Path) -> Result<Self, notify::Error> {
        let root = std::fs::canonicalize(root.as_std_path()).map_err(notify::Error::io)?;
        let (sender, events) = channel();
        let watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })?;
        let mut this = Self {
            watcher,
            events,
            root: root.clone(),
            watched: BTreeSet::new(),
        };
        this.watch(&root)?;
        Ok(this)
    }

    /// Subscribe to the directory of every file in `files`, project-relative,
    /// that is not already subscribed to.
    ///
    /// Called with the whole selection after each scan, so a directory that
    /// gained its first watched file is heard from from then on. A path that is
    /// not safely project-relative is skipped rather than joined.
    ///
    /// # Errors
    ///
    /// The backend's error for the first directory it refused, typically an
    /// inotify watch limit. The subscriptions made before it stay.
    pub fn watch_directories<'a>(
        &mut self,
        files: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), notify::Error> {
        let mut wanted = BTreeSet::new();
        for file in files {
            if !is_safe_relative(file) {
                continue;
            }
            if let Some(parent) = self.root.join(file).parent() {
                wanted.insert(parent.to_path_buf());
            }
        }
        for directory in wanted {
            if !self.watched.contains(&directory) {
                self.watch(&directory)?;
            }
        }
        Ok(())
    }

    /// Prove that events actually arrive, by writing a file where the kernel
    /// is watching and waiting to hear about it.
    ///
    /// Starting a subscription is not the same as receiving events. A process
    /// in a sandbox that denies it the file-event service, some network and
    /// container file systems, and a kernel queue that is already full all let
    /// the watch be set up and then say nothing — and a session that trusted
    /// that silence would never notice a save. So the caller asks once, before
    /// relying on it, and polls instead when the answer is no.
    ///
    /// `directory` is created if it is missing, subscribed to, and given one
    /// small file, `watch-probe`, which is left in place for the next session.
    /// It should be somewhere the project keeps its own state (`.uf/`), never
    /// among the project's sources. Every event received while waiting is
    /// consumed.
    ///
    /// # Errors
    ///
    /// The I/O or backend error when the probe could not be written or
    /// watched. `Ok(false)` when it was, and nothing was heard within
    /// `timeout`.
    pub fn verify(
        &mut self,
        directory: &Utf8Path,
        timeout: Duration,
    ) -> Result<bool, notify::Error> {
        std::fs::create_dir_all(directory.as_std_path()).map_err(notify::Error::io)?;
        let directory =
            std::fs::canonicalize(directory.as_std_path()).map_err(notify::Error::io)?;
        if !self.watched.contains(&directory) {
            self.watch(&directory)?;
        }
        let probe = directory.join("watch-probe");
        let deadline = Instant::now() + timeout;
        let mut written = 0u64;
        while Instant::now() < deadline {
            written += 1;
            std::fs::write(&probe, written.to_string()).map_err(notify::Error::io)?;
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(250));
            match self.events.recv_timeout(wait) {
                Ok(Ok(event)) if event.paths.iter().any(|path| path == &probe) => return Ok(true),
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(false),
            }
        }
        Ok(false)
    }

    /// How many directories are subscribed to.
    #[must_use]
    pub fn directories(&self) -> usize {
        self.watched.len()
    }

    /// Wait up to `timeout` for a change, and report it with everything that
    /// arrived in the same burst.
    ///
    /// `None` when nothing happened within `timeout`. A burst ends when no
    /// event has arrived for [`EVENT_QUIET`], or [`EVENT_BURST_LIMIT`] after it
    /// began. Events that only read a file are ignored. A backend that has
    /// stopped altogether — its channel closed — is reported as a rescan, once
    /// per call, so the caller notices and can fall back.
    pub fn wait(&self, timeout: Duration) -> Option<EventBatch> {
        let first = match self.events.recv_timeout(timeout) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => return None,
            Err(RecvTimeoutError::Disconnected) => {
                return Some(EventBatch {
                    changed: Vec::new(),
                    rescan: true,
                });
            }
        };
        let began = Instant::now();
        let mut changed = BTreeSet::new();
        let mut rescan = false;
        self.take(first, &mut changed, &mut rescan);
        loop {
            let left = EVENT_BURST_LIMIT.saturating_sub(began.elapsed());
            if left.is_zero() {
                break;
            }
            match self.events.recv_timeout(EVENT_QUIET.min(left)) {
                Ok(event) => self.take(event, &mut changed, &mut rescan),
                Err(_) => break,
            }
        }
        if changed.is_empty() && !rescan {
            // A burst of reads only: nothing to report, but it was not a
            // timeout either. Reported as an empty batch the caller can skip.
            return Some(EventBatch::default());
        }
        Some(EventBatch {
            changed: changed.into_iter().collect(),
            rescan,
        })
    }

    fn watch(&mut self, directory: &Path) -> Result<(), notify::Error> {
        self.watcher.watch(directory, RecursiveMode::NonRecursive)?;
        self.watched.insert(directory.to_path_buf());
        Ok(())
    }

    /// Fold one backend event into the burst being collected.
    fn take(
        &self,
        event: notify::Result<notify::Event>,
        changed: &mut BTreeSet<CompactString>,
        rescan: &mut bool,
    ) {
        let event = match event {
            Ok(event) => event,
            Err(_) => {
                *rescan = true;
                return;
            }
        };
        if event.need_rescan() {
            *rescan = true;
        }
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        for path in &event.paths {
            if let Some(relative) = relative_to(&self.root, path) {
                changed.insert(relative);
            }
        }
    }
}

/// `path` relative to `root`, with `/` separators, when it is inside `root`.
fn relative_to(root: &Path, path: &Path) -> Option<CompactString> {
    let inside = path.strip_prefix(root).ok()?;
    let relative = Utf8PathBuf::from_path_buf(inside.to_path_buf()).ok()?;
    let spelled = relative.as_str().replace('\\', "/");
    (!spelled.is_empty() && is_safe_relative(&spelled)).then(|| CompactString::from(spelled))
}
