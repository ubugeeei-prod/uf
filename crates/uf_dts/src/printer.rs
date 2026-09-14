//! Flow text, laid out on the declaration file's own lines.
//!
//! A translated module is never opened by anyone, but locations inside it are
//! read all the time: a Flow error about a call into `zod` says *"string [1]
//! is incompatible with number [2]"* and `[2]` is a line in the translation.
//! If that line is the line of the `.d.ts` file the declaration came from, the
//! reader can open the file the path names and find the declaration there.
//! So the printer puts every statement and every member on the source line it
//! was written on, padding with blank lines to get there.
//!
//! It never goes backwards. A translation that needs more lines than the
//! source did — a merged interface printed where the first one was — runs
//! ahead, and the next anchor on a later line catches it up again. And it
//! never breaks a line it does not have to: two members written on one source
//! line stay on one output line, separated by a space, because a newline the
//! source did not have would push every later line down by one for good.

/// Output being written, and where it is relative to the source.
pub(crate) struct Printer {
    out: String,
    /// The byte offset each source line starts at.
    line_starts: Vec<u32>,
    /// The zero-based line the output is on.
    line: u32,
    /// Nesting depth, for the indentation a new line starts with.
    depth: u32,
}

impl Printer {
    pub(crate) fn new(source: &str) -> Self {
        let mut line_starts = Vec::with_capacity(source.len() / 32 + 1);
        line_starts.push(0);
        for newline in uf_infra::memchr_iter(b'\n', source.as_bytes()) {
            line_starts.push(u32::try_from(newline + 1).unwrap_or(u32::MAX));
        }
        Self {
            out: String::with_capacity(source.len()),
            line_starts,
            line: 0,
            depth: 0,
        }
    }

    /// The zero-based source line `offset` is on.
    pub(crate) fn source_line(&self, offset: u32) -> u32 {
        let index = self
            .line_starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        u32::try_from(index).unwrap_or(u32::MAX)
    }

    /// Start something that was written at `offset`: on its source line when
    /// the output is behind it, and after a space on the current line when it
    /// is not.
    pub(crate) fn anchor(&mut self, offset: u32) {
        let target = self.source_line(offset);
        if self.line < target {
            while self.line < target {
                self.out.push('\n');
                self.line += 1;
            }
            for _ in 0..self.depth {
                self.out.push_str("  ");
            }
        } else if !(self.out.is_empty() || self.out.ends_with(['\n', ' '])) {
            self.out.push(' ');
        }
    }

    /// Separate what comes next from what came before with one space, unless
    /// the output is at the start of a line or already ends in one.
    pub(crate) fn space(&mut self) {
        if !(self.out.is_empty() || self.out.ends_with(['\n', ' '])) {
            self.out.push(' ');
        }
    }

    /// Write text. A newline inside it — a template literal type can hold
    /// one — is counted, so later anchors still know where the output is.
    pub(crate) fn text(&mut self, text: &str) {
        self.line += u32::try_from(uf_infra::memchr_iter(b'\n', text.as_bytes()).count())
            .unwrap_or(u32::MAX);
        self.out.push_str(text);
    }

    /// Write one character that is not a newline.
    pub(crate) fn char(&mut self, character: char) {
        debug_assert_ne!(character, '\n');
        self.out.push(character);
    }

    /// Put `text` at the very start of the output, on its first line, so
    /// nothing below it moves.
    pub(crate) fn prepend(&mut self, text: &str) {
        debug_assert!(!text.contains('\n'));
        if self.out.is_empty() || self.out.starts_with('\n') {
            self.out.insert_str(0, text.trim_end());
        } else {
            self.out.insert_str(0, text);
        }
    }

    /// Indent the lines a nested body starts.
    pub(crate) fn indent(&mut self) {
        self.depth += 1;
    }

    /// Undo [`Self::indent`].
    pub(crate) fn dedent(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// The text, ending in exactly one newline.
    pub(crate) fn finish(mut self) -> String {
        while self.out.ends_with(['\n', ' ']) {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_statement_lands_on_its_source_line() {
        let source = "\n\n// a comment\nexport type A = string;\n";
        let mut printer = Printer::new(source);
        printer.anchor(u32::try_from(source.find("export").unwrap()).unwrap());
        printer.text("export type A = string;");
        assert_eq!(printer.finish(), "\n\n\nexport type A = string;\n");
    }

    #[test]
    fn two_things_on_one_source_line_stay_on_one_output_line() {
        let source = "type A = 1; type B = 2;\n";
        let mut printer = Printer::new(source);
        printer.anchor(0);
        printer.text("type A = 1;");
        printer.anchor(12);
        printer.text("type B = 2;");
        assert_eq!(printer.finish(), "type A = 1; type B = 2;\n");
    }

    #[test]
    fn an_output_that_ran_ahead_does_not_go_back() {
        let source = "a\nb\n";
        let mut printer = Printer::new(source);
        printer.anchor(0);
        printer.text("first\nsecond\nthird");
        printer.anchor(2);
        printer.text("next");
        assert_eq!(printer.finish(), "first\nsecond\nthird next\n");
    }

    #[test]
    fn a_nested_line_is_indented() {
        let source = "declare namespace N {\n  type T = 1;\n}\n";
        let mut printer = Printer::new(source);
        printer.anchor(0);
        printer.text("declare namespace N {");
        printer.indent();
        printer.anchor(24);
        printer.text("type T = 1;");
        printer.dedent();
        printer.anchor(36);
        printer.char('}');
        assert_eq!(
            printer.finish(),
            "declare namespace N {\n  type T = 1;\n}\n"
        );
    }
}
