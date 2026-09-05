//! One pass over a file, shared by every rule.
//!
//! Rules used to re-walk `source.lines()` once each, which meant N passes and N
//! chances to disagree about where a line starts. [`FileScan`] does the walk once:
//! it builds the [`LineIndex`] a single time, records each line's byte offset, the
//! sub-slice of the line that is *not* inside a comment, and the brace depth the
//! line opens at. Rules then borrow those slices instead of re-scanning.

mod line;
mod search;

#[cfg(test)]
mod tests;

use uf_infra::LineIndex;

use crate::SourceFile;
use line::{Carry, scan_line};

pub(crate) use search::{
    ends_word, find_all, find_words, identifier_len, is_hook_name, is_word_byte, next_non_space,
    prev_non_space, previous_word, starts_word,
};

/// A single physical line, plus the derived facts rules need about it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line<'a> {
    /// Byte offset of the line's first byte within the file.
    pub offset: usize,
    /// Line contents with the terminator (and a CRLF `\r`) removed.
    pub text: &'a str,
    /// Byte range of `text` that lies outside comments.
    code_start: usize,
    code_end: usize,
    /// Brace nesting depth at the first byte of this line.
    pub depth_at_start: u32,
    /// Whether this line begins inside a backtick template literal.
    ///
    /// A template does not respect line boundaries, so a rule asking whether a
    /// match sits inside a string has to be told where the line started. The
    /// scanner already carries this from line to line for its own sake; this
    /// is the same fact, kept for the rules.
    opens_in_template: bool,
}

impl<'a> Line<'a> {
    /// The part of the line that is code, i.e. outside `//` and `/* */` comments.
    ///
    /// String literals are *not* stripped: rules such as
    /// `uniflowed/no-npm-script-invocation` exist precisely to look inside them.
    #[inline]
    pub fn code(&self) -> &'a str {
        &self.text[self.code_start..self.code_end]
    }

    /// Byte offset of [`Line::code`] within [`Line::text`].
    #[inline]
    pub fn code_offset(&self) -> usize {
        self.code_start
    }

    /// Whether the byte at `at` in [`Line::code`] stands inside a string.
    ///
    /// [`Line::code`] keeps string literals on purpose —
    /// `uniflowed/no-npm-script-invocation` exists to read them — so a rule
    /// about what the code *does* has to ask before it fires. `const message =
    /// "no globalThis.fetch here"` overrides nothing, and `it("treats Object
    /// as any non-null object", …)` annotates nothing.
    ///
    /// A line that began inside a template literal is inside one from its
    /// first byte, which is why this is a method rather than a free function
    /// over the text: the answer depends on where the line started.
    #[inline]
    pub fn in_string(&self, at: usize) -> bool {
        search::in_string_from(self.code(), at, self.opens_in_template)
    }

    /// The trailing comment on this line, including its `//` or `/*` opener.
    #[inline]
    pub fn trailing_comment(&self) -> &'a str {
        &self.text[self.code_end..]
    }

    /// Byte offset of [`Line::trailing_comment`] within [`Line::text`].
    #[inline]
    pub fn comment_offset(&self) -> usize {
        self.code_end
    }
}

/// Cheap whole-file predicates, computed once so rules can bail out early.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FileFacts {
    /// The file uses Flow `component` declaration syntax.
    pub declares_component: bool,
    /// The file carries a `use client` directive somewhere.
    pub has_use_client: bool,
    /// The file mentions React at all.
    pub mentions_react: bool,
    /// The file mentions React Native at all.
    pub mentions_react_native: bool,
    /// Index into [`FileScan::lines`] of the first line with any code on it.
    pub first_code_line: Option<usize>,
    /// The file has at least one ESM `import` statement.
    pub has_esm_import: bool,
}

/// Everything the rules need about one file, computed once.
pub(crate) struct FileScan<'a> {
    /// The file under test.
    pub file: &'a SourceFile,
    /// Offset → line/column index, built exactly once per file.
    pub index: LineIndex,
    /// Physical lines with their comment-stripped code slices.
    pub lines: Vec<Line<'a>>,
    /// Whole-file predicates.
    pub facts: FileFacts,
}

impl<'a> FileScan<'a> {
    /// Walk `file` once and record everything the rules need.
    pub fn new(file: &'a SourceFile) -> Self {
        let source = file.source.as_str();
        let index = LineIndex::new(source);

        let mut lines = Vec::with_capacity(index.line_count());
        let mut facts = FileFacts::default();
        let mut carry = Carry::Code;
        let mut depth: u32 = 0;
        let mut offset = 0usize;

        for raw in split_lines(source) {
            let text = raw.strip_suffix('\r').unwrap_or(raw);
            let opened_in_template = carry == Carry::Template;
            let scanned = scan_line(text, carry);
            carry = scanned.carry;

            let line = Line {
                offset,
                text,
                code_start: scanned.code_start,
                code_end: scanned.code_end,
                depth_at_start: depth,
                opens_in_template: opened_in_template,
            };
            depth = depth.saturating_add_signed(scanned.brace_delta);

            if facts.first_code_line.is_none() && !line.code().trim().is_empty() {
                facts.first_code_line = Some(lines.len());
            }

            lines.push(line);
            offset += raw.len() + 1;
        }

        facts.declares_component = source.contains("component ");
        facts.has_use_client = source.contains("\"use client\"") || source.contains("'use client'");
        facts.mentions_react = source.contains("React");
        facts.mentions_react_native = source.contains("react-native");
        facts.has_esm_import = lines.iter().any(|line| {
            let code = line.code().trim_start();
            code.starts_with("import ") || code.starts_with("import{")
        });

        Self {
            file,
            index,
            lines,
            facts,
        }
    }
}

/// Split on `\n` with `str::lines` semantics but without dropping the final
/// empty line's offset bookkeeping.
fn split_lines(source: &str) -> impl Iterator<Item = &str> {
    source.split('\n')
}
