//! The bracket-nesting stack, and the indentation level it hands to each line.
//!
//! A frame remembers where its group opened and whether the author already broke
//! it across lines, which is what decides both the indentation of the lines
//! inside it and whether the printer is still allowed to reflow it.

use super::analyze::Anno;
use super::angle::Angle;
use super::{MAX_INDENT_LEVELS, NO_MATCH};
use crate::lexer::{BraceKind, Keyword, Punctuator, TokenKind};

/// A bracketed region tracked while analysing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) kind: FrameKind,
    open: u32,
    pub(crate) ternary_depth: u32,
    /// A newline occurred somewhere inside this group.
    pub(crate) has_newline: bool,
    /// A newline occurred at this group's own level rather than inside a nested
    /// group, which is what earns the group an indentation level.
    pub(crate) has_own_newline: bool,
    pub(crate) has_separator: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameKind {
    Paren { statement_header: bool },
    Bracket,
    Brace(BraceKind),
    Template,
    JsxTag { closing: bool },
    JsxChildren,
}

impl FrameKind {
    /// Whether `{ … }` of this flavour keeps a space inside its braces.
    pub(crate) const fn pads_braces(self) -> bool {
        matches!(
            self,
            FrameKind::Brace(BraceKind::Block | BraceKind::Object | BraceKind::Class)
        )
    }

    /// Whether the region holds statements, which may take a semicolon.
    pub(crate) const fn holds_statements(self) -> bool {
        matches!(
            self,
            FrameKind::Brace(BraceKind::Block | BraceKind::Class | BraceKind::Switch)
        )
    }

    pub(crate) const fn is_jsx_children(self) -> bool {
        matches!(self, FrameKind::JsxChildren)
    }

    pub(crate) const fn is_jsx_tag(self) -> bool {
        matches!(self, FrameKind::JsxTag { .. })
    }

    pub(crate) const fn is_jsx(self) -> bool {
        self.is_jsx_tag() || self.is_jsx_children()
    }
}

pub(crate) fn push_frame(frames: &mut Vec<Frame>, kind: FrameKind, open: usize) {
    frames.push(Frame {
        kind,
        open: u32::try_from(open).unwrap_or(NO_MATCH),
        ternary_depth: 0,
        has_newline: false,
        has_own_newline: false,
        has_separator: false,
    });
}

/// Pop the innermost frame, if any.
///
/// Unbalanced sources are formatted rather than rejected, so a stray closing
/// delimiter simply closes whatever frame happens to be open.
pub(crate) fn pop_frame(
    frames: &mut Vec<Frame>,
    indent_level: &mut u16,
    jsx_depth: &mut u32,
) -> Option<Frame> {
    let frame = frames.pop()?;
    if frame.kind.is_jsx() {
        *jsx_depth = jsx_depth.saturating_sub(1);
    }
    if frame.has_own_newline {
        *indent_level = indent_level.saturating_sub(1);
    }
    // Newlines inside a closed group still count as newlines inside its parent,
    // which is what makes the parent unbreakable.
    if let Some(parent) = frames.last_mut() {
        parent.has_newline |= frame.has_newline || frame.has_own_newline;
    }
    Some(frame)
}

pub(crate) fn close_group(annos: &mut [Anno], frame: &Frame, close: usize, angles: &[Angle]) {
    let open = frame.open as usize;
    if open >= annos.len() {
        return;
    }
    annos[open].close = u32::try_from(close).unwrap_or(NO_MATCH);
    annos[open].statement_group = frame.kind.holds_statements();
    annos[open].breakable = frame.has_separator
        && !frame.has_newline
        && !frame.has_own_newline
        && !frame.kind.is_jsx()
        && angles[open] == Angle::No;
}

/// Indentation level for a line that starts with `kind`.
pub(crate) fn indent_for(
    kind: TokenKind,
    next: Option<TokenKind>,
    frame: Option<Frame>,
    indent_level: u16,
    switch_depth: u16,
) -> u16 {
    let top = frame.map(|frame| frame.kind);
    let dedents = match kind {
        TokenKind::Punctuator(punctuator) if punctuator.is_close_delimiter() => true,
        // The trailing `|` of a Flow exact object type belongs to the closing
        // brace, not to the body.
        TokenKind::Punctuator(Punctuator::Pipe) => {
            next == Some(TokenKind::Punctuator(Punctuator::CloseBrace))
        }
        TokenKind::JsxTagEnd | TokenKind::JsxSelfClose => top.is_some_and(FrameKind::is_jsx_tag),
        TokenKind::JsxCloseStart => top.is_some_and(FrameKind::is_jsx_children),
        TokenKind::TemplateMiddle | TokenKind::TemplateTail => {
            matches!(top, Some(FrameKind::Template))
        }
        _ => false,
    };

    let innermost_is_switch = matches!(top, Some(FrameKind::Brace(BraceKind::Switch)));
    // Statements inside a `switch` body sit one level deeper than its `case` and
    // `default` labels, and the offset accumulates through nested switches.
    let mut level = indent_level.saturating_add(switch_depth);
    if innermost_is_switch
        && (dedents || matches!(kind, TokenKind::Keyword(Keyword::Case | Keyword::Default)))
    {
        level = level.saturating_sub(1);
    }

    if dedents {
        // Only groups that broke at their own level contributed a level to undo.
        if frame.is_some_and(|frame| frame.has_own_newline) {
            level = level.saturating_sub(1);
        }
    } else if matches!(
        kind,
        TokenKind::Punctuator(Punctuator::Dot | Punctuator::QuestionDot)
    ) {
        // A line that starts with `.` continues the previous expression.
        level = level.saturating_add(1);
    }
    level.min(MAX_INDENT_LEVELS)
}
