//! Writing chunks out as ES modules.
//!
//! # Shape of a chunk
//!
//! A chunk is a real ES module. It imports from the packages it does not bundle
//! and from the other chunks it depends on, wraps each of its own modules in an
//! immediately-invoked function, and exports those wrappers under a symbol
//! derived from the module's path:
//!
//! ```js
//! // uf chunk: entry-app
//! import { useState as __uf_x0 } from "react";
//! import { uf_9f8e7d6c as __uf_c0 } from "./shared-client-a1b2c3d4.js";
//!
//! function __uf_init0() { … }
//! const __uf_m0 = __uf_init0();
//!
//! export { __uf_m0 as uf_1a2b3c4d };
//! export default __uf_m0["default"];
//! ```
//!
//! # Determinism
//!
//! Building the same input twice must produce byte-identical files, so nothing
//! here depends on iteration order, time, or a counter that survives a build.
//! Chunks are sorted by name, modules within a chunk are ordered by an
//! iterative post-order walk from a sorted start, namespace entries are sorted,
//! and every identifier is derived from a path hash or a position. The content
//! hash in a file name covers the chunk's modules and the *logical* names of
//! the chunks it depends on, so it is well defined even when two chunks import
//! each other.

use camino::Utf8PathBuf;
use compact_str::CompactString;
use uf_infra::{FxHashMap, FxHashSet};

use crate::chunk::{Chunk, ChunkEnvironment, ChunkKind};
use crate::graph::{Edge, ModuleGraph, ModuleIndex};
use crate::hash::ContentHasher;
use crate::shake::Shaken;
use crate::sourcemap::{LineOrigin, SourceMapBuilder};

mod body;
mod reference;

pub use body::{ModuleBody, apply_patches, render_module};
pub use reference::{
    CROSS_PREFIX, EXTERNAL_PREFIX, ExternalImport, ExternalKind, INIT_PREFIX, MODULE_PREFIX,
    References, STAR_HELPER, SYMBOL_PREFIX, is_identifier, module_symbol, quote,
};

/// Directory emitted chunks are written into, under the build output directory.
pub const ASSET_DIR: &str = "assets";

/// The helper a chunk defines when one of its modules uses `export * from`.
///
/// `export *` republishes every name except `default`, so the helper drops it
/// rather than letting one module's default export shadow another's.
const STAR_HELPER_SOURCE: &str =
    "const __uf_star = (source) => { const { default: _d, ...rest } = source; return rest; };";

/// One chunk, rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedChunk {
    /// Path relative to the build output directory.
    pub file_name: CompactString,
    /// Why the chunk exists.
    pub kind: ChunkKind,
    /// Which half of the app loads it.
    pub environment: ChunkEnvironment,
    /// The modules it holds, in evaluation order.
    pub modules: Vec<Utf8PathBuf>,
    /// File names of the other chunks it imports from, sorted.
    pub imports: Vec<CompactString>,
    /// The JavaScript.
    pub code: String,
    /// The source map, when the build asked for one.
    pub source_map: Option<String>,
}

impl EmittedChunk {
    /// Path of the source map beside the chunk, when there is one.
    #[must_use]
    pub fn source_map_file_name(&self) -> Option<CompactString> {
        self.source_map
            .as_ref()
            .map(|_| CompactString::new(format!("{}.map", self.file_name)))
    }
}

/// Render every chunk of a build.
#[must_use]
pub fn emit_chunks(
    graph: &ModuleGraph,
    shaken: &Shaken,
    chunks: &[Chunk],
    sourcemap: bool,
) -> Vec<EmittedChunk> {
    let chunk_of = chunk_assignment(chunks);
    let exported = exported_modules(graph, chunks, &chunk_of);
    let file_names = file_names(graph, chunks, &chunk_of);

    chunks
        .iter()
        .enumerate()
        .map(|(position, chunk)| {
            render_chunk(
                graph,
                shaken,
                chunk,
                position,
                &chunk_of,
                &exported,
                &file_names,
                sourcemap,
            )
        })
        .collect()
}

/// Which chunk each module belongs to, by module position.
fn chunk_assignment(chunks: &[Chunk]) -> FxHashMap<usize, usize> {
    let mut assignment = FxHashMap::default();
    for (position, chunk) in chunks.iter().enumerate() {
        for module in &chunk.modules {
            assignment.insert(module.get(), position);
        }
    }
    assignment
}

/// Modules another chunk reaches, and which therefore need a chunk export.
fn exported_modules(
    graph: &ModuleGraph,
    chunks: &[Chunk],
    chunk_of: &FxHashMap<usize, usize>,
) -> FxHashSet<ModuleIndex> {
    let mut exported = FxHashSet::default();

    for (position, chunk) in chunks.iter().enumerate() {
        for index in &chunk.modules {
            let module = graph.module(*index);
            for (import, edge) in module.edges.iter().enumerate() {
                let Edge::Module(target) = edge else {
                    continue;
                };
                if !module.record.imports[import].form.is_linked() {
                    continue;
                }
                if chunk_of.get(&target.get()).copied() != Some(position) {
                    exported.insert(*target);
                }
            }
        }
    }

    exported
}

/// The output path of every chunk, hash included.
fn file_names(
    graph: &ModuleGraph,
    chunks: &[Chunk],
    chunk_of: &FxHashMap<usize, usize>,
) -> Vec<CompactString> {
    chunks
        .iter()
        .enumerate()
        .map(|(position, chunk)| {
            let mut hasher = ContentHasher::new();
            hasher.field(chunk.name.as_bytes());
            hasher.field(chunk.kind.as_str().as_bytes());
            hasher.field(chunk.environment.as_str().as_bytes());
            for index in &chunk.modules {
                let module = graph.module(*index);
                hasher.field(module.path.as_str().as_bytes());
                hasher.field(module.code.as_bytes());
            }
            for name in dependency_names(graph, chunks, position, chunk_of) {
                hasher.field(name.as_bytes());
            }
            CompactString::new(format!("{ASSET_DIR}/{}-{}.js", chunk.name, hasher.finish()))
        })
        .collect()
}

/// The logical names of the chunks a chunk depends on, sorted.
fn dependency_names(
    graph: &ModuleGraph,
    chunks: &[Chunk],
    position: usize,
    chunk_of: &FxHashMap<usize, usize>,
) -> Vec<CompactString> {
    let mut names = Vec::new();
    for index in &chunks[position].modules {
        for edge in &graph.module(*index).edges {
            let Edge::Module(target) = edge else {
                continue;
            };
            let Some(other) = chunk_of.get(&target.get()).copied() else {
                continue;
            };
            if other != position {
                names.push(chunks[other].name.clone());
            }
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

/// Assemble one chunk's text and source map.
#[allow(clippy::too_many_arguments)]
fn render_chunk(
    graph: &ModuleGraph,
    shaken: &Shaken,
    chunk: &Chunk,
    position: usize,
    chunk_of: &FxHashMap<usize, usize>,
    exported: &FxHashSet<ModuleIndex>,
    file_names: &[CompactString],
    sourcemap: bool,
) -> EmittedChunk {
    let mut references = References::with_locals(&chunk.modules);
    let bodies = chunk
        .modules
        .iter()
        .map(|index| {
            render_module(
                graph.module(*index),
                shaken.used(*index),
                &mut references,
                chunk_of,
            )
        })
        .collect::<Vec<_>>();

    let mut writer = Writer::new();
    writer.line(&format!("// uf chunk: {}", chunk.name));

    for external in references.externals() {
        writer.raw(&external.render());
    }
    for (other, module) in references.cross_imports() {
        let alias = references.cross_alias(*module).cloned().unwrap_or_default();
        let symbol = module_symbol(&graph.module(*module).path);
        let from = base_name(&file_names[*other]);
        writer.line(&format!(
            "import {{ {symbol} as {alias} }} from {};",
            quote(&format!("./{from}"))
        ));
    }
    if bodies.iter().any(|body| body.needs_star_helper) {
        writer.line(STAR_HELPER_SOURCE);
    }

    for (slot, (index, body)) in chunk.modules.iter().zip(&bodies).enumerate() {
        let module = graph.module(*index);
        let source = writer.map.add_source(module.path.as_str(), &module.source);
        writer.blank();
        writer.line(&format!("function {INIT_PREFIX}{slot}() {{"));
        for binding in &body.bindings {
            writer.raw(binding);
        }
        writer.mapped(&body.code, source, sourcemap);
        writer.raw(&body.namespace);
        writer.line("}");
        writer.line(&format!(
            "const {MODULE_PREFIX}{slot} = {INIT_PREFIX}{slot}();"
        ));
    }

    writer.blank();
    for (slot, index) in chunk.modules.iter().enumerate() {
        if !exported.contains(index) {
            continue;
        }
        writer.line(&format!(
            "export {{ {MODULE_PREFIX}{slot} as {} }};",
            module_symbol(&graph.module(*index).path)
        ));
    }
    render_root_exports(graph, shaken, chunk, &mut writer);

    let file_name = file_names[position].clone();
    let source_map = sourcemap.then(|| writer.map.clone().finish(&base_name(&file_name)));
    let mut code = writer.out;
    if source_map.is_some() {
        code.push_str(&format!(
            "//# sourceMappingURL={}.map\n",
            base_name(&file_name)
        ));
    }

    let mut imports = references
        .cross_imports()
        .iter()
        .map(|(other, _)| file_names[*other].clone())
        .collect::<Vec<_>>();
    imports.sort();
    imports.dedup();

    EmittedChunk {
        file_name,
        kind: chunk.kind,
        environment: chunk.environment,
        modules: chunk
            .modules
            .iter()
            .map(|index| graph.module(*index).path.clone())
            .collect(),
        imports,
        code,
        source_map,
    }
}

/// Re-export an entry or client root's own names from its chunk.
fn render_root_exports(graph: &ModuleGraph, shaken: &Shaken, chunk: &Chunk, writer: &mut Writer) {
    let root = match chunk.kind {
        ChunkKind::Entry { module } | ChunkKind::Client { module } => module,
        ChunkKind::Shared => return,
    };
    let Some(slot) = chunk.modules.iter().position(|index| *index == root) else {
        return;
    };

    let module = graph.module(root);
    let used = shaken.used(root);
    let mut names = module
        .record
        .exports
        .iter()
        .map(|export| export.exported.as_str())
        .filter(|name| used.contains(name))
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();

    for name in names {
        if name == "default" {
            writer.line(&format!(
                "export default {MODULE_PREFIX}{slot}[\"default\"];"
            ));
        } else if is_identifier(name) {
            writer.line(&format!(
                "export const {name} = {MODULE_PREFIX}{slot}[{}];",
                quote(name)
            ));
        }
    }
}

/// The file name of a chunk path, without its directory.
fn base_name(file_name: &str) -> String {
    file_name
        .rsplit('/')
        .next()
        .unwrap_or(file_name)
        .to_string()
}

/// Appends lines while keeping the source map's line table in step.
struct Writer {
    out: String,
    map: SourceMapBuilder,
}

impl Writer {
    fn new() -> Self {
        Self {
            out: String::new(),
            map: SourceMapBuilder::new(),
        }
    }

    /// One generated line, with the newline supplied here.
    fn line(&mut self, text: &str) {
        self.out.push_str(text);
        self.out.push('\n');
        self.map.generated_line();
    }

    /// An empty generated line.
    fn blank(&mut self) {
        self.line("");
    }

    /// Text that already carries its own line terminators.
    fn raw(&mut self, text: &str) {
        for _ in 0..line_count(text) {
            self.map.generated_line();
        }
        self.out.push_str(text);
        if !text.ends_with('\n') {
            self.out.push('\n');
        }
    }

    /// A module's code, one output line per source line.
    fn mapped(&mut self, code: &str, source: u32, sourcemap: bool) {
        let mut lines = code.split('\n').collect::<Vec<_>>();
        if code.ends_with('\n') {
            lines.pop();
        }
        for (offset, line) in lines.into_iter().enumerate() {
            self.out.push_str(line);
            self.out.push('\n');
            if sourcemap {
                self.map.mapped_line(LineOrigin {
                    source,
                    line: offset as u32,
                });
            } else {
                self.map.generated_line();
            }
        }
    }
}

/// How many lines a string that carries its own terminators occupies.
fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let breaks = text.bytes().filter(|byte| *byte == b'\n').count();
    if text.ends_with('\n') {
        breaks
    } else {
        breaks + 1
    }
}
