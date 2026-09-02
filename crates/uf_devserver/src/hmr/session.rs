//! One `uf dev` session: watch, rescan, invalidate, publish.
//!
//! Everything the other modules in [`crate::hmr`] do separately is sequenced
//! here, in the order the ordering matters:
//!
//! 1. read the file the watcher named — and treat a read that fails with
//!    "not found" as a delete, because a file *can* disappear between the
//!    change event and the read, and that is an ordinary Tuesday rather than a
//!    crash;
//! 2. rescan exactly that file into the graph, which relinks the modules that
//!    were waiting for it;
//! 3. invalidate;
//! 4. turn the invalidation into a payload, refusing to name a module the
//!    request pipeline could not serve;
//! 5. publish.
//!
//! The whole sequence is timed with [`std::time::Instant`], never the wall
//! clock, so a clock adjustment mid-session cannot produce a negative duration.
#![expect(
    clippy::disallowed_types,
    reason = "the session shares one update channel with the connection threads; see crate::hmr::channel"
)]

use std::sync::Arc;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;

use super::channel::UpdateChannel;
use super::graph::{DevGraph, DevModuleId, GraphError, module_path};
use super::invalidate::{ChangeKind, Invalidation, ReloadReason, UpdateKind, invalidate};
use super::update::{HmrUpdate, MAX_UPDATE_MODULES, UpdateModule, UpdateRole, update_target};
use super::watch::FileChange;

/// The graph, the channel, and the rule that connects them.
#[derive(Debug)]
pub struct HmrSession {
    root: Utf8PathBuf,
    graph: DevGraph,
    channel: Arc<UpdateChannel>,
}

impl HmrSession {
    /// A session over `root` publishing to `channel`.
    pub fn new(root: &Utf8Path, channel: Arc<UpdateChannel>) -> Self {
        Self {
            root: root.to_owned(),
            graph: DevGraph::new(),
            channel,
        }
    }

    /// The project root.
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The dev graph.
    pub fn graph(&self) -> &DevGraph {
        &self.graph
    }

    /// The update channel.
    pub fn channel(&self) -> &Arc<UpdateChannel> {
        &self.channel
    }

    /// Read and scan `relative` into the graph without publishing anything.
    ///
    /// Used to seed the graph at start-up. A file that cannot be read is
    /// skipped rather than failing the seed: a project with one unreadable file
    /// should still get hot module replacement for the rest.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] when the file breaks one of the graph's bounds.
    pub fn seed(&mut self, relative: &Utf8Path) -> Result<bool, GraphError> {
        let normalized = module_path(relative.as_str())?;
        let Some(source) = self.read(&normalized) else {
            return Ok(false);
        };
        self.graph.insert(normalized.as_str(), &source)?;
        Ok(true)
    }

    /// Apply one watched change and publish the update it produced.
    ///
    /// Returns the published update, including an inert one: a client that
    /// wants to say "nothing to do" needs to be told, and a channel that drops
    /// events is a channel nobody can debug.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] when the changed file breaks one of the graph's
    /// bounds. The graph is left as it was; the change is simply not applied.
    pub fn apply(&mut self, change: &FileChange) -> Result<HmrUpdate, GraphError> {
        let started = Instant::now();
        let path = module_path(change.path.as_str())?;
        let (id, kind) = self.record(&path)?;
        let invalidation = match id {
            Some(id) => invalidate(&self.graph, id, kind),
            None => Invalidation::default(),
        };
        let mut update = self.payload(&path, kind, &invalidation, started);
        update.id = self.channel.publish(update.clone());
        Ok(update)
    }

    /// Bring the graph in line with one change, and say what really happened.
    ///
    /// The watcher's opinion is not final: a `Modified` event for a file that
    /// has since been removed is recorded as a delete, and a `Deleted` event
    /// for a file that is back is recorded as a write. The filesystem at read
    /// time is the truth, exactly as the canonical path is the truth in
    /// [`crate::resolve`].
    ///
    /// `relative` has already been through
    /// [`module_path`](crate::hmr::graph::module_path), so nothing that climbs
    /// out of the project ever reaches the join in [`Self::read`].
    fn record(
        &mut self,
        relative: &Utf8Path,
    ) -> Result<(Option<DevModuleId>, ChangeKind), GraphError> {
        let path = relative.as_str();
        match self.read(relative) {
            Some(source) => {
                let insertion = self.graph.insert(path, &source)?;
                let kind = if insertion.is_new() {
                    ChangeKind::Created
                } else {
                    ChangeKind::Modified
                };
                Ok((Some(insertion.id()), kind))
            }
            None => Ok((self.graph.remove(path), ChangeKind::Deleted)),
        }
    }

    /// Read a project file, mapping every failure to "there is nothing here".
    ///
    /// A file can vanish, become a directory, lose its permissions or turn out
    /// not to be UTF-8 between the change event and this read. None of those is
    /// exceptional for a dev server watching a directory a human is editing,
    /// and none of them may panic: a source the scanner cannot read is a source
    /// the graph must forget, which is exactly what a delete does.
    fn read(&self, relative: &Utf8Path) -> Option<String> {
        std::fs::read_to_string(self.root.join(relative)).ok()
    }

    /// Turn an invalidation into the payload the browser receives.
    fn payload(
        &self,
        path: &Utf8Path,
        change: ChangeKind,
        invalidation: &Invalidation,
        started: Instant,
    ) -> HmrUpdate {
        let mut kind = invalidation.kind();
        let mut reason = invalidation.reload_reason();
        let mut modules = Vec::with_capacity(invalidation.client().len());

        if invalidation.client().len() > MAX_UPDATE_MODULES {
            kind = UpdateKind::FullReload;
            reason = Some(ReloadReason::TooManyModules);
        } else {
            for id in invalidation.client() {
                let Some(module) = self.graph.module(*id) else {
                    continue;
                };
                let Some(url) = update_target(module.path(), module.revision()) else {
                    // A module the request pipeline could not serve is never
                    // named in a payload. The browser reloads instead, and is
                    // told why.
                    kind = UpdateKind::FullReload;
                    reason = Some(ReloadReason::Unservable);
                    modules.clear();
                    break;
                };
                let role = if invalidation.boundaries().binary_search(id).is_ok() {
                    UpdateRole::Boundary
                } else {
                    UpdateRole::Dependency
                };
                modules.push(UpdateModule {
                    path: CompactString::new(module.path().as_str()),
                    url,
                    role,
                });
            }
        }

        // Dependencies first, so a client applying the list in order evaluates
        // what a boundary imports before it re-renders the boundary.
        modules.sort_by(|left, right| {
            left.role
                .apply_order()
                .cmp(&right.role.apply_order())
                .then_with(|| left.path.cmp(&right.path))
        });
        if kind.is_full_reload() {
            modules.clear();
        }

        let mut routes: Vec<CompactString> = invalidation
            .server()
            .iter()
            .filter_map(|id| self.graph.module(*id))
            .map(|module| CompactString::new(module.path().as_str()))
            .collect();
        // Sorted by path rather than by graph identifier: the payload and the
        // terminal line read the same however the project happened to be
        // scanned.
        routes.sort_unstable();

        HmrUpdate {
            id: 0,
            path: CompactString::new(path.as_str()),
            change,
            kind,
            reason,
            modules,
            routes,
            elapsed_micros: u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests;
