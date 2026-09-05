//! Turning a [`Menu`] into the text of one frame.
//!
//! Separate from the loop that reads keys so a frame can be asserted on
//! character by character in a test, which is the only way to be sure a
//! terminal nobody is watching lines up.

use crate::capability::Capabilities;
use crate::glyph::Glyphs;
use crate::style::Style;
use crate::text::display_width;
use crate::theme::Theme;

use super::menu::Menu;

/// The end of every line in a frame.
///
/// A carriage return as well as a line feed, because the frame is drawn with
/// the terminal in raw mode and raw mode turns off `onlcr` — the mapping that
/// makes a bare line feed also return the cursor to column zero. Without the
/// return, each row starts in the column the row above ended in, and a menu of
/// twenty commands walks off the right edge one row at a time.
const NEWLINE: &str = "\r\n";

/// The mark in front of the highlighted row.
const POINTER: &str = "❯";
/// Its ASCII stand-in, one cell wide like the original.
const POINTER_ASCII: &str = ">";

/// The prompt in front of what has been typed.
const CARET: &str = "›";
/// Its ASCII stand-in.
const CARET_ASCII: &str = ">";

/// Everything a frame needs that is not the menu itself.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    /// The question at the top.
    pub title: &'a str,
    /// What to type when nothing has been typed.
    pub placeholder: &'a str,
    /// How much colour and which glyphs the terminal takes.
    pub capabilities: Capabilities,
    /// The palette.
    pub theme: &'a Theme,
}

impl Frame<'_> {
    /// The pointer and caret for this terminal's glyph vocabulary.
    fn marks(&self) -> (&'static str, &'static str) {
        match Glyphs::of(self.capabilities.glyphs()).leader {
            // The Unicode vocabulary is identified by its own leader dot, so
            // the two never disagree about which set is in use.
            '·' => (POINTER, CARET),
            _ => (POINTER_ASCII, CARET_ASCII),
        }
    }
}

/// Draw one frame, appending to `out`.
///
/// Every line is written in full and the caller clears from the cursor down
/// before drawing, so a row that shrinks — a long description replaced by a
/// short one — leaves nothing of the old row behind.
pub fn frame(menu: &Menu<'_>, frame: &Frame<'_>, out: &mut String) {
    let level = frame.capabilities.color();
    let theme = frame.theme;
    let (pointer, caret) = frame.marks();

    out.push_str(NEWLINE);
    push_line(out, |out| {
        theme.title.paint(level, frame.title, out);
    });

    // The filter line. The placeholder is dimmed rather than absent so the
    // line does not change height the moment a reader types into it.
    push_line(out, |out| {
        theme.accent.paint(level, caret, out);
        out.push(' ');
        if menu.filter().is_empty() {
            theme.muted.paint(level, frame.placeholder, out);
        } else {
            theme.value.paint(level, menu.filter(), out);
        }
    });
    out.push_str(NEWLINE);

    if menu.is_empty() {
        push_line(out, |out| {
            theme.muted.paint(level, "nothing matches", out);
        });
        return;
    }

    let width = menu.name_width();
    let mut group = "";
    for (choice, highlighted) in menu.visible() {
        // A heading only where the group changes, and never above the first
        // row when the menu is unfiltered enough to have only one group.
        if choice.group != group && !choice.group.is_empty() {
            group = choice.group;
            push_line(out, |out| {
                theme.muted.paint(level, group, out);
            });
        }
        push_row(
            out,
            choice.name,
            choice.about,
            width,
            highlighted,
            pointer,
            frame,
        );
    }

    footer(menu, frame, out);
}

/// One row: the pointer, the name padded to a column, and the description.
fn push_row(
    out: &mut String,
    name: &str,
    about: &str,
    width: usize,
    highlighted: bool,
    pointer: &str,
    frame: &Frame<'_>,
) {
    let level = frame.capabilities.color();
    let theme = frame.theme;
    push_line(out, |out| {
        if highlighted {
            theme.accent.paint(level, pointer, out);
            out.push(' ');
            // Bold rather than a reversed background: a reversed row is the
            // width of the terminal and repaints as a bar every keystroke,
            // which flickers on a slow connection.
            Style::new().bold().paint(level, name, out);
        } else {
            out.push_str("  ");
            theme.value.paint(level, name, out);
        }
        pad(out, width.saturating_sub(display_width(name)) + 2);
        theme.muted.paint(level, about, out);
    });
}

/// The line under the list: what is off screen, and which keys do what.
fn footer(menu: &Menu<'_>, frame: &Frame<'_>, out: &mut String) {
    let level = frame.capabilities.color();
    let theme = frame.theme;
    let unicode = frame.marks().0 == POINTER;

    out.push_str(NEWLINE);
    push_line(out, |out| {
        let hidden = menu.hidden_below() + menu.hidden_above();
        if hidden > 0 {
            theme.muted.paint(level, &format!("{hidden} more · "), out);
        }
        let keys = if unicode {
            "↑↓ move · ⏎ run · esc cancel"
        } else {
            "up/down move · enter run · esc cancel"
        };
        theme.muted.paint(level, keys, out);
    });
}

/// Write one indented line, with everything after it cleared.
///
/// `\x1b[K` rather than padding with spaces: it erases to the right edge
/// whatever the width of the terminal, and costs three bytes instead of one
/// per column.
fn push_line(out: &mut String, body: impl FnOnce(&mut String)) {
    out.push_str("  ");
    body(out);
    out.push_str("\x1b[K");
    out.push_str(NEWLINE);
}

/// `count` spaces.
fn pad(out: &mut String, count: usize) {
    for _ in 0..count {
        out.push(' ');
    }
}
