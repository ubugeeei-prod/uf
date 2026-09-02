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

use std::borrow::Cow;
use std::collections::VecDeque;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uf_infra::{FxHashMap, InlineVec, LineIndex};

use crate::directive::{
    DirectiveIssue, DirectiveIssueList, FunctionDirective, FunctionDirectiveList, FunctionOwner,
    ModuleEnvironment, scan_directive_tokens,
};
use crate::scan::{
    ClientApiUseList, ExportKind, ExportList, ImportKind, ImportList, ImportSpecifier,
    ModuleExport, client_api_uses_from_tokens, exports_from_tokens, imports_from_tokens, tokenize,
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

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RscSeverity {
    /// Worth fixing, but the build can continue.
    Warn,
    /// Breaks the RSC contract.
    Error,
}

impl RscSeverity {
    /// Stable identifier used in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// A violation of the React Server Components contract.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RscDiagnostic {
    /// A module in the client graph imports server-only code.
    #[error("client module `{module}` imports server-only `{specifier}` at line {line}")]
    ServerOnlyImportInClientModule {
        /// Importing module, relative to the project root.
        module: Utf8PathBuf,
        /// The specifier as written.
        specifier: CompactString,
        /// 1-based line of the import.
        line: u32,
    },
    /// A Server Component reaches for an API that only exists in the browser.
    #[error("server module `{module}` uses client-only `{api}` at line {line}:{column}")]
    ClientOnlyApiInServerModule {
        /// The server module.
        module: Utf8PathBuf,
        /// Name of the client-only API.
        api: &'static str,
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
    },
    /// A `"use server"` export that React cannot call.
    #[error("server action `{export}` in `{module}` must be an async function")]
    ServerActionNotAsync {
        /// The `"use server"` module.
        module: Utf8PathBuf,
        /// Exported name.
        export: CompactString,
        /// 1-based line of the export.
        line: u32,
    },
    /// A `"use server"` module exporting something that is not a function at all.
    #[error("`use server` module `{module}` exports non-function `{export}`")]
    ServerActionNotFunction {
        /// The `"use server"` module.
        module: Utf8PathBuf,
        /// Exported name.
        export: CompactString,
        /// 1-based line of the export.
        line: u32,
    },
    /// An import specifier that resolves outside the project root.
    #[error("module `{module}` imports `{specifier}` from outside the project root")]
    ImportEscapesProjectRoot {
        /// The importing module.
        module: Utf8PathBuf,
        /// The specifier as written.
        specifier: CompactString,
        /// 1-based line of the import.
        line: u32,
    },
    /// A module whose own path is not inside the project root.
    #[error("module path `{module}` is not inside the project root")]
    ModulePathOutsideProject {
        /// The offending path, as supplied.
        module: Utf8PathBuf,
    },
    /// A rejected directive, lifted from the directive pass.
    #[error("in `{module}`: {issue}")]
    Directive {
        /// The module the directive was written in.
        module: Utf8PathBuf,
        /// What was wrong with it.
        issue: DirectiveIssue,
    },
}

impl RscDiagnostic {
    /// Stable rule identifier.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::ServerOnlyImportInClientModule { .. } => "rsc/server-only-import-in-client",
            Self::ClientOnlyApiInServerModule { .. } => "rsc/client-only-api-in-server",
            Self::ServerActionNotAsync { .. } => "rsc/server-action-not-async",
            Self::ServerActionNotFunction { .. } => "rsc/server-action-not-a-function",
            Self::ImportEscapesProjectRoot { .. } => "rsc/import-escapes-project-root",
            Self::ModulePathOutsideProject { .. } => "rsc/module-outside-project-root",
            Self::Directive { issue, .. } => issue.rule(),
        }
    }

    /// Severity of the diagnostic. Every RSC contract violation is an error.
    pub fn severity(&self) -> RscSeverity {
        RscSeverity::Error
    }

    /// Module the diagnostic belongs to.
    pub fn module(&self) -> &Utf8Path {
        match self {
            Self::ServerOnlyImportInClientModule { module, .. }
            | Self::ClientOnlyApiInServerModule { module, .. }
            | Self::ServerActionNotAsync { module, .. }
            | Self::ServerActionNotFunction { module, .. }
            | Self::ImportEscapesProjectRoot { module, .. }
            | Self::ModulePathOutsideProject { module }
            | Self::Directive { module, .. } => module,
        }
    }

    /// 1-based line the diagnostic points at, when there is one.
    pub fn line(&self) -> u32 {
        match self {
            Self::ServerOnlyImportInClientModule { line, .. }
            | Self::ClientOnlyApiInServerModule { line, .. }
            | Self::ServerActionNotAsync { line, .. }
            | Self::ServerActionNotFunction { line, .. }
            | Self::ImportEscapesProjectRoot { line, .. } => *line,
            Self::ModulePathOutsideProject { .. } => 0,
            Self::Directive { issue, .. } => issue.line(),
        }
    }
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

/// Collects modules and entries, then resolves them into an [`RscGraph`].
#[derive(Debug, Clone, Default)]
pub struct RscGraphBuilder {
    modules: Vec<RscModuleInput>,
    entries: Vec<(Utf8PathBuf, EntryKind)>,
}

impl RscGraphBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a module. The first registration of a path wins.
    pub fn add_module(&mut self, module: RscModuleInput) -> &mut Self {
        self.modules.push(module);
        self
    }

    /// Register a module from its source text.
    pub fn add_source(&mut self, path: impl Into<Utf8PathBuf>, source: &str) -> &mut Self {
        self.add_module(RscModuleInput::from_source(path, source))
    }

    /// Mark a module path as an entry point.
    ///
    /// Modules that no entry reaches are reported as
    /// [`ModuleReachability::Unreachable`] and their actions are never
    /// registered as callable endpoints.
    pub fn add_entry(&mut self, path: impl Into<Utf8PathBuf>, kind: EntryKind) -> &mut Self {
        self.entries
            .push((normalize_module_path(&path.into()), kind));
        self
    }

    /// Resolve imports, propagate reachability and collect diagnostics.
    pub fn build(self) -> RscGraph {
        let Self {
            mut modules,
            entries,
        } = self;

        let mut diagnostics = Vec::new();

        // Deterministic module ids: sort by path, keep the first of any duplicate.
        modules.sort_by(|left, right| left.path.cmp(&right.path));
        modules.dedup_by(|left, right| left.path == right.path);
        modules.retain(|module| {
            if is_inside_project(&module.path) {
                true
            } else {
                diagnostics.push(RscDiagnostic::ModulePathOutsideProject {
                    module: module.path.clone(),
                });
                false
            }
        });

        let mut index: FxHashMap<Utf8PathBuf, ModuleId> = FxHashMap::default();
        index.reserve(modules.len());
        for (position, module) in modules.iter().enumerate() {
            index.insert(module.path.clone(), ModuleId(position as u32));
        }

        let resolved = resolve_edges(&modules, &index, &mut diagnostics);
        let environments: Vec<ModuleEnvironment> =
            modules.iter().map(|module| module.environment).collect();

        let (server_seen, client_seen) = propagate(&resolved, &environments, &entries, &index);
        let boundaries = collect_boundaries(&resolved, &environments, &server_seen);
        let proximity = compute_proximity(&resolved, &boundaries);
        let bundle_roots = collect_bundle_roots(&boundaries, &entries, &index, &environments);

        let mut graph_modules = Vec::with_capacity(modules.len());
        for (position, module) in modules.into_iter().enumerate() {
            let reachability =
                ModuleReachability::from_colours(server_seen[position], client_seen[position]);
            report_module_diagnostics(
                &module,
                reachability,
                &resolved[position].external,
                &mut diagnostics,
            );
            graph_modules.push(RscModule {
                path: module.path,
                environment: module.environment,
                reachability,
                proximity: proximity[position],
                imports: resolved[position].imports.clone(),
                external_imports: resolved[position]
                    .external
                    .iter()
                    .map(|import| import.specifier.clone())
                    .collect(),
                exports: module.exports,
                function_actions: module.function_actions,
            });
        }

        report_client_graph_leaks(&graph_modules, &resolved, &mut diagnostics);

        diagnostics.sort_by(|left, right| {
            left.module()
                .cmp(right.module())
                .then(left.line().cmp(&right.line()))
                .then(left.rule().cmp(right.rule()))
                .then(left.to_string().cmp(&right.to_string()))
        });
        diagnostics.dedup();

        RscGraph {
            modules: graph_modules,
            index,
            boundaries,
            bundle_roots,
            diagnostics,
        }
    }
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

/// Resolved edges of one module.
#[derive(Debug, Clone, Default)]
struct ResolvedImports {
    imports: InlineVec<ModuleId, 8>,
    external: Vec<ImportSpecifier>,
}

fn resolve_edges(
    modules: &[RscModuleInput],
    index: &FxHashMap<Utf8PathBuf, ModuleId>,
    diagnostics: &mut Vec<RscDiagnostic>,
) -> Vec<ResolvedImports> {
    let mut resolved = Vec::with_capacity(modules.len());

    for module in modules {
        let mut edges = ResolvedImports::default();
        for import in &module.imports {
            match resolve_specifier(&module.path, &import.specifier) {
                SpecifierResolution::Relative(candidate) => {
                    match resolve_candidates(&candidate, index) {
                        Some(id) => {
                            if !edges.imports.contains(&id) {
                                edges.imports.push(id);
                            }
                        }
                        None => edges.external.push(import.clone()),
                    }
                }
                SpecifierResolution::Bare => edges.external.push(import.clone()),
                // Path traversal guard: a specifier that climbs out of the
                // project root must never be resolved or read.
                SpecifierResolution::Escapes => {
                    diagnostics.push(RscDiagnostic::ImportEscapesProjectRoot {
                        module: module.path.clone(),
                        specifier: import.specifier.clone(),
                        line: import.line,
                    });
                    edges.external.push(import.clone());
                }
            }
        }
        edges.imports.sort_unstable();
        resolved.push(edges);
    }

    resolved
}

/// Walk the graph with an explicit worklist, once per `(module, colour)` pair.
fn propagate(
    resolved: &[ResolvedImports],
    environments: &[ModuleEnvironment],
    entries: &[(Utf8PathBuf, EntryKind)],
    index: &FxHashMap<Utf8PathBuf, ModuleId>,
) -> (Vec<bool>, Vec<bool>) {
    let mut server_seen = vec![false; resolved.len()];
    let mut client_seen = vec![false; resolved.len()];
    let mut work: VecDeque<(usize, EntryKind)> = VecDeque::new();

    for (path, kind) in entries {
        let Some(id) = index.get(path) else {
            continue;
        };
        let colour = match (kind, environments[id.index()]) {
            // A `"use client"` module declared as a server entry is still a
            // client module: it is referenced, never executed, by the server.
            (EntryKind::Server, ModuleEnvironment::Client) => EntryKind::Client,
            (kind, _) => *kind,
        };
        enqueue(
            id.index(),
            colour,
            &mut server_seen,
            &mut client_seen,
            &mut work,
        );
    }

    while let Some((position, colour)) = work.pop_front() {
        for target in resolved[position].imports.iter().copied() {
            let next = match colour {
                EntryKind::Server => match environments[target.index()] {
                    // The server never executes a client module; it emits a
                    // reference, and the client module becomes a bundle root.
                    ModuleEnvironment::Client => EntryKind::Client,
                    _ => EntryKind::Server,
                },
                EntryKind::Client => EntryKind::Client,
            };
            enqueue(
                target.index(),
                next,
                &mut server_seen,
                &mut client_seen,
                &mut work,
            );
        }
    }

    (server_seen, client_seen)
}

fn enqueue(
    position: usize,
    colour: EntryKind,
    server_seen: &mut [bool],
    client_seen: &mut [bool],
    work: &mut VecDeque<(usize, EntryKind)>,
) {
    let seen = match colour {
        EntryKind::Server => &mut server_seen[position],
        EntryKind::Client => &mut client_seen[position],
    };
    if !*seen {
        *seen = true;
        work.push_back((position, colour));
    }
}

fn collect_boundaries(
    resolved: &[ResolvedImports],
    environments: &[ModuleEnvironment],
    server_seen: &[bool],
) -> Vec<ClientBoundary> {
    let mut boundaries = Vec::new();
    for (position, edges) in resolved.iter().enumerate() {
        if !server_seen[position] || environments[position] == ModuleEnvironment::Client {
            continue;
        }
        for target in edges.imports.iter().copied() {
            if environments[target.index()] == ModuleEnvironment::Client {
                boundaries.push(ClientBoundary {
                    importer: ModuleId(position as u32),
                    client_module: target,
                });
            }
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

/// Reverse walk from the boundary importers, again with an explicit worklist.
fn compute_proximity(
    resolved: &[ResolvedImports],
    boundaries: &[ClientBoundary],
) -> Vec<ClientBoundaryProximity> {
    let mut reverse: Vec<Vec<u32>> = vec![Vec::new(); resolved.len()];
    for (position, edges) in resolved.iter().enumerate() {
        for target in edges.imports.iter().copied() {
            reverse[target.index()].push(position as u32);
        }
    }

    let mut proximity = vec![ClientBoundaryProximity::Isolated; resolved.len()];
    let mut work: VecDeque<usize> = VecDeque::new();
    for boundary in boundaries {
        let position = boundary.importer.index();
        if proximity[position] == ClientBoundaryProximity::Isolated {
            proximity[position] = ClientBoundaryProximity::ReachesBoundary;
            work.push_back(position);
        }
    }

    while let Some(position) = work.pop_front() {
        for importer in reverse[position].iter().copied() {
            let importer = importer as usize;
            if proximity[importer] == ClientBoundaryProximity::Isolated {
                proximity[importer] = ClientBoundaryProximity::ReachesBoundary;
                work.push_back(importer);
            }
        }
    }

    proximity
}

fn collect_bundle_roots(
    boundaries: &[ClientBoundary],
    entries: &[(Utf8PathBuf, EntryKind)],
    index: &FxHashMap<Utf8PathBuf, ModuleId>,
    environments: &[ModuleEnvironment],
) -> Vec<ModuleId> {
    let mut roots: Vec<ModuleId> = boundaries
        .iter()
        .map(|boundary| boundary.client_module)
        .collect();
    for (path, kind) in entries {
        if *kind != EntryKind::Client {
            continue;
        }
        if let Some(id) = index.get(path)
            && environments[id.index()] == ModuleEnvironment::Client
        {
            roots.push(*id);
        }
    }
    roots.sort_unstable();
    roots.dedup();
    roots
}

fn report_module_diagnostics(
    module: &RscModuleInput,
    reachability: ModuleReachability,
    external: &[ImportSpecifier],
    diagnostics: &mut Vec<RscDiagnostic>,
) {
    for issue in &module.directive_issues {
        diagnostics.push(RscDiagnostic::Directive {
            module: module.path.clone(),
            issue: issue.clone(),
        });
    }

    if module.environment == ModuleEnvironment::ServerActions {
        for export in &module.exports {
            match export.kind {
                ExportKind::AsyncFunction | ExportKind::ReExport => {}
                ExportKind::SyncFunction => {
                    diagnostics.push(RscDiagnostic::ServerActionNotAsync {
                        module: module.path.clone(),
                        export: export.name.clone(),
                        line: export.line,
                    });
                }
                ExportKind::Class | ExportKind::Value => {
                    diagnostics.push(RscDiagnostic::ServerActionNotFunction {
                        module: module.path.clone(),
                        export: export.name.clone(),
                        line: export.line,
                    });
                }
            }
        }
    }

    // Client-only APIs only matter for code the server actually executes. A
    // module without a directive that is reached solely from the client graph is
    // bundled for the browser, where the hooks it calls are legal.
    if module.environment.runs_on_server() && reachability.is_server_reachable() {
        for use_site in &module.client_api_uses {
            diagnostics.push(RscDiagnostic::ClientOnlyApiInServerModule {
                module: module.path.clone(),
                api: use_site.api,
                line: use_site.line,
                column: use_site.column,
            });
        }
    }

    if module.environment == ModuleEnvironment::Client || reachability.is_client_reachable() {
        for import in external {
            if is_server_only_specifier(&import.specifier) {
                diagnostics.push(RscDiagnostic::ServerOnlyImportInClientModule {
                    module: module.path.clone(),
                    specifier: import.specifier.clone(),
                    line: import.line,
                });
            }
        }
    }
}

/// Report `*.server.js` modules that the client graph resolved to.
fn report_client_graph_leaks(
    modules: &[RscModule],
    resolved: &[ResolvedImports],
    diagnostics: &mut Vec<RscDiagnostic>,
) {
    for (position, module) in modules.iter().enumerate() {
        if module.environment != ModuleEnvironment::Client
            && !module.reachability.is_client_reachable()
        {
            continue;
        }
        for target in resolved[position].imports.iter().copied() {
            let imported = &modules[target.index()];
            if is_server_only_path(imported.path.as_str()) {
                diagnostics.push(RscDiagnostic::ServerOnlyImportInClientModule {
                    module: module.path.clone(),
                    specifier: CompactString::from(imported.path.as_str()),
                    line: 0,
                });
            }
        }
    }
}

/// Whether an import specifier names server-only code.
pub fn is_server_only_specifier(specifier: &str) -> bool {
    if SERVER_ONLY_PACKAGES.binary_search(&specifier).is_ok() {
        return true;
    }
    if SERVER_ONLY_PACKAGES.iter().any(|package| {
        specifier
            .strip_prefix(package)
            .is_some_and(|rest| rest.starts_with('/'))
    }) {
        return true;
    }
    is_server_only_path(specifier)
}

fn is_server_only_path(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|name| {
        name.len() > SERVER_ONLY_SUFFIX.len() && name.ends_with(SERVER_ONLY_SUFFIX)
    })
}

enum SpecifierResolution {
    Relative(Utf8PathBuf),
    Bare,
    Escapes,
}

fn resolve_specifier(importer: &Utf8Path, specifier: &str) -> SpecifierResolution {
    let specifier = if specifier.contains('\\') {
        Cow::Owned(specifier.replace('\\', "/"))
    } else {
        Cow::Borrowed(specifier)
    };
    if !(specifier.starts_with("./") || specifier.starts_with("../") || specifier == ".") {
        return SpecifierResolution::Bare;
    }
    let base = importer.parent().unwrap_or(Utf8Path::new(""));
    match normalize_relative(base, &specifier) {
        Some(path) => SpecifierResolution::Relative(path),
        None => SpecifierResolution::Escapes,
    }
}

/// Join and normalize without touching the file system.
///
/// `..` segments are resolved textually and a specifier that climbs above the
/// project root returns `None` rather than a path outside it. This is the
/// path-traversal guard for the graph: no analysis, and no bundler consuming the
/// manifest, is ever handed a path that leaves the project.
fn normalize_relative(base: &Utf8Path, specifier: &str) -> Option<Utf8PathBuf> {
    let mut segments: Vec<&str> = base
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();

    for segment in specifier.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }

    let mut path = String::with_capacity(specifier.len() + base.as_str().len());
    for (position, segment) in segments.iter().enumerate() {
        if position > 0 {
            path.push('/');
        }
        path.push_str(segment);
    }
    Some(Utf8PathBuf::from(path))
}

/// Try the resolved path, then the `.js` and `/index.js` forms.
fn resolve_candidates(
    candidate: &Utf8Path,
    index: &FxHashMap<Utf8PathBuf, ModuleId>,
) -> Option<ModuleId> {
    if let Some(id) = index.get(candidate) {
        return Some(*id);
    }
    let with_extension = Utf8PathBuf::from(format!("{candidate}.js"));
    if let Some(id) = index.get(&with_extension) {
        return Some(*id);
    }
    let with_index = candidate.join("index.js");
    index.get(&with_index).copied()
}

/// Normalize a module path to the project-relative, forward-slash form.
///
/// A `..` that cannot be resolved is kept so [`is_inside_project`] can reject the
/// path instead of silently rewriting an escape into a plausible-looking module.
fn normalize_module_path(path: &Utf8Path) -> Utf8PathBuf {
    let text = path.as_str().replace('\\', "/");
    let mut segments: Vec<&str> = Vec::new();

    for segment in text.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.last().is_some_and(|last| *last != "..") {
                    segments.pop();
                } else {
                    segments.push("..");
                }
            }
            other => segments.push(other),
        }
    }

    let mut normalized = String::with_capacity(text.len());
    if text.starts_with('/') {
        normalized.push('/');
    }
    for (position, segment) in segments.iter().enumerate() {
        if position > 0 {
            normalized.push('/');
        }
        normalized.push_str(segment);
    }
    Utf8PathBuf::from(normalized)
}

/// Whether a normalized module path stays inside the project root.
fn is_inside_project(path: &Utf8Path) -> bool {
    let text = path.as_str();
    !text.is_empty()
        && !text.starts_with('/')
        && !text.starts_with("../")
        && text != ".."
        && !text.contains(':')
}

#[cfg(test)]
mod tests;
