//! Workers kept from one run to the next.
//!
//! A run of one small file is mostly a worker starting: the host process, the
//! Flow loader, and the dependency graph the file imports — React, a DOM, the
//! package under test — before a single case runs. `uf test --watch` reruns a
//! handful of files after every save, and paying that each time is most of
//! what a developer waits for. So a watch session keeps its workers here
//! between runs, and a rerun costs the file and whatever the edit touched.
//!
//! # What keeps a kept worker honest
//!
//! A process keeps every module it evaluated, so a worker that ran the file
//! yesterday would run yesterday's code. Before a kept worker is handed a file
//! it is told which files changed ([`WorkerPool::invalidate`]), and its loader
//! imports those, and every loaded module that reaches one, afresh from then
//! on. The graph it walks is the one it actually loaded, not one read off the
//! source, so a bare specifier or a dynamic `import()` is an edge like any
//! other. A worker that cannot promise that — its host has no in-thread
//! loader, or a module in the way was loaded with `require()` — says so and is
//! discarded, and the next file gets a fresh process: slower, never stale.
//!
//! Between files, a kept worker is exactly what every worker already is: the
//! files it runs share the modules neither of them changed, and the state
//! `@uniflowed/test` puts back before each file is put back before each rerun
//! too.
//!
//! # Bounds
//!
//! Every file a worker loads afresh is another copy of those modules in its
//! registry, and nothing takes one out. So a worker is retired after
//! [`MAX_REQUESTS_PER_KEPT_WORKER`] requests, and a fresh one takes its place
//! on the next run.

use std::sync::Mutex;
use std::time::Duration;

use crate::host::Worker;

/// How many requests one kept worker serves before it is retired.
///
/// A bound on the memory an afresh-loaded module holds on to, not a guess at
/// when a worker goes bad: at a few files per save it is an hour or more of
/// editing, and the cost of crossing it is one worker start.
pub const MAX_REQUESTS_PER_KEPT_WORKER: usize = 400;

/// How long a kept worker has to answer which files changed.
///
/// Walking the graph it loaded is microseconds; this is how long a worker that
/// is still busy with something a finished file left behind may take before it
/// is discarded instead.
pub const INVALIDATE_TIMEOUT: Duration = Duration::from_secs(5);

/// The workers a watch session keeps between runs.
///
/// Lent to [`crate::TestRunner::run_observed_in`]. A run takes workers from here
/// before it starts any, and gives back the ones that finished healthy.
#[derive(Debug, Default)]
pub struct WorkerPool {
    idle: Mutex<Vec<Worker>>,
}

impl WorkerPool {
    /// An empty pool.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many workers are waiting for the next run.
    ///
    /// Zero after a run that retired every worker it used — each one timed
    /// out, crashed or served its share — and before the first run, which is
    /// when the next run starts fresh ones. A poisoned lock reads as zero: a
    /// pool that cannot be looked at has nothing to offer.
    #[must_use]
    pub fn idle(&self) -> usize {
        self.idle.lock().map_or(0, |idle| idle.len())
    }

    /// Tell every waiting worker that `changed` changed.
    ///
    /// Absolute, canonical paths, as the host's loader resolves them — a path
    /// spelled through a symlink names nothing the worker loaded, and the edit
    /// would not reach it. Each worker is asked in turn and answers in
    /// microseconds; one that does not answer within [`INVALIDATE_TIMEOUT`],
    /// or answers that it cannot promise its next file sees the change, is
    /// dropped, which stops it. The returned count is how many were kept.
    ///
    /// Only waiting workers are told, which is every worker there is between
    /// two runs of a watch session: a run gives back what it used before the
    /// session looks for the next change.
    pub fn invalidate(&self, changed: &[String]) -> usize {
        let Ok(mut idle) = self.idle.lock() else {
            return 0;
        };
        idle.retain_mut(|worker| worker.invalidate(changed, INVALIDATE_TIMEOUT));
        idle.len()
    }

    /// A waiting worker, when there is one: it has been told every change
    /// since it last ran a file, and promised its next file sees them.
    pub(crate) fn take(&self) -> Option<Worker> {
        self.idle.lock().ok().and_then(|mut idle| idle.pop())
    }

    /// Keep `worker` for the next run, unless it has served its share.
    ///
    /// Only a worker that finished its last file healthy reaches here: one
    /// that timed out or lost its host was stopped by the run that used it.
    /// One past [`MAX_REQUESTS_PER_KEPT_WORKER`] is dropped, which stops it.
    pub(crate) fn give_back(&self, worker: Worker) {
        if worker.served() >= MAX_REQUESTS_PER_KEPT_WORKER {
            return;
        }
        if let Ok(mut idle) = self.idle.lock() {
            idle.push(worker);
        }
    }

    /// Stop every waiting worker.
    ///
    /// For a session that is ending, or one that has to start over — a
    /// changed environment, a different host — where no worker the pool holds
    /// was started the way the next run needs.
    pub fn clear(&self) {
        if let Ok(mut idle) = self.idle.lock() {
            idle.clear();
        }
    }
}
