//! Diagnostics rendered as a code frame.
//!
//! The shape is the one rustc and the modern JavaScript tools converged on: a
//! severity and rule id, the location, the offending source line, and a caret
//! span under the exact columns at fault.
//!
//! Three things routinely break naive implementations, and each has a test:
//! tabs (which are not one column), wide characters (which are not one column
//! either), and a span that runs past the end of the line (which must not
//! panic, misalign, or print an unbounded row of carets).

use crate::render::Renderer;
use crate::text::{char_width, floor_char_boundary, push_repeat, push_spaces, push_usize};

/// How a tab is rendered and measured inside a code frame.
const TAB_WIDTH: usize = 4;
/// The widest source line a frame will print before windowing around the span.
const MAX_LINE_WIDTH: usize = 120;
/// How much of the line to keep to the left of the span when windowing.
const WINDOW_LEAD: usize = 24;

/// The severity of a rendered diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    /// The run fails.
    Error,
    /// The run continues.
    Warning,
    /// Extra context attached to another diagnostic.
    Note,
    /// A suggested fix.
    Help,
}

impl DiagnosticLevel {
    /// The word printed at the start of the header line.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Help => "help",
        }
    }
}

/// One diagnostic, with everything needed to draw a code frame.
///
/// `column` and `span` are **byte** offsets, matching the linter, and are
/// converted to display columns here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeFrame<'a> {
    /// Severity.
    pub level: DiagnosticLevel,
    /// Canonical rule id, when the diagnostic came from a rule.
    pub rule: Option<&'a str>,
    /// The one-line explanation.
    pub message: &'a str,
    /// Path of the offending file.
    pub path: &'a str,
    /// One-based line number.
    pub line: usize,
    /// One-based byte column within the line.
    pub column: usize,
    /// Length of the offending span in bytes; at least one column is drawn.
    pub span: usize,
    /// The source line itself; without it only the header is drawn.
    pub source_line: Option<&'a str>,
    /// Short note printed after the carets.
    pub label: Option<&'a str>,
}

impl<'a> CodeFrame<'a> {
    /// A frame with no rule, span, source line, or label.
    pub fn new(
        level: DiagnosticLevel,
        message: &'a str,
        path: &'a str,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            level,
            rule: None,
            message,
            path,
            line,
            column,
            span: 1,
            source_line: None,
            label: None,
        }
    }

    /// Attach the rule id.
    pub fn with_rule(mut self, rule: &'a str) -> Self {
        self.rule = Some(rule);
        self
    }

    /// Attach the byte length of the offending span.
    pub fn with_span(mut self, span: usize) -> Self {
        self.span = span;
        self
    }

    /// Attach the source line the frame draws.
    pub fn with_source_line(mut self, source_line: &'a str) -> Self {
        self.source_line = Some(source_line);
        self
    }

    /// Attach a short note printed after the carets.
    pub fn with_label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }
}

/// Where the caret goes, in display columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: usize,
    len: usize,
    total: usize,
}

/// Measure one source line, expanding tabs and honouring character widths.
fn measure(line: &str, column: usize, span: usize) -> Span {
    let start_byte = floor_char_boundary(line, column.saturating_sub(1));
    let end_byte = floor_char_boundary(line, start_byte.saturating_add(span.max(1)));

    let mut col = 0usize;
    let mut start = None;
    let mut end = None;
    for (offset, ch) in line.char_indices() {
        if offset == start_byte {
            start = Some(col);
        }
        if offset == end_byte {
            end = Some(col);
        }
        col += cell_width(ch, col);
    }
    let start = start.unwrap_or(col);
    let end = end.unwrap_or(col);
    Span {
        start,
        len: end.saturating_sub(start).max(1),
        total: col,
    }
}

/// The number of columns `ch` occupies when it starts at column `col`.
fn cell_width(ch: char, col: usize) -> usize {
    if ch == '\t' {
        TAB_WIDTH - (col % TAB_WIDTH)
    } else {
        char_width(ch)
    }
}

/// The window of the line that will be printed, in display columns.
fn window(span: Span) -> (usize, usize) {
    if span.total <= MAX_LINE_WIDTH {
        return (0, span.total);
    }
    let start = span.start.saturating_sub(WINDOW_LEAD);
    (start, start + MAX_LINE_WIDTH)
}

pub(crate) fn render_frame(
    renderer: &Renderer,
    out: &mut String,
    frame: &CodeFrame<'_>,
    indent: usize,
) {
    let theme = renderer.theme();
    let glyphs = renderer.glyphs();
    let level = renderer.color();
    let level_style = match frame.level {
        DiagnosticLevel::Error => theme.error.bold(),
        DiagnosticLevel::Warning => theme.warning.bold(),
        DiagnosticLevel::Note => theme.muted.bold(),
        DiagnosticLevel::Help => theme.info.bold(),
    };

    push_spaces(out, indent);
    level_style.open(level, out);
    out.push_str(frame.level.label());
    if let Some(rule) = frame.rule {
        out.push('[');
        out.push_str(rule);
        out.push(']');
    }
    level_style.close(level, out);
    out.push_str(": ");
    theme.title.paint(level, frame.message, out);
    out.push('\n');

    let gutter = crate::text::decimal_digits(frame.line);
    push_spaces(out, indent + gutter);
    theme.gutter.open(level, out);
    out.push_str(glyphs.arrow);
    theme.gutter.close(level, out);
    out.push(' ');
    theme.path.open(level, out);
    out.push_str(frame.path);
    out.push(':');
    push_usize(out, frame.line);
    out.push(':');
    push_usize(out, frame.column);
    theme.path.close(level, out);
    out.push('\n');

    let Some(source_line) = frame.source_line else {
        return;
    };

    let span = measure(source_line, frame.column, frame.span);
    let (window_start, window_end) = window(span);

    // Bar above the source line.
    push_spaces(out, indent + gutter + 1);
    theme.gutter.open(level, out);
    out.push(glyphs.vertical);
    theme.gutter.close(level, out);
    out.push('\n');

    // The source line itself, with tabs expanded so the caret can line up.
    push_spaces(out, indent);
    theme.gutter.open(level, out);
    push_usize(out, frame.line);
    out.push(' ');
    out.push(glyphs.vertical);
    theme.gutter.close(level, out);
    out.push(' ');
    if window_start > 0 {
        theme.muted.paint(level, glyphs.ellipsis, out);
    }
    let mut col = 0usize;
    for ch in source_line.chars() {
        let width = cell_width(ch, col);
        if col >= window_start && col + width <= window_end {
            if ch == '\t' {
                push_spaces(out, width);
            } else {
                out.push(ch);
            }
        }
        col += width;
    }
    if span.total > window_end {
        theme.muted.paint(level, glyphs.ellipsis, out);
    }
    out.push('\n');

    // The caret row.
    push_spaces(out, indent + gutter + 1);
    theme.gutter.open(level, out);
    out.push(glyphs.vertical);
    theme.gutter.close(level, out);
    out.push(' ');
    let caret_start = span.start.saturating_sub(window_start);
    let lead = if window_start > 0 {
        caret_start + crate::text::display_width(glyphs.ellipsis)
    } else {
        caret_start
    };
    push_spaces(out, lead);
    let caret_len = span.len.min(MAX_LINE_WIDTH);
    level_style.open(level, out);
    push_repeat(out, glyphs.caret, caret_len);
    if let Some(label) = frame.label {
        out.push(' ');
        out.push_str(label);
    }
    level_style.close(level, out);
    out.push('\n');
}

#[cfg(test)]
mod tests;
