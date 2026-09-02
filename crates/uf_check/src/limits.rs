//! What the checker refuses to do.
//!
//! Type inference is a fixed-point computation over a graph the user controls,
//! so every input is a potential denial of service: a deeply nested generic
//! recurses, a type that expands into itself diverges, and a file large enough
//! makes the AST alone exhaust memory. Flow already carries the two knobs that
//! matter — `Options::recursion_limit` and `CheckBudget` — and this type is how
//! `uf` sets them, plus the one guard Flow has no opinion on: how much text it
//! is willing to be handed in the first place.

use std::time::Duration;

/// The stack the checker runs on, in bytes.
///
/// Both the parser and inference are recursive descent over user-controlled
/// nesting, so they need far more than a default 2 MiB thread. Giving the
/// worker a large stack turns "deeply nested input aborts the process" into
/// "deeply nested input hits `recursion_limit` and reports a diagnostic",
/// which is the difference between a crash and an error message.
pub const CHECK_STACK_BYTES: usize = 1024 * 1024 * 1024;

/// A default Rust worker gets 2 MiB. Ten thousand nested generics need three
/// orders of magnitude more than that in an unoptimized build, so shrinking
/// this back toward the default would turn a hostile file into an abort.
const _: () = assert!(CHECK_STACK_BYTES >= 512 * 1024 * 1024);

/// The bounds one type check runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckLimits {
    /// The largest source the checker accepts, in bytes.
    ///
    /// Past this the file is rejected with [`crate::CheckError::SourceTooLarge`]
    /// before a parser sees it.
    pub max_source_bytes: usize,
    /// Flow's own limit on how deep inference may recurse.
    ///
    /// Reaching it produces a [`crate::DiagnosticKind::RecursionLimit`]
    /// diagnostic; it does not abort the check.
    pub recursion_limit: u32,
    /// How far a type is expanded before Flow stops unfolding it.
    pub type_expansion_recursion_limit: u32,
    /// Wall-clock budget for one file, or [`None`] for no budget.
    ///
    /// Exhausting it fails with [`crate::CheckError::Budget`].
    pub file_timeout: Option<Duration>,
}

impl CheckLimits {
    /// 4 MiB: comfortably past any hand-written module, and an order of
    /// magnitude below the point where the AST alone becomes a memory problem.
    pub const DEFAULT_MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
    /// Flow's own default, and what `flow_dot_js` runs with.
    pub const DEFAULT_RECURSION_LIMIT: u32 = 10_000;
    /// Flow's own default.
    pub const DEFAULT_TYPE_EXPANSION_RECURSION_LIMIT: u32 = 3;
    /// Long enough that no honest file reaches it, short enough that a
    /// pathological one does not wedge a build.
    pub const DEFAULT_FILE_TIMEOUT: Duration = Duration::from_secs(30);

    /// Limits with no wall-clock budget, for tests that must be deterministic
    /// on a loaded machine.
    pub const fn without_timeout(mut self) -> Self {
        self.file_timeout = None;
        self
    }

    /// Replace the source size limit.
    pub const fn with_max_source_bytes(mut self, bytes: usize) -> Self {
        self.max_source_bytes = bytes;
        self
    }

    /// Replace the wall-clock budget.
    pub const fn with_file_timeout(mut self, timeout: Duration) -> Self {
        self.file_timeout = Some(timeout);
        self
    }
}

impl Default for CheckLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: Self::DEFAULT_MAX_SOURCE_BYTES,
            recursion_limit: Self::DEFAULT_RECURSION_LIMIT,
            type_expansion_recursion_limit: Self::DEFAULT_TYPE_EXPANSION_RECURSION_LIMIT,
            file_timeout: Some(Self::DEFAULT_FILE_TIMEOUT),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limits_reject_a_five_megabyte_file() {
        assert!(CheckLimits::default().max_source_bytes < 5_000_000);
    }

    #[test]
    fn default_limits_accept_a_large_hand_written_module() {
        assert!(CheckLimits::default().max_source_bytes >= 1_000_000);
    }

    #[test]
    fn dropping_the_timeout_keeps_every_other_limit() {
        let limits = CheckLimits::default();

        let without = limits.without_timeout();

        assert_eq!(without.file_timeout, None);
        assert_eq!(without.max_source_bytes, limits.max_source_bytes);
        assert_eq!(without.recursion_limit, limits.recursion_limit);
    }

    #[test]
    fn limits_are_overridable_one_at_a_time() {
        let limits = CheckLimits::default()
            .with_max_source_bytes(64)
            .with_file_timeout(Duration::from_millis(5));

        assert_eq!(limits.max_source_bytes, 64);
        assert_eq!(limits.file_timeout, Some(Duration::from_millis(5)));
    }
}
