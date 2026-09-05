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
    pub fn files_per_second(&self) -> Option<f64> {
        let seconds = self.elapsed.as_secs_f64();
        (seconds > 0.0).then(|| self.files_checked as f64 / seconds)
    }
}
