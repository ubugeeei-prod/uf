//! The import graph, and what one edit invalidates.
//!
//! Watch mode is only worth having if it is *exact*: rerunning everything is
//! not watch mode, and rerunning too little is a lie. So the graph answers one
//! question precisely — which test files transitively import the file that
//! changed — and the answer is closed under the reverse edges, so a change to a
//! module three levels down still reaches the test that depends on it.
//!
//! Import specifiers are read with `uf_rsc::scan_imports`, the same lexer the
//! RSC module graph uses. There is deliberately no second scanner here: a
//! runner that disagreed with the bundler about what a file imports would
//! invalidate the wrong things.
//!
//! Only relative specifiers resolve to project files; a bare `react` is a
//! package, lives outside the watched tree, and is dropped. Resolution goes
//! through [`crate::path::normalize_relative`], so a specifier that climbs out
//! of the project (`../../../../etc/passwd`) resolves to nothing rather than to
//! a path the watcher would then stat.

use compact_str::CompactString;
use uf_infra::{FxHashMap, FxHashSet};
use uf_rsc::scan_imports;

use crate::path::normalize_relative;

/// Extensions tried when a specifier omits one, in resolution order.
pub const MODULE_EXTENSIONS: [&str; 4] = ["js", "jsx", "mjs", "cjs"];

/// Most modules a graph will hold.
pub const MAX_MODULES: usize = 200_000;

/// Most import edges recorded for one module.
pub const MAX_IMPORTS_PER_MODULE: usize = 1_000;

/// Largest source the graph will scan, matching `uf_rsc`.
pub const MAX_SOURCE_BYTES: usize = uf_rsc::MAX_SOURCE_BYTES;

/// Which modules import which.
///
/// Edges are stored as *targets* rather than as resolved module ids, and
/// resolution happens when the reverse map is built. That is what lets a file
/// added later close an edge that pointed nowhere when it was first scanned,
/// without rescanning anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportGraph {
    targets: FxHashMap<CompactString, Vec<CompactString>>,
}

impl ImportGraph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a graph from every module in the project.
    pub fn build<'a>(modules: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut graph = Self::new();
        for (path, source) in modules {
            graph.insert(path, source);
        }
        graph
    }

    /// Record or replace one module's imports.
    ///
    /// A module's outgoing edges depend only on its own source, so an edit is
    /// exactly one call: watch mode never rescans a file that did not change.
    pub fn insert(&mut self, path: &str, source: &str) {
        if !crate::path::is_safe_relative(path) {
            return;
        }
        if !self.targets.contains_key(path) && self.targets.len() >= MAX_MODULES {
            return;
        }
        self.targets
            .insert(CompactString::from(path), import_targets(path, source));
    }

    /// Forget a module that no longer exists.
    pub fn remove(&mut self, path: &str) {
        self.targets.remove(path);
    }

    /// Whether the graph knows about `path`.
    pub fn contains(&self, path: &str) -> bool {
        self.targets.contains_key(path)
    }

    /// How many modules the graph holds.
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// Whether the graph holds nothing.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// The resolved dependencies of one module, sorted.
    pub fn dependencies_of(&self, path: &str) -> Vec<CompactString> {
        let modules: FxHashSet<&str> = self.targets.keys().map(CompactString::as_str).collect();
        let mut resolved: Vec<CompactString> = self
            .targets
            .get(path)
            .into_iter()
            .flatten()
            .filter_map(|target| resolve_target(target, &modules))
            .collect();
        resolved.sort_unstable();
        resolved.dedup();
        resolved
    }

    /// Every module that transitively imports any of `changed`, including the
    /// changed modules themselves.
    ///
    /// Sorted and deduplicated, so the answer does not depend on hash order.
    /// Cycles are safe: a module is visited once.
    pub fn affected<'a>(&self, changed: impl IntoIterator<Item = &'a str>) -> Vec<CompactString> {
        let dependents = self.dependents();
        let mut seen: FxHashSet<CompactString> = FxHashSet::default();
        let mut queue: Vec<CompactString> = Vec::new();

        for path in changed {
            let path = CompactString::from(path);
            if seen.insert(path.clone()) {
                queue.push(path);
            }
        }

        while let Some(path) = queue.pop() {
            let Some(importers) = dependents.get(&path) else {
                continue;
            };
            for importer in importers {
                if seen.insert(importer.clone()) {
                    queue.push(importer.clone());
                }
            }
        }

        let mut affected: Vec<CompactString> = seen.into_iter().collect();
        affected.sort_unstable();
        affected
    }

    /// The affected modules that `is_test` accepts: exactly the files a watch
    /// run must re-execute.
    pub fn affected_tests<'a>(
        &self,
        changed: impl IntoIterator<Item = &'a str>,
        is_test: impl Fn(&str) -> bool,
    ) -> Vec<CompactString> {
        self.affected(changed)
            .into_iter()
            .filter(|path| is_test(path.as_str()))
            .collect()
    }

    /// The reverse edges: module -> the modules that import it.
    fn dependents(&self) -> FxHashMap<CompactString, Vec<CompactString>> {
        let modules: FxHashSet<&str> = self.targets.keys().map(CompactString::as_str).collect();
        let mut dependents: FxHashMap<CompactString, Vec<CompactString>> = FxHashMap::default();
        for (importer, targets) in &self.targets {
            for target in targets {
                let Some(resolved) = resolve_target(target, &modules) else {
                    continue;
                };
                dependents
                    .entry(resolved)
                    .or_default()
                    .push(importer.clone());
            }
        }
        dependents
    }
}

/// The normalized, still-unresolved targets one module imports.
fn import_targets(path: &str, source: &str) -> Vec<CompactString> {
    if source.len() > MAX_SOURCE_BYTES {
        return Vec::new();
    }
    let mut targets: Vec<CompactString> = scan_imports(source)
        .into_iter()
        .filter_map(|import| normalize_relative(path, &import.specifier))
        .take(MAX_IMPORTS_PER_MODULE)
        .collect();
    targets.sort_unstable();
    targets.dedup();
    targets
}

/// Resolve a normalized target onto a module that actually exists.
///
/// Node's resolution order, minus anything that would touch the file system:
/// the path as written, then each extension, then a directory index. Resolving
/// against the known module set rather than against `stat` keeps the graph a
/// pure function of the sources it was given.
fn resolve_target(target: &str, modules: &FxHashSet<&str>) -> Option<CompactString> {
    if modules.contains(target) {
        return Some(CompactString::from(target));
    }
    for extension in MODULE_EXTENSIONS {
        let candidate = format!("{target}.{extension}");
        if modules.contains(candidate.as_str()) {
            return Some(CompactString::from(candidate));
        }
    }
    for extension in MODULE_EXTENSIONS {
        let candidate = format!("{target}/index.{extension}");
        if modules.contains(candidate.as_str()) {
            return Some(CompactString::from(candidate));
        }
    }
    None
}
