//! The first pass: everything about the layout that does not depend on the
//! output column.
//!
//! One linear walk over the tokens records, per token, the space in front of it,
//! the indentation a line starting there would take, its flat printed width, and
//! -- for opening delimiters -- where the group closes and whether it may be
//! exploded. The emit pass then only has to decide what will not fit.

use super::NO_MATCH;
use super::angle::{Angle, mark_type_angles};
use super::frame::{Frame, FrameKind, close_group, indent_for, pop_frame, push_frame};
use super::spacing::{Role, hugs_left, role_of, wants_space};
use crate::lexer::{
    BraceKind, GroupKind, Keyword, Prev, Punctuator, Token, TokenKind, classify_brace,
};

/// Per-token layout decisions produced by [`analyze`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Anno {
    /// Whether a single space separates this token from the previous one.
    pub(crate) space_before: bool,
    /// Whether this opening delimiter may be exploded across several lines.
    pub(crate) breakable: bool,
    /// Whether this whitespace token is significant JSX character data.
    pub(crate) jsx_space: bool,
    /// Whether this `)` closes an `if`/`for`/`while`/`catch`/`switch` header.
    pub(crate) statement_paren: bool,
    /// Whether this opening delimiter encloses statements rather than operands.
    pub(crate) statement_group: bool,
    /// Whether this `}` closes an object literal or object type, which may end a
    /// statement, rather than a block, which may not.
    pub(crate) object_close: bool,
    /// Whether this token sits inside a type-argument list such as `Array<T>`.
    pub(crate) in_angle: bool,
    /// Whether this token is still inside a JSX element after it is printed.
    pub(crate) in_jsx: bool,
    /// Printed width of the token, measured from its last line.
    pub(crate) width: u32,
    /// Whether the token's text spans more than one line.
    pub(crate) multiline: bool,
    /// Indentation level to use when this token starts a line.
    pub(crate) indent: u16,
    /// Index of the matching closing delimiter, or [`NO_MATCH`].
    pub(crate) close: u32,
}

impl Default for Anno {
    fn default() -> Self {
        Self {
            space_before: false,
            breakable: false,
            jsx_space: false,
            statement_paren: false,
            statement_group: false,
            object_close: false,
            in_angle: false,
            in_jsx: false,
            width: 0,
            multiline: false,
            indent: 0,
            close: NO_MATCH,
        }
    }
}

/// Everything the emit pass needs to know about the token stream.
pub(crate) struct Analysis {
    pub(crate) annos: Vec<Anno>,
    /// Prefix sums of the flat printed width of each token.
    pub(crate) cost: Vec<u32>,
    /// For each index, the index of the next newline token at or after it.
    pub(crate) line_end: Vec<u32>,
}

/// Walk the tokens once and record every layout decision that does not depend on
/// the output column.
#[allow(
    clippy::too_many_lines,
    reason = "one linear pass over the token kinds"
)]
pub(crate) fn analyze(source: &str, tokens: &[Token]) -> Analysis {
    let count = tokens.len();
    let angles = mark_type_angles(tokens);
    let mut annos = vec![Anno::default(); count];
    let mut cost = vec![0u32; count + 1];
    let mut line_end = vec![u32::try_from(count).unwrap_or(u32::MAX); count + 1];

    let mut frames: Vec<Frame> = Vec::with_capacity(16);
    let mut brace_stack: Vec<BraceKind> = Vec::with_capacity(16);
    let mut indent_level: u16 = 0;
    let mut jsx_depth: u32 = 0;
    let mut root_ternary: u32 = 0;
    let mut switch_depth: u16 = 0;
    let mut prev: Option<Prev> = None;
    let mut prev_role = Role::Normal;
    let mut after_comment = false;
    let mut pending_body: Option<BraceKind> = None;

    for index in 0..count {
        let token = tokens[index];
        let kind = token.kind;
        let text = token.text(source);
        let frame = frames.last().copied();

        if kind == TokenKind::Newline {
            if let Some(frame) = frames.last_mut()
                && !frame.has_own_newline
            {
                frame.has_own_newline = true;
                indent_level = indent_level.saturating_add(1);
            }
            cost[index + 1] = cost[index];
            after_comment = false;
            continue;
        }

        let width = display_width(text);
        annos[index].width = width;
        if kind == TokenKind::Whitespace {
            let jsx_space = frame.is_some_and(|frame| frame.kind.is_jsx_children());
            annos[index].jsx_space = jsx_space;
            cost[index + 1] = cost[index] + if jsx_space { width } else { 0 };
            continue;
        }

        // A token that spans lines (a multi-line template or block comment) keeps
        // its group from being reflowed, but does not indent anything.
        if memchr::memchr(b'\n', text.as_bytes()).is_some() {
            annos[index].multiline = true;
            if let Some(frame) = frames.last_mut() {
                frame.has_newline = true;
            }
        }

        let next_significant = tokens[index + 1..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map(|token| token.kind);
        let ternary_depth = frames
            .last()
            .map_or(root_ternary, |frame| frame.ternary_depth);
        let role = role_of(kind, angles[index], prev, next_significant, ternary_depth);
        let space_before = if prev.is_none() {
            false
        } else {
            (after_comment && !hugs_left(kind)) || wants_space(prev, prev_role, kind, role, frame)
        };

        annos[index].space_before = space_before;
        annos[index].in_angle = angles[index] != Angle::No;
        annos[index].indent = indent_for(kind, next_significant, frame, indent_level, switch_depth);
        cost[index + 1] = cost[index]
            .saturating_add(width)
            .saturating_add(u32::from(space_before));

        // A separator only counts as "top level" when neither a nested group nor
        // a type-argument list stands between it and the enclosing delimiter.
        if matches!(
            kind,
            TokenKind::Punctuator(Punctuator::Comma | Punctuator::Semicolon)
        ) && angles[index] == Angle::No
            && let Some(frame) = frames.last_mut()
        {
            frame.has_separator = true;
        }

        let mut group = GroupKind::None;
        match kind {
            TokenKind::Punctuator(Punctuator::OpenParen) => {
                let statement_header = matches!(
                    prev.map(|prev| prev.kind),
                    Some(TokenKind::Keyword(keyword)) if keyword.starts_statement_header()
                );
                push_frame(&mut frames, FrameKind::Paren { statement_header }, index);
            }
            TokenKind::Punctuator(Punctuator::OpenBracket) => {
                push_frame(&mut frames, FrameKind::Bracket, index);
            }
            TokenKind::Punctuator(Punctuator::OpenBrace) => {
                let brace = match frame.map(|frame| frame.kind) {
                    Some(kind) if kind.is_jsx() => BraceKind::JsxExpression,
                    _ => pending_body
                        .take()
                        .unwrap_or_else(|| classify_brace(prev, brace_stack.last().copied())),
                };
                brace_stack.push(brace);
                if brace == BraceKind::Switch {
                    switch_depth = switch_depth.saturating_add(1);
                }
                push_frame(&mut frames, FrameKind::Brace(brace), index);
            }
            TokenKind::Punctuator(Punctuator::CloseParen) => {
                if let Some(frame) = pop_frame(&mut frames, &mut indent_level, &mut jsx_depth) {
                    annos[index].statement_paren = matches!(
                        frame.kind,
                        FrameKind::Paren {
                            statement_header: true
                        }
                    );
                    close_group(&mut annos, &frame, index, &angles);
                }
                group = if annos[index].statement_paren {
                    GroupKind::StatementParen
                } else {
                    GroupKind::ExpressionParen
                };
            }
            TokenKind::Punctuator(Punctuator::CloseBracket) => {
                if let Some(frame) = pop_frame(&mut frames, &mut indent_level, &mut jsx_depth) {
                    close_group(&mut annos, &frame, index, &angles);
                }
            }
            TokenKind::Punctuator(Punctuator::CloseBrace) => {
                let brace = brace_stack.pop().unwrap_or(BraceKind::Block);
                if brace == BraceKind::Switch {
                    switch_depth = switch_depth.saturating_sub(1);
                }
                annos[index].object_close = brace == BraceKind::Object;
                if let Some(frame) = pop_frame(&mut frames, &mut indent_level, &mut jsx_depth) {
                    close_group(&mut annos, &frame, index, &angles);
                }
                group = GroupKind::Brace(brace);
            }
            TokenKind::TemplateHead => {
                push_frame(&mut frames, FrameKind::Template, index);
            }
            TokenKind::TemplateTail => {
                pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
            }
            TokenKind::JsxOpenStart => {
                jsx_depth += 1;
                push_frame(&mut frames, FrameKind::JsxTag { closing: false }, index);
            }
            TokenKind::JsxCloseStart => {
                jsx_depth += 1;
                push_frame(&mut frames, FrameKind::JsxTag { closing: true }, index);
            }
            TokenKind::JsxTagEnd => {
                let closing = matches!(
                    frames.last().map(|frame| frame.kind),
                    Some(FrameKind::JsxTag { closing: true })
                );
                pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
                if closing {
                    pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
                } else {
                    jsx_depth += 1;
                    push_frame(&mut frames, FrameKind::JsxChildren, index);
                }
            }
            TokenKind::JsxSelfClose => {
                pop_frame(&mut frames, &mut indent_level, &mut jsx_depth);
            }
            TokenKind::Keyword(keyword) => match keyword {
                Keyword::Class | Keyword::Interface | Keyword::Enum => {
                    pending_body = Some(BraceKind::Class);
                }
                Keyword::Switch => pending_body = Some(BraceKind::Switch),
                _ => {}
            },
            _ => {}
        }

        annos[index].in_jsx = jsx_depth > 0;

        if role == Role::TernaryColon {
            match frames.last_mut() {
                Some(frame) => frame.ternary_depth = frame.ternary_depth.saturating_sub(1),
                None => root_ternary = root_ternary.saturating_sub(1),
            }
        } else if kind == TokenKind::Punctuator(Punctuator::Question)
            && role == Role::Normal
            && angles[index] == Angle::No
        {
            match frames.last_mut() {
                Some(frame) => frame.ternary_depth = frame.ternary_depth.saturating_add(1),
                None => root_ternary = root_ternary.saturating_add(1),
            }
        }

        if kind.is_comment() {
            after_comment = true;
        } else {
            after_comment = false;
            prev = Some(Prev { kind, group });
            prev_role = role;
        }
    }

    for index in (0..count).rev() {
        line_end[index] = if tokens[index].kind == TokenKind::Newline {
            u32::try_from(index).unwrap_or(u32::MAX)
        } else {
            line_end[index + 1]
        };
    }

    Analysis {
        annos,
        cost,
        line_end,
    }
}

/// Printed width of a token, measured from the last line it covers.
pub(crate) fn display_width(text: &str) -> u32 {
    let tail = match memchr::memrchr(b'\n', text.as_bytes()) {
        Some(offset) => &text[offset + 1..],
        None => text,
    };
    // Source is overwhelmingly ASCII, where the byte length is the width.
    let width = if tail.is_ascii() {
        tail.len()
    } else {
        tail.chars().count()
    };
    u32::try_from(width).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_counts_characters_after_the_last_newline() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("ab\ncde"), 3);
        assert_eq!(display_width("日本"), 2);
    }
}
