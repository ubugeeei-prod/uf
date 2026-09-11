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
use crate::cache::{CachedRequire, Digest, Fields, hex};

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
    /// Per module, what each of its `requires` resolved to, in the same order.
    resolutions: Vec<Vec<Resolution>>,
    /// Per module, a digest of everything about *it* a dependent must notice.
    local: Vec<Digest>,
}

impl<'a> Graph<'a> {
    /// Resolve every import in the batch.
    pub(super) fn new(
        paths: Vec<&'a str>,
        facts: &'a [ModuleFacts],
        modules: &ProjectModules,
    ) -> Self {
        profile_span!("check::graph");
        let resolutions: Vec<Vec<Resolution>> = paths
            .iter()
            .zip(facts)
            .map(|(importer, importer_facts)| {
                importer_facts
                    .requires
                    .iter()
                    .map(|require| resolve(modules, facts, importer, require))
                    .collect()
            })
            .collect();
        let local = paths
            .iter()
            .zip(facts)
            .zip(&resolutions)
            .map(|((path, facts), resolutions)| local_digest(path, facts, resolutions))
            .collect();
        Self {
            paths,
            facts,
            resolutions,
            local,
        }
    }

    /// The digest the `index`th file's diagnostics are only valid under.
    ///
    /// Built from the closure in path order rather than in discovery order, so
    /// that two runs that reach the same modules by different routes — which
    /// they do, because discovery order follows whichever file was checked
    /// first — agree on the digest.
    pub(super) fn dependency_digest(&self, index: usize) -> String {
        let mut reached = vec![index];
        let mut seen = vec![false; self.facts.len()];
        seen[index] = true;
        let mut frontier = vec![index];
        while let Some(module) = frontier.pop() {
            for resolution in &self.resolutions[module] {
                if let Resolution::Module(next) = *resolution
                    && !seen[next]
                {
                    seen[next] = true;
                    reached.push(next);
                    frontier.push(next);
                }
            }
        }
        // By path, then by position: `ModuleIndex` lets two sources share a
        // path and gives the first one every import, so the second can still
        // be reached as itself — and two files that sort equal must not be
        // ordered by whichever the sort happened to move.
        reached.sort_unstable_by_key(|module| (self.paths[*module], *module));

        let mut digest = Fields::new("uf-check-dependencies-v1");
        for module in reached {
            digest.push(self.paths[module]);
            digest.push_digest(&self.local[module]);
        }
        hex(&digest.finish())
    }

    /// The specifiers the `index`th file imports that resolved to nothing
    /// typed.
    ///
    /// Only a file that has a signature contributes: one that did not parse or
    /// said `@noflow` is never checked, so its imports are never resolved, and
    /// naming a hole nobody looked through would be a report of something that
    /// did not happen.
    pub(super) fn untyped(&self, index: usize) -> Vec<CompactString> {
        if self.facts[index].signature.is_none() {
            return Vec::new();
        }
        self.facts[index]
            .requires
            .iter()
            .zip(&self.resolutions[index])
            .filter(|(_, resolution)| {
                matches!(
                    **resolution,
                    Resolution::HostConditional | Resolution::Untyped
                )
            })
            .map(|(require, _)| require.specifier.clone())
            .collect()
    }

    /// The specifiers that stayed untyped because their `exports` map only
    /// resolved for a concrete host.
    pub(super) fn host_conditional(&self, index: usize) -> Vec<CompactString> {
        if self.facts[index].signature.is_none() {
            return Vec::new();
        }
        self.facts[index]
            .requires
            .iter()
            .zip(&self.resolutions[index])
            .filter(|(_, resolution)| **resolution == Resolution::HostConditional)
            .map(|(require, _)| require.specifier.clone())
            .collect()
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
