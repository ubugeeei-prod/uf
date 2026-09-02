//! The speculative scan that tells `A<B>` from `a < b`.
//!
//! Angle brackets are the one place a token-driven formatter has to guess, and a
//! wrong guess changes the spacing of ordinary comparisons. The scan is therefore
//! deliberately timid: it starts only after an identifier, accepts only
//! type-shaped tokens, commits only when the token after the closing `>` could
//! follow a type, and gives up once it has inspected more tokens than any real
//! type argument list contains.

use crate::lexer::{Keyword, Punctuator, Token, TokenKind};

/// Upper bound on how many tokens a speculative type-argument scan may inspect,
/// which keeps long `a < b` chains from making the scan quadratic.
const TYPE_ARGUMENT_SCAN_BUDGET: u32 = 4096;

/// Where a token sits relative to a type-argument list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Angle {
    /// Not part of a type-argument list.
    No,
    /// The `<` or `>` of a type-argument list.
    Bracket,
    /// A token nested inside a type-argument list.
    Inside,
}

/// Mark the tokens that belong to a type-argument list such as `Array<Map<K, V>>`.
///
/// Angle brackets are the one place a token-driven formatter has to guess: `a < b`
/// is a comparison and `A<B>` is a type application, and the two are only told
/// apart by what surrounds them. The scan therefore starts only after an
/// identifier, accepts only type-shaped tokens, and commits only when the token
/// after the closing `>` is one that may legally follow a type.
pub(crate) fn mark_type_angles(tokens: &[Token]) -> Vec<Angle> {
    let mut marks = vec![Angle::No; tokens.len()];
    let mut prev: Option<TokenKind> = None;
    let mut index = 0;

    while index < tokens.len() {
        let kind = tokens[index].kind;
        if kind.is_trivia() {
            index += 1;
            continue;
        }

        if kind == TokenKind::Punctuator(Punctuator::Less)
            && matches!(prev, Some(TokenKind::Identifier))
            && let Some(close) = scan_type_arguments(tokens, index)
        {
            for (offset, mark) in marks.iter_mut().enumerate().take(close + 1).skip(index) {
                let is_bracket = matches!(
                    tokens[offset].kind,
                    TokenKind::Punctuator(
                        Punctuator::Less
                            | Punctuator::Greater
                            | Punctuator::GreaterGreater
                            | Punctuator::GreaterGreaterGreater
                    )
                );
                *mark = if is_bracket {
                    Angle::Bracket
                } else {
                    Angle::Inside
                };
            }
            prev = Some(tokens[close].kind);
            index = close + 1;
            continue;
        }

        prev = Some(kind);
        index += 1;
    }

    marks
}

/// Find the `>` that closes the type-argument list opening at `start`.
fn scan_type_arguments(tokens: &[Token], start: usize) -> Option<usize> {
    let mut depth: u32 = 1;
    let mut budget = TYPE_ARGUMENT_SCAN_BUDGET;
    let mut index = start + 1;
    let mut saw_content = false;

    while index < tokens.len() {
        let kind = tokens[index].kind;
        if kind.is_trivia() {
            index += 1;
            continue;
        }
        if budget == 0 {
            return None;
        }
        budget -= 1;

        match kind {
            TokenKind::Identifier | TokenKind::Number | TokenKind::String => saw_content = true,
            // `extends` bounds a type parameter and `in` marks a contravariant
            // one — both sit inside the brackets in modern Flow, and both
            // replace syntax that is now deprecated (`<T: Bound>` and `<-T>`).
            // Rejecting them made the scan give up and print the brackets as
            // comparisons, so the formatter mangled exactly the spellings uf
            // tells users to write. (`out`, the covariant sigil, lexes as an
            // identifier and was already accepted.)
            TokenKind::Keyword(
                Keyword::Typeof
                | Keyword::Void
                | Keyword::Null
                | Keyword::True
                | Keyword::False
                | Keyword::This
                | Keyword::Static
                | Keyword::Interface
                | Keyword::Extends
                | Keyword::In,
            ) => saw_content = true,
            TokenKind::Punctuator(punctuator) => match punctuator {
                Punctuator::Less => {
                    depth += 1;
                    saw_content = true;
                }
                Punctuator::Greater
                | Punctuator::GreaterGreater
                | Punctuator::GreaterGreaterGreater => {
                    let closes = punctuator.angle_close_count();
                    if closes > depth {
                        return None;
                    }
                    depth -= closes;
                    if depth == 0 {
                        if !saw_content {
                            return None;
                        }
                        return type_arguments_may_precede(tokens, index).then_some(index);
                    }
                }
                Punctuator::Dot
                | Punctuator::Comma
                | Punctuator::Pipe
                | Punctuator::Amp
                | Punctuator::Question
                | Punctuator::Colon
                | Punctuator::OpenBracket
                | Punctuator::CloseBracket
                | Punctuator::OpenBrace
                | Punctuator::CloseBrace
                | Punctuator::OpenParen
                | Punctuator::CloseParen
                | Punctuator::Arrow
                | Punctuator::Ellipsis
                | Punctuator::Star
                | Punctuator::Plus
                | Punctuator::Minus
                | Punctuator::Equal => saw_content = true,
                _ => return None,
            },
            _ => return None,
        }

        index += 1;
    }

    None
}

/// Whether the token after a closing `>` is compatible with a type application.
fn type_arguments_may_precede(tokens: &[Token], close: usize) -> bool {
    let Some(next) = tokens[close + 1..]
        .iter()
        .find(|token| !token.kind.is_trivia())
    else {
        return true;
    };

    match next.kind {
        TokenKind::Punctuator(punctuator) => matches!(
            punctuator,
            Punctuator::OpenParen
                | Punctuator::CloseParen
                | Punctuator::OpenBrace
                | Punctuator::CloseBrace
                | Punctuator::OpenBracket
                | Punctuator::CloseBracket
                | Punctuator::Comma
                | Punctuator::Semicolon
                | Punctuator::Colon
                | Punctuator::Equal
                | Punctuator::Arrow
                | Punctuator::Pipe
                | Punctuator::Amp
                | Punctuator::Question
                | Punctuator::Dot
                | Punctuator::Ellipsis
                | Punctuator::Greater
                | Punctuator::GreaterGreater
                | Punctuator::GreaterGreaterGreater
        ),
        TokenKind::TemplateFull | TokenKind::TemplateHead => true,
        TokenKind::Keyword(keyword) => matches!(
            keyword,
            Keyword::Extends | Keyword::Implements | Keyword::From | Keyword::Renders
        ),
        _ => false,
    }
}
