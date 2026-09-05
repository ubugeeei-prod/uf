//! Byte-level search over a line of code.
//!
//! Rules look for words, not substrings: `anywhere` must not answer a search for
//! `any`. These helpers keep the word-boundary test next to each search so that
//! no rule has to remember to add it.

/// Whether `byte` can appear inside a JavaScript identifier.
#[inline]
pub(crate) fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Whether the byte at `at` starts a word (nothing word-ish precedes it).
#[inline]
pub(crate) fn starts_word(haystack: &str, at: usize) -> bool {
    at == 0 || !is_word_byte(haystack.as_bytes()[at - 1])
}

/// Whether `at + len` ends a word (nothing word-ish follows it).
#[inline]
pub(crate) fn ends_word(haystack: &str, end: usize) -> bool {
    haystack
        .as_bytes()
        .get(end)
        .is_none_or(|&byte| !is_word_byte(byte))
}

/// Byte offsets of every occurrence of `needle` in `haystack`.
///
/// Uses `memchr` on the needle's first byte rather than a naive character walk.
pub(crate) fn find_all<'a>(haystack: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    let first = needle.as_bytes().first().copied();
    let bytes = haystack.as_bytes();
    first
        .into_iter()
        .flat_map(move |first| uf_infra::memchr_iter(first, bytes))
        .filter(move |&at| haystack.as_bytes()[at..].starts_with(needle.as_bytes()))
}

/// Byte offsets of every standalone-word occurrence of `needle` in `haystack`.
pub(crate) fn find_words<'a>(
    haystack: &'a str,
    needle: &'a str,
) -> impl Iterator<Item = usize> + 'a {
    find_all(haystack, needle)
        .filter(move |&at| starts_word(haystack, at) && ends_word(haystack, at + needle.len()))
}

/// Whether the byte at `at` stands inside a string literal.
///
/// [`crate::scan::Line::code`] keeps string literals on purpose —
/// `uniflowed/no-npm-script-invocation` exists to read them — so a rule about
/// what the code *does* has to ask before it fires. `const message = "no
/// globalThis.fetch here"` overrides nothing, and `it("treats Object as any
/// non-null object", …)` annotates nothing.
///
/// One line at a time, like every rule that reads a `Line`: a template literal
/// that spans lines is understood as far as this line goes. A `${…}`
/// substitution counts as string rather than as code, which errs towards
/// silence — the direction a rule should err when it cannot tell.
pub(crate) fn in_string(haystack: &str, at: usize) -> bool {
    let bytes = haystack.as_bytes();
    let end = at.min(bytes.len());
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < end {
        let byte = bytes[index];
        match quote {
            Some(open) => {
                if byte == b'\\' {
                    index += 2;
                    continue;
                }
                if byte == open {
                    quote = None;
                }
            }
            None if matches!(byte, b'"' | b'\'' | b'`') => quote = Some(byte),
            None => {}
        }
        index += 1;
    }
    quote.is_some()
}

/// The next non-whitespace byte at or after `from`.
#[inline]
pub(crate) fn next_non_space(haystack: &str, from: usize) -> Option<(usize, u8)> {
    haystack.as_bytes()[from.min(haystack.len())..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|offset| (from + offset, haystack.as_bytes()[from + offset]))
}

/// The last non-whitespace byte strictly before `before`.
#[inline]
pub(crate) fn prev_non_space(haystack: &str, before: usize) -> Option<(usize, u8)> {
    haystack.as_bytes()[..before.min(haystack.len())]
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|at| (at, haystack.as_bytes()[at]))
}

/// The identifier immediately before `before`, skipping whitespace.
///
/// Returns its start offset and text, or `None` when the preceding token is not
/// an identifier.
#[inline]
pub(crate) fn previous_word(haystack: &str, before: usize) -> Option<(usize, &str)> {
    let (end, byte) = prev_non_space(haystack, before)?;
    if !is_word_byte(byte) {
        return None;
    }
    let bytes = haystack.as_bytes();
    let mut start = end;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    Some((start, &haystack[start..=end]))
}

/// Length of the identifier starting at `from`, or zero if none starts there.
#[inline]
pub(crate) fn identifier_len(haystack: &str, from: usize) -> usize {
    let bytes = haystack.as_bytes();
    if from >= bytes.len() || bytes[from].is_ascii_digit() || !is_word_byte(bytes[from]) {
        return 0;
    }
    let mut end = from;
    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }
    end - from
}

/// Whether `name` follows React's `useSomething` hook naming convention.
#[inline]
pub(crate) fn is_hook_name(name: &str) -> bool {
    name.len() > 3 && name.starts_with("use") && name.as_bytes()[3].is_ascii_uppercase()
}
