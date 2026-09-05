//! Turning a module specifier into another file in the same batch.
//!
//! This is the lexical half of Flow's node resolver — `flow_services_module`'s
//! `Node.resolve_relative` — restricted to what an in-memory batch can answer.
//! Flow walks a real filesystem: it tries the path as written, then the path
//! with each of `module.file_ext` appended, then treats the path as a package
//! directory and reads its `package.json`. `uf_check` is handed a list of
//! sources and no filesystem, so the first two steps carry over exactly and the
//! third becomes "look for an `index` file", because a `package.json` is not in
//! the batch to read.
//!
//! Everything here is pure: it maps `(importer, specifier)` onto a path, and
//! [`ModuleIndex`] says whether the batch holds that path. What happens to a
//! specifier that resolves to nothing is decided in [`super::project`], not
//! here.

use std::collections::HashMap;

use compact_str::{CompactString, ToCompactString};

/// The extensions a specifier without one is tried with, in order.
///
/// Flow's `module.file_ext` default is this list; `.json` is deliberately left
/// off, because a JSON module has a signature `uf` does not build and would
/// resolve to a typed module with nothing in it.
const IMPLICIT_EXTENSIONS: [&str; 4] = [".js", ".mjs", ".cjs", ".jsx"];

/// The basenames a directory specifier is tried with.
const INDEX_BASENAMES: [&str; 4] = ["index.js", "index.mjs", "index.cjs", "index.jsx"];

/// Whether `specifier` names a file relative to the importer rather than a
/// package.
///
/// Flow's own resolver branches on exactly this: a specifier starting with `.`
/// is resolved against the importing file's directory, and anything else goes
/// through package resolution — `node_modules`, Haste, or a `declare module`.
pub(super) fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

/// The path `specifier` names, resolved against the file that imported it.
///
/// Returns [`None`] when the specifier climbs above the project root, which no
/// file in the batch can be: the batch's paths are project-relative, so
/// `../../elsewhere.js` is by construction outside it.
pub(super) fn join(importer: &str, specifier: &str) -> Option<CompactString> {
    let mut segments: Vec<&str> = Vec::new();
    // The importer's own basename is not part of its directory.
    let directory = importer.rsplit_once('/').map_or("", |(head, _)| head);
    for segment in directory
        .split('/')
        .chain(specifier.split('/'))
        .filter(|segment| !segment.is_empty() && *segment != ".")
    {
        if segment == ".." {
            // A `..` that cannot be cancelled has left the project root, and
            // nothing in the batch is above it.
            segments.pop()?;
        } else {
            segments.push(segment);
        }
    }
    (!segments.is_empty()).then(|| segments.join("/").to_compact_string())
}

/// Which source in a batch a path belongs to.
///
/// Built once per batch. A resolution is two hash lookups in the common case —
/// the specifier as written, then with `.js` — so a project whose every file
/// imports every other stays linear in the number of imports rather than in
/// the number of files.
pub(super) struct ModuleIndex {
    by_path: HashMap<CompactString, usize>,
}

impl ModuleIndex {
    /// Index a batch by path.
    ///
    /// A duplicate path keeps the first source, matching `check_sources`'s
    /// own order-defined result: the batch is checked front to back, so the
    /// first occurrence is the one a reader saw reported.
    pub(super) fn new<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut by_path = HashMap::new();
        for (index, path) in paths.into_iter().enumerate() {
            by_path.entry(normalize(path)).or_insert(index);
        }
        Self { by_path }
    }

    /// The source `specifier` resolves to, imported from `importer`.
    ///
    /// [`None`] means no file in the batch answers to it — which is not the
    /// same as the module not existing, only that this check was not handed it.
    pub(super) fn resolve(&self, importer: &str, specifier: &str) -> Option<usize> {
        if !is_relative(specifier) {
            return None;
        }
        let base = join(importer, specifier)?;
        self.lookup(&base)
            .or_else(|| self.with_suffixes(&base, &IMPLICIT_EXTENSIONS, ""))
            .or_else(|| self.with_suffixes(&base, &INDEX_BASENAMES, "/"))
    }

    fn with_suffixes(&self, base: &str, suffixes: &[&str], separator: &str) -> Option<usize> {
        suffixes
            .iter()
            .find_map(|suffix| self.lookup(&format!("{base}{separator}{suffix}")))
    }

    fn lookup(&self, path: &str) -> Option<usize> {
        self.by_path.get(path).copied()
    }
}

/// A batch path in the shape [`join`] produces.
///
/// `./src/app.js` and `src/app.js` are the same file to a reader and have to be
/// the same key here, or a relative import would miss a source that is in the
/// batch under a differently spelled path.
fn normalize(path: &str) -> CompactString {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
    {
        if segment == ".." {
            if segments.pop().is_none() {
                // Above the root: keep the path as written rather than
                // silently indexing it somewhere it is not.
                return path.to_compact_string();
            }
        } else {
            segments.push(segment);
        }
    }
    segments.join("/").to_compact_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sibling_resolves_against_the_importing_directory() {
        assert_eq!(
            join("packages/immer/patches.js", "./draft.js").as_deref(),
            Some("packages/immer/draft.js")
        );
    }

    #[test]
    fn a_parent_specifier_climbs_one_directory() {
        assert_eq!(
            join("packages/ui/internal/field.js", "../tokens.js").as_deref(),
            Some("packages/ui/tokens.js")
        );
    }

    #[test]
    fn dot_segments_collapse() {
        assert_eq!(
            join("a/b/c.js", "./../d/./e.js").as_deref(),
            Some("a/d/e.js")
        );
    }

    #[test]
    fn climbing_above_the_project_root_resolves_to_nothing() {
        assert_eq!(join("app.js", "../outside.js"), None);
        assert_eq!(join("a/b.js", "../../../outside.js"), None);
    }

    #[test]
    fn only_a_dot_prefixed_specifier_is_relative() {
        assert!(is_relative("./a.js"));
        assert!(is_relative("../a.js"));
        assert!(is_relative("."));
        assert!(!is_relative("react"));
        assert!(!is_relative("@uniflowed/react"));
        assert!(!is_relative("node:fs"));
        // A specifier that merely starts with a dot in its first segment is a
        // package name, not a path.
        assert!(!is_relative(".hidden"));
    }

    #[test]
    fn a_specifier_written_with_its_extension_resolves_directly() {
        let index = ModuleIndex::new(["a.js", "b.js"]);

        assert_eq!(index.resolve("b.js", "./a.js"), Some(0));
    }

    #[test]
    fn an_extensionless_specifier_tries_flows_own_extensions() {
        let index = ModuleIndex::new(["src/a.js", "src/b.mjs", "src/c.js"]);

        assert_eq!(index.resolve("src/c.js", "./a"), Some(0));
        assert_eq!(index.resolve("src/c.js", "./b"), Some(1));
    }

    #[test]
    fn a_directory_specifier_resolves_to_its_index() {
        let index = ModuleIndex::new(["src/internal/index.js", "src/app.js"]);

        assert_eq!(index.resolve("src/app.js", "./internal"), Some(0));
        assert_eq!(index.resolve("src/app.js", "./internal/"), Some(0));
    }

    #[test]
    fn an_exact_match_beats_an_extension_or_an_index() {
        let index = ModuleIndex::new(["src/a", "src/a.js", "src/a/index.js", "src/b.js"]);

        assert_eq!(index.resolve("src/b.js", "./a"), Some(0));
    }

    #[test]
    fn a_bare_specifier_never_resolves_to_a_project_file() {
        let index = ModuleIndex::new(["react.js", "a.js"]);

        assert_eq!(index.resolve("a.js", "react"), None);
    }

    #[test]
    fn a_specifier_naming_nothing_in_the_batch_resolves_to_nothing() {
        let index = ModuleIndex::new(["a.js"]);

        assert_eq!(index.resolve("a.js", "./missing.js"), None);
    }

    #[test]
    fn a_batch_path_written_with_a_leading_dot_is_indexed_under_its_plain_form() {
        let index = ModuleIndex::new(["./src/a.js", "./src/b.js"]);

        assert_eq!(index.resolve("src/b.js", "./a.js"), Some(0));
    }

    #[test]
    fn a_file_can_resolve_to_itself() {
        let index = ModuleIndex::new(["a.js"]);

        assert_eq!(index.resolve("a.js", "./a.js"), Some(0));
    }

    #[test]
    fn the_first_source_wins_a_duplicated_path() {
        let index = ModuleIndex::new(["a.js", "a.js", "b.js"]);

        assert_eq!(index.resolve("b.js", "./a.js"), Some(0));
    }
}
