//! Running text, wrapped to the width of the terminal, and the two-column
//! lists built from it.
//!
//! Everything else in this crate draws text that is short by construction: a
//! path, a count, a rule name. Help pages and explanations are sentences, and
//! a sentence written for a 100-column editor reaches a 60-column terminal
//! wrapped wherever the terminal cut it — mid-word, and back at column zero
//! under the flag it was describing rather than beside it.
//!
//! # What the wrapper understands
//!
//! * **Words.** Lines break between words and never inside one. A word wider
//!   than the whole line (a URL) gets a line to itself and overruns it, which
//!   is better than a URL a terminal can no longer recognise.
//! * **Code spans.** Text between backticks is something a reader types. With
//!   colour it is drawn in the accent and the backticks are dropped, exactly as
//!   [`Renderer::hint`] does; without colour the backticks stay, because they
//!   are then the only thing marking it. A span may cross a line break, and
//!   its width is measured the way it will be drawn.
//! * **Paragraphs and lists.** A blank line separates paragraphs. A line that
//!   starts with whitespace or a `- ` bullet keeps its own line and its
//!   indentation — that is how a doc comment writes a list or an example —
//!   and wraps under its own first character rather than under the margin.

use crate::glyph::Status;
use crate::render::Renderer;
use crate::style::Style;
use crate::text::{display_width, push_spaces};

/// The narrowest column a description is laid out beside its term in. Below
/// this, a two-column list stacks instead: the term on its own line and the
/// description under it.
///
/// Twenty-four columns is about three words a line, and a description broken
/// into three-word lines is harder to read than the same description on
/// full-width lines underneath its term.
pub const MIN_DESCRIPTION_WIDTH: usize = 24;

/// How far a stacked description is indented under its term.
const STACKED_INDENT: usize = 4;

/// The gap between a term and its description.
const GUTTER: usize = 2;

/// One row of a two-column list: a term, what it means, and a muted note.
///
/// ```text
///   --color <WHEN>  When to colourise output [default: auto]
///   ^ term          ^ text                    ^ meta
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Definition<'a> {
    /// What is being described. It may already be styled; its width is
    /// measured without the escapes.
    pub term: &'a str,
    /// The description, as prose: it wraps, and its code spans are drawn as
    /// code.
    pub text: &'a str,
    /// A note after the description that recedes: a default, an alias.
    pub meta: &'a str,
}

impl Definition<'_> {
    /// Whether a list of definitions indented by `indent`, with a term column
    /// `term_column` wide, stacks every row — term above description — in
    /// `width` columns, because the description column would be narrower
    /// than [`MIN_DESCRIPTION_WIDTH`].
    ///
    /// Asked by a caller that lays out the terms themselves differently when
    /// they stand alone on their line.
    pub fn stacks(indent: usize, width: usize, term_column: usize) -> bool {
        width.saturating_sub(text_start(indent, width, term_column)) < MIN_DESCRIPTION_WIDTH
    }
}

/// Where the descriptions of a list start when they sit beside their terms.
fn text_start(indent: usize, width: usize, term_column: usize) -> usize {
    indent + term_column.min(width.saturating_sub(indent) / 3).max(1) + GUTTER
}

impl<'a> Definition<'a> {
    /// A row with no note.
    pub fn new(term: &'a str, text: &'a str) -> Self {
        Self {
            term,
            text,
            meta: "",
        }
    }

    /// The same row with a muted note after the description.
    pub fn with_meta(mut self, meta: &'a str) -> Self {
        self.meta = meta;
        self
    }
}

/// Where the writer is on the current line, and what it is inside of.
struct Cursor {
    /// The column the next character lands in.
    column: usize,
    /// Where a wrapped line starts.
    margin: usize,
    /// The last column a line may reach.
    width: usize,
    /// Whether backticks are markup: colour is on, and they pair up. An
    /// unmatched backtick is text, and drawing everything after it as code
    /// would be a guess.
    markup: bool,
    /// Whether the text is inside a backtick span.
    in_code: bool,
    /// Whether anything has been written on this line past the margin.
    started: bool,
}

impl Renderer {
    /// Append `text` word-wrapped to `width` columns.
    ///
    /// The first line continues from `column`, where the caller left the
    /// cursor; every later line starts at `margin`. Nothing is written after
    /// the last word — no newline — so a caller can follow it with a note.
    ///
    /// Returns the column the cursor is left in.
    pub fn prose(
        &self,
        out: &mut String,
        text: &str,
        style: Style,
        column: usize,
        margin: usize,
        width: usize,
    ) -> usize {
        let mut cursor = Cursor {
            column,
            margin,
            width: width.max(margin + 1),
            markup: self.color().is_enabled() && text.matches('`').count().is_multiple_of(2),
            in_code: false,
            started: false,
        };
        let mut paragraphs = text.split("\n\n").filter(|p| !p.trim().is_empty());
        if let Some(first) = paragraphs.next() {
            self.paragraph(out, first, style, &mut cursor);
        }
        for paragraph in paragraphs {
            // A blank line between paragraphs, and the next one at the margin.
            out.push('\n');
            out.push('\n');
            push_spaces(out, cursor.margin);
            cursor.column = cursor.margin;
            cursor.started = false;
            self.paragraph(out, paragraph, style, &mut cursor);
        }
        cursor.column
    }

    /// One paragraph: flowing lines joined, list and indented lines kept.
    fn paragraph(&self, out: &mut String, paragraph: &str, style: Style, cursor: &mut Cursor) {
        let base_margin = cursor.margin;
        let mut first = true;
        for line in paragraph.lines() {
            let lead = line.len() - line.trim_start().len();
            let bullet = line.trim_start().starts_with("- ");
            let own_line = lead > 0 || bullet;
            if own_line && !first {
                out.push('\n');
                push_spaces(out, base_margin + lead);
                cursor.column = base_margin + lead;
                cursor.started = false;
            }
            // A list item wraps under its text, not under its bullet.
            cursor.margin = if own_line {
                base_margin + lead + if bullet { 2 } else { 0 }
            } else if first {
                base_margin
            } else {
                cursor.margin
            };
            for word in line.split_whitespace() {
                self.word(out, word, style, cursor);
            }
            first = false;
        }
        cursor.margin = base_margin;
    }

    /// One word, on this line if it fits and on the next if it does not.
    fn word(&self, out: &mut String, word: &str, style: Style, cursor: &mut Cursor) {
        let markup = cursor.markup;
        let width = if markup {
            display_width(word) - word.matches('`').count()
        } else {
            display_width(word)
        };
        if cursor.started {
            if cursor.column + 1 + width > cursor.width {
                out.push('\n');
                push_spaces(out, cursor.margin);
                cursor.column = cursor.margin;
            } else {
                out.push(' ');
                cursor.column += 1;
            }
        }
        cursor.started = true;
        cursor.column += width;
        if !markup {
            style.paint(self.color(), word, out);
            return;
        }
        // Split on the backticks, flipping in and out of code at each one.
        for (index, part) in word.split('`').enumerate() {
            if index > 0 {
                cursor.in_code = !cursor.in_code;
            }
            if part.is_empty() {
                continue;
            }
            let part_style = if cursor.in_code {
                self.theme().accent
            } else {
                style
            };
            part_style.paint(self.color(), part, out);
        }
    }

    /// [`Renderer::hint`], word-wrapped to `width` with its continuation lines
    /// under the hint's text rather than under its mark.
    pub fn hint_within(&self, out: &mut String, indent: usize, width: usize, text: &str) {
        push_spaces(out, indent);
        self.theme()
            .info
            .paint(self.color(), Status::Info.glyph(self.glyph_set()), out);
        out.push(' ');
        let margin = indent + 2;
        for (index, line) in text.lines().enumerate() {
            if index > 0 {
                out.push('\n');
                push_spaces(out, margin);
            }
            self.prose(out, line, self.theme().muted, margin, margin, width);
        }
        out.push('\n');
    }

    /// Append a two-column list: terms in one column, their descriptions
    /// wrapped in the other, all inside `width`.
    ///
    /// The term column is as wide as the widest term, up to a third of the
    /// line; a term wider than that puts its description on the next line in
    /// the same column. When the description column would be narrower than
    /// [`MIN_DESCRIPTION_WIDTH`] — a narrow terminal — every row stacks
    /// instead, term above description.
    ///
    /// `spaced` puts a blank line between rows, for descriptions long enough
    /// to run to several lines each.
    pub fn definitions(
        &self,
        out: &mut String,
        indent: usize,
        width: usize,
        rows: &[Definition<'_>],
        spaced: bool,
    ) {
        let widest = rows
            .iter()
            .map(|row| display_width(row.term))
            .max()
            .unwrap_or(0);
        self.definitions_at(out, indent, width, rows, spaced, widest);
    }

    /// [`Renderer::definitions`] with the term column stated rather than
    /// measured, so that several lists on one page — a section per heading —
    /// can share one column instead of each starting its descriptions where
    /// its own widest term happens to end.
    pub fn definitions_at(
        &self,
        out: &mut String,
        indent: usize,
        width: usize,
        rows: &[Definition<'_>],
        spaced: bool,
        term_column: usize,
    ) {
        let stacked = Definition::stacks(indent, width, term_column);
        let text_start = text_start(indent, width, term_column);
        let term_column = text_start - indent - GUTTER;

        for (index, row) in rows.iter().enumerate() {
            if spaced && index > 0 {
                out.push('\n');
            }
            push_spaces(out, indent);
            out.push_str(row.term);
            let term_width = display_width(row.term);
            if row.text.is_empty() && row.meta.is_empty() {
                out.push('\n');
                continue;
            }
            let margin = if stacked {
                out.push('\n');
                push_spaces(out, indent + STACKED_INDENT);
                indent + STACKED_INDENT
            } else if term_width > term_column {
                out.push('\n');
                push_spaces(out, text_start);
                text_start
            } else {
                push_spaces(out, text_start - indent - term_width);
                text_start
            };
            let column = self.prose(out, row.text, self.theme().value, margin, margin, width);
            if !row.meta.is_empty() {
                self.note(out, row.meta, column, margin, width, !row.text.is_empty());
            }
            out.push('\n');
        }
    }

    /// A muted note after prose.
    ///
    /// A note is one or more bracketed groups — `[values: auto, always,
    /// never] [default: auto]` — and a group split across two lines reads as
    /// two notes, so each moves to the next line as a unit. Only a group too
    /// wide for a line of its own is broken between its words.
    fn note(
        &self,
        out: &mut String,
        note: &str,
        column: usize,
        margin: usize,
        width: usize,
        started: bool,
    ) {
        let mut cursor = Cursor {
            column,
            margin,
            width: width.max(margin + 1),
            markup: false,
            in_code: false,
            started,
        };
        let muted = self.theme().muted;
        for group in groups(note) {
            let group_width = display_width(group);
            let fits_here =
                cursor.column + usize::from(cursor.started) + group_width <= cursor.width;
            let fits_a_line = cursor.margin + group_width <= cursor.width;
            if cursor.started && !fits_here && fits_a_line {
                out.push('\n');
                push_spaces(out, cursor.margin);
                cursor.column = cursor.margin;
                cursor.started = false;
            }
            for word in group.split_whitespace() {
                self.word(out, word, muted, &mut cursor);
            }
        }
    }
}

/// The bracketed groups of a note, each with its brackets; text outside any
/// bracket is a group per word.
fn groups(note: &str) -> impl Iterator<Item = &str> {
    let mut rest = note.trim_start();
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = if rest.starts_with('[') {
            rest.find(']').map_or(rest.len(), |at| at + 1)
        } else {
            rest.find(char::is_whitespace).unwrap_or(rest.len())
        };
        let (group, tail) = rest.split_at(end);
        rest = tail.trim_start();
        Some(group)
    })
}

#[cfg(test)]
mod tests;
