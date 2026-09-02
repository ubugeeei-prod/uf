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
    /// An unterminated string, template or comment.
    Invalid,
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

/// Tokenize a module, skipping a BOM and a leading `#!` line.
pub fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens: Vec<Token> = Vec::with_capacity(bytes.len() / 6 + 8);
    let mut cursor = skip_bom(bytes);
    let mut newline_before = false;

    if bytes[cursor..].starts_with(b"#!") {
        cursor = line_end(bytes, cursor);
    }

    while cursor < bytes.len() {
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
        let kind = lex_token(bytes, &mut cursor, tokens.last(), source);
        tokens.push(Token {
            kind,
            start,
            end: cursor,
            newline_before,
        });
        newline_before = false;
    }

    tokens
}

fn skip_bom(bytes: &[u8]) -> usize {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        3
    } else {
        0
    }
}

/// Width in bytes of a line terminator at `at`, if there is one.
///
/// Handles LF, CR, CRLF and the two non-ASCII terminators U+2028 and U+2029.
fn line_terminator_width(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b'\n' => Some(1),
        b'\r' if bytes.get(at + 1) == Some(&b'\n') => Some(2),
        b'\r' => Some(1),
        0xe2 if bytes.get(at + 1) == Some(&0x80)
            && matches!(bytes.get(at + 2), Some(0xa8 | 0xa9)) =>
        {
            Some(3)
        }
        _ => None,
    }
}

/// Width in bytes of non-line-terminator whitespace at `at`, if there is any.
fn whitespace_width(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes[at] {
        b' ' | b'\t' | 0x0b | 0x0c => Some(1),
        // U+00A0 NO-BREAK SPACE.
        0xc2 if bytes.get(at + 1) == Some(&0xa0) => Some(2),
        // U+FEFF, legal whitespace away from the BOM.
        0xef if bytes.get(at + 1) == Some(&0xbb) && bytes.get(at + 2) == Some(&0xbf) => Some(3),
        _ => None,
    }
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    let mut cursor = from;
    while cursor < bytes.len() && line_terminator_width(bytes, cursor).is_none() {
        cursor += 1;
    }
    cursor
}

fn block_comment_end(bytes: &[u8], from: usize) -> (usize, bool) {
    let mut cursor = from + 2;
    let mut saw_newline = false;
    while cursor < bytes.len() {
        if line_terminator_width(bytes, cursor).is_some() {
            saw_newline = true;
        }
        if bytes[cursor] == b'*' && bytes.get(cursor + 1) == Some(&b'/') {
            return (cursor + 2, saw_newline);
        }
        cursor += 1;
    }
    (bytes.len(), saw_newline)
}

fn lex_token(bytes: &[u8], cursor: &mut usize, prev: Option<&Token>, source: &str) -> TokenKind {
    let byte = bytes[*cursor];
    match byte {
        b'"' | b'\'' => lex_quoted(bytes, cursor, byte),
        b'`' => lex_template(bytes, cursor),
        b'0'..=b'9' => {
            lex_number(bytes, cursor);
            TokenKind::Number
        }
        b'.' if matches!(bytes.get(*cursor + 1), Some(b'0'..=b'9')) => {
            lex_number(bytes, cursor);
            TokenKind::Number
        }
        b'/' if regex_allowed(prev, source) => {
            if lex_regex(bytes, cursor) {
                TokenKind::Regex
            } else {
                *cursor += 1;
                TokenKind::Punct(b'/')
            }
        }
        b'=' if bytes.get(*cursor + 1) == Some(&b'>') => {
            *cursor += 2;
            TokenKind::Arrow
        }
        _ if is_ident_start(byte) => {
            *cursor += 1;
            while *cursor < bytes.len() && is_ident_part(bytes[*cursor]) {
                *cursor += 1;
            }
            TokenKind::Ident
        }
        _ => {
            *cursor += 1;
            TokenKind::Punct(byte)
        }
    }
}

fn lex_quoted(bytes: &[u8], cursor: &mut usize, quote: u8) -> TokenKind {
    let mut at = *cursor + 1;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if byte == quote {
            *cursor = at + 1;
            return TokenKind::String;
        }
        if line_terminator_width(bytes, at).is_some() {
            break;
        }
        at += 1;
    }
    *cursor = at.min(bytes.len());
    TokenKind::Invalid
}

fn lex_template(bytes: &[u8], cursor: &mut usize) -> TokenKind {
    let mut at = *cursor + 1;
    let mut depth = 0usize;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if depth == 0 && byte == b'`' {
            *cursor = at + 1;
            return TokenKind::Template;
        }
        if byte == b'$' && bytes.get(at + 1) == Some(&b'{') {
            depth += 1;
            at += 2;
            continue;
        }
        if depth > 0 && byte == b'}' {
            depth -= 1;
        }
        at += 1;
    }
    *cursor = bytes.len();
    TokenKind::Invalid
}

fn lex_number(bytes: &[u8], cursor: &mut usize) {
    let mut at = *cursor;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' {
            at += 1;
            continue;
        }
        if (byte == b'+' || byte == b'-') && at > *cursor {
            let previous = bytes[at - 1];
            if previous == b'e' || previous == b'E' {
                at += 1;
                continue;
            }
        }
        break;
    }
    *cursor = at;
}

/// Consume a regular-expression literal, reporting whether one was found.
fn lex_regex(bytes: &[u8], cursor: &mut usize) -> bool {
    let mut at = *cursor + 1;
    let mut in_class = false;
    while at < bytes.len() {
        let byte = bytes[at];
        if byte == b'\\' {
            at += 2;
            continue;
        }
        if line_terminator_width(bytes, at).is_some() {
            return false;
        }
        match byte {
            b'[' => in_class = true,
            b']' => in_class = false,
            b'/' if !in_class => {
                at += 1;
                while at < bytes.len() && is_ident_part(bytes[at]) {
                    at += 1;
                }
                *cursor = at;
                return true;
            }
            _ => {}
        }
        at += 1;
    }
    false
}

/// Whether a `/` here starts a regular expression rather than a division.
///
/// A `<` before the slash is excluded so `</div>` in JSX is never lexed as the
/// start of a regular expression, which would swallow the rest of the line.
fn regex_allowed(prev: Option<&Token>, source: &str) -> bool {
    let Some(prev) = prev else {
        return true;
    };
    match prev.kind {
        TokenKind::Arrow => true,
        TokenKind::Ident => matches!(
            prev.text(source),
            "await"
                | "case"
                | "delete"
                | "do"
                | "else"
                | "in"
                | "instanceof"
                | "new"
                | "of"
                | "return"
                | "throw"
                | "typeof"
                | "void"
                | "yield"
        ),
        TokenKind::Punct(byte) => !matches!(byte, b')' | b']' | b'}' | b'<'),
        _ => false,
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' || byte >= 0x80
}

fn is_ident_part(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
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
