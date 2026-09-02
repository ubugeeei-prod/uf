//! Unicode-aware display width, and the small allocation-free text helpers the
//! renderers are built from.
//!
//! Column alignment is a correctness problem, not a cosmetic one: a table whose
//! widths are computed from `str::len` falls apart the moment a path contains a
//! Japanese identifier. The width rules here are implemented natively from the
//! East Asian Width property plus the emoji presentation ranges, with combining
//! marks, zero-width joiners, and variation selectors contributing nothing.

use std::cmp::Ordering;

mod tables;

use tables::{WIDE, ZERO_WIDTH};

/// Horizontal alignment of a padded cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Align {
    /// Pad on the right.
    #[default]
    Left,
    /// Pad on the left.
    Right,
    /// Pad on both sides, favouring the right when the padding is odd.
    Center,
}

/// Zero-width joiner: the following scalar joins the previous cluster.
const ZWJ: char = '\u{200d}';
/// Variation selector 16: request emoji presentation, which is double width.
const VS16: char = '\u{fe0f}';

/// The number of terminal columns one scalar value occupies.
///
/// Control characters and combining marks are zero, East Asian Wide and
/// Fullwidth characters and default-emoji-presentation characters are two, and
/// everything else is one.
pub fn char_width(ch: char) -> usize {
    let code = u32::from(ch);
    if code < 0x20 || (0x7f..0xa0).contains(&code) {
        return 0;
    }
    if in_ranges(ZERO_WIDTH, code) {
        return 0;
    }
    if in_ranges(WIDE, code) { 2 } else { 1 }
}

/// The number of terminal columns a string occupies.
///
/// ANSI escape sequences are skipped, so the width of already-styled text is
/// the width a reader sees. Zero-width-joiner sequences collapse to the width
/// of their first scalar, and a variation selector that requests emoji
/// presentation widens the scalar before it.
pub fn display_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars();
    let mut joined = false;
    let mut previous_narrow = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => {
                skip_escape(&mut chars);
                previous_narrow = false;
            }
            ZWJ => {
                joined = true;
                previous_narrow = false;
            }
            VS16 => {
                if previous_narrow {
                    width += 1;
                    previous_narrow = false;
                }
            }
            _ => {
                let ch_width = char_width(ch);
                if joined {
                    // The scalar continues the previous cluster, which already
                    // paid for its cells.
                    joined = false;
                } else {
                    width += ch_width;
                }
                previous_narrow = ch_width == 1;
            }
        }
    }
    width
}

/// The longest prefix of `text` that fits in `max` columns.
///
/// Slices on a character boundary, never in the middle of one.
pub fn truncate_to_width(text: &str, max: usize) -> &str {
    let mut width = 0usize;
    for (offset, ch) in text.char_indices() {
        let next = width + char_width(ch);
        if next > max {
            return &text[..offset];
        }
        width = next;
    }
    text
}

/// Append `text` padded to `width` columns, using the string's display width.
///
/// Text wider than `width` is written in full rather than truncated: losing a
/// character is worse than losing an alignment.
pub fn push_padded(out: &mut String, text: &str, width: usize, align: Align) {
    let text_width = display_width(text);
    let padding = width.saturating_sub(text_width);
    match align {
        Align::Left => {
            out.push_str(text);
            push_spaces(out, padding);
        }
        Align::Right => {
            push_spaces(out, padding);
            out.push_str(text);
        }
        Align::Center => {
            let left = padding / 2;
            push_spaces(out, left);
            out.push_str(text);
            push_spaces(out, padding - left);
        }
    }
}

/// Append `count` spaces.
pub fn push_spaces(out: &mut String, count: usize) {
    push_repeat(out, ' ', count);
}

/// Append `ch` `count` times.
pub fn push_repeat(out: &mut String, ch: char, count: usize) {
    out.reserve(count * ch.len_utf8());
    for _ in 0..count {
        out.push(ch);
    }
}

/// Append `text` `count` times.
pub fn push_repeat_str(out: &mut String, text: &str, count: usize) {
    out.reserve(count * text.len());
    for _ in 0..count {
        out.push_str(text);
    }
}

/// Append a decimal integer without going through `format!`.
pub fn push_u32(out: &mut String, value: u32) {
    push_usize(out, value as usize);
}

/// Append a decimal integer without going through `format!`.
pub fn push_usize(out: &mut String, value: usize) {
    let mut buffer = [0u8; 20];
    let mut index = buffer.len();
    let mut rest = value;
    loop {
        index -= 1;
        buffer[index] = b'0' + (rest % 10) as u8;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    for byte in &buffer[index..] {
        out.push(char::from(*byte));
    }
}

/// The number of decimal digits in `value`.
pub fn decimal_digits(value: usize) -> usize {
    let mut digits = 1;
    let mut rest = value;
    while rest >= 10 {
        rest /= 10;
        digits += 1;
    }
    digits
}

/// The largest byte index `<= offset` that starts a character.
pub fn floor_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let mut index = offset;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Consume the remainder of an ANSI escape sequence, having seen the `ESC`.
fn skip_escape(chars: &mut std::str::Chars<'_>) {
    let Some(introducer) = chars.next() else {
        return;
    };
    match introducer {
        // CSI: parameters and intermediates, then one final byte.
        '[' => {
            for ch in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&ch) {
                    return;
                }
            }
        }
        // OSC: runs until BEL or ST.
        ']' => {
            for ch in chars.by_ref() {
                if ch == '\u{7}' || ch == '\u{1b}' {
                    return;
                }
            }
        }
        _ => {}
    }
}

fn in_ranges(table: &[(u32, u32)], code: u32) -> bool {
    table
        .binary_search_by(|&(low, high)| {
            if code < low {
                Ordering::Greater
            } else if code > high {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests;
