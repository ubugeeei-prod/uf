//! Reading an installed package, for the imports a project's own files did not
//! answer.
//!
//! # Why this is not the scan's job
//!
//! `uf_project`'s scan is what the project *owns*: the files `uf fmt` may
//! rewrite, `uf lint` reports on and `uf check` reports on. `node_modules` is
//! none of those, and a scan that walked it would put a dependency's
//! diagnostics in front of an author who cannot fix them.
//!
//! The checker's need is different, and narrower. An import is typed against a
//! file in the same batch, so `import type { Control } from "@uniflowed/form"`
//! is `any` unless `@uniflowed/form`'s own source is in it. In this repository
//! the source is in `packages/form`, which the scan already collects; in a
//! project that merely *uses* uf it is in `node_modules/@uniflowed/form`, and
//! nothing collects it. That is the second half of ubugeeei-prod/uf#403, and
//! this module is it: a package is read only because a specifier asked for it,
//! and only ever as a dependency to be typed against.
//!
//! # Which packages are read
//!
//! Exactly the ones `uf_check::module_closure` reports as unresolved: no
//! source in the batch answered the specifier, *and* Flow's own library
//! definitions do not describe it. The second condition is what keeps `react`
//! out. A file in the batch outranks a `declare module`, so reading React's
//! shipped JavaScript would replace Flow's description of React — written by
//! people who know what its types are — with a bundle that has none. A package
//! Flow describes stays Flow's to describe.
//!
//! There is a second condition, and it is Flow's own. A dependency that does
//! not opt into Flow exports `any` whether it is read or not, so reading it
//! buys nothing and costs a parse of every byte it ships — `react-dom` alone
//! is megabytes of bundled JavaScript with no types in it. So a package is
//! read only if something in it declares `@flow`. uf checks a file with no
//! pragma because a *project's* files are uf's to have an opinion about; a
//! dependency's are not, and the pragma is how a package says otherwise.
//!
//! # And how much of one
//!
//! All of it: every `.js`, `.jsx`, `.mjs`, `.cjs` and `package.json` under the
//! package directory, because an `exports` map may name any of them and a
//! module reached from one may name any other. A package larger than
//! [`MAX_PACKAGE_FILES`] is skipped whole rather than in part — a batch holding
//! half a package resolves an import to a file that is missing for no reason
//! the author could discover, whereas one holding none of it leaves the
//! specifier in `untyped_modules`, where the footer names it out loud.
//!
//! # Which *copy* of one
//!
//! The one Node would load, found by climbing `node_modules` from the file
//! that wrote the specifier: code inside `node_modules/foo` that imports `bar`
//! gets `node_modules/foo/node_modules/bar` when there is one, and the hoisted
//! `node_modules/bar` only when there is not. That is why the closure hands
//! back the importer beside each specifier rather than a set of names —
//! ubugeeei-prod/uf#486, where the previous rule (always the hoisted copy)
//! typed the files inside a nested consumer against the wrong version's
//! declarations. Two copies of one name can now be in one batch, because
//! `uf_check`'s `WorkspacePackages` resolves them per importer too, so which
//! types a file sees no longer depends on the batch's order.

use std::fs;
use std::io::Read;

use camino::{Utf8Path, Utf8PathBuf};
use uf_check::UnresolvedImport;
use uf_infra::FxHashSet;
use uf_lint::SourceFile;
use uf_project::SourceKind;
use walkdir::WalkDir;

/// The directory an installed package is read from.
const INSTALLED: &str = "node_modules";

/// The file that makes a directory a package.
const MANIFEST: &str = "package.json";

/// How many files a package may hold before it is left unread.
///
/// Generous on purpose: it is a guard against a directory nobody meant to
/// publish, not a policy about package size. Every `@uniflowed` package is two
/// orders of magnitude below it.
const MAX_PACKAGE_FILES: usize = 4_000;

/// How much of a file is read while looking for its `@flow` pragma.
///
/// A docblock is the first comment in a file, and Flow bounds how far it will
/// look for one with `max_header_tokens`. This is the same idea in bytes, and
/// it is what keeps the question "does this package use Flow" from costing a
/// full read of a package that does not.
const HEADER_BYTES: usize = 8 * 1024;

/// The pragma a package uses to say its source is Flow.
const FLOW_PRAGMA: &str = "@flow";

/// Read every installed package `unresolved` reaches that has not been read
/// already.
///
/// `read` is both the filter and the record, and it holds **directories**
/// rather than names: two copies of one package are two entries, and a copy
/// that was read and yielded nothing — it ships no Flow — is in it too, so the
/// next round does not walk it again. That is what makes the caller's loop
/// terminate: a round that finds no directory it has not already attempted
/// adds no sources, and the loop ends.
pub(super) fn load_packages(
    root: &Utf8Path,
    unresolved: &[UnresolvedImport],
    read: &mut FxHashSet<String>,
) -> Vec<SourceFile> {
    let mut loaded = Vec::new();
    for import in unresolved {
        let Some(name) = package_name(&import.specifier) else {
            continue;
        };
        let Some(directory) = installed_for(root, &import.importer, name) else {
            continue;
        };
        if !read.insert(directory.clone()) {
            continue;
        }
        loaded.extend(read_package(root, &directory));
    }
    loaded
}

/// The directory Node's climb from `importer` finds `name` in, or [`None`]
/// when no `node_modules` on that path holds it.
///
/// Existence is decided by the manifest, because that is what makes a
/// directory a package: `node_modules/.bin/foo` and a leftover empty directory
/// are both on the path and neither is one.
fn installed_for(root: &Utf8Path, importer: &str, name: &str) -> Option<String> {
    search_paths(importer).into_iter().find_map(|base| {
        let candidate = if base.is_empty() {
            format!("{INSTALLED}/{name}")
        } else {
            format!("{base}/{INSTALLED}/{name}")
        };
        root.join(&candidate)
            .join(MANIFEST)
            .is_file()
            .then_some(candidate)
    })
}

/// The directories whose `node_modules` Node consults for a specifier written
/// in `importer`, nearest first.
///
/// Node's own `NODE_MODULES_PATHS`: every ancestor directory of the importing
/// file, ending at the root, and never a `node_modules` directory itself —
/// `node_modules/node_modules` is not a place packages are installed, and
/// looking there would be one stat per import for nothing.
fn search_paths(importer: &str) -> Vec<&str> {
    let mut paths = Vec::new();
    let mut directory = parent(importer);
    loop {
        if directory.rsplit('/').next() != Some(INSTALLED) {
            paths.push(directory);
        }
        if directory.is_empty() {
            return paths;
        }
        directory = parent(directory);
    }
}

/// The directory a batch path sits in, or the empty string for the root.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(head, _)| head)
}

/// The package a bare specifier names, or [`None`] when it names none.
///
/// Node's own rule — one segment, or two when scoped — with the three shapes
/// that are not a package at all excluded first: a relative path, an absolute
/// one, and a runtime builtin such as `node:fs`, whose colon can appear in no
/// package name.
fn package_name(specifier: &str) -> Option<&str> {
    if specifier.starts_with('.') || specifier.starts_with('/') || specifier.contains(':') {
        return None;
    }
    let scoped = specifier.starts_with('@');
    let mut segments = specifier.splitn(if scoped { 3 } else { 2 }, '/');

    let mut length = segments.next()?.len();
    if scoped {
        length += 1 + segments.next().filter(|scope| !scope.is_empty())?.len();
    }
    (length > 0).then(|| &specifier[..length])
}

/// Every Flow source and manifest the package installed at `directory` holds.
///
/// `directory` is project-relative and is the path the package is *reported*
/// under, so a nested copy and a hoisted one stay two modules in the batch even
/// though they publish one name.
fn read_package(root: &Utf8Path, directory: &str) -> Vec<SourceFile> {
    let installed = root.join(directory);
    // Resolved through the link before the walk rather than during it. A
    // workspace package *is* a symlink — `uf install` links `packages/form`
    // into `node_modules/@uniflowed/` — so a walk that did not follow one
    // would find the very case this exists for empty, and a walk that followed
    // every link it met could leave the package entirely.
    let Ok(resolved) = installed.canonicalize_utf8() else {
        return Vec::new();
    };
    if !resolved.join(MANIFEST).is_file() {
        return Vec::new();
    }

    let mut paths = Vec::new();
    let walk = WalkDir::new(resolved.as_std_path())
        .into_iter()
        // A dependency's own dependencies are packages in their own right, and
        // are read as packages when a specifier reaches one — from
        // `installed_for`'s climb, which finds this package's nested copy
        // before the hoisted one. Reading them *here* would be reading them
        // under this package's path, where nothing resolves to them, and would
        // pull in the whole transitive tree of every package that has one.
        .filter_entry(|entry| entry.depth() == 0 || entry.file_name() != INSTALLED);
    for entry in walk.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(path) = Utf8PathBuf::from_path_buf(entry.path().to_path_buf()) else {
            continue;
        };
        let kind = SourceKind::from_path(&path);
        if !matches!(kind, Some(kind) if kind.is_flow() || kind == SourceKind::PackageManifest) {
            continue;
        }
        if paths.len() == MAX_PACKAGE_FILES {
            return Vec::new();
        }
        paths.push(path);
    }
    // Asked of the headers, before a byte of any body is read.
    if !paths.iter().any(declares_flow) {
        return Vec::new();
    }

    let mut files = Vec::new();
    for path in paths {
        let (Ok(relative), Ok(source)) = (path.strip_prefix(&resolved), fs::read_to_string(&path))
        else {
            continue;
        };
        files.push(SourceFile {
            // Under the directory that named it, not under wherever the link
            // went. Every target inside a manifest is relative to the
            // manifest, so a package read as
            // `node_modules/@uniflowed/form/package.json` resolves its own
            // `./index.js` to a path in the same shape, and one read as
            // `packages/form/package.json` would collide with the copy the
            // scan already holds.
            path: format!("{directory}/{relative}"),
            source,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

/// Whether a file's header opts into Flow.
///
/// Read rather than parsed: this decides whether a package is worth handing to
/// the checker at all, and parsing it to find out would be the cost the answer
/// exists to avoid.
fn declares_flow(path: &Utf8PathBuf) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut header = Vec::new();
    if Read::take(file, HEADER_BYTES as u64)
        .read_to_end(&mut header)
        .is_err()
    {
        return false;
    }
    // Lossy, because a package is not obliged to be UTF-8 and a byte sequence
    // that is not says nothing about the pragma either way.
    String::from_utf8_lossy(&header).contains(FLOW_PRAGMA)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_specifier_names_its_package() {
        assert_eq!(package_name("react"), Some("react"));
        assert_eq!(package_name("react-dom/client"), Some("react-dom"));
        assert_eq!(package_name("@uniflowed/form"), Some("@uniflowed/form"));
        assert_eq!(
            package_name("@uniflowed/form/watch"),
            Some("@uniflowed/form")
        );
    }

    #[test]
    fn what_is_not_a_package_names_none() {
        assert_eq!(package_name("./sibling.js"), None);
        assert_eq!(package_name("../parent.js"), None);
        assert_eq!(package_name("/absolute.js"), None);
        assert_eq!(package_name("node:fs"), None);
        assert_eq!(package_name("bun:test"), None);
        // A scope with no package after it names nothing installable.
        assert_eq!(package_name("@uniflowed"), None);
        assert_eq!(package_name("@uniflowed/"), None);
    }

    /// One unresolved import, as the closure hands it over.
    fn import(specifier: &str, importer: &str) -> UnresolvedImport {
        UnresolvedImport {
            specifier: specifier.into(),
            importer: importer.into(),
        }
    }

    /// A project root under a temporary directory, with `files` written into
    /// it.
    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a temporary directory");
        for (path, source) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().expect("a parent")).expect("the directory is made");
            fs::write(path, source).expect("the file is written");
        }
        root
    }

    fn root_of(directory: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("a UTF-8 path")
    }

    #[test]
    fn a_package_that_is_not_installed_reads_as_nothing() {
        let root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut read = FxHashSet::default();

        assert!(
            load_packages(
                &root,
                &[import("@uniflowed/not-installed-anywhere", "src/app.js")],
                &mut read,
            )
            .is_empty()
        );
        // Nothing is recorded, because no directory was found to record. The
        // caller's loop still terminates: a round that adds no sources is the
        // last one.
        assert!(read.is_empty());
    }

    #[test]
    fn a_specifier_is_looked_for_the_way_node_looks_for_it() {
        // Every ancestor of the importing file, nearest first, and never a
        // `node_modules` directory itself.
        assert_eq!(search_paths("src/app.js"), ["src", ""]);
        assert_eq!(
            search_paths("node_modules/foo/lib/index.js"),
            ["node_modules/foo/lib", "node_modules/foo", ""]
        );
        assert_eq!(
            search_paths("node_modules/@scope/foo/index.js"),
            ["node_modules/@scope/foo", "node_modules/@scope", ""]
        );
        assert_eq!(search_paths("app.js"), [""]);
    }

    /// A tree with two copies of `bar`: one hoisted, one nested inside `foo`.
    fn two_versions() -> tempfile::TempDir {
        project(&[
            (
                "node_modules/bar/package.json",
                r#"{ "name": "bar", "version": "2.0.0", "exports": { ".": "./index.js" } }"#,
            ),
            (
                "node_modules/bar/index.js",
                "// @flow\ndeclare export const version: 2;\n",
            ),
            (
                "node_modules/foo/node_modules/bar/package.json",
                r#"{ "name": "bar", "version": "1.0.0", "exports": { ".": "./index.js" } }"#,
            ),
            (
                "node_modules/foo/node_modules/bar/index.js",
                "// @flow\ndeclare export const version: 1;\n",
            ),
        ])
    }

    #[test]
    fn a_nested_consumer_reads_the_copy_installed_beside_it() {
        // ubugeeei-prod/uf#486: before this, both importers got the hoisted
        // copy and the files inside `foo` were typed against version 2.
        let directory = two_versions();
        let root = root_of(&directory);
        let mut read = FxHashSet::default();

        let loaded = load_packages(
            &root,
            &[import("bar", "node_modules/foo/index.js")],
            &mut read,
        );

        assert_eq!(
            loaded
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            [
                "node_modules/foo/node_modules/bar/index.js",
                "node_modules/foo/node_modules/bar/package.json",
            ]
        );
    }

    #[test]
    fn both_copies_can_be_in_one_batch() {
        let directory = two_versions();
        let root = root_of(&directory);
        let mut read = FxHashSet::default();

        let loaded = load_packages(
            &root,
            &[
                import("bar", "src/app.js"),
                import("bar", "node_modules/foo/index.js"),
            ],
            &mut read,
        );

        let paths: Vec<&str> = loaded.iter().map(|file| file.path.as_str()).collect();
        assert!(paths.contains(&"node_modules/bar/index.js"), "{paths:?}");
        assert!(
            paths.contains(&"node_modules/foo/node_modules/bar/index.js"),
            "{paths:?}"
        );
        // Two directories, so a second round asks for neither again.
        assert_eq!(read.len(), 2);
    }

    #[test]
    fn a_package_that_ships_no_flow_is_read_once_and_not_again() {
        let directory = project(&[
            (
                "node_modules/plain/package.json",
                r#"{ "name": "plain", "main": "./index.js" }"#,
            ),
            ("node_modules/plain/index.js", "module.exports = 1;\n"),
        ]);
        let root = root_of(&directory);
        let mut read = FxHashSet::default();

        assert!(
            load_packages(&root, &[import("plain", "src/app.js")], &mut read).is_empty(),
            "a dependency that does not opt into Flow exports `any` either way"
        );
        // Recorded even so, or the caller's loop would walk it every round.
        assert!(read.contains("node_modules/plain"));
        assert!(load_packages(&root, &[import("plain", "src/app.js")], &mut read).is_empty());
    }
}
