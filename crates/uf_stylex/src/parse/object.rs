//! Reading one level of an object literal, and one declaration value.
//!
//! Both are flat: [`entries`] walks a single level and hands back the token
//! span of every value, and the caller decides whether to walk into a span. No
//! function here calls itself, so `{{{{…}}}}` from a dependency's `.stylex.js`
//! costs a bounded number of iterations rather than a bounded number of native
//! stack frames.

use compact_str::CompactString;
use uf_infra::LineIndex;
use uf_rsc::{Token, TokenKind, matching_close};

use crate::class::variable_name;
use crate::error::{MAX_VALUE_BYTES, SourcePosition, StyleXError};
use crate::parse::bindings::ModuleBindings;
use crate::value::{StyleValue, check_number, check_value_text};

/// Everything a walk needs to turn a token index into a position or a string.
#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    /// The module's source text.
    pub source: &'a str,
    /// Its token vector.
    pub tokens: &'a [Token],
    /// Its line index, for reporting positions.
    pub lines: &'a LineIndex,
}

impl<'a> Cursor<'a> {
    /// The position of the token at `index`, or of the end of the module.
    pub fn position(&self, index: usize) -> SourcePosition {
        let offset = self
            .tokens
            .get(index)
            .map_or(self.source.len(), |token| token.start);
        SourcePosition::new(self.lines, offset)
    }

    /// The text of the token at `index`.
    pub fn text(&self, index: usize) -> &'a str {
        self.tokens
            .get(index)
            .map_or("", |token| token.text(self.source))
    }
}

/// One `key: value` pair at one level of an object literal.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The key, unquoted.
    pub key: CompactString,
    /// Where the key was written.
    pub at: SourcePosition,
    /// Token index of the first token of the value.
    pub value_start: usize,
    /// Token index one past the last token of the value.
    pub value_end: usize,
}

/// Read every `key: value` pair at the top level of the object opening at `open`.
///
/// `open` must be the index of the `{`; the matching `}` is found here so a
/// caller never has to track nesting itself.
pub fn entries(cursor: Cursor<'_>, open: usize) -> Result<Vec<Entry>, StyleXError> {
    let Some(close) = matching_close(cursor.tokens, open, b'{', b'}') else {
        return Err(StyleXError::UnterminatedObject {
            at: cursor.position(open),
        });
    };

    let mut found = Vec::new();
    let mut at = open + 1;
    while at < close {
        if cursor.tokens[at].is_punct(b',') {
            at += 1;
            continue;
        }

        let key_at = cursor.position(at);
        let key = match cursor.tokens[at].kind {
            TokenKind::Ident | TokenKind::Number => CompactString::new(cursor.text(at)),
            TokenKind::String => {
                CompactString::new(cursor.tokens[at].quoted_content(cursor.source))
            }
            // A computed key, a spread, a method, or a shorthand property: none
            // of them can be resolved to a constant declaration.
            _ => return Err(StyleXError::MalformedEntry { at: key_at }),
        };

        at += 1;
        if !cursor
            .tokens
            .get(at)
            .is_some_and(|token| token.is_punct(b':'))
        {
            return Err(StyleXError::MalformedEntry { at: key_at });
        }
        at += 1;

        let value_start = at;
        let value_end = value_span_end(cursor, at, close)?;
        if value_end == value_start {
            return Err(StyleXError::MalformedEntry { at: key_at });
        }
        found.push(Entry {
            key,
            at: key_at,
            value_start,
            value_end,
        });
        at = value_end;
    }

    Ok(found)
}

/// Token index one past the end of the value starting at `start`.
///
/// Stops at the first `,` that is not inside a nested group, or at `close`.
fn value_span_end(cursor: Cursor<'_>, start: usize, close: usize) -> Result<usize, StyleXError> {
    let mut depth = 0i32;
    let mut at = start;
    while at < close {
        match cursor.tokens[at].kind {
            TokenKind::Punct(b'{' | b'[' | b'(') => depth += 1,
            TokenKind::Punct(b'}' | b']' | b')') => {
                depth -= 1;
                if depth < 0 {
                    return Err(StyleXError::UnterminatedObject {
                        at: cursor.position(start),
                    });
                }
            }
            TokenKind::Punct(b',') if depth == 0 => return Ok(at),
            _ => {}
        }
        at += 1;
    }
    if depth != 0 {
        return Err(StyleXError::UnterminatedObject {
            at: cursor.position(start),
        });
    }
    Ok(close)
}

/// Whether a value span is an object literal, and where it opens.
pub fn object_at(cursor: Cursor<'_>, start: usize, end: usize) -> Option<usize> {
    (end > start && cursor.tokens[start].is_punct(b'{')).then_some(start)
}

/// Resolve a value span to a constant, or say why it is not one.
pub fn value(
    cursor: Cursor<'_>,
    bindings: &ModuleBindings,
    start: usize,
    end: usize,
) -> Result<StyleValue, StyleXError> {
    let at = cursor.position(start);
    match end - start {
        1 => scalar(cursor, start, at),
        2 if cursor.tokens[start].is_punct(b'-')
            && cursor.tokens[start + 1].kind == TokenKind::Number =>
        {
            let mut text = CompactString::const_new("-");
            text.push_str(cursor.text(start + 1));
            check_number(&text, at)?;
            Ok(StyleValue::Number(text))
        }
        3 if cursor.tokens[start].kind == TokenKind::Ident
            && cursor.tokens[start + 1].is_punct(b'.')
            && cursor.tokens[start + 2].kind == TokenKind::Ident =>
        {
            let binding = cursor.text(start);
            let Some(namespace) = bindings.variables_namespace(binding) else {
                return Err(StyleXError::UnknownVariableBinding {
                    at,
                    binding: CompactString::new(binding),
                });
            };
            Ok(StyleValue::Variable(variable_name(
                namespace,
                cursor.text(start + 2),
            )))
        }
        _ => Err(unsupported(cursor, start, end, at)),
    }
}

/// Resolve a one-token value.
fn scalar(cursor: Cursor<'_>, start: usize, at: SourcePosition) -> Result<StyleValue, StyleXError> {
    let token = &cursor.tokens[start];
    match token.kind {
        TokenKind::String => {
            let text = token.quoted_content(cursor.source);
            check_value_text(text, at)?;
            Ok(StyleValue::Text(CompactString::new(text)))
        }
        TokenKind::Template => {
            let text = token.quoted_content(cursor.source);
            if text.contains("${") {
                return Err(StyleXError::UnsupportedValue {
                    at,
                    value: truncate(token.text(cursor.source)),
                });
            }
            check_value_text(text, at)?;
            Ok(StyleValue::Text(CompactString::new(text)))
        }
        TokenKind::Number => {
            let text = token.text(cursor.source);
            check_number(text, at)?;
            Ok(StyleValue::Number(CompactString::new(text)))
        }
        _ => Err(StyleXError::UnsupportedValue {
            at,
            value: truncate(token.text(cursor.source)),
        }),
    }
}

/// The error for a value span uf cannot resolve, quoting what was written.
fn unsupported(cursor: Cursor<'_>, start: usize, end: usize, at: SourcePosition) -> StyleXError {
    let text = match (
        cursor.tokens.get(start),
        end.checked_sub(1).and_then(|last| cursor.tokens.get(last)),
    ) {
        (Some(first), Some(last)) => cursor.source.get(first.start..last.end).unwrap_or_default(),
        _ => "",
    };
    StyleXError::UnsupportedValue {
        at,
        value: truncate(text),
    }
}

/// Keep an error's quoted text inside the value ceiling.
fn truncate(text: &str) -> CompactString {
    let mut end = text.len().min(MAX_VALUE_BYTES);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    CompactString::new(&text[..end])
}
