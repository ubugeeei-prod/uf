//! The routines that read one literal each: comments, regular expressions,
//! strings, template chunks, numbers and identifiers.
//!
//! Each one starts at the scanner's cursor, consumes exactly the bytes the
//! literal covers, and pushes a single token -- or an [`Unterminated`] token
//! covering the rest of the input when the closing delimiter never arrives.

use super::context::{BraceKind, expression_allowed};
use super::keyword::Keyword;
use super::punctuator::Punctuator;
use super::scanner::{Ctx, Lexer};
use super::token::{TokenKind, Unterminated};

impl<'a> Lexer<'a> {
    pub(crate) fn scan_slash(&mut self) {
        let start = self.pos;
        match self.byte(start + 1) {
            b'/' => {
                let end = self.line_end_from(start + 2);
                self.pos = end;
                self.push(TokenKind::LineComment, start, end);
            }
            b'*' => {
                let doc = self.byte(start + 2) == b'*' && self.byte(start + 3) != b'/';
                let mut cursor = start + 2;
                let mut end = None;
                while cursor + 1 < self.bytes.len() {
                    match memchr::memchr(b'*', &self.bytes[cursor..]) {
                        Some(offset) => {
                            let at = cursor + offset;
                            if self.byte(at + 1) == b'/' {
                                end = Some(at + 2);
                                break;
                            }
                            cursor = at + 1;
                        }
                        None => break,
                    }
                }
                match end {
                    Some(end) => {
                        self.pos = end;
                        let kind = if doc {
                            TokenKind::DocComment
                        } else {
                            TokenKind::BlockComment
                        };
                        self.push(kind, start, end);
                    }
                    None => {
                        self.pos = self.bytes.len();
                        self.push(
                            TokenKind::Unterminated(Unterminated::BlockComment),
                            start,
                            self.pos,
                        );
                    }
                }
            }
            _ if expression_allowed(self.prev) => self.scan_regex(),
            b'=' => {
                self.pos = start + 2;
                self.push_punctuator(Punctuator::SlashEqual, start, self.pos);
            }
            _ => {
                self.pos = start + 1;
                self.push_punctuator(Punctuator::Slash, start, self.pos);
            }
        }
    }

    fn scan_regex(&mut self) {
        let start = self.pos;
        let mut cursor = start + 1;
        let mut in_class = false;
        loop {
            let byte = match self.bytes.get(cursor) {
                Some(byte) => *byte,
                None => {
                    self.pos = self.bytes.len();
                    self.push(
                        TokenKind::Unterminated(Unterminated::Regex),
                        start,
                        self.pos,
                    );
                    return;
                }
            };
            match byte {
                b'\\' => {
                    // A backslash escapes the next code unit, including `/` and `]`.
                    cursor += 1 + self.char_len(cursor + 1).max(1);
                    continue;
                }
                b'\n' | b'\r' => {
                    self.pos = cursor;
                    self.push(
                        TokenKind::Unterminated(Unterminated::Regex),
                        start,
                        self.pos,
                    );
                    return;
                }
                b'[' => in_class = true,
                b']' => in_class = false,
                b'/' if !in_class => {
                    cursor += 1;
                    break;
                }
                _ => {}
            }
            cursor += self.char_len(cursor);
        }

        while self.ident_char_len(cursor, false).is_some() {
            cursor += self.char_len(cursor);
        }
        self.pos = cursor;
        self.push(TokenKind::Regex, start, cursor);
    }

    pub(crate) fn scan_string(&mut self, quote: u8) {
        let start = self.pos;
        let mut cursor = start + 1;
        loop {
            let byte = match self.bytes.get(cursor) {
                Some(byte) => *byte,
                None => {
                    self.pos = self.bytes.len();
                    self.push(
                        TokenKind::Unterminated(Unterminated::String),
                        start,
                        self.pos,
                    );
                    return;
                }
            };
            match byte {
                b'\\' => {
                    // Line continuations and every other escape consume one char.
                    let next = cursor + 1;
                    if self.byte(next) == b'\r' && self.byte(next + 1) == b'\n' {
                        cursor = next + 2;
                    } else {
                        cursor = next + self.char_len(next).max(1);
                    }
                }
                b'\n' | b'\r' => {
                    // A raw line terminator ends an unterminated string literal.
                    self.pos = cursor;
                    self.push(
                        TokenKind::Unterminated(Unterminated::String),
                        start,
                        self.pos,
                    );
                    return;
                }
                _ if byte == quote => {
                    cursor += 1;
                    self.pos = cursor;
                    self.push(TokenKind::String, start, cursor);
                    return;
                }
                _ => cursor += self.char_len(cursor),
            }
        }
    }

    /// Scan a template chunk. `head` selects between a leading backtick and a
    /// leading `}` that resumes an interpolated template.
    pub(crate) fn scan_template_from(&mut self, start: usize, head: bool) {
        let mut cursor = start + 1;
        loop {
            let byte = match self.bytes.get(cursor) {
                Some(byte) => *byte,
                None => {
                    self.pos = self.bytes.len();
                    self.push(
                        TokenKind::Unterminated(Unterminated::Template),
                        start,
                        self.pos,
                    );
                    return;
                }
            };
            match byte {
                b'\\' => {
                    let next = cursor + 1;
                    cursor = next + self.char_len(next).max(1);
                }
                b'`' => {
                    cursor += 1;
                    self.pos = cursor;
                    let kind = if head {
                        TokenKind::TemplateFull
                    } else {
                        TokenKind::TemplateTail
                    };
                    self.push(kind, start, cursor);
                    return;
                }
                b'$' if self.byte(cursor + 1) == b'{' => {
                    cursor += 2;
                    self.pos = cursor;
                    self.stack.push(Ctx::TemplateExpression);
                    let kind = if head {
                        TokenKind::TemplateHead
                    } else {
                        TokenKind::TemplateMiddle
                    };
                    self.push(kind, start, cursor);
                    return;
                }
                _ => cursor += self.char_len(cursor),
            }
        }
    }

    pub(crate) fn scan_number(&mut self) {
        let start = self.pos;
        let mut cursor = start;

        if self.byte(cursor) == b'0' && matches!(self.byte(cursor + 1), b'x' | b'X') {
            cursor += 2;
            while self.byte(cursor).is_ascii_hexdigit() || self.byte(cursor) == b'_' {
                cursor += 1;
            }
        } else if self.byte(cursor) == b'0' && matches!(self.byte(cursor + 1), b'b' | b'B') {
            cursor += 2;
            while matches!(self.byte(cursor), b'0' | b'1' | b'_') {
                cursor += 1;
            }
        } else if self.byte(cursor) == b'0' && matches!(self.byte(cursor + 1), b'o' | b'O') {
            cursor += 2;
            while matches!(self.byte(cursor), b'0'..=b'7' | b'_') {
                cursor += 1;
            }
        } else {
            while self.byte(cursor).is_ascii_digit() || self.byte(cursor) == b'_' {
                cursor += 1;
            }
            if self.byte(cursor) == b'.' {
                cursor += 1;
                while self.byte(cursor).is_ascii_digit() || self.byte(cursor) == b'_' {
                    cursor += 1;
                }
            }
            if matches!(self.byte(cursor), b'e' | b'E') {
                let mut lookahead = cursor + 1;
                if matches!(self.byte(lookahead), b'+' | b'-') {
                    lookahead += 1;
                }
                if self.byte(lookahead).is_ascii_digit() {
                    cursor = lookahead;
                    while self.byte(cursor).is_ascii_digit() || self.byte(cursor) == b'_' {
                        cursor += 1;
                    }
                }
            }
        }

        if self.byte(cursor) == b'n' {
            cursor += 1;
        }

        self.pos = cursor;
        self.push(TokenKind::Number, start, cursor);
    }

    pub(crate) fn scan_identifier(&mut self) {
        let start = self.pos;
        let mut cursor = start;
        while let Some(len) = self.ident_char_len(cursor, cursor == start) {
            cursor += len;
        }
        self.pos = cursor;

        let text = &self.source[start..cursor];
        // After `.` or `?.` every reserved word is just a property name, so
        // `promise.catch(f)` must not be lexed as the `catch` keyword.
        let member_name = matches!(
            self.prev.map(|prev| prev.kind),
            Some(TokenKind::Punctuator(
                Punctuator::Dot | Punctuator::QuestionDot
            ))
        );
        let kind = match Keyword::lookup(text).filter(|_| !member_name) {
            Some(keyword) => {
                match keyword {
                    Keyword::Class | Keyword::Interface | Keyword::Enum => {
                        self.pending_body = Some(BraceKind::Class);
                    }
                    Keyword::Switch => self.pending_body = Some(BraceKind::Switch),
                    Keyword::Function | Keyword::Component | Keyword::Hook => {
                        self.pending_body = None;
                    }
                    _ => {}
                }
                TokenKind::Keyword(keyword)
            }
            None => TokenKind::Identifier,
        };
        self.push(kind, start, cursor);
    }
}
