//! Tree shaking: which exports anything uses, and which modules survive.
//!
//! Two questions, answered by one fixed point:
//!
//! 1. **Which exports are used?** A module starts with an empty used-set and
//!    grows it as importers are processed. Growing a set puts the module back
//!    on the worklist, because a name that becomes used can pull a name through
//!    a re-export chain behind it.
//! 2. **Which modules are live?** A module is live when something needs it: a
//!    used export, a top-level side effect, or a bare `import "…"` that exists
//!    only for those side effects. A module whose package declares
//!    `"sideEffects": false` gives up the last of those, which is exactly what
//!    the field promises.
//!
//! The walk is an explicit worklist and every step only ever adds to a set, so
//! it terminates on any graph, including a cyclic one.

use compact_str::CompactString;
use uf_infra::{FxHashSet, InlineVec};

use crate::graph::{Edge, ModuleGraph, ModuleIndex};
use crate::record::{ExportSource, ImportBinding, SideEffectKind};

/// Which of a module's exports anything imports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UsedExports {
    /// Nothing yet.
    #[default]
    None,
    /// A specific set of names.
    Named(FxHashSet<CompactString>),
    /// Every export: a namespace import, a `export * from`, or an entry point.
    All,
}

impl UsedExports {
    /// Whether `name` is used.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        match self {
            Self::None => false,
            Self::Named(names) => names.contains(name),
            Self::All => true,
        }
    }

    /// Whether anything at all is used.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::None => true,
            Self::Named(names) => names.is_empty(),
            Self::All => false,
        }
    }

    /// Add one name, reporting whether the set grew.
    fn insert(&mut self, name: &str) -> bool {
        match self {
            Self::All => false,
            Self::None => {
                let mut names = FxHashSet::default();
                names.insert(CompactString::new(name));
                *self = Self::Named(names);
                true
            }
            Self::Named(names) => names.insert(CompactString::new(name)),
        }
    }

    /// Widen to every export, reporting whether the set grew.
    fn widen(&mut self) -> bool {
        if matches!(self, Self::All) {
            return false;
        }
        *self = Self::All;
        true
    }
}

/// What tree shaking decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shaken {
    live: Vec<bool>,
    used: Vec<UsedExports>,
}

impl Shaken {
    /// Whether a module survives into a chunk.
    #[must_use]
    pub fn is_live(&self, index: ModuleIndex) -> bool {
        self.live.get(index.get()).copied().unwrap_or(false)
    }

    /// Which of a module's exports anything imports.
    #[must_use]
    pub fn used(&self, index: ModuleIndex) -> &UsedExports {
        &self.used[index.get()]
    }

    /// How many modules survived.
    #[must_use]
    pub fn live_count(&self) -> usize {
        self.live.iter().filter(|live| **live).count()
    }

    /// The modules that survived, in graph order.
    #[must_use]
    pub fn live_modules(&self) -> Vec<ModuleIndex> {
        (0..self.live.len())
            .map(ModuleIndex::from_position)
            .filter(|index| self.is_live(*index))
            .collect()
    }
}

/// Run tree shaking over a graph.
///
/// Entry modules and client-bundle roots keep every export: they are the
/// public surface of the build, and nothing inside it can say which of their
/// names an HTML page or a server renderer will reach for.
#[must_use]
pub fn shake(graph: &ModuleGraph) -> Shaken {
    let count = graph.modules().len();
    let mut used = vec![UsedExports::None; count];
    let mut live = vec![false; count];
    let mut worklist: Vec<ModuleIndex> = Vec::with_capacity(count);

    let mut roots: InlineVec<ModuleIndex, 16> = InlineVec::new();
    roots.extend(graph.entries().iter().copied());
    roots.extend(graph.client_roots());
    for root in roots {
        used[root.get()].widen();
        if !std::mem::replace(&mut live[root.get()], true) {
            worklist.push(root);
        }
    }

    while let Some(index) = worklist.pop() {
        let module = graph.module(index);
        for (position, import) in module.record.imports.iter().enumerate() {
            if !import.form.is_linked() {
                continue;
            }
            let Some(Edge::Module(target)) = module.edge(position) else {
                continue;
            };
            let target = *target;
            let mut needed = false;

            for binding in &import.bindings {
                needed = true;
                let grew = match binding {
                    ImportBinding::Namespace { .. } => used[target.get()].widen(),
                    ImportBinding::Default { .. } => used[target.get()].insert("default"),
                    ImportBinding::Named { imported, .. } => {
                        used[target.get()].insert(imported.as_str())
                    }
                };
                if grew && live[target.get()] {
                    worklist.push(target);
                }
            }

            // `export * from "…"` republishes names this module cannot see, so
            // the target has to keep all of them.
            if module.record.star_reexports.contains(&position) {
                needed = true;
                if used[target.get()].widen() && live[target.get()] {
                    worklist.push(target);
                }
            }

            for export in &module.record.exports {
                let ExportSource::Reexport { import, imported } = &export.source else {
                    continue;
                };
                if *import != position || !used[index.get()].contains(&export.exported) {
                    continue;
                }
                needed = true;
                let grew = if imported == "*" {
                    used[target.get()].widen()
                } else {
                    used[target.get()].insert(imported.as_str())
                };
                if grew && live[target.get()] {
                    worklist.push(target);
                }
            }

            let target_module = graph.module(target);
            // A bare `import "…"` exists only for the side effects, so it keeps
            // the module unless the package promised there are none.
            if import.bindings.is_empty() && !module.record.star_reexports.contains(&position) {
                needed |= !target_module.shakeable;
            }
            needed |= target_module.record.side_effects == SideEffectKind::Present
                && !target_module.shakeable;

            if needed && !std::mem::replace(&mut live[target.get()], true) {
                worklist.push(target);
            }
        }
    }

    Shaken { live, used }
}
