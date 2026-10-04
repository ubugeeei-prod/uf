//! One pass over a file, shared by every rule.
//!
//! Rules used to re-walk `source.lines()` once each, which meant N passes and N
//! chances to disagree about where a line starts. [`FileScan`] does the walk once:
//! it builds the [`LineIndex`] a single time, records each line's byte offset, the
//! sub-slice of the line that is *not* inside a comment, and the brace depth the
//! line opens at. Rules then borrow those slices instead of re-scanning.

mod line;
mod mask;
mod search;

#[cfg(test)]
mod tests;

use nonmax::NonMaxUsize;
use uf_infra::LineIndex;
use uf_profiler::profile_span;

use crate::SourceFile;
use line::{Carry, scan_line};
pub(crate) use mask::mask_inline_comments;

pub(crate) use search::{
    ends_word, find_all, find_words, heads_a_command, identifier_len, in_jsx_text, is_hook_name,
    is_word_byte, next_non_space, opens_jsx_text, prev_non_space, previous_word, starts_word,
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

    /// Whether this line begins inside a backtick template literal.
    #[inline]
    pub fn opens_in_template(&self) -> bool {
        self.opens_in_template
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
    /// The file has a `"use client"` directive statement.
    ///
    /// A mention of the words inside another string, or as the text of an
    /// element, is not a directive, so it does not make the file a client module.
    pub has_use_client: bool,
    /// The file mentions React at all.
    pub mentions_react: bool,
    /// The file mentions React Native at all.
    pub mentions_react_native: bool,
    /// Index into [`FileScan::lines`] of the first line with any code on it.
    pub first_code_line: Option<NonMaxUsize>,
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
    /// `source` is `file.source` with every same-line `/* … */` blanked — see
    /// [`mask_inline_comments`], which explains why it is the caller's to hold.
    /// Masking preserves every byte offset, so a diagnostic measured against it
    /// lands in the same place in the real file.
    pub fn new(file: &'a SourceFile, source: &'a str) -> Self {
        profile_span!("FileScan::new");
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
                facts.first_code_line = NonMaxUsize::new(lines.len());
            }

            lines.push(line);
            offset += raw.len() + 1;
        }

        // Read from the code and not from the whole file. These four decide
        // which rules run at all, and `source` includes every comment: a file
        // whose header explains the `component` syntax, or mentions the
        // "server-component analysis", was treated as declaring one, and
        // `react/no-default-export-component` then reported a plugin factory.
        // `Line::code` is the line with its comments removed and its string
        // literals kept, which is what each of these wants — `use client` is a
        // string by construction, and a name is only a mention where it could
        // be read.
        //
        // A `component ` inside a string is not a declaration, a
        // `"use client"` inside a string is not a directive, and an `import`
        // inside a string is not a module, so those ask `in_string`. An
        // `export … from` is an import too. The other two do not: a package
        // name in an import specifier is exactly how `react-native` and
        // `React` reach a file, and both of those are strings.
        facts.declares_component = lines.iter().any(|line| {
            let code = line.code();
            code.match_indices("component ")
                .any(|(at, _)| !line.in_string(at))
        });
        facts.mentions_react = lines.iter().any(|line| line.code().contains("React"));
        facts.mentions_react_native = lines
            .iter()
            .any(|line| line.code().contains("react-native"));

        let mut built = Self {
            file,
            index,
            lines,
            facts,
        };
        // An `import` drawn as the text of an element is not a module. The
        // predicate has to run after `built` exists: telling JSX text from a
        // statement looks at the previous line as well as this one.
        built.facts.has_esm_import = built.lines.iter().enumerate().any(|(position, line)| {
            line_imports_esm(line) && !line_starts_in_jsx_text(&built, position)
        });
        // A directive is a statement. The opening quote of `"use client"` is
        // not inside a string; the same characters inside a template, in the
        // middle of another string, or on the line after `<pre>`, are text.
        built.facts.has_use_client = built.lines.iter().enumerate().any(|(position, line)| {
            let code = line.code();
            let Some((at, _)) = next_non_space(code, 0) else {
                return false;
            };
            if line.in_string(at) || line_starts_in_jsx_text(&built, position) {
                return false;
            }
            let rest = &code[at..];
            rest.starts_with("\"use client\"") || rest.starts_with("'use client'")
        });
        built
    }
}

/// Whether the first token of `position` is JSX text.
///
/// `"use client"` and `import { a } from "./a.js"` on the line after `<pre>`
/// are the element's contents. A quote is not an identifier, so the span
/// checked is one byte when the token has no name.
fn line_starts_in_jsx_text(scan: &FileScan<'_>, position: usize) -> bool {
    let line = &scan.lines[position];
    let code = line.code();
    let Some((at, _)) = next_non_space(code, 0) else {
        return false;
    };
    word_in_jsx_text(scan, position, at, identifier_len(code, at).max(1))
}

/// Whether `line` is an ES module import or re-export.
///
/// A template line that reads `import { … }` is source this module quotes,
/// not a module system it joins. `export { A } from "…"` and a continued
/// `} from "…"` are imports: the binding arrives through the module graph.
fn line_imports_esm(line: &Line<'_>) -> bool {
    let code = line.code();
    let Some((at, _)) = next_non_space(code, 0) else {
        return false;
    };
    if line.in_string(at) {
        return false;
    }
    let rest = &code[at..];
    if rest.starts_with("import ") || rest.starts_with("import{") {
        return true;
    }
    find_all(code, "from").any(|from_at| reexport_from(line, code, from_at))
}

/// Whether `from` at `from_at` is the clause of `export … from "…"`.
///
/// `}` and `*` are the tokens a re-export puts in front of `from`, including
/// a continued `} from "…"`. An identifier is that clause only on a line that
/// itself starts with `export`, as in `export * as ns from "…"`. JSX text
/// such as `<p>from "package"</p>` has neither, so it is not an import.
fn reexport_from(line: &Line<'_>, code: &str, from_at: usize) -> bool {
    if !starts_word(code, from_at)
        || !ends_word(code, from_at + "from".len())
        || line.in_string(from_at)
    {
        return false;
    }
    if !next_non_space(code, from_at + "from".len())
        .is_some_and(|(_, byte)| matches!(byte, b'\'' | b'"' | b'`'))
    {
        return false;
    }
    match prev_non_space(code, from_at) {
        Some((_, b'}' | b'*')) => true,
        Some((_, byte)) if is_word_byte(byte) => {
            let Some((start, _)) = next_non_space(code, 0) else {
                return false;
            };
            !line.in_string(start)
                && code[start..].starts_with("export")
                && ends_word(code, start + "export".len())
        }
        _ => false,
    }
}

/// Whether the word at `at` on line `position` is JSX text.
///
/// [`in_jsx_text`] covers `<p>any</p>`. Two shapes it refuses are still text:
/// an assignment or a `{` body after a tag on this line (`<p>globalThis.fetch
/// = mine</p>`, `<pre>component Child() { … }`), and a word on the line after
/// one that opened an element. A comparison (`count > eval(x)`) has a `>` that
/// is not a tag, and `{eval(x)}` has already closed the text.
pub(crate) fn word_in_jsx_text(
    scan: &FileScan<'_>,
    position: usize,
    at: usize,
    len: usize,
) -> bool {
    let line = &scan.lines[position];
    let code = line.code();
    let template = line.opens_in_template();
    if in_jsx_text(code, at, len, template) {
        return true;
    }
    let continued = position > 0 && opens_jsx_text(scan.lines[position - 1].code());
    if continued && search::jsx_text_is_open(code, at, template, true) {
        return true;
    }
    search::jsx_text_is_open(code, at, template, false) && search::tag_opened_before(code, at)
}

/// Split on `\n` with `str::lines` semantics but without dropping the final
/// empty line's offset bookkeeping.
fn split_lines(source: &str) -> impl Iterator<Item = &str> {
    source.split('\n')
}
