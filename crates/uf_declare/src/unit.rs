//! One translation: the Flow modules a set of entries reaches, and the
//! TypeScript declarations each of them becomes.
//!
//! The walk starts at the library's entries and follows every relative
//! import and re-export to a module in the same project. It stops at a bare
//! specifier: another package ships — or does not ship — its own declarations,
//! and inlining somebody else's types into this library's would publish a
//! copy that goes stale the first time they release.
//!
//! Nothing here touches a filesystem. The caller hands over a function that
//! reads a project path, which is what lets `uf build` translate out of
//! whatever it already has in memory and what lets a test translate out of a
//! table of strings.
//!
//! # Why the parser runs on a thread of its own
//!
//! `uf_flow::parse` documents a stack of [`uf_flow::PARSE_STACK_BYTES`],
//! because the port is a recursive-descent parser with large frames and the
//! walk below recurses over its tree again. A caller on an ordinary 8 MiB
//! main thread — or a 2 MiB test thread — overflows on a source the parser
//! itself accepts, and a stack overflow cannot be caught. So [`translate`]
//! puts the whole walk on a thread with the room for it rather than leaving
//! the obligation to every caller, which is the same choice `uf_doc` and
//! `uf_i18n` made.

use std::collections::VecDeque;

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use uf_flow::ast::statement;
use uf_infra::{FxHashMap, FxHashSet};

use crate::emit;
use crate::gap::{Construct, Gap};
use crate::resolve::{candidates, declaration_path, is_relative, normalize};

/// What a translation produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    /// One module per Flow file the entries reach, sorted by path.
    pub modules: Vec<Module>,
}

impl Translation {
    /// Every gap in every module, in module order.
    pub fn gaps(&self) -> impl Iterator<Item = (&Module, &Gap)> {
        self.modules
            .iter()
            .flat_map(|module| module.gaps.iter().map(move |gap| (module, gap)))
    }

    /// Whether any module failed to produce declarations at all.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.modules.iter().all(|module| module.text.is_some())
    }
}

/// One Flow module, translated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Module {
    /// The Flow module's project path, as the reader passed it.
    pub path: CompactString,
    /// Where its declarations belong, relative to the output directory:
    /// `index.js` becomes `index.d.ts`.
    pub declaration: CompactString,
    /// The TypeScript declaration file, or [`None`] when the module did not
    /// parse.
    ///
    /// [`None`] rather than an empty file on purpose. An empty declaration
    /// file declares a module that exports nothing, so every import from it
    /// would be an error in the *consumer's* code — a confident, wrong
    /// answer. No file at all leaves the import untyped, which is what the
    /// consumer had before uf wrote anything, and the [`Construct::ParseError`]
    /// gap says why.
    pub text: Option<String>,
    /// What the translation could not carry over, in line order.
    pub gaps: Vec<Gap>,
}

/// Translate the Flow modules `entries` reach.
///
/// Paths are relative to the project root and `/`-separated, and `read`
/// answers one: the module's text, or [`None`] when there is no such file. It
/// is called at most once per path. An entry that cannot be read produces no
/// module.
pub fn translate(
    entries: &[&str],
    read: &mut (dyn FnMut(&str) -> Option<String> + Send),
) -> Translation {
    // See the module header: the parser and the walk over its tree both
    // recurse, and the stack they need is not the stack a caller is on.
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("uf-declare".into())
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, || walk(entries, read));
        match worker {
            // A process that cannot start a thread is failing anyway, and a
            // translation is not the place to report that: no modules is the
            // same answer a project with no entries gets, and the build says
            // it wrote no declarations.
            Err(_) => Translation {
                modules: Vec::new(),
            },
            Ok(worker) => worker.join().unwrap_or(Translation {
                modules: Vec::new(),
            }),
        }
    })
}

fn walk(entries: &[&str], read: &mut dyn FnMut(&str) -> Option<String>) -> Translation {
    let mut sources: FxHashMap<CompactString, Option<String>> = FxHashMap::default();
    let mut queue: VecDeque<CompactString> = entries
        .iter()
        .filter_map(|entry| normalize(entry))
        .map(CompactString::from)
        .collect();
    let mut seen: FxHashSet<CompactString> = FxHashSet::default();
    let mut modules: Vec<Module> = Vec::new();

    while let Some(path) = queue.pop_front() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let Some(source) = load(&mut sources, &path, read) else {
            continue;
        };
        let source = source.to_owned();
        let declaration = declaration_path(&path).to_compact_string();

        let parsed = match uf_flow::parse(&source) {
            Ok(parsed) => parsed,
            Err(failure) => {
                modules.push(Module {
                    path: path.clone(),
                    declaration,
                    text: None,
                    gaps: vec![Gap {
                        declaration: CompactString::const_new("(module)"),
                        construct: Construct::ParseError,
                        reason: uf_infra::into_string(uf_infra::cstr!(
                            "uf could not parse this module ({failure}), so nothing it exports is \
                             declared"
                        ))
                        .into(),
                        line: 1,
                    }],
                });
                continue;
            }
        };
        if !parsed.is_ok() {
            let first = parsed.diagnostics.first();
            let line = first.and_then(|diagnostic| diagnostic.line).unwrap_or(1);
            let message = first.map_or_else(
                || String::from("the parser gave up"),
                |diagnostic| diagnostic.message.clone(),
            );
            modules.push(Module {
                path: path.clone(),
                declaration,
                text: None,
                gaps: vec![Gap {
                    declaration: CompactString::const_new("(module)"),
                    construct: Construct::ParseError,
                    reason: uf_infra::into_string(uf_infra::cstr!(
                        "this module has a syntax error ({message}), so nothing it exports is \
                         declared"
                    ))
                    .into(),
                    line,
                }],
            });
            continue;
        }

        let (text, mut gaps) = emit::module(&path, source.len(), &parsed.program);

        // Follow what this module imports, and say so when a specifier names
        // nothing. A relative import that resolves to no file is a module
        // whose declarations reference a file this build will not write, so
        // the consumer's compiler would fail on it — reporting it here is
        // what turns that into something the author can see.
        let mut next: Vec<CompactString> = Vec::new();
        for (specifier, line) in specifiers(&parsed.program) {
            if !is_relative(&specifier) {
                continue;
            }
            let resolved = candidates(&path, &specifier)
                .into_iter()
                .find(|candidate| load(&mut sources, candidate, read).is_some());
            match resolved {
                Some(target) => next.push(target.into()),
                None => gaps.push(Gap {
                    declaration: CompactString::const_new("(module)"),
                    construct: Construct::MissingFile,
                    reason: uf_infra::into_string(uf_infra::cstr!(
                        "`{specifier}` names no module in this project, so the declarations import \
                         a file that will not be written"
                    ))
                    .into(),
                    line,
                }),
            }
        }
        // The probe order above is the resolver's; the queue's must not
        // depend on a hash map's iteration.
        next.sort();
        next.dedup();
        queue.extend(next);

        modules.push(Module {
            path: path.clone(),
            declaration,
            text: Some(text),
            gaps,
        });
    }

    modules.sort_by(|left, right| left.path.cmp(&right.path));
    Translation { modules }
}

/// The source at `path`, read at most once.
fn load<'a>(
    sources: &'a mut FxHashMap<CompactString, Option<String>>,
    path: &str,
    read: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<&'a str> {
    let key = path.to_compact_string();
    sources.entry(key).or_insert_with(|| read(path)).as_deref()
}

/// Every module specifier the program names at the top level, with the line
/// it was written on.
///
/// Top level only, and deliberately: a declaration file is built from what a
/// module exports, and an `import()` inside a function body contributes
/// nothing to that.
fn specifiers(program: &uf_flow::ast::Program<uf_flow::Loc, uf_flow::Loc>) -> Vec<(String, u32)> {
    let mut found = Vec::new();
    for item in program.statements.iter() {
        let line = u32::try_from(item.loc().start.line).unwrap_or(1).max(1);
        match &**item {
            statement::StatementInner::ImportDeclaration { inner, .. } => {
                found.push((inner.source.1.value.to_string(), line));
            }
            statement::StatementInner::ExportNamedDeclaration { inner, .. } => {
                if let Some((_, source)) = &inner.source {
                    found.push((source.value.to_string(), line));
                }
            }
            statement::StatementInner::DeclareExportDeclaration { inner, .. } => {
                if let Some((_, source)) = &inner.source {
                    found.push((source.value.to_string(), line));
                }
            }
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> impl FnMut(&str) -> Option<String> + Send + use<> {
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
                "index.js",
                "// @flow\nexport type { A } from \"./a.js\";\nimport type { X } from \"other\";\n",
            ),
            ("a.js", "// @flow\nexport type A = string;\n"),
            ("unreached.js", "// @flow\nexport type U = number;\n"),
        ]);
        let translation = translate(&["index.js"], &mut read);
        let paths: Vec<&str> = translation
            .modules
            .iter()
            .map(|module| module.path.as_str())
            .collect();
        assert_eq!(paths, ["a.js", "index.js"]);
    }

    #[test]
    fn a_cycle_of_re_exports_terminates() {
        let mut read = files(&[
            ("a.js", "// @flow\nexport type * from \"./b.js\";\n"),
            ("b.js", "// @flow\nexport type * from \"./a.js\";\n"),
        ]);
        let translation = translate(&["a.js"], &mut read);
        assert_eq!(translation.modules.len(), 2);
    }

    #[test]
    fn a_module_that_does_not_parse_is_no_declaration_and_a_gap() {
        let mut read = files(&[("index.js", "// @flow\nexport type = ;\n")]);
        let translation = translate(&["index.js"], &mut read);
        let module = &translation.modules[0];
        assert!(module.text.is_none(), "{:?}", module.text);
        assert_eq!(module.gaps[0].construct, Construct::ParseError);
        assert!(!translation.is_complete());
    }

    #[test]
    fn an_import_of_a_module_that_is_not_there_is_reported() {
        let mut read = files(&[(
            "index.js",
            "// @flow\nimport type { A } from \"./missing.js\";\nexport type B = A;\n",
        )]);
        let translation = translate(&["index.js"], &mut read);
        let gaps: Vec<Construct> = translation
            .modules
            .iter()
            .flat_map(|module| module.gaps.iter().map(|gap| gap.construct))
            .collect();
        assert!(gaps.contains(&Construct::MissingFile), "{gaps:?}");
    }

    #[test]
    fn a_declaration_is_named_after_its_module() {
        let mut read = files(&[("index.js", "// @flow\nexport type A = string;\n")]);
        let translation = translate(&["index.js"], &mut read);
        assert_eq!(translation.modules[0].declaration, "index.d.ts");
    }
}
