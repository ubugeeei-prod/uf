//! What each file in a batch reaches, and the digest that says so.
//!
//! [`super::project::ProjectModules`] resolves an import *while* it types a
//! file, lazily, and only as far as the merge actually forces. That is the
//! right shape for checking and the wrong shape for a cache key: a key has to
//! describe everything an answer *could* have depended on before the answer is
//! computed, and it has to describe it the same way whether this run checked
//! the file or read it from disk. So this module resolves the whole batch
//! eagerly, from the same rules, and produces two things from it:
//!
//! * a per-file **dependency digest** over the signature of every module the
//!   file reaches and how every one of those modules' specifiers resolved; and
//! * the specifiers that resolved to nothing typed, which is
//!   [`crate::CheckReport::untyped_modules`].
//!
//! # Why the digest is over the closure and not the direct imports
//!
//! Merging a dependency's exports gives the importing file a *shallow* module
//! type whose members are signature tvars, and forcing one of those reaches
//! into the dependency's own dependencies (see [`super::project`]'s header on
//! cycles). An error in `app.js` can therefore be a consequence of a type two
//! modules away, and can point at a location inside it. Anything less than the
//! transitive closure would leave a file believed while something it really
//! does depend on has moved underneath it.
//!
//! # Why the resolutions are in it, not only the signatures
//!
//! What a specifier resolves to is a property of the *batch*, not of any file:
//! adding `src/generated/table.js` to a project turns an import that was typed
//! `any` into a typed module, and every diagnostic downstream of it changes,
//! with no file's own text having changed at all. Recording, for each module in
//! the closure, what each of its specifiers resolved to is what notices that —
//! and, in the same stroke, what notices a `package.json` that starts or stops
//! publishing a name.

use compact_str::CompactString;
use uf_profiler::profile_span;

use super::project::ProjectModules;
use super::resolve;
use crate::cache::{CachedRequire, Digest, Fields, hex_into};

/// What a check needs to know about one file before it checks anything.
///
/// Either read from that file's cache record or worked out by packing its
/// signature; the two must describe the same file the same way, which is why
/// this is one type and not two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModuleFacts {
    /// A digest of the file's packed signature, or [`None`] when it has none:
    /// it did not parse, or it said `@noflow`.
    pub(super) signature: Option<Digest>,
    /// Every module specifier the file imports, sorted and de-duplicated.
    pub(super) requires: Vec<CachedRequire>,
    /// Whether the file opted out of inference with `@noflow`.
    pub(super) skipped: bool,
}

/// What one specifier resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    /// A file in this batch, which contributes its signature.
    Module(usize),
    /// A module Flow's own library definitions declare.
    Declared,
    /// A package export that would resolve after choosing a concrete host.
    HostConditional,
    /// Nothing typed: the import is `any`, and the specifier is reported.
    Untyped,
}

impl Resolution {
    /// The mark this resolution leaves in a digest.
    ///
    /// One character each, and all three distinct: a specifier that stops
    /// resolving to a batch module and starts resolving to a `declare module`
    /// is a different check even though both are "typed".
    fn mark(self) -> &'static str {
        match self {
            Self::Module(_) => "@",
            Self::Declared => "~",
            Self::HostConditional => "!",
            Self::Untyped => "?",
        }
    }
}

/// The batch's imports, resolved.
pub(super) struct Graph<'a> {
    paths: Vec<&'a str>,
    facts: &'a [ModuleFacts],
    /// Per module, the slice of [`Self::resolutions`] that belongs to it.
    resolution_ranges: Vec<std::ops::Range<usize>>,
    /// What each module's `requires` resolved to, in module order.
    resolutions: Vec<Resolution>,
    /// Per module, a digest of everything about *it* a dependent must notice.
    local: Vec<Digest>,
}

/// Reused storage for one dependency walk.
///
/// A batch asks for one dependency digest per source. Allocating the reached,
/// seen and frontier buffers inside each query made a warm cache hit pay the
/// same tiny setup cost once per file, even though the graph itself is fixed.
pub(super) struct DependencyScratch {
    reached: Vec<usize>,
    seen: Vec<bool>,
    frontier: Vec<usize>,
    digest: String,
}

impl DependencyScratch {
    pub(super) fn new(modules: usize) -> Self {
        Self {
            reached: Vec::new(),
            seen: vec![false; modules],
            frontier: Vec::new(),
            digest: String::with_capacity(std::mem::size_of::<Digest>() * 2),
        }
    }

    fn reset(&mut self) {
        self.reached.clear();
        self.seen.fill(false);
        self.frontier.clear();
    }
}

impl<'a> Graph<'a> {
    /// Resolve every import in the batch.
    pub(super) fn new(
        paths: Vec<&'a str>,
        facts: &'a [ModuleFacts],
        modules: &ProjectModules,
    ) -> Self {
        profile_span!("check::graph");
        let total_requires = facts.iter().map(|facts| facts.requires.len()).sum();
        let mut resolutions = Vec::with_capacity(total_requires);
        let mut resolution_ranges = Vec::with_capacity(facts.len());
        let mut local = Vec::with_capacity(facts.len());
        for (path, importer_facts) in paths.iter().zip(facts) {
            let start = resolutions.len();
            resolutions.extend(
                importer_facts
                    .requires
                    .iter()
                    .map(|require| resolve(modules, facts, path, require)),
            );
            let end = resolutions.len();
            resolution_ranges.push(start..end);
            local.push(local_digest(path, importer_facts, &resolutions[start..end]));
        }
        Self {
            paths,
            facts,
            resolution_ranges,
            resolutions,
            local,
        }
    }

    fn resolutions(&self, index: usize) -> &[Resolution] {
        &self.resolutions[self.resolution_ranges[index].clone()]
    }

    /// The digest the `index`th file's diagnostics are only valid under.
    ///
    /// Built from the closure in path order rather than in discovery order, so
    /// that two runs that reach the same modules by different routes — which
    /// they do, because discovery order follows whichever file was checked
    /// first — agree on the digest.
    pub(super) fn scratch(&self) -> DependencyScratch {
        DependencyScratch::new(self.facts.len())
    }

    pub(super) fn dependency_digest<'scratch>(
        &self,
        index: usize,
        scratch: &'scratch mut DependencyScratch,
    ) -> &'scratch str {
        scratch.reset();
        scratch.reached.push(index);
        scratch.seen[index] = true;
        scratch.frontier.push(index);
        while let Some(module) = scratch.frontier.pop() {
            for resolution in self.resolutions(module) {
                if let Resolution::Module(next) = *resolution
                    && !scratch.seen[next]
                {
                    scratch.seen[next] = true;
                    scratch.reached.push(next);
                    scratch.frontier.push(next);
                }
            }
        }
        // By path, then by position: `ModuleIndex` lets two sources share a
        // path and gives the first one every import, so the second can still
        // be reached as itself — and two files that sort equal must not be
        // ordered by whichever the sort happened to move.
        scratch
            .reached
            .sort_unstable_by_key(|module| (self.paths[*module], *module));

        let mut digest = Fields::new("uf-check-dependencies-v1");
        for &module in &scratch.reached {
            digest.push(self.paths[module]);
            digest.push_digest(&self.local[module]);
        }
        hex_into(&digest.finish(), &mut scratch.digest);
        &scratch.digest
    }

    /// The specifiers the `index`th file imports that resolved to nothing
    /// typed, and whether each one was a host-conditional package export.
    ///
    /// Only a file that has a signature contributes: one that did not parse or
    /// said `@noflow` is never checked, so its imports are never resolved, and
    /// naming a hole nobody looked through would be a report of something that
    /// did not happen.
    pub(super) fn untyped_specifiers(
        &self,
        index: usize,
    ) -> impl Iterator<Item = (&CompactString, bool)> {
        let contributes = self.facts[index].signature.is_some();
        self.facts[index]
            .requires
            .iter()
            .zip(self.resolutions(index))
            .filter_map(move |(require, resolution)| {
                contributes.then_some(())?;
                match resolution {
                    Resolution::HostConditional => Some((&require.specifier, true)),
                    Resolution::Untyped => Some((&require.specifier, false)),
                    Resolution::Module(_) | Resolution::Declared => None,
                }
            })
    }
}

/// What one specifier resolves to, by [`ProjectModules::resolve`]'s own order.
///
/// A bare `declare module` outranks the package manifests in the batch, while
/// relative modules still resolve to the batch before asset declarations. The
/// order mirrors [`ProjectModules::resolve`] rather than reimplementing a
/// separate answer to what one specifier means.
fn resolve(
    modules: &ProjectModules,
    facts: &[ModuleFacts],
    importer: &str,
    require: &CachedRequire,
) -> Resolution {
    if !resolve::is_relative(&require.specifier) && require.declared {
        return Resolution::Declared;
    }
    match modules.locate(importer, &require.specifier) {
        // Whether the target has a signature is read from the batch's facts
        // rather than asked of `ProjectModules`, which would pack one to find
        // out — and packing every file is exactly the work a run answered from
        // the cache exists not to do.
        Some(index) if facts[index].signature.is_some() => Resolution::Module(index),
        _ if require.declared => Resolution::Declared,
        _ if modules.host_conditional_exports(importer, &require.specifier) => {
            Resolution::HostConditional
        }
        _ => Resolution::Untyped,
    }
}

/// Everything about one module that a file reaching it must notice.
fn local_digest(path: &str, facts: &ModuleFacts, resolutions: &[Resolution]) -> Digest {
    let mut digest = Fields::new("uf-check-module-v1");
    digest.push(path);
    match &facts.signature {
        // The packed signature *and* the table of locations it was packed
        // with, which `pack` folds into one digest: a dependent's diagnostics
        // can point into this file, so a declaration that moved down a line
        // changes what the dependent renders even when its exported types are
        // untouched.
        Some(signature) => digest.push_digest(signature),
        None => digest.push("-"),
    };
    for (require, resolution) in facts.requires.iter().zip(resolutions) {
        digest.push(&require.specifier);
        digest.push(resolution.mark());
    }
    digest.finish()
}

#[cfg(test)]
mod tests {
    use compact_str::ToCompactString;

    use super::*;
    use crate::{CheckLimits, Source};

    #[test]
    fn graph_keeps_resolutions_in_one_flat_buffer() {
        const MODULES: usize = 32;

        let limits = CheckLimits::default().without_timeout();
        let paths: Vec<String> = (0..MODULES)
            .map(|index| uf_infra::into_string(uf_infra::cstr!("module{index}.js")))
            .collect();
        let texts: Vec<String> = (0..MODULES).map(|_| "// @flow\n".to_owned()).collect();
        let sources: Vec<Source<'_>> = paths
            .iter()
            .zip(&texts)
            .map(|(path, source)| Source::new(path, source))
            .collect();
        let facts: Vec<ModuleFacts> = (0..MODULES)
            .map(|index| {
                let requires = if index + 1 == MODULES {
                    Vec::new()
                } else {
                    vec![CachedRequire {
                        specifier: uf_infra::into_string(uf_infra::cstr!(
                            "./module{}.js",
                            index + 1
                        ))
                        .to_compact_string(),
                        declared: false,
                    }]
                };
                ModuleFacts {
                    signature: Some([1; 32]),
                    requires,
                    skipped: false,
                }
            })
            .collect();
        let modules = ProjectModules::new(
            &sources,
            super::super::options::options(&limits),
            None,
            &limits,
        );

        let graph = Graph::new(paths.iter().map(String::as_str).collect(), &facts, &modules);

        assert_eq!(graph.resolutions.len(), MODULES - 1);
        assert_eq!(graph.resolutions.capacity(), MODULES - 1);
        assert_eq!(graph.resolution_ranges.len(), MODULES);
        for index in 0..MODULES {
            let resolutions = graph.resolutions(index);
            if index + 1 == MODULES {
                assert!(resolutions.is_empty());
            } else {
                assert_eq!(resolutions, &[Resolution::Module(index + 1)]);
            }
        }
        modules.release();
    }

    #[test]
    fn graph_reports_untyped_specifiers_without_intermediate_lists() {
        let limits = CheckLimits::default().without_timeout();
        let paths = ["app.js".to_owned()];
        let texts = ["// @flow\nimport value from \"missing\";\n".to_owned()];
        let sources = [Source::new(paths[0].as_str(), texts[0].as_str())];
        let facts = [ModuleFacts {
            signature: Some([1; 32]),
            requires: vec![
                CachedRequire {
                    specifier: "react".to_compact_string(),
                    declared: true,
                },
                CachedRequire {
                    specifier: "missing".to_compact_string(),
                    declared: false,
                },
            ],
            skipped: false,
        }];
        let modules = ProjectModules::new(
            &sources,
            super::super::options::options(&limits),
            None,
            &limits,
        );

        let graph = Graph::new(vec![paths[0].as_str()], &facts, &modules);

        let untyped: Vec<_> = graph
            .untyped_specifiers(0)
            .map(|(specifier, host_conditional)| (specifier.as_str(), host_conditional))
            .collect();
        assert_eq!(untyped, [("missing", false)]);

        modules.release();
    }
}
