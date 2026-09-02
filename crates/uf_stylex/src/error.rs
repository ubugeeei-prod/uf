//! Where a StyleX module can be refused, and why.
//!
//! Every variant carries the position it was found at, because a `.stylex.js`
//! module is source a human wrote and the only useful failure is one that says
//! which declaration is wrong. The ceilings are here rather than in the parser
//! so that "how much will uf hold in memory for one module" is a list you can
//! read in one place: a dependency in `node_modules` can ship a hostile
//! `.stylex.js`, and every one of these limits exists because of that.

use compact_str::CompactString;
use serde::Serialize;
use thiserror::Error;
use uf_infra::LineIndex;

/// Longest module the StyleX pass will scan, in bytes.
///
/// Smaller than [`uf_rsc::MAX_SOURCE_BYTES`] on purpose: the RSC scan has to
/// cope with generated bundles, while a module that authors styles is
/// hand-written. Anything larger is refused rather than tokenized, which is the
/// guard against unbounded allocation from a generated or hostile file.
pub const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Deepest object nesting accepted inside a `stylex.create` call.
///
/// `namespace -> property -> condition -> value` is three levels of keys, and
/// that is all uf compiles. The cap is what lets the extractor walk with an
/// explicit stack instead of recursing, so a `.stylex.js` full of `{{{{…}}}}`
/// cannot exhaust the native stack.
pub const MAX_OBJECT_DEPTH: u32 = 3;

/// Longest CSS value accepted, in bytes.
pub const MAX_VALUE_BYTES: usize = 512;

/// Most declarations one module may contribute to the sheet.
pub const MAX_DECLARATIONS: usize = 16_384;

/// A 1-based position in a module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePosition {
    /// 1-based line.
    pub line: u32,
    /// 1-based byte column within the line.
    pub column: u32,
    /// Byte offset from the start of the module.
    pub offset: u32,
}

impl SourcePosition {
    /// Resolve a byte offset through a module's line index.
    pub fn new(index: &LineIndex, offset: usize) -> Self {
        let position = index.line_col(offset);
        Self {
            line: clamp_u32(position.line),
            column: clamp_u32(position.column),
            offset: clamp_u32(offset),
        }
    }
}

/// Clamp a `usize` into the `u32` positions are reported in.
pub(crate) fn clamp_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Why a module could not be compiled.
///
/// Rejecting is always better than guessing here: a StyleX declaration uf does
/// not understand would otherwise be dropped from the sheet, and a dropped rule
/// is a visual bug that only shows up in production.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StyleXError {
    /// The module is larger than [`MAX_SOURCE_BYTES`].
    #[error("module is {bytes} bytes, over the {limit} byte ceiling")]
    SourceTooLarge {
        /// Size of the module.
        bytes: usize,
        /// The ceiling, always [`MAX_SOURCE_BYTES`].
        limit: usize,
    },
    /// A `stylex.create(...)` call is not followed by an object literal.
    #[error("expected an object literal argument at {}:{}", at.line, at.column)]
    ExpectedObjectLiteral {
        /// Where the argument should have been.
        at: SourcePosition,
    },
    /// A brace, bracket or parenthesis is never closed.
    #[error("unterminated object literal at {}:{}", at.line, at.column)]
    UnterminatedObject {
        /// Where the unbalanced group opened.
        at: SourcePosition,
    },
    /// An object entry is not `key: value`.
    #[error("expected `key: value` at {}:{}", at.line, at.column)]
    MalformedEntry {
        /// Where the entry started.
        at: SourcePosition,
    },
    /// Objects nest deeper than [`MAX_OBJECT_DEPTH`].
    #[error("object nesting is deeper than {limit} at {}:{}", at.line, at.column)]
    NestingTooDeep {
        /// Where the too-deep object opened.
        at: SourcePosition,
        /// The ceiling, always [`MAX_OBJECT_DEPTH`].
        limit: u32,
    },
    /// A value uf cannot resolve at compile time.
    #[error("value `{value}` at {}:{} is not a compile-time constant", at.line, at.column)]
    UnsupportedValue {
        /// Where the value started.
        at: SourcePosition,
        /// The value's source text, truncated to the value ceiling.
        value: CompactString,
    },
    /// A key that is not a usable CSS property, namespace or condition.
    #[error("`{key}` at {}:{} is not a usable key", at.line, at.column)]
    InvalidKey {
        /// Where the key was written.
        at: SourcePosition,
        /// The key as written.
        key: CompactString,
    },
    /// A key that would poison the prototype of the emitted object literal.
    ///
    /// The compiled module is JavaScript uf writes, so a `__proto__` key
    /// extracted from a dependency's `.stylex.js` would be a prototype
    /// pollution primitive in generated code (CWE-1321).
    #[error("`{key}` at {}:{} is a reserved object key", at.line, at.column)]
    ForbiddenKey {
        /// Where the key was written.
        at: SourcePosition,
        /// The key as written.
        key: CompactString,
    },
    /// A value that would let a declaration escape its own CSS rule.
    ///
    /// A `}` closes the rule, a `;` starts a sibling declaration, and a `<`
    /// closes the `<style>` element the sheet may be inlined into — the last of
    /// which is a stored-XSS primitive, so all three are refused rather than
    /// escaped.
    #[error("value at {}:{} contains `{fragment}`, which cannot appear in CSS uf emits", at.line, at.column)]
    UnsafeValue {
        /// Where the value was written.
        at: SourcePosition,
        /// The offending fragment.
        fragment: CompactString,
    },
    /// A value longer than [`MAX_VALUE_BYTES`].
    #[error("value at {}:{} is {bytes} bytes, over the {limit} byte ceiling", at.line, at.column)]
    ValueTooLong {
        /// Where the value was written.
        at: SourcePosition,
        /// Size of the value.
        bytes: usize,
        /// The ceiling, always [`MAX_VALUE_BYTES`].
        limit: usize,
    },
    /// A `X.y` value whose `X` is not a binding produced by `stylex.defineVars`.
    #[error("`{binding}` at {}:{} is not a StyleX variables object", at.line, at.column)]
    UnknownVariableBinding {
        /// Where the reference was written.
        at: SourcePosition,
        /// The binding that could not be resolved.
        binding: CompactString,
    },
    /// The module declares more than [`MAX_DECLARATIONS`] declarations.
    #[error("module declares more than {limit} style declarations")]
    TooManyDeclarations {
        /// The ceiling, always [`MAX_DECLARATIONS`].
        limit: usize,
    },
}

impl StyleXError {
    /// Where the failure was found, when it has a position.
    pub fn position(&self) -> Option<SourcePosition> {
        match self {
            Self::SourceTooLarge { .. } | Self::TooManyDeclarations { .. } => None,
            Self::ExpectedObjectLiteral { at }
            | Self::UnterminatedObject { at }
            | Self::MalformedEntry { at }
            | Self::NestingTooDeep { at, .. }
            | Self::UnsupportedValue { at, .. }
            | Self::InvalidKey { at, .. }
            | Self::ForbiddenKey { at, .. }
            | Self::UnsafeValue { at, .. }
            | Self::ValueTooLong { at, .. }
            | Self::UnknownVariableBinding { at, .. } => Some(*at),
        }
    }
}
