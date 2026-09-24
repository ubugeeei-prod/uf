//! Turning a module specifier into another file in the same batch.
//!
//! This is the lexical half of Flow's node resolver — `flow_services_module`'s
//! `Node.resolve_relative` — restricted to what an in-memory batch can answer.
//! Flow walks a real filesystem: it tries the path as written, then the path
//! with each of `module.file_ext` appended, then treats the path as a package
//! directory and reads its `package.json`. `uf_check` is handed a list of
//! sources and no filesystem, so the first two steps carry over exactly and the
//! third becomes "look for an `index` file" — a *relative* specifier naming a
//! directory is not a package, and reading a manifest to enter one is
//! [`super::packages`]'s job rather than this module's.
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

/// The module a `.flow` file stands in for — `lib/index.js` for
/// `lib/index.js.flow` — or [`None`] for any other path.
///
/// Only a path Flow would resolve an import to has one. A `.flow` suffix after
/// anything else, like the `index.d.ts.flow` a translated declaration file is
/// filed under, shadows nothing.
fn shadowed_module(path: &str) -> Option<&str> {
    path.strip_suffix(".flow").filter(|module| {
        IMPLICIT_EXTENSIONS
            .iter()
            .any(|extension| module.ends_with(extension))
    })
}

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
            // A `..` cancels the segment before it when there is one to
            // cancel. When there is not, the path is *above* the batch root —
            // which a hoisted dependency in a workspace is, since npm installs
            // an app's packages in the repository root's `node_modules` and
            // the app is below it — so the `..` is kept rather than the path
            // refused. It used to be refused, on the assumption that nothing
            // in a batch is above its root, and that assumption is what made
            // every `@uniflowed/*` type in a workspace app resolve to nothing.
            // See ubugeeei-prod/uf#654.
            match segments.last() {
                Some(last) if *last != ".." => {
                    segments.pop();
                }
                _ => segments.push(".."),
            }
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
    /// The `.flow` files in the batch, by the path of the module each one
    /// stands beside: `lib/index.js` for `lib/index.js.flow`.
    ///
    /// Flow's resolver tries `<path>.flow` before `<path>` for every file it
    /// resolves to, and that is how a package ships Flow for JavaScript it
    /// compiled: `index.js` is what a runtime loads, `index.js.flow` what a
    /// checker reads. Kept apart from `by_path` because the rule is about
    /// *imports* — a file somebody asked to check is still that file, and
    /// [`Self::index_of`] answers for it exactly.
    shadows: HashMap<CompactString, usize>,
}

impl ModuleIndex {
    /// Index a batch by path.
    ///
    /// A duplicate path keeps the first source, matching `check_sources`'s
    /// own order-defined result: the batch is checked front to back, so the
    /// first occurrence is the one a reader saw reported.
    pub(super) fn new<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut by_path = HashMap::new();
        let mut shadows = HashMap::new();
        for (index, path) in paths.into_iter().enumerate() {
            let path = normalize(path);
            if let Some(module) = shadowed_module(&path) {
                shadows.entry(module.to_compact_string()).or_insert(index);
            }
            by_path.entry(path).or_insert(index);
        }
        Self { by_path, shadows }
    }

    /// The source `specifier` resolves to, imported from `importer`.
    ///
    /// [`None`] means no file in the batch answers to it — which is not the
    /// same as the module not existing, only that this check was not handed it.
    ///
    /// A query or a fragment on a module specifier names the same file: Node
    /// and every bundler key a module *instance* by its whole URL, so
    /// `./log.js?a-second-copy` is a second evaluation of `./log.js` with the
    /// same exports. It resolves to that file, typed as it is, where it used to
    /// miss and come back `any`. The query of a Vite asset import (`?raw`,
    /// `?url`, `?worker`, ...) changes what the import *is*, so a specifier
    /// [`super::assets::declared_module_for`] recognises keeps its query and
    /// goes on to that declared shape instead.
    pub(super) fn resolve(&self, importer: &str, specifier: &str) -> Option<usize> {
        if !is_relative(specifier) {
            return None;
        }
        let specifier = if super::assets::declared_module_for(specifier).is_some() {
            specifier
        } else {
            specifier
                .split_once(['?', '#'])
                .map_or(specifier, |(path, _)| path)
        };
        self.resolve_file(&join(importer, specifier)?)
    }

    /// The source at `base`, tried as written, then with each of Flow's
    /// extensions, then as a directory holding an `index` file.
    ///
    /// This is the half of node resolution that applies to a path *however* the
    /// path was arrived at: a relative specifier, a package's `main`, or a
    /// subpath under a package that publishes no `exports` map. What an
    /// `exports` map names goes through [`Self::lookup`] instead, because that
    /// is a file rather than a path to search from.
    pub(super) fn resolve_file(&self, base: &str) -> Option<usize> {
        if let Some(index) = self.lookup(base) {
            return Some(index);
        }

        let mut candidate = String::with_capacity(base.len() + 1 + "index.jsx".len());
        self.with_suffixes(base, &IMPLICIT_EXTENSIONS, "", &mut candidate)
            .or_else(|| self.with_suffixes(base, &INDEX_BASENAMES, "/", &mut candidate))
    }

    fn with_suffixes(
        &self,
        base: &str,
        suffixes: &[&str],
        separator: &str,
        candidate: &mut String,
    ) -> Option<usize> {
        candidate.clear();
        candidate.push_str(base);
        candidate.push_str(separator);
        let prefix_len = candidate.len();
        suffixes.iter().find_map(|suffix| {
            candidate.truncate(prefix_len);
            candidate.push_str(suffix);
            self.lookup(candidate)
        })
    }

    /// The source an import of exactly this path loads: no extension and no
    /// `index`, but the `.flow` file beside the path when the batch has one,
    /// because that is the file Flow reads for it.
    pub(super) fn lookup(&self, path: &str) -> Option<usize> {
        self.shadows
            .get(path)
            .or_else(|| self.by_path.get(path))
            .copied()
    }

    /// The source at this path, however the path is spelled.
    ///
    /// [`Self::lookup`] is keyed by the [`normalize`]d form, which is what a
    /// resolution produces; a path that came from somewhere else — a `--path`
    /// argument, a caller's own list of files to start from — has not been
    /// through that, so `./src/app.js` and `src/app.js` would be two keys.
    ///
    /// Exact, unlike an import: a file somebody asked to check is that file
    /// even when a `.flow` file stands beside it.
    pub(super) fn index_of(&self, path: &str) -> Option<usize> {
        self.by_path.get(&normalize(path)).copied()
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
    use uf_profiler::ThreadWindow;

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

    /// A `..` that cannot be cancelled is kept, not refused.
    ///
    /// It used to be refused, on the stated assumption that nothing in a batch
    /// is above its root. That stopped being true when `uf check` learned to
    /// read a *hoisted* dependency: npm installs a workspace app's packages in
    /// the repository root's `node_modules`, which is above the app, so a
    /// manifest at `../node_modules/dep/package.json` has to be able to
    /// resolve its own `main`. Refusing it is what made every `@uniflowed/*`
    /// type in such an app resolve to nothing. See ubugeeei-prod/uf#654.
    #[test]
    fn climbing_above_the_project_root_keeps_the_dots() {
        assert_eq!(
            join("app.js", "../outside.js").as_deref(),
            Some("../outside.js")
        );
        assert_eq!(
            join("a/b.js", "../../../outside.js").as_deref(),
            Some("../../outside.js")
        );
        // The case it exists for: a hoisted package resolving its own entry.
        assert_eq!(
            join("../node_modules/dep/package.json", "index.js").as_deref(),
            Some("../node_modules/dep/index.js")
        );
    }

    /// And escaping still resolves to nothing, because the batch decides.
    ///
    /// Keeping the `..` moved the refusal rather than removing it: a path is
    /// only a module if the batch holds one at it, and a relative import that
    /// climbs out of the project reaches nothing uf read.
    #[test]
    fn a_path_above_the_root_is_a_module_only_when_the_batch_holds_one() {
        let index = ModuleIndex::new(["app.js", "../node_modules/dep/index.js"]);

        assert_eq!(
            index.resolve("app.js", "../node_modules/dep/index.js"),
            Some(1)
        );
        assert_eq!(index.resolve("app.js", "../outside.js"), None);
        assert_eq!(index.resolve("app.js", "../../../../etc/passwd"), None);
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

    /// A query names a second instance of the same file, so it resolves to
    /// that file; an asset query keeps its meaning and resolves to no source.
    #[test]
    fn a_query_or_fragment_names_the_same_module_unless_it_makes_an_asset() {
        let index = ModuleIndex::new(["src/log.js", "src/app.js"]);

        assert_eq!(
            index.resolve("src/app.js", "./log.js?a-second-copy"),
            Some(0)
        );
        assert_eq!(index.resolve("src/app.js", "./log?copy&x=1"), Some(0));
        assert_eq!(index.resolve("src/app.js", "./log.js#frag"), Some(0));
        assert_eq!(index.resolve("src/app.js", "./log.js?raw"), None);
        assert_eq!(index.resolve("src/app.js", "./log.js?url"), None);
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
    fn an_exact_lookup_does_not_fall_back_to_an_extension_or_an_index() {
        // What an `exports` map names is a file, not a path to search from, so
        // [`super::packages`] resolves its targets through `lookup` alone.
        let index = ModuleIndex::new(["src/a.js", "src/b/index.js"]);

        assert_eq!(index.lookup("src/a.js"), Some(0));
        assert_eq!(index.lookup("src/a"), None);
        assert_eq!(index.lookup("src/b"), None);
        assert_eq!(index.resolve_file("src/b"), Some(1));
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

    /// One candidate buffer per resolution, however many fallbacks it tries.
    ///
    /// An extensionless miss looks the path up as written, then with each of
    /// four extensions, then with each of four `index` basenames, and every
    /// one of those lookups goes through a single `String` the resolution
    /// allocates once. So the ceiling is that invariant, one allocation per
    /// resolution, rather than a guess at how much noise to tolerate. A fresh
    /// candidate per fallback, which this exists to catch, costs the same 128
    /// misses 2,358.
    ///
    /// It can be exact because it is counted on this thread alone. It used to
    /// be read from the allocator's process-wide counters, in a binary whose
    /// other tests run on other threads at the same moment, so the figure was
    /// this loop plus whatever they allocated meanwhile: 7,239 in the quietest
    /// of eight windows on a CI runner, against a 6,500 ceiling that had been
    /// raised twice to absorb exactly that — and that the regression itself
    /// sat far underneath (ubugeeei-prod/uf#1015).
    #[test]
    fn repeated_extensionless_misses_reuse_one_candidate_buffer_per_resolution() {
        let index = ModuleIndex::new(["app.js"]);
        let bases: Vec<String> = (0..128).map(|index| format!("missing{index}")).collect();
        let one_per_resolution = u64::try_from(bases.len()).expect("the count fits in u64");

        let window = ThreadWindow::open();
        for base in &bases {
            assert_eq!(index.resolve_file(base), None);
        }
        let delta = window.close();

        assert!(
            delta.allocations <= one_per_resolution,
            "{} extensionless misses took {} allocations, over the ceiling of one per \
             resolution. That usually means path resolution is allocating a fresh \
             candidate for every extension and index fallback.",
            bases.len(),
            delta.allocations,
        );
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
    fn a_flow_file_answers_imports_of_the_module_it_sits_beside() {
        let index = ModuleIndex::new(["lib/index.js", "lib/index.js.flow", "app.js"]);

        assert_eq!(index.resolve("app.js", "./lib/index.js"), Some(1));
        assert_eq!(index.resolve("app.js", "./lib/index"), Some(1));
        assert_eq!(index.resolve("app.js", "./lib"), Some(1));
        assert_eq!(index.lookup("lib/index.js"), Some(1));
        // A file somebody asked to check is still that file.
        assert_eq!(index.index_of("lib/index.js"), Some(0));
    }

    #[test]
    fn a_flow_suffix_after_anything_but_a_module_shadows_nothing() {
        let index = ModuleIndex::new(["index.d.ts.flow", "app.js"]);

        assert_eq!(index.lookup("index.d.ts"), None);
        assert_eq!(index.resolve("app.js", "./index.d.ts.flow"), Some(0));
    }

    #[test]
    fn the_first_source_wins_a_duplicated_path() {
        let index = ModuleIndex::new(["a.js", "a.js", "b.js"]);

        assert_eq!(index.resolve("b.js", "./a.js"), Some(0));
    }
}
