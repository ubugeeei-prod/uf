//! Bundling through Rolldown.
//!
//! Rolldown owns module resolution, the module graph, tree shaking, chunking,
//! hashing, source maps and rendering — everything a bundler does that is not
//! specific to Flow. uf contributes only what is: erasing Flow types, lowering
//! JSX, blanking the RSC directive prologue, serving the generated route table,
//! and turning a non-JavaScript import into a URL module. Those ride Rolldown's
//! plugin hooks, which are the Rollup/Vite contract `uf.config.js` plugins
//! already speak. The user never sees any of it: `uf.config.js` stays the only
//! surface.
//!
//! This output type is deliberately not [`crate::EmittedChunk`]. That one
//! describes a slice of uf's own module graph — `ChunkKind::Entry` carries a
//! `ModuleIndex` into it — and Rolldown has no such index because it built its
//! own graph. Describing Rolldown's output in terms of a graph uf no longer
//! builds would be a translation layer that lies.

// Rolldown's plugin list is `Vec<SharedPluginable>`, and `SharedPluginable` is
// `Arc<dyn Pluginable>` — the reference counting is the dependency's API, not a
// choice this module makes. It stops here: no `Arc` reaches this crate's public
// types, which are plain owned data.
#![allow(
    clippy::disallowed_types,
    reason = "Rolldown registers plugins as `Arc<dyn Pluginable>`"
)]

mod plugins;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use camino::Utf8PathBuf;
use compact_str::{CompactString, ToCompactString};
use rolldown::{Bundler, BundlerBuilder, BundlerOptions, InputItem, OutputFormat};
use rolldown_common::Output;
use uf_config::UniflowedConfig;
use uf_jsx::JsxOptions;
use uf_router::Route;

use crate::{BundleError, BundleOptions};

/// One chunk Rolldown emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolldownChunk {
    /// Path relative to the build output directory.
    pub file_name: CompactString,
    /// Whether this chunk is one of the build's entries.
    pub is_entry: bool,
    /// Modules it holds, as Rolldown identified them.
    pub modules: Vec<Utf8PathBuf>,
    /// File names of the other chunks it imports from.
    pub imports: Vec<CompactString>,
    /// The JavaScript.
    pub code: String,
}

/// Everything a Rolldown build produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RolldownBundle {
    /// Emitted chunks, sorted by file name.
    pub chunks: Vec<RolldownChunk>,
}

impl RolldownBundle {
    /// Total rendered bytes across every chunk.
    #[must_use]
    pub fn code_size(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.code.len()).sum()
    }

    /// The chunk with `file_name`, if it was emitted.
    #[must_use]
    pub fn chunk(&self, file_name: &str) -> Option<&RolldownChunk> {
        self.chunks
            .iter()
            .find(|chunk| chunk.file_name == file_name)
    }
}

/// Bundle a project with Rolldown, running uf's stages as Rolldown plugins.
pub fn bundle(
    options: &BundleOptions,
    config: &UniflowedConfig,
    routes: &[Route],
) -> Result<RolldownBundle, BundleError> {
    // Rolldown is async and `uf build` is not. One runtime per build, torn down
    // with it, rather than a process-wide one nothing owns.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|source| BundleError::Runtime(source.to_string()))?;

    runtime.block_on(bundle_async(options, config, routes))
}

async fn bundle_async(
    options: &BundleOptions,
    config: &UniflowedConfig,
    routes: &[Route],
) -> Result<RolldownBundle, BundleError> {
    let input = options
        .entries
        .iter()
        .map(|entry| InputItem {
            name: Some(entry.file_stem().unwrap_or("entry").to_string()),
            import: options.root.join(entry).to_string(),
        })
        .collect::<Vec<_>>();

    let mut bundler: Bundler = BundlerBuilder::default()
        .with_options(BundlerOptions {
            input: Some(input),
            cwd: Some(options.root.clone().into_std_path_buf()),
            dir: Some(options.out_dir.to_string()),
            format: Some(OutputFormat::Esm),
            ..Default::default()
        })
        .with_plugins(vec![
            Arc::new(plugins::RouterPlugin {
                table: crate::pipeline::route_table(routes),
            }),
            Arc::new(plugins::AssetPlugin {
                root: options.root.clone(),
            }),
            Arc::new(plugins::FlowPlugin),
            Arc::new(plugins::RscPlugin),
            Arc::new(plugins::JsxPlugin {
                options: JsxOptions::from_config(config),
            }),
        ])
        .build()
        .map_err(|errors| BundleError::Rolldown(format!("{errors:?}")))?;

    let output = bundler
        .generate()
        .await
        .map_err(|errors| BundleError::Rolldown(format!("{errors:?}")))?;

    let mut chunks = output
        .assets
        .iter()
        .filter_map(|asset| match asset {
            Output::Chunk(chunk) => Some(RolldownChunk {
                file_name: chunk.filename.as_str().to_compact_string(),
                is_entry: chunk.is_entry,
                // Rolldown sorts these by execution order, which is the order
                // the chunk evaluates them in.
                modules: chunk
                    .modules
                    .keys
                    .iter()
                    .map(|id| Utf8PathBuf::from(id.to_string()))
                    .collect(),
                imports: chunk
                    .imports
                    .iter()
                    .map(|import| import.as_str().to_compact_string())
                    .collect(),
                code: chunk.code.to_string(),
            }),
            Output::Asset(_) => None,
        })
        .collect::<Vec<_>>();
    chunks.sort_by(|left, right| left.file_name.cmp(&right.file_name));

    Ok(RolldownBundle { chunks })
}
