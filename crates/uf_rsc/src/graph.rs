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

use std::sync::OnceLock;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use uf_infra::{FxHashMap, FxHashSet, InlineVec, LineIndex};

use crate::directive::{
    DirectiveIssueList, FunctionDirective, FunctionDirectiveList, FunctionOwner, ModuleEnvironment,
    scan_directive_tokens,
};
use crate::scan::{
    ClientApiUseList, ExportKind, ExportList, HookCallList, ImportBindingList, ImportKind,
    ImportList, ImportSpecifier, ModuleExport, client_api_uses_from_tokens, exports_from_tokens,
    hook_calls_from_tokens, imports_from_tokens, owner_spans, tokenize,
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

/// The uf package whose hooks this crate can decide without reading a body.
///
/// [`SERVER_ONLY_PACKAGES`] for the other direction. That list says which
/// specifiers a *client* module must not import; this one is the head of the
/// answer to which names a *server* module must not call.
///
/// One package rather than a table of them, and the reason is what a declared
/// fact costs. `uf_lib`'s `hook_descriptors()` carries
/// `server_component_safe` for every hook `@uniflowed/hooks` exports, and
/// `the_hook_table_names_exactly_the_hooks_the_package_exports` holds that
/// list to the package it describes — so these 58 are checked rather than
/// asserted. The other ten packages that export hooks have no such table, and
/// writing one by hand is 36 more judgements whose failure mode is turning a
/// warning into an error in somebody's build. Those wait for the export-graph
/// fixpoint in ubugeeei-prod/uf#388, which can derive them; this is the half
/// that needs no new data at all.
pub const CLIENT_ONLY_HOOK_PACKAGE: &str = "@uniflowed/hooks";

/// The hooks of [`CLIENT_ONLY_HOOK_PACKAGE`] a Server Component may not call.
///
/// Read from the registry rather than copied into a list here, so there is one
/// place a hook's environment is written down and no second one to drift from
/// it. Built once: `hook_descriptors()` allocates a `Vec` of owned names, and
/// this is asked once per hook call in a server module.
fn client_only_hooks() -> &'static FxHashSet<CompactString> {
    static HOOKS: OnceLock<FxHashSet<CompactString>> = OnceLock::new();
    HOOKS.get_or_init(|| {
        uf_lib::hook_descriptors()
            .into_iter()
            .filter(|hook| !hook.server_component_safe)
            .map(|hook| hook.name)
            .collect()
    })
}

/// Whether `specifier` names [`CLIENT_ONLY_HOOK_PACKAGE`] or a subpath of it.
fn is_client_only_hook_package(specifier: &str) -> bool {
    specifier == CLIENT_ONLY_HOOK_PACKAGE
        || specifier
            .strip_prefix(CLIENT_ONLY_HOOK_PACKAGE)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The package a called hook came from, when this crate can say.
///
/// Attributed through the module's imports rather than through the binding the
/// import introduced. That makes this answer wrong in exactly one shape: a
/// module that imports `@uniflowed/hooks` *and* separately defines or imports
/// its own `useMediaQuery`, and calls that one. It is the same imprecision the
/// rest of this scanner already has — it matches identifiers against name
/// lists — and it is narrower than the alternative of saying nothing, which is
/// what this did before.
///
/// [`ImportSpecifier::bindings`] now carries what would narrow it: the
/// `{ imported, local }` pairs say whether `useMediaQuery` is the name this
/// module bound from that package or a different function that shares its
/// spelling. Reading them here would change which calls are reported, so it
/// belongs with the rest of the classification work rather than with the
/// plumbing that made it possible — ubugeeei-prod/uf#388.
///
/// [`None`] is the honest answer and stays reported as one: a hook from a
/// package with no table, a hook the project wrote, a hook reached through a
/// value. See [`RscDiagnostic::UnclassifiedHookInServerModule`].
fn client_only_hook_package(hook: &str, imports: &[ImportSpecifier]) -> Option<&'static str> {
    if !client_only_hooks().contains(hook) {
        return None;
    }
    imports
        .iter()
        .any(|import| is_client_only_hook_package(&import.specifier))
        .then_some(CLIENT_ONLY_HOOK_PACKAGE)
}

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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
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

    /// Stable identifier used in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::ReachesBoundary => "reaches-boundary",
        }
    }
}

/// The far side of a server-to-client import.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClientBoundaryTarget {
    /// A project module the graph scanned.
    Module(ModuleId),
    /// A package module the graph knows without walking `node_modules`.
    Package(CompactString),
}

/// A server module importing a client module.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientBoundary {
    /// The server module that owns the import.
    pub importer: ModuleId,
    /// The client module, which becomes a client bundle root.
    pub target: ClientBoundaryTarget,
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
    /// Hook calls the client-only name lists cannot classify.
    pub hook_calls: HookCallList,
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
            hook_calls: HookCallList::new(),
            directive_issues: DirectiveIssueList::new(),
        }
    }

    /// Scan a module from its source text, in one pass over the tokens.
    pub fn from_source(path: impl Into<Utf8PathBuf>, source: &str) -> Self {
        let tokens = tokenize(source);
        let index = LineIndex::new(source);
        let directives = scan_directive_tokens(source, &tokens, &index);
        // One span table for both use-site collectors, for the same reason
        // there is one token vector for all five passes.
        let owners = owner_spans(source, &tokens);

        Self {
            path: normalize_module_path(&path.into()),
            environment: directives.environment,
            imports: imports_from_tokens(source, &tokens, &index),
            exports: exports_from_tokens(source, &tokens, &index),
            function_actions: directives.function_directives,
            client_api_uses: client_api_uses_from_tokens(source, &tokens, &index, &owners),
            hook_calls: hook_calls_from_tokens(source, &tokens, &index, &owners),
            directive_issues: directives.issues,
        }
    }

    /// Add a static import specifier.
    pub fn with_import(mut self, specifier: impl Into<CompactString>) -> Self {
        self.imports.push(ImportSpecifier {
            specifier: specifier.into(),
            kind: ImportKind::Static,
            line: 1,
            bindings: ImportBindingList::new(),
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

impl RscModule {
    /// Whether the browser has to be able to evaluate this module.
    ///
    /// True for a `"use client"` module, which is a client bundle root by
    /// definition, and for any module that transitively imports one.
    ///
    /// The second half is uf's client renderer talking rather than React's.
    /// `packages/router/client.js` hydrates by re-rendering the whole matched
    /// tree from the same modules the server rendered it from, so a Server
    /// Component *above* a client boundary is a module React needs in the
    /// browser to reach the boundary at all. Dropping it needs a Flight-shaped
    /// payload describing the rendered tree, which uf does not have yet; see
    /// ubugeeei-prod/uf#252.
    ///
    /// What this does decide is the module that reaches *no* boundary. Nothing
    /// under it is ever rendered in the browser, so nothing under it has to be
    /// shipped — which is what `@uniflowed/vite` uses to leave a whole route's
    /// page out of the client route table.
    pub fn requires_client_bundle(&self) -> bool {
        self.environment == ModuleEnvironment::Client || self.proximity.reaches_boundary()
    }
}

/// The resolved React Server Components graph of a project.
#[derive(Debug, Clone)]
pub struct RscGraph {
    modules: Vec<RscModule>,
    index: FxHashMap<Utf8PathBuf, ModuleId>,
    boundaries: Vec<ClientBoundary>,
    bundle_roots: Vec<ClientBoundaryTarget>,
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

    /// The id of the module at `path`, if the graph has one.
    ///
    /// [`Self::modules`] hands out modules and not ids, which is all a reader
    /// that only wants to *print* them needs. Anything that then asks the
    /// graph a second question about one — [`Self::client_bundle_reason`] is
    /// the first such question — needs the id, and deriving it from a position
    /// in `modules()` would be a caller relying on an ordering this type does
    /// not promise to keep.
    pub fn module_id(&self, path: impl AsRef<str>) -> Option<ModuleId> {
        let path = normalize_module_path(Utf8Path::new(path.as_ref()));
        self.index.get(&path).copied()
    }

    /// Server-to-client import edges, ordered.
    pub fn client_boundaries(&self) -> &[ClientBoundary] {
        &self.boundaries
    }

    /// Client bundle roots, ordered.
    pub fn client_bundle_roots(&self) -> &[ClientBoundaryTarget] {
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

    /// Why the browser has to be able to evaluate `id` — or why it does not.
    ///
    /// [`RscModule::requires_client_bundle`] answers the same question with a
    /// `bool`, which is what the bundler needs. This answers it with the chain
    /// of imports that decided it, which is what a *person* needs when the
    /// bundler's answer surprises them, and it is the whole of what makes
    /// "why is this in the client bundle" a question uf can answer from its
    /// own analysis rather than by reading a bundle back.
    ///
    /// # One decision, read two ways
    ///
    /// [`ClientBundleReason::Isolated`] is returned for exactly the modules
    /// `requires_client_bundle` answers `false` for, and never for one it
    /// answers `true` for — both read the same two fields, and
    /// `graph::tests::reachability` holds them against each other over every
    /// module of every graph it builds. An explanation that could disagree
    /// with the split would be a second analysis of the same question, which
    /// is the thing this crate exists in order not to have.
    ///
    /// # Why a chain always exists when one is claimed
    ///
    /// [`ClientBoundaryProximity::ReachesBoundary`] is propagated *backwards*
    /// from the importer side of every client boundary, so a module that has
    /// it is one from which some server-reachable module that imports a
    /// `"use client"` module is forward-reachable. The walk below is that path
    /// taken forwards, and it therefore cannot come up empty. Where it does
    /// the answer is [`ClientBundleReason::Isolated`] rather than a panic: a
    /// report is not worth stopping a build for, and the `bool` beside it is
    /// still the one the bundle was built from.
    ///
    /// # Termination
    ///
    /// Breadth-first with a seen-set and one predecessor per module, no
    /// recursion — the same shape, and for the same reason, as the propagation
    /// this module's header describes. Every module and every edge is visited
    /// at most once, so it is `O(V + E)` on a cyclic import graph as much as on
    /// an acyclic one, and the chain rebuilt from the predecessors is acyclic
    /// by construction because a module is given a predecessor only once.
    ///
    /// Breadth-first rather than depth-first for the reader rather than for the
    /// complexity: a module can reach a boundary many ways, and the one worth
    /// printing is the shortest, which is the one a person can hold in their
    /// head while deciding what to move.
    pub fn client_bundle_reason(&self, id: ModuleId) -> ClientBundleReason {
        let Some(module) = self.module_by_id(id) else {
            return ClientBundleReason::Isolated;
        };
        if module.environment == ModuleEnvironment::Client {
            return ClientBundleReason::Declared;
        }
        if !module.proximity.reaches_boundary() {
            return ClientBundleReason::Isolated;
        }

        // `u32::MAX` is "no predecessor", which is also what the start module
        // has. A graph with `u32::MAX` modules in it cannot be built — the ids
        // are `u32` — so the sentinel cannot collide with a real position.
        const NONE: u32 = u32::MAX;
        let mut predecessor = vec![NONE; self.modules.len()];
        let mut seen = vec![false; self.modules.len()];
        let mut work = std::collections::VecDeque::new();
        seen[id.index()] = true;
        work.push_back(id);

        while let Some(current) = work.pop_front() {
            if let Some(specifier) = self.modules[current.index()]
                .external_imports
                .iter()
                .find(|specifier| uf_lib::is_client_module(specifier))
            {
                return ClientBundleReason::ImportsPackage {
                    chain: self.chain_to(id, current, &predecessor),
                    specifier: specifier.clone(),
                };
            }
            for target in self.modules[current.index()].imports.iter().copied() {
                if seen[target.index()] {
                    continue;
                }
                seen[target.index()] = true;
                predecessor[target.index()] = current.0;
                if self.modules[target.index()].environment == ModuleEnvironment::Client {
                    return ClientBundleReason::Imports(self.chain_to(id, target, &predecessor));
                }
                work.push_back(target);
            }
        }
        ClientBundleReason::Isolated
    }

    /// Walk the predecessors back from `target` to `from`, and turn them round.
    ///
    /// Separated from the search because the two are read for different
    /// reasons: above is "which module do we reach", and this is "say it in
    /// the order somebody would write the imports down".
    fn chain_to(&self, from: ModuleId, target: ModuleId, predecessor: &[u32]) -> Vec<ModuleId> {
        let mut chain = vec![target];
        let mut current = target;
        while current != from {
            let previous = predecessor[current.index()];
            debug_assert_ne!(previous, u32::MAX, "a visited module has a predecessor");
            current = ModuleId(previous);
            chain.push(current);
        }
        chain.reverse();
        chain
    }
}

/// Why the browser has to be able to evaluate a module.
///
/// The answer [`RscGraph::client_bundle_reason`] gives.
///
/// A package client module can be the far side of the boundary too, but it is
/// still the same question: this module is a boundary, is above one, or is not
/// the browser's business.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientBundleReason {
    /// Nothing the browser evaluates reaches this module.
    ///
    /// The module is server code, or it is dead code, and either way its
    /// source is not in a bundle a visitor downloads.
    Isolated,
    /// The module declares `"use client"`.
    ///
    /// It is a client bundle root by definition, and there is no chain to
    /// give: the directive on its first line is the whole answer.
    Declared,
    /// The module imports a `"use client"` module, through this chain.
    ///
    /// The first element is the module that was asked about and the last is
    /// the `"use client"` module it reaches; every element imports the one
    /// after it. Never empty, and never one element long — a chain of one
    /// would be [`Self::Declared`] said badly.
    Imports(Vec<ModuleId>),
    /// The module imports a known package client module, through this chain.
    ///
    /// The chain names project modules only: the package specifier is not a
    /// module id because it deliberately was not read from `node_modules`.
    ImportsPackage {
        /// Project modules followed to the importer of `specifier`.
        chain: Vec<ModuleId>,
        /// Bare specifier imported at the boundary.
        specifier: CompactString,
    },
}

#[cfg(test)]
mod tests;
