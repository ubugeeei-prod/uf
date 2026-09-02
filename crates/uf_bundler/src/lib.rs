#![deny(missing_docs)]
//! The uniflowed bundler: module graph, chunking, tree shaking, and emission.
//!
//! `uf build` discovers routes and computes the RSC boundary; this crate is
//! what turns that into JavaScript on disk. It is deliberately assembled out of
//! the crates that already own each answer rather than re-deriving any of them:
//!
//! * [`uf_plugin`] owns the hook contract and the run order, and
//!   [`pipeline::build_pipeline`] hands its six built-in descriptors the
//!   closures that do their work — so `uf inspect --json` describes the
//!   pipeline that actually runs;
//! * [`uf_flow`] owns Flow syntax, so type erasure is
//!   [`uf_flow::strip_types`] and the token scanner is [`uf_flow::scan`];
//! * [`uf_rsc`] owns the RSC contract, so reachability, client boundaries and
//!   the client-bundle roots are its answers, and so is the path guard that
//!   keeps a specifier from climbing out of the project.
//!
//! What this crate adds is the part nobody else had: [`resolve`] (subpaths,
//! `node_modules`, `package.json#exports`), [`graph`] (one worklist, no
//! recursion), [`shake`] (used exports and live modules), [`chunk`] (one chunk
//! per route, one per client boundary, one shared), and [`emit`] (ES modules
//! with real cross-chunk `import`/`export`, content-hashed names, and source
//! maps).
//!
//! # Determinism
//!
//! Building the same input twice produces byte-identical files. Nothing depends
//! on time, hash iteration order, or a global counter: chunks sort by name,
//! modules order by a post-order walk from a sorted start, namespace entries
//! sort by name, and every emitted identifier is derived from a path hash or a
//! position.
//!
//! # Bounds
//!
//! A dependency can put a hostile file in `node_modules`, so module count,
//! graph depth, file size, chunk count, specifier length, manifest size and
//! `exports` nesting each have a ceiling in [`BundlerLimits`] and a typed
//! [`LimitError`] when it is reached.
//!
//! ```
//! use camino::Utf8PathBuf;
//! use uf_bundler::{BundleOptions, BundlerLimits, bundle, pipeline::build_pipeline};
//! use uf_config::{PipelineMode, UniflowedConfig};
//!
//! let directory = tempfile::tempdir().unwrap();
//! let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).unwrap();
//! std::fs::write(root.join("app.js"), "// @flow\nexport const answer: number = 42;\n").unwrap();
//!
//! let config = UniflowedConfig::default();
//! let container = build_pipeline(&config, &root, PipelineMode::Build, &[]).unwrap();
//! let options = BundleOptions::new(&root, root.join("dist"))
//!     .with_entries(vec![Utf8PathBuf::from("app.js")])
//!     .with_limits(BundlerLimits::small());
//!
//! let output = bundle(&options, &container).unwrap();
//! assert_eq!(output.chunks.len(), 1);
//! assert!(output.chunks[0].code.contains("export const answer"));
//! assert!(!output.chunks[0].code.contains(": number"));
//! ```

use camino::Utf8PathBuf;
use thiserror::Error;

pub mod build;
pub mod chunk;
pub mod emit;
pub mod graph;
pub mod hash;
pub mod limits;
pub mod pipeline;
pub mod record;
pub mod resolve;
pub mod rolldown_backend;
pub mod shake;
pub mod sourcemap;

#[cfg(test)]
mod tests;

pub use build::{BundleOptions, BundleOutput, BundleStats, bundle, write_bundle};
pub use chunk::{Chunk, ChunkEnvironment, ChunkKind, plan_chunks};
pub use emit::{ASSET_DIR, EmittedChunk, emit_chunks};
pub use graph::{BundleModule, Edge, ModuleGraph, ModuleIndex, build_graph};
pub use hash::{HASH_HEX_LEN, hash_bytes};
pub use limits::{BundlerLimits, LimitError};
pub use pipeline::{PipelineError, build_entries, build_pipeline};
pub use record::{ModuleRecord, SideEffectKind, scan_module};
pub use resolve::{Resolution, ResolveError, Resolver};
pub use shake::{Shaken, UsedExports, shake};
pub use sourcemap::SourceMapBuilder;

/// Anything that can stop a build.
#[derive(Debug, Error)]
pub enum BundleError {
    /// Rolldown could not build or generate the bundle.
    #[error("bundling failed: {0}")]
    Rolldown(String),
    /// The async runtime Rolldown needs could not be created.
    #[error("could not start the bundler runtime: {0}")]
    Runtime(String),
    /// An entry point names a path outside the project root.
    #[error("entry {entry} is outside the project root")]
    EntryOutsideProject {
        /// The rejected entry.
        entry: Utf8PathBuf,
    },
    /// A module could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Module path, relative to the project root.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A module is not valid UTF-8.
    #[error("module {path} is not valid UTF-8")]
    NonUtf8 {
        /// Module path, relative to the project root.
        path: Utf8PathBuf,
    },
    /// An output file could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that was being written.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A specifier could not be resolved.
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    /// A ceiling was reached.
    #[error(transparent)]
    Limit(#[from] LimitError),
    /// A plugin failed, or the pipeline could not be assembled.
    #[error(transparent)]
    Plugin(#[from] uf_plugin::ContainerError),
    /// The pipeline could not be resolved from the project config.
    #[error(transparent)]
    Pipeline(#[from] PipelineError),
}
