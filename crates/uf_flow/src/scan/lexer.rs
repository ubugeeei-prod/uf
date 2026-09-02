//! Lexing one JavaScript token, and skipping what sits between them.
//!
//! Everything here is a byte scan with no lookahead beyond a couple of bytes.
//! The one decision that is not local is whether a `/` opens a regular
//! expression, which is answered from the token before it — see
//! [`regex_allowed`].

use super::{Token, TokenKind};

pub(super) fn skip_bom(bytes: &[u8]) -> usize {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    }
}

/// Width in bytes of a line terminator at `at`, if there is one.
///
/// Handles LF, CR, CRLF and the two non-ASCII terminators U+2028 and U+2029.
pub(super) fn line_terminator_width(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b'\n' => Some(1),
        b'\r' if bytes.get(at + 1) == Some(&b'\n') => Some(2),
        b'\r' => Some(1),
        0xe2 if bytes.get(at + 1) == Some(&0x80)
            && matches!(bytes.get(at + 2), Some(0xa8 | 0xa9)) =>
        {
            Some(3)
        }
        _ => None,
    }
}

/// Width in bytes of non-line-terminator whitespace at `at`, if there is any.
pub(super) fn whitespace_width(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b' ' | b'\t' | 0x0b | 0x0c => Some(1),
        // U+00A0 NO-BREAK SPACE.
        0xc2 if bytes.get(at + 1) == Some(&0xa0) => Some(2),
        // U+FEFF, legal whitespace away from the BOM.
        0xef if bytes.get(at + 1) == Some(&0xbb) && bytes.get(at + 2) == Some(&0xbf) => Some(3),
        _ => None,
    }
}

pub(super) fn line_end(bytes: &[u8], from: usize) -> usize {
    let mut cursor = from;
    while cursor < bytes.len() && line_terminator_width(bytes, cursor).is_none() {
        cursor += 1;
    }
    cursor
}

pub(super) fn block_comment_end(bytes: &[u8], from: usize) -> (usize, bool) {
    let mut cursor = from + 2;
    let mut saw_newline = false;
    while cursor < bytes.len() {
        if line_terminator_width(bytes, cursor).is_some() {
            saw_newline = true;
        }
        if bytes[cursor] == b'*' && bytes.get(cursor + 1) == Some(&b'/') {
            return (cursor + 2, saw_newline);
        }
        cursor += 1;
    }
    (bytes.len(), saw_newline)
}

pub(super) fn lex_token(
    bytes: &[u8],
    cursor: &mut usize,
    prev: Option<&Token>,
    source: &str,
) -> TokenKind {
    let byte = bytes[*cursor];
    match byte {
        b'"' | b'\'' => lex_quoted(bytes, cursor, byte),
        b'`' => lex_template(bytes, cursor),
        b'0'..=b'9' => {
            lex_number(bytes, cursor);
            TokenKind::Number
        }
        b'.' if matches!(bytes.get(*cursor + 1), Some(b'0'..=b'9')) => {
            lex_number(bytes, cursor);
            TokenKind::Number
        }
        b'/' if regex_allowed(prev, source) => {
            if lex_regex(bytes, cursor) {
                TokenKind::Regex
            } else {
                *cursor += 1;
                TokenKind::Punct(b'/')
            }
        }
        b'=' if bytes.get(*cursor + 1) == Some(&b'>') => {
            *cursor += 2;
            TokenKind::Arrow
        }
        _ if is_ident_start(byte) => {
            *cursor += 1;
            while *cursor < bytes.len() && is_ident_part(bytes[*cursor]) {
                *cursor += 1;
            }
            TokenKind::Ident
        }
        _ => {
            *cursor += 1;
            TokenKind::Punct(byte)
        }
    }
}

fn lex_quoted(bytes: &[u8], cursor: &mut usize, quote: u8) -> TokenKind {
    let mut at = *cursor + 1;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if byte == quote {
            *cursor = at + 1;
            return TokenKind::String;
        }
        if line_terminator_width(bytes, at).is_some() {
            break;
        }
        at += 1;
    }
    *cursor = at.min(bytes.len());
    TokenKind::Invalid
}

fn lex_template(bytes: &[u8], cursor: &mut usize) -> TokenKind {
    let mut at = *cursor + 1;
    let mut depth = 0usize;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if depth == 0 && byte == b'`' {
            *cursor = at + 1;
            return TokenKind::Template;
        }
        if byte == b'$' && bytes.get(at + 1) == Some(&b'{') {
            depth += 1;
            at += 2;
            continue;
        }
        if depth > 0 && byte == b'}' {
            depth -= 1;
        }
        at += 1;
    }
    *cursor = bytes.len();
    TokenKind::Invalid
}

fn lex_number(bytes: &[u8], cursor: &mut usize) {
    let mut at = *cursor;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
            at += 1;
            continue;
        }
        if (byte == b'+' || byte == b'-') && at > *cursor {
            let previous = bytes[at - 1];
            if previous == b'e' || previous == b'E' {
                at += 1;
                continue;
            }
        }
        break;
    }
    *cursor = at;
}

/// Consume a regular-expression literal, reporting whether one was found.
fn lex_regex(bytes: &[u8], cursor: &mut usize) -> bool {
    let mut at = *cursor + 1;
    let mut in_class = false;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if line_terminator_width(bytes, at).is_some() {
            return false;
        }
        match byte {
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => {
                at += 1;
                while at < bytes.len() && is_ident_part(bytes[at]) {
                    at += 1;
                }
                *cursor = at;
                return true;
            }
            _ => {}
        }
        at += 1;
    }
    false
}

/// Whether a `/` here starts a regular expression rather than a division.
///
/// A `<` before the slash is excluded so `</div>` in JSX is never lexed as the
/// start of a regular expression, which would swallow the rest of the line.
fn regex_allowed(prev: Option<&Token>, source: &str) -> bool {
    let Some(prev) = prev else {
        return true;
    };
    match prev.kind {
        TokenKind::Arrow => true,
        TokenKind::Ident => matches!(
            prev.text(source),
            "await"
                | "case"
                | "delete"
                | "do"
                | "else"
                | "in"
                | "instanceof"
                | "new"
                | "of"
                | "return"
                | "throw"
                | "typeof"
                | "void"
                | "yield"
        ),
        TokenKind::Punct(byte) => !matches!(byte, b')' | b']' | b'}' | b'<'),
        _ => false,
    }
}

pub(super) fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

pub(super) fn is_ident_part(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}
