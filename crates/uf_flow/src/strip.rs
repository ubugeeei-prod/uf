//! Flow type erasure: turning `// @flow` source into the JavaScript that ships.
//!
//! `uf` projects are written in Flow and shipped as JavaScript, so something
//! has to remove the types. That job belongs to this crate — the one that owns
//! Flow syntax — rather than to the bundler, which should never learn what a
//! `renders` clause is.
//!
//! # What erasure guarantees
//!
//! * **Byte length is preserved** for everything that is erased. A type is
//!   overwritten with spaces and its line terminators are kept, so an offset
//!   computed on the original source still points at the same token in the
//!   result. `flow-remove-types` takes the same approach for the same reason.
//! * **Line count is preserved** unconditionally, including by the two
//!   rewrites that do change length ([`component` and `hook`
//!   declarations](strip_types)). A source map for stripped output is therefore
//!   an identity mapping line for line, which is why one is cheap to produce.
//! * **Erasure fails open.** Every rule proves what it is looking at from the
//!   tokens around it, and a rule that cannot leaves the bytes alone. The
//!   result is that unusual sources keep types they did not need to keep, not
//!   that they lose code.
//!
//! # What is not erased
//!
//! Flow enum declarations and `as` casts are left in place: the first is a
//! runtime construct that needs a helper this crate does not ship, and the
//! second cannot be told from `import * as ns` and `export { a as b }` without
//! a parser. Both are reported by [`Stripped::is_unchanged`] being false only
//! when something else was erased, never silently rewritten.

use thiserror::Error;

mod erase;
mod span;

#[cfg(test)]
mod tests;

use erase::{Edit, Replacement};

/// Longest source the eraser will look at, in bytes.
///
/// A dependency can put a generated or hostile file in `node_modules`, and
/// every scan in `uf` has an explicit ceiling above it rather than trusting the
/// input to be a reasonable size.
pub const MAX_STRIP_BYTES: usize = 8 * 1024 * 1024;

/// Why a source could not be stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StripError {
    /// The source is larger than [`MAX_STRIP_BYTES`].
    #[error("source is {bytes} bytes, over the {limit} byte ceiling")]
    SourceTooLarge {
        /// Size of the rejected source.
        bytes: usize,
        /// The ceiling, always [`MAX_STRIP_BYTES`].
        limit: usize,
    },
}

/// Source with its Flow-only syntax removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stripped {
    /// The JavaScript to emit.
    pub code: String,
    /// How many spans were erased or rewritten.
    pub erasures: u32,
}

impl Stripped {
    /// Whether the source held no Flow-only syntax.
    #[must_use]
    pub fn is_unchanged(&self) -> bool {
        self.erasures == 0
    }
}

/// Remove Flow-only syntax from `source`.
///
/// Erased: type aliases, opaque types, interfaces, `declare` statements,
/// `import type`/`export type` statements and specifiers, parameter, return,
/// variable and class-member annotations, declaration type parameters, and the
/// type arguments of generic calls.
///
/// Rewritten: `component Name(a: A, b: B) renders R { … }` becomes
/// `function Name({a, b}) { … }`, and `hook useName(…)` becomes
/// `function useName(…)`. A component whose parameters cannot be spelled as a
/// destructuring pattern — a string key, or an `as` rename — keeps its
/// parameter list unchanged rather than being rewritten into something that
/// would not parse.
pub fn strip_types(source: &str) -> Result<Stripped, StripError> {
    if source.len() > MAX_STRIP_BYTES {
        return Err(StripError::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_STRIP_BYTES,
        });
    }

    let tokens = crate::scan::tokenize(source);
    let edits = erase::collect_edits(source, &tokens);
    if edits.is_empty() {
        return Ok(Stripped {
            code: source.to_string(),
            erasures: 0,
        });
    }

    Ok(Stripped {
        code: apply(source, &edits),
        erasures: u32::try_from(edits.len()).unwrap_or(u32::MAX),
    })
}

/// Apply ordered edits, keeping the first of any pair that overlaps.
fn apply(source: &str, edits: &[Edit]) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0usize;

    for edit in edits {
        if edit.start < cursor || edit.end > source.len() {
            continue;
        }
        output.push_str(&source[cursor..edit.start]);
        match edit.replacement {
            Replacement::Blank => blank_into(&mut output, &source[edit.start..edit.end]),
            Replacement::Text(text) => output.push_str(text),
        }
        cursor = edit.end;
    }
    output.push_str(&source[cursor..]);
    output
}

/// Overwrite a span with spaces, keeping line terminators and byte length.
fn blank_into(output: &mut String, span: &str) {
    for character in span.chars() {
        match character {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => output.push(character),
            _ => {
                for _ in 0..character.len_utf8() {
                    output.push(' ');
                }
            }
        }
    }
}
