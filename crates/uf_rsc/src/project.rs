//! Scanning a whole project into an [`RscAnalysis`].
//!
//! This is the entry point `uf build` and `uf dev` call. It walks the project,
//! scans every `.js` module once, resolves the graph and derives the action
//! registry.
//!
//! # Guards
//!
//! * symbolic links are never followed, so a link pointing outside the project
//!   cannot pull foreign files into the graph;
//! * files above [`ProjectScanOptions::max_file_bytes`] are skipped rather than
//!   read into memory;
//! * generated and vendored directories are skipped by name.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::RscError;
use crate::action::{BuildId, ServerActionRegistry};
use crate::graph::{EntryKind, RscGraph, RscGraphBuilder, RscModuleInput};
use crate::manifest::RscManifest;
use crate::scan::MAX_SOURCE_BYTES;

/// Directory names that never contain application modules.
pub const IGNORED_DIRECTORIES: &[&str] =
    &[".git", ".uf", "coverage", "dist", "node_modules", "target"];

/// File-name suffixes that are not part of the application graph.
pub const NON_APP_SUFFIXES: &[&str] = &[".bench.js", ".spec.js", ".stories.js", ".test.js"];

/// Reserved router files that act as server entries.
pub const ROUTER_ENTRY_FILES: &[&str] = &["_uf.layout.js", "_uf.middleware.js", "_uf.page.js"];

/// How [`analyze_project`] walks a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectScanOptions {
    /// Directory names never descended into.
    pub ignored_directories: Vec<CompactString>,
    /// File names that make a module a server entry.
    pub server_entry_files: Vec<CompactString>,
    /// Extra entry points, relative to the project root.
    pub extra_entries: Vec<(Utf8PathBuf, EntryKind)>,
    /// Largest module that will be read.
    pub max_file_bytes: usize,
}

impl Default for ProjectScanOptions {
    fn default() -> Self {
        Self {
            ignored_directories: IGNORED_DIRECTORIES
                .iter()
                .map(|name| CompactString::from(*name))
                .collect(),
            server_entry_files: ROUTER_ENTRY_FILES
                .iter()
                .map(|name| CompactString::from(*name))
                .collect(),
            extra_entries: Vec::new(),
            max_file_bytes: MAX_SOURCE_BYTES,
        }
    }
}

/// The resolved graph of a project and the actions it declares.
#[derive(Debug, Clone)]
pub struct RscAnalysis {
    /// The module graph.
    pub graph: RscGraph,
    /// The server action registry.
    pub registry: ServerActionRegistry,
}

impl RscAnalysis {
    /// Render the manifest for this analysis.
    pub fn manifest(&self) -> RscManifest {
        RscManifest::new(&self.graph, &self.registry)
    }

    /// Number of modules that end up in the client bundle as roots.
    pub fn client_bundle_root_count(&self) -> usize {
        self.graph.client_bundle_roots().len()
    }

    /// Number of actions that are callable endpoints.
    pub fn callable_action_count(&self) -> usize {
        self.registry.callable_actions().count()
    }
}

/// Scan a project rooted at `root` and resolve its RSC graph.
pub fn analyze_project(
    root: &Utf8Path,
    build_id: &BuildId,
    options: &ProjectScanOptions,
) -> Result<RscAnalysis, RscError> {
    let paths = collect_module_paths(root, options)?;

    let modules = paths
        .par_iter()
        .map(|(absolute, relative)| {
            let bytes = std::fs::read(absolute).map_err(|source| RscError::Read {
                path: relative.clone(),
                source,
            })?;
            let source = uf_infra::validate_utf8(&bytes).map_err(|_| RscError::NonUtf8Source {
                path: relative.clone(),
            })?;
            Ok(RscModuleInput::from_source(relative.clone(), source))
        })
        .collect::<Result<Vec<_>, RscError>>()?;

    let mut builder = RscGraphBuilder::new();
    for module in modules {
        let is_entry = module
            .path
            .file_name()
            .is_some_and(|name| options.server_entry_files.iter().any(|entry| entry == name));
        if is_entry {
            builder.add_entry(module.path.clone(), EntryKind::Server);
        }
        builder.add_module(module);
    }
    for (path, kind) in &options.extra_entries {
        builder.add_entry(path.clone(), *kind);
    }

    let graph = builder.build();
    let registry = ServerActionRegistry::from_graph(&graph, build_id);
    Ok(RscAnalysis { graph, registry })
}

/// Collect `(absolute, project-relative)` paths of every scannable module.
fn collect_module_paths(
    root: &Utf8Path,
    options: &ProjectScanOptions,
) -> Result<Vec<(Utf8PathBuf, Utf8PathBuf)>, RscError> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    // `WalkDir` does not follow symbolic links by default, which is what keeps a
    // link inside the project from pulling in files outside it.
    let walk = WalkDir::new(root).into_iter().filter_entry(|entry| {
        entry.depth() == 0
            || !options
                .ignored_directories
                .iter()
                .any(|ignored| ignored.as_str() == entry.file_name().to_string_lossy())
    });

    for entry in walk {
        let entry = entry.map_err(|source| RscError::Walk {
            path: root.to_path_buf(),
            source,
        })?;
        if !entry.file_type().is_file() {
            continue;
        }

        let absolute = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|path| RscError::NonUtf8Path(path.display().to_string()))?;
        let Some(name) = absolute.file_name() else {
            continue;
        };
        if !name.ends_with(".js") || NON_APP_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)) {
            continue;
        }
        if entry
            .metadata()
            .map(|metadata| metadata.len() as usize > options.max_file_bytes)
            .unwrap_or(true)
        {
            continue;
        }

        let relative = absolute
            .strip_prefix(root)
            .unwrap_or(&absolute)
            .to_path_buf();
        paths.push((absolute, relative));
    }

    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 root");
        for (path, contents) in files {
            let target = root.join(path);
            fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
            fs::write(&target, contents).expect("write");
        }
        (dir, root)
    }

    fn build_id() -> BuildId {
        BuildId::new("project-test-build-id").expect("valid build id")
    }

    #[test]
    fn a_missing_root_analyses_to_nothing() {
        let analysis = analyze_project(
            Utf8Path::new("/nonexistent/uf/project"),
            &build_id(),
            &ProjectScanOptions::default(),
        )
        .unwrap();
        assert!(analysis.graph.modules().is_empty());
    }

    #[test]
    fn a_scaffold_shaped_project_resolves_its_boundary_and_action() {
        let (_dir, root) = project(&[
            (
                "app/_uf.page.js",
                "// @flow\nimport Counter from \"./client/Counter.js\";\nimport { refreshGreeting } from \"../server/actions.js\";\n",
            ),
            (
                "app/client/Counter.js",
                "\"use client\";\n// @flow\nimport { useCounter } from \"./useCounter.js\";\n",
            ),
            (
                "app/client/useCounter.js",
                "\"use client\";\n// @flow\nimport { useState } from \"@uniflowed/react\";\n",
            ),
            (
                "server/actions.js",
                "\"use server\";\n// @flow\nexport const refreshGreeting = serverAction(async () => {});\n",
            ),
        ]);

        let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();

        assert_eq!(analysis.graph.modules().len(), 4);
        assert_eq!(analysis.graph.client_boundaries().len(), 1);
        assert_eq!(analysis.client_bundle_root_count(), 1);
        assert_eq!(analysis.callable_action_count(), 1);
        assert!(analysis.graph.diagnostics().is_empty());
    }

    #[test]
    fn ignored_directories_are_not_scanned() {
        let (_dir, root) = project(&[
            ("app/_uf.page.js", "// @flow\n"),
            ("node_modules/pkg/index.js", "\"use client\";\n"),
            ("dist/bundle.js", "\"use client\";\n"),
        ]);
        let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();
        assert_eq!(analysis.graph.modules().len(), 1);
    }

    #[test]
    fn test_files_are_not_part_of_the_app_graph() {
        let (_dir, root) = project(&[
            ("app/_uf.page.js", "// @flow\n"),
            ("app/_uf.page.test.js", "// @flow\n"),
        ]);
        let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();
        assert_eq!(analysis.graph.modules().len(), 1);
    }

    #[test]
    fn oversized_modules_are_skipped_rather_than_read() {
        let (_dir, root) = project(&[("app/_uf.page.js", "// @flow\nconst a = 1;\n")]);
        let options = ProjectScanOptions {
            max_file_bytes: 4,
            ..ProjectScanOptions::default()
        };
        let analysis = analyze_project(&root, &build_id(), &options).unwrap();
        assert!(analysis.graph.modules().is_empty());
    }

    #[test]
    fn router_files_become_server_entries() {
        let (_dir, root) = project(&[
            ("app/_uf.page.js", "import \"./data.js\";\n"),
            ("app/data.js", "// @flow\n"),
        ]);
        let analysis = analyze_project(&root, &build_id(), &ProjectScanOptions::default()).unwrap();
        assert!(
            analysis
                .graph
                .module("app/data.js")
                .unwrap()
                .reachability
                .is_reachable()
        );
    }

    #[test]
    fn extra_entries_are_honoured() {
        let (_dir, root) = project(&[("app/entry.js", "\"use client\";\n")]);
        let options = ProjectScanOptions {
            extra_entries: vec![(Utf8PathBuf::from("app/entry.js"), EntryKind::Client)],
            ..ProjectScanOptions::default()
        };
        let analysis = analyze_project(&root, &build_id(), &options).unwrap();
        assert_eq!(analysis.client_bundle_root_count(), 1);
    }

    #[test]
    fn analysing_the_same_project_twice_gives_the_same_manifest() {
        let (_dir, root) = project(&[
            ("app/_uf.page.js", "import \"./client/Counter.js\";\n"),
            ("app/client/Counter.js", "\"use client\";\n"),
        ]);
        let first = analyze_project(&root, &build_id(), &ProjectScanOptions::default())
            .unwrap()
            .manifest()
            .to_json()
            .unwrap();
        let second = analyze_project(&root, &build_id(), &ProjectScanOptions::default())
            .unwrap()
            .manifest()
            .to_json()
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_non_utf8_module_is_reported_not_ignored() {
        let (_dir, root) = project(&[("app/_uf.page.js", "// @flow\n")]);
        fs::write(root.join("app/broken.js"), [0xff, 0xfe, 0xfd]).unwrap();
        let error = analyze_project(&root, &build_id(), &ProjectScanOptions::default())
            .expect_err("invalid utf-8 must be reported");
        assert!(matches!(error, RscError::NonUtf8Source { .. }));
    }
}
