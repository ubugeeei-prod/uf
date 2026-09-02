//! Byte-level token scanning for Flow and JavaScript source.
//!
//! Anything that rewrites source text needs to know where strings, template
//! literals, comments and regular expressions begin and end — a `":"` inside a
//! string is not a type annotation, and `/* type X = number */` is not a
//! declaration. [`strip`](crate::strip) needs exactly that, and so does a
//! bundler rewriting `import` statements, so the scanner is public rather than
//! private to the eraser: there is one token scanner for uf source, and it
//! lives in the crate that owns the syntax.
//!
//! This is deliberately a lexer and not a parser. It never decides what a
//! construct *means*, only where its bytes are, and the Flow grammar stays the
//! business of [`validate_source`](crate::validate_source). Angle brackets come
//! out as ordinary punctuation so type parameters can be matched by a caller
//! that wants them.
//!
//! The scanner fails open. Anything it cannot lex confidently becomes
//! [`TokenKind::Invalid`], and callers leave such regions alone rather than
//! rewriting bytes nobody understood.
//!
//! # Two entry points, one scanner
//!
//! JSX is a *lexical mode*, not extra grammar: between `>` and `</` the same
//! bytes that are operators in JavaScript are text, and `it's` is an
//! apostrophe rather than the start of a string. So there are two entry points
//! over one implementation.
//!
//! [`tokenize`] reads plain JavaScript and Flow. It is what
//! [`strip`](crate::strip) uses, because Flow's `<T>` type parameters and
//! JSX's `<div>` are the same two bytes and only one of those readings can be
//! right for a source that still has types in it.
//!
//! [`tokenize_jsx`] adds the JSX modes, and is for source whose Flow types are
//! already gone. It emits [`TokenKind::JsxText`] for the text between tags, so
//! a caller can tell "this file still contains JSX" from the token stream
//! rather than by searching for a `<`.

/// What a token is, as far as byte scanning can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// An identifier, a keyword, or a Flow contextual keyword.
    Ident,
    /// A single- or double-quoted string literal.
    String,
    /// A template literal, including its substitutions.
    Template,
    /// A numeric literal.
    Number,
    /// A regular-expression literal.
    Regex,
    /// `=>`, lexed as one token so it is never seen as `=` then `>`.
    Arrow,
    /// A single punctuation byte.
    Punct(u8),
    /// A run of JSX text between two tags, newlines and all.
    ///
    /// Only [`tokenize_jsx`] ever produces one.
    JsxText,
    /// The `<` that opens a JSX tag, closing tags and fragments included.
    ///
    /// Distinct from `Punct(b'<')` so a caller never has to re-derive the
    /// "element or comparison" decision the scanner already made.
    JsxTagOpen,
    /// The `>` that closes a JSX tag.
    JsxTagClose,
    /// An unterminated string, template or comment.
    Invalid,
}

impl TokenKind {
    /// Whether this kind only ever appears in JSX.
    ///
    /// A token stream with none of these holds no JSX, which is how a build
    /// checks that its output is JavaScript rather than by searching for a
    /// `<` that might be a comparison.
    #[must_use]
    pub const fn is_jsx(self) -> bool {
        matches!(self, Self::JsxText | Self::JsxTagOpen | Self::JsxTagClose)
    }
}

/// One token, with the byte range it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Byte offset of the first byte of the token.
    pub start: usize,
    /// Byte offset one past the last byte of the token.
    pub end: usize,
    /// Whether a line terminator separates this token from the previous one.
    pub newline_before: bool,
}

impl Token {
    /// The token's text, borrowed from the source it was scanned from.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        source.get(self.start..self.end).unwrap_or_default()
    }

    /// Whether the token is the single punctuation byte `byte`.
    pub fn is_punct(&self, byte: u8) -> bool {
        self.kind == TokenKind::Punct(byte)
    }

    /// Whether the token is the identifier `name`.
    pub fn is_ident(&self, source: &str, name: &str) -> bool {
        self.kind == TokenKind::Ident && self.text(source) == name
    }
}

mod jsx;
mod lexer;

#[cfg(test)]
mod tests;

use jsx::{Frame, JsxMode, lex_children, lex_tag, step_frames};
use lexer::{
    block_comment_end, lex_token, line_end, line_terminator_width, skip_bom, whitespace_width,
};

/// Tokenize a module as JavaScript and Flow, skipping a BOM and a `#!` line.
///
/// `<` and `>` come out as ordinary punctuation, which is what a caller
/// matching Flow type parameters needs. Use [`tokenize_jsx`] for source whose
/// angle brackets are elements rather than types.
pub fn tokenize(source: &str) -> Vec<Token> {
    scan(source, JsxMode::Off)
}

/// Tokenize a module that may contain JSX.
///
/// Text between tags becomes [`TokenKind::JsxText`] rather than being lexed as
/// JavaScript, so an apostrophe in `it's` is text and not the start of a
/// string literal. Everything inside `{ … }` is lexed as JavaScript again, to
/// any depth.
///
/// Whether a `<` opens an element or is a comparison is decided the same way
/// a `/` is decided to be a regular expression: by whether the token before it
/// can end an expression. That is a heuristic, and it is the same one every
/// tool without a full parser uses; [`tokenize`] exists precisely so that
/// callers who must not apply it do not have to.
pub fn tokenize_jsx(source: &str) -> Vec<Token> {
    scan(source, JsxMode::On)
}

fn scan(source: &str, jsx: JsxMode) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens: Vec<Token> = Vec::with_capacity(bytes.len() / 6 + 8);
    let mut cursor = skip_bom(bytes);
    let mut newline_before = false;
    let mut frames: Vec<Frame> = vec![Frame::Js { braces: 0 }];

    if bytes[cursor..].starts_with(b"#!") {
        cursor = line_end(bytes, cursor);
    }

    while cursor < bytes.len() {
        let frame = *frames.last().unwrap_or(&Frame::Js { braces: 0 });

        if frame == Frame::Children {
            let start = cursor;
            let kind = lex_children(bytes, &mut cursor, &mut frames);
            if cursor > start {
                tokens.push(Token {
                    kind,
                    start,
                    end: cursor,
                    newline_before,
                });
                newline_before = false;
            }
            continue;
        }

        let byte = bytes[cursor];

        if let Some(width) = line_terminator_width(bytes, cursor) {
            newline_before = true;
            cursor += width;
            continue;
        }
        if let Some(width) = whitespace_width(bytes, cursor) {
            cursor += width;
            continue;
        }
        if byte == b'/' && bytes.get(cursor + 1) == Some(&b'/') {
            cursor = line_end(bytes, cursor);
            continue;
        }
        if byte == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
            let (end, saw_newline) = block_comment_end(bytes, cursor);
            newline_before |= saw_newline;
            cursor = end;
            continue;
        }

        let start = cursor;
        let kind = match frame {
            Frame::Tag { .. } => lex_tag(bytes, &mut cursor),
            _ => lex_token(bytes, &mut cursor, tokens.last(), source),
        };
        tokens.push(Token {
            kind,
            start,
            end: cursor,
            newline_before,
        });
        newline_before = false;

        if jsx == JsxMode::On {
            step_frames(&mut frames, &mut tokens, source);
        }
    }

    tokens
}

/// Index of the token closing the group opened at `position`.
pub fn matching_close(tokens: &[Token], position: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    for (at, token) in tokens.iter().enumerate().skip(position) {
        if token.is_punct(open) {
            depth += 1;
        } else if token.is_punct(close) {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

/// Index of the token opening the group closed at `position`.
pub fn matching_open(tokens: &[Token], position: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = position;
    loop {
        let token = tokens.get(at)?;
        if token.is_punct(close) {
            depth += 1;
        } else if token.is_punct(open) {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
        at = at.checked_sub(1)?;
    }
}

/// Whether the token at `position` begins a statement.
///
/// A lexer cannot see blocks, so this is the conservative reading: a statement
/// begins at the start of the file and after `;`, `{` and `}`. Callers acting
/// on the answer must tolerate an object-literal key looking like a statement,
/// because to a token scanner it does.
pub fn starts_statement(tokens: &[Token], position: usize) -> bool {
    match position.checked_sub(1) {
        None => true,
        Some(previous) => {
            let token = tokens[previous];
            token.is_punct(b';') || token.is_punct(b'{') || token.is_punct(b'}')
        }
    }
}
