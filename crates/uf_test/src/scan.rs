//! Byte-level source scanning shared by discovery and execution.
//!
//! Neither pass parses JavaScript: they locate calls and balanced delimiters
//! directly in the source. The comment-and-string mask is what keeps a `test(`
//! inside a comment or a string literal from being mistaken for a declaration,
//! and the delimiter walk is quote-aware for the same reason.

pub(crate) fn code_byte_mask(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut mask = vec![false; bytes.len() + 1];
    let mut i = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while i < bytes.len() {
        let byte = bytes[i];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
                mask[i] = true;
            }
            i += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                i += 2;
                block_comment = false;
            } else {
                i += 1;
            }
            continue;
        }

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            i += 1;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            line_comment = true;
            i += 2;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block_comment = true;
            i += 2;
            continue;
        }

        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            i += 1;
            continue;
        }

        mask[i] = true;
        i += 1;
    }

    mask
}

pub(crate) fn extract_quoted(source: &str) -> Option<String> {
    let mut chars = source.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let mut output = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            output.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(output);
        }
        output.push(ch);
    }

    None
}

pub(crate) fn matching_delimiter(
    source: &str,
    open: usize,
    open_byte: u8,
    close_byte: u8,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&open_byte) {
        return None;
    }

    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut i = open;

    while i < bytes.len() {
        let byte = bytes[i];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            i += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            i += 1;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            line_comment = true;
            i += 2;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block_comment = true;
            i += 2;
            continue;
        }

        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            i += 1;
            continue;
        }

        if byte == open_byte {
            depth += 1;
        } else if byte == close_byte {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(i);
            }
        }

        i += 1;
    }

    None
}

pub(crate) fn is_call_at(source: &str, offset: usize, ident: &str) -> bool {
    let before = source[..offset].chars().next_back();
    if before.is_some_and(is_identifier_char) {
        return false;
    }

    let after_ident = offset + ident.len();
    let tail = &source[after_ident..];
    let Some(next) = tail.chars().find(|ch| !ch.is_whitespace()) else {
        return false;
    };

    next == '('
}

pub(crate) fn extract_first_string_arg(tail_after_ident: &str) -> Option<String> {
    let open = tail_after_ident.find('(')?;
    let tail = tail_after_ident[open + 1..].trim_start();
    let mut chars = tail.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let mut name = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            name.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(name);
        }
        name.push(ch);
    }

    None
}

pub(crate) fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
}
