//! JSX scanning, which is a second grammar layered over the first.
//!
//! Inside a tag, names may contain `-`, `.` and `:`, and attribute strings have
//! no escape sequences at all. Between an opening and a closing tag, everything
//! that is not `<` or `{` is character data whose whitespace is significant.

use super::context::BraceKind;
use super::punctuator::Punctuator;
use super::scanner::{Ctx, Lexer};
use super::token::{TokenKind, Unterminated};

impl<'a> Lexer<'a> {
    pub(crate) fn scan_jsx_tag(&mut self) {
        let start = self.pos;
        let byte = self.byte(start);
        let closing = matches!(self.stack.last(), Some(Ctx::JsxTag { closing: true }));

        match byte {
            b'\n' => {
                self.pos = start + 1;
                self.push(TokenKind::Newline, start, self.pos);
            }
            b'\r' => {
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
            b'>' => {
                self.pos = start + 1;
                self.stack.pop();
                if closing {
                    if matches!(self.stack.last(), Some(Ctx::JsxChildren)) {
                        self.stack.pop();
                    }
                } else {
                    self.stack.push(Ctx::JsxChildren);
                }
                self.push(TokenKind::JsxTagEnd, start, self.pos);
            }
            b'/' if self.byte(start + 1) == b'>' => {
                self.pos = start + 2;
                self.stack.pop();
                self.push(TokenKind::JsxSelfClose, start, self.pos);
            }
            b'/' => {
                self.pos = start + 1;
                self.push(TokenKind::Punctuator(Punctuator::Slash), start, self.pos);
            }
            b'=' => {
                self.pos = start + 1;
                self.push(TokenKind::Punctuator(Punctuator::Equal), start, self.pos);
            }
            b'{' => {
                self.pos = start + 1;
                self.stack.push(Ctx::Brace(BraceKind::JsxExpression));
                self.push(
                    TokenKind::Punctuator(Punctuator::OpenBrace),
                    start,
                    self.pos,
                );
            }
            b'\'' | b'"' => {
                // JSX attribute strings have no escape sequences at all.
                let mut cursor = start + 1;
                let end = loop {
                    match self.bytes.get(cursor) {
                        None => break None,
                        Some(found) if *found == byte => break Some(cursor + 1),
                        Some(_) => cursor += self.char_len(cursor),
                    }
                };
                match end {
                    Some(end) => {
                        self.pos = end;
                        self.push(TokenKind::JsxString, start, end);
                    }
                    None => {
                        self.pos = self.bytes.len();
                        self.push(
                            TokenKind::Unterminated(Unterminated::String),
                            start,
                            self.pos,
                        );
                    }
                }
            }
            _ => {
                if self.ident_char_len(start, true).is_some() {
                    let mut cursor = start;
                    loop {
                        if let Some(len) = self.ident_char_len(cursor, false) {
                            cursor += len;
                            continue;
                        }
                        // `data-testid`, `Foo.Bar` and `svg:rect` are single names.
                        if matches!(self.byte(cursor), b'-' | b'.' | b':')
                            && self.ident_char_len(cursor + 1, true).is_some()
                        {
                            cursor += 1;
                            continue;
                        }
                        break;
                    }
                    self.pos = cursor;
                    self.push(TokenKind::JsxName, start, cursor);
                } else {
                    let len = self.char_len(start);
                    self.pos = start + len;
                    self.push(TokenKind::Unknown, start, self.pos);
                }
            }
        }
    }

    pub(crate) fn scan_jsx_child(&mut self) {
        let start = self.pos;
        let byte = self.byte(start);

        match byte {
            b'\n' => {
                self.pos = start + 1;
                self.push(TokenKind::Newline, start, self.pos);
            }
            b'\r' => {
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
            b'<' => {
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
            b'{' => {
                self.pos = start + 1;
                self.stack.push(Ctx::Brace(BraceKind::JsxExpression));
                self.push(
                    TokenKind::Punctuator(Punctuator::OpenBrace),
                    start,
                    self.pos,
                );
            }
            _ => {
                let mut end = start;
                while end < self.bytes.len() {
                    // Every stop byte is ASCII, so scanning raw bytes can never
                    // split a multi-byte character.
                    if matches!(
                        self.bytes[end],
                        b'<' | b'{' | b'\n' | b'\r' | b' ' | b'\t' | 0x0b | 0x0c
                    ) {
                        break;
                    }
                    end += 1;
                }
                self.pos = end;
                self.push(TokenKind::JsxText, start, end);
            }
        }
    }
}
