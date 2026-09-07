//! What the checker refuses to do.
//!
//! Type inference is a fixed-point computation over a graph the user controls,
//! so every input is a potential denial of service: a deeply nested generic
//! recurses, a type that expands into itself diverges, and a file large enough
//! makes the AST alone exhaust memory. Flow already carries the knobs that
//! matter — `Options::recursion_limit` and `type_expansion_recursion_limit` —
//! and this type is how `uf` sets them, plus the one guard Flow has no opinion
//! on: how much text it is willing to be handed in the first place.
//!
//! # Every bound here is a bound on the file
//!
//! A depth, an expansion depth, a size in bytes. All three are functions of the
//! source alone, so two runs over the same tree reach them at the same places
//! and report the same diagnostics — on a busy CI runner and on an idle laptop
//! alike. That is not a nicety: a type checker whose answer depends on how busy
//! the machine is cannot be put in CI at all, because a red build no longer
//! means the code is wrong.
//!
//! `uf` used to run a fourth bound, a flat 30-second wall clock per file, and
//! it broke exactly that way — a 6,300-line test file passed on an idle machine
//! and aborted the whole run on a loaded one, with a message indistinguishable
//! from a real failure. ubugeeei-prod/uf#565. A wall clock measures the
//! machine, and the thing it was protecting against — an inference that does
//! not terminate — is what `recursion_limit` and
//! `type_expansion_recursion_limit` are for, both of which bound work rather
//! than time and both of which Flow enforces itself.
//!
//! So [`CheckLimits::file_timeout`] is [`None`] by default and `uf check` sets
//! none. It survives only for an embedder that has to bound *latency* rather
//! than work — an editor that would rather answer late than not at all — and
//! [`crate::CheckError::Budget`] says out loud that what stopped the check was
//! a clock and not the code.

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
    /// Wall-clock budget for one file, or [`None`] for no budget, which is
    /// the default and what `uf check` runs with.
    ///
    /// **Not a bound on the file.** Setting one makes the answer a function of
    /// how busy the machine is, because the same file reaches it under load and
    /// does not when idle; exhausting it fails the whole batch with
    /// [`crate::CheckError::Budget`], which is a failure a reader cannot tell
    /// from a real one. It is here for an embedder that must bound how long a
    /// keystroke waits, and for nothing else. See this module's header.
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

    /// Limits with no wall-clock budget.
    ///
    /// The default already has none; this is for a caller that took one and
    /// wants it back off.
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
            file_timeout: None,
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
        let limits = CheckLimits::default().with_file_timeout(Duration::from_secs(30));

        let without = limits.without_timeout();

        assert_eq!(without.file_timeout, None);
        assert_eq!(without.max_source_bytes, limits.max_source_bytes);
        assert_eq!(without.recursion_limit, limits.recursion_limit);
    }

    #[test]
    fn the_default_limits_bound_the_file_and_never_the_clock() {
        let limits = CheckLimits::default();

        // ubugeeei-prod/uf#565: a wall-clock budget makes the same tree pass on
        // an idle machine and fail on a loaded one, which is a checker nobody
        // can put in CI. What is left bounds depth, expansion and size — all
        // three functions of the source alone.
        assert_eq!(limits.file_timeout, None);
        assert!(limits.recursion_limit > 0);
        assert!(limits.type_expansion_recursion_limit > 0);
        assert!(limits.max_source_bytes > 0);
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
