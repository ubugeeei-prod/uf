//! The whole build: entries in, chunks on disk.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use uf_plugin::{PluginContainer, PluginHook};

use crate::BundleError;
use crate::chunk::plan_chunks;
use crate::emit::{ASSET_DIR, EmittedChunk, emit_chunks};
use crate::graph::{ModuleGraph, build_graph};
use crate::limits::BundlerLimits;
use crate::pipeline::{asset_extension, asset_file_name};
use crate::resolve::Resolver;
use crate::shake::{Shaken, shake};

/// What a build was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleOptions {
    /// Project root. Every module path is relative to it.
    pub root: Utf8PathBuf,
    /// Where chunks are written.
    pub out_dir: Utf8PathBuf,
    /// Entry modules, relative to the root.
    pub entries: Vec<Utf8PathBuf>,
    /// Whether to emit source maps.
    pub sourcemap: bool,
    /// The ceilings the build runs under.
    pub limits: BundlerLimits,
}

impl BundleOptions {
    /// Options for a project, with default limits and no entries.
    #[must_use]
    pub fn new(root: impl Into<Utf8PathBuf>, out_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            root: root.into(),
            out_dir: out_dir.into(),
            entries: Vec::new(),
            sourcemap: true,
            limits: BundlerLimits::default(),
        }
    }

    /// Replace the entry list.
    #[must_use]
    pub fn with_entries(mut self, entries: Vec<Utf8PathBuf>) -> Self {
        self.entries = entries;
        self
    }

    /// Turn source maps on or off.
    #[must_use]
    pub const fn with_sourcemap(mut self, sourcemap: bool) -> Self {
        self.sourcemap = sourcemap;
        self
    }

    /// Replace the ceilings.
    #[must_use]
    pub const fn with_limits(mut self, limits: BundlerLimits) -> Self {
        self.limits = limits;
        self
    }
}

/// Counts a build reports when it finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BundleStats {
    /// Modules loaded into the graph.
    pub modules_loaded: usize,
    /// Modules that survived tree shaking.
    pub modules_kept: usize,
    /// Exports dropped because nothing imported them.
    pub exports_dropped: usize,
    /// Chunks emitted.
    pub chunks: usize,
}

/// Everything a build produced.
#[derive(Debug, Clone)]
pub struct BundleOutput {
    /// The rendered chunks, sorted by file name.
    pub chunks: Vec<EmittedChunk>,
    /// Non-JavaScript modules to copy into the output, sorted.
    pub assets: Vec<Utf8PathBuf>,
    /// What the build did.
    pub stats: BundleStats,
}

impl BundleOutput {
    /// The chunk that holds `module`, if any.
    #[must_use]
    pub fn chunk_of(&self, module: &Utf8Path) -> Option<&EmittedChunk> {
        self.chunks
            .iter()
            .find(|chunk| chunk.modules.iter().any(|path| path == module))
    }

    /// Every chunk file a module's entry chunk pulls in, transitively.
    ///
    /// This is what a visitor downloads before the module can run, and what
    /// [`uf_bundle`](https://docs.rs/uf_bundle) measures as a route's initial
    /// JavaScript.
    #[must_use]
    pub fn closure_of(&self, module: &Utf8Path) -> Vec<CompactString> {
        let Some(start) = self.chunk_of(module) else {
            return Vec::new();
        };
        let mut names: Vec<CompactString> = Vec::new();
        let mut queue = vec![start.file_name.clone()];

        while let Some(name) = queue.pop() {
            if names.contains(&name) {
                continue;
            }
            names.push(name.clone());
            let Some(chunk) = self.chunks.iter().find(|chunk| chunk.file_name == name) else {
                continue;
            };
            queue.extend(chunk.imports.iter().cloned());
        }

        names.sort();
        names
    }

    /// Total emitted JavaScript, in bytes.
    #[must_use]
    pub fn code_bytes(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.code.len()).sum()
    }
}

/// Load, shake, chunk and render a project.
pub fn bundle(
    options: &BundleOptions,
    container: &PluginContainer,
) -> Result<BundleOutput, BundleError> {
    container.notify(PluginHook::BuildStart)?;

    let mut resolver = Resolver::new(options.root.clone(), options.limits);
    let graph = build_graph(&mut resolver, container, &options.entries, &options.limits)?;
    container.notify(PluginHook::BuildEnd)?;

    let shaken = shake(&graph);
    let chunks = plan_chunks(&graph, &shaken, &options.limits)?;
    let mut emitted = emit_chunks(&graph, &shaken, &chunks, options.sourcemap);
    emitted.sort_by(|left, right| left.file_name.cmp(&right.file_name));
    container.notify(PluginHook::GenerateBundle)?;

    let assets = collect_asset_modules(&graph, &shaken);
    let stats = BundleStats {
        modules_loaded: graph.modules().len(),
        modules_kept: shaken.live_count(),
        exports_dropped: dropped_exports(&graph, &shaken),
        chunks: emitted.len(),
    };

    Ok(BundleOutput {
        chunks: emitted,
        assets,
        stats,
    })
}

/// Write a build's chunks, maps and assets into the output directory.
pub fn write_bundle(
    options: &BundleOptions,
    output: &BundleOutput,
    container: &PluginContainer,
) -> Result<Vec<Utf8PathBuf>, BundleError> {
    let mut written = Vec::with_capacity(output.chunks.len() * 2 + output.assets.len());

    for chunk in &output.chunks {
        written.push(write_file(
            &options.out_dir,
            &chunk.file_name,
            chunk.code.as_bytes(),
        )?);
        if let (Some(name), Some(map)) = (chunk.source_map_file_name(), chunk.source_map.as_ref()) {
            written.push(write_file(&options.out_dir, &name, map.as_bytes())?);
        }
    }

    for asset in &output.assets {
        let source = options.root.join(asset);
        let bytes = std::fs::read(&source).map_err(|source| BundleError::Read {
            path: asset.clone(),
            source,
        })?;
        let name = format!("{ASSET_DIR}/{}", asset_file_name(asset));
        written.push(write_file(&options.out_dir, &name, &bytes)?);
    }

    container.notify(PluginHook::WriteBundle)?;
    written.sort();
    Ok(written)
}

fn write_file(
    out_dir: &Utf8Path,
    relative: &str,
    bytes: &[u8],
) -> Result<Utf8PathBuf, BundleError> {
    let path = out_dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| BundleError::Write {
            path: path.clone(),
            source,
        })?;
    }
    std::fs::write(&path, bytes).map_err(|source| BundleError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

/// Live modules that are assets rather than JavaScript.
fn collect_asset_modules(graph: &ModuleGraph, shaken: &Shaken) -> Vec<Utf8PathBuf> {
    let mut assets = shaken
        .live_modules()
        .into_iter()
        .map(|index| graph.module(index).path.clone())
        .filter(|path| asset_extension(path).is_some())
        .collect::<Vec<_>>();
    assets.sort();
    assets.dedup();
    assets
}

/// How many exported names tree shaking removed.
fn dropped_exports(graph: &ModuleGraph, shaken: &Shaken) -> usize {
    graph
        .modules()
        .iter()
        .enumerate()
        .map(|(position, module)| {
            let index = crate::graph::ModuleIndex::from_position(position);
            if !shaken.is_live(index) {
                return module.record.exports.len();
            }
            let used = shaken.used(index);
            module
                .record
                .exports
                .iter()
                .filter(|export| !used.contains(&export.exported))
                .count()
        })
        .sum()
}
