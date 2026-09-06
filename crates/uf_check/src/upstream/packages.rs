//! Turning a package's published name into a file in the same batch.
//!
//! A uf repository is a workspace: `packages/cell` publishes as
//! `@uniflowed/cell`, `uf install` links it into `node_modules/@uniflowed/`,
//! and every file that uses it — the documentation site, the library test
//! suite, another package — imports it by that published name rather than by
//! the path it happens to live at. Before this module a bare specifier
//! resolved to nothing typed, so `import type { Cell } from "@uniflowed/cell"`
//! made `Cell` an `any`-typed value and every inference that rested on it
//! rested on nothing. That is ubugeeei-prod/uf#248.
//!
//! # Where the answer comes from
//!
//! The batch already contains the answer. `uf check` collects `package.json`
//! alongside the Flow sources — `uf_project`'s `SourceKind::PackageManifest`,
//! which `uf_lint` reads — so the manifest that says *this directory is
//! `@uniflowed/cell`, and its `.` export is `./index.js`* is one of the
//! sources this check was handed. Nothing here touches the filesystem, and
//! `uf_check` stays a function of the sources it is given.
//!
//! # What is resolved, and what is not
//!
//! The `exports` map decides, and it is upstream's implementation of it that
//! decides: `PackageExports::resolve_package` is the same code Flow's own node
//! resolver calls, so conditions, nested conditions, `null` targets and `*`
//! patterns behave here exactly as they do in `flow check`. A package with no
//! `exports` map falls back to `main` and then to the directory's `index`,
//! which is Node's own order.
//!
//! The wrong answer, tried first, was to map the last segment of the name onto
//! `packages/<segment>/index.js`. It resolves `@uniflowed/cell` and lies about
//! everything else: `@uniflowed/core/native` is `packages/core/internal/
//! native-runtime.js`, `@uniflowed/host` publishes no `.` export at all, and a
//! package that moved a file would silently take its consumers' types with it.
//! Guessing at paths is what the manifest exists to make unnecessary.
//!
//! A package this batch holds no manifest for stays unresolved and is recorded
//! in [`crate::CheckReport::untyped_modules`], exactly as before: `react` is
//! Flow's own `declare module`, and a dependency under `node_modules` is not a
//! source `uf check` collects.

use std::collections::HashMap;

use compact_str::{CompactString, ToCompactString};
use flow_common::options::Options;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::file_key::{FileKey, FileKeyInner};
use flow_parser_utils::package_json::PackageJson;
use flow_parsing::parsing_service::parse_package_json_file;

use super::resolve;
use crate::Source;

/// The file name a package manifest is recognised by.
const MANIFEST: &str = "package.json";

/// The path a bare specifier named, and how much of a path it is.
///
/// The two cases resolve differently on purpose, and the difference is Node's
/// rather than uf's.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PackageFile {
    /// The file an `exports` map named, to be taken as written.
    ///
    /// A package that declares `exports` has said which file each subpath *is*.
    /// Trying an extension or an `index` beyond it would resolve to a file the
    /// runtime would refuse to load, and would put the checker's answer and the
    /// bundler's answer out of step.
    Exact(CompactString),
    /// A file a package with no `exports` map implies: its `main`, or the
    /// subpath as written under the package directory.
    ///
    /// Node resolves those the way it resolves any file path — as written,
    /// then with an extension, then as a directory holding an `index` — so
    /// this one goes back through the same fallbacks a relative specifier
    /// takes.
    Implied(CompactString),
}

/// One workspace package: where its manifest is, and what the manifest says.
struct Package {
    /// The manifest's own path in the batch.
    ///
    /// Kept rather than the directory because every target inside a manifest is
    /// relative to the manifest, so this is what [`resolve::join`] wants —
    /// `./index.js` against `packages/cell/package.json` is
    /// `packages/cell/index.js`, with the same normalisation a relative import
    /// gets.
    manifest_path: CompactString,
    manifest: PackageJson,
}

/// Every package the batch declares, indexed by the name it publishes.
pub(super) struct WorkspacePackages {
    by_name: HashMap<CompactString, Package>,
    /// The `exports` conditions a subpath is resolved under, from [`Options`].
    conditions: Vec<FlowSmolStr>,
}

impl WorkspacePackages {
    /// Read every manifest in the batch and index it by its published name.
    ///
    /// Built once per batch, eagerly: a repository has one manifest per package
    /// against hundreds of Flow files, and parsing one is a fraction of the
    /// cost of checking one. Doing it lazily would buy nothing — the first
    /// bare specifier in the batch would force it anyway.
    ///
    /// A manifest with no `name`, or one whose name is already taken, is
    /// skipped. First-in-the-batch wins, which is the rule
    /// [`super::resolve::ModuleIndex`] already applies to a duplicated path, so
    /// the answer stays a function of the batch's order and nothing else.
    pub(super) fn new(sources: &[Source<'_>], options: &Options) -> Self {
        let mut by_name: HashMap<CompactString, Package> = HashMap::new();
        for source in sources.iter().filter(|source| is_manifest(source.path)) {
            let Some(manifest) = parse_manifest(source, options) else {
                continue;
            };
            let Some(name) = manifest.name() else {
                continue;
            };
            by_name
                .entry(name.as_str().to_compact_string())
                .or_insert_with(|| Package {
                    manifest_path: source.path.to_compact_string(),
                    manifest,
                });
        }

        Self {
            by_name,
            conditions: options
                .node_package_export_conditions
                .iter()
                .map(|condition| FlowSmolStr::new(condition.as_str()))
                .collect(),
        }
    }

    /// The file `specifier` names, or [`None`] when no package in the batch
    /// publishes it — or publishes that subpath of it.
    pub(super) fn resolve(&self, specifier: &str) -> Option<PackageFile> {
        let (name, subpath) = split(specifier)?;
        let package = self.by_name.get(name)?;

        match package.manifest.exports() {
            // The map is authoritative once it exists: Node stops consulting
            // `main` and stops treating the subpath as a path the moment a
            // package declares `exports`, and a subpath the map does not list
            // is not importable at all. Honouring that is the point — a package
            // may move `./internal/x.js` wherever it likes without any consumer
            // losing its types, and may keep a file unpublished without a
            // consumer being able to reach past the map for it.
            Some(exports) => exports
                .resolve_package(&subpath, &self.conditions)
                .and_then(|target| resolve::join(&package.manifest_path, target.as_str()))
                .map(PackageFile::Exact),
            // No map: `main` is the package itself, any subpath is a path
            // under the package directory, and a package with neither is the
            // directory — which resolves to its `index` the way any directory
            // specifier does. That is Node's order, and it is the only reason
            // `main` is read at all.
            None => {
                let target = match (subpath.as_str(), package.manifest.main()) {
                    (".", Some(main)) => main.as_str().to_compact_string(),
                    (".", None) => CompactString::const_new("."),
                    _ => subpath,
                };
                resolve::join(&package.manifest_path, &target).map(PackageFile::Implied)
            }
        }
    }
}

/// Whether a batch path is a package manifest.
fn is_manifest(path: &str) -> bool {
    path == MANIFEST || path.ends_with("/package.json")
}

/// Parse one manifest, or [`None`] when it is not JSON uf can read.
///
/// A manifest that does not parse is not this crate's error to report: it is
/// still handed to the checker as a source, where it is parsed again as a
/// program and its syntax error is reported against the file itself. Reporting
/// it a second time from here would attach a copy to whoever imported the
/// package.
fn parse_manifest(source: &Source<'_>, options: &Options) -> Option<PackageJson> {
    let file_key = FileKey::new(FileKeyInner::JsonFile(source.path.to_owned()));
    parse_package_json_file(options, Ok(source.source), &file_key).ok()
}

/// Split a bare specifier into the package it names and the subpath inside it.
///
/// Node's own rule: the name is one segment, or two when it is scoped, and
/// what follows is the subpath in the shape an `exports` map is keyed by —
/// `.` for the package itself, `./schedule` for `@uniflowed/effect/schedule`.
fn split(specifier: &str) -> Option<(&str, CompactString)> {
    let scoped = specifier.starts_with('@');
    let mut segments = specifier.splitn(if scoped { 3 } else { 2 }, '/');

    let mut name_len = segments.next()?.len();
    if scoped {
        // `@scope` on its own names no package: a scoped specifier needs the
        // name after the slash before it is a package at all.
        name_len += 1 + segments.next().filter(|scope| !scope.is_empty())?.len();
    }
    if name_len == 0 {
        return None;
    }

    let subpath = match segments.next() {
        Some(rest) => format!("./{rest}").to_compact_string(),
        None => CompactString::const_new("."),
    };
    Some((&specifier[..name_len], subpath))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CheckLimits;
    use crate::upstream::options;

    fn packages(sources: &[Source<'_>]) -> WorkspacePackages {
        WorkspacePackages::new(sources, &options::options(&CheckLimits::default()))
    }

    /// The file an `exports` map named, or a panic saying what came instead.
    fn exact(packages: &WorkspacePackages, specifier: &str) -> CompactString {
        match packages.resolve(specifier) {
            Some(PackageFile::Exact(path)) => path,
            other => panic!("expected an `exports` target for {specifier}, got {other:?}"),
        }
    }

    const CELL: &str = r#"{
      "name": "@uniflowed/cell",
      "exports": { ".": "./index.js" }
    }"#;

    const CORE: &str = r#"{
      "name": "@uniflowed/core",
      "exports": {
        ".": "./index.js",
        "./native": "./internal/native-runtime.js"
      }
    }"#;

    #[test]
    fn a_package_root_resolves_through_its_exports_map() {
        let packages = packages(&[Source::new("packages/cell/package.json", CELL)]);

        assert_eq!(
            exact(&packages, "@uniflowed/cell"),
            "packages/cell/index.js"
        );
    }

    #[test]
    fn a_subpath_resolves_to_the_file_the_map_names_rather_than_to_the_subpath() {
        let packages = packages(&[Source::new("packages/core/package.json", CORE)]);

        // The whole reason to read the map: `./native` is not a file, and
        // `packages/core/native.js` does not exist.
        assert_eq!(
            exact(&packages, "@uniflowed/core/native"),
            "packages/core/internal/native-runtime.js"
        );
    }

    #[test]
    fn a_subpath_an_exports_map_does_not_list_resolves_to_nothing() {
        let packages = packages(&[Source::new("packages/core/package.json", CORE)]);

        // `packages/core/internal/native-runtime.js` is in the repository and
        // is deliberately not published under that path. A checker that
        // reached past the map for it would type an import the runtime would
        // refuse to load.
        assert!(
            packages
                .resolve("@uniflowed/core/internal/native-runtime.js")
                .is_none()
        );
    }

    #[test]
    fn a_package_with_no_dot_export_does_not_resolve_at_its_root() {
        // `@uniflowed/host` is this shape: subpaths only, no package root.
        let packages = packages(&[Source::new(
            "packages/host/package.json",
            r#"{ "name": "@uniflowed/host", "exports": { "./transform": "./transform.js" } }"#,
        )]);

        assert!(packages.resolve("@uniflowed/host").is_none());
        assert_eq!(
            exact(&packages, "@uniflowed/host/transform"),
            "packages/host/transform.js"
        );
    }

    #[test]
    fn an_exports_condition_picks_the_module_an_import_would_load() {
        let packages = packages(&[Source::new(
            "packages/dual/package.json",
            r#"{
              "name": "dual",
              "exports": { ".": { "require": "./cjs.js", "import": "./esm.js" } }
            }"#,
        )]);

        assert_eq!(exact(&packages, "dual"), "packages/dual/esm.js");
    }

    #[test]
    fn a_wildcard_subpath_expands() {
        let packages = packages(&[Source::new(
            "packages/glob/package.json",
            r#"{ "name": "glob", "exports": { "./*": "./src/*.js" } }"#,
        )]);

        assert_eq!(
            exact(&packages, "glob/deep/thing"),
            "packages/glob/src/deep/thing.js"
        );
    }

    #[test]
    fn a_package_with_no_exports_map_falls_back_to_main() {
        let packages = packages(&[Source::new(
            "vendor/legacy/package.json",
            r#"{ "name": "legacy", "main": "./lib/entry.js" }"#,
        )]);

        assert_eq!(
            packages.resolve("legacy"),
            Some(PackageFile::Implied("vendor/legacy/lib/entry.js".into()))
        );
    }

    #[test]
    fn a_package_with_neither_exports_nor_main_falls_back_to_its_directory() {
        let packages = packages(&[Source::new(
            "vendor/bare/package.json",
            r#"{ "name": "bare" }"#,
        )]);

        // The directory, which the caller resolves the way it resolves
        // `./internal` — with an extension, then with an `index`.
        assert_eq!(
            packages.resolve("bare"),
            Some(PackageFile::Implied("vendor/bare".into()))
        );
    }

    #[test]
    fn a_subpath_of_a_package_with_no_exports_map_is_a_path_under_it() {
        let packages = packages(&[Source::new(
            "vendor/legacy/package.json",
            r#"{ "name": "legacy", "main": "./lib/entry.js" }"#,
        )]);

        assert_eq!(
            packages.resolve("legacy/lib/util"),
            Some(PackageFile::Implied("vendor/legacy/lib/util".into()))
        );
    }

    #[test]
    fn a_name_no_manifest_in_the_batch_publishes_resolves_to_nothing() {
        let packages = packages(&[Source::new("packages/cell/package.json", CELL)]);

        assert!(packages.resolve("react").is_none());
        assert!(packages.resolve("@uniflowed/state").is_none());
        assert!(packages.resolve("node:fs").is_none());
    }

    #[test]
    fn a_manifest_that_is_not_json_is_skipped_rather_than_failing_the_batch() {
        let packages = packages(&[
            Source::new("packages/broken/package.json", r#"{ "name": "broken", "#),
            Source::new("packages/cell/package.json", CELL),
        ]);

        assert!(packages.resolve("broken").is_none());
        assert_eq!(
            exact(&packages, "@uniflowed/cell"),
            "packages/cell/index.js"
        );
    }

    #[test]
    fn the_first_manifest_wins_a_duplicated_name() {
        let packages = packages(&[
            Source::new("packages/cell/package.json", CELL),
            Source::new(
                "vendor/cell/package.json",
                r#"{ "name": "@uniflowed/cell", "exports": { ".": "./other.js" } }"#,
            ),
        ]);

        assert_eq!(
            exact(&packages, "@uniflowed/cell"),
            "packages/cell/index.js"
        );
    }

    #[test]
    fn a_specifier_splits_the_way_node_splits_it() {
        assert_eq!(split("react"), Some(("react", ".".into())));
        assert_eq!(
            split("react-dom/client"),
            Some(("react-dom", "./client".into()))
        );
        assert_eq!(
            split("@uniflowed/cell"),
            Some(("@uniflowed/cell", ".".into()))
        );
        assert_eq!(
            split("@uniflowed/effect/schedule"),
            Some(("@uniflowed/effect", "./schedule".into()))
        );
        assert_eq!(
            split("@uniflowed/stylex/tokens.stylex.js"),
            Some(("@uniflowed/stylex", "./tokens.stylex.js".into()))
        );
        // A scope with no package after it is not a package specifier.
        assert_eq!(split("@uniflowed"), None);
        assert_eq!(split("@uniflowed/"), None);
        assert_eq!(split(""), None);
    }

    #[test]
    fn only_a_package_manifest_is_read_as_one() {
        assert!(is_manifest("package.json"));
        assert!(is_manifest("packages/cell/package.json"));
        assert!(!is_manifest("packages/cell/index.js"));
        // A file whose name merely ends in the manifest's is not one.
        assert!(!is_manifest("packages/cell/not-package.json"));
    }
}
