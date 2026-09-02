//! Single-pass, lossless tokenizer for Flow-typed JavaScript sources.
//!
//! The tokenizer scans the source exactly once and never drops a byte: every
//! byte of the input belongs to exactly one token, trivia included. Concatenating
//! the source slice of every token therefore reproduces the input byte for byte,
//! which is what lets [`crate::format_source`] guarantee that formatting can only
//! ever rewrite trivia.
//!
//! The scanner is not a parser. It resolves the three classic JavaScript
//! tokenizer ambiguities with the standard "previous significant token" rule plus
//! an explicit context stack:
//!
//! * `/` is a regular expression when an expression may start at that position,
//!   and division otherwise;
//! * `<` starts JSX when an expression may start at that position, and is a
//!   relational/type-argument punctuator otherwise;
//! * `}` resumes a template literal when it closes a `${` interpolation.
//!
//! The context stack is an explicit [`Vec`], never recursion, so pathological
//! inputs such as `{{{{…}}}}` nested ten thousand deep cannot overflow the stack.

use std::fmt;

/// A half-open byte range within the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    /// Inclusive start offset, in bytes.
    pub start: usize,
    /// Exclusive end offset, in bytes.
    pub end: usize,
}

impl Span {
    /// Length of the span in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the span covers no bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// A single lexical token: its classification plus the bytes it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Where the token lives in the source.
    pub span: Span,
}

impl Token {
    /// The exact source text this token covers.
    ///
    /// # Panics
    ///
    /// Panics if `source` is not the string the token was produced from.
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.span.start..self.span.end]
    }
}

/// Why a construct could not be closed before the input ran out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unterminated {
    /// A `'`/`"` string literal that hit a line terminator or the end of input.
    String,
    /// A template literal whose closing backtick is missing.
    Template,
    /// A `/* … */` comment whose `*/` is missing.
    BlockComment,
    /// A regular expression literal whose closing `/` is missing.
    Regex,
}

/// The classification of a single token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// A run of horizontal whitespace.
    Whitespace,
    /// A single line terminator.
    Newline,
    /// A `#!` interpreter directive on the first line.
    Shebang,
    /// A `//` comment, up to but excluding the line terminator.
    LineComment,
    /// A `/* … */` comment.
    BlockComment,
    /// A `/** … */` documentation comment.
    DocComment,
    /// An identifier, including Unicode identifiers.
    Identifier,
    /// A reserved or contextual keyword.
    Keyword(Keyword),
    /// A `#name` private class member.
    PrivateName,
    /// A numeric literal, including `0x`/`0b`/`0o`, separators and `BigInt`.
    Number,
    /// A `'…'` or `"…"` string literal.
    String,
    /// A template literal with no interpolation: `` `…` ``.
    TemplateFull,
    /// The head of an interpolated template: `` `…${ ``.
    TemplateHead,
    /// A template chunk between two interpolations: `}…${`.
    TemplateMiddle,
    /// The tail of an interpolated template: `` }…` ``.
    TemplateTail,
    /// A regular expression literal including flags.
    Regex,
    /// The `<` that opens a JSX opening or self-closing tag.
    JsxOpenStart,
    /// The `</` that opens a JSX closing tag.
    JsxCloseStart,
    /// The `>` that ends a JSX tag.
    JsxTagEnd,
    /// The `/>` that ends a self-closing JSX tag.
    JsxSelfClose,
    /// A JSX element or attribute name, including `-`, `.` and `:` parts.
    JsxName,
    /// A JSX attribute string, which has no escape sequences.
    JsxString,
    /// A run of JSX character data.
    JsxText,
    /// Punctuation or an operator.
    Punctuator(Punctuator),
    /// A construct that the input ended in the middle of.
    Unterminated(Unterminated),
    /// A byte that is not valid anywhere in the grammar.
    Unknown,
}

impl TokenKind {
    /// Whether the token carries no program meaning: whitespace or a comment.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            TokenKind::Whitespace
                | TokenKind::Newline
                | TokenKind::LineComment
                | TokenKind::BlockComment
                | TokenKind::DocComment
        )
    }

    /// Whether the token is a comment of any flavour.
    #[must_use]
    pub const fn is_comment(self) -> bool {
        matches!(
            self,
            TokenKind::LineComment | TokenKind::BlockComment | TokenKind::DocComment
        )
    }

    /// Whether the token belongs to JSX syntax rather than plain JavaScript.
    #[must_use]
    pub const fn is_jsx(self) -> bool {
        matches!(
            self,
            TokenKind::JsxOpenStart
                | TokenKind::JsxCloseStart
                | TokenKind::JsxTagEnd
                | TokenKind::JsxSelfClose
                | TokenKind::JsxName
                | TokenKind::JsxString
                | TokenKind::JsxText
        )
    }
}

/// Reserved and contextual keywords the formatter cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    missing_docs,
    reason = "each variant is the identically spelled keyword"
)]
pub enum Keyword {
    As,
    Async,
    Await,
    Break,
    Case,
    Catch,
    Class,
    Component,
    Const,
    Continue,
    Debugger,
    Declare,
    Default,
    Delete,
    Do,
    Else,
    Enum,
    Export,
    Extends,
    False,
    Finally,
    For,
    From,
    Function,
    Get,
    Hook,
    If,
    Implements,
    Import,
    In,
    Instanceof,
    Interface,
    Let,
    Mixins,
    New,
    Null,
    Of,
    Opaque,
    Renders,
    Return,
    Set,
    Static,
    Super,
    Switch,
    This,
    Throw,
    True,
    Try,
    Type,
    Typeof,
    Var,
    Void,
    While,
    With,
    Yield,
}

impl Keyword {
    /// Look up a keyword by its exact spelling.
    #[must_use]
    pub fn lookup(value: &str) -> Option<Self> {
        // A `match` on the string literal compiles to a length switch followed by
        // a memcmp chain, which beats a hash lookup for inputs this short.
        let keyword = match value {
            "as" => Keyword::As,
            "async" => Keyword::Async,
            "await" => Keyword::Await,
            "break" => Keyword::Break,
            "case" => Keyword::Case,
            "catch" => Keyword::Catch,
            "class" => Keyword::Class,
            "component" => Keyword::Component,
            "const" => Keyword::Const,
            "continue" => Keyword::Continue,
            "debugger" => Keyword::Debugger,
            "declare" => Keyword::Declare,
            "default" => Keyword::Default,
            "delete" => Keyword::Delete,
            "do" => Keyword::Do,
            "else" => Keyword::Else,
            "enum" => Keyword::Enum,
            "export" => Keyword::Export,
            "extends" => Keyword::Extends,
            "false" => Keyword::False,
            "finally" => Keyword::Finally,
            "for" => Keyword::For,
            "from" => Keyword::From,
            "function" => Keyword::Function,
            "get" => Keyword::Get,
            "hook" => Keyword::Hook,
            "if" => Keyword::If,
            "implements" => Keyword::Implements,
            "import" => Keyword::Import,
            "in" => Keyword::In,
            "instanceof" => Keyword::Instanceof,
            "interface" => Keyword::Interface,
            "let" => Keyword::Let,
            "mixins" => Keyword::Mixins,
            "new" => Keyword::New,
            "null" => Keyword::Null,
            "of" => Keyword::Of,
            "opaque" => Keyword::Opaque,
            "renders" => Keyword::Renders,
            "return" => Keyword::Return,
            "set" => Keyword::Set,
            "static" => Keyword::Static,
            "super" => Keyword::Super,
            "switch" => Keyword::Switch,
            "this" => Keyword::This,
            "throw" => Keyword::Throw,
            "true" => Keyword::True,
            "try" => Keyword::Try,
            "type" => Keyword::Type,
            "typeof" => Keyword::Typeof,
            "var" => Keyword::Var,
            "void" => Keyword::Void,
            "while" => Keyword::While,
            "with" => Keyword::With,
            "yield" => Keyword::Yield,
            _ => return None,
        };
        Some(keyword)
    }

    /// Whether an expression may start immediately after this keyword.
    ///
    /// Only unambiguously reserved words are listed. Contextual keywords such as
    /// `type` or `get` can legally be plain variable names, and treating them as
    /// expression starters would mis-lex `type / 2` as a regular expression.
    #[must_use]
    pub const fn allows_expression_after(self) -> bool {
        matches!(
            self,
            Keyword::Await
                | Keyword::Case
                | Keyword::Default
                | Keyword::Delete
                | Keyword::Do
                | Keyword::Else
                | Keyword::Extends
                | Keyword::In
                | Keyword::Instanceof
                | Keyword::New
                | Keyword::Of
                | Keyword::Return
                | Keyword::Throw
                | Keyword::Typeof
                | Keyword::Void
                | Keyword::Yield
        )
    }

    /// Whether the keyword introduces a parenthesised statement header, whose
    /// closing `)` may legally be followed by a regular expression.
    #[must_use]
    pub const fn starts_statement_header(self) -> bool {
        matches!(
            self,
            Keyword::Catch
                | Keyword::For
                | Keyword::If
                | Keyword::Switch
                | Keyword::While
                | Keyword::With
        )
    }

    /// Whether the keyword introduces a type-only declaration, where `<` opens a
    /// type parameter list rather than a JSX element.
    #[must_use]
    pub const fn starts_type_declaration(self) -> bool {
        matches!(
            self,
            Keyword::Declare | Keyword::Interface | Keyword::Opaque | Keyword::Type
        )
    }
}

/// Punctuation and operator tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    missing_docs,
    reason = "each variant is named after the spelling it holds"
)]
pub enum Punctuator {
    OpenBrace,
    CloseBrace,
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    Semicolon,
    Comma,
    Dot,
    Ellipsis,
    QuestionDot,
    Question,
    Colon,
    Arrow,
    At,
    Plus,
    Minus,
    Star,
    StarStar,
    Slash,
    Percent,
    PlusPlus,
    MinusMinus,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    EqualEqual,
    BangEqual,
    EqualEqualEqual,
    BangEqualEqual,
    LessLess,
    GreaterGreater,
    GreaterGreaterGreater,
    Amp,
    Pipe,
    Caret,
    Bang,
    Tilde,
    AmpAmp,
    PipePipe,
    QuestionQuestion,
    Equal,
    PlusEqual,
    MinusEqual,
    StarEqual,
    StarStarEqual,
    SlashEqual,
    PercentEqual,
    LessLessEqual,
    GreaterGreaterEqual,
    GreaterGreaterGreaterEqual,
    AmpEqual,
    PipeEqual,
    CaretEqual,
    AmpAmpEqual,
    PipePipeEqual,
    QuestionQuestionEqual,
}

impl Punctuator {
    /// Whether this punctuator opens a bracketed group.
    #[must_use]
    pub const fn is_open_delimiter(self) -> bool {
        matches!(
            self,
            Punctuator::OpenBrace | Punctuator::OpenParen | Punctuator::OpenBracket
        )
    }

    /// Whether this punctuator closes a bracketed group.
    #[must_use]
    pub const fn is_close_delimiter(self) -> bool {
        matches!(
            self,
            Punctuator::CloseBrace | Punctuator::CloseParen | Punctuator::CloseBracket
        )
    }

    /// How many `>` characters this punctuator contributes when it closes a type
    /// argument list. `Array<Array<T>>` closes two levels with a single `>>`.
    #[must_use]
    pub const fn angle_close_count(self) -> u32 {
        match self {
            Punctuator::Greater => 1,
            Punctuator::GreaterGreater => 2,
            Punctuator::GreaterGreaterGreater => 3,
            _ => 0,
        }
    }
}

/// What kind of `{ … }` a brace pair delimits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceKind {
    /// A statement block, function body or arrow body.
    Block,
    /// An object literal or an object type annotation.
    Object,
    /// A `class`, `interface` or `enum` body.
    Class,
    /// A `switch` body, whose `case` labels sit one level out.
    Switch,
    /// A `{ … }` inside JSX: an attribute value or a child expression container.
    JsxExpression,
}

/// What a "previous significant token" was, with the extra context needed to
/// disambiguate `/`, `<` and `{`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prev {
    pub(crate) kind: TokenKind,
    pub(crate) group: GroupKind,
}

/// Extra context attached to a closing delimiter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupKind {
    /// The token is not a closing delimiter.
    None,
    /// A `)` that closed an `if`/`for`/`while`/`with`/`catch`/`switch` header.
    StatementParen,
    /// A `)` that closed a call or a parenthesised expression.
    ExpressionParen,
    /// A `}` that closed the given brace flavour.
    Brace(BraceKind),
}

/// Whether an expression may start immediately after `prev`.
///
/// This single predicate drives both regular-expression detection and JSX
/// detection, because `/` and `<` may only introduce a literal where an
/// expression is allowed to begin.
pub(crate) fn expression_allowed(prev: Option<Prev>) -> bool {
    let Some(prev) = prev else {
        return true;
    };

    match prev.kind {
        TokenKind::Identifier
        | TokenKind::PrivateName
        | TokenKind::Number
        | TokenKind::String
        | TokenKind::TemplateFull
        | TokenKind::TemplateTail
        | TokenKind::Regex
        | TokenKind::Shebang
        | TokenKind::Unknown
        | TokenKind::Unterminated(_) => false,
        TokenKind::Keyword(keyword) => keyword.allows_expression_after(),
        TokenKind::Punctuator(punctuator) => match punctuator {
            Punctuator::CloseParen => prev.group == GroupKind::StatementParen,
            Punctuator::CloseBracket | Punctuator::PlusPlus | Punctuator::MinusMinus => false,
            Punctuator::CloseBrace => !matches!(prev.group, GroupKind::Brace(BraceKind::Object)),
            _ => true,
        },
        // Inside JSX, `{` is the only place a nested expression begins, and that
        // case is covered by the punctuator arm above.
        TokenKind::JsxSelfClose | TokenKind::JsxTagEnd | TokenKind::JsxName => false,
        _ => true,
    }
}

/// Classify a `{` given the token before it and the group that encloses it.
pub(crate) fn classify_brace(prev: Option<Prev>, enclosing: Option<BraceKind>) -> BraceKind {
    let Some(prev) = prev else {
        return BraceKind::Block;
    };

    match prev.kind {
        TokenKind::Identifier => BraceKind::Block,
        TokenKind::Keyword(
            Keyword::Else
            | Keyword::Do
            | Keyword::Try
            | Keyword::Finally
            | Keyword::Static
            | Keyword::Renders,
        ) => BraceKind::Block,
        TokenKind::Keyword(_) => BraceKind::Object,
        TokenKind::Punctuator(punctuator) => match punctuator {
            Punctuator::CloseParen
            | Punctuator::CloseBrace
            | Punctuator::Semicolon
            | Punctuator::OpenBrace
            | Punctuator::Arrow
            // `function f(): Promise<void> {` — a `{` after a closing angle
            // bracket always follows a return type, never an operand.
            | Punctuator::Greater
            | Punctuator::GreaterGreater
            | Punctuator::GreaterGreaterGreater
            // `hook useSelection(): [string, (next: string) => void] {` — same
            // reasoning for a tuple or array return type. Nothing valid puts an
            // object literal directly after `]`; `[1, 2] {}` is not an
            // expression. Reading it as an object made `needs_semicolon` emit a
            // `;` after the body, which is a token the input never had.
            | Punctuator::CloseBracket => BraceKind::Block,
            // `case 1: {` is a block, but `{ key: { … } }` is a nested object.
            Punctuator::Colon => match enclosing {
                Some(BraceKind::Object) | None => BraceKind::Object,
                Some(_) => BraceKind::Block,
            },
            _ => BraceKind::Object,
        },
        _ => BraceKind::Object,
    }
}

/// Tokenize `source` into a lossless token stream.
///
/// The returned tokens tile the whole input: `tokens[0].span.start == 0`, each
/// token starts where the previous one ended, and the last token ends at
/// `source.len()`.
#[must_use]
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).run()
}

/// Nesting contexts the scanner tracks with an explicit stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
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
struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    stack: Vec<Ctx>,
    tokens: Vec<Token>,
    prev: Option<Prev>,
    /// Set by `class`/`interface`/`enum`/`switch` so the matching `{` is
    /// classified as a body rather than an object literal.
    pending_body: Option<BraceKind>,
    /// The keyword a statement started with, used to keep `type X = <T>(…) => T`
    /// from being scanned as JSX.
    statement_head: Option<Keyword>,
    /// Whether the innermost `(` opened a statement header.
    paren_headers: Vec<bool>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
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

    fn run(mut self) -> Vec<Token> {
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

    fn byte(&self, at: usize) -> u8 {
        // Out of range reads report NUL, which is never a meaningful delimiter.
        self.bytes.get(at).copied().unwrap_or(0)
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
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

    fn line_end_from(&self, from: usize) -> usize {
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

    fn push_punctuator(&mut self, punctuator: Punctuator, start: usize, end: usize) {
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

    fn scan_slash(&mut self) {
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

    fn scan_string(&mut self, quote: u8) {
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
    fn scan_template_from(&mut self, start: usize, head: bool) {
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

    fn scan_number(&mut self) {
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

    fn scan_identifier(&mut self) {
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

    /// Byte length of the UTF-8 character starting at `at`, or 1 past the end.
    fn char_len(&self, at: usize) -> usize {
        match self.bytes.get(at) {
            None => 1,
            Some(byte) if *byte < 0x80 => 1,
            Some(_) => self.source[at..].chars().next().map_or(1, char::len_utf8),
        }
    }

    /// Byte length of the identifier character at `at`, if there is one.
    fn ident_char_len(&self, at: usize, start: bool) -> Option<usize> {
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

    fn scan_punctuator(&self, at: usize) -> Option<(Punctuator, usize)> {
        let b0 = *self.bytes.get(at)?;
        let b1 = self.byte(at + 1);
        let b2 = self.byte(at + 2);
        let b3 = self.byte(at + 3);

        let found = match b0 {
            b'{' => (Punctuator::OpenBrace, 1),
            b'}' => (Punctuator::CloseBrace, 1),
            b'(' => (Punctuator::OpenParen, 1),
            b')' => (Punctuator::CloseParen, 1),
            b'[' => (Punctuator::OpenBracket, 1),
            b']' => (Punctuator::CloseBracket, 1),
            b';' => (Punctuator::Semicolon, 1),
            b',' => (Punctuator::Comma, 1),
            b'~' => (Punctuator::Tilde, 1),
            b'@' => (Punctuator::At, 1),
            b':' => (Punctuator::Colon, 1),
            b'.' => {
                if b1 == b'.' && b2 == b'.' {
                    (Punctuator::Ellipsis, 3)
                } else {
                    (Punctuator::Dot, 1)
                }
            }
            b'?' => match b1 {
                // `?.5` is a ternary followed by a number, not optional chaining.
                b'.' if !b2.is_ascii_digit() => (Punctuator::QuestionDot, 2),
                b'?' if b2 == b'=' => (Punctuator::QuestionQuestionEqual, 3),
                b'?' => (Punctuator::QuestionQuestion, 2),
                _ => (Punctuator::Question, 1),
            },
            b'+' => match b1 {
                b'+' => (Punctuator::PlusPlus, 2),
                b'=' => (Punctuator::PlusEqual, 2),
                _ => (Punctuator::Plus, 1),
            },
            b'-' => match b1 {
                b'-' => (Punctuator::MinusMinus, 2),
                b'=' => (Punctuator::MinusEqual, 2),
                _ => (Punctuator::Minus, 1),
            },
            b'*' => match (b1, b2) {
                (b'*', b'=') => (Punctuator::StarStarEqual, 3),
                (b'*', _) => (Punctuator::StarStar, 2),
                (b'=', _) => (Punctuator::StarEqual, 2),
                _ => (Punctuator::Star, 1),
            },
            b'/' => match b1 {
                b'=' => (Punctuator::SlashEqual, 2),
                _ => (Punctuator::Slash, 1),
            },
            b'%' => match b1 {
                b'=' => (Punctuator::PercentEqual, 2),
                _ => (Punctuator::Percent, 1),
            },
            b'=' => match (b1, b2) {
                (b'=', b'=') => (Punctuator::EqualEqualEqual, 3),
                (b'=', _) => (Punctuator::EqualEqual, 2),
                (b'>', _) => (Punctuator::Arrow, 2),
                _ => (Punctuator::Equal, 1),
            },
            b'!' => match (b1, b2) {
                (b'=', b'=') => (Punctuator::BangEqualEqual, 3),
                (b'=', _) => (Punctuator::BangEqual, 2),
                _ => (Punctuator::Bang, 1),
            },
            b'<' => match (b1, b2) {
                (b'<', b'=') => (Punctuator::LessLessEqual, 3),
                (b'<', _) => (Punctuator::LessLess, 2),
                (b'=', _) => (Punctuator::LessEqual, 2),
                _ => (Punctuator::Less, 1),
            },
            b'>' => match (b1, b2, b3) {
                (b'>', b'>', b'=') => (Punctuator::GreaterGreaterGreaterEqual, 4),
                (b'>', b'>', _) => (Punctuator::GreaterGreaterGreater, 3),
                (b'>', b'=', _) => (Punctuator::GreaterGreaterEqual, 3),
                (b'>', _, _) => (Punctuator::GreaterGreater, 2),
                (b'=', _, _) => (Punctuator::GreaterEqual, 2),
                _ => (Punctuator::Greater, 1),
            },
            b'&' => match (b1, b2) {
                (b'&', b'=') => (Punctuator::AmpAmpEqual, 3),
                (b'&', _) => (Punctuator::AmpAmp, 2),
                (b'=', _) => (Punctuator::AmpEqual, 2),
                _ => (Punctuator::Amp, 1),
            },
            b'|' => match (b1, b2) {
                (b'|', b'=') => (Punctuator::PipePipeEqual, 3),
                (b'|', _) => (Punctuator::PipePipe, 2),
                (b'=', _) => (Punctuator::PipeEqual, 2),
                _ => (Punctuator::Pipe, 1),
            },
            b'^' => match b1 {
                b'=' => (Punctuator::CaretEqual, 2),
                _ => (Punctuator::Caret, 1),
            },
            _ => return None,
        };
        Some(found)
    }

    // ------------------------------------------------------------------- JSX

    fn scan_jsx_tag(&mut self) {
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

    fn scan_jsx_child(&mut self) {
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

impl fmt::Display for Unterminated {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Unterminated::String => "string literal",
            Unterminated::Template => "template literal",
            Unterminated::BlockComment => "block comment",
            Unterminated::Regex => "regular expression",
        };
        formatter.write_str(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::SOURCE_CORPUS as CORPUS;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source).into_iter().map(|t| t.kind).collect()
    }

    fn significant(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .into_iter()
            .filter(|token| !token.kind.is_trivia())
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn tokens_reproduce_the_source_byte_for_byte() {
        for source in CORPUS {
            let mut rebuilt = String::with_capacity(source.len());
            for token in tokenize(source) {
                rebuilt.push_str(token.text(source));
            }
            assert_eq!(&rebuilt, source, "round trip failed for {source:?}");
        }
    }

    #[test]
    fn token_spans_tile_the_source_without_gaps() {
        for source in CORPUS {
            let mut cursor = 0;
            for token in tokenize(source) {
                assert_eq!(token.span.start, cursor, "gap in {source:?}");
                assert!(
                    token.span.end > token.span.start,
                    "empty token in {source:?}"
                );
                cursor = token.span.end;
            }
            assert_eq!(cursor, source.len(), "trailing gap in {source:?}");
        }
    }

    #[test]
    fn empty_source_produces_no_tokens() {
        assert!(tokenize("").is_empty());
    }

    #[test]
    fn line_comments_stop_before_the_newline() {
        let tokens = kinds("// hi\nx");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LineComment,
                TokenKind::Newline,
                TokenKind::Identifier
            ]
        );
    }

    #[test]
    fn doc_comments_are_distinguished_from_block_comments() {
        assert_eq!(kinds("/** doc */"), vec![TokenKind::DocComment]);
        assert_eq!(kinds("/* plain */"), vec![TokenKind::BlockComment]);
        assert_eq!(kinds("/**/"), vec![TokenKind::BlockComment]);
    }

    #[test]
    fn unterminated_block_comment_runs_to_end_of_input() {
        assert_eq!(
            kinds("/* nope"),
            vec![TokenKind::Unterminated(Unterminated::BlockComment)]
        );
    }

    #[test]
    fn strings_handle_escapes_and_line_continuations() {
        assert_eq!(significant("'it\\'s'"), vec![TokenKind::String]);
        assert_eq!(significant("\"a\\\"b\""), vec![TokenKind::String]);
        assert_eq!(significant("'a\\\nb'"), vec![TokenKind::String]);
    }

    #[test]
    fn raw_newline_terminates_a_string_literal() {
        assert_eq!(
            kinds("'oops\nx"),
            vec![
                TokenKind::Unterminated(Unterminated::String),
                TokenKind::Newline,
                TokenKind::Identifier
            ]
        );
    }

    #[test]
    fn templates_track_nested_interpolations() {
        assert_eq!(
            significant("`a${`b${c}d`}e`"),
            vec![
                TokenKind::TemplateHead,
                TokenKind::TemplateHead,
                TokenKind::Identifier,
                TokenKind::TemplateTail,
                TokenKind::TemplateTail,
            ]
        );
    }

    #[test]
    fn template_middle_is_emitted_between_interpolations() {
        assert_eq!(
            significant("`${a}-${b}`"),
            vec![
                TokenKind::TemplateHead,
                TokenKind::Identifier,
                TokenKind::TemplateMiddle,
                TokenKind::Identifier,
                TokenKind::TemplateTail,
            ]
        );
    }

    #[test]
    fn object_literals_inside_interpolations_do_not_end_the_template() {
        assert_eq!(
            significant("`${ {a: 1} }`"),
            vec![
                TokenKind::TemplateHead,
                TokenKind::Punctuator(Punctuator::OpenBrace),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Colon),
                TokenKind::Number,
                TokenKind::Punctuator(Punctuator::CloseBrace),
                TokenKind::TemplateTail,
            ]
        );
    }

    #[test]
    fn unterminated_template_is_reported() {
        assert_eq!(
            kinds("`abc"),
            vec![TokenKind::Unterminated(Unterminated::Template)]
        );
    }

    #[test]
    fn regex_is_recognized_at_the_start_of_an_expression() {
        assert_eq!(
            significant("x = /a\\/b/g"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Equal),
                TokenKind::Regex,
            ]
        );
    }

    #[test]
    fn slash_after_an_identifier_is_division() {
        assert_eq!(
            significant("a / b / c"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Slash),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Slash),
                TokenKind::Identifier,
            ]
        );
    }

    #[test]
    fn slash_after_a_call_is_division_but_after_a_statement_header_is_regex() {
        assert!(significant("f() / 2").contains(&TokenKind::Punctuator(Punctuator::Slash)));
        assert!(!significant("f() / 2").contains(&TokenKind::Regex));
        assert!(significant("if (a) /re/.test(b)").contains(&TokenKind::Regex));
        assert!(significant("while (a) /re/.test(b)").contains(&TokenKind::Regex));
    }

    #[test]
    fn reserved_words_after_a_dot_are_property_names() {
        assert_eq!(
            significant("promise.catch(f)"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Dot),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::OpenParen),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::CloseParen),
            ]
        );
        assert_eq!(significant("a?.default")[2], TokenKind::Identifier);
    }

    #[test]
    fn slash_after_an_object_literal_is_division() {
        let tokens = significant("const x = {a: 1} / 2;");
        assert!(tokens.contains(&TokenKind::Punctuator(Punctuator::Slash)));
        assert!(!tokens.contains(&TokenKind::Regex));
    }

    #[test]
    fn a_brace_after_a_return_type_is_a_block() {
        let source = "function f(): Promise<void> {}\n/re/.test(x);";
        // The block classification is what lets the following `/` start a regex.
        assert!(significant(source).contains(&TokenKind::Regex));
    }

    #[test]
    fn slash_after_a_block_is_a_regex() {
        let tokens = significant("function f() {}\n/re/.test(x);");
        assert!(tokens.contains(&TokenKind::Regex));
    }

    #[test]
    fn regex_character_class_may_contain_a_slash() {
        assert_eq!(
            significant("x = /[/]/"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Equal),
                TokenKind::Regex,
            ]
        );
    }

    #[test]
    fn regex_with_a_newline_before_its_close_is_unterminated() {
        assert_eq!(
            kinds("x = /ab\n")[4],
            TokenKind::Unterminated(Unterminated::Regex)
        );
    }

    #[test]
    fn keywords_after_which_a_regex_may_start() {
        assert!(significant("return /re/;").contains(&TokenKind::Regex));
        assert!(significant("typeof /re/;").contains(&TokenKind::Regex));
        assert!(significant("case /re/:").contains(&TokenKind::Regex));
    }

    #[test]
    fn contextual_keywords_do_not_start_a_regex() {
        // `type` is a plain identifier here, so `/` must stay division.
        let tokens = significant("const type = 4; const half = type / 2;");
        assert!(!tokens.contains(&TokenKind::Regex));
    }

    #[test]
    fn numbers_cover_every_literal_form() {
        for source in [
            "0x1F",
            "0XFF",
            "0b1010",
            "0o777",
            "1_000_000",
            "1e10",
            "1E-10",
            "1.5",
            ".5",
            "1n",
            "0.5e+3",
        ] {
            assert_eq!(significant(source), vec![TokenKind::Number], "{source}");
        }
    }

    #[test]
    fn a_number_does_not_swallow_a_following_member_access() {
        assert_eq!(
            significant("1..toString()"),
            vec![
                TokenKind::Number,
                TokenKind::Punctuator(Punctuator::Dot),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::OpenParen),
                TokenKind::Punctuator(Punctuator::CloseParen),
            ]
        );
    }

    #[test]
    fn unicode_identifiers_are_single_tokens() {
        assert_eq!(significant("const café = 1;")[1], TokenKind::Identifier);
        assert_eq!(significant("日本語")[0], TokenKind::Identifier);
    }

    #[test]
    fn private_names_are_lexed_as_one_token() {
        assert_eq!(significant("this.#count")[2], TokenKind::PrivateName);
    }

    #[test]
    fn shebang_is_only_recognized_at_offset_zero() {
        assert_eq!(kinds("#!/usr/bin/env uf\n")[0], TokenKind::Shebang);
        assert_eq!(significant("x\n#!y")[1], TokenKind::Unknown);
    }

    #[test]
    fn jsx_elements_produce_jsx_tokens() {
        assert_eq!(
            significant("<div a=\"1\">hi</div>"),
            vec![
                TokenKind::JsxOpenStart,
                TokenKind::JsxName,
                TokenKind::JsxName,
                TokenKind::Punctuator(Punctuator::Equal),
                TokenKind::JsxString,
                TokenKind::JsxTagEnd,
                TokenKind::JsxText,
                TokenKind::JsxCloseStart,
                TokenKind::JsxName,
                TokenKind::JsxTagEnd,
            ]
        );
    }

    #[test]
    fn jsx_text_keeps_quotes_and_apostrophes_as_text() {
        let tokens = significant("<p>it's \"fine\"</p>");
        assert!(!tokens.contains(&TokenKind::String));
    }

    #[test]
    fn jsx_expression_containers_return_to_javascript() {
        assert_eq!(
            significant("<p>{a / b}</p>"),
            vec![
                TokenKind::JsxOpenStart,
                TokenKind::JsxName,
                TokenKind::JsxTagEnd,
                TokenKind::Punctuator(Punctuator::OpenBrace),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Slash),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::CloseBrace),
                TokenKind::JsxCloseStart,
                TokenKind::JsxName,
                TokenKind::JsxTagEnd,
            ]
        );
    }

    #[test]
    fn self_closing_jsx_tags_are_a_single_token() {
        assert_eq!(significant("<br />")[2], TokenKind::JsxSelfClose);
    }

    #[test]
    fn jsx_fragments_open_and_close() {
        assert_eq!(
            significant("<><b>x</b></>"),
            vec![
                TokenKind::JsxOpenStart,
                TokenKind::JsxTagEnd,
                TokenKind::JsxOpenStart,
                TokenKind::JsxName,
                TokenKind::JsxTagEnd,
                TokenKind::JsxText,
                TokenKind::JsxCloseStart,
                TokenKind::JsxName,
                TokenKind::JsxTagEnd,
                TokenKind::JsxCloseStart,
                TokenKind::JsxTagEnd,
            ]
        );
    }

    #[test]
    fn dashed_and_namespaced_jsx_names_stay_together() {
        let tokens = tokenize("<a data-x=\"1\" svg:y=\"2\" Foo.Bar />");
        let names: Vec<&str> = tokens
            .iter()
            .filter(|token| token.kind == TokenKind::JsxName)
            .map(|token| token.text("<a data-x=\"1\" svg:y=\"2\" Foo.Bar />"))
            .collect();
        assert_eq!(names, vec!["a", "data-x", "svg:y", "Foo.Bar"]);
    }

    #[test]
    fn less_than_after_an_identifier_is_not_jsx() {
        assert_eq!(
            significant("a < b"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Less),
                TokenKind::Identifier,
            ]
        );
    }

    #[test]
    fn generic_function_types_in_type_aliases_are_not_jsx() {
        let tokens = significant("type F = <T>(value: T) => T;");
        assert!(!tokens.iter().any(|kind| kind.is_jsx()));
    }

    #[test]
    fn jsx_after_an_arrow_is_recognized() {
        let tokens = significant("const A = () => <div />;");
        assert!(tokens.contains(&TokenKind::JsxOpenStart));
    }

    #[test]
    fn deeply_nested_braces_do_not_overflow_the_stack() {
        let depth = 10_000;
        let mut source = String::with_capacity(depth * 2);
        for _ in 0..depth {
            source.push('{');
        }
        for _ in 0..depth {
            source.push('}');
        }
        assert_eq!(tokenize(&source).len(), depth * 2);
    }

    #[test]
    fn unpaired_closing_delimiters_do_not_panic() {
        for source in [
            ")",
            "]",
            "}",
            "))))",
            "`${}}`",
            "<div></p></div>",
            "\\",
            "\\\\",
        ] {
            let tokens = tokenize(source);
            let mut rebuilt = String::new();
            for token in &tokens {
                rebuilt.push_str(token.text(source));
            }
            assert_eq!(rebuilt, source);
        }
    }

    #[test]
    fn crlf_is_a_single_newline_token() {
        assert_eq!(
            kinds("a\r\nb"),
            vec![
                TokenKind::Identifier,
                TokenKind::Newline,
                TokenKind::Identifier
            ]
        );
    }

    #[test]
    fn maximal_munch_picks_the_longest_operator() {
        assert_eq!(
            significant("a >>>= b"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::GreaterGreaterGreaterEqual),
                TokenKind::Identifier,
            ]
        );
        assert_eq!(
            significant("a ??= b"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::QuestionQuestionEqual),
                TokenKind::Identifier,
            ]
        );
        assert_eq!(
            significant("...rest"),
            vec![
                TokenKind::Punctuator(Punctuator::Ellipsis),
                TokenKind::Identifier
            ]
        );
    }

    #[test]
    fn optional_chaining_is_distinguished_from_a_ternary_on_a_number() {
        assert_eq!(
            significant("a?.b"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::QuestionDot),
                TokenKind::Identifier,
            ]
        );
        assert_eq!(
            significant("a ? .5 : 1"),
            vec![
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Question),
                TokenKind::Number,
                TokenKind::Punctuator(Punctuator::Colon),
                TokenKind::Number,
            ]
        );
    }

    #[test]
    fn flow_type_punctuation_is_tokenized() {
        assert_eq!(
            significant("type A = {| +x?: ?number |};"),
            vec![
                TokenKind::Keyword(Keyword::Type),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Equal),
                TokenKind::Punctuator(Punctuator::OpenBrace),
                TokenKind::Punctuator(Punctuator::Pipe),
                TokenKind::Punctuator(Punctuator::Plus),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Question),
                TokenKind::Punctuator(Punctuator::Colon),
                TokenKind::Punctuator(Punctuator::Question),
                TokenKind::Identifier,
                TokenKind::Punctuator(Punctuator::Pipe),
                TokenKind::Punctuator(Punctuator::CloseBrace),
                TokenKind::Punctuator(Punctuator::Semicolon),
            ]
        );
    }
}
