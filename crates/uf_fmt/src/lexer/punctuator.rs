//! The operator and punctuation vocabulary.
//!
//! Alongside the spellings, this is where the printer asks whether a punctuator
//! opens or closes a bracketed group and how many type-argument levels a run of
//! `>` characters closes.

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
