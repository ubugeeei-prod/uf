//! Normalizing string delimiters without ever adding escaping.
//!
//! A literal is rewritten into the configured quote style only when the result
//! needs no more backslashes than the spelling the author chose, so
//! `'say \"hi\"'` keeps its single quotes even under a double-quote config.

use uf_config::QuoteStyle;

/// Rewrite a string literal into the configured quote style, but only when that
/// does not require more escaping than the original spelling.
pub(crate) fn requote(text: &str, style: QuoteStyle) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote != b'\'' && quote != b'"') || bytes[bytes.len() - 1] != quote {
        return None;
    }

    let body = &text[1..text.len() - 1];
    let (mut doubles, mut singles) = (0usize, 0usize);
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some('"') => doubles += 1,
                Some('\'') => singles += 1,
                _ => {}
            },
            '"' => doubles += 1,
            '\'' => singles += 1,
            _ => {}
        }
    }

    let chosen = match style {
        QuoteStyle::Double if doubles > singles => '\'',
        QuoteStyle::Double => '"',
        QuoteStyle::Single if singles > doubles => '"',
        QuoteStyle::Single => '\'',
    };
    if chosen as u8 == quote {
        return None;
    }

    let mut rewritten = String::with_capacity(text.len() + 4);
    rewritten.push(chosen);
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                None => rewritten.push('\\'),
                Some(next) => {
                    // An escaped quote that is no longer the delimiter can drop
                    // its backslash; every other escape is copied verbatim.
                    if (next == '"' || next == '\'') && next != chosen {
                        rewritten.push(next);
                    } else {
                        rewritten.push('\\');
                        rewritten.push(next);
                    }
                }
            }
        } else if ch == chosen {
            rewritten.push('\\');
            rewritten.push(chosen);
        } else {
            rewritten.push(ch);
        }
    }
    rewritten.push(chosen);
    Some(rewritten)
}

/// JSX attribute strings have no escape sequences, so they may only be requoted
/// when the body does not contain the target quote at all.
pub(crate) fn requote_jsx(text: &str, style: QuoteStyle) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote != b'\'' && quote != b'"') || bytes[bytes.len() - 1] != quote {
        return None;
    }
    let target = match style {
        QuoteStyle::Double => b'"',
        QuoteStyle::Single => b'\'',
    };
    if target == quote {
        return None;
    }
    let body = &text[1..text.len() - 1];
    if body.as_bytes().contains(&target) {
        return None;
    }
    let mut rewritten = String::with_capacity(text.len());
    rewritten.push(char::from(target));
    rewritten.push_str(body);
    rewritten.push(char::from(target));
    Some(rewritten)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quotes_become_double_quotes() {
        assert_eq!(requote("'a'", QuoteStyle::Double).as_deref(), Some("\"a\""));
    }

    #[test]
    fn already_preferred_quotes_are_left_alone() {
        assert_eq!(requote("\"a\"", QuoteStyle::Double), None);
        assert_eq!(requote("'a'", QuoteStyle::Single), None);
    }

    #[test]
    fn a_string_full_of_double_quotes_keeps_single_quotes() {
        assert_eq!(requote("'say \"hi\"'", QuoteStyle::Double), None);
    }

    #[test]
    fn converting_drops_now_redundant_escapes() {
        assert_eq!(
            requote("'it\\'s'", QuoteStyle::Double).as_deref(),
            Some("\"it's\"")
        );
    }

    #[test]
    fn converting_adds_escapes_only_when_it_does_not_lose_ground() {
        // One of each: the preferred quote wins and the escape count is unchanged.
        assert_eq!(
            requote("'a\"b\\'c'", QuoteStyle::Double).as_deref(),
            Some("\"a\\\"b'c\"")
        );
    }

    #[test]
    fn other_escapes_survive_requoting() {
        assert_eq!(
            requote("'a\\nb\\u0041\\\\'", QuoteStyle::Double).as_deref(),
            Some("\"a\\nb\\u0041\\\\\"")
        );
    }

    #[test]
    fn line_continuations_survive_requoting() {
        assert_eq!(
            requote("'a\\\nb'", QuoteStyle::Double).as_deref(),
            Some("\"a\\\nb\"")
        );
    }

    #[test]
    fn requoting_ignores_malformed_literals() {
        assert_eq!(requote("'", QuoteStyle::Double), None);
        assert_eq!(requote("'abc", QuoteStyle::Double), None);
        assert_eq!(requote("", QuoteStyle::Double), None);
    }

    #[test]
    fn jsx_strings_are_requoted_only_when_no_escape_would_be_needed() {
        assert_eq!(
            requote_jsx("'a'", QuoteStyle::Double).as_deref(),
            Some("\"a\"")
        );
        assert_eq!(requote_jsx("'a\"b'", QuoteStyle::Double), None);
    }
}
