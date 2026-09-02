//! Every ceiling the bundler enforces, in one place.
//!
//! A build reads whatever `node_modules` contains, and a dependency can put a
//! hostile file there: a two-million-line generated module, an import chain a
//! thousand deep, a package that re-exports itself. Each of those is bounded
//! here with a typed refusal rather than by whatever the machine runs out of
//! first.

use thiserror::Error;

use camino::Utf8PathBuf;
use compact_str::CompactString;

/// The bounds one build runs under.
///
/// Defaults are generous enough for any real application and small enough that
/// exceeding one is a bug or an attack, never a large project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundlerLimits {
    /// Most modules one build will pull into the graph.
    pub max_modules: usize,
    /// Deepest import chain from an entry to a module.
    pub max_depth: u32,
    /// Largest module the bundler will read, in bytes.
    pub max_module_bytes: u64,
    /// Most chunks one build will emit.
    pub max_chunks: usize,
    /// Longest import specifier the resolver will look at, in bytes.
    pub max_specifier_bytes: usize,
    /// How far up the directory tree a `node_modules` lookup will walk.
    pub max_node_modules_depth: usize,
    /// Largest `package.json` the resolver will read, in bytes.
    pub max_manifest_bytes: u64,
    /// Deepest nesting an `exports` map may use.
    pub max_exports_depth: usize,
}

impl Default for BundlerLimits {
    fn default() -> Self {
        Self {
            max_modules: 100_000,
            max_depth: 512,
            max_module_bytes: 8 * 1024 * 1024,
            max_chunks: 10_000,
            max_specifier_bytes: 1024,
            max_node_modules_depth: 64,
            max_manifest_bytes: 4 * 1024 * 1024,
            max_exports_depth: 16,
        }
    }
}

impl BundlerLimits {
    /// Limits scaled for a small fixture or a unit test.
    #[must_use]
    pub const fn small() -> Self {
        Self {
            max_modules: 256,
            max_depth: 32,
            max_module_bytes: 64 * 1024,
            max_chunks: 64,
            max_specifier_bytes: 256,
            max_node_modules_depth: 8,
            max_manifest_bytes: 64 * 1024,
            max_exports_depth: 8,
        }
    }
}

/// A ceiling a build ran into.
///
/// Every variant names the limit it broke as well as the observed value, so a
/// failure says which knob to turn rather than only that something was too big.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LimitError {
    /// The graph grew past [`BundlerLimits::max_modules`].
    #[error("build reached {count} modules, over the ceiling of {limit}")]
    TooManyModules {
        /// Modules in the graph when the ceiling was hit.
        count: usize,
        /// The ceiling.
        limit: usize,
    },
    /// An import chain grew past [`BundlerLimits::max_depth`].
    #[error("module {module} sits {depth} imports deep, over the ceiling of {limit}")]
    GraphTooDeep {
        /// The module that broke the ceiling.
        module: Utf8PathBuf,
        /// Its distance from the nearest entry.
        depth: u32,
        /// The ceiling.
        limit: u32,
    },
    /// A module is larger than [`BundlerLimits::max_module_bytes`].
    #[error("module {module} is {bytes} bytes, over the ceiling of {limit}")]
    ModuleTooLarge {
        /// The oversized module.
        module: Utf8PathBuf,
        /// Its size on disk.
        bytes: u64,
        /// The ceiling.
        limit: u64,
    },
    /// The build wanted more chunks than [`BundlerLimits::max_chunks`].
    #[error("build produced {count} chunks, over the ceiling of {limit}")]
    TooManyChunks {
        /// Chunks the build wanted to emit.
        count: usize,
        /// The ceiling.
        limit: usize,
    },
    /// A specifier is longer than [`BundlerLimits::max_specifier_bytes`].
    #[error("import specifier is {bytes} bytes, over the ceiling of {limit}")]
    SpecifierTooLong {
        /// Length of the rejected specifier.
        bytes: usize,
        /// The ceiling.
        limit: usize,
    },
    /// A `package.json` is larger than [`BundlerLimits::max_manifest_bytes`].
    #[error("package manifest {manifest} is {bytes} bytes, over the ceiling of {limit}")]
    ManifestTooLarge {
        /// The oversized manifest.
        manifest: Utf8PathBuf,
        /// Its size on disk.
        bytes: u64,
        /// The ceiling.
        limit: u64,
    },
    /// An `exports` map nests deeper than [`BundlerLimits::max_exports_depth`].
    #[error("exports map of {package} nests deeper than {limit}")]
    ExportsTooDeep {
        /// The package whose manifest was refused.
        package: CompactString,
        /// The ceiling.
        limit: usize,
    },
}
