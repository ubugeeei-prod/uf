//! Flow type inference for `uf`.
//!
//! `uf lint` answers "is this file well-formed and idiomatic". This crate
//! answers the other half — "do the types hold" — by running Flow's own
//! inference from the `upstream/flow` submodule rather than approximating it.
//! Nothing here reimplements a typing rule; the crate is an embedding.
//!
//! # Why this is a crate and not a feature on `uf_flow`
//!
//! `uf_flow` is the parser adapter, and `uf_lint` depends on it, so anything
//! added there lands in every crate that lints. Inference needs sixteen path
//! dependencies on the submodule and a nightly old enough to still accept
//! `#![feature(box_patterns)]`; hanging that off `uf_flow` would put the whole
//! lint graph behind those constraints, and Cargo resolves path dependencies
//! even for features that are off. Keeping it here means exactly one crate —
//! and, through it, `uf_cli` — knows about the typing crates.
//!
//! # Availability
//!
//! The checker is compiled in only with the `upstream-typecheck` feature. With
//! it off every entry point returns [`CheckError::Unavailable`], which
//! [`CheckError::is_unavailable`] distinguishes from a real failure.
//!
//! ```
//! # use uf_check::{CheckLimits, Source, check_sources};
//! let sources = [Source::new("app.js", "// @flow\nconst n: number = 1;\n")];
//! match check_sources(&sources, &CheckLimits::default()) {
//!     Ok(report) => assert_eq!(report.files_checked, 1),
//!     Err(error) => assert!(error.is_unavailable()),
//! }
//! ```

#![deny(missing_docs)]

mod cache;
mod diagnostic;
mod error;
mod limits;
mod report;
#[cfg(feature = "upstream-typecheck")]
mod upstream;

pub use crate::cache::CheckCache;
pub use crate::diagnostic::{
    DiagnosticKind, MessageFeatures, MessageSegment, Position, RelatedLocation, RelatedLocations,
    Severity, Span, TypeDiagnostic,
};
pub use crate::error::CheckError;
pub use crate::limits::{CHECK_STACK_BYTES, CheckLimits};
pub use crate::report::{BuiltinsTiming, CheckReport, ModuleClosure, Source};

/// Which type checker a build compiled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckerBackend {
    /// Meta's official Flow Rust port, from `upstream/flow/rust_port`.
    UpstreamRustPort,
    /// No checker: every entry point reports [`CheckError::Unavailable`].
    Unavailable,
}

/// The checker compiled into this build.
pub const fn active_backend() -> CheckerBackend {
    #[cfg(feature = "upstream-typecheck")]
    {
        CheckerBackend::UpstreamRustPort
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    {
        CheckerBackend::Unavailable
    }
}

/// Whether this build can type check.
pub const fn is_available() -> bool {
    matches!(active_backend(), CheckerBackend::UpstreamRustPort)
}

/// A stable identifier for a backend, for `uf inspect` and the LSP.
pub const fn backend_name(backend: CheckerBackend) -> &'static str {
    match backend {
        CheckerBackend::UpstreamRustPort => "upstream-flow-rust-port",
        CheckerBackend::Unavailable => "unavailable",
    }
}

/// Merge Flow's builtin library definitions, or report that they are already
/// merged.
///
/// Calling this before a batch moves the one-time cost somewhere a caller can
/// account for it — a progress line, a benchmark — instead of hiding it inside
/// the first file's timing.
pub fn prepare_builtins() -> Result<BuiltinsTiming, CheckError> {
    #[cfg(feature = "upstream-typecheck")]
    {
        upstream::prepare_builtins()
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    {
        Err(CheckError::Unavailable)
    }
}

/// The batch `seeds` need in order to be checked against what they import.
///
/// [`check_sources`] resolves an import to a file in the batch or to nothing
/// typed, which leaves a caller that wants to check *part* of a project with
/// no good option: hand over the one file and it is checked against nothing,
/// or hand over everything and pay for the whole project's inference. This
/// walks the module graph from `seeds` — through relative specifiers, and
/// through the `exports` map of any `package.json` in `available` — and
/// returns just the sources that are reachable, with the manifests that named
/// them.
///
/// The walk resolves with the same rules the check does, so a specifier that
/// resolved here resolves there. It parses each file it reaches and builds no
/// signatures, so it costs a parse per reachable module rather than an
/// inference.
///
/// ```
/// # use uf_check::{CheckLimits, Source, module_closure};
/// let available = [
///     Source::new("app.js", "import { b } from './b.js';\n"),
///     Source::new("b.js", "export const b = 1;\n"),
///     Source::new("unrelated.js", "export const c = 1;\n"),
/// ];
/// match module_closure(&["app.js"], &available, &CheckLimits::default()) {
///     Ok(closure) => assert_eq!(
///         closure.sources.iter().map(|source| source.path).collect::<Vec<_>>(),
///         ["app.js", "b.js"],
///     ),
///     Err(error) => assert!(error.is_unavailable()),
/// }
/// ```
pub fn module_closure<'a>(
    seeds: &[&str],
    available: &[Source<'a>],
    limits: &CheckLimits,
) -> Result<ModuleClosure<'a>, CheckError> {
    #[cfg(feature = "upstream-typecheck")]
    {
        upstream::module_closure(seeds, available, limits)
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    {
        let _ = (seeds, available, limits);
        Err(CheckError::Unavailable)
    }
}

/// Type check one source file.
pub fn check_source(
    source: Source<'_>,
    limits: &CheckLimits,
) -> Result<Vec<TypeDiagnostic>, CheckError> {
    check_sources(std::slice::from_ref(&source), limits).map(|report| report.diagnostics)
}

/// Type check a batch of files against one shared builtin environment.
///
/// Files are checked in the order given and diagnostics come back in that
/// order, so the result is a function of the input alone.
pub fn check_sources(
    sources: &[Source<'_>],
    limits: &CheckLimits,
) -> Result<CheckReport, CheckError> {
    check_sources_cached(sources, limits, None)
}

/// Type check a batch of files, answering from `cache` whatever it still knows.
///
/// The report is the same report [`check_sources`] would have produced — the
/// cache decides how much work that took and nothing else. Passing [`None`]
/// checks everything, which is what a caller with no project on disk (an
/// editor holding unsaved buffers, say) wants.
pub fn check_sources_cached(
    sources: &[Source<'_>],
    limits: &CheckLimits,
    cache: Option<&CheckCache>,
) -> Result<CheckReport, CheckError> {
    #[cfg(feature = "upstream-typecheck")]
    {
        upstream::check_sources(sources, limits, cache)
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    {
        let _ = (sources, limits, cache);
        Err(CheckError::Unavailable)
    }
}

#[cfg(test)]
mod tests;
