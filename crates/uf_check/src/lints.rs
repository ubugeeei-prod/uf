//! Flow lint severities a project asked `uf check` to run.
//!
//! `uf lint` decides these rules from source text only when it can. The rest
//! are Flow's own, and they fire inside inference when a project sets a level.
//! An empty list leaves every one of them off, which is what a caller that
//! has not read a config gets.

use compact_str::CompactString;

/// How loudly one Flow lint speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowLintLevel {
    /// The lint does not run.
    Off,
    /// The lint reports, and the run continues.
    Warn,
    /// The lint fails the run.
    Error,
}

impl FlowLintLevel {
    /// The spelling a cache key uses.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// One Flow lint and the level a project configured for it.
///
/// `name` is Flow's own spelling, without the `flow/` prefix: `sketchy-null`,
/// not `flow/sketchy-null`. A name Flow does not know is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowLint {
    /// Bare Flow lint name.
    pub name: CompactString,
    /// The level the project asked for.
    pub level: FlowLintLevel,
}
