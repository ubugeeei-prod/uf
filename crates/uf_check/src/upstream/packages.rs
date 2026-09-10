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
//! source `uf check` collects until something asks for it.
//!
//! # Two copies of one name
//!
//! Node resolves a bare specifier by climbing from the importing file, so a
//! file inside `node_modules/foo` that imports `bar` gets
//! `node_modules/foo/node_modules/bar` when that exists and the hoisted
//! `node_modules/bar` only when it does not. This module does the same, and
//! that is why every entry point takes the importer: an index from name to
//! manifest cannot express two versions of one name, and one that kept the
//! first would make which types a file sees depend on the batch's order.
//! ubugeeei-prod/uf#486.
//!
//! A manifest that is *not* under a `node_modules` directory is the project's
//! own — a workspace package such as `packages/cell` — and is visible from
//! everywhere, which is what a monorepo means and what no climb would find,
//! since `packages/cell` is not on any importer's path. The climb comes first
//! so that a package inside `node_modules` still gets its own nested copy.

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

/// The directory an installed package sits under, with its separator.
const INSTALLED_PREFIX: &str = "node_modules/";

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

/// One package: where its manifest is, and what the manifest says.
#[derive(Clone)]
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

/// One installed copy of a package, and where it is installed.
struct Installed {
    /// The directory whose `node_modules` holds this copy, without a trailing
    /// slash; empty for the batch root's own `node_modules`.
    ///
    /// This is what an importer has to be inside for Node to find this copy,
    /// and comparing its length is how the deepest copy wins.
    enclosing: CompactString,
    package: Package,
}

/// Every package the batch declares, indexed so that a specifier can be
/// resolved from the file that wrote it.
pub(super) struct WorkspacePackages {
    /// Every manifest by package scope, deepest first.
    ///
    /// `package.json#imports` is not reached through a package name. A
    /// `#internal/foo` specifier belongs to the nearest parent package scope
    /// of the file that wrote it, even when that manifest has no `name`, so it
    /// needs a scope index separate from `project` and `installed`.
    scopes: Vec<Package>,
    /// Manifests outside any `node_modules`, by the name they publish.
    ///
    /// The project's own packages. Visible from every file in the batch: a
    /// workspace package is on no importer's `node_modules` path, so a climb
    /// would never reach one, and it is exactly the package a uf repository
    /// imports by name.
    project: HashMap<CompactString, Package>,
    /// Manifests under a `node_modules`, by the directory name that holds
    /// them, deepest installation first.
    ///
    /// Keyed by the directory rather than by `name`, because that is what Node
    /// resolves by: a package whose manifest claims a different name is still
    /// loaded from the directory the specifier spells.
    installed: HashMap<CompactString, Vec<Installed>>,
    /// The `exports` conditions a subpath is resolved under, from [`Options`].
    conditions: Vec<FlowSmolStr>,
}

impl WorkspacePackages {
    /// Read every manifest in the batch and index it by how a specifier reaches it.
    ///
    /// Built once per batch, eagerly: a repository has one manifest per package
    /// against hundreds of Flow files, and parsing one is a fraction of the
    /// cost of checking one. Doing it lazily would buy nothing — the first
    /// bare specifier in the batch would force it anyway.
    ///
    /// A project manifest with no `name`, or one whose name is already taken,
    /// is skipped. First-in-the-batch wins, which is the rule
    /// [`super::resolve::ModuleIndex`] already applies to a duplicated path, so
    /// the answer stays a function of the batch's order and nothing else. An
    /// *installed* manifest needs no `name`, because the directory it sits in
    /// is the name a specifier reaches it by; two copies at one path are the
    /// same collision and the first still wins.
    pub(super) fn new(sources: &[Source<'_>], options: &Options) -> Self {
        let mut scopes: Vec<Package> = Vec::new();
        let mut project: HashMap<CompactString, Package> = HashMap::new();
        let mut installed: HashMap<CompactString, Vec<Installed>> = HashMap::new();
        for source in sources.iter().filter(|source| is_manifest(source.path)) {
            let Some(manifest) = parse_manifest(source, options) else {
                continue;
            };
            let package = Package {
                manifest_path: source.path.to_compact_string(),
                manifest,
            };
            scopes.push(package.clone());
            match installed_at(source.path) {
                Some((enclosing, name)) => installed
                    .entry(name.to_compact_string())
                    .or_default()
                    .push(Installed {
                        enclosing: enclosing.to_compact_string(),
                        package,
                    }),
                None => {
                    let Some(name) = package.manifest.name() else {
                        continue;
                    };
                    project
                        .entry(name.as_str().to_compact_string())
                        .or_insert(package);
                }
            }
        }
        scopes.sort_by_key(|package| std::cmp::Reverse(scope_specificity(&package.manifest_path)));
        // Deepest first, so the climb is a linear scan that stops at the first
        // copy the importer can see. Ties keep batch order, which only a
        // duplicated path can produce.
        for copies in installed.values_mut() {
            // By how near the copy is, not how long its path is: `..` is two
            // characters and the furthest away of all.
            copies.sort_by_key(|copy| std::cmp::Reverse(specificity(&copy.enclosing)));
        }

        Self {
            scopes,
            project,
            installed,
            conditions: options
                .node_package_export_conditions
                .iter()
                .map(|condition| FlowSmolStr::new(condition.as_str()))
                .collect(),
        }
    }

    /// The package `name` means to a file at `importer`, by Node's climb.
    ///
    /// The nearest installed copy wins; a project package answers only when no
    /// installed copy is on the importer's path at all.
    fn lookup(&self, importer: &str, name: &str) -> Option<&Package> {
        if let Some(copies) = self.installed.get(name)
            && let Some(nearest) = copies
                .iter()
                .find(|copy| encloses(&copy.enclosing, importer))
        {
            return Some(&nearest.package);
        }
        self.project.get(name)
    }

    fn scope(&self, importer: &str) -> Option<&Package> {
        self.scopes
            .iter()
            .find(|package| scope_encloses(package.manifest_path.as_str(), importer))
    }

    /// The file `specifier` names when it is written in `importer`, or [`None`]
    /// when no package the importer can see publishes it — or publishes that
    /// subpath of it.
    pub(super) fn resolve(&self, importer: &str, specifier: &str) -> Option<PackageFile> {
        if specifier.starts_with('#') {
            return self.resolve_import(importer, specifier);
        }
        let (name, subpath) = split(specifier)?;
        let package = self.lookup(importer, name)?;

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

    fn resolve_import(&self, importer: &str, specifier: &str) -> Option<PackageFile> {
        let package = self.scope(importer)?;
        let imports = package.manifest.imports()?;
        let target = imports.resolve_package(specifier, &self.conditions)?;

        if resolve::is_relative(target.as_str()) {
            return resolve::join(&package.manifest_path, target.as_str()).map(PackageFile::Exact);
        }

        // Node's `imports` can point at another package. Reuse the ordinary
        // package resolver for that half, while leaving recursive `#` aliases
        // unresolved until there is a concrete need for them.
        (!target.starts_with('#'))
            .then(|| self.resolve(package.manifest_path.as_str(), target.as_str()))
            .flatten()
    }

    /// The manifest that publishes `specifier`'s package, if the batch holds
    /// one.
    ///
    /// A caller assembling a batch needs this and not just the file: a check
    /// that is handed `packages/cell/index.js` but not
    /// `packages/cell/package.json` cannot resolve `@uniflowed/cell` at all,
    /// because the manifest is where the name comes from. So whatever pulls a
    /// package's file into a batch has to pull the manifest that named it in
    /// alongside. See [`super::closure`].
    pub(super) fn manifest_of(&self, importer: &str, specifier: &str) -> Option<&str> {
        if specifier.starts_with('#') {
            return Some(self.scope(importer)?.manifest_path.as_str());
        }
        let (name, _) = split(specifier)?;
        Some(self.lookup(importer, name)?.manifest_path.as_str())
    }
}

/// Where an installed package is installed, and under what name.
///
/// [`None`] for a manifest that is not inside a `node_modules` directory — the
/// project's own — and for one whose directory under `node_modules` is not a
/// package name: Node reads one segment there, or two when the first is a
/// scope, so `node_modules/a/b/package.json` names no package and is left to
/// be indexed by whatever its manifest publishes.
fn installed_at(manifest_path: &str) -> Option<(&str, &str)> {
    let directory = manifest_path.strip_suffix(MANIFEST)?.strip_suffix('/')?;
    let (enclosing, name) = directory.rsplit_once(INSTALLED_PREFIX)?;
    // A path boundary, not a substring: `vendor/mynode_modules/x` holds no
    // installed package.
    if !(enclosing.is_empty() || enclosing.ends_with('/')) {
        return None;
    }
    let segments = name.split('/').count();
    let scoped = name.starts_with('@');
    if segments != usize::from(scoped) + 1 {
        return None;
    }
    Some((enclosing.strip_suffix('/').unwrap_or(enclosing), name))
}

fn scope_directory(manifest_path: &str) -> &str {
    manifest_path
        .strip_suffix(MANIFEST)
        .and_then(|path| path.strip_suffix('/'))
        .unwrap_or("")
}

fn scope_encloses(manifest_path: &str, importer: &str) -> bool {
    let scope = scope_directory(manifest_path);
    if scope.is_empty() {
        return leading_parent_segments(importer) == 0;
    }
    if is_above_root(scope) {
        return leading_parent_segments(importer) <= scope.split('/').count();
    }
    path_encloses(scope, importer)
}

fn scope_specificity(manifest_path: &str) -> isize {
    let scope = scope_directory(manifest_path);
    if scope.is_empty() {
        return 0;
    }
    let segments = scope.split('/').count() as isize;
    if is_above_root(scope) {
        -segments
    } else {
        segments
    }
}

/// Whether a file at `importer` is inside `enclosing`, so that Node's climb
/// from it reaches that directory's `node_modules`.
///
/// The batch root encloses everything, which is the hoisted copy: it is what a
/// climb reaches last and what answers when no nested copy does.
fn encloses(enclosing: &str, importer: &str) -> bool {
    // A base of nothing but `..` is a directory above the batch root, and an
    // ancestor encloses everything under it — that is the hoisted copy a
    // workspace app resolves through, which is above the app rather than in
    // it. See ubugeeei-prod/uf#654.
    if enclosing.is_empty() || is_above_root(enclosing) {
        return true;
    }
    importer.len() > enclosing.len()
        && importer.starts_with(enclosing)
        && importer.as_bytes()[enclosing.len()] == b'/'
}

/// Whether a base names a directory above the batch root.
fn is_above_root(enclosing: &str) -> bool {
    !enclosing.is_empty() && enclosing.split('/').all(|segment| segment == "..")
}

fn leading_parent_segments(path: &str) -> usize {
    path.split('/')
        .take_while(|segment| *segment == "..")
        .count()
}

fn path_encloses(scope: &str, importer: &str) -> bool {
    let mut importer_segments = importer.split('/');
    for scope_segment in scope.split('/') {
        if Some(scope_segment) != importer_segments.next() {
            return false;
        }
    }
    importer_segments.next().is_some()
}

/// How specific a base is, so the nearest copy answers first.
///
/// Inside the root, deeper is nearer, and the root itself is zero. Above the
/// root the sign flips: one directory up is `-1`, two is `-2`, and both come
/// after every copy inside — which is Node's climb, where the hoisted copy is
/// what answers when no nearer one does.
fn specificity(enclosing: &str) -> isize {
    if enclosing.is_empty() {
        return 0;
    }
    let segments = enclosing.split('/').count() as isize;
    if is_above_root(enclosing) {
        -segments
    } else {
        segments
    }
}

/// Whether a batch path is a package manifest.
pub(super) fn is_manifest(path: &str) -> bool {
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
    ///
    /// Imported from a file at the batch root, which is the hoisted case: a
    /// test about the climb names its own importer.
    fn exact(packages: &WorkspacePackages, specifier: &str) -> CompactString {
        exact_from(packages, "app.js", specifier)
    }

    fn exact_from(packages: &WorkspacePackages, importer: &str, specifier: &str) -> CompactString {
        match packages.resolve(importer, specifier) {
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
                .resolve("app.js", "@uniflowed/core/internal/native-runtime.js")
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

        assert!(packages.resolve("app.js", "@uniflowed/host").is_none());
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
            packages.resolve("app.js", "legacy"),
            Some(PackageFile::Implied("vendor/legacy/lib/entry.js".into()))
        );
    }

    #[test]
    fn a_package_with_no_exports_map_prefers_module_to_main() {
        let packages = packages(&[Source::new(
            "vendor/dual/package.json",
            r#"{ "name": "dual", "main": "./cjs.js", "module": "./esm.js" }"#,
        )]);

        assert_eq!(
            packages.resolve("app.js", "dual"),
            Some(PackageFile::Implied("vendor/dual/esm.js".into()))
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
            packages.resolve("app.js", "bare"),
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
            packages.resolve("app.js", "legacy/lib/util"),
            Some(PackageFile::Implied("vendor/legacy/lib/util".into()))
        );
    }

    #[test]
    fn a_package_imports_map_resolves_a_hash_specifier_from_its_scope() {
        let packages = packages(&[Source::new(
            "package.json",
            r##"{ "imports": { "#cell": "./packages/cell/index.js" } }"##,
        )]);

        assert_eq!(exact(&packages, "#cell"), "packages/cell/index.js");
    }

    #[test]
    fn a_package_imports_map_above_the_batch_root_is_visible() {
        let packages = packages(&[Source::new(
            "../package.json",
            r##"{ "imports": { "#cell": "./packages/cell/index.js" } }"##,
        )]);

        assert_eq!(exact(&packages, "#cell"), "../packages/cell/index.js");
    }

    #[test]
    fn a_nearer_package_scope_wins_over_one_above_the_batch_root() {
        let packages = packages(&[
            Source::new(
                "../package.json",
                r##"{ "imports": { "#mode": "./above.js" } }"##,
            ),
            Source::new(
                "package.json",
                r##"{ "imports": { "#mode": "./root.js" } }"##,
            ),
        ]);

        assert_eq!(exact(&packages, "#mode"), "root.js");
    }

    #[test]
    fn a_parent_package_scope_answers_files_above_the_batch_root() {
        let packages = packages(&[
            Source::new(
                "package.json",
                r##"{ "imports": { "#mode": "./root.js" } }"##,
            ),
            Source::new(
                "../package.json",
                r##"{ "imports": { "#mode": "./parent.js" } }"##,
            ),
        ]);

        assert_eq!(exact_from(&packages, "../app.js", "#mode"), "../parent.js");
    }

    #[test]
    fn a_grandparent_package_scope_beats_parent_for_grandparent_files() {
        let packages = packages(&[
            Source::new(
                "package.json",
                r##"{ "imports": { "#mode": "./root.js" } }"##,
            ),
            Source::new(
                "../package.json",
                r##"{ "imports": { "#mode": "./parent.js" } }"##,
            ),
            Source::new(
                "../../package.json",
                r##"{ "imports": { "#mode": "./grandparent.js" } }"##,
            ),
        ]);

        assert_eq!(
            exact_from(&packages, "../../app.js", "#mode"),
            "../../grandparent.js"
        );
    }

    #[test]
    fn the_nearest_package_scope_answers_a_hash_specifier() {
        let packages = packages(&[
            Source::new(
                "package.json",
                r##"{ "imports": { "#mode": "./root.js" } }"##,
            ),
            Source::new(
                "packages/app/package.json",
                r##"{ "name": "app", "imports": { "#mode": "./local.js" } }"##,
            ),
        ]);

        assert_eq!(
            exact_from(&packages, "packages/app/index.js", "#mode"),
            "packages/app/local.js"
        );
        assert_eq!(exact_from(&packages, "other.js", "#mode"), "root.js");
    }

    #[test]
    fn a_name_no_manifest_in_the_batch_publishes_resolves_to_nothing() {
        let packages = packages(&[Source::new("packages/cell/package.json", CELL)]);

        assert!(packages.resolve("app.js", "react").is_none());
        assert!(packages.resolve("app.js", "@uniflowed/state").is_none());
        assert!(packages.resolve("app.js", "node:fs").is_none());
    }

    #[test]
    fn a_manifest_that_is_not_json_is_skipped_rather_than_failing_the_batch() {
        let packages = packages(&[
            Source::new("packages/broken/package.json", r#"{ "name": "broken", "#),
            Source::new("packages/cell/package.json", CELL),
        ]);

        assert!(packages.resolve("app.js", "broken").is_none());
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

    const HOISTED: &str = r#"{ "name": "bar", "version": "2.0.0", "exports": { ".": "./v2.js" } }"#;
    const NESTED: &str = r#"{ "name": "bar", "version": "1.0.0", "exports": { ".": "./v1.js" } }"#;

    /// Two versions of `bar`: one hoisted, one nested inside `foo`.
    fn two_versions() -> [Source<'static>; 4] {
        [
            Source::new("node_modules/bar/package.json", HOISTED),
            Source::new("node_modules/bar/v2.js", "export const bar = 2;\n"),
            Source::new("node_modules/foo/node_modules/bar/package.json", NESTED),
            Source::new(
                "node_modules/foo/node_modules/bar/v1.js",
                "export const bar = 1;\n",
            ),
        ]
    }

    #[test]
    fn a_nested_copy_answers_the_package_that_holds_it() {
        // ubugeeei-prod/uf#486: `foo`'s own `bar` is the module the runtime
        // loads for `foo`, so it is the module its types come from.
        let packages = packages(&two_versions());

        assert_eq!(
            exact_from(&packages, "node_modules/foo/index.js", "bar"),
            "node_modules/foo/node_modules/bar/v1.js"
        );
    }

    #[test]
    fn the_hoisted_copy_answers_everyone_the_nested_one_does_not_enclose() {
        let packages = packages(&two_versions());

        assert_eq!(
            exact_from(&packages, "src/app.js", "bar"),
            "node_modules/bar/v2.js"
        );
        // A sibling package under `node_modules` is not inside `foo`, so the
        // nested copy is not on its path either.
        assert_eq!(
            exact_from(&packages, "node_modules/other/index.js", "bar"),
            "node_modules/bar/v2.js"
        );
        // Nor is a directory whose name merely starts the same way.
        assert_eq!(
            exact_from(&packages, "node_modules/food/index.js", "bar"),
            "node_modules/bar/v2.js"
        );
    }

    #[test]
    fn a_nested_copy_is_the_deepest_one_the_importer_is_inside() {
        let packages = packages(&[
            Source::new("node_modules/bar/package.json", HOISTED),
            Source::new("node_modules/bar/v2.js", "export const bar = 2;\n"),
            Source::new("node_modules/foo/node_modules/bar/package.json", NESTED),
            Source::new(
                "node_modules/foo/node_modules/bar/v1.js",
                "export const bar = 1;\n",
            ),
            Source::new(
                "node_modules/foo/node_modules/deep/node_modules/bar/package.json",
                r#"{ "name": "bar", "exports": { ".": "./v0.js" } }"#,
            ),
            Source::new(
                "node_modules/foo/node_modules/deep/node_modules/bar/v0.js",
                "export const bar = 0;\n",
            ),
        ]);

        assert_eq!(
            exact_from(
                &packages,
                "node_modules/foo/node_modules/deep/index.js",
                "bar"
            ),
            "node_modules/foo/node_modules/deep/node_modules/bar/v0.js"
        );
        assert_eq!(
            exact_from(&packages, "node_modules/foo/lib/util.js", "bar"),
            "node_modules/foo/node_modules/bar/v1.js"
        );
    }

    #[test]
    fn an_installed_package_is_found_by_its_directory_and_not_by_its_name() {
        // npm aliases (`npm i bar@npm:other`) install a package whose manifest
        // publishes another name. Node loads it from the directory the
        // specifier spells, so this must too.
        let packages = packages(&[Source::new(
            "node_modules/bar/package.json",
            r#"{ "name": "other", "exports": { ".": "./index.js" } }"#,
        )]);

        assert_eq!(exact(&packages, "bar"), "node_modules/bar/index.js");
        assert!(packages.resolve("app.js", "other").is_none());
    }

    #[test]
    fn a_workspace_package_is_visible_from_everywhere() {
        // `packages/cell` is on no importer's `node_modules` path, so a climb
        // alone would never find it — and it is the package this repository's
        // own files import by name.
        //
        // The first importer is the package's own test, which is where this
        // repository's suite lives now: a co-located test says `@uniflowed/cell`
        // rather than `./index.js`, because the specifier is part of what it is
        // testing. The second is a stranger, and is the "from everywhere" half.
        let packages = packages(&[Source::new("packages/cell/package.json", CELL)]);

        assert_eq!(
            exact_from(&packages, "packages/cell/cell.test.js", "@uniflowed/cell"),
            "packages/cell/index.js"
        );
        assert_eq!(
            exact_from(&packages, "node_modules/foo/index.js", "@uniflowed/cell"),
            "packages/cell/index.js"
        );
    }

    #[test]
    fn the_manifest_a_specifier_reaches_is_the_one_that_answered_it() {
        let packages = packages(&two_versions());

        assert_eq!(
            packages.manifest_of("node_modules/foo/index.js", "bar"),
            Some("node_modules/foo/node_modules/bar/package.json")
        );
        assert_eq!(
            packages.manifest_of("src/app.js", "bar"),
            Some("node_modules/bar/package.json")
        );
    }

    #[test]
    fn an_installed_package_is_recognised_by_where_it_sits() {
        assert_eq!(
            installed_at("node_modules/bar/package.json"),
            Some(("", "bar"))
        );
        assert_eq!(
            installed_at("node_modules/@scope/bar/package.json"),
            Some(("", "@scope/bar"))
        );
        assert_eq!(
            installed_at("node_modules/foo/node_modules/bar/package.json"),
            Some(("node_modules/foo", "bar"))
        );
        // Not under a `node_modules` at all: the project's own.
        assert_eq!(installed_at("packages/cell/package.json"), None);
        // A directory whose name merely ends in the same characters.
        assert_eq!(installed_at("vendor/mynode_modules/x/package.json"), None);
        // Too many segments to be a package name.
        assert_eq!(installed_at("node_modules/a/b/package.json"), None);
        assert_eq!(installed_at("node_modules/@scope/a/b/package.json"), None);
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

    /// A package hoisted **above** the batch root answers, and one inside it
    /// wins over it.
    ///
    /// npm installs a workspace app's dependencies in the repository root's
    /// `node_modules`, which is above the app — so `uf check` run from the app
    /// reads them at `../node_modules/…`, and a base of nothing but `..`
    /// encloses everything under it. Before this, `encloses` compared it as a
    /// prefix, `"app.js".starts_with("..")` was false, and every hoisted
    /// package resolved to nothing. See ubugeeei-prod/uf#654.
    #[test]
    fn a_package_above_the_batch_root_is_found_and_a_nearer_one_still_wins() {
        let hoisted = r#"{ "name": "dep", "exports": { ".": "./hoisted.js" } }"#;
        let nested = r#"{ "name": "dep", "exports": { ".": "./nested.js" } }"#;
        let packages = packages(&[
            Source::new("../node_modules/dep/package.json", hoisted),
            Source::new("../node_modules/dep/hoisted.js", ""),
            Source::new("node_modules/foo/package.json", r#"{ "name": "foo" }"#),
            Source::new("node_modules/foo/node_modules/dep/package.json", nested),
            Source::new("node_modules/foo/node_modules/dep/nested.js", ""),
            Source::new("app.js", ""),
        ]);

        // From the app: nothing nearer, so the hoisted copy answers.
        assert_eq!(
            exact_from(&packages, "app.js", "dep"),
            "../node_modules/dep/hoisted.js"
        );

        // From inside `foo`: its own nested copy is nearer than the hoisted
        // one, and `..` must not outrank it just because it sorts oddly by
        // path length.
        assert_eq!(
            exact_from(&packages, "node_modules/foo/index.js", "dep"),
            "node_modules/foo/node_modules/dep/nested.js"
        );
    }

    /// Two directories up answers too, and is further away than one.
    #[test]
    fn the_nearer_of_two_ancestors_answers_first() {
        let one_up = r#"{ "name": "dep", "exports": { ".": "./one-up.js" } }"#;
        let two_up = r#"{ "name": "dep", "exports": { ".": "./two-up.js" } }"#;
        let packages = packages(&[
            Source::new("../node_modules/dep/package.json", one_up),
            Source::new("../node_modules/dep/one-up.js", ""),
            Source::new("../../node_modules/dep/package.json", two_up),
            Source::new("../../node_modules/dep/two-up.js", ""),
            Source::new("app.js", ""),
        ]);

        assert_eq!(
            exact_from(&packages, "app.js", "dep"),
            "../node_modules/dep/one-up.js"
        );
    }
}
