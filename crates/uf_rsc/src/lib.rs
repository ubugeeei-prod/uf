#![deny(missing_docs)]
//! React Server Components analysis for uniflowed.
//!
//! `uf` renders every module on the server by default. A module opts into the
//! client bundle with `"use client";` and into the server action protocol with
//! `"use server";`. This crate turns those directives into the three artefacts
//! the rest of the toolchain needs:
//!
//! * [`module_environment`] — where a single module runs, with the directive
//!   rules pinned down in [`directive`];
//! * [`RscGraph`] — the resolved module graph: reachability, client boundaries,
//!   client-bundle roots, and typed [`RscDiagnostic`]s for every way an app can
//!   break the RSC contract;
//! * [`ServerActionRegistry`] — every server action behind a keyed, unguessable
//!   [`ActionId`], with a lookup that refuses to be used as an enumeration
//!   oracle.
//!
//! [`analyze_project`] runs all three over a project directory, and
//! [`RscManifest`] serializes the result to `dist/uf-rsc-manifest.json`.
//!
//! ```
//! use uf_rsc::{EntryKind, ModuleEnvironment, RscGraphBuilder, module_environment};
//!
//! assert_eq!(module_environment("\"use client\";\n"), ModuleEnvironment::Client);
//!
//! let mut builder = RscGraphBuilder::new();
//! builder.add_source("app/page.js", "import Counter from \"./Counter.js\";\n");
//! builder.add_source("app/Counter.js", "\"use client\";\n");
//! builder.add_entry("app/page.js", EntryKind::Server);
//!
//! let graph = builder.build();
//! assert_eq!(graph.client_boundaries().len(), 1);
//! ```

use camino::Utf8PathBuf;
use thiserror::Error;

pub mod action;
pub mod directive;
pub mod graph;
pub mod manifest;
pub mod project;
pub mod scan;

pub use action::{
    ACTION_ID_HEX_LEN, ActionExposure, ActionId, ActionIdError, BuildId, BuildIdError,
    MAX_BUILD_ID_BYTES, MIN_BUILD_ID_BYTES, ServerAction, ServerActionKind, ServerActionRegistry,
    UnknownAction,
};
pub use directive::{
    DirectiveIssue, DirectiveKind, DirectiveScan, FileDirective, FunctionDirective, FunctionOwner,
    ModuleEnvironment, module_environment, scan_directives,
};
pub use graph::{
    ClientBoundary, ClientBoundaryProximity, EntryKind, ModuleId, ModuleReachability,
    RscDiagnostic, RscGraph, RscGraphBuilder, RscModule, RscModuleInput, RscSeverity,
    SERVER_ONLY_PACKAGES, SERVER_ONLY_SUFFIX, is_server_only_specifier,
};
pub use manifest::{
    RSC_MANIFEST_FILE_NAME, RSC_MANIFEST_VERSION, RscManifest, RscManifestAction,
    RscManifestBoundary, RscManifestDiagnostic, RscManifestModule, write_manifest,
};
pub use project::{
    IGNORED_DIRECTORIES, NON_APP_SUFFIXES, ProjectScanOptions, ROUTER_ENTRY_FILES, RscAnalysis,
    analyze_project,
};
pub use scan::{
    CLIENT_ONLY_APIS, CLIENT_ONLY_GLOBALS, ClientApiUse, ExportKind, ImportKind, ImportSpecifier,
    MAX_SOURCE_BYTES, ModuleExport, scan_client_api_uses, scan_exports, scan_imports,
};

/// Anything that can go wrong outside the analyses themselves.
///
/// Violations of the RSC contract are [`RscDiagnostic`]s, not errors: a build
/// collects all of them instead of stopping at the first.
#[derive(Debug, Error)]
pub enum RscError {
    /// A module could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Module path, relative to the project root.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The manifest could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that was being written.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The project tree could not be walked.
    #[error("failed to walk {path}: {source}")]
    Walk {
        /// Directory that was being walked.
        path: Utf8PathBuf,
        /// Underlying walk error.
        #[source]
        source: walkdir::Error,
    },
    /// The manifest could not be serialized.
    #[error("failed to serialize the RSC manifest: {source}")]
    Serialize {
        /// Underlying serialization error.
        #[source]
        source: serde_json::Error,
    },
    /// A path in the project is not valid UTF-8.
    #[error("path is not UTF-8: {0}")]
    NonUtf8Path(String),
    /// A module is not valid UTF-8.
    #[error("module {path} is not valid UTF-8")]
    NonUtf8Source {
        /// Module path, relative to the project root.
        path: Utf8PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crate_reexports_the_public_surface() {
        assert_eq!(module_environment(""), ModuleEnvironment::Server);
        assert_eq!(RSC_MANIFEST_FILE_NAME, "uf-rsc-manifest.json");
        assert_eq!(RSC_MANIFEST_VERSION, 1);
        assert_eq!(ACTION_ID_HEX_LEN, 64);
    }

    #[test]
    fn errors_render_with_their_path() {
        let error = RscError::NonUtf8Source {
            path: Utf8PathBuf::from("app/page.js"),
        };
        assert_eq!(error.to_string(), "module app/page.js is not valid UTF-8");
    }

    #[test]
    fn a_full_analysis_flows_from_sources_to_a_manifest() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source(
            "app/_uf.page.js",
            "import Counter from \"./Counter.js\";\nimport { save } from \"../server/actions.js\";\n",
        );
        builder.add_source("app/Counter.js", "\"use client\";\n");
        builder.add_source(
            "server/actions.js",
            "\"use server\";\nexport async function save() {}\n",
        );
        builder.add_entry("app/_uf.page.js", EntryKind::Server);

        let graph = builder.build();
        let build_id = BuildId::new("lib-test-build-id").unwrap();
        let registry = ServerActionRegistry::from_graph(&graph, &build_id);
        let manifest = RscManifest::new(&graph, &registry);

        assert_eq!(manifest.modules.len(), 3);
        assert_eq!(manifest.client_boundaries.len(), 1);
        assert_eq!(manifest.server_actions.len(), 1);
        assert!(manifest.diagnostics.is_empty());
    }
}
