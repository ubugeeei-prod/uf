//! The dev module graph: incremental, and shaped for invalidation.
//!
//! # Why a second graph exists
//!
//! [`uf_rsc::RscGraph`] is the *build* graph. It is built once from a whole
//! project, answers reachability from entries, and produces diagnostics. A dev
//! server needs the opposite shape: one file changes every few seconds, and the
//! only question is *which modules above it went stale*. So this graph keeps
//! reverse edges, updates one node at a time, and never re-reads a file it was
//! not told about.
//!
//! What it deliberately shares with the build graph is the scanner. Import
//! specifiers, the directive prologue and the export surface all come out of
//! [`uf_rsc::RscModuleInput::from_source`], which tokenizes a module **once**
//! and answers all three questions from that single token vector. There is no
//! second lexer for this syntax in the workspace and there must not be one: two
//! lexers means two opinions about what `"use client"` inside a comment means.
//!
//! # Incrementality
//!
//! [`DevGraph::insert`] scans exactly the source it is handed. Relinking is
//! separate from scanning: when a module appears or disappears, the modules
//! that named it are re-*resolved* from the specifiers already stored on them,
//! never re-tokenized. A `waiting` table keyed by the module path an
//! unresolved specifier is looking for turns "who did this new file just
//! satisfy?" into one hash lookup rather than a sweep.
//!
//! # Bounds
//!
//! Rule 4 of `docs/security.md`: every container has a ceiling and a typed
//! error above it. A dev server watches a directory an attacker may have
//! written — a cloned repository — so [`MAX_MODULES`], [`MAX_MODULE_DEPTH`],
//! [`MAX_MODULE_IMPORTS`] and [`MAX_MODULE_BYTES`] are all enforced at
//! [`DevGraph::insert`], and each has its own [`GraphError`] variant.

use camino::{Utf8Path, Utf8PathBuf};
use smallvec::SmallVec;
use thiserror::Error;
use uf_infra::FxHashMap;
use uf_rsc::{RscModuleInput, scan::ImportList};

mod module;
mod path;

pub use module::{DevModule, DevModuleId, ModuleState, ModuleSurface};
pub use path::module_path;

use module::classify;
use path::{candidate_paths, relative_candidate};

/// Most modules one dev graph will track.
///
/// Counts slots, not live files: a module that is deleted keeps its slot so the
/// identifiers handed out earlier stay valid.
pub const MAX_MODULES: usize = 65_536;

/// Most path segments a module path may have.
pub const MAX_MODULE_DEPTH: usize = 32;

/// Most import specifiers one module may name.
pub const MAX_MODULE_IMPORTS: usize = 512;

/// Largest module source the graph will scan, in bytes.
///
/// The same ceiling [`uf_rsc`] applies, restated here so the dev server refuses
/// the file with its own typed error instead of silently scanning nothing.
pub const MAX_MODULE_BYTES: usize = uf_rsc::MAX_SOURCE_BYTES;

/// Why a module could not be added to the graph.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GraphError {
    /// The graph already holds [`MAX_MODULES`] slots.
    #[error("the dev graph is full at {MAX_MODULES} modules; {path} does not fit")]
    TooManyModules {
        /// The module that did not fit.
        path: Utf8PathBuf,
    },
    /// The module path has more than [`MAX_MODULE_DEPTH`] segments.
    #[error("{path} is {depth} segments deep, over the {MAX_MODULE_DEPTH} segment limit")]
    TooDeep {
        /// The offending path.
        path: Utf8PathBuf,
        /// How deep it was.
        depth: usize,
    },
    /// The module names more than [`MAX_MODULE_IMPORTS`] specifiers.
    #[error("{path} names {count} imports, over the {MAX_MODULE_IMPORTS} import limit")]
    TooManyImports {
        /// The offending module.
        path: Utf8PathBuf,
        /// How many specifiers it named.
        count: usize,
    },
    /// The source is larger than [`MAX_MODULE_BYTES`].
    #[error("{path} is {len} bytes, over the {MAX_MODULE_BYTES} byte limit")]
    SourceTooLarge {
        /// The offending module.
        path: Utf8PathBuf,
        /// How large it was.
        len: usize,
    },
    /// The path is absolute, escapes the project root, or is not a path.
    #[error("{path} is not a project-relative module path")]
    NotProjectRelative {
        /// The rejected path, exactly as it was supplied.
        path: Utf8PathBuf,
    },
}

/// What [`DevGraph::insert`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insertion {
    /// The path had no module before this call.
    Created(DevModuleId),
    /// The path already held a scanned module, and it was replaced.
    Updated(DevModuleId),
}

impl Insertion {
    /// The module that was written.
    pub fn id(self) -> DevModuleId {
        match self {
            Self::Created(id) | Self::Updated(id) => id,
        }
    }

    /// Whether the module is new to the graph.
    pub fn is_new(self) -> bool {
        matches!(self, Self::Created(_))
    }
}

/// The incremental import graph behind `uf dev`.
#[derive(Debug, Clone, Default)]
pub struct DevGraph {
    modules: Vec<DevModule>,
    index: FxHashMap<Utf8PathBuf, DevModuleId>,
    waiting: FxHashMap<Utf8PathBuf, SmallVec<[DevModuleId; 2]>>,
}

impl DevGraph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many slots the graph holds, present and absent together.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Whether the graph holds no slots at all.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// How many slots hold a file that exists.
    pub fn present_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| module.state == ModuleState::Present)
            .count()
    }

    /// The identifier of an already-normalized `path`, if the graph knows it.
    pub fn id(&self, path: &Utf8Path) -> Option<DevModuleId> {
        self.index.get(path).copied()
    }

    /// The identifier of `path`, normalizing it first.
    ///
    /// Returns `None` for a path the graph would have refused, so a lookup can
    /// never be the thing that admits an unnormalized path.
    pub fn find(&self, path: &str) -> Option<DevModuleId> {
        self.id(&module_path(path).ok()?)
    }

    /// The module behind an identifier.
    pub fn module(&self, id: DevModuleId) -> Option<&DevModule> {
        self.modules.get(id.index())
    }

    /// Every module the graph holds, in identifier order.
    pub fn modules(&self) -> &[DevModule] {
        &self.modules
    }

    /// Scan `source` as the module at `path`, replacing whatever was there.
    ///
    /// This is the only entry point that tokenizes, and it tokenizes exactly
    /// the source it is handed: a project of five thousand modules costs one
    /// scan per changed file, not five thousand.
    ///
    /// A module whose export list is empty is recorded as
    /// [`ModuleSurface::Erased`]. That is exact for a module of `export type`
    /// declarations, which is the case that matters, and it deliberately
    /// treats a module with *no* exports the same way: such a module cannot
    /// change what any importer sees, because there is nothing to import from
    /// it. A module kept purely for a top-level side effect is therefore
    /// reported as inert, and re-running a side effect is what a full reload is
    /// for in any case.
    ///
    /// # Errors
    ///
    /// Returns [`GraphError`] when the path is not project-relative or when any
    /// of the graph's bounds would be exceeded.
    pub fn insert(&mut self, path: &str, source: &str) -> Result<Insertion, GraphError> {
        let normalized = module_path(path)?;
        if source.len() > MAX_MODULE_BYTES {
            return Err(GraphError::SourceTooLarge {
                path: normalized,
                len: source.len(),
            });
        }

        let scanned = RscModuleInput::from_source(normalized.clone(), source);
        if scanned.imports.len() > MAX_MODULE_IMPORTS {
            return Err(GraphError::TooManyImports {
                path: normalized,
                count: scanned.imports.len(),
            });
        }

        let existed = self
            .id(&normalized)
            .is_some_and(|id| self.modules[id.index()].state == ModuleState::Present);
        let id = self.slot_for(&normalized)?;

        self.detach(id);
        {
            let module = &mut self.modules[id.index()];
            module.state = ModuleState::Present;
            module.environment = scanned.environment;
            module.surface = classify(&normalized, &scanned.exports);
            module.specifiers = scanned.imports;
            module.revision = module.revision.wrapping_add(1);
        }
        self.link(id);
        self.relink_waiters(&normalized);

        Ok(if existed {
            Insertion::Updated(id)
        } else {
            Insertion::Created(id)
        })
    }

    /// Record that the file at `path` is gone.
    ///
    /// Returns the slot that was marked absent, or `None` when the graph never
    /// knew the path. Removing a module the graph does not have is not an
    /// error: a delete event can arrive for a file the dev server never scanned.
    pub fn remove(&mut self, path: &str) -> Option<DevModuleId> {
        let normalized = module_path(path).ok()?;
        let id = self.id(&normalized)?;
        if self.modules[id.index()].state == ModuleState::Absent {
            return Some(id);
        }

        self.detach(id);
        let module = &mut self.modules[id.index()];
        module.state = ModuleState::Absent;
        module.specifiers = ImportList::new();
        module.revision = module.revision.wrapping_add(1);
        // The environment and the export surface are deliberately kept. The
        // browser still holds the module that was there a moment ago, and
        // invalidation has to reason about *that* module: whether it was a
        // `"use client"` boundary decides whether its deletion reaches the
        // client graph or the route.
        //
        // The incoming edges are left alone for the same reason. An importer
        // still *names* the file that just vanished, and the invalidation walk
        // has to be able to climb to it; dropping the edge here would make a
        // delete look like a change nobody depends on.
        Some(id)
    }

    /// Find or create the slot for `path`.
    fn slot_for(&mut self, path: &Utf8Path) -> Result<DevModuleId, GraphError> {
        if let Some(id) = self.index.get(path) {
            return Ok(*id);
        }
        if self.modules.len() >= MAX_MODULES {
            return Err(GraphError::TooManyModules {
                path: path.to_owned(),
            });
        }
        let id = DevModuleId(self.modules.len() as u32);
        self.modules.push(DevModule::vacant(path.to_owned()));
        self.index.insert(path.to_owned(), id);
        Ok(id)
    }

    /// Drop every outgoing edge and every `waiting` registration of `id`.
    fn detach(&mut self, id: DevModuleId) {
        let imports = std::mem::take(&mut self.modules[id.index()].imports);
        for target in imports {
            let importers = &mut self.modules[target.index()].importers;
            if let Ok(position) = importers.binary_search(&id) {
                importers.remove(position);
            }
        }
        let waiting_on = std::mem::take(&mut self.modules[id.index()].waiting_on);
        for key in waiting_on {
            if let Some(entry) = self.waiting.get_mut(&key) {
                entry.retain(|waiting| *waiting != id);
                if entry.is_empty() {
                    self.waiting.remove(&key);
                }
            }
        }
    }

    /// Resolve `id`'s stored specifiers into edges, registering the ones that
    /// resolve to nothing so a later insert can link them in one lookup.
    fn link(&mut self, id: DevModuleId) {
        let specifiers = std::mem::take(&mut self.modules[id.index()].specifiers);
        let mut imports: SmallVec<[DevModuleId; 8]> = SmallVec::new();
        let mut waiting_on: SmallVec<[Utf8PathBuf; 4]> = SmallVec::new();

        for specifier in &specifiers {
            let Some(candidate) = relative_candidate(&self.modules[id.index()].path, specifier)
            else {
                continue;
            };
            let target = self.resolve_candidate(&candidate);
            // A self-import is dropped rather than recorded: it is an edge no
            // invalidation can act on, and keeping it would make every walk
            // special-case it.
            if let Some(target) = target
                && target != id
                && let Err(position) = imports.binary_search(&target)
            {
                imports.insert(position, target);
            }
            // Nothing on disk answers this specifier yet, or only an absent
            // slot does. Either way a file arriving at one of the candidate
            // paths changes the answer, so the module is registered against all
            // of them and a later insert relinks it with one hash lookup.
            let settled =
                target.is_some_and(|id| self.modules[id.index()].state == ModuleState::Present);
            if settled {
                continue;
            }
            for key in candidate_paths(&candidate) {
                if waiting_on.contains(&key) {
                    continue;
                }
                self.waiting.entry(key.clone()).or_default().push(id);
                waiting_on.push(key);
            }
        }

        for target in &imports {
            let importers = &mut self.modules[target.index()].importers;
            if let Err(position) = importers.binary_search(&id) {
                importers.insert(position, id);
            }
        }

        let module = &mut self.modules[id.index()];
        module.specifiers = specifiers;
        module.imports = imports;
        module.waiting_on = waiting_on;
    }

    /// Re-resolve every module that was waiting for a module at `path`.
    fn relink_waiters(&mut self, path: &Utf8Path) {
        let Some(waiters) = self.waiting.remove(path) else {
            return;
        };
        for waiter in waiters {
            self.detach(waiter);
            self.link(waiter);
        }
    }

    /// Resolve a candidate through the `.js` and `/index.js` fallbacks.
    ///
    /// A file that exists wins over one that does not, in candidate order. A
    /// slot whose file was deleted is still returned when nothing exists,
    /// because the edge to it is what lets an invalidation reach the importers
    /// of a file that has just disappeared.
    fn resolve_candidate(&self, candidate: &Utf8Path) -> Option<DevModuleId> {
        let mut absent = None;
        for path in candidate_paths(candidate) {
            let Some(id) = self.index.get(&path).copied() else {
                continue;
            };
            if self.modules[id.index()].state == ModuleState::Present {
                return Some(id);
            }
            absent.get_or_insert(id);
        }
        absent
    }
}

#[cfg(test)]
mod tests;
