//! One node of the dev graph, and the classification that decides what a
//! change to it can do.
//!
//! [`ModuleSurface`] is the whole of the Fast Refresh decision, reduced to
//! three states so that "can this be swapped in place?" is answered by a match
//! rather than by re-reading the exports at every invalidation.

use camino::{Utf8Path, Utf8PathBuf};
use smallvec::SmallVec;
use uf_rsc::{ExportKind, ModuleEnvironment, ModuleExport, scan::ImportList};

/// Identifier of a module inside one [`DevGraph`](super::DevGraph).
///
/// Stable for the life of the graph: deleting a file marks its slot absent
/// rather than removing it, so an identifier handed to an invalidation walk
/// never starts pointing at a different module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DevModuleId(pub(super) u32);

impl DevModuleId {
    /// Index of the module in the graph's slot table.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// What a module contributes to the running program.
///
/// The three states are what an invalidation needs to know, and nothing else:
/// whether anything observes the module at runtime, and if so whether React
/// Fast Refresh can swap it without losing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ModuleSurface {
    /// Every export is a `type`, `interface`, `opaque type`, `enum` or
    /// `declare` — all erased before a browser sees the module — so no importer
    /// can observe a change to it at runtime.
    ///
    /// A module that exports nothing at all lands here too. The cost of that is
    /// documented at [`DevGraph::insert`](super::DevGraph::insert).
    #[default]
    Erased,
    /// Every runtime export is a React component, so the module is a Fast
    /// Refresh boundary: it can be re-evaluated and re-rendered in place.
    Component,
    /// At least one runtime export is not a component — a hook, a constant, a
    /// factory — so re-evaluating the module would hand existing callers a
    /// different binding. The update has to propagate to importers.
    Opaque,
}

impl ModuleSurface {
    /// Whether anything observes this module at runtime.
    pub fn is_observable(self) -> bool {
        !matches!(self, Self::Erased)
    }

    /// Whether the module can accept a hot update in place.
    pub fn accepts_update(self) -> bool {
        matches!(self, Self::Component)
    }

    /// Stable kebab-case name, for payloads and terminal output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Erased => "erased",
            Self::Component => "component",
            Self::Opaque => "opaque",
        }
    }
}

/// Whether a slot holds a file that exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleState {
    /// The file was scanned and is on disk.
    Present,
    /// The file was deleted, or was only ever named by an importer.
    ///
    /// The slot is kept so importer edges and identifiers survive, which is
    /// what makes "a file deleted between the change event and the read" an
    /// ordinary state transition rather than a special case.
    Absent,
}

/// One module of the dev graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevModule {
    pub(super) path: Utf8PathBuf,
    pub(super) state: ModuleState,
    pub(super) environment: ModuleEnvironment,
    pub(super) surface: ModuleSurface,
    pub(super) specifiers: ImportList,
    pub(super) imports: SmallVec<[DevModuleId; 8]>,
    pub(super) importers: SmallVec<[DevModuleId; 4]>,
    pub(super) waiting_on: SmallVec<[Utf8PathBuf; 4]>,
    pub(super) revision: u32,
}

impl DevModule {
    /// A slot for a path nothing has been scanned into yet.
    pub(super) fn vacant(path: Utf8PathBuf) -> Self {
        Self {
            path,
            state: ModuleState::Absent,
            environment: ModuleEnvironment::default(),
            surface: ModuleSurface::Erased,
            specifiers: ImportList::new(),
            imports: SmallVec::new(),
            importers: SmallVec::new(),
            waiting_on: SmallVec::new(),
            revision: 0,
        }
    }

    /// The project-relative path, with forward slashes.
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Whether the file exists.
    pub fn state(&self) -> ModuleState {
        self.state
    }

    /// The RSC environment the module runs in.
    pub fn environment(&self) -> ModuleEnvironment {
        self.environment
    }

    /// What the module contributes at runtime.
    pub fn surface(&self) -> ModuleSurface {
        self.surface
    }

    /// Modules this one imports, sorted and deduplicated.
    pub fn imports(&self) -> &[DevModuleId] {
        &self.imports
    }

    /// Modules that import this one, sorted and deduplicated.
    pub fn importers(&self) -> &[DevModuleId] {
        &self.importers
    }

    /// How many times this module has been scanned.
    ///
    /// Doubles as the cache-busting token on the URL the browser re-fetches.
    pub fn revision(&self) -> u32 {
        self.revision
    }
}

/// Decide what a module's export list lets a hot update do.
///
/// The component test is `ExportKind` plus a PascalCase name, and a `default`
/// export is judged by the file stem because the scanner records the binding as
/// `default` rather than by its identifier. The bias is deliberate: a
/// misclassified component costs a page reload, while a misclassified helper
/// would leave a stale binding in place, and `docs/security.md`'s rule about
/// which way to be wrong applies to correctness too.
pub(super) fn classify(path: &Utf8Path, exports: &[ModuleExport]) -> ModuleSurface {
    if exports.is_empty() {
        return ModuleSurface::Erased;
    }
    if exports
        .iter()
        .all(|export| is_component_export(path, export))
    {
        ModuleSurface::Component
    } else {
        ModuleSurface::Opaque
    }
}

fn is_component_export(path: &Utf8Path, export: &ModuleExport) -> bool {
    if !matches!(export.kind, ExportKind::SyncFunction | ExportKind::Class) {
        return false;
    }
    let name = if export.name == "default" {
        path.file_stem().unwrap_or_default()
    } else {
        export.name.as_str()
    };
    name.starts_with(|first: char| first.is_uppercase())
}
