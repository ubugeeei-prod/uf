//! The bounds the validator runs inside, and the one way it gives up.
//!
//! Validation runs over every module of every build, including modules that
//! came out of `node_modules`, so each limit here is a refusal rather than an
//! attempt to cope: a module that trips one is reported, not analysed harder.

use thiserror::Error;

/// Longest module the validator will scan, in bytes.
///
/// A module that declares components is hand-written; a generated bundle is not
/// something the React compiler has an opinion about. Refusing above this is
/// the guard against unbounded allocation from a hostile or generated file.
pub const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Deepest scope nesting the walk will track.
///
/// The walk keeps an explicit stack rather than recursing, so this is a memory
/// ceiling and not a native-stack one; a module nested deeper than this is not
/// a module a person wrote.
pub const MAX_SCOPE_DEPTH: usize = 256;

/// Most diagnostics one module will produce.
///
/// A module that trips this many is broken in a way the first few findings
/// already said; the rest would only be noise, and holding them would let one
/// file decide how much memory a build uses.
pub const MAX_DIAGNOSTICS: usize = 1_024;

/// Most bindings the validator will track in one module.
pub const MAX_TRACKED_BINDINGS: usize = 8_192;

/// Why a module could not be validated.
///
/// Rule violations are [`ReactDiagnostic`](crate::ReactDiagnostic)s, not
/// errors: a build collects every one of them instead of stopping at the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReactCompilerError {
    /// The module is larger than [`MAX_SOURCE_BYTES`].
    #[error("module is {bytes} bytes, over the {limit} byte ceiling")]
    SourceTooLarge {
        /// Size of the module.
        bytes: usize,
        /// The ceiling, always [`MAX_SOURCE_BYTES`].
        limit: usize,
    },
    /// The module nests scopes deeper than [`MAX_SCOPE_DEPTH`].
    #[error("module nests scopes deeper than {limit}")]
    ScopeTooDeep {
        /// The ceiling, always [`MAX_SCOPE_DEPTH`].
        limit: usize,
        /// Byte offset of the scope that went over.
        offset: usize,
    },
}
