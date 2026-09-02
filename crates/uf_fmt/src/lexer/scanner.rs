//! The scanner state machine: the cursor, the context stack and the dispatch
//! that decides which routine reads the next token.
//!
//! The nesting contexts live in an explicit [`Vec`] rather than in the call
//! stack, so a source nested ten thousand delimiters deep is scanned iteratively
//! and cannot overflow.

use super::context::{BraceKind, GroupKind, Prev, classify_brace, expression_allowed};
use super::keyword::Keyword;
use super::punctuator::Punctuator;
use super::token::{Span, Token, TokenKind};

/// Nesting contexts the scanner tracks with an explicit stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ctx {
    Paren,
    Bracket,
    Brace(BraceKind),
    /// A `${` interpolation inside a template literal.
    TemplateExpression,
    /// Inside `< … >` of a JSX tag.
    JsxTag {
        closing: bool,
    },
    /// Between a JSX opening tag and its closing tag.
    JsxChildren,
}

/// The scanner itself.
pub(crate) struct Lexer<'a> {
    pub(crate) source: &'a str,
    pub(crate) bytes: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) stack: Vec<Ctx>,
    tokens: Vec<Token>,
    pub(crate) prev: Option<Prev>,
    /// Set by `class`/`interface`/`enum`/`switch` so the matching `{` is
    /// classified as a body rather than an object literal.
    pub(crate) pending_body: Option<BraceKind>,
    /// The keyword a statement started with, used to keep `type X = <T>(…) => T`
    /// from being scanned as JSX.
    statement_head: Option<Keyword>,
    /// Whether the innermost `(` opened a statement header.
    paren_headers: Vec<bool>,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        // One token per ~4 bytes is a good first guess for real sources and keeps
        // the token vector from repeatedly reallocating on large files.
        let capacity = source.len() / 4 + 16;
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            stack: Vec::with_capacity(16),
            tokens: Vec::with_capacity(capacity),
            prev: None,
            pending_body: None,
            statement_head: None,
            paren_headers: Vec::with_capacity(16),
        }
    }

    pub(crate) fn run(mut self) -> Vec<Token> {
        if self.bytes.starts_with(b"#!") {
            let end = self.line_end_from(2);
            self.push(TokenKind::Shebang, 0, end);
            self.pos = end;
        }

        while self.pos < self.bytes.len() {
            match self.stack.last() {
                Some(Ctx::JsxTag { .. }) => self.scan_jsx_tag(),
                Some(Ctx::JsxChildren) => self.scan_jsx_child(),
                _ => self.scan_normal(),
            }
        }

        self.tokens
    }

    pub(crate) fn byte(&self, at: usize) -> u8 {
        // Out of range reads report NUL, which is never a meaningful delimiter.
        self.bytes.get(at).copied().unwrap_or(0)
    }

    pub(crate) fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        debug_assert!(end > start, "every token must cover at least one byte");
        self.tokens.push(Token {
            kind,
            span: Span { start, end },
        });
        if !kind.is_trivia() {
            self.record_prev(kind, GroupKind::None);
        }
    }

    fn push_with_group(&mut self, kind: TokenKind, start: usize, end: usize, group: GroupKind) {
        self.tokens.push(Token {
            kind,
            span: Span { start, end },
        });
        self.record_prev(kind, group);
    }

    fn record_prev(&mut self, kind: TokenKind, group: GroupKind) {
        // Track the keyword that opened the current statement. Statements begin
        // after `;`, `{`, `}` or at the start of the file.
        let boundary = matches!(
            self.prev.map(|prev| prev.kind),
            None | Some(TokenKind::Punctuator(
                Punctuator::Semicolon | Punctuator::OpenBrace | Punctuator::CloseBrace
            ))
        );
        if boundary {
            self.statement_head = match kind {
                TokenKind::Keyword(keyword) => Some(keyword),
                _ => None,
            };
        }
        self.prev = Some(Prev { kind, group });
    }

    pub(crate) fn line_end_from(&self, from: usize) -> usize {
        match memchr::memchr(b'\n', &self.bytes[from..]) {
            Some(offset) => from + offset,
            None => self.bytes.len(),
        }
    }

    fn enclosing_brace(&self) -> Option<BraceKind> {
        self.stack.iter().rev().find_map(|ctx| match ctx {
            Ctx::Brace(kind) => Some(*kind),
            _ => None,
        })
    }

    // ---------------------------------------------------------------- normal

    fn scan_normal(&mut self) {
        let start = self.pos;
        let byte = self.byte(start);

        match byte {
            b'\n' => {
                self.pos = start + 1;
                self.push(TokenKind::Newline, start, self.pos);
            }
            b'\r' => {
                // Defensive: `format_source` normalizes line endings first, but the
                // lexer is public API and must handle raw CRLF on its own.
                self.pos = if self.byte(start + 1) == b'\n' {
                    start + 2
                } else {
                    start + 1
                };
                self.push(TokenKind::Newline, start, self.pos);
            }
            b' ' | b'\t' | 0x0b | 0x0c => {
                let mut end = start + 1;
                while matches!(self.byte(end), b' ' | b'\t' | 0x0b | 0x0c) {
                    end += 1;
                }
                self.pos = end;
                self.push(TokenKind::Whitespace, start, end);
            }
            b'/' => self.scan_slash(),
            b'\'' | b'"' => self.scan_string(byte),
            b'`' => self.scan_template_from(start, true),
            b'0'..=b'9' => self.scan_number(),
            b'#' => {
                let mut end = start + 1;
                while let Some(len) = self.ident_char_len(end, end == start + 1) {
                    end += len;
                }
                self.pos = end;
                if end == start + 1 {
                    self.push(TokenKind::Unknown, start, end);
                } else {
                    self.push(TokenKind::PrivateName, start, end);
                }
            }
            b'<' if self.jsx_allowed() => {
                let closing = self.byte(start + 1) == b'/';
                if closing {
                    self.pos = start + 2;
                    self.stack.push(Ctx::JsxTag { closing: true });
                    self.push(TokenKind::JsxCloseStart, start, self.pos);
                } else {
                    self.pos = start + 1;
                    self.stack.push(Ctx::JsxTag { closing: false });
                    self.push(TokenKind::JsxOpenStart, start, self.pos);
                }
            }
            b'}' if matches!(self.stack.last(), Some(Ctx::TemplateExpression)) => {
                self.stack.pop();
                self.scan_template_from(start, false);
            }
            _ => {
                if self.ident_char_len(start, true).is_some() {
                    self.scan_identifier();
                } else if byte == b'.' && self.byte(start + 1).is_ascii_digit() {
                    self.scan_number();
                } else if let Some((punctuator, len)) = self.scan_punctuator(start) {
                    self.pos = start + len;
                    self.push_punctuator(punctuator, start, self.pos);
                } else {
                    let len = self.char_len(start);
                    self.pos = start + len;
                    self.push(TokenKind::Unknown, start, self.pos);
                }
            }
        }
    }

    pub(crate) fn push_punctuator(&mut self, punctuator: Punctuator, start: usize, end: usize) {
        let kind = TokenKind::Punctuator(punctuator);
        match punctuator {
            Punctuator::OpenParen => {
                let header = matches!(
                    self.prev.map(|prev| prev.kind),
                    Some(TokenKind::Keyword(keyword)) if keyword.starts_statement_header()
                );
                self.paren_headers.push(header);
                self.stack.push(Ctx::Paren);
                self.push(kind, start, end);
            }
            Punctuator::CloseParen => {
                if matches!(self.stack.last(), Some(Ctx::Paren)) {
                    self.stack.pop();
                }
                let header = self.paren_headers.pop().unwrap_or(false);
                let group = if header {
                    GroupKind::StatementParen
                } else {
                    GroupKind::ExpressionParen
                };
                self.push_with_group(kind, start, end, group);
            }
            Punctuator::OpenBracket => {
                self.stack.push(Ctx::Bracket);
                self.push(kind, start, end);
            }
            Punctuator::CloseBracket => {
                if matches!(self.stack.last(), Some(Ctx::Bracket)) {
                    self.stack.pop();
                }
                self.push(kind, start, end);
            }
            Punctuator::OpenBrace => {
                let brace = match self.stack.last() {
                    Some(Ctx::JsxTag { .. } | Ctx::JsxChildren) => BraceKind::JsxExpression,
                    _ => self
                        .pending_body
                        .take()
                        .unwrap_or_else(|| classify_brace(self.prev, self.enclosing_brace())),
                };
                self.stack.push(Ctx::Brace(brace));
                self.push(kind, start, end);
            }
            Punctuator::CloseBrace => {
                let brace = match self.stack.last() {
                    Some(Ctx::Brace(brace)) => {
                        let brace = *brace;
                        self.stack.pop();
                        brace
                    }
                    _ => BraceKind::Block,
                };
                self.push_with_group(kind, start, end, GroupKind::Brace(brace));
            }
            _ => self.push(kind, start, end),
        }
    }

    fn jsx_allowed(&self) -> bool {
        if !expression_allowed(self.prev) {
            return false;
        }
        // `type Handler = <T>(value: T) => T` is a generic function type, not a
        // JSX element, even though an expression could otherwise start here.
        !matches!(self.statement_head, Some(keyword) if keyword.starts_type_declaration())
    }

    /// Byte length of the UTF-8 character starting at `at`, or 1 past the end.
    pub(crate) fn char_len(&self, at: usize) -> usize {
        match self.bytes.get(at) {
            None => 1,
            Some(byte) if *byte < 0x80 => 1,
            Some(_) => self.source[at..].chars().next().map_or(1, char::len_utf8),
        }
    }

    /// Byte length of the identifier character at `at`, if there is one.
    pub(crate) fn ident_char_len(&self, at: usize, start: bool) -> Option<usize> {
        let byte = *self.bytes.get(at)?;
        if byte < 0x80 {
            let ok = byte == b'_'
                || byte == b'$'
                || byte.is_ascii_alphabetic()
                || (!start && byte.is_ascii_digit());
            return ok.then_some(1);
        }

        let ch = self.source[at..].chars().next()?;
        let ok = if start {
            ch.is_alphabetic()
        } else {
            // Approximates ID_Continue: letters, marks and digits, plus the two
            // zero-width joiners the spec allows inside identifiers.
            ch.is_alphanumeric() || ch == '\u{200c}' || ch == '\u{200d}'
        };
        ok.then(|| ch.len_utf8())
    }
}
