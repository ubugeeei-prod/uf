//! Maximal-munch scan of the operator table.
//!
//! Four bytes of lookahead are enough for every spelling in the grammar, the
//! longest being `>>>=`, so the table is a flat match rather than a trie.

use super::punctuator::Punctuator;
use super::scanner::Lexer;

impl<'a> Lexer<'a> {
    pub(crate) fn scan_punctuator(&self, at: usize) -> Option<(Punctuator, usize)> {
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
}
