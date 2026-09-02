//! The token-level half of automatic semicolon insertion.
//!
//! A semicolon may only be written between a token that can end a statement and
//! one that can start a fresh one. Anything that could instead continue the
//! previous expression -- `(`, `[`, a template literal, an operator, `in` -- is
//! excluded, because inserting a semicolon there would change what the program
//! means rather than how it looks.

use crate::lexer::{Keyword, Punctuator, TokenKind};

/// Whether a statement may end with this token.
pub(crate) fn ends_a_statement(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::PrivateName
            | TokenKind::Number
            | TokenKind::String
            | TokenKind::Regex
            | TokenKind::TemplateFull
            | TokenKind::TemplateTail
            | TokenKind::JsxTagEnd
            | TokenKind::JsxSelfClose
            | TokenKind::Punctuator(
                Punctuator::CloseParen
                    | Punctuator::CloseBracket
                    | Punctuator::PlusPlus
                    | Punctuator::MinusMinus
            )
            | TokenKind::Keyword(
                Keyword::This
                    | Keyword::Super
                    | Keyword::Null
                    | Keyword::True
                    | Keyword::False
                    | Keyword::Break
                    | Keyword::Continue
                    | Keyword::Return
                    | Keyword::Debugger
            )
    )
}

/// Whether a new statement may begin with this token.
///
/// Everything that could instead continue the previous expression — `(`, `[`, a
/// template literal, an operator, `in`/`of`/`instanceof` — is excluded, which is
/// exactly the set of tokens for which automatic semicolon insertion does not
/// fire.
pub(crate) fn starts_a_statement(kind: TokenKind) -> bool {
    match kind {
        TokenKind::Identifier | TokenKind::PrivateName => true,
        TokenKind::Keyword(keyword) => !matches!(
            keyword,
            Keyword::In
                | Keyword::Of
                | Keyword::Instanceof
                | Keyword::Extends
                | Keyword::As
                | Keyword::From
                | Keyword::Implements
                | Keyword::Mixins
                | Keyword::Renders
        ),
        TokenKind::Punctuator(punctuator) => matches!(
            punctuator,
            Punctuator::OpenBrace | Punctuator::CloseBrace | Punctuator::At
        ),
        _ => false,
    }
}
