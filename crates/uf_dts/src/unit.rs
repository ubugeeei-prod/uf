//! One translation: the declaration files a set of entries reaches, and what
//! each of them becomes.
//!
//! The walk is TypeScript's own. From each entry it follows every import,
//! re-export, `import("…")` type and `/// <reference path>` that names a file
//! in the package, and it stops at a bare specifier: another package is
//! another translation, which the caller asks for when something imports it.
//!
//! Nothing here touches a filesystem. The caller hands over a function that
//! reads a package path, which is what makes a translation a function of the
//! files it read — and what lets the caller hash exactly those files to decide
//! whether a translation it cached is still the right one.

use std::collections::VecDeque;

use compact_str::{CompactString, ToCompactString};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use serde::{Deserialize, Serialize};
use uf_infra::{Bump, FxHashMap, FxHashSet};

use crate::emit;
use crate::hole::{Construct, Hole};
use crate::resolve::{self, candidates, is_relative};
use crate::summary::{self, Export, Imported, Kinds, Summary, Target};

/// What a translation produced.
///
/// Serializable because a translation is a function of the files it read, so
/// a caller that has recorded those files can keep one and hand it back
/// without translating again — which is what `uf check` does between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    /// One module per declaration file the entries reach, sorted by path.
    pub modules: Vec<Module>,
}

impl Translation {
    /// Every hole in every module, in module order.
    pub fn holes(&self) -> impl Iterator<Item = (&Module, &Hole)> {
        self.modules
            .iter()
            .flat_map(|module| module.holes.iter().map(move |hole| (module, hole)))
    }
}

/// One declaration file, translated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Module {
    /// The declaration file's package path, as the reader passed it.
    pub path: CompactString,
    /// The Flow declaration module, or [`None`] when the file did not parse.
    ///
    /// [`None`] rather than an empty module on purpose. An empty module
    /// exports nothing, so every import from it would be an error in the
    /// *consumer's* code; no module at all leaves those imports unresolved,
    /// which is `any` — and the [`Construct::ParseError`] hole says why.
    pub flow: Option<String>,
    /// What the translation could not carry over, in line order.
    pub holes: Vec<Hole>,
}

/// Translate the declaration files `entries` reach.
///
/// Paths are relative to the package root and `/`-separated, and `read`
/// answers one: the file's text, or [`None`] when there is no such file. It is
/// called at most once per path. An entry that cannot be read produces no
/// module.
pub fn translate(entries: &[&str], read: &mut dyn FnMut(&str) -> Option<String>) -> Translation {
    let arena = Bump::new();
    let mut unit = Unit {
        arena: &arena,
        sources: FxHashMap::default(),
        files: FxHashMap::default(),
    };

    let mut queue: VecDeque<CompactString> = entries
        .iter()
        .filter_map(|entry| resolve::normalize(entry))
        .map(CompactString::from)
        .collect();
    let mut seen: FxHashSet<CompactString> = FxHashSet::default();
    let mut reached: Vec<CompactString> = Vec::new();
    while let Some(path) = queue.pop_front() {
        if !seen.insert(path.clone()) {
            continue;
        }
        if unit.load(&path, read).is_none() {
            continue;
        }
        unit.summarize(&path, read);
        if let Some(File::Summarized(summary)) = unit.files.get(&path) {
            let mut next: Vec<&CompactString> = summary
                .targets
                .values()
                .filter_map(|target| match target {
                    Target::Module(module) => Some(module),
                    Target::Package(_) | Target::Missing(_) => None,
                })
                .collect();
            // The map's order is a hash's; the walk's must not be.
            next.sort();
            queue.extend(next.into_iter().cloned());
        }
        reached.push(path);
    }

    reached.sort();
    let modules = reached.iter().map(|path| unit.emit(path)).collect();
    Translation { modules }
}

/// Facts about the other files in a translation, for the one being emitted.
pub(crate) trait Oracle {
    /// The kinds of the export `name` of the package module `module`, or
    /// [`None`] when this translation cannot see it.
    fn export_kinds(&self, module: &str, name: &str) -> Option<Kinds>;

    /// Whether `module` assigns `export =`.
    fn assigns_exports(&self, module: &str) -> bool;

    /// Every name `module` exports directly, with its kinds when they are
    /// known, sorted by name.
    fn exports_of(&self, module: &str) -> Vec<(CompactString, Option<Kinds>)>;
}

/// A declaration file the walk reached, as the rest of the translation sees
/// it.
///
/// The summary is boxed because a map holds one of these per file and the
/// two variants are far apart in size — a summary is a few hundred bytes of
/// tables, a parse failure a message and a line — so boxing keeps the map's
/// slots small at the price of one allocation per file that parsed, which is
/// nothing beside the parse itself.
enum File {
    Summarized(Box<Summary>),
    Unparsed { message: CompactString, line: u32 },
}

struct Unit<'b> {
    arena: &'b Bump,
    sources: FxHashMap<CompactString, Option<&'b str>>,
    files: FxHashMap<CompactString, File>,
}

impl<'b> Unit<'b> {
    fn load(
        &mut self,
        path: &str,
        read: &mut dyn FnMut(&str) -> Option<String>,
    ) -> Option<&'b str> {
        if let Some(known) = self.sources.get(path) {
            return *known;
        }
        let loaded = read(path).map(|text| &*self.arena.alloc_str(&text));
        self.sources.insert(path.to_compact_string(), loaded);
        loaded
    }

    fn summarize(&mut self, path: &str, read: &mut dyn FnMut(&str) -> Option<String>) {
        let Some(source) = self.load(path, read) else {
            return;
        };
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
        if let Some((message, line)) = parse_failure(&parsed, source) {
            self.files
                .insert(path.to_compact_string(), File::Unparsed { message, line });
            return;
        }
        let scan = summary::scan(&parsed.program);
        let mut targets = FxHashMap::default();
        for specifier in &scan.specifiers {
            let key = specifier.to_compact_string();
            if targets.contains_key(&key) {
                continue;
            }
            let target = self.resolve(path, specifier, read);
            targets.insert(key, target);
        }
        let summary = summary::summarize(&parsed.program, scan, targets);
        self.files.insert(
            path.to_compact_string(),
            File::Summarized(Box::new(summary)),
        );
    }

    fn resolve(
        &mut self,
        importer: &str,
        specifier: &str,
        read: &mut dyn FnMut(&str) -> Option<String>,
    ) -> Target {
        if !is_relative(specifier) {
            return Target::Package(specifier.to_compact_string());
        }
        for candidate in candidates(importer, specifier) {
            if self.load(&candidate, read).is_some() {
                return Target::Module(candidate.into());
            }
        }
        Target::Missing(specifier.to_compact_string())
    }

    fn emit(&self, path: &str) -> Module {
        let source = self
            .sources
            .get(path)
            .copied()
            .flatten()
            .unwrap_or_default();
        match self.files.get(path) {
            Some(File::Summarized(summary)) => {
                let allocator = Allocator::default();
                let parsed = Parser::new(&allocator, source, SourceType::d_ts()).parse();
                let (flow, holes) = emit::module(self, path, &parsed.program, summary);
                Module {
                    path: path.to_compact_string(),
                    flow: Some(flow),
                    holes,
                }
            }
            Some(File::Unparsed { message, line }) => Module {
                path: path.to_compact_string(),
                flow: None,
                holes: vec![Hole {
                    declaration: CompactString::const_new("(module)"),
                    construct: Construct::ParseError,
                    reason: uf_infra::cstr!(
                        "oxc could not parse this file as TypeScript ({message}), so nothing it \
                         declares is typed"
                    )
                    .into_string()
                    .into(),
                    line: *line,
                }],
            },
            None => Module {
                path: path.to_compact_string(),
                flow: None,
                holes: Vec::new(),
            },
        }
    }

    fn summary(&self, module: &str) -> Option<&Summary> {
        match self.files.get(module)? {
            File::Summarized(summary) => Some(summary),
            File::Unparsed { .. } => None,
        }
    }

    fn kinds_in(
        &self,
        module: &str,
        name: &str,
        seen: &mut Vec<(CompactString, CompactString)>,
    ) -> Option<Kinds> {
        if seen
            .iter()
            .any(|(seen_module, seen_name)| seen_module == module && seen_name == name)
        {
            return None;
        }
        seen.push((module.to_compact_string(), name.to_compact_string()));
        let summary = self.summary(module)?;
        if let Some(export) = summary.exports.get(name) {
            return match export {
                Export::Local(local) => self.local_kinds_in(summary, local, seen),
                Export::From {
                    type_only: true, ..
                } => Some(Kinds::TYPE),
                Export::From {
                    target, imported, ..
                } => self.target_kinds(target, imported, seen),
                Export::Anonymous(kinds) => Some(*kinds),
            };
        }
        if name == "default" {
            return None;
        }
        summary.stars.iter().find_map(|star| match star {
            Target::Module(star) => self.kinds_in(star, name, seen),
            Target::Package(_) | Target::Missing(_) => None,
        })
    }

    fn local_kinds_in(
        &self,
        summary: &Summary,
        local: &str,
        seen: &mut Vec<(CompactString, CompactString)>,
    ) -> Option<Kinds> {
        if let Some(declared) = summary.locals.get(local) {
            return Some(declared.kinds());
        }
        let import = summary.imports.get(local)?;
        if import.type_only {
            return Some(Kinds::TYPE);
        }
        self.target_kinds(&import.target, &import.imported, seen)
    }

    fn target_kinds(
        &self,
        target: &Target,
        imported: &Imported,
        seen: &mut Vec<(CompactString, CompactString)>,
    ) -> Option<Kinds> {
        let Target::Module(module) = target else {
            return None;
        };
        match imported {
            Imported::Namespace => Some(Kinds::BOTH),
            Imported::Named(name) => self.kinds_in(module, name, seen),
        }
    }
}

impl Oracle for Unit<'_> {
    fn export_kinds(&self, module: &str, name: &str) -> Option<Kinds> {
        self.kinds_in(module, name, &mut Vec::new())
    }

    fn assigns_exports(&self, module: &str) -> bool {
        self.summary(module)
            .is_some_and(|summary| summary.equals.is_some())
    }

    fn exports_of(&self, module: &str) -> Vec<(CompactString, Option<Kinds>)> {
        let Some(summary) = self.summary(module) else {
            return Vec::new();
        };
        let mut names: Vec<(CompactString, Option<Kinds>)> = summary
            .exports
            .keys()
            .filter(|name| name.as_str() != "default")
            .map(|name| (name.clone(), self.export_kinds(module, name)))
            .collect();
        names.sort_by(|left, right| left.0.cmp(&right.0));
        names
    }
}

/// The first syntax error in a parse, as a message and a one-based line.
fn parse_failure(
    parsed: &oxc_parser::ParserReturn<'_>,
    source: &str,
) -> Option<(CompactString, u32)> {
    if !parsed.fatal_error && parsed.diagnostics.is_empty() {
        return None;
    }
    let first = parsed.diagnostics.iter().next();
    let message = first.map_or_else(
        || CompactString::const_new("the parser gave up"),
        |diagnostic| diagnostic.to_string().to_compact_string(),
    );
    let offset = first
        .and_then(|diagnostic| diagnostic.labels.first())
        .map_or(0, |label| usize::try_from(label.offset()).unwrap_or(0));
    let line = source
        .get(..offset.min(source.len()))
        .map_or(0, |before| before.matches('\n').count());
    Some((message, u32::try_from(line + 1).unwrap_or(u32::MAX)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> impl FnMut(&str) -> Option<String> + use<> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(path, source)| ((*path).to_owned(), (*source).to_owned()))
            .collect();
        move |path: &str| {
            owned
                .iter()
                .find(|(candidate, _)| candidate == path)
                .map(|(_, source)| source.clone())
        }
    }

    #[test]
    fn the_walk_follows_relative_imports_and_stops_at_packages() {
        let mut read = files(&[
            (
                "index.d.ts",
                "export * from \"./a.js\";\nimport type { X } from \"other\";\n",
            ),
            ("a.d.ts", "export interface A {}\n"),
            ("unreached.d.ts", "export interface U {}\n"),
        ]);
        let translation = translate(&["index.d.ts"], &mut read);
        let paths: Vec<&str> = translation
            .modules
            .iter()
            .map(|module| module.path.as_str())
            .collect();
        assert_eq!(paths, ["a.d.ts", "index.d.ts"]);
    }

    #[test]
    fn a_re_exported_name_is_seen_through_its_chain() {
        let mut read = files(&[
            ("index.d.ts", "export { A as B } from \"./mid.js\";\n"),
            ("mid.d.ts", "export * from \"./leaf.js\";\n"),
            (
                "leaf.d.ts",
                "export interface A {}\nexport declare const v: number;\n",
            ),
        ]);
        let arena = Bump::new();
        let mut unit = Unit {
            arena: &arena,
            sources: FxHashMap::default(),
            files: FxHashMap::default(),
        };
        for path in ["index.d.ts", "mid.d.ts", "leaf.d.ts"] {
            unit.summarize(path, &mut read);
        }
        assert_eq!(unit.export_kinds("index.d.ts", "B"), Some(Kinds::TYPE));
        assert_eq!(unit.export_kinds("mid.d.ts", "v"), Some(Kinds::VALUE));
        assert_eq!(unit.export_kinds("mid.d.ts", "missing"), None);
    }

    #[test]
    fn a_cycle_of_re_exports_terminates() {
        let mut read = files(&[
            ("a.d.ts", "export * from \"./b.js\";\n"),
            ("b.d.ts", "export * from \"./a.js\";\n"),
        ]);
        let translation = translate(&["a.d.ts"], &mut read);
        assert_eq!(translation.modules.len(), 2);
    }

    #[test]
    fn a_file_that_does_not_parse_is_no_module_and_a_hole() {
        let mut read = files(&[("index.d.ts", "export interface {\n")]);
        let translation = translate(&["index.d.ts"], &mut read);
        let module = &translation.modules[0];
        assert!(module.flow.is_none());
        assert_eq!(module.holes[0].construct, Construct::ParseError);
    }
}
