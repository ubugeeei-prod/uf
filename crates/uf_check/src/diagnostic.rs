//! The typed shape of a Flow type error.
//!
//! Flow does not produce strings. It produces a severity, an error code, a
//! primary location, an optional root location, a message built from labelled
//! inline fragments, and a set of referenced locations that the fragments point
//! at. Flattening that into one line throws away everything an editor, an LSP
//! server, or a code-frame renderer needs, so nothing here is a rendered
//! string: [`TypeDiagnostic::message`] keeps the fragments and
//! [`TypeDiagnostic::related`] keeps the locations they refer to.

use compact_str::CompactString;
use serde::Serialize;
use smallvec::SmallVec;

/// Message fragments held inline before spilling to the heap.
///
/// Flow's own messages are short; six covers the common
/// "Cannot assign _ to _ because _ is incompatible with _" shape.
pub type MessageFeatures = SmallVec<[MessageSegment; 6]>;

/// Related locations held inline before spilling to the heap.
pub type RelatedLocations = SmallVec<[RelatedLocation; 2]>;

/// Whether a diagnostic fails the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// The run fails.
    Error,
    /// The run continues.
    Warning,
}

impl Severity {
    /// The stable identifier used in JSON output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// Which stage of Flow produced a diagnostic.
///
/// Flow distinguishes these because they are suppressed, sorted, and rendered
/// differently; collapsing them into "error" loses the distinction between a
/// checker bug ([`Self::Internal`]), a guard firing
/// ([`Self::RecursionLimit`]), and a real type error ([`Self::Infer`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticKind {
    /// The parser rejected the source, or the checker reported as if it had.
    Parse,
    /// Type inference found a real error.
    Infer,
    /// A lint rule fired inside the checker.
    Lint,
    /// The checker hit an internal invariant.
    Internal,
    /// Two files claim to provide the same module.
    DuplicateProvider,
    /// Inference gave up at [`crate::CheckLimits::recursion_limit`].
    RecursionLimit,
}

impl DiagnosticKind {
    /// The stable identifier used in JSON output.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::Infer => "infer",
            Self::Lint => "lint",
            Self::Internal => "internal",
            Self::DuplicateProvider => "duplicate-provider",
            Self::RecursionLimit => "recursion-limit",
        }
    }
}

/// One point in a source file.
///
/// Both fields are one-based, matching every other diagnostic surface in `uf`,
/// and `column` counts **bytes** rather than characters — which is what the
/// code-frame renderer expects, and what Flow's parser produces before its JSON
/// layer converts to codepoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Position {
    /// One-based line.
    pub line: u32,
    /// One-based column, in bytes.
    pub column: u32,
}

impl Position {
    /// A position at the very start of a file.
    pub const START: Self = Self { line: 1, column: 1 };
}

/// A half-open range in one source file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    /// The file the range belongs to, as Flow reported it. Library definitions
    /// baked into the checker report their own synthetic names.
    pub path: CompactString,
    /// Inclusive start.
    pub start: Position,
    /// Exclusive end.
    pub end: Position,
}

impl Span {
    /// The byte length of the range when it stays on one line.
    ///
    /// A caret span is only meaningful within a single rendered line, so a
    /// multi-line range reports the remainder of its first line as unknown and
    /// yields [`None`].
    pub fn single_line_len(&self) -> Option<usize> {
        (self.start.line == self.end.line)
            .then(|| self.end.column.saturating_sub(self.start.column).max(1) as usize)
    }
}

/// One fragment of a Flow error message.
///
/// Flow's messages interleave prose, quoted code, and references to other
/// locations. Keeping the three apart is what lets a renderer highlight code
/// and hyperlink references instead of printing one grey sentence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum MessageSegment {
    /// Prose.
    Text {
        /// The prose itself.
        text: CompactString,
    },
    /// A quoted type, name, or expression.
    Code {
        /// The quoted source text.
        text: CompactString,
    },
    /// A pointer at one of [`TypeDiagnostic::related`].
    Reference {
        /// The text Flow renders for the reference.
        text: CompactString,
        /// Matches [`RelatedLocation::id`].
        id: u32,
    },
}

impl MessageSegment {
    /// The text this segment contributes to a one-line rendering.
    pub fn text(&self) -> &str {
        match self {
            Self::Text { text } | Self::Code { text } | Self::Reference { text, .. } => text,
        }
    }
}

/// A location one of the message's references points at.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedLocation {
    /// The reference number Flow used in the message, starting at one.
    pub id: u32,
    /// Where the reference points.
    pub span: Span,
}

/// One diagnostic from Flow's own type inference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeDiagnostic {
    /// Whether the diagnostic fails the run.
    pub severity: Severity,
    /// Which stage produced it.
    pub kind: DiagnosticKind,
    /// Flow's own error code, such as `incompatible-type` or `prop-missing`.
    ///
    /// Borrowed from upstream's table rather than mirrored into a `uf` enum:
    /// the codes are Flow's public contract, they gain members every release,
    /// and a copy here would silently drift. [`None`] is Flow reporting an
    /// error it has not given a code to.
    pub code: Option<&'static str>,
    /// Where to put the caret.
    pub primary: Span,
    /// The wider range Flow associates with the error, when it has one. Always
    /// contains [`Self::primary`].
    pub root: Option<Span>,
    /// The message, in fragments.
    pub message: MessageFeatures,
    /// Every location [`MessageSegment::Reference`] fragments point at.
    pub related: RelatedLocations,
}

impl TypeDiagnostic {
    /// Render the message as one line.
    ///
    /// Reference fragments carry a trailing `[n]` marker so a reader can match
    /// them to [`Self::related`], which is how Flow's own CLI renders them.
    pub fn message_text(&self) -> String {
        let mut out = String::with_capacity(
            self.message
                .iter()
                .map(|segment| segment.text().len() + 4)
                .sum(),
        );
        for segment in &self.message {
            out.push_str(segment.text());
            if let MessageSegment::Reference { id, .. } = segment {
                out.push_str(" [");
                // Reference ids are small; formatting through `Display` here
                // would allocate a second buffer per fragment.
                push_u32(&mut out, *id);
                out.push(']');
            }
        }
        out
    }

    /// Whether this diagnostic fails the run.
    pub const fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }
}

/// Append a decimal `u32` without going through `format!`.
fn push_u32(out: &mut String, mut value: u32) {
    if value == 0 {
        out.push('0');
        return;
    }
    let mut digits = [0u8; 10];
    let mut len = 0;
    while value > 0 {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    for index in (0..len).rev() {
        out.push(digits[index] as char);
    }
}

#[cfg(test)]
mod tests;
