//! Rewrites as byte spans, and the line-count guarantee that applying them keeps.
//!
//! `uf`'s source maps are a per-line table: output line *n* of a module is
//! source line *n*. Every transform in the pipeline holds that up, which is why
//! [`strip`](uf_flow::strip) blanks types in place instead of deleting them.
//! Lowering JSX has to hold it up too, and it is the harder case: an element
//! written over eight lines becomes one call.
//!
//! The rule that makes it work is enforced here rather than remembered at each
//! call site. [`Edit::replace`] is **padded** at apply time: if the span it
//! covers held more line terminators than the replacement does, the missing
//! ones are appended. So a replacement can never *shrink* the file's line
//! count, whatever text it carries.
//!
//! [`Edit::squash`] is the one exception, and it exists for exactly one job:
//! moving a `key={…}` expression out of the props and behind them. The bytes it
//! covers become spaces, newlines included, and the same newlines travel with
//! the copied text — so the pair nets out at zero and the file's line count is
//! unchanged. Nothing else may use it.

use std::ops::Range;

/// What replaces a span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Replacement {
    /// Overwrite with spaces, keeping every line terminator.
    Blank,
    /// Overwrite with text, padded with any line terminators the span had and
    /// the text does not.
    Text(String),
    /// Overwrite with spaces, line terminators included.
    ///
    /// Only legal when the same line terminators are re-emitted elsewhere in
    /// the same transform. See the module docs.
    Squash,
}

/// One rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) replacement: Replacement,
}

impl Edit {
    pub(crate) fn blank(span: Range<usize>) -> Self {
        Self {
            start: span.start,
            end: span.end,
            replacement: Replacement::Blank,
        }
    }

    pub(crate) fn replace(span: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            start: span.start,
            end: span.end,
            replacement: Replacement::Text(text.into()),
        }
    }

    pub(crate) fn insert(at: usize, text: impl Into<String>) -> Self {
        Self::replace(at..at, text)
    }

    pub(crate) fn squash(span: Range<usize>) -> Self {
        Self {
            start: span.start,
            end: span.end,
            replacement: Replacement::Squash,
        }
    }
}

/// Apply ordered edits to `source`.
///
/// Overlapping edits keep the first, and an edit reaching past the end of the
/// source is dropped: the spans all come from one token stream over this exact
/// string, but a rewriter must not be the thing that panics if that ever stops
/// being true.
pub(crate) fn apply(source: &str, edits: &mut [Edit]) -> String {
    edits.sort_by_key(|edit| (edit.start, edit.end));

    let mut out = String::with_capacity(source.len() + source.len() / 4);
    let mut cursor = 0usize;

    for edit in edits.iter() {
        if edit.start < cursor || edit.end > source.len() || edit.start > edit.end {
            continue;
        }
        out.push_str(&source[cursor..edit.start]);
        let span = &source[edit.start..edit.end];
        match &edit.replacement {
            Replacement::Blank => blank_into(&mut out, span, true),
            Replacement::Squash => blank_into(&mut out, span, false),
            Replacement::Text(text) => {
                out.push_str(text);
                let terminator = terminator_of(span);
                for _ in 0..newlines(span).saturating_sub(newlines(text)) {
                    out.push_str(terminator);
                }
            }
        }
        cursor = edit.end;
    }
    out.push_str(&source[cursor..]);
    out
}

/// Overwrite a span with spaces, optionally keeping its line terminators.
fn blank_into(out: &mut String, span: &str, keep_lines: bool) {
    for character in span.chars() {
        match character {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' if keep_lines => out.push(character),
            _ => {
                for _ in 0..character.len_utf8() {
                    out.push(' ');
                }
            }
        }
    }
}

/// The line terminator a span uses, so padding matches the file it lands in.
///
/// A module with CRLF endings must not come back with a mix of the two.
fn terminator_of(span: &str) -> &'static str {
    if span.contains("\r\n") {
        "\r\n"
    } else if span.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

/// How many line terminators a string holds.
///
/// `\r\n` counts once, because it ends one line and not two.
pub(crate) fn newlines(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut count = 0usize;
    let mut at = 0usize;
    while at < bytes.len() {
        match bytes[at] {
            b'\n' => {
                count += 1;
                at += 1;
            }
            b'\r' => {
                count += 1;
                at += if bytes.get(at + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
            }
            0xe2 if bytes.get(at + 1) == Some(&0x80)
                && matches!(bytes.get(at + 2), Some(0xa8 | 0xa9)) =>
            {
                count += 1;
                at += 3;
            }
            _ => at += 1,
        }
    }
    count
}
