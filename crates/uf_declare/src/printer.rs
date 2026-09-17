//! TypeScript text, laid out on the Flow source's own lines.
//!
//! A published `.d.ts` is opened all the time — it is where "go to
//! definition" lands for every TypeScript consumer of the library, and it is
//! what a reader compares against the source when a type looks wrong. Those
//! two files are the same API written twice, so the printer puts every
//! declaration on the line its Flow original was written on, padding with
//! blank lines to get there. Line 40 of `dist/index.d.ts` is then line 40 of
//! `index.js`, and a gap reported at `index.js:40` names a line in both.
//!
//! It never goes backwards. A declaration that needs more lines than the
//! source did — an opaque type, which becomes a brand and an alias — runs
//! ahead, and the next anchor on a later line catches it up again. And it
//! never breaks a line it does not have to: two declarations written on one
//! source line stay on one output line, separated by a space, because a
//! newline the source did not have would push every later line down by one
//! for good.
//!
//! `uf_dts` has the same printer for the mirror-image reason, and the two are
//! not shared. That one anchors to byte offsets, because oxc's spans are byte
//! offsets; this one anchors to lines, because the Flow port's locations
//! carry a line already and computing an offset to derive it back would be
//! two conversions for nothing. Each is private to its crate, and a third
//! crate owning eight methods for two callers would be a dependency edge
//! bought with no reuse.

/// Output being written, and where it is relative to the Flow source.
pub(crate) struct Printer {
    out: String,
    /// The zero-based line the output is on.
    line: u32,
    /// Nesting depth, for the indentation a new line starts with.
    depth: u32,
}

impl Printer {
    /// A printer for a source of `capacity` bytes.
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            out: String::with_capacity(capacity),
            line: 0,
            depth: 0,
        }
    }

    /// Start something written on the one-based source line `line`: on that
    /// line when the output is behind it, and after a space on the current
    /// line when it is not.
    pub(crate) fn anchor(&mut self, line: u32) {
        let target = line.saturating_sub(1);
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

    /// Move to the next output line without anchoring to the source.
    ///
    /// For the second half of a declaration that became two — the alias after
    /// an opaque type's brand. The source has one line for it and the output
    /// needs two, so this one is deliberately not anchored.
    pub(crate) fn newline(&mut self) {
        self.out.push('\n');
        self.line += 1;
        for _ in 0..self.depth {
            self.out.push_str("  ");
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

    /// Whether anything has been written.
    pub(crate) fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// Where the output is now, for [`Self::rewind`].
    ///
    /// Whether a member prints anything is not knowable before printing it: a
    /// spread, an internal slot and an unkeyable indexer all decide partway
    /// through, and the separator in front of them has already been written by
    /// then. Asking the question twice — once to decide and once to print —
    /// would be two copies of the same decision, and the copies would drift.
    /// So the caller marks, prints, and rewinds when nothing came of it.
    pub(crate) fn mark(&self) -> (usize, u32) {
        (self.out.len(), self.line)
    }

    /// Undo everything written since `mark`, including its line count.
    pub(crate) fn rewind(&mut self, mark: (usize, u32)) {
        let (offset, line) = mark;
        self.out.truncate(offset);
        self.line = line;
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
    fn a_declaration_lands_on_its_source_line() {
        let mut printer = Printer::new(64);
        printer.anchor(4);
        printer.text("export type A = string;");
        assert_eq!(printer.finish(), "\n\n\nexport type A = string;\n");
    }

    #[test]
    fn two_things_on_one_source_line_stay_on_one_output_line() {
        let mut printer = Printer::new(64);
        printer.anchor(1);
        printer.text("type A = 1;");
        printer.anchor(1);
        printer.text("type B = 2;");
        assert_eq!(printer.finish(), "type A = 1; type B = 2;\n");
    }

    #[test]
    fn an_output_that_ran_ahead_does_not_go_back() {
        let mut printer = Printer::new(64);
        printer.anchor(1);
        printer.text("first\nsecond\nthird");
        printer.anchor(2);
        printer.text("next");
        assert_eq!(printer.finish(), "first\nsecond\nthird next\n");
    }

    #[test]
    fn a_declaration_that_became_two_lines_runs_ahead() {
        let mut printer = Printer::new(64);
        printer.anchor(1);
        printer.text("declare const Id$brand: unique symbol;");
        printer.newline();
        printer.text("export type Id = { readonly [Id$brand]: \"Id\" };");
        // The second source line is already behind the output, so the next
        // anchor joins the current line rather than moving backwards.
        printer.anchor(2);
        printer.text("export type B = 1;");
        assert_eq!(
            printer.finish(),
            "declare const Id$brand: unique symbol;\nexport type Id = { readonly [Id$brand]: \"Id\" }; export type B = 1;\n"
        );
    }

    #[test]
    fn a_nested_line_is_indented() {
        let mut printer = Printer::new(64);
        printer.anchor(1);
        printer.text("declare class Thing {");
        printer.indent();
        printer.anchor(2);
        printer.text("size: number;");
        printer.dedent();
        printer.anchor(3);
        printer.char('}');
        assert_eq!(
            printer.finish(),
            "declare class Thing {\n  size: number;\n}\n"
        );
    }
}
