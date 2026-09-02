//! Tables with per-column alignment.

use crate::render::Renderer;
use crate::text::{Align, display_width, push_padded, push_spaces};
use crate::theme::Tone;

/// One table cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell<'a> {
    /// Cell text.
    pub text: &'a str,
    /// How the cell should read.
    pub tone: Tone,
}

impl<'a> Cell<'a> {
    /// A cell with the default tone.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            tone: Tone::Plain,
        }
    }

    /// A cell with a tone.
    pub fn toned(text: &'a str, tone: Tone) -> Self {
        Self { text, tone }
    }
}

/// One table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column<'a> {
    /// Header text.
    pub header: &'a str,
    /// How cells in this column are aligned.
    pub align: Align,
}

impl<'a> Column<'a> {
    /// A left-aligned column.
    pub fn left(header: &'a str) -> Self {
        Self {
            header,
            align: Align::Left,
        }
    }

    /// A right-aligned column, for counts and measurements.
    pub fn right(header: &'a str) -> Self {
        Self {
            header,
            align: Align::Right,
        }
    }
}

/// A table with per-column alignment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table<'a> {
    columns: Vec<Column<'a>>,
    rows: Vec<Vec<Cell<'a>>>,
}

impl<'a> Table<'a> {
    /// A table with the given columns and no rows.
    pub fn new(columns: Vec<Column<'a>>) -> Self {
        Self {
            columns,
            rows: Vec::new(),
        }
    }

    /// Append a row. Rows shorter than the column list are padded with blanks.
    pub fn push(&mut self, row: Vec<Cell<'a>>) {
        self.rows.push(row);
    }

    /// The columns.
    pub fn columns(&self) -> &[Column<'a>] {
        &self.columns
    }

    /// The rows.
    pub fn rows(&self) -> &[Vec<Cell<'a>>] {
        &self.rows
    }

    /// Whether the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

impl Renderer {
    /// Append a table, sizing every column to its widest cell.
    ///
    /// Column widths are measured once into one `Vec`; nothing is allocated per
    /// cell, so a table of thousands of rows costs one pass and one buffer.
    pub fn table(&self, out: &mut String, indent: usize, table: &Table<'_>) {
        if table.columns().is_empty() {
            return;
        }
        let mut widths: Vec<usize> = table
            .columns()
            .iter()
            .map(|column| display_width(column.header))
            .collect();
        for row in table.rows() {
            for (index, cell) in row.iter().enumerate() {
                if let Some(width) = widths.get_mut(index) {
                    *width = (*width).max(display_width(cell.text));
                }
            }
        }

        push_spaces(out, indent);
        for (index, column) in table.columns().iter().enumerate() {
            if index > 0 {
                out.push_str("  ");
            }
            self.theme().key.open(self.color(), out);
            push_padded(out, column.header, widths[index], column.align);
            self.theme().key.close(self.color(), out);
        }
        trim_trailing_spaces(out);
        out.push('\n');

        for row in table.rows() {
            push_spaces(out, indent);
            for (index, column) in table.columns().iter().enumerate() {
                if index > 0 {
                    out.push_str("  ");
                }
                let cell = row.get(index).copied().unwrap_or(Cell::new(""));
                let style = self.theme().tone(cell.tone);
                let padding = widths[index].saturating_sub(display_width(cell.text));
                match column.align {
                    Align::Left => {
                        style.paint(self.color(), cell.text, out);
                        push_spaces(out, padding);
                    }
                    Align::Right => {
                        push_spaces(out, padding);
                        style.paint(self.color(), cell.text, out);
                    }
                    Align::Center => {
                        let left = padding / 2;
                        push_spaces(out, left);
                        style.paint(self.color(), cell.text, out);
                        push_spaces(out, padding - left);
                    }
                }
            }
            trim_trailing_spaces(out);
            out.push('\n');
        }
    }
}

/// Drop the trailing spaces a padded final column leaves behind.
///
/// Trailing whitespace is invisible on screen but shows up in every diff, log
/// scrape, and golden test, so no line is allowed to end with it.
fn trim_trailing_spaces(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
}

#[cfg(test)]
mod tests;
