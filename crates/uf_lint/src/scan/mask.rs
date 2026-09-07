//! Blanking the comments that sit *inside* a line.
//!
//! # The bug this exists to close
//!
//! [`super::line::scan_line`] used to stop a line's code at the first `/*` and
//! call it a comment to the end, even when the comment closed three characters
//! later. Its own doc comment said that cost "a few diagnostics" and "never
//! invents one".
//!
//! It invented plenty. `{/* a comment */}` in a JSX tree is a `{` with no `}`,
//! so the brace stack `flow/nested-component` keeps never came back down, and
//! **every `component` declared after it in the file** was reported as nested
//! inside something — however plainly it sat at module scope
//! (ubugeeei-prod/uf#476). One comment in one component made the rest of the
//! file unlintable.
//!
//! # Why blanking rather than stitching
//!
//! The alternative was to let a line's code be several spans instead of one.
//! That changes [`super::Line`] from `Copy` to something that owns a list, and
//! it changes every one of the three dozen `line.code()` call sites, for a
//! problem that has a much simpler shape: a comment is text nobody should see,
//! and a space is text that means nothing.
//!
//! So a comment that opens and closes on one line is replaced, byte for byte,
//! with spaces. The file keeps its exact length and every offset in it, so line
//! and column numbers, diagnostic spans and every existing rule are untouched —
//! and `scan_line` never sees a `/*` to stop at, so the code after it is code
//! again.
//!
//! A comment that spans lines is left alone: `scan_line` already carries those
//! from line to line correctly, and blanking one would only be a slower way to
//! reach the same answer.
//!
//! So is a comment with nothing after it. `scan_line` already ends the line's
//! code at one, and blanking it would leave a line of spaces that
//! `uniflowed/no-trailing-whitespace` reports — 1,556 of them in this
//! repository, every one of them a `/** … */` above a declaration. The only
//! comment worth blanking is the one with code on the far side of it, because
//! that is the only one whose code was being lost.
//!
//! # Why it matches `scan_line`'s idea of a comment exactly
//!
//! Neither this nor `scan_line` knows a regular expression from a division, so
//! both read `/*` as a comment wherever it appears outside a string. That is a
//! shared blind spot rather than a disagreement, which is the important
//! property: two scanners that disagreed about where a comment is would produce
//! a code slice neither of them describes.

use std::borrow::Cow;

/// Replace every same-line `/* … */` with spaces.
///
/// Returns the source unchanged, and unallocated, when there is nothing to
/// blank — which is most files.
pub(crate) fn mask_inline_comments(source: &str) -> Cow<'_, str> {
    let bytes = source.as_bytes();
    // Cheap bail: no `/*` anywhere means no work, and a file without one is the
    // common case on a hot path that runs per file per lint.
    if !contains_block_open(bytes) {
        return Cow::Borrowed(source);
    }

    let mut out: Option<Vec<u8>> = None;
    let mut index = 0usize;
    let mut quote: Option<u8> = None;

    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            // Inside a string: only its own closing quote ends it, and a
            // backslash hides the next byte. A `'` or `"` string cannot cross a
            // newline, so one ends the literal as well — an unterminated quote
            // is a syntax error, and treating the rest of the file as a string
            // would blank nothing at all after it.
            Some(open) => {
                if byte == b'\\' {
                    index += 2;
                    continue;
                }
                if byte == open || (open != b'`' && byte == b'\n') {
                    quote = None;
                }
                index += 1;
            }
            None => match byte {
                b'\'' | b'"' | b'`' => {
                    quote = Some(byte);
                    index += 1;
                }
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    // A line comment runs to the newline, and `scan_line` cuts
                    // the line there anyway.
                    index = newline_from(bytes, index).unwrap_or(bytes.len());
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    let line_end = newline_from(bytes, index).unwrap_or(bytes.len());
                    match close_from(bytes, index + 2, line_end) {
                        // Only when something follows it on the line. A comment
                        // that ends the line — the whole-line `/** … */` above
                        // a declaration, or a note after the last statement —
                        // is already handled correctly by `scan_line`, and
                        // blanking one would turn it into a line of spaces that
                        // `uniflowed/no-trailing-whitespace` then reports. That
                        // is 1,556 errors in this repository alone, which is
                        // what the narrow rule is here to avoid.
                        Some(close) if has_code_after(bytes, close + 2, line_end) => {
                            let end = close + 2;
                            let buffer = out.get_or_insert_with(|| source.as_bytes().to_vec());
                            buffer[index..end].fill(b' ');
                            index = end;
                        }
                        Some(close) => {
                            index = close + 2;
                        }
                        None => {
                            // Spans lines: `scan_line` carries it, and there is
                            // nothing on this line after it to rescue.
                            index = match close_from(bytes, index + 2, bytes.len()) {
                                Some(close) => close + 2,
                                None => bytes.len(),
                            };
                        }
                    }
                }
                _ => index += 1,
            },
        }
    }

    match out {
        // Every byte written was an ASCII space over a byte range that started
        // at `/` and ended at `/`, so the result is still valid UTF-8 — but the
        // fallible conversion is kept rather than an `unwrap`, because a panic
        // in a linter over somebody's source file is the wrong failure.
        Some(buffer) => match String::from_utf8(buffer) {
            Ok(masked) => Cow::Owned(masked),
            Err(_) => Cow::Borrowed(source),
        },
        None => Cow::Borrowed(source),
    }
}

fn contains_block_open(bytes: &[u8]) -> bool {
    uf_infra::memchr_iter(b'/', bytes).any(|at| bytes.get(at + 1) == Some(&b'*'))
}

/// Whether anything but whitespace stands between `from` and `limit`.
fn has_code_after(bytes: &[u8], from: usize, limit: usize) -> bool {
    bytes[from.min(bytes.len())..limit.min(bytes.len())]
        .iter()
        .any(|byte| !byte.is_ascii_whitespace())
}

fn newline_from(bytes: &[u8], from: usize) -> Option<usize> {
    uf_infra::memchr_iter(b'\n', &bytes[from..])
        .next()
        .map(|at| from + at)
}

/// The `*/` at or after `from` and before `limit`.
fn close_from(bytes: &[u8], from: usize, limit: usize) -> Option<usize> {
    let mut index = from;
    while index + 1 < limit.min(bytes.len()) {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests;
