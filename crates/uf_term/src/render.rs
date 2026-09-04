//! The rendering primitives every `uf` command is built from.
//!
//! Every primitive appends to a caller-owned `String`. Nothing here allocates
//! per cell, per column, or per diagnostic: a render loop reuses one buffer, so
//! formatting a report with thousands of rows costs one growth curve rather
//! than one allocation per piece of text.

use std::time::Duration;

use crate::capability::{Capabilities, ColorLevel, GlyphSet};
use crate::diagnostic::{CodeFrame, render_frame};
use crate::glyph::{Glyphs, Status};
use crate::image::ImageProtocol;
use crate::style::Style;
use crate::text::{display_width, push_repeat, push_spaces, push_usize};
use crate::theme::{Theme, Tone};
use crate::timing::{Phase, push_duration};

/// The narrowest banner rule.
const MIN_RULE: usize = 4;
/// The widest banner rule.
const MAX_RULE: usize = 72;
/// Columns of leader dots between a timing label and its duration.
const LEADER_WIDTH: usize = 24;

/// One row of a key/value block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyValue<'a> {
    /// The label.
    pub key: &'a str,
    /// The value.
    pub value: &'a str,
    /// How the value should read.
    pub tone: Tone,
}

impl<'a> KeyValue<'a> {
    /// A row whose value carries no particular meaning.
    pub fn new(key: &'a str, value: &'a str) -> Self {
        Self {
            key,
            value,
            tone: Tone::Plain,
        }
    }

    /// A row with a tone.
    pub fn toned(key: &'a str, value: &'a str, tone: Tone) -> Self {
        Self { key, value, tone }
    }
}

/// Renders the primitives with one fixed set of capabilities.
#[derive(Debug, Clone, Copy)]
pub struct Renderer {
    capabilities: Capabilities,
    theme: Theme,
    glyphs: Glyphs,
}

impl Renderer {
    /// A renderer for a stream with the given capabilities.
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            capabilities,
            theme: Theme::default(),
            glyphs: Glyphs::of(capabilities.glyphs()),
        }
    }

    /// Replace the palette.
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// The capabilities this renderer was built with.
    pub fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// How much colour may be written.
    pub fn color(&self) -> ColorLevel {
        self.capabilities.color()
    }

    /// The glyph vocabulary in use.
    pub fn glyphs(&self) -> Glyphs {
        self.glyphs
    }

    /// The glyph set in use.
    pub fn glyph_set(&self) -> GlyphSet {
        self.capabilities.glyphs()
    }

    /// The inline-image protocol this stream accepts, if any.
    pub fn image(&self) -> Option<ImageProtocol> {
        self.capabilities.image()
    }

    /// The palette in use.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Append `text` in `style`, followed by a newline.
    pub fn line(&self, out: &mut String, style: Style, text: &str) {
        style.paint(self.color(), text, out);
        out.push('\n');
    }

    /// Append an empty line.
    pub fn blank(&self, out: &mut String) {
        out.push('\n');
    }

    /// Append a command banner: a bold title, an optional subtitle, and a rule
    /// as wide as the title line.
    pub fn banner(&self, out: &mut String, title: &str, subtitle: Option<&str>) {
        self.theme.title.paint(self.color(), title, out);
        let mut width = display_width(title);
        if let Some(subtitle) = subtitle {
            out.push_str("  ");
            self.theme.subtitle.paint(self.color(), subtitle, out);
            width += 2 + display_width(subtitle);
        }
        out.push('\n');
        self.rule(out, width.clamp(MIN_RULE, MAX_RULE));
    }

    /// Append a horizontal rule of `width` columns.
    pub fn rule(&self, out: &mut String, width: usize) {
        self.theme.rule.open(self.color(), out);
        push_repeat(out, self.glyphs.horizontal, width);
        self.theme.rule.close(self.color(), out);
        out.push('\n');
    }

    /// Append a section heading.
    pub fn heading(&self, out: &mut String, indent: usize, text: &str) {
        push_spaces(out, indent);
        self.line(out, self.theme.title, text);
    }

    /// Append a status line: a one-cell mark, then the message.
    pub fn status(&self, out: &mut String, status: Status, message: &str) {
        let style = self.status_style(status);
        style.paint(self.color(), status.glyph(self.glyph_set()), out);
        out.push(' ');
        out.push_str(message);
        out.push('\n');
    }

    /// The style a status mark is drawn in.
    pub fn status_style(&self, status: Status) -> Style {
        match status {
            Status::Success => self.theme.success,
            Status::Warn => self.theme.warning,
            Status::Error => self.theme.error,
            Status::Info => self.theme.info,
            Status::Skip => self.theme.muted,
        }
    }

    /// Append a key/value block with the values in one aligned column.
    pub fn key_values(&self, out: &mut String, indent: usize, rows: &[KeyValue<'_>]) {
        let key_width = rows
            .iter()
            .map(|row| display_width(row.key))
            .max()
            .unwrap_or(0);
        for row in rows {
            push_spaces(out, indent);
            self.theme.key.open(self.color(), out);
            out.push_str(row.key);
            self.theme.key.close(self.color(), out);
            push_spaces(out, key_width - display_width(row.key) + 2);
            self.theme
                .tone(row.tone)
                .paint(self.color(), row.value, out);
            out.push('\n');
        }
    }

    /// Append a phase-timing block with right-aligned durations.
    pub fn timings(
        &self,
        out: &mut String,
        indent: usize,
        phases: &[Phase],
        total: Option<Duration>,
    ) {
        if phases.is_empty() && total.is_none() {
            return;
        }
        let mut scratch = String::new();
        let mut label_width = 0usize;
        let mut value_width = 0usize;
        for phase in phases {
            label_width = label_width.max(display_width(phase.label));
            scratch.clear();
            push_duration(&mut scratch, phase.duration);
            value_width = value_width.max(display_width(&scratch));
        }
        if let Some(total) = total {
            label_width = label_width.max(display_width("total"));
            scratch.clear();
            push_duration(&mut scratch, total);
            value_width = value_width.max(display_width(&scratch));
        }

        for phase in phases {
            scratch.clear();
            push_duration(&mut scratch, phase.duration);
            self.timing_row(
                out,
                indent,
                phase.label,
                label_width,
                &scratch,
                value_width,
                self.theme.value,
            );
        }
        if let Some(total) = total {
            scratch.clear();
            push_duration(&mut scratch, total);
            self.timing_row(
                out,
                indent,
                "total",
                label_width,
                &scratch,
                value_width,
                self.theme.title,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn timing_row(
        &self,
        out: &mut String,
        indent: usize,
        label: &str,
        label_width: usize,
        value: &str,
        value_width: usize,
        label_style: Style,
    ) {
        push_spaces(out, indent);
        label_style.paint(self.color(), label, out);
        push_spaces(out, label_width - display_width(label) + 1);
        self.theme.rule.open(self.color(), out);
        push_repeat(out, self.glyphs.leader, LEADER_WIDTH);
        self.theme.rule.close(self.color(), out);
        push_spaces(out, value_width - display_width(value) + 1);
        self.theme.number.paint(self.color(), value, out);
        out.push('\n');
    }

    /// Append a numbered list, for "next steps" style blocks.
    pub fn ordered_list(&self, out: &mut String, indent: usize, items: &[&str]) {
        for (index, item) in items.iter().enumerate() {
            push_spaces(out, indent);
            self.theme.key.open(self.color(), out);
            push_usize(out, index + 1);
            out.push('.');
            self.theme.key.close(self.color(), out);
            out.push(' ');
            self.theme.value.paint(self.color(), item, out);
            out.push('\n');
        }
    }

    /// Append a bulleted list.
    pub fn bullet_list(&self, out: &mut String, indent: usize, items: &[&str]) {
        for item in items {
            push_spaces(out, indent);
            self.theme.rule.open(self.color(), out);
            out.push_str("- ");
            self.theme.rule.close(self.color(), out);
            self.theme.value.paint(self.color(), item, out);
            out.push('\n');
        }
    }

    /// Append a diagnostic code frame.
    pub fn code_frame(&self, out: &mut String, frame: &CodeFrame<'_>) {
        render_frame(self, out, frame, 0);
    }

    /// Append a diagnostic code frame indented by `indent` columns.
    pub fn code_frame_at(&self, out: &mut String, frame: &CodeFrame<'_>, indent: usize) {
        render_frame(self, out, frame, indent);
    }
}

#[cfg(test)]
mod tests;
