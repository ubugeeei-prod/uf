//! Turning a list of modules into a resolved graph.
//!
//! [`RscGraphBuilder`] resolves edges, then propagates server and client
//! reachability with an explicit `(module, colour)` worklist so that a cyclic
//! import graph still terminates. Client boundaries, bundle roots and closure
//! proximity fall out of that walk, and the per-module contract checks run once
//! the colours are known.

use std::collections::VecDeque;

use camino::Utf8PathBuf;
use uf_infra::{FxHashMap, InlineVec};

use crate::directive::ModuleEnvironment;
use crate::scan::ImportSpecifier;

use super::diagnostic::RscDiagnostic;
use super::report::{report_client_graph_leaks, report_module_diagnostics};
use super::resolve::{
    SpecifierResolution, is_inside_project, normalize_module_path, resolve_candidates,
    resolve_specifier,
};
use super::{
    ClientBoundary, ClientBoundaryProximity, EntryKind, ModuleId, ModuleReachability, RscGraph,
    RscModule, RscModuleInput,
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

/// Resolved edges of one module.
#[derive(Debug, Clone, Default)]
pub(crate) struct ResolvedImports {
    pub(crate) imports: InlineVec<ModuleId, 8>,
    pub(crate) external: Vec<ImportSpecifier>,
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
