//! The GraphQL lexer: the June 2018 grammar's tokens, plus the `\u{…}`
//! escape and block strings from the October 2021 edition.
//!
//! Two things here are worth knowing before reading the parser.
//!
//! A `#` comment is a token rather than an ignored character. The parser
//! refuses a document that holds one, because reproducing where Prettier
//! *puts* a comment means reproducing its generic comment-attachment pass,
//! and a comment moved to the wrong node is a worse outcome than a template
//! left alone. Making the lexer swallow comments silently was the first
//! attempt and it is wrong twice over: the parser then cannot see them to
//! refuse, and a comment inside a template would simply vanish from the
//! output.
//!
//! String values are cooked here, escapes resolved and block-string
//! indentation stripped, because that is the value Prettier's printer
//! prints — it re-escapes the value rather than echoing the source
//! spelling, so `A` comes back out as `A`.

use std::str::CharIndices;

use super::ast::StringValue;

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// `!`
    Bang,
    /// `$`
    Dollar,
    /// `&`
    Amp,
    /// `(`
    ParenL,
    /// `)`
    ParenR,
    /// `...`
    Spread,
    /// `:`
    Colon,
    /// `=`
    Equals,
    /// `@`
    At,
    /// `[`
    BracketL,
    /// `]`
    BracketR,
    /// `{`
    BraceL,
    /// `}`
    BraceR,
    /// `|`
    Pipe,
    /// A name.
    Name,
    /// An integer.
    Int,
    /// A float.
    Float,
    /// A `"…"` string.
    String,
    /// A `"""…"""` block string.
    BlockString,
    /// A `#…` comment, which the parser refuses.
    Comment,
    /// The end of the document.
    Eof,
}

/// One token.
#[derive(Debug, Clone)]
pub struct Token<'a> {
    /// What it is.
    pub kind: TokenKind,
    /// Its source text, delimiters included.
    pub text: &'a str,
    /// Its first byte.
    pub start: usize,
    /// One past its last byte.
    pub end: usize,
    /// For [`TokenKind::String`] and [`TokenKind::BlockString`], the value
    /// the string denotes.
    pub string: Option<StringValue>,
}

/// Why the lexer stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexError {
    /// The byte offset the bad token starts at.
    pub offset: usize,
    /// What went wrong.
    pub kind: LexErrorKind,
}

/// The two ways a token can be refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexErrorKind {
    /// Not GraphQL.
    Syntax,
    /// GraphQL, but holding a character the printer cannot write back.
    Unprintable,
}

impl LexError {
    fn syntax(offset: usize) -> Self {
        Self {
            offset,
            kind: LexErrorKind::Syntax,
        }
    }

    fn unprintable(offset: usize) -> Self {
        Self {
            offset,
            kind: LexErrorKind::Unprintable,
        }
    }
}

/// A cursor over the document text.
pub struct Lexer<'a> {
    source: &'a str,
    chars: CharIndices<'a>,
    peeked: Option<(usize, char)>,
}

impl<'a> Lexer<'a> {
    /// A lexer over `source`.
    pub fn new(source: &'a str) -> Self {
        let mut chars = source.char_indices();
        let peeked = chars.next();
        Self {
            source,
            chars,
            peeked,
        }
    }

    fn peek(&self) -> Option<(usize, char)> {
        self.peeked
    }

    fn bump(&mut self) -> Option<(usize, char)> {
        let current = self.peeked;
        self.peeked = self.chars.next();
        current
    }

    fn offset(&self) -> usize {
        self.peeked.map_or(self.source.len(), |(at, _)| at)
    }

    /// The next token, ignored characters skipped.
    ///
    /// # Errors
    ///
    /// Returns [`LexError`] at the first character that starts no token.
    pub fn next_token(&mut self) -> Result<Token<'a>, LexError> {
        self.skip_ignored();
        let start = self.offset();
        let Some((_, ch)) = self.peek() else {
            return Ok(self.token(TokenKind::Eof, start, start, None));
        };

        let single = match ch {
            '!' => Some(TokenKind::Bang),
            '$' => Some(TokenKind::Dollar),
            '&' => Some(TokenKind::Amp),
            '(' => Some(TokenKind::ParenL),
            ')' => Some(TokenKind::ParenR),
            ':' => Some(TokenKind::Colon),
            '=' => Some(TokenKind::Equals),
            '@' => Some(TokenKind::At),
            '[' => Some(TokenKind::BracketL),
            ']' => Some(TokenKind::BracketR),
            '{' => Some(TokenKind::BraceL),
            '}' => Some(TokenKind::BraceR),
            '|' => Some(TokenKind::Pipe),
            _ => None,
        };
        if let Some(kind) = single {
            self.bump();
            return Ok(self.token(kind, start, self.offset(), None));
        }

        match ch {
            '.' => self.lex_spread(start),
            '#' => Ok(self.lex_comment(start)),
            '"' => self.lex_string(start),
            '-' | '0'..='9' => self.lex_number(start),
            '_' | 'a'..='z' | 'A'..='Z' => Ok(self.lex_name(start)),
            _ => Err(LexError::syntax(start)),
        }
    }

    fn token(
        &self,
        kind: TokenKind,
        start: usize,
        end: usize,
        string: Option<StringValue>,
    ) -> Token<'a> {
        Token {
            kind,
            text: &self.source[start..end],
            start,
            end,
            string,
        }
    }

    /// Whitespace, line terminators, commas and the byte order mark are
    /// "ignored tokens" in the grammar; a comment is not, and is lexed.
    fn skip_ignored(&mut self) {
        while let Some((_, ch)) = self.peek() {
            match ch {
                ' ' | '\t' | '\n' | '\r' | ',' | '\u{feff}' => {
                    self.bump();
                }
                _ => break,
            }
        }
    }

    fn lex_spread(&mut self, start: usize) -> Result<Token<'a>, LexError> {
        for _ in 0..3 {
            match self.peek() {
                Some((_, '.')) => {
                    self.bump();
                }
                _ => return Err(LexError::syntax(start)),
            }
        }
        Ok(self.token(TokenKind::Spread, start, self.offset(), None))
    }

    fn lex_comment(&mut self, start: usize) -> Token<'a> {
        while let Some((_, ch)) = self.peek() {
            if ch == '\n' || ch == '\r' {
                break;
            }
            self.bump();
        }
        self.token(TokenKind::Comment, start, self.offset(), None)
    }

    fn lex_name(&mut self, start: usize) -> Token<'a> {
        while let Some((_, ch)) = self.peek() {
            if ch == '_' || ch.is_ascii_alphanumeric() {
                self.bump();
            } else {
                break;
            }
        }
        self.token(TokenKind::Name, start, self.offset(), None)
    }

    fn lex_number(&mut self, start: usize) -> Result<Token<'a>, LexError> {
        if matches!(self.peek(), Some((_, '-'))) {
            self.bump();
        }
        match self.peek() {
            // A leading zero admits no more digits: `01` is not a number.
            Some((_, '0')) => {
                self.bump();
                if matches!(self.peek(), Some((_, '0'..='9'))) {
                    return Err(LexError::syntax(start));
                }
            }
            Some((_, '1'..='9')) => self.digits(),
            _ => return Err(LexError::syntax(start)),
        }

        let mut float = false;
        if matches!(self.peek(), Some((_, '.'))) {
            float = true;
            self.bump();
            if !matches!(self.peek(), Some((_, '0'..='9'))) {
                return Err(LexError::syntax(start));
            }
            self.digits();
        }
        if matches!(self.peek(), Some((_, 'e' | 'E'))) {
            float = true;
            self.bump();
            if matches!(self.peek(), Some((_, '+' | '-'))) {
                self.bump();
            }
            if !matches!(self.peek(), Some((_, '0'..='9'))) {
                return Err(LexError::syntax(start));
            }
            self.digits();
        }
        // `1abc` and `1.2.3` are errors rather than a number followed by a
        // name, which is what the specification's lookahead restriction says.
        if matches!(
            self.peek(),
            Some((_, '_' | '.' | 'a'..='z' | 'A'..='Z' | '0'..='9'))
        ) {
            return Err(LexError::syntax(start));
        }
        let kind = if float {
            TokenKind::Float
        } else {
            TokenKind::Int
        };
        Ok(self.token(kind, start, self.offset(), None))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some((_, '0'..='9'))) {
            self.bump();
        }
    }

    fn lex_string(&mut self, start: usize) -> Result<Token<'a>, LexError> {
        self.bump();
        if matches!(self.peek(), Some((_, '"'))) {
            self.bump();
            if matches!(self.peek(), Some((_, '"'))) {
                self.bump();
                return self.lex_block_string(start);
            }
            // `""` is the empty string.
            let end = self.offset();
            return Ok(self.token(
                TokenKind::String,
                start,
                end,
                Some(StringValue {
                    value: String::new(),
                    block: false,
                }),
            ));
        }

        let mut value = String::new();
        loop {
            let Some((at, ch)) = self.bump() else {
                return Err(LexError::syntax(start));
            };
            match ch {
                '"' => break,
                '\n' | '\r' => return Err(LexError::syntax(start)),
                '\\' => self.lex_escape(&mut value, at)?,
                _ => value.push(ch),
            }
        }
        // Prettier prints a string *value*, escaping only `"`, `\\` and a
        // newline, so every other character in it goes out raw. For a
        // control character that is not a thing uf can do: a carriage
        // return written `\\r` would come back out as a real CR inside the
        // template, and the formatter normalises line endings on the way
        // in, so the next run would read a different string. Refusing the
        // document keeps `format(format(x)) == format(x)` true; the first
        // attempt was to escape those characters on the way out, which
        // matches the guarantee and stops matching Prettier.
        if let Some(offset) = value
            .char_indices()
            .find(|(_, ch)| (*ch as u32) < 0x20 && *ch != '\t' && *ch != '\n')
            .map(|(offset, _)| start + offset)
        {
            return Err(LexError::unprintable(offset));
        }
        let end = self.offset();
        Ok(self.token(
            TokenKind::String,
            start,
            end,
            Some(StringValue {
                value,
                block: false,
            }),
        ))
    }

    fn lex_escape(&mut self, out: &mut String, at: usize) -> Result<(), LexError> {
        let Some((_, ch)) = self.bump() else {
            return Err(LexError::syntax(at));
        };
        let simple = match ch {
            '"' => Some('"'),
            '\\' => Some('\\'),
            '/' => Some('/'),
            'b' => Some('\u{8}'),
            'f' => Some('\u{c}'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            _ => None,
        };
        if let Some(ch) = simple {
            out.push(ch);
            return Ok(());
        }
        if ch != 'u' {
            return Err(LexError::syntax(at));
        }
        if matches!(self.peek(), Some((_, '{'))) {
            self.bump();
            let mut code = 0u32;
            let mut digits = 0;
            loop {
                let Some((_, ch)) = self.bump() else {
                    return Err(LexError::syntax(at));
                };
                if ch == '}' {
                    break;
                }
                let digit = ch.to_digit(16).ok_or(LexError::syntax(at))?;
                code = code
                    .checked_mul(16)
                    .and_then(|code| code.checked_add(digit))
                    .ok_or(LexError::syntax(at))?;
                digits += 1;
                if digits > 8 {
                    return Err(LexError::syntax(at));
                }
            }
            if digits == 0 {
                return Err(LexError::syntax(at));
            }
            out.push(char::from_u32(code).ok_or(LexError::syntax(at))?);
            return Ok(());
        }

        let leading = self.hex4(at)?;
        // A leading surrogate takes a trailing one with it; the pair is one
        // character, and neither half is a character on its own.
        if (0xd800..0xdc00).contains(&leading) {
            if !matches!(self.peek(), Some((_, '\\'))) {
                return Err(LexError::syntax(at));
            }
            self.bump();
            if !matches!(self.peek(), Some((_, 'u'))) {
                return Err(LexError::syntax(at));
            }
            self.bump();
            let trailing = self.hex4(at)?;
            if !(0xdc00..0xe000).contains(&trailing) {
                return Err(LexError::syntax(at));
            }
            let code = 0x1_0000 + ((leading - 0xd800) << 10) + (trailing - 0xdc00);
            out.push(char::from_u32(code).ok_or(LexError::syntax(at))?);
            return Ok(());
        }
        out.push(char::from_u32(leading).ok_or(LexError::syntax(at))?);
        Ok(())
    }

    fn hex4(&mut self, at: usize) -> Result<u32, LexError> {
        let mut code = 0u32;
        for _ in 0..4 {
            let Some((_, ch)) = self.bump() else {
                return Err(LexError::syntax(at));
            };
            let digit = ch.to_digit(16).ok_or(LexError::syntax(at))?;
            code = code * 16 + digit;
        }
        Ok(code)
    }

    fn lex_block_string(&mut self, start: usize) -> Result<Token<'a>, LexError> {
        let mut raw = String::new();
        loop {
            let Some((_, ch)) = self.peek() else {
                return Err(LexError::syntax(start));
            };
            if ch == '\\' {
                // `\"""` is the one escape a block string has.
                let rest = &self.source[self.offset()..];
                if rest.starts_with("\\\"\"\"") {
                    for _ in 0..4 {
                        self.bump();
                    }
                    raw.push_str("\"\"\"");
                    continue;
                }
                self.bump();
                raw.push('\\');
                continue;
            }
            if ch == '"' && self.source[self.offset()..].starts_with("\"\"\"") {
                for _ in 0..3 {
                    self.bump();
                }
                break;
            }
            self.bump();
            raw.push(ch);
        }
        let end = self.offset();
        Ok(self.token(
            TokenKind::BlockString,
            start,
            end,
            Some(StringValue {
                value: block_string_value(&raw),
                block: true,
            }),
        ))
    }
}

/// The specification's `BlockStringValue`: normalise line endings, strip the
/// indentation common to every line after the first, then drop leading and
/// trailing blank lines.
fn block_string_value(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = normalized.split('\n').collect();

    let mut common: Option<usize> = None;
    for line in lines.iter().skip(1) {
        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
        if indent < line.len() && common.is_none_or(|current| indent < current) {
            common = Some(indent);
        }
    }
    if let Some(common) = common {
        for line in lines.iter_mut().skip(1) {
            if line.len() >= common {
                *line = &line[common..];
            } else {
                *line = "";
            }
        }
    }

    while lines
        .first()
        .is_some_and(|line| line.trim_matches([' ', '\t']).is_empty())
    {
        lines.remove(0);
    }
    while lines
        .last()
        .is_some_and(|line| line.trim_matches([' ', '\t']).is_empty())
    {
        lines.pop();
    }
    lines.join("\n")
}
