//! Turning a list of modules into a resolved graph.
//!
//! [`RscGraphBuilder`] resolves edges, then propagates server and client
//! reachability with an explicit `(module, colour)` worklist so that a cyclic
//! import graph still terminates. Client boundaries, bundle roots and closure
//! proximity fall out of that walk, and the per-module contract checks run once
//! the colours are known.

use std::collections::VecDeque;

use camino::Utf8PathBuf;
use compact_str::CompactString;
use uf_infra::{FxHashMap, FxHashSet, InlineVec};

use crate::directive::ModuleEnvironment;
use crate::scan::{ExportKind, HookCall, ImportKind, ImportSpecifier};

use super::diagnostic::{ClientOnlyHookOrigin, RscDiagnostic};
use super::report::{report_client_graph_leaks, report_module_diagnostics};
use super::resolve::{
    SpecifierResolution, is_inside_project, normalize_module_path, resolve_candidates,
    resolve_specifier,
};
use super::{
    ClientBoundary, ClientBoundaryProximity, ClientBoundaryTarget, EntryKind, ModuleId,
    ModuleReachability, RscGraph, RscModule, RscModuleInput, is_client_only_hook_package,
    package_hook_server_component_safe,
};

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
        let hook_classifications = HookClassifications::new(&modules, &resolved);

        let mut graph_modules = Vec::with_capacity(modules.len());
        for (position, module) in modules.iter().enumerate() {
            let reachability =
                ModuleReachability::from_colours(server_seen[position], client_seen[position]);
            report_module_diagnostics(
                module,
                ModuleId(position as u32),
                reachability,
                &resolved[position],
                &hook_classifications,
                &mut diagnostics,
            );
            graph_modules.push(RscModule {
                path: module.path.clone(),
                environment: module.environment,
                reachability,
                proximity: proximity[position],
                imports: resolved[position].imports.clone(),
                external_imports: resolved[position]
                    .external
                    .iter()
                    .map(|import| import.specifier.clone())
                    .collect(),
                exports: module.exports.clone(),
                function_actions: module.function_actions.clone(),
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

/// Resolved edges of one module.
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedImports {
    pub(crate) imports: InlineVec<ModuleId, 8>,
    pub(crate) resolved: Vec<ResolvedModuleImport>,
    pub(crate) external: Vec<ImportSpecifier>,
}

/// One import clause resolved to a module in the graph.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedModuleImport {
    pub(crate) import: ImportSpecifier,
    pub(crate) target: ModuleId,
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
                            edges.resolved.push(ResolvedModuleImport {
                                import: import.clone(),
                                target: id,
                            });
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

/// One exported function or re-export the hook fixpoint can classify.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ExportKey {
    module: ModuleId,
    name: CompactString,
}

/// Verdict for one hook call in a server-reachable module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HookCallVerdict {
    /// The hook resolves to an export that only runs in the browser.
    ClientOnly(ClientOnlyHookOrigin),
    /// The hook resolves to an export whose body was checked and found safe.
    ServerSafe,
    /// The graph cannot see enough to decide.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportStatus {
    ClientOnly,
    ServerSafe,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HookDependency {
    PackageClientOnly,
    PackageServerSafe,
    Project(ExportKey),
    Unknown,
}

/// Client-only and unknown project hook exports.
///
/// Function exports start as classified: the scanner saw their body and can
/// decide whether that body directly reaches a client-only API. A body that
/// calls an unknown hook stays unknown; a body that calls another project hook
/// follows the edge until either a client-only export or an unknown edge is
/// found. Re-exported hooks join the same graph through their `export { ... }
/// from` binding.
#[derive(Debug, Clone, Default)]
pub(crate) struct HookClassifications {
    candidates: FxHashSet<ExportKey>,
    client_only: FxHashSet<ExportKey>,
    unknown: FxHashSet<ExportKey>,
    module_paths: Vec<Utf8PathBuf>,
}

impl HookClassifications {
    fn new(modules: &[RscModuleInput], resolved: &[ResolvedImports]) -> Self {
        let mut candidates = FxHashSet::default();
        for (position, module) in modules.iter().enumerate() {
            let module_id = ModuleId(position as u32);
            for export in &module.exports {
                if matches!(export.kind, ExportKind::ReExport) || export.kind.is_function() {
                    candidates.insert(ExportKey {
                        module: module_id,
                        name: export.name.clone(),
                    });
                }
            }
        }

        let mut direct_client = FxHashSet::default();
        let mut direct_unknown = FxHashSet::default();
        let mut edges = Vec::new();

        for (position, module) in modules.iter().enumerate() {
            let module_id = ModuleId(position as u32);
            let mut collector = HookDependencyCollector {
                module,
                module_id,
                candidates: &candidates,
                resolved: &resolved[position],
                direct_client: &mut direct_client,
                direct_unknown: &mut direct_unknown,
                edges: &mut edges,
            };
            for export in &module.exports {
                let source = ExportKey {
                    module: module_id,
                    name: export.name.clone(),
                };
                if !collector.is_candidate(&source) {
                    continue;
                }

                if export.kind.is_function() {
                    let owner = export.local.as_ref().unwrap_or(&export.name);
                    collector.collect_function(&source, owner);
                }

                if matches!(export.kind, ExportKind::ReExport) {
                    collector.collect_reexport(&source);
                }
            }
        }

        let mut client_only = direct_client;
        loop {
            let mut changed = false;
            for (source, target) in &edges {
                if client_only.contains(target) && client_only.insert(source.clone()) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut unknown = direct_unknown;
        loop {
            let mut changed = false;
            for (source, target) in &edges {
                if !client_only.contains(target)
                    && unknown.contains(target)
                    && unknown.insert(source.clone())
                {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        Self {
            candidates,
            client_only,
            unknown,
            module_paths: modules.iter().map(|module| module.path.clone()).collect(),
        }
    }

    pub(crate) fn call_verdict(
        &self,
        module: ModuleId,
        call: &HookCall,
        resolved: &ResolvedImports,
    ) -> HookCallVerdict {
        match package_hook_dependency(&call.name, &resolved.external) {
            Some(HookDependency::PackageClientOnly) => {
                return HookCallVerdict::ClientOnly(ClientOnlyHookOrigin::Package(
                    CompactString::from(super::CLIENT_ONLY_HOOK_PACKAGE),
                ));
            }
            Some(HookDependency::PackageServerSafe) => return HookCallVerdict::ServerSafe,
            Some(HookDependency::Unknown) => return HookCallVerdict::Unknown,
            Some(HookDependency::Project(_)) | None => {}
        }

        match project_hook_dependency(module, &call.name, resolved, &self.candidates) {
            HookDependency::Project(target) => self.export_verdict(target),
            HookDependency::Unknown => HookCallVerdict::Unknown,
            HookDependency::PackageClientOnly | HookDependency::PackageServerSafe => {
                unreachable!("project dependency only")
            }
        }
    }

    fn export_verdict(&self, key: ExportKey) -> HookCallVerdict {
        match self.export_status(&key) {
            Some(ExportStatus::ClientOnly) => {
                let path = self.module_paths[key.module.index()].clone();
                HookCallVerdict::ClientOnly(ClientOnlyHookOrigin::Module(path))
            }
            Some(ExportStatus::ServerSafe) => HookCallVerdict::ServerSafe,
            Some(ExportStatus::Unknown) | None => HookCallVerdict::Unknown,
        }
    }

    fn export_status(&self, key: &ExportKey) -> Option<ExportStatus> {
        if !self.candidates.contains(key) {
            return None;
        }
        if self.client_only.contains(key) {
            return Some(ExportStatus::ClientOnly);
        }
        if self.unknown.contains(key) {
            return Some(ExportStatus::Unknown);
        }
        Some(ExportStatus::ServerSafe)
    }
}

struct HookDependencyCollector<'a> {
    module: &'a RscModuleInput,
    module_id: ModuleId,
    candidates: &'a FxHashSet<ExportKey>,
    resolved: &'a ResolvedImports,
    direct_client: &'a mut FxHashSet<ExportKey>,
    direct_unknown: &'a mut FxHashSet<ExportKey>,
    edges: &'a mut Vec<(ExportKey, ExportKey)>,
}

impl HookDependencyCollector<'_> {
    fn is_candidate(&self, source: &ExportKey) -> bool {
        self.candidates.contains(source)
    }

    fn collect_function(&mut self, source: &ExportKey, owner: &str) {
        if self
            .module
            .client_api_uses
            .iter()
            .any(|use_site| use_site.owner.as_deref() == Some(owner))
        {
            self.direct_client.insert(source.clone());
        }

        for call in self
            .module
            .hook_calls
            .iter()
            .filter(|call| call.owner.as_deref() == Some(owner))
        {
            if let Some(dependency) = package_hook_dependency(&call.name, &self.resolved.external) {
                match dependency {
                    HookDependency::PackageClientOnly => {
                        self.direct_client.insert(source.clone());
                    }
                    HookDependency::PackageServerSafe => {}
                    HookDependency::Unknown => {
                        self.direct_unknown.insert(source.clone());
                    }
                    HookDependency::Project(_) => unreachable!("package dependency only"),
                }
                continue;
            }

            match project_hook_dependency(
                self.module_id,
                &call.name,
                self.resolved,
                self.candidates,
            ) {
                HookDependency::Project(target) => self.edges.push((source.clone(), target)),
                HookDependency::Unknown => {
                    self.direct_unknown.insert(source.clone());
                }
                HookDependency::PackageClientOnly | HookDependency::PackageServerSafe => {
                    unreachable!("project dependency only")
                }
            }
        }
    }

    fn collect_reexport(&mut self, source: &ExportKey) {
        let mut found = false;
        for import in self
            .resolved
            .resolved
            .iter()
            .filter(|import| import.import.kind == ImportKind::ReExport)
        {
            for binding in &import.import.bindings {
                if binding.local != source.name {
                    continue;
                }
                found = true;
                let Some(name) = binding.imported.as_export_name() else {
                    self.direct_unknown.insert(source.clone());
                    continue;
                };
                let target = ExportKey {
                    module: import.target,
                    name: CompactString::from(name),
                };
                if self.candidates.contains(&target) {
                    self.edges.push((source.clone(), target));
                } else {
                    self.direct_unknown.insert(source.clone());
                }
            }
        }
        for import in self.resolved.external.iter().filter(|import| {
            import.kind == ImportKind::ReExport && is_client_only_hook_package(&import.specifier)
        }) {
            for binding in &import.bindings {
                if binding.local != source.name {
                    continue;
                }
                found = true;
                let Some(imported) = binding.imported.as_export_name() else {
                    self.direct_unknown.insert(source.clone());
                    continue;
                };
                match package_hook_server_component_safe(imported) {
                    Some(true) => {}
                    Some(false) => {
                        self.direct_client.insert(source.clone());
                    }
                    None => {
                        self.direct_unknown.insert(source.clone());
                    }
                }
            }
        }
        if !found {
            self.direct_unknown.insert(source.clone());
        }
    }
}

fn package_hook_dependency(hook: &str, imports: &[ImportSpecifier]) -> Option<HookDependency> {
    for import in imports
        .iter()
        .filter(|import| is_client_only_hook_package(&import.specifier))
    {
        for binding in &import.bindings {
            if binding.local != hook {
                continue;
            }
            let Some(imported) = binding.imported.as_export_name() else {
                return Some(HookDependency::Unknown);
            };
            return Some(match package_hook_server_component_safe(imported) {
                Some(true) => HookDependency::PackageServerSafe,
                Some(false) => HookDependency::PackageClientOnly,
                None => HookDependency::Unknown,
            });
        }
    }
    if imports
        .iter()
        .any(|import| import.bindings.iter().any(|binding| binding.local == hook))
    {
        return Some(HookDependency::Unknown);
    }
    None
}

fn project_hook_dependency(
    module: ModuleId,
    hook: &str,
    resolved: &ResolvedImports,
    candidates: &FxHashSet<ExportKey>,
) -> HookDependency {
    for import in resolved
        .resolved
        .iter()
        .filter(|import| import.import.kind == ImportKind::Static)
    {
        for binding in &import.import.bindings {
            if binding.local != hook {
                continue;
            }
            let Some(name) = binding.imported.as_export_name() else {
                return HookDependency::Unknown;
            };
            let target = ExportKey {
                module: import.target,
                name: CompactString::from(name),
            };
            if candidates.contains(&target) {
                return HookDependency::Project(target);
            }
            return HookDependency::Unknown;
        }
    }

    let local = ExportKey {
        module,
        name: CompactString::from(hook),
    };
    if candidates.contains(&local) {
        return HookDependency::Project(local);
    }
    HookDependency::Unknown
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
                EntryKind::Client => match environments[target.index()] {
                    // And the mirror of it: the client never executes a
                    // `"use server"` module either. `@uniflowed/vite` replaces
                    // that module, in the browser's graph, with one
                    // `createServerReference` per callable export — so what
                    // crosses the import is an id and a `fetch`, and the
                    // module's own body, its imports and everything only they
                    // reached stay on the server.
                    //
                    // Colouring it server rather than client is what makes the
                    // rest of this file agree with that. A database handle
                    // reached only through an action is not in the client
                    // graph, so it is not a leak to report; the client-only
                    // API check does run over the action's body, because the
                    // server is what executes it; and the module's `imports`
                    // are walked with the server colour, so a Server Component
                    // an action calls is server code and not shared code.
                    //
                    // The rule is the transform's, so it is only true while
                    // the transform is: see `serverActionModules` in
                    // `packages/vite/internal/rsc.js` and
                    // `crates/uf_rsc/src/action/registry.rs` for which exports
                    // become references at all.
                    ModuleEnvironment::ServerActions => EntryKind::Server,
                    _ => EntryKind::Client,
                },
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
                    target: ClientBoundaryTarget::Module(target),
                });
            }
        }
        for import in &edges.external {
            if uf_lib::is_client_module(&import.specifier) {
                boundaries.push(ClientBoundary {
                    importer: ModuleId(position as u32),
                    target: ClientBoundaryTarget::Package(import.specifier.clone()),
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
) -> Vec<ClientBoundaryTarget> {
    let mut roots: Vec<ClientBoundaryTarget> = boundaries
        .iter()
        .map(|boundary| boundary.target.clone())
        .collect();
    for (path, kind) in entries {
        if *kind != EntryKind::Client {
            continue;
        }
        if let Some(id) = index.get(path)
            && environments[id.index()] == ModuleEnvironment::Client
        {
            roots.push(ClientBoundaryTarget::Module(*id));
        }
    }
    roots.sort_unstable();
    roots.dedup();
    roots
}
