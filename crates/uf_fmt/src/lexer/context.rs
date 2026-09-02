//! The classification that resolves JavaScript's tokenizer ambiguities.
//!
//! `/` is division or a regular expression, and `<` is a comparison or the start
//! of JSX, purely according to whether an expression may begin at that position.
//! Answering that needs to know what the previous significant token was and, when
//! it closed a delimiter, what kind of group it closed -- an object literal is an
//! operand, a block is not.

use super::keyword::Keyword;
use super::punctuator::Punctuator;
use super::token::TokenKind;

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
