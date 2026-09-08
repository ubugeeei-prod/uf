//! Preparing text that came off a checkout for a terminal.
//!
//! A repository is attacker-authored input. `uf` is run against a clone, and
//! nothing stops a filename in that clone from holding an ANSI escape: a
//! diagnostic that names the file then hands `ESC [ 2 J` to the terminal, which
//! clears the screen, and `\r` rewrites the row that was already drawn. That is
//! the same hole `uf_pm::progress::safe_label` closes for a package name taken
//! out of a manager's output, and `docs/security.md`'s first rule — *source
//! text, file paths, … are attacker-controlled in the threat model* — covers a
//! path exactly as it covers registry text.
//!
//! So this module is the one place a reporter prepares a path, and the one
//! place it prepares the message drawn beside it. A message is untrusted for
//! the same reason: an RSC diagnostic quotes the module, an import specifier
//! and an exported name, and all three are read out of the checkout.
//!
//! # What is removed, and what deliberately is not
//!
//! Every **control character** — C0, `DEL`, and the C1 block that holds the
//! eight-bit `CSI` at `U+009B` — because those are what move a cursor, clear a
//! screen, or end a row early. Rust's [`char::is_control`] is exactly that set,
//! and it is exactly the set [`crate::char_width`] already measures as
//! zero-width, so dropping one changes no column and no caret lines up
//! differently for it.
//!
//! **Nothing else.** `\` is a path separator on Windows and renders as itself;
//! a Japanese, Cyrillic or emoji filename is a legitimate filename and renders
//! as itself. Widening this to "strip anything unfamiliar" would make `uf`
//! unusable outside ASCII and would close nothing further, because what steers
//! a terminal is a control character and not an unfamiliar one. What is left is
//! then **cut to a bounded width**, so a path long enough to wrap cannot smear
//! a region that is redrawn in place.
//!
//! This is a guard against a *terminal being driven*, and it is not a guard
//! against a name that reads as another name — a right-to-left override in a
//! filename still reorders what a reader sees. That is a separate question with
//! a separate answer, and claiming it here would be claiming more than the code
//! does.

use std::borrow::Cow;

use crate::text::char_width;

/// The widest a path is drawn before the left of it is elided.
///
/// The same 120 columns [`crate::CodeFrame`] windows a source line to: a
/// header wider than the frame under it is a header that wraps, and a wrapped
/// row is what makes every "cursor up" after it land one line short.
pub const MAX_PATH_WIDTH: usize = 120;

/// The most scalars a prepared path may hold, however narrow they are.
///
/// A width bound alone is not a bound: combining marks and zero-width joiners
/// measure zero columns, so a name built out of them is unbounded output under
/// a column ceiling. Rule 4 — *no unbounded anything* — wants the second one.
pub const MAX_PATH_SCALARS: usize = 4 * MAX_PATH_WIDTH;

/// The widest a diagnostic message is drawn before it is cut.
///
/// Generous on purpose: a message is a sentence, and a sentence cut in half is
/// a finding a reader cannot act on. Flow's longest real messages are a few
/// hundred columns, so this bounds the attacker's half — an import specifier
/// the length of the file it was read from — without touching anybody's
/// diagnostic.
pub const MAX_MESSAGE_WIDTH: usize = 512;

/// The most scalars a prepared message may hold. See [`MAX_PATH_SCALARS`].
pub const MAX_MESSAGE_SCALARS: usize = 4 * MAX_MESSAGE_WIDTH;

/// What marks a value that did not fit.
///
/// ASCII, because [`crate::GlyphSet`] exists for terminals that cannot draw
/// anything else and a path is drawn on both of them.
const ELIDED: &str = "...";

/// The columns [`ELIDED`] occupies.
const ELIDED_WIDTH: usize = 3;

/// A path reduced to something a terminal can be handed.
///
/// Control characters are dropped and the result is cut to [`MAX_PATH_WIDTH`]
/// columns and [`MAX_PATH_SCALARS`] scalars, keeping the **end** of the path
/// behind a leading `...`. The end is what a reader needs: `.../ui/button.js`
/// names the file, and the first hundred columns of a path name a checkout the
/// reader is standing in.
///
/// Borrows when there was nothing to do, which is every path in every ordinary
/// run.
#[must_use]
pub fn safe_path(path: &str) -> Cow<'_, str> {
    if is_drawable(path, MAX_PATH_WIDTH, MAX_PATH_SCALARS) {
        return Cow::Borrowed(path);
    }
    Cow::Owned(keep_tail(path, MAX_PATH_WIDTH, MAX_PATH_SCALARS))
}

/// Append `path`, prepared by [`safe_path`].
pub fn push_safe_path(out: &mut String, path: &str) {
    out.push_str(&safe_path(path));
}

/// A diagnostic message reduced to something a terminal can be handed.
///
/// Control characters are dropped and the result is cut to
/// [`MAX_MESSAGE_WIDTH`] columns and [`MAX_MESSAGE_SCALARS`] scalars, keeping
/// the **start** and marking the cut with a trailing `...` — the opposite end
/// from [`safe_path`], because a sentence says what it is about first and a
/// path says it last.
///
/// Borrows when there was nothing to do.
#[must_use]
pub fn safe_message(message: &str) -> Cow<'_, str> {
    if is_drawable(message, MAX_MESSAGE_WIDTH, MAX_MESSAGE_SCALARS) {
        return Cow::Borrowed(message);
    }
    Cow::Owned(keep_head(message, MAX_MESSAGE_WIDTH, MAX_MESSAGE_SCALARS))
}

/// Append `message`, prepared by [`safe_message`].
pub fn push_safe_message(out: &mut String, message: &str) {
    out.push_str(&safe_message(message));
}

/// Whether `text` can be drawn as it stands.
fn is_drawable(text: &str, max_width: usize, max_scalars: usize) -> bool {
    let mut width = 0usize;
    let mut scalars = 0usize;
    for ch in text.chars() {
        if ch.is_control() {
            return false;
        }
        width += char_width(ch);
        scalars += 1;
        if width > max_width || scalars > max_scalars {
            return false;
        }
    }
    true
}

/// The last `max_width` columns of `text` with its control characters removed,
/// behind [`ELIDED`] when anything was dropped for length.
fn keep_tail(text: &str, max_width: usize, max_scalars: usize) -> String {
    let mut kept: Vec<char> = Vec::new();
    let mut width = 0usize;
    let mut elided = false;
    for ch in text.chars().rev().filter(|ch| !ch.is_control()) {
        let next = width + char_width(ch);
        if next > max_width || kept.len() == max_scalars {
            elided = true;
            break;
        }
        width = next;
        kept.push(ch);
    }
    if elided {
        // Give back the columns the marker needs. `kept` is tail-first, so the
        // start of what will be drawn is the end of it.
        while width > max_width.saturating_sub(ELIDED_WIDTH) {
            match kept.pop() {
                Some(ch) => width -= char_width(ch),
                None => break,
            }
        }
    }

    let mut out = String::with_capacity(kept.len() * 4 + ELIDED.len());
    if elided {
        out.push_str(ELIDED);
    }
    out.extend(kept.iter().rev());
    out
}

/// The first `max_width` columns of `text` with its control characters removed,
/// followed by [`ELIDED`] when anything was dropped for length.
fn keep_head(text: &str, max_width: usize, max_scalars: usize) -> String {
    let budget = max_width.saturating_sub(ELIDED_WIDTH);
    let mut out = String::new();
    let mut width = 0usize;
    let mut elided = false;
    for (scalars, ch) in text.chars().filter(|ch| !ch.is_control()).enumerate() {
        let next = width + char_width(ch);
        if next > budget || scalars == max_scalars {
            elided = true;
            break;
        }
        width = next;
        out.push(ch);
    }
    if elided {
        out.push_str(ELIDED);
    }
    out
}

#[cfg(test)]
mod tests;
