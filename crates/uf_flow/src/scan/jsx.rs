//! The JSX modes: when `<` opens an element, and what the bytes mean after it.
//!
//! JSX is a lexical mode rather than extra grammar. Between `>` and `</` the
//! same bytes that are operators in JavaScript are text, `it's` is an
//! apostrophe rather than the start of a string, and `/` is a slash rather than
//! a regular expression. A frame stack is what tells those apart, and it is
//! bounded by [`MAX_JSX_DEPTH`] because source comes from `node_modules`.
//!
//! Whether a `<` opens an element or compares two numbers is the same question
//! as whether a `/` opens a regular expression, and it gets the same answer:
//! yes exactly where an expression may begin. A lexer cannot do better than
//! that without a parser telling it the context, which is why
//! [`tokenize`](super::tokenize) exists for callers who must not apply the
//! heuristic at all.

use super::lexer::{is_ident_part, is_ident_start};
use super::{Token, TokenKind};

/// Whether the scanner recognizes JSX at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JsxMode {
    Off,
    On,
}

/// Where the scan currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Frame {
    /// Ordinary JavaScript. `braces` counts the `{` opened since this frame
    /// began, so the `}` that closes a JSX expression container is the one
    /// that would take the count below zero.
    Js { braces: usize },
    /// Inside `<tag …>`, `</tag>` or `<tag … />`.
    Tag { closing: bool },
    /// Between an opening tag and its closing tag.
    Children,
}

/// How deeply JSX may nest before the scanner gives up on it.
///
/// Source comes from `node_modules`, and the frame stack is the one thing here
/// that grows with the input. Past the ceiling the scanner stops opening new
/// frames and lexes on as JavaScript, which fails open in the usual way.
pub(super) const MAX_JSX_DEPTH: usize = 256;

/// Update the frame stack for the token just pushed.
pub(super) fn step_frames(frames: &mut Vec<Frame>, tokens: &mut [Token], source: &str) {
    let Some(&token) = tokens.last() else {
        return;
    };
    let Some(&frame) = frames.last() else {
        return;
    };

    match frame {
        Frame::Js { braces } => match token.kind {
            TokenKind::Punct(b'{') => replace_top(frames, Frame::Js { braces: braces + 1 }),
            TokenKind::Punct(b'}') => {
                if braces == 0 {
                    // The `}` closing a JSX expression container.
                    frames.pop();
                } else {
                    replace_top(frames, Frame::Js { braces: braces - 1 });
                }
            }
            TokenKind::Punct(b'<') if opens_element(tokens, source) => {
                mark_top(tokens, TokenKind::JsxTagOpen);
                push_frame(frames, Frame::Tag { closing: false });
            }
            _ => {}
        },
        Frame::Tag { closing } => match token.kind {
            // The `/` of `</`, directly after the `<` that opened this tag.
            TokenKind::Punct(b'/') if is_tag_start(tokens) => {
                replace_top(frames, Frame::Tag { closing: true });
            }
            TokenKind::Punct(b'>') => {
                let self_closing = tokens
                    .len()
                    .checked_sub(2)
                    .is_some_and(|index| tokens[index].is_punct(b'/'));
                mark_top(tokens, TokenKind::JsxTagClose);
                frames.pop();
                if closing {
                    // `</tag>` ends the element, so the children frame that
                    // tag belonged to goes with it.
                    if frames.last() == Some(&Frame::Children) {
                        frames.pop();
                    }
                } else if !self_closing {
                    // `<tag>` opens children; `<tag />` opens nothing and
                    // simply returns to whatever frame held it.
                    push_frame(frames, Frame::Children);
                }
            }
            TokenKind::Punct(b'{') => push_frame(frames, Frame::Js { braces: 0 }),
            _ => {}
        },
        Frame::Children => {}
    }
}

fn mark_top(tokens: &mut [Token], kind: TokenKind) {
    if let Some(top) = tokens.last_mut() {
        top.kind = kind;
    }
}

fn push_frame(frames: &mut Vec<Frame>, frame: Frame) {
    if frames.len() < MAX_JSX_DEPTH {
        frames.push(frame);
    }
}

fn replace_top(frames: &mut [Frame], frame: Frame) {
    if let Some(top) = frames.last_mut() {
        *top = frame;
    }
}

/// Whether the last two tokens are the `<` and `/` of a closing tag.
fn is_tag_start(tokens: &[Token]) -> bool {
    let Some(last) = tokens.len().checked_sub(1) else {
        return false;
    };
    last.checked_sub(1).is_some_and(|index| {
        tokens[index].kind == TokenKind::JsxTagOpen && tokens[index].end == tokens[last].start
    })
}

/// Whether the `<` just pushed opens a JSX element rather than comparing.
///
/// The same question, and the same answer, as "is this `/` a regular
/// expression": a `<` opens an element exactly where an expression may begin.
fn opens_element(tokens: &[Token], source: &str) -> bool {
    let Some(previous) = tokens.len().checked_sub(2).map(|index| tokens[index]) else {
        return true;
    };
    match previous.kind {
        TokenKind::Arrow => true,
        TokenKind::Ident => matches!(
            previous.text(source),
            "await"
                | "case"
                | "default"
                | "do"
                | "else"
                | "in"
                | "of"
                | "return"
                | "typeof"
                | "void"
                | "yield"
        ),
        TokenKind::Punct(byte) => !matches!(byte, b')' | b']' | b'}' | b'>'),
        _ => false,
    }
}

/// Lex one token inside `< … >`.
///
/// Two differences from JavaScript: a `/` is never a regular expression (it is
/// the `/` of `</` or `/>`), and an attribute string may span lines and hold
/// the other quote, because JSX string values are raw.
pub(super) fn lex_tag(bytes: &[u8], cursor: &mut usize) -> TokenKind {
    match bytes[*cursor] {
        quote @ (b'"' | b'\'') => lex_attribute_string(bytes, cursor, quote),
        b'{' | b'}' | b'<' | b'>' | b'/' | b'=' | b'.' | b':' | b'-' => {
            let byte = bytes[*cursor];
            *cursor += 1;
            TokenKind::Punct(byte)
        }
        byte if is_ident_start(byte) => {
            *cursor += 1;
            while *cursor < bytes.len() && is_ident_part(bytes[*cursor]) {
                *cursor += 1;
            }
            TokenKind::Ident
        }
        byte => {
            *cursor += 1;
            TokenKind::Punct(byte)
        }
    }
}

/// A JSX attribute value: raw bytes up to the matching quote.
fn lex_attribute_string(bytes: &[u8], cursor: &mut usize, quote: u8) -> TokenKind {
    let mut at = *cursor + 1;
    while at < bytes.len() {
        if bytes[at] == quote {
            *cursor = at + 1;
            return TokenKind::String;
        }
        at += 1;
    }
    *cursor = bytes.len();
    TokenKind::Invalid
}

/// Lex one token between an opening and a closing tag.
///
/// Everything that is not `<` or `{` is text, including whitespace, newlines,
/// quotes and operators.
pub(super) fn lex_children(bytes: &[u8], cursor: &mut usize, frames: &mut Vec<Frame>) -> TokenKind {
    match bytes[*cursor] {
        b'<' => {
            *cursor += 1;
            if frames.len() < MAX_JSX_DEPTH {
                frames.push(Frame::Tag { closing: false });
            }
            TokenKind::JsxTagOpen
        }
        b'{' => {
            *cursor += 1;
            if frames.len() < MAX_JSX_DEPTH {
                frames.push(Frame::Js { braces: 0 });
            }
            TokenKind::Punct(b'{')
        }
        _ => {
            while *cursor < bytes.len() && !matches!(bytes[*cursor], b'<' | b'{') {
                *cursor += 1;
            }
            TokenKind::JsxText
        }
    }
}
