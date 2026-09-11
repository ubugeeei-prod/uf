//! What goes into a type check, and what comes out of one.

use std::time::Duration;

use compact_str::CompactString;

use crate::diagnostic::{Severity, TypeDiagnostic};

/// One file to check.
///
/// `path` is what diagnostics are reported under, so it should be the path the
/// user recognises — project-relative, in `uf`'s case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source<'a> {
    /// The path diagnostics are reported under.
    pub path: &'a str,
    /// The file's text.
    pub source: &'a str,
}

impl<'a> Source<'a> {
    /// A source with a path and text.
    pub const fn new(path: &'a str, source: &'a str) -> Self {
        Self { path, source }
    }
}

/// The batch a set of files needs in order to be checked against what they
/// import.
///
/// [`crate::check_sources`] resolves an import to a file in the batch or to
/// nothing typed, so a caller that hands it one file gets that file checked
/// against nothing: `import type { Control } from "@uniflowed/form"` becomes
/// an `any`-typed value and every annotation written against it goes unread.
/// [`crate::module_closure`] is how a caller finds out what else to hand over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleClosure<'a> {
    /// The seeds and everything they reach, in the order they were given.
    ///
    /// This is the batch: pass it to [`crate::check_sources`] and every
    /// specifier that resolved during the walk resolves again during the check,
    /// because both use the same rules. A package's `package.json` is in here
    /// beside the file it publishes, since that is where the name comes from.
    pub sources: Vec<Source<'a>>,
    /// Specifiers no source answered, each beside the file that imported it,
    /// sorted and de-duplicated.
    ///
    /// Not all of these are holes. `react` is Flow's own `declare module` and
    /// `node:fs` is a builtin; the walk has no builtin environment to ask, and
    /// giving it one would make assembling a batch depend on merging the
    /// library definitions. This list is for the caller that can go and find
    /// more sources — reading `node_modules` for a bare specifier, say — and
    /// then ask again.
    ///
    /// The importer is here and not deduplicated away because *where* a
    /// specifier was written decides what it means: Node resolves a bare
    /// specifier by climbing from the importing file, so `bar` imported from
    /// inside `node_modules/foo` is `node_modules/foo/node_modules/bar` when
    /// that exists and the hoisted `node_modules/bar` only when it does not.
    /// A caller handed a bare set of names could only ever find one copy.
    /// ubugeeei-prod/uf#486.
    pub unresolved: Vec<UnresolvedImport>,
}

/// A specifier nothing in the batch answered, and the file that wrote it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnresolvedImport {
    /// The specifier, exactly as the import wrote it.
    pub specifier: CompactString,
    /// The batch path of the file that imported it.
    ///
    /// What a caller resolving the specifier itself must climb from. It is one
    /// importer of possibly many — the same specifier written in two packages
    /// appears twice, because the two may mean different files.
    pub importer: CompactString,
}

/// What one call to [`crate::prepare_builtins`] cost.
///
/// The builtin environment is merged once per process and shared, so the first
/// call pays for it and every later call pays nothing. Both numbers are worth
/// reporting: the cold cost is the floor on a one-shot `uf check`, and the warm
/// cost is what a watch mode or an editor session actually sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinsTiming {
    /// How long this call took.
    pub elapsed: Duration,
    /// How long the one-time merge took, whenever in the process it happened.
    pub cold_elapsed: Duration,
    /// Whether this call is the one that did the merge.
    pub cold: bool,
}

/// The result of checking a batch of files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// Every diagnostic, errors before warnings and ordered within each.
    pub diagnostics: Vec<TypeDiagnostic>,
    /// How many files inference actually ran over.
    pub files_checked: usize,
    /// How many files opted out of inference with `@noflow`.
    ///
    /// Kept apart from `files_checked` rather than folded into it, because a
    /// clean check over a project that opted every file out is not the same
    /// result as a clean check over the project, and a reader is entitled to
    /// tell those apart at a glance.
    pub files_skipped: usize,
    /// How many of [`Self::files_checked`] were answered from a cache rather
    /// than inferred again.
    ///
    /// Reported because it is the only way to tell a fast run from a wrong
    /// one: a check that answered every file from disk and a check that found
    /// nothing to say look identical from the outside, and they are not the
    /// same event. A file that opted out with `@noflow` is not counted, for the
    /// same reason it is not counted in [`Self::files_checked`] — it was not
    /// checked either way, so a cache cannot be what saved it.
    pub files_from_cache: usize,
    /// Module specifiers that resolved to nothing typed, sorted and de-duped.
    ///
    /// A relative import of another file in the batch is checked against that
    /// file's signature, and a package Flow's library definitions declare is
    /// checked against the declaration. What is left over is everything else:
    /// a package name that resolves through `node_modules` or a workspace,
    /// which the checker is not handed; a relative path to a file this run did
    /// not collect; and a file that cannot contribute a signature because it
    /// did not parse or said `@noflow`.
    ///
    /// Flow's answer to a dependency it cannot type is to type the import as
    /// `any` and carry on, which is what happens here — and this list is how a
    /// caller says so out loud instead of letting the hole be silent.
    pub untyped_modules: Vec<CompactString>,
    /// Untyped package imports whose `exports` map only resolved for a
    /// concrete host condition such as `node`, `bun`, `deno` or `browser`.
    ///
    /// These are also present in [`Self::untyped_modules`], because the import
    /// is still typed as `any`. This list says why: the package was visible,
    /// but `uf check` deliberately did not pick one host's branch for a graph
    /// it has to share across hosts.
    pub host_conditional_modules: Vec<CompactString>,
    /// What the shared builtin environment cost.
    pub builtins: BuiltinsTiming,
    /// Wall time spent in inference, excluding the builtin merge.
    pub elapsed: Duration,
}

impl CheckReport {
    /// How many diagnostics have the given severity.
    pub fn count(&self, severity: Severity) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == severity)
            .count()
    }

    /// Whether the run should fail.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.is_error())
    }

    /// Files checked per second, or [`None`] when no measurable time passed.
    ///
    /// Reported rather than logged so a benchmark and the CLI agree on what
    /// throughput means here: inference only, with the builtins already warm.
    /// A run with [`Self::files_from_cache`] above zero is not measuring
    /// inference over those files and this number does not describe it.
    pub fn files_per_second(&self) -> Option<f64> {
        let seconds = self.elapsed.as_secs_f64();
        (seconds > 0.0).then(|| self.files_checked as f64 / seconds)
    }
}
