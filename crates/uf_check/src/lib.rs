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

mod diagnostic;
mod error;
mod limits;
mod report;
#[cfg(feature = "upstream-typecheck")]
mod upstream;

pub use crate::diagnostic::{
    DiagnosticKind, MessageFeatures, MessageSegment, Position, RelatedLocation, RelatedLocations,
    Severity, Span, TypeDiagnostic,
};
pub use crate::error::CheckError;
pub use crate::limits::{CHECK_STACK_BYTES, CheckLimits};
pub use crate::report::{BuiltinsTiming, CheckReport, Source};

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
    #[cfg(feature = "upstream-typecheck")]
    {
        upstream::check_sources(sources, limits)
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    {
        let _ = (sources, limits);
        Err(CheckError::Unavailable)
    }
}

#[cfg(test)]
mod tests;
