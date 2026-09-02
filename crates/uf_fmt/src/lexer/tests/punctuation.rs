//! Maximal munch over the operator table, and the Flow type punctuation that
//! shares its spellings with ordinary operators.

use super::significant;
use crate::lexer::{Keyword, Punctuator, TokenKind};

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
