//! One pass over a file, shared by every rule.
//!
//! Rules used to re-walk `source.lines()` once each, which meant N passes and N
//! chances to disagree about where a line starts. [`FileScan`] does the walk once:
//! it builds the [`LineIndex`] a single time, records each line's byte offset, the
//! sub-slice of the line that is *not* inside a comment, and the brace depth the
//! line opens at. Rules then borrow those slices instead of re-scanning.

use uf_infra::LineIndex;

use crate::SourceFile;

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

/// State carried from one line into the next by the comment scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carry {
    /// Ordinary code.
    Code,
    /// Inside a `/* ... */` comment.
    BlockComment,
    /// Inside a backtick template literal.
    Template,
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
            let scanned = scan_line(text, carry);
            carry = scanned.carry;

            let line = Line {
                offset,
                text,
                code_start: scanned.code_start,
                code_end: scanned.code_end,
                depth_at_start: depth,
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

/// Result of classifying one physical line.
struct ScannedLine {
    code_start: usize,
    code_end: usize,
    carry: Carry,
    /// Net `{` minus `}` outside comments and string literals.
    brace_delta: i32,
}

/// Classify one line into `[code_start, code_end)` plus the state the next line
/// starts in, counting braces that are really braces on the way.
///
/// Known, deliberate simplification: when a `/* ... */` comment opens *and*
/// closes on the same line, the code after the closing `*/` is dropped rather
/// than stitched back together. That costs a few diagnostics on lines like
/// `const a = /* why */ any;` and never invents one, which is the right way for a
/// linter to be wrong.
fn scan_line(text: &str, carry: Carry) -> ScannedLine {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut code_start = 0usize;

    match carry {
        Carry::BlockComment => match find_pair(bytes, 0, b'*', b'/') {
            Some(pos) => {
                i = pos + 2;
                code_start = i;
            }
            None => {
                return ScannedLine {
                    code_start: 0,
                    code_end: 0,
                    carry: Carry::BlockComment,
                    brace_delta: 0,
                };
            }
        },
        Carry::Template => match find_unescaped(bytes, 0, b'`') {
            Some(pos) => i = pos + 1,
            None => {
                return ScannedLine {
                    code_start: 0,
                    code_end: len,
                    carry: Carry::Template,
                    brace_delta: 0,
                };
            }
        },
        Carry::Code => {}
    }

    let mut carry = Carry::Code;
    let mut code_end = len;
    let mut brace_delta = 0i32;
    while i < len {
        match bytes[i] {
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                code_end = i;
                break;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                code_end = i;
                if find_pair(bytes, i + 2, b'*', b'/').is_none() {
                    carry = Carry::BlockComment;
                }
                break;
            }
            quote @ (b'\'' | b'"') => {
                i = match find_unescaped(bytes, i + 1, quote) {
                    Some(pos) => pos + 1,
                    None => len,
                };
            }
            b'`' => match find_unescaped(bytes, i + 1, b'`') {
                Some(pos) => i = pos + 1,
                None => {
                    carry = Carry::Template;
                    i = len;
                }
            },
            b'{' => {
                brace_delta += 1;
                i += 1;
            }
            b'}' => {
                brace_delta -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }

    ScannedLine {
        code_start,
        code_end: code_end.max(code_start),
        carry,
        brace_delta,
    }
}

/// First `needle` at or after `from` that is not backslash escaped.
fn find_unescaped(bytes: &[u8], from: usize, needle: u8) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            byte if byte == needle => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// First occurrence of the two-byte sequence `first`,`second` at or after `from`.
fn find_pair(bytes: &[u8], from: usize, first: u8, second: u8) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    uf_infra::memchr_iter(first, &bytes[from..])
        .map(|offset| from + offset)
        .find(|&pos| bytes.get(pos + 1) == Some(&second))
}

/// Whether `byte` can appear inside a JavaScript identifier.
#[inline]
pub(crate) fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Whether the byte at `at` starts a word (nothing word-ish precedes it).
#[inline]
pub(crate) fn starts_word(haystack: &str, at: usize) -> bool {
    at == 0 || !is_word_byte(haystack.as_bytes()[at - 1])
}

/// Whether `at + len` ends a word (nothing word-ish follows it).
#[inline]
pub(crate) fn ends_word(haystack: &str, end: usize) -> bool {
    haystack
        .as_bytes()
        .get(end)
        .is_none_or(|&byte| !is_word_byte(byte))
}

/// Byte offsets of every occurrence of `needle` in `haystack`.
///
/// Uses `memchr` on the needle's first byte rather than a naive character walk.
pub(crate) fn find_all<'a>(haystack: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    let first = needle.as_bytes().first().copied();
    let bytes = haystack.as_bytes();
    first
        .into_iter()
        .flat_map(move |first| uf_infra::memchr_iter(first, bytes))
        .filter(move |&at| haystack.as_bytes()[at..].starts_with(needle.as_bytes()))
}

/// Byte offsets of every standalone-word occurrence of `needle` in `haystack`.
pub(crate) fn find_words<'a>(
    haystack: &'a str,
    needle: &'a str,
) -> impl Iterator<Item = usize> + 'a {
    find_all(haystack, needle)
        .filter(move |&at| starts_word(haystack, at) && ends_word(haystack, at + needle.len()))
}

/// The next non-whitespace byte at or after `from`.
#[inline]
pub(crate) fn next_non_space(haystack: &str, from: usize) -> Option<(usize, u8)> {
    haystack.as_bytes()[from.min(haystack.len())..]
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .map(|offset| (from + offset, haystack.as_bytes()[from + offset]))
}

/// The last non-whitespace byte strictly before `before`.
#[inline]
pub(crate) fn prev_non_space(haystack: &str, before: usize) -> Option<(usize, u8)> {
    haystack.as_bytes()[..before.min(haystack.len())]
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|at| (at, haystack.as_bytes()[at]))
}

/// The identifier immediately before `before`, skipping whitespace.
///
/// Returns its start offset and text, or `None` when the preceding token is not
/// an identifier.
#[inline]
pub(crate) fn previous_word(haystack: &str, before: usize) -> Option<(usize, &str)> {
    let (end, byte) = prev_non_space(haystack, before)?;
    if !is_word_byte(byte) {
        return None;
    }
    let bytes = haystack.as_bytes();
    let mut start = end;
    while start > 0 && is_word_byte(bytes[start - 1]) {
        start -= 1;
    }
    Some((start, &haystack[start..=end]))
}

/// Length of the identifier starting at `from`, or zero if none starts there.
#[inline]
pub(crate) fn identifier_len(haystack: &str, from: usize) -> usize {
    let bytes = haystack.as_bytes();
    if from >= bytes.len() || bytes[from].is_ascii_digit() || !is_word_byte(bytes[from]) {
        return 0;
    }
    let mut end = from;
    while end < bytes.len() && is_word_byte(bytes[end]) {
        end += 1;
    }
    end - from
}

/// Whether `name` follows React's `useSomething` hook naming convention.
#[inline]
pub(crate) fn is_hook_name(name: &str) -> bool {
    name.len() > 3 && name.starts_with("use") && name.as_bytes()[3].is_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str) -> SourceFile {
        SourceFile {
            path: "app/index.js".to_string(),
            source: source.to_string(),
        }
    }

    fn codes(source: &str) -> Vec<String> {
        let file = file(source);
        FileScan::new(&file)
            .lines
            .iter()
            .map(|line| line.code().to_string())
            .collect()
    }

    #[test]
    fn line_comments_are_stripped_from_code() {
        assert_eq!(codes("let a = 1; // note\n"), vec!["let a = 1; ", ""]);
    }

    #[test]
    fn urls_inside_strings_are_not_mistaken_for_comments() {
        assert_eq!(
            codes("const u = \"https://x.dev\"; // real\n"),
            vec!["const u = \"https://x.dev\"; ", ""]
        );
    }

    #[test]
    fn block_comments_span_lines() {
        assert_eq!(
            codes("a;\n/* one\n two */ b;\nc;\n"),
            vec!["a;", "", " b;", "c;", ""]
        );
    }

    #[test]
    fn unterminated_template_literals_carry_across_lines() {
        assert_eq!(
            codes("const t = `a\n// not a comment\n`;\n"),
            vec!["const t = `a", "// not a comment", "`;", ""]
        );
    }

    #[test]
    fn escaped_quotes_do_not_end_a_string() {
        assert_eq!(
            codes("const s = 'a\\'// b'; c;\n"),
            vec!["const s = 'a\\'// b'; c;", ""]
        );
    }

    #[test]
    fn crlf_terminators_are_trimmed_from_line_text() {
        let file = file("a;\r\nb;\r\n");
        let scan = FileScan::new(&file);
        assert_eq!(scan.lines[0].text, "a;");
        assert_eq!(scan.lines[1].text, "b;");
    }

    #[test]
    fn line_offsets_match_the_line_index() {
        let file = file("one\ntwo\nthree\n");
        let scan = FileScan::new(&file);
        for (position, line) in scan.lines.iter().enumerate() {
            assert_eq!(scan.index.line_col(line.offset).line, position + 1);
        }
    }

    #[test]
    fn brace_depth_tracks_across_lines() {
        let file = file("component A() {\n  const x = { y: 1 };\n}\n");
        let scan = FileScan::new(&file);
        assert_eq!(scan.lines[0].depth_at_start, 0);
        assert_eq!(scan.lines[1].depth_at_start, 1);
        assert_eq!(scan.lines[2].depth_at_start, 1);
        assert_eq!(scan.lines[3].depth_at_start, 0);
    }

    #[test]
    fn first_code_line_skips_comments_and_blanks() {
        let file = file("\n// @flow\n\n'use client';\n");
        let scan = FileScan::new(&file);
        assert_eq!(scan.facts.first_code_line, Some(3));
    }

    #[test]
    fn empty_input_has_one_empty_line() {
        let file = file("");
        let scan = FileScan::new(&file);
        assert_eq!(scan.lines.len(), 1);
        assert_eq!(scan.facts.first_code_line, None);
    }

    #[test]
    fn byte_order_mark_does_not_break_offsets() {
        let file = file("\u{feff}// @flow\nlet a = 1;\n");
        let scan = FileScan::new(&file);
        assert_eq!(scan.lines[1].text, "let a = 1;");
        assert_eq!(scan.index.line_col(scan.lines[1].offset).line, 2);
    }

    #[test]
    fn non_ascii_lines_keep_byte_offsets_consistent() {
        let file = file("const s = 'ünïcødé'; // ok\nlet a = 1;\n");
        let scan = FileScan::new(&file);
        assert_eq!(scan.lines[1].text, "let a = 1;");
        assert_eq!(scan.index.line_col(scan.lines[1].offset).column, 1);
    }

    #[test]
    fn find_words_respects_identifier_boundaries() {
        assert_eq!(
            find_words("any anything many any", "any").collect::<Vec<_>>(),
            vec![0, 18]
        );
        assert_eq!(
            find_words("$any", "any").collect::<Vec<_>>(),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn find_all_reports_every_occurrence() {
        assert_eq!(find_all("aXbXc", "X").collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(find_all("abc", "").collect::<Vec<_>>(), Vec::<usize>::new());
    }

    #[test]
    fn identifier_len_rejects_digit_starts() {
        assert_eq!(identifier_len("useThing(", 0), 8);
        assert_eq!(identifier_len("9lives", 0), 0);
        assert_eq!(identifier_len("(", 0), 0);
    }

    #[test]
    fn hook_names_need_an_uppercase_fourth_character() {
        assert!(is_hook_name("useState"));
        assert!(!is_hook_name("used"));
        assert!(!is_hook_name("use"));
        assert!(!is_hook_name("useful"));
    }

    #[test]
    fn very_large_input_scans_without_quadratic_blowup() {
        let source = "let a = 1; // c\n".repeat(20_000);
        let file = file(&source);
        let scan = FileScan::new(&file);
        assert_eq!(scan.lines.len(), 20_001);
        assert_eq!(scan.lines[0].code(), "let a = 1; ");
    }
}
