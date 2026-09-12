//! The `.uf/build/meta/uf-rsc-manifest.json` build artefact.
//!
//! The manifest is what `uf build` hands to the bundler and what `uf dev` reads
//! back, so it is deterministic by construction: every collection is sorted, and
//! two builds of the same sources with the same build id produce byte-identical
//! JSON.
//!
//! Two things are deliberately absent:
//!
//! * the build id, which is the HMAC key behind every action id — only a
//!   [`BuildId::fingerprint`] derived from it is published;
//! * actions no client boundary can reach, which are tracked in the registry but
//!   never written here, so nothing downstream can turn one into an endpoint.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::RscError;
use crate::action::{ActionId, ServerActionKind, ServerActionRegistry};
use crate::directive::ModuleEnvironment;
use crate::graph::{
    ClientBoundaryProximity, ClientBoundaryTarget, ModuleReachability, RscGraph, RscSeverity,
};

/// File name of the manifest inside the build output directory.
pub const RSC_MANIFEST_FILE_NAME: &str = "uf-rsc-manifest.json";

/// Directory, relative to the project root, the bundler reads the manifest from.
///
/// The split the bundler performs needs the analysis *before* Vite emits
/// anything, and `uf dev` recomputes it on every edit — so this copy is the
/// bundler's input, written before the build runs and rewritten by the dev
/// server, and its path is handed to the driver in [`RSC_MANIFEST_ENV`]. The
/// build writes the same bytes a second time when it is over, into its own
/// metadata directory, so that the record of a finished build is not the file
/// the next `uf dev` overwrites.
pub const RSC_MANIFEST_BUILD_DIR: &str = ".uf/rsc";

/// Environment variable naming the manifest the bundler must read.
///
/// Handed to `@uniflowed/vite`'s driver the way `UF_BINARY` is: the plugin has
/// no way to run the analysis itself, and a path it guessed could be a manifest
/// some older build left behind. Absent, the plugin performs no split at all
/// and every route keeps its page — which is what a project driving Vite
/// directly, without `uf build` or `uf dev`, gets.
pub const RSC_MANIFEST_ENV: &str = "UF_RSC_MANIFEST";

/// Schema version of the manifest.
///
/// 3 lets client boundaries and bundle roots name package specifiers as well
/// as project paths. Version 2 could only name scanned files, so a reader could
/// not represent a package client module without pretending it was a path.
pub const RSC_MANIFEST_VERSION: u32 = 3;

/// The serialized React Server Components manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RscManifest {
    /// Schema version.
    pub version: u32,
    /// Engine that produced the manifest.
    pub engine: CompactString,
    /// Digest of the build id; the build id itself is never published.
    pub build_fingerprint: CompactString,
    /// Every module of the graph, ordered by path.
    pub modules: Vec<RscManifestModule>,
    /// Server-to-client import edges, ordered.
    pub client_boundaries: Vec<RscManifestBoundary>,
    /// Client bundle roots, ordered.
    pub client_bundle_roots: Vec<RscManifestClientReference>,
    /// Callable server actions, ordered by id.
    pub server_actions: Vec<RscManifestAction>,
    /// Contract violations, ordered.
    pub diagnostics: Vec<RscManifestDiagnostic>,
}

/// One module in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RscManifestModule {
    /// Path relative to the project root.
    pub path: Utf8PathBuf,
    /// Environment the module executes in.
    pub environment: ModuleEnvironment,
    /// Which halves of the app reach it.
    pub reachability: ModuleReachability,
    /// Whether this module, or something it imports, crosses a client boundary.
    ///
    /// The bundler's question: a route whose page and layouts are all
    /// `isolated` renders nothing the browser will ever re-render, so its page
    /// is left out of the client route table.
    pub proximity: ClientBoundaryProximity,
    /// Resolved internal imports, ordered.
    pub imports: Vec<Utf8PathBuf>,
    /// Unresolved specifiers, ordered.
    pub external_imports: Vec<CompactString>,
    /// Exported names, ordered.
    pub exports: Vec<CompactString>,
}

/// One server-to-client import edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RscManifestBoundary {
    /// The server module that owns the import.
    pub importer: Utf8PathBuf,
    /// The client module it imports.
    pub target: RscManifestClientReference,
}

/// A client module named by the manifest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RscManifestClientReference {
    /// A project module, path-relative to the project root.
    Module {
        /// Path relative to the project root.
        path: Utf8PathBuf,
    },
    /// A package module, named by the exact imported specifier.
    Package {
        /// Imported bare specifier.
        specifier: CompactString,
    },
}

/// One callable server action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RscManifestAction {
    /// Keyed action id.
    pub id: ActionId,
    /// Declaring module.
    pub module: Utf8PathBuf,
    /// Export name or closure binding.
    pub export: CompactString,
    /// Where the action was declared.
    pub kind: ServerActionKind,
}

/// One reported contract violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RscManifestDiagnostic {
    /// Stable rule identifier.
    pub rule: CompactString,
    /// How serious it is.
    pub severity: RscSeverity,
    /// Module the violation belongs to.
    pub module: Utf8PathBuf,
    /// 1-based line, or 0 when the violation is not tied to one.
    pub line: u32,
    /// Human-readable rendering of the typed diagnostic.
    pub message: CompactString,
}

impl RscManifest {
    /// Build the manifest from a resolved graph and its action registry.
    pub fn new(graph: &RscGraph, registry: &ServerActionRegistry) -> Self {
        let modules = graph
            .modules()
            .iter()
            .map(|module| {
                let mut imports: Vec<Utf8PathBuf> = module
                    .imports
                    .iter()
                    .filter_map(|id| graph.module_by_id(*id))
                    .map(|imported| imported.path.clone())
                    .collect();
                imports.sort();
                imports.dedup();

                let mut external_imports: Vec<CompactString> =
                    module.external_imports.iter().cloned().collect();
                external_imports.sort();
                external_imports.dedup();

                let mut exports: Vec<CompactString> = module
                    .exports
                    .iter()
                    .map(|export| export.name.clone())
                    .collect();
                exports.sort();
                exports.dedup();

                RscManifestModule {
                    path: module.path.clone(),
                    environment: module.environment,
                    reachability: module.reachability,
                    proximity: module.proximity,
                    imports,
                    external_imports,
                    exports,
                }
            })
            .collect();

        let mut client_boundaries: Vec<RscManifestBoundary> = graph
            .client_boundaries()
            .iter()
            .filter_map(|boundary| {
                Some(RscManifestBoundary {
                    importer: graph.module_by_id(boundary.importer)?.path.clone(),
                    target: manifest_client_reference(graph, &boundary.target)?,
                })
            })
            .collect();
        client_boundaries.sort_by(|left, right| {
            left.importer
                .cmp(&right.importer)
                .then(left.target.cmp(&right.target))
        });
        client_boundaries.dedup();

        let mut client_bundle_roots: Vec<RscManifestClientReference> = graph
            .client_bundle_roots()
            .iter()
            .filter_map(|target| manifest_client_reference(graph, target))
            .collect();
        client_bundle_roots.sort();
        client_bundle_roots.dedup();

        let mut server_actions: Vec<RscManifestAction> = registry
            .callable_actions()
            .map(|action| RscManifestAction {
                id: action.id,
                module: action.module.clone(),
                export: action.export.clone(),
                kind: action.kind,
            })
            .collect();
        server_actions.sort_by_key(|action| action.id);

        let mut diagnostics: Vec<RscManifestDiagnostic> = graph
            .diagnostics()
            .iter()
            .map(|diagnostic| RscManifestDiagnostic {
                rule: CompactString::from(diagnostic.rule()),
                severity: diagnostic.severity(),
                module: diagnostic.module().to_path_buf(),
                line: diagnostic.line(),
                message: CompactString::from(diagnostic.to_string()),
            })
            .collect();
        diagnostics.sort_by(|left, right| {
            left.module
                .cmp(&right.module)
                .then(left.line.cmp(&right.line))
                .then(left.rule.cmp(&right.rule))
                .then(left.message.cmp(&right.message))
        });
        diagnostics.dedup();

        Self {
            version: RSC_MANIFEST_VERSION,
            engine: CompactString::const_new("uf-native"),
            build_fingerprint: CompactString::from(registry.build_fingerprint()),
            modules,
            client_boundaries,
            client_bundle_roots,
            server_actions,
            diagnostics,
        }
    }

    /// Render the manifest exactly as it is written to disk.
    pub fn to_json(&self) -> Result<String, RscError> {
        let mut json =
            serde_json::to_string_pretty(self).map_err(|source| RscError::Serialize { source })?;
        json.push('\n');
        Ok(json)
    }
}

fn manifest_client_reference(
    graph: &RscGraph,
    target: &ClientBoundaryTarget,
) -> Option<RscManifestClientReference> {
    match target {
        ClientBoundaryTarget::Module(id) => {
            graph
                .module_by_id(*id)
                .map(|module| RscManifestClientReference::Module {
                    path: module.path.clone(),
                })
        }
        ClientBoundaryTarget::Package(specifier) => Some(RscManifestClientReference::Package {
            specifier: specifier.clone(),
        }),
    }
}

/// Write `uf-rsc-manifest.json` into `dir`, creating it if it is not there.
///
/// Never the build output directory: the manifest names every module in the
/// application and which of them the browser is handed, and the output
/// directory is served to whoever asks. Both callers pass a directory under
/// `.uf/` — [`RSC_MANIFEST_BUILD_DIR`] for the bundler's copy, and `uf build`'s
/// own metadata directory for the record of the build. See
/// ubugeeei-prod/uf#339.
pub fn write_manifest(dir: &Utf8Path, manifest: &RscManifest) -> Result<Utf8PathBuf, RscError> {
    fs::create_dir_all(dir).map_err(|source| RscError::Write {
        path: dir.to_path_buf(),
        source,
    })?;
    let path = dir.join(RSC_MANIFEST_FILE_NAME);
    fs::write(&path, manifest.to_json()?).map_err(|source| RscError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests;
