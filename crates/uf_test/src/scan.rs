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

/// The shape of a registration call found at `offset`.
///
/// `it(` and `it.only(` are both declarations; `it.each(` is a declaration this
/// runner cannot expand, and `page.it(` is not a declaration at all. Telling
/// those four apart is what keeps an unexpandable form from being silently
/// dropped, so the scanner reports the property name instead of just a boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallShape<'a> {
    /// `ident(`.
    Plain,
    /// `ident.property(`.
    Property {
        /// The property name as written.
        name: &'a str,
        /// Byte offset one past the property name.
        end: usize,
    },
}

pub(crate) fn call_shape_at<'a>(
    source: &'a str,
    offset: usize,
    ident: &str,
) -> Option<CallShape<'a>> {
    // A preceding identifier byte means this is the tail of a longer word
    // (`unit`, `submit`); a preceding `.` means it is somebody else's member.
    let before = source[..offset].chars().next_back();
    if before.is_some_and(|ch| is_identifier_char(ch) || ch == '.') {
        return None;
    }

    let after_ident = offset + ident.len();
    let (next, next_offset) = next_significant(source, after_ident)?;
    if next == '(' {
        return Some(CallShape::Plain);
    }
    if next != '.' {
        return None;
    }

    let (_, property_start) = next_significant(source, next_offset + 1)?;
    let property_len = identifier_len(&source[property_start..]);
    if property_len == 0 {
        return None;
    }
    let property_end = property_start + property_len;
    let (after_property, _) = next_significant(source, property_end)?;
    if after_property != '(' {
        return None;
    }

    Some(CallShape::Property {
        name: &source[property_start..property_end],
        end: property_end,
    })
}

/// The next non-whitespace character at or after `from`, with its offset.
fn next_significant(source: &str, from: usize) -> Option<(char, usize)> {
    source
        .get(from..)?
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(relative, ch)| (ch, from + relative))
}

/// Length in bytes of the identifier starting at the front of `source`.
fn identifier_len(source: &str) -> usize {
    source
        .char_indices()
        .find(|(_, ch)| !is_identifier_char(*ch))
        .map(|(offset, _)| offset)
        .unwrap_or(source.len())
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

/// One name a static `import` brings into a file, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImportedBinding<'a> {
    /// The name as it is written in this file.
    pub(crate) local: &'a str,
    /// The module specifier, exactly as written.
    pub(crate) module: &'a str,
}

/// Longest import statement this scanner will read, in bytes.
///
/// A bound rather than a limit anybody should reach: the point is that a stray
/// `import` the mask did not exclude cannot make the scan walk the rest of a
/// generated file looking for a specifier that is not there.
const MAX_IMPORT_BYTES: usize = 4096;

/// Every value a file's static imports bind, with the module each came from.
///
/// This is what lets discovery tell `import { test } from "@uniflowed/test"`
/// from `import test from "node:test"` — the same call, in the same shape, and
/// only one of them is a declaration this runner can execute. See
/// [`crate::discovery`].
///
/// Static imports only, and deliberately: `import(…)` is a promise whose
/// binding is decided at run time, and `require` is not a form these files
/// take. A name this misses is a name discovery treats as it always did, which
/// is the safe direction — the run-time check in [`crate::runner`] is what
/// catches the rest.
///
/// Type imports are skipped: `import type { Test } from "…"` binds nothing a
/// call can reach, and treating it as a value binding would make a file that
/// imports a *type* called `test` stop declaring tests.
pub(crate) fn value_imports<'a>(source: &'a str, mask: &[bool]) -> Vec<ImportedBinding<'a>> {
    const KEYWORD: &str = "import";
    let mut bindings = Vec::new();
    let mut search_start = 0;
    while let Some(relative) = source[search_start..].find(KEYWORD) {
        let offset = search_start + relative;
        search_start = offset + KEYWORD.len();

        if !mask.get(offset).copied().unwrap_or(false) {
            continue;
        }
        // The tail of a longer word (`reimport`), or somebody's property.
        let before = source[..offset].chars().next_back();
        if before.is_some_and(|ch| is_identifier_char(ch) || ch == '.') {
            continue;
        }
        let Some((next, next_offset)) = next_significant(source, search_start) else {
            continue;
        };
        // `import(…)` is dynamic and `import.meta` is not an import at all.
        // `import "./side-effect.js"` binds nothing.
        if matches!(next, '(' | '.' | '"' | '\'') {
            continue;
        }
        let Some((clause, module, end)) = import_statement(source, next_offset) else {
            continue;
        };
        search_start = end;
        push_bindings(clause, module, &mut bindings);
    }
    bindings
}

/// Split one `import` statement into its clause and its module specifier.
///
/// Returns the text between the keyword and the specifier, the specifier
/// itself, and the offset one past the closing quote. A statement whose
/// specifier is not found within [`MAX_IMPORT_BYTES`] yields nothing.
fn import_statement(source: &str, from: usize) -> Option<(&str, &str, usize)> {
    let bytes = source.as_bytes();
    let end = source.len().min(from + MAX_IMPORT_BYTES);
    let mut i = from;
    while i < end {
        match bytes[i] {
            // The specifier is the only string literal a static import holds.
            quote @ (b'"' | b'\'') => {
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == quote {
                        return Some((&source[from..i], &source[start..j], j + 1));
                    }
                    j += 1;
                }
                return None;
            }
            // Nothing that can follow these belongs to an import statement, so
            // whatever this `import` was, it was not one.
            b';' | b'=' | b'(' | b'`' => return None,
            _ => i += 1,
        }
    }
    None
}

/// Record the value names one import clause binds.
fn push_bindings<'a>(clause: &'a str, module: &'a str, out: &mut Vec<ImportedBinding<'a>>) {
    let clause = clause.trim();
    // `import type …` and `import typeof …` bind types; a call cannot reach
    // one, so the file has no value by that name.
    if first_word(clause) == "type" || first_word(clause) == "typeof" {
        return;
    }
    let (head, named) = match (clause.find('{'), clause.find('}')) {
        (Some(open), Some(close)) if close > open => {
            (&clause[..open], Some(&clause[open + 1..close]))
        }
        _ => (clause, None),
    };

    // Whatever is left of the braces runs up to the `from` keyword, which is
    // part of the statement rather than a name it binds.
    let head = head.trim();
    let head = match head.strip_suffix("from") {
        Some(rest) if rest.ends_with(char::is_whitespace) => rest.trim(),
        _ => head,
    };

    // The default binding, and `* as name`, in whichever order they appear.
    for part in head.split(',') {
        let part = part.trim();
        match part.strip_prefix('*') {
            Some(rest) => {
                if let Some(local) = rest.trim().strip_prefix("as") {
                    push_local(local.trim(), module, out);
                }
            }
            None => push_local(part, module, out),
        }
    }

    for specifier in named.into_iter().flat_map(|named| named.split(',')) {
        let specifier = specifier.trim();
        if specifier.is_empty() || first_word(specifier) == "type" {
            continue;
        }
        match specifier.split_once(" as ") {
            Some((_, local)) => push_local(local.trim(), module, out),
            None => push_local(specifier, module, out),
        }
    }
}

/// Keep `local` if it is a plain identifier.
fn push_local<'a>(local: &'a str, module: &'a str, out: &mut Vec<ImportedBinding<'a>>) {
    if !local.is_empty() && local.chars().all(is_identifier_char) {
        out.push(ImportedBinding { local, module });
    }
}

/// The leading identifier of `text`, or an empty string.
fn first_word(text: &str) -> &str {
    let end = identifier_len(text);
    &text[..end]
}
