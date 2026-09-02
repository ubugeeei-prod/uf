//! The module graph: every module a build reaches, and how it got there.
//!
//! # Termination
//!
//! Import graphs contain cycles, and a graph walk that recurses on them either
//! overflows the stack or never finishes. The walk here is an explicit queue of
//! `(path, depth)` pairs with a seen-set, so every module is loaded once and
//! the build is `O(V + E)` on any input, cyclic or not. There is no recursion
//! in this module.
//!
//! # Where the facts come from
//!
//! Each module is loaded, transformed through the plugin container, and then
//! read twice, for two different questions:
//!
//! * [`uf_rsc::module_environment`] answers *where does this run* from the
//!   module as it was loaded, before any transform can move the directive
//!   prologue;
//! * [`crate::record::scan_module`] answers *what does it import and export*
//!   from the transformed code, which is the code that will actually be
//!   emitted.
//!
//! Reachability, client boundaries and client-bundle roots are then
//! [`uf_rsc`]'s answer, not a second one: the resolved edges are handed back to
//! [`RscGraphBuilder`] as relative specifiers so the RSC contract is checked by
//! the crate that owns it.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use uf_infra::FxHashMap;
use uf_plugin::{HookOutcome, PluginContainer, ResolvedKind};
use uf_rsc::{EntryKind, ModuleEnvironment, RscGraph, RscGraphBuilder, RscModuleInput};

use crate::BundleError;
use crate::limits::{BundlerLimits, LimitError};
use crate::record::{ModuleRecord, SideEffectKind, scan_module};
use crate::resolve::{Resolution, Resolver, SideEffectsField};

mod relative;

pub use relative::relative_specifier;

/// Identifier of a module inside one [`ModuleGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModuleIndex(u32);

impl ModuleIndex {
    /// Position of the module in [`ModuleGraph::modules`].
    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }

    /// An index for a position in [`ModuleGraph::modules`].
    #[must_use]
    pub const fn from_position(position: usize) -> Self {
        Self(position as u32)
    }
}

/// What one import specifier resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edge {
    /// A module in this graph.
    Module(ModuleIndex),
    /// Left to the runtime, and emitted as a real `import` in the chunk.
    External(CompactString),
}

/// One module, loaded, transformed and read.
#[derive(Debug, Clone)]
pub struct BundleModule {
    /// Path relative to the project root.
    pub path: Utf8PathBuf,
    /// The module exactly as it was loaded, kept for source maps.
    pub source: String,
    /// The module after every transform in the pipeline.
    pub code: String,
    /// Imports, exports and rewrite spans of [`Self::code`].
    pub record: ModuleRecord,
    /// Where the module runs.
    pub environment: ModuleEnvironment,
    /// Whether it can be dropped when nothing imports anything from it.
    pub shakeable: bool,
    /// Resolved targets, parallel to `record.imports`.
    pub edges: Vec<Edge>,
    /// Shortest distance from an entry.
    pub depth: u32,
}

impl BundleModule {
    /// The edge for one of this module's imports.
    #[must_use]
    pub fn edge(&self, import: usize) -> Option<&Edge> {
        self.edges.get(import)
    }
}

/// Every module a build reaches, with the RSC facts about them.
#[derive(Debug, Clone)]
pub struct ModuleGraph {
    modules: Vec<BundleModule>,
    index: FxHashMap<Utf8PathBuf, ModuleIndex>,
    entries: Vec<ModuleIndex>,
    rsc: RscGraph,
}

impl ModuleGraph {
    /// Every module, in the order it was first reached.
    #[must_use]
    pub fn modules(&self) -> &[BundleModule] {
        &self.modules
    }

    /// One module by index.
    #[must_use]
    pub fn module(&self, index: ModuleIndex) -> &BundleModule {
        &self.modules[index.get()]
    }

    /// One module by project-relative path.
    #[must_use]
    pub fn index_of(&self, path: &Utf8Path) -> Option<ModuleIndex> {
        self.index.get(path).copied()
    }

    /// The entry modules, in declaration order.
    #[must_use]
    pub fn entries(&self) -> &[ModuleIndex] {
        &self.entries
    }

    /// The RSC view of the same modules: reachability, boundaries, roots.
    #[must_use]
    pub const fn rsc(&self) -> &RscGraph {
        &self.rsc
    }

    /// The modules `uf_rsc` computed as client-bundle roots.
    #[must_use]
    pub fn client_roots(&self) -> Vec<ModuleIndex> {
        let mut roots = self
            .rsc
            .client_bundle_roots()
            .iter()
            .filter_map(|id| self.rsc.module_by_id(*id))
            .filter_map(|module| self.index_of(&module.path))
            .collect::<Vec<_>>();
        roots.sort_unstable();
        roots.dedup();
        roots
    }

    /// Whether a module is reachable from the client half of the app.
    #[must_use]
    pub fn is_client_reachable(&self, index: ModuleIndex) -> bool {
        self.rsc
            .module(self.modules[index.get()].path.as_str())
            .is_some_and(|module| module.reachability.is_client_reachable())
    }
}

/// Load, transform and resolve everything the entries reach.
pub fn build_graph(
    resolver: &mut Resolver,
    container: &PluginContainer,
    entries: &[Utf8PathBuf],
    limits: &BundlerLimits,
) -> Result<ModuleGraph, BundleError> {
    let mut builder = GraphBuilder::new(limits);
    for entry in entries {
        let path = uf_rsc::normalize_module_path(entry);
        if !uf_rsc::is_inside_project(&path) {
            return Err(BundleError::EntryOutsideProject { entry: path });
        }
        builder.enqueue(path, 0);
    }

    while let Some((path, depth)) = builder.queue.pop_front() {
        builder.load_module(resolver, container, &path, depth)?;
    }
    builder.link(resolver, container)?;
    Ok(builder.finish())
}

/// The mutable half of graph construction.
struct GraphBuilder<'a> {
    modules: Vec<BundleModule>,
    index: FxHashMap<Utf8PathBuf, ModuleIndex>,
    entries: Vec<ModuleIndex>,
    queue: std::collections::VecDeque<(Utf8PathBuf, u32)>,
    limits: &'a BundlerLimits,
}

impl<'a> GraphBuilder<'a> {
    fn new(limits: &'a BundlerLimits) -> Self {
        Self {
            modules: Vec::new(),
            index: FxHashMap::default(),
            entries: Vec::new(),
            queue: std::collections::VecDeque::new(),
            limits,
        }
    }

    fn enqueue(&mut self, path: Utf8PathBuf, depth: u32) {
        if self.index.contains_key(&path) || self.queue.iter().any(|(queued, _)| *queued == path) {
            return;
        }
        self.queue.push_back((path, depth));
    }

    fn load_module(
        &mut self,
        resolver: &mut Resolver,
        container: &PluginContainer,
        path: &Utf8Path,
        depth: u32,
    ) -> Result<(), BundleError> {
        if self.index.contains_key(path) {
            return Ok(());
        }
        if self.modules.len() >= self.limits.max_modules {
            return Err(LimitError::TooManyModules {
                count: self.modules.len() + 1,
                limit: self.limits.max_modules,
            }
            .into());
        }
        if depth > self.limits.max_depth {
            return Err(LimitError::GraphTooDeep {
                module: path.to_path_buf(),
                depth,
                limit: self.limits.max_depth,
            }
            .into());
        }

        let source = load_source(resolver, container, path, self.limits)?;
        let environment = uf_rsc::module_environment(&source);
        let code = match container.transform(path.as_str(), &source)? {
            HookOutcome::Handled(produced) => produced.code,
            HookOutcome::Passthrough => source.clone(),
        };
        let record = scan_module(&code);

        let package_free = matches!(
            resolver.manifest_for(path)?.side_effects,
            SideEffectsField::None
        );
        let shakeable = package_free || record.side_effects == SideEffectKind::None;

        let index = ModuleIndex(self.modules.len() as u32);
        self.index.insert(path.to_path_buf(), index);
        if depth == 0 {
            self.entries.push(index);
        }
        self.modules.push(BundleModule {
            path: path.to_path_buf(),
            source,
            code,
            record,
            environment,
            shakeable,
            edges: Vec::new(),
            depth,
        });

        for import in self.modules[index.get()].record.imports.clone() {
            if !import.form.is_linked() {
                continue;
            }
            if let Resolution::Module(target) =
                resolve_import(resolver, container, path, &import.specifier)?
            {
                self.enqueue(target, depth.saturating_add(1));
            }
        }

        Ok(())
    }

    /// Resolve every import into an edge, now that every module is known.
    fn link(
        &mut self,
        resolver: &mut Resolver,
        container: &PluginContainer,
    ) -> Result<(), BundleError> {
        for position in 0..self.modules.len() {
            let path = self.modules[position].path.clone();
            let specifiers = self.modules[position]
                .record
                .imports
                .iter()
                .map(|import| (import.specifier.clone(), import.form))
                .collect::<Vec<_>>();

            let mut edges = Vec::with_capacity(specifiers.len());
            for (specifier, form) in specifiers {
                let edge = if form.is_linked() {
                    match resolve_import(resolver, container, &path, &specifier)? {
                        Resolution::Module(target) => match self.index.get(&target) {
                            Some(index) => Edge::Module(*index),
                            None => Edge::External(specifier),
                        },
                        Resolution::External(specifier) => Edge::External(specifier),
                    }
                } else {
                    Edge::External(specifier)
                };
                edges.push(edge);
            }
            self.modules[position].edges = edges;
        }
        Ok(())
    }

    fn finish(self) -> ModuleGraph {
        let rsc = build_rsc_graph(&self.modules, &self.entries);
        ModuleGraph {
            modules: self.modules,
            index: self.index,
            entries: self.entries,
            rsc,
        }
    }
}

/// Ask the pipeline for a specifier's id, then fall back to the resolver.
fn resolve_import(
    resolver: &mut Resolver,
    container: &PluginContainer,
    importer: &Utf8Path,
    specifier: &str,
) -> Result<Resolution, BundleError> {
    if let HookOutcome::Handled(resolved) =
        container.resolve_id(specifier, Some(importer.as_str()))?
    {
        return Ok(match resolved.kind {
            ResolvedKind::External => Resolution::External(resolved.id),
            ResolvedKind::Bundled | ResolvedKind::Virtual => {
                let path = uf_rsc::normalize_module_path(Utf8Path::new(resolved.id.as_str()));
                if uf_rsc::is_inside_project(&path) {
                    Resolution::Module(path)
                } else {
                    Resolution::External(resolved.id)
                }
            }
        });
    }
    Ok(resolver.resolve(importer, specifier)?)
}

/// Read a module, letting a `Load` plugin answer first.
fn load_source(
    resolver: &Resolver,
    container: &PluginContainer,
    path: &Utf8Path,
    limits: &BundlerLimits,
) -> Result<String, BundleError> {
    if let HookOutcome::Handled(code) = container.load(path.as_str())? {
        return Ok(code.code);
    }

    let absolute = resolver.root().join(path);
    let metadata = std::fs::metadata(&absolute).map_err(|source| BundleError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > limits.max_module_bytes {
        return Err(LimitError::ModuleTooLarge {
            module: path.to_path_buf(),
            bytes: metadata.len(),
            limit: limits.max_module_bytes,
        }
        .into());
    }

    let bytes = std::fs::read(&absolute).map_err(|source| BundleError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    uf_infra::validate_utf8(&bytes)
        .map(str::to_owned)
        .map_err(|_| BundleError::NonUtf8 {
            path: path.to_path_buf(),
        })
}

/// Hand the resolved edges back to `uf_rsc` as relative specifiers.
///
/// Resolution happened here, so the specifiers fed back are already-resolved
/// paths expressed relative to their importer. That keeps reachability, client
/// boundaries and the RSC diagnostics in the crate that owns them instead of
/// growing a second, subtly different implementation in the bundler.
fn build_rsc_graph(modules: &[BundleModule], entries: &[ModuleIndex]) -> RscGraph {
    let mut builder = RscGraphBuilder::new();

    for module in modules {
        let mut input = RscModuleInput::new(module.path.clone(), module.environment);
        for (position, import) in module.record.imports.iter().enumerate() {
            match module.edge(position) {
                Some(Edge::Module(target)) => {
                    input = input.with_import(relative_specifier(
                        &module.path,
                        &modules[target.get()].path,
                    ));
                }
                // Externals keep the specifier the author wrote, which is what
                // the `server-only` checks in `uf_rsc` look at.
                Some(Edge::External(specifier)) => input = input.with_import(specifier.clone()),
                None => input = input.with_import(import.specifier.clone()),
            }
        }
        for export in &module.record.exports {
            input = input.with_export(export.exported.clone(), uf_rsc::ExportKind::Value);
        }
        builder.add_module(input);
    }

    for entry in entries {
        builder.add_entry(modules[entry.get()].path.clone(), EntryKind::Server);
    }

    builder.build()
}
