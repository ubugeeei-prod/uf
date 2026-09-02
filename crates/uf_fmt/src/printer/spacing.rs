//! Which pairs of adjacent tokens are separated by a space.
//!
//! Spacing is not a property of a token but of the role it plays: the same `-` is
//! a binary operator in `a - b` and a prefix in `-1`, and the same `?` is a
//! ternary head, an optional-property marker or a Flow nullable prefix. This
//! module resolves the role from the neighbouring tokens first, then answers the
//! spacing question from the pair of roles.

use super::angle::Angle;
use super::frame::{Frame, FrameKind};
use crate::lexer::{Keyword, Prev, Punctuator, TokenKind, expression_allowed};

/// The syntactic role of a token, which decides the spacing around it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Normal,
    /// A prefix operator: `!x`, `-1`, `...rest`, `?number`, `+variance`.
    Prefix,
    /// The `<` that opens a type-argument list.
    TypeAngleOpen,
    /// The `>` (or `>>`) that closes a type-argument list.
    TypeAngleClose,
    /// The `:` of a conditional expression.
    TernaryColon,
    /// The `?` of `name?: T` or `name?,`.
    OptionalMarker,
}

/// Resolve the syntactic role of a token from its neighbours.
pub(crate) fn role_of(
    kind: TokenKind,
    angle: Angle,
    prev: Option<Prev>,
    next: Option<TokenKind>,
    ternary_depth: u32,
) -> Role {
    if angle == Angle::Bracket {
        return if kind == TokenKind::Punctuator(Punctuator::Less) {
            Role::TypeAngleOpen
        } else {
            Role::TypeAngleClose
        };
    }

    let TokenKind::Punctuator(punctuator) = kind else {
        return Role::Normal;
    };

    match punctuator {
        Punctuator::Bang | Punctuator::Tilde | Punctuator::Ellipsis | Punctuator::At => {
            Role::Prefix
        }
        // `function* gen()` and `yield* other()` keep the star attached on the
        // left instead, so they are not prefix operators.
        Punctuator::Star
            if matches!(
                prev.map(|prev| prev.kind),
                Some(TokenKind::Keyword(Keyword::Function | Keyword::Yield))
            ) =>
        {
            Role::Normal
        }
        Punctuator::Plus
        | Punctuator::Minus
        | Punctuator::PlusPlus
        | Punctuator::MinusMinus
        | Punctuator::Star
            if expression_allowed(prev) =>
        {
            Role::Prefix
        }
        Punctuator::Colon => {
            if ternary_depth > 0 {
                Role::TernaryColon
            } else {
                Role::Normal
            }
        }
        Punctuator::Question => {
            let optional = matches!(
                next,
                Some(TokenKind::Punctuator(
                    Punctuator::Colon
                        | Punctuator::Comma
                        | Punctuator::CloseParen
                        | Punctuator::CloseBracket
                        | Punctuator::CloseBrace
                        | Punctuator::Equal
                ))
            );
            if optional {
                return Role::OptionalMarker;
            }
            // `?string`, `Array<?T>` and `(x: ?number)` are nullable types, so the
            // `?` is a prefix rather than the head of a conditional expression.
            let prefix = matches!(
                prev.map(|prev| prev.kind),
                Some(TokenKind::Punctuator(
                    Punctuator::Colon
                        | Punctuator::Equal
                        | Punctuator::Pipe
                        | Punctuator::Amp
                        | Punctuator::Comma
                        | Punctuator::OpenParen
                        | Punctuator::OpenBracket
                        | Punctuator::Less
                        | Punctuator::Arrow
                        | Punctuator::Question
                        | Punctuator::Ellipsis
                ))
            );
            if prefix { Role::Prefix } else { Role::Normal }
        }
        _ => Role::Normal,
    }
}

/// Whether a single space separates `prev` from the current token.
#[allow(clippy::too_many_lines, reason = "a flat table of spacing rules")]
pub(crate) fn wants_space(
    prev: Option<Prev>,
    prev_role: Role,
    kind: TokenKind,
    role: Role,
    frame: Option<Frame>,
) -> bool {
    let Some(prev) = prev else {
        return false;
    };
    let prev_kind = prev.kind;
    let frame_kind = frame.map(|frame| frame.kind);

    // JSX character data carries its own whitespace verbatim.
    if frame_kind.is_some_and(FrameKind::is_jsx_children) {
        return false;
    }
    if frame_kind.is_some_and(FrameKind::is_jsx_tag) {
        return jsx_tag_space(prev_kind, kind);
    }

    // Flow exact object types: `{| … |}`.
    if prev_kind == TokenKind::Punctuator(Punctuator::OpenBrace)
        && kind == TokenKind::Punctuator(Punctuator::Pipe)
    {
        return false;
    }
    if prev_kind == TokenKind::Punctuator(Punctuator::Pipe)
        && kind == TokenKind::Punctuator(Punctuator::CloseBrace)
    {
        return false;
    }

    if matches!(
        prev_kind,
        TokenKind::Punctuator(Punctuator::OpenParen | Punctuator::OpenBracket)
    ) {
        return false;
    }
    if matches!(
        kind,
        TokenKind::Punctuator(Punctuator::CloseParen | Punctuator::CloseBracket)
    ) {
        return false;
    }

    if prev_kind == TokenKind::Punctuator(Punctuator::OpenBrace) {
        if kind == TokenKind::Punctuator(Punctuator::CloseBrace) {
            return false;
        }
        return frame_kind.is_some_and(FrameKind::pads_braces);
    }
    if kind == TokenKind::Punctuator(Punctuator::CloseBrace) {
        return frame_kind.is_some_and(FrameKind::pads_braces);
    }

    if matches!(prev_role, Role::Prefix | Role::TypeAngleOpen) {
        return false;
    }
    if matches!(
        role,
        Role::TypeAngleOpen | Role::TypeAngleClose | Role::OptionalMarker
    ) {
        return false;
    }
    // `useState<number>(0)` and `Array<T>[]` keep the call or index attached to
    // the closing angle bracket; everything else after `>` is spaced normally.
    if prev_role == Role::TypeAngleClose
        && matches!(
            kind,
            TokenKind::Punctuator(Punctuator::OpenParen | Punctuator::OpenBracket)
                | TokenKind::TemplateFull
                | TokenKind::TemplateHead
        )
    {
        return false;
    }
    if matches!(
        prev_kind,
        TokenKind::Punctuator(Punctuator::Dot | Punctuator::QuestionDot)
            | TokenKind::TemplateHead
            | TokenKind::TemplateMiddle
    ) {
        return false;
    }

    match kind {
        TokenKind::Punctuator(
            Punctuator::Comma | Punctuator::Semicolon | Punctuator::Dot | Punctuator::QuestionDot,
        ) => false,
        TokenKind::Punctuator(Punctuator::Colon) => role == Role::TernaryColon,
        TokenKind::TemplateMiddle | TokenKind::TemplateTail => false,
        TokenKind::Punctuator(Punctuator::OpenParen) => !callable_prefix(prev_kind),
        TokenKind::Punctuator(Punctuator::OpenBracket)
        | TokenKind::TemplateFull
        | TokenKind::TemplateHead
        | TokenKind::Punctuator(Punctuator::PlusPlus | Punctuator::MinusMinus) => {
            !indexable_prefix(prev_kind)
        }
        TokenKind::Punctuator(Punctuator::Star) => !matches!(
            prev_kind,
            TokenKind::Keyword(Keyword::Function | Keyword::Yield)
        ),
        _ => true,
    }
}

/// Whether a token always sits flush against whatever precedes it, so that a
/// preceding comment does not force a space in front of it.
pub(crate) fn hugs_left(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Punctuator(
            Punctuator::CloseParen
                | Punctuator::CloseBracket
                | Punctuator::Comma
                | Punctuator::Semicolon
                | Punctuator::Dot
                | Punctuator::QuestionDot
        ) | TokenKind::JsxTagEnd
            | TokenKind::JsxSelfClose
    )
}

/// Spacing rules inside a JSX tag, where `=` binds attributes tightly.
fn jsx_tag_space(prev_kind: TokenKind, kind: TokenKind) -> bool {
    if matches!(
        prev_kind,
        TokenKind::JsxOpenStart | TokenKind::JsxCloseStart
    ) {
        return false;
    }
    if kind == TokenKind::JsxTagEnd {
        return false;
    }
    if kind == TokenKind::Punctuator(Punctuator::Equal)
        || prev_kind == TokenKind::Punctuator(Punctuator::Equal)
    {
        return false;
    }
    if prev_kind == TokenKind::Punctuator(Punctuator::OpenBrace)
        || kind == TokenKind::Punctuator(Punctuator::CloseBrace)
        || prev_kind == TokenKind::Punctuator(Punctuator::Slash)
    {
        return false;
    }
    true
}

/// Whether `(` directly after this token is a call rather than a grouping.
fn callable_prefix(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::PrivateName
            | TokenKind::Punctuator(Punctuator::CloseParen | Punctuator::CloseBracket)
            | TokenKind::Keyword(Keyword::Super | Keyword::Import)
    )
}

/// Whether `[` directly after this token is an index rather than a literal.
fn indexable_prefix(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::PrivateName
            | TokenKind::Number
            | TokenKind::String
            | TokenKind::TemplateFull
            | TokenKind::TemplateTail
            | TokenKind::Punctuator(Punctuator::CloseParen | Punctuator::CloseBracket)
            | TokenKind::Keyword(Keyword::This | Keyword::Super)
    )
}
