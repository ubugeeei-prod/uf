//! What JSX text means, and what a JSX string literal says.
//!
//! Text between tags is not a string literal: it is trimmed per line, joined
//! with single spaces, and dropped entirely when nothing is left. Getting this
//! wrong is the classic way a JSX transform breaks a page — `<p>\n  hello\n</p>`
//! must render `hello`, not `"\n  hello\n"`, and `<b>a</b> <i>b</i>` on one line
//! must keep the space between them.
//!
//! The rule implemented here is the one every React toolchain implements, so
//! that a component looks the same however it was built:
//!
//! * tabs become spaces;
//! * every line but the first loses its leading spaces;
//! * every line but the last loses its trailing spaces;
//! * a line that is empty afterwards contributes nothing;
//! * every surviving line but the last non-empty one gains a trailing space.

use compact_str::CompactString;

/// Longest entity name the decoder will look at, `&` and `;` excluded.
const MAX_ENTITY_BYTES: usize = 32;

/// The HTML entities JSX sources actually use.
///
/// Deliberately short: an unknown entity is left exactly as written rather
/// than guessed at, so a page shows `&hellip;` instead of the wrong glyph.
const NAMED_ENTITIES: &[(&str, char)] = &[
    ("amp", '&'),
    ("apos", '\''),
    ("copy", '\u{a9}'),
    ("gt", '>'),
    ("lt", '<'),
    ("mdash", '\u{2014}'),
    ("ndash", '\u{2013}'),
    ("nbsp", '\u{a0}'),
    ("quot", '"'),
    ("times", '\u{d7}'),
];

/// Clean one run of JSX text into the string it renders as.
///
/// Returns [`None`] when the run is whitespace only, which is how the blank
/// lines between two elements stop being children.
#[must_use]
pub fn clean(raw: &str) -> Option<CompactString> {
    let lines: Vec<&str> = split_lines(raw);
    // Zero rather than "none" when every line is blank: a run that is only
    // spaces on one line is a real child — it is the space that holds
    // `<b>a</b> <i>b</i>` apart — and it must not gain a second one.
    let last_non_empty = lines
        .iter()
        .rposition(|line| line.bytes().any(|byte| byte != b' ' && byte != b'\t'))
        .unwrap_or(0);

    let mut out = String::with_capacity(raw.len());
    for (index, line) in lines.iter().enumerate() {
        let mut trimmed = line.replace('\t', " ");
        if index > 0 {
            trimmed = trimmed.trim_start_matches(' ').to_string();
        }
        if index + 1 < lines.len() {
            trimmed = trimmed.trim_end_matches(' ').to_string();
        }
        if trimmed.is_empty() {
            continue;
        }
        if index != last_non_empty {
            trimmed.push(' ');
        }
        out.push_str(&trimmed);
    }

    if out.is_empty() {
        return None;
    }
    Some(decode_entities(&out))
}

/// Split on every line terminator JavaScript recognizes.
fn split_lines(raw: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let bytes = raw.as_bytes();
    let mut start = 0usize;
    let mut at = 0usize;

    while at < bytes.len() {
        let width = match bytes[at] {
            b'\n' => 1,
            b'\r' if bytes.get(at + 1) == Some(&b'\n') => 2,
            b'\r' => 1,
            0xe2 if bytes.get(at + 1) == Some(&0x80)
                && matches!(bytes.get(at + 2), Some(0xa8 | 0xa9)) =>
            {
                3
            }
            _ => {
                at += 1;
                continue;
            }
        };
        lines.push(&raw[start..at]);
        at += width;
        start = at;
    }
    lines.push(&raw[start..]);
    lines
}

/// Decode the HTML entities a JSX text or attribute value may carry.
#[must_use]
pub fn decode_entities(text: &str) -> CompactString {
    if !text.contains('&') {
        return CompactString::new(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after
            .as_bytes()
            .iter()
            .take(MAX_ENTITY_BYTES)
            .position(|byte| *byte == b';')
        else {
            out.push('&');
            rest = after;
            continue;
        };
        match decode_one(&after[..end]) {
            Some(character) => {
                out.push(character);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    CompactString::new(out)
}

/// One entity body, without its `&` and `;`.
fn decode_one(body: &str) -> Option<char> {
    if let Some(digits) = body.strip_prefix('#') {
        let code = match digits.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => digits.parse::<u32>().ok()?,
        };
        return char::from_u32(code);
    }
    NAMED_ENTITIES
        .iter()
        .find(|(name, _)| *name == body)
        .map(|(_, character)| *character)
}

/// Render `value` as a double-quoted JavaScript string literal.
#[must_use]
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            character if (character as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", character as u32);
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Whether `name` can be written as a bare object key.
#[must_use]
pub fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_' || first == b'$') {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$')
}
