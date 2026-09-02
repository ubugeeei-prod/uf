//! A project on disk, bundled, for tests to make assertions about.

use camino::{Utf8Path, Utf8PathBuf};
use uf_config::{PipelineMode, UniflowedConfig};
use uf_plugin::PluginContainer;
use uf_router::Route;

use crate::{BundleError, BundleOptions, BundleOutput, BundlerLimits, build_pipeline, bundle};

/// A temporary project, its config, and the pipeline it builds through.
pub(crate) struct Fixture {
    directory: tempfile::TempDir,
    pub(crate) root: Utf8PathBuf,
    pub(crate) config: UniflowedConfig,
    pub(crate) routes: Vec<Route>,
    pub(crate) entries: Vec<Utf8PathBuf>,
    pub(crate) sourcemap: bool,
    pub(crate) limits: BundlerLimits,
}

impl Fixture {
    /// An empty project.
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let root = Utf8PathBuf::from_path_buf(
            directory
                .path()
                .canonicalize()
                .expect("canonical temporary directory"),
        )
        .expect("temporary directory is UTF-8");

        Self {
            directory,
            root,
            config: UniflowedConfig::default(),
            routes: Vec::new(),
            entries: Vec::new(),
            sourcemap: false,
            limits: BundlerLimits::small(),
        }
    }

    /// Write a file, creating its parent directories.
    pub(crate) fn write(&self, relative: &str, contents: &str) -> &Self {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent directory");
        }
        std::fs::write(&path, contents).expect("write fixture file");
        self
    }

    /// Write a file and make it an entry point.
    pub(crate) fn entry(&mut self, relative: &str, contents: &str) -> &mut Self {
        self.write(relative, contents);
        self.entries.push(Utf8PathBuf::from(relative));
        self
    }

    /// Turn source maps on.
    pub(crate) fn with_sourcemap(&mut self) -> &mut Self {
        self.sourcemap = true;
        self
    }

    /// The path a fixture-relative name has on disk.
    pub(crate) fn path(&self, relative: &str) -> Utf8PathBuf {
        self.root.join(relative)
    }

    /// The output directory, which is never created unless a test writes it.
    pub(crate) fn out_dir(&self) -> Utf8PathBuf {
        self.root.join("dist")
    }

    /// The pipeline this project builds through.
    pub(crate) fn container(&self) -> PluginContainer {
        build_pipeline(&self.config, &self.root, PipelineMode::Build, &self.routes)
            .expect("pipeline resolves")
    }

    /// The options this project builds with.
    pub(crate) fn options(&self) -> BundleOptions {
        BundleOptions::new(self.root.clone(), self.out_dir())
            .with_entries(self.entries.clone())
            .with_sourcemap(self.sourcemap)
            .with_limits(self.limits)
    }

    /// Build the project, expecting it to succeed.
    pub(crate) fn bundle(&self) -> BundleOutput {
        self.try_bundle().expect("bundle succeeds")
    }

    /// Build the project.
    pub(crate) fn try_bundle(&self) -> Result<BundleOutput, BundleError> {
        bundle(&self.options(), &self.container())
    }

    /// Keep the temporary directory alive for the whole test.
    pub(crate) fn keep(&self) -> &Utf8Path {
        let _ = &self.directory;
        &self.root
    }
}

/// The chunk whose logical name starts with `prefix`.
pub(crate) fn chunk_named<'a>(output: &'a BundleOutput, prefix: &str) -> &'a crate::EmittedChunk {
    output
        .chunks
        .iter()
        .find(|chunk| chunk.file_name.starts_with(&format!("assets/{prefix}")))
        .unwrap_or_else(|| {
            panic!(
                "no chunk named {prefix}; have {:?}",
                output
                    .chunks
                    .iter()
                    .map(|chunk| chunk.file_name.as_str())
                    .collect::<Vec<_>>()
            )
        })
}

/// Assert every emitted chunk parses as JavaScript.
pub(crate) fn assert_chunks_parse(output: &BundleOutput) {
    for chunk in &output.chunks {
        let outcome = uf_flow::validate_source(&chunk.code).expect("parser ran");
        assert!(
            outcome.is_ok(),
            "chunk {} does not parse: {:?}\n{}",
            chunk.file_name,
            outcome.diagnostics,
            chunk.code
        );
    }
}
