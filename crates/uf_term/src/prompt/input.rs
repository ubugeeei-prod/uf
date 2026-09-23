//! Asking the reader to type one value.
//!
//! The same rules as [`super::select`], for a question with no list of
//! answers: refused unless a person is watching, drawn on stderr, redrawn in
//! place, and erased once it is answered. The line is deliberately small — no
//! cursor movement inside it, no history — because it asks for one word, and
//! `Backspace` and `Ctrl-U` are all the editing one word needs.

use std::io::{self, Write};

use crate::capability::{Capabilities, ColorChoice, TerminalEnv};
use crate::glyph::Glyphs;
use crate::theme::Theme;

use super::key::{Key, read_key};
use super::raw::RawMode;
use super::{Keys, erase, is_interactive, redraw_until};

/// What to ask for.
#[derive(Debug, Clone, Copy)]
pub struct Question<'a> {
    /// The question, shown above the line.
    pub title: &'a str,
    /// The hint shown in the line while it is empty.
    pub placeholder: &'a str,
    /// How much colour to use.
    pub color: ColorChoice,
}

impl<'a> Question<'a> {
    /// A question with automatic colour.
    pub fn new(title: &'a str, placeholder: &'a str) -> Self {
        Self {
            title,
            placeholder,
            color: ColorChoice::Auto,
        }
    }
}

/// How the question ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The reader typed this and pressed `Enter`. Never empty.
    Typed(String),
    /// The reader left without answering: `Escape`, `Ctrl-C`, or a closed
    /// input.
    Cancelled,
    /// There was nobody to ask.
    NotInteractive,
}

/// Ask, and return what was typed.
///
/// An empty line is not an answer: `Enter` on one does nothing, and the
/// placeholder stays where the value will go. A question that may be left
/// unanswered is one the caller should not be asking.
pub fn input(question: &Question<'_>) -> Answer {
    if !is_interactive() {
        return Answer::NotInteractive;
    }
    let Ok(raw) = RawMode::enter() else {
        return Answer::NotInteractive;
    };
    let theme = Theme::default();
    let capabilities = Capabilities::for_stderr(question.color, &TerminalEnv::from_process());
    let mut stderr = io::stderr();
    let answer = ask(
        &mut || read_key(&raw),
        &mut |line, out| draw(line, question, capabilities, &theme, out),
        &mut stderr,
    );
    drop(raw);
    answer
}

/// The loop, over any key source and any writer.
fn ask<W: Write + ?Sized>(
    keys: &mut Keys<'_>,
    draw: &mut dyn FnMut(&Line, &mut String),
    out: &mut W,
) -> Answer {
    let mut line = Line::default();
    let answer = redraw_until(&mut line, keys, draw, out, |line, key| line.press(key))
        .unwrap_or(Answer::Cancelled);
    erase(&line, draw, out);
    answer
}

/// What has been typed so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Line {
    text: String,
}

impl Line {
    /// What has been typed.
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// What one key does, and whether it ends the question.
    pub(super) fn press(&mut self, key: Key) -> Option<Answer> {
        match key {
            Key::Escape => return Some(Answer::Cancelled),
            Key::Enter if !self.text.is_empty() => {
                return Some(Answer::Typed(self.text.clone()));
            }
            Key::Backspace => {
                self.text.pop();
            }
            Key::ClearLine => self.text.clear(),
            Key::Char(character) => self.text.push(character),
            Key::Enter | Key::Up | Key::Down | Key::Other => {}
        }
        None
    }
}

/// One frame: the question, the line, and the keys.
fn draw(
    line: &Line,
    question: &Question<'_>,
    capabilities: Capabilities,
    theme: &Theme,
    out: &mut String,
) {
    const NEWLINE: &str = "\r\n";
    let level = capabilities.color();
    let unicode = Glyphs::of(capabilities.glyphs()).leader == '·';
    let push_line = |out: &mut String, body: &dyn Fn(&mut String)| {
        out.push_str("  ");
        body(out);
        out.push_str("\x1b[K");
        out.push_str(NEWLINE);
    };

    out.push_str(NEWLINE);
    push_line(out, &|out| theme.title.paint(level, question.title, out));
    push_line(out, &|out| {
        theme
            .accent
            .paint(level, if unicode { "›" } else { ">" }, out);
        out.push(' ');
        if line.text().is_empty() {
            theme.muted.paint(level, question.placeholder, out);
        } else {
            theme.value.paint(level, line.text(), out);
            theme
                .accent
                .paint(level, if unicode { "▏" } else { "_" }, out);
        }
    });
    out.push_str(NEWLINE);
    push_line(out, &|out| {
        let keys = if unicode {
            "⏎ accept · esc cancel"
        } else {
            "enter accept · esc cancel"
        };
        theme.muted.paint(level, keys, out);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{ColorLevel, GlyphSet, Tty};

    fn capabilities() -> Capabilities {
        Capabilities::new(ColorLevel::Never, GlyphSet::Unicode, Tty::Interactive)
    }

    /// Drive the whole loop with `keys`, as a terminal would, and return the
    /// answer and everything written.
    fn drive(keys: &[Key]) -> (Answer, String) {
        let question = Question::new("deploy · tag", "any value");
        let theme = Theme::default();
        let mut keys = keys.iter().copied();
        let mut written = Vec::new();
        let answer = ask(
            &mut || Ok(keys.next()),
            &mut |line, out| draw(line, &question, capabilities(), &theme, out),
            &mut written,
        );
        (answer, String::from_utf8(written).unwrap())
    }

    #[test]
    fn typing_and_enter_answers_with_what_was_typed() {
        let (answer, written) = drive(&[
            Key::Char('v'),
            Key::Char('2'),
            Key::Backspace,
            Key::Char('1'),
            Key::Enter,
        ]);
        assert_eq!(answer, Answer::Typed("v1".into()));
        assert!(written.contains("deploy · tag"), "{written:?}");
        assert!(written.contains("v1"), "{written:?}");
    }

    #[test]
    fn enter_on_an_empty_line_is_not_an_answer() {
        let (answer, _) = drive(&[Key::Enter, Key::Char('x'), Key::ClearLine, Key::Enter]);
        // The keys ran out with nothing accepted, which reads as cancelled.
        assert_eq!(answer, Answer::Cancelled);
    }

    #[test]
    fn escape_cancels_and_the_block_is_erased() {
        let (answer, written) = drive(&[Key::Char('a'), Key::Escape]);
        assert_eq!(answer, Answer::Cancelled);
        assert!(written.ends_with("\x1b[J"), "{written:?}");
    }

    #[test]
    fn the_placeholder_shows_until_something_is_typed() {
        let theme = Theme::default();
        let question = Question::new("q", "any value");
        let mut out = String::new();
        let mut line = Line::default();
        draw(&line, &question, capabilities(), &theme, &mut out);
        assert!(out.contains("any value"));

        line.press(Key::Char('z'));
        out.clear();
        draw(&line, &question, capabilities(), &theme, &mut out);
        assert!(!out.contains("any value"));
        assert!(out.contains("z▏"));
    }
}
