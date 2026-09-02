//! What a token is: its byte range, its classification, and the ways a literal
//! can run off the end of the input.
//!
//! The kinds here are the vocabulary the whole formatter speaks. Trivia is a
//! first-class kind rather than something the scanner throws away, which is what
//! makes the token stream lossless.

use std::fmt;

use super::keyword::Keyword;
use super::punctuator::Punctuator;

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
