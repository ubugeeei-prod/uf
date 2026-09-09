//! Reading a project's library definitions, for the types its dependencies do
//! not ship.
//!
//! A libdef is not a source file the project owns in `uf_project`'s sense:
//! `uf lint` does not report on it and a diagnostic never points inside it —
//! [`declared_paths`] is what makes that true, and #699 is what it was not
//! true of. `uf fmt` *does* rewrite one, and deliberately: a libdef is Flow
//! source in a repository uf formats, and reformatting it invents no opinion
//! about the project, where a lint rule about avoiding `any` does. It is
//! *declaration* — the
//! `declare module` block that says what `@xyflow/react` exports, the
//! `declare type` that says what a project-wide global is — and it belongs to
//! the type environment rather than to the batch. So it is read here, beside
//! [`super::dependencies`], and handed to `uf_check` as library definitions
//! rather than as sources.
//!
//! [`uf_check::lib_paths`] says which directories to read, by parsing
//! `.flowconfig`'s `[libs]` with Flow's own parser. This module turns each of
//! those into files, with the rules Flow's `ordered_and_unordered_lib_paths`
//! uses:
//!
//! * an entry may name a **file** or a **directory**, and a directory is read
//!   recursively;
//! * `.json` is never a library definition, whatever else is;
//! * the files of one entry are read in **sorted order**, and the entries in
//!   the order the config declared them, because a later definition shadows an
//!   earlier one and the order is therefore part of what the project means.
//!
//! # What a missing directory means
//!
//! Nothing. `flow-typed` is on the list whether or not it exists — Flow adds it
//! unasked — so an entry that names no directory is the ordinary case for a
//! project that has no libdefs at all, and costs one directory read that
//! misses.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use uf_check::LibPaths;
use uf_infra::FxHashSet;
use uf_lint::SourceFile;
use uf_project::SourceKind;
use walkdir::WalkDir;

/// How many files the library definitions may hold before the rest are left
/// unread.
///
/// A bound on a directory nobody meant to name, not a policy about how many
/// libdefs a project may have: `flow-typed` for a large application holds
/// dozens, and `node_modules` pointed at by mistake holds a hundred thousand.
/// Past this the extras are dropped rather than the whole set, because a
/// project whose libdefs went missing reports one error per declared type and
/// no explanation.
const MAX_LIBDEF_FILES: usize = 2_000;

/// Every library definition the project's configuration names, as paths
/// relative to the project root.
///
/// The set [`super::super::lint::run_lint`] takes a file *out* of the lint with.
/// This module's first line says a libdef is not a source file the project
/// owns — `uf lint` does not report on it, and a diagnostic never points
/// inside it — and until this existed that was true of the type check and
/// false of the lint: the scan collects `flow-typed/` like any other
/// directory, so `declare type FoldingRangeProvider = any` in a hand-written
/// libdef was reported as `flow/unclear-type` and failed the build. An `any`
/// is what a libdef for an untyped package is *made of*; a lint rule about
/// avoiding it has nothing to say there. See ubugeeei-prod/uf#699.
///
/// Shares [`files_under`] with [`load`] rather than restating the rule, so the
/// files uf declines to lint are exactly the files it merges.
pub(crate) fn declared_paths(root: &Utf8Path, paths: &LibPaths) -> FxHashSet<String> {
    let mut declared = FxHashSet::default();
    for entry in paths.paths() {
        for path in files_under(root, entry) {
            if declared.len() == MAX_LIBDEF_FILES {
                return declared;
            }
            declared.insert(path.into_string());
        }
    }
    declared
}

/// Every library definition the project's configuration names, in merge order.
pub(super) fn load(root: &Utf8Path, paths: &LibPaths) -> Vec<SourceFile> {
    let mut loaded: Vec<SourceFile> = Vec::new();
    let mut seen: FxHashSet<String> = FxHashSet::default();
    for entry in paths.paths() {
        for path in files_under(root, entry) {
            if loaded.len() == MAX_LIBDEF_FILES {
                return loaded;
            }
            // A file two entries both name is merged once. Merging it twice
            // would be a file shadowing itself, which changes nothing except
            // how long the merge takes.
            if !seen.insert(path.as_str().to_owned()) {
                continue;
            }
            let Ok(source) = fs::read_to_string(root.join(&path)) else {
                continue;
            };
            loaded.push(SourceFile {
                path: path.into_string(),
                source,
            });
        }
    }
    loaded
}

/// The library definition files one `[libs]` entry names, sorted, as paths
/// relative to the project root.
///
/// Relative to the root and spelled the way the entry spelled it, not the way
/// a symlink resolved: a libdef's path is what a cache key and an error message
/// carry, and a project that reaches its shared declarations through a link
/// should not see the link's target in either.
fn files_under(root: &Utf8Path, entry: &str) -> Vec<Utf8PathBuf> {
    let base = root.join(entry);
    // Resolved before the walk rather than during it, for the reason
    // `dependencies::read_package` gives: the directory itself may be a
    // symlink — a monorepo pointing every package at one `flow-typed` — and a
    // walk that followed every link it met could leave the directory
    // entirely.
    let Ok(resolved) = base.canonicalize_utf8() else {
        return Vec::new();
    };
    if resolved.is_file() {
        // An entry that names a file is that file, whatever its extension:
        // Flow reads it because the config asked for it by name.
        return vec![Utf8PathBuf::from(entry)];
    }

    let mut found = Vec::new();
    for walked in WalkDir::new(resolved.as_std_path()).into_iter().flatten() {
        if !walked.file_type().is_file() {
            continue;
        }
        let Ok(path) = Utf8PathBuf::from_path_buf(walked.path().to_path_buf()) else {
            continue;
        };
        // Flow's own rule: a lib directory holds module files, and JSON is
        // never one. `SourceKind::is_flow` is uf's spelling of the same set.
        if !matches!(SourceKind::from_path(&path), Some(kind) if kind.is_flow()) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(&resolved) else {
            continue;
        };
        found.push(Utf8Path::new(entry).join(relative));
    }
    // Sorted, because a directory walk is not ordered and the merge order is
    // part of what the libdefs declare.
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_directory_is_read_recursively_and_in_sorted_order() {
        let directory = project(&[
            ("flow-typed/b.js", "declare type B = string;\n"),
            ("flow-typed/a.js", "declare type A = string;\n"),
            ("flow-typed/nested/c.js", "declare type C = string;\n"),
        ]);
        let root = root_of(&directory);

        let loaded = load(
            &root,
            &uf_check::lib_paths(root.as_std_path()).expect("no config"),
        );

        assert_eq!(
            loaded
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            [
                "flow-typed/a.js",
                "flow-typed/b.js",
                "flow-typed/nested/c.js"
            ]
        );
    }

    #[test]
    fn json_in_a_lib_directory_is_not_a_library_definition() {
        let directory = project(&[
            ("flow-typed/a.js", "declare type A = string;\n"),
            ("flow-typed/versions.json", "{}\n"),
        ]);
        let root = root_of(&directory);

        let loaded = load(
            &root,
            &uf_check::lib_paths(root.as_std_path()).expect("no config"),
        );

        assert_eq!(
            loaded
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["flow-typed/a.js"]
        );
    }

    #[test]
    fn a_project_with_no_libdefs_reads_none() {
        let directory = project(&[("src/app.js", "export const a = 1;\n")]);
        let root = root_of(&directory);

        assert!(
            load(
                &root,
                &uf_check::lib_paths(root.as_std_path()).expect("no config")
            )
            .is_empty()
        );
    }

    #[test]
    fn a_configured_directory_is_read_after_flow_typed() {
        let directory = project(&[
            (".flowconfig", "[libs]\nlibdefs\n"),
            ("flow-typed/a.js", "declare type A = string;\n"),
            ("libdefs/b.js", "declare type B = string;\n"),
        ]);
        let root = root_of(&directory);

        let loaded = load(
            &root,
            &uf_check::lib_paths(root.as_std_path()).expect("the config"),
        );

        // Order is merge order, and a later definition shadows an earlier one.
        assert_eq!(
            loaded
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["flow-typed/a.js", "libdefs/b.js"]
        );
    }

    #[test]
    fn an_entry_that_names_a_file_reads_that_file() {
        let directory = project(&[
            (".flowconfig", "[libs]\ntypes/globals.js\n"),
            ("types/globals.js", "declare type G = string;\n"),
        ]);
        let root = root_of(&directory);

        let loaded = load(
            &root,
            &uf_check::lib_paths(root.as_std_path()).expect("the config"),
        );

        assert_eq!(
            loaded
                .iter()
                .map(|file| file.path.as_str())
                .collect::<Vec<_>>(),
            ["types/globals.js"]
        );
    }
}
