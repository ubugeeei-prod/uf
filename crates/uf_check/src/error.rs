//! Failures of the checker itself, as opposed to type errors in the source.
//!
//! A type error is a [`crate::TypeDiagnostic`]. Everything in this module means
//! the check did not run to completion, which is a different thing and gets a
//! different exit path.

use compact_str::CompactString;
use thiserror::Error;

/// A checker failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CheckError {
    /// The build has no type checker compiled in.
    #[error(
        "Flow type inference is not compiled into this build; rebuild uf_check with the \
         `upstream-typecheck` feature"
    )]
    Unavailable,
    /// The source is larger than [`crate::CheckLimits::max_source_bytes`].
    ///
    /// Inference allocates per AST node, so an unbounded file is an unbounded
    /// allocation. The limit is checked before the parser is handed the text.
    #[error("{path} is {size} bytes, over the {limit}-byte type-check limit")]
    SourceTooLarge {
        /// The file that was rejected.
        path: CompactString,
        /// Its size in bytes.
        size: usize,
        /// The configured limit.
        limit: usize,
    },
    /// Inference ran out of its wall-clock budget.
    #[error("type inference for {path} exceeded its {limit_ms}ms budget")]
    Budget {
        /// The file being checked.
        path: CompactString,
        /// The configured budget.
        limit_ms: u64,
    },
    /// Inference was asked to stop.
    #[error("type inference for {path} was cancelled")]
    Cancelled {
        /// The file being checked.
        path: CompactString,
    },
    /// The builtin library definitions did not merge.
    ///
    /// This means the vendored `upstream/flow` submodule and the checker
    /// disagree, not that anything is wrong with the user's code.
    #[error("Flow's builtin library definitions failed to load: {detail}")]
    Builtins {
        /// What upstream reported.
        detail: CompactString,
    },
    /// The worker thread the check runs on could not be started or died.
    #[error("the type-check worker failed for {path}: {detail}")]
    Worker {
        /// The file being checked.
        path: CompactString,
        /// What went wrong.
        detail: CompactString,
    },
}

impl CheckError {
    /// Whether the failure is the absence of a compiled-in checker rather than
    /// something going wrong.
    ///
    /// Callers use this to stay quiet on a default build instead of reporting a
    /// failure the user cannot act on.
    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }
}
