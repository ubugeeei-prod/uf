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

use std::fs;
use std::io::Read;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use uf_infra::FxHashSet;
use uf_lint::SourceFile;
use uf_project::SourceKind;
use walkdir::WalkDir;

/// The directory an installed package is read from.
const INSTALLED: &str = "node_modules";

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

/// Read every package `unresolved` names that has not been read already.
///
/// `read` is both the filter and the record: a package that was looked for and
/// not found is in it too, so a specifier that cannot be answered is not
/// searched for again on the next round.
pub(super) fn load_packages(
    root: &Utf8Path,
    unresolved: &[CompactString],
    read: &mut FxHashSet<String>,
) -> Vec<SourceFile> {
    let mut loaded = Vec::new();
    for specifier in unresolved {
        let Some(name) = package_name(specifier) else {
            continue;
        };
        if !read.insert(name.to_owned()) {
            continue;
        }
        loaded.extend(read_package(root, name));
    }
    loaded
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

/// Every Flow source and manifest one installed package holds.
fn read_package(root: &Utf8Path, name: &str) -> Vec<SourceFile> {
    let installed = root.join(INSTALLED).join(name);
    // Resolved through the link before the walk rather than during it. A
    // workspace package *is* a symlink — `uf install` links `packages/form`
    // into `node_modules/@uniflowed/` — so a walk that did not follow one
    // would find the very case this exists for empty, and a walk that followed
    // every link it met could leave the package entirely.
    let Ok(directory) = installed.canonicalize_utf8() else {
        return Vec::new();
    };
    if !directory.join("package.json").is_file() {
        return Vec::new();
    }

    let mut paths = Vec::new();
    let walk = WalkDir::new(directory.as_std_path())
        .into_iter()
        // A dependency's own dependencies are a package of their own: they are
        // reached, if at all, by a specifier of their own, which brings them
        // back through here under the name that named them.
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
        let (Ok(relative), Ok(source)) = (path.strip_prefix(&directory), fs::read_to_string(&path))
        else {
            continue;
        };
        files.push(SourceFile {
            // Under the name that named it, not under wherever the link went.
            // Every target inside a manifest is relative to the manifest, so a
            // package read as `node_modules/@uniflowed/form/package.json`
            // resolves its own `./index.js` to a path in the same shape, and
            // one read as `packages/form/package.json` would collide with the
            // copy the scan already holds.
            path: format!("{INSTALLED}/{name}/{relative}"),
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

    #[test]
    fn a_package_that_is_not_installed_reads_as_nothing() {
        let root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut read = FxHashSet::default();

        assert!(
            load_packages(
                &root,
                &[CompactString::const_new(
                    "@uniflowed/not-installed-anywhere"
                )],
                &mut read,
            )
            .is_empty()
        );
        // Recorded even so: a package that is not there is not there on the
        // next round either, and looking again would walk the tree for nothing.
        assert!(read.contains("@uniflowed/not-installed-anywhere"));
    }
}
