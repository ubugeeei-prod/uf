//! The React Server Components module graph.
//!
//! [`RscGraph`] answers the three questions `uf build` and `uf dev` need before
//! they can emit anything:
//!
//! 1. which environment does each module execute in;
//! 2. which modules are reachable from a server entry, from a client entry, or
//!    from both;
//! 3. where does the server graph hand off to the client — the *client
//!    boundaries*, whose targets become client-bundle roots.
//!
//! # Termination
//!
//! Import graphs contain cycles, and a graph walk that recurses on them either
//! overflows the stack or never finishes. Propagation here is an explicit
//! worklist over `(module, colour)` pairs with a seen-set per colour, so every
//! pair is processed at most once and the walk is `O(V + E)` on any input,
//! cyclic or not. There is no recursion anywhere in this module.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use uf_infra::{FxHashMap, InlineVec, LineIndex};

use crate::directive::{
    DirectiveIssueList, FunctionDirective, FunctionDirectiveList, FunctionOwner, ModuleEnvironment,
    scan_directive_tokens,
};
use crate::scan::{
    ClientApiUseList, ExportKind, ExportList, ImportKind, ImportList, ImportSpecifier,
    ModuleExport, client_api_uses_from_tokens, exports_from_tokens, imports_from_tokens, tokenize,
};

mod build;
mod diagnostic;
mod report;
mod resolve;

pub use build::RscGraphBuilder;
pub use diagnostic::{RscDiagnostic, RscSeverity};
pub use resolve::{
    SpecifierResolution, is_inside_project, is_server_only_specifier, normalize_module_path,
    resolve_specifier,
};

/// Packages whose code must never reach the browser.
///
/// Importing one of these from the client graph is the "server code leaked into
/// the client bundle" class: database handles, secrets and privileged helpers
/// end up served to every visitor. Sorted for binary search.
pub const SERVER_ONLY_PACKAGES: &[&str] = &["@uniflowed/db", "@uniflowed/server", "server-only"];

/// Suffix marking a module as server-only by file name.
pub const SERVER_ONLY_SUFFIX: &str = ".server.js";

/// Identifier of a module inside one [`RscGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleId(u32);

impl ModuleId {
    /// Index of the module in [`RscGraph::modules`].
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Why a module is an entry point of the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryKind {
    /// Rendered by the server: a page, a layout, middleware.
    Server,
    /// Loaded by the browser: a client bundle entry.
    Client,
}

/// Which halves of the app a module is reachable from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleReachability {
    /// No entry reaches this module; it is dead code.
    #[default]
    Unreachable,
    /// Reachable only while rendering on the server.
    ServerOnly,
    /// Reachable only from the client bundle.
    ClientOnly,
    /// Reachable from both halves; the module is shared code.
    ServerAndClient,
}

impl ModuleReachability {
    /// Build a reachability from the two propagation colours.
    pub fn from_colours(server: bool, client: bool) -> Self {
        match (server, client) {
            (false, false) => Self::Unreachable,
            (true, false) => Self::ServerOnly,
            (false, true) => Self::ClientOnly,
            (true, true) => Self::ServerAndClient,
        }
    }

    /// Whether a server entry reaches this module.
    pub fn is_server_reachable(self) -> bool {
        matches!(self, Self::ServerOnly | Self::ServerAndClient)
    }

    /// Whether a client entry or a client boundary reaches this module.
    pub fn is_client_reachable(self) -> bool {
        matches!(self, Self::ClientOnly | Self::ServerAndClient)
    }

    /// Whether any entry reaches this module.
    pub fn is_reachable(self) -> bool {
        !matches!(self, Self::Unreachable)
    }

    /// Stable identifier used in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::ServerOnly => "server-only",
            Self::ClientOnly => "client-only",
            Self::ServerAndClient => "server-and-client",
        }
    }
}

/// Whether a module can hand a server action to the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ClientBoundaryProximity {
    /// Neither this module nor anything it imports crosses a client boundary.
    #[default]
    Isolated,
    /// This module, or a module it transitively imports, imports a `"use client"`
    /// module, so a closure defined here can be passed across the boundary.
    ReachesBoundary,
}

impl ClientBoundaryProximity {
    /// Whether the module reaches a client boundary.
    pub fn reaches_boundary(self) -> bool {
        matches!(self, Self::ReachesBoundary)
    }
}

/// A server module importing a `"use client"` module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientBoundary {
    /// The server module that owns the import.
    pub importer: ModuleId,
    /// The `"use client"` module, which becomes a client bundle root.
    pub client_module: ModuleId,
}

/// One module as it is fed into [`RscGraphBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RscModuleInput {
    /// Path relative to the project root, with forward slashes.
    pub path: Utf8PathBuf,
    /// Environment the module executes in.
    pub environment: ModuleEnvironment,
    /// Import specifiers exactly as written.
    pub imports: ImportList,
    /// Exported bindings.
    pub exports: ExportList,
    /// Function-level `"use server"` closures.
    pub function_actions: FunctionDirectiveList,
    /// Client-only APIs the module reaches for.
    pub client_api_uses: ClientApiUseList,
    /// Rejected directives found while scanning the module.
    pub directive_issues: DirectiveIssueList,
}

impl RscModuleInput {
    /// An empty module at `path` with an explicit environment.
    pub fn new(path: impl Into<Utf8PathBuf>, environment: ModuleEnvironment) -> Self {
        Self {
            path: normalize_module_path(&path.into()),
            environment,
            imports: ImportList::new(),
            exports: ExportList::new(),
            function_actions: FunctionDirectiveList::new(),
            client_api_uses: ClientApiUseList::new(),
            directive_issues: DirectiveIssueList::new(),
        }
    }

    /// Scan a module from its source text, in one pass over the tokens.
    pub fn from_source(path: impl Into<Utf8PathBuf>, source: &str) -> Self {
        let tokens = tokenize(source);
        let index = LineIndex::new(source);
        let directives = scan_directive_tokens(source, &tokens, &index);

        Self {
            path: normalize_module_path(&path.into()),
            environment: directives.environment,
            imports: imports_from_tokens(source, &tokens, &index),
            exports: exports_from_tokens(source, &tokens, &index),
            function_actions: directives.function_directives,
            client_api_uses: client_api_uses_from_tokens(source, &tokens, &index),
            directive_issues: directives.issues,
        }
    }

    /// Add a static import specifier.
    pub fn with_import(mut self, specifier: impl Into<CompactString>) -> Self {
        self.imports.push(ImportSpecifier {
            specifier: specifier.into(),
            kind: ImportKind::Static,
            line: 1,
        });
        self
    }

    /// Add an export.
    pub fn with_export(mut self, name: impl Into<CompactString>, kind: ExportKind) -> Self {
        self.exports.push(ModuleExport {
            name: name.into(),
            kind,
            line: 1,
        });
        self
    }

    /// Add a function-level `"use server"` closure.
    pub fn with_function_action(mut self, owner: FunctionOwner) -> Self {
        self.function_actions.push(FunctionDirective {
            owner,
            line: 1,
            column: 1,
        });
        self
    }
}

/// A module after the graph has been resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RscModule {
    /// Path relative to the project root.
    pub path: Utf8PathBuf,
    /// Environment the module executes in.
    pub environment: ModuleEnvironment,
    /// Which halves of the app reach this module.
    pub reachability: ModuleReachability,
    /// Whether a closure defined here can cross into the client.
    pub proximity: ClientBoundaryProximity,
    /// Modules imported from this one, sorted and deduplicated.
    pub imports: InlineVec<ModuleId, 8>,
    /// Import specifiers that do not resolve to a module of this graph.
    pub external_imports: InlineVec<CompactString, 4>,
    /// Exported bindings.
    pub exports: ExportList,
    /// Function-level `"use server"` closures.
    pub function_actions: FunctionDirectiveList,
}

/// The resolved React Server Components graph of a project.
#[derive(Debug, Clone)]
pub struct RscGraph {
    modules: Vec<RscModule>,
    index: FxHashMap<Utf8PathBuf, ModuleId>,
    boundaries: Vec<ClientBoundary>,
    bundle_roots: Vec<ModuleId>,
    diagnostics: Vec<RscDiagnostic>,
}

impl RscGraph {
    /// Every module, ordered by path.
    pub fn modules(&self) -> &[RscModule] {
        &self.modules
    }

    /// Look up a module by its project-relative path.
    pub fn module(&self, path: impl AsRef<str>) -> Option<&RscModule> {
        let path = normalize_module_path(Utf8Path::new(path.as_ref()));
        self.index.get(&path).map(|id| &self.modules[id.index()])
    }

    /// Look up a module by id.
    pub fn module_by_id(&self, id: ModuleId) -> Option<&RscModule> {
        self.modules.get(id.index())
    }

    /// Server-to-client import edges, ordered.
    pub fn client_boundaries(&self) -> &[ClientBoundary] {
        &self.boundaries
    }

    /// Client bundle roots, ordered.
    pub fn client_bundle_roots(&self) -> &[ModuleId] {
        &self.bundle_roots
    }

    /// Contract violations found while building the graph, ordered.
    pub fn diagnostics(&self) -> &[RscDiagnostic] {
        &self.diagnostics
    }

    /// Whether any diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == RscSeverity::Error)
    }
}

#[cfg(test)]
mod tests;
