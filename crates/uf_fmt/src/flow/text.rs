//! The source text, indexed the way the printer asks questions of it.
//!
//! The port reports every location as a line and a column counted in UTF-8
//! bytes from the line start. The printer wants absolute byte offsets — to slice raw text, and to ask the
//! questions Prettier asks of the original source: is the next line blank,
//! what is the next character after this node that is not a comment, is
//! there a newline between here and there. [`SourceText`] converts once and
//! answers the rest, and the answers are ports of the helpers in Prettier's
//! `utils`, byte for byte, so the layout decisions built on them match.

use uf_flow::{Loc, Position};

/// A byte range in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// First byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
}

/// The source with a line index over it.
pub struct SourceText<'a> {
    text: &'a str,
    /// Byte offset where each line starts; line one is entry zero.
    line_starts: Vec<usize>,
}

impl<'a> SourceText<'a> {
    /// Index `text`.
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = Vec::with_capacity(text.len() / 32 + 1);
        line_starts.push(0);
        line_starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|at| at + 1));
        Self { text, line_starts }
    }

    /// The whole source.
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// The bytes in `span`, clamped to the source.
    pub fn slice(&self, span: Span) -> &'a str {
        let end = span.end.min(self.text.len());
        let start = span.start.min(end);
        self.text.get(start..end).unwrap_or("")
    }

    /// Byte offset of a parser position.
    ///
    /// The port's lexer counts columns in UTF-8 bytes from the start of the
    /// line (measured, not documented: a `…` advances the column by three),
    /// so a position is one addition. Positions past the end of the source
    /// clamp to it, and one that lands inside a multi-byte character — which
    /// the lexer never produces — is moved back to the character's start so
    /// a slice at it cannot panic.
    pub fn offset(&self, position: Position) -> usize {
        let line = usize::try_from(position.line).unwrap_or(0).max(1) - 1;
        let column = usize::try_from(position.column).unwrap_or(0);
        let Some(&start) = self.line_starts.get(line) else {
            return self.text.len();
        };
        let line_end = self
            .line_starts
            .get(line + 1)
            .map_or(self.text.len(), |&next| next);
        let mut at = (start + column).min(line_end);
        while at > start && !self.text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// Byte span of a location.
    pub fn span(&self, loc: &Loc) -> Span {
        let start = self.offset(loc.start);
        let end = self.offset(loc.end).max(start);
        Span { start, end }
    }

    /// One-based line holding byte `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.line_starts.binary_search(&offset) {
            Ok(index) => index + 1,
            Err(index) => index,
        }
    }

    fn byte(&self, at: usize) -> Option<u8> {
        self.text.as_bytes().get(at).copied()
    }

    /// Skip spaces and tabs from `from`, forwards or backwards. [`None`] when
    /// the end of the text is reached.
    fn skip_spaces(&self, from: Option<usize>, backwards: bool) -> Option<usize> {
        self.skip_while(from, backwards, |byte| byte == b' ' || byte == b'\t')
    }

    /// Skip anything that is not a line terminator.
    fn skip_everything_but_newline(&self, from: Option<usize>) -> Option<usize> {
        self.skip_while(from, false, |byte| byte != b'\n' && byte != b'\r')
    }

    /// Skip `,`, `;`, spaces and tabs: the bytes that may follow a node on
    /// its own line before a comment or a newline.
    fn skip_to_line_end(&self, from: Option<usize>) -> Option<usize> {
        self.skip_while(from, false, |byte| {
            matches!(byte, b',' | b';' | b' ' | b'\t')
        })
    }

    fn skip_while(
        &self,
        from: Option<usize>,
        backwards: bool,
        keep_going: impl Fn(u8) -> bool,
    ) -> Option<usize> {
        let bytes = self.text.as_bytes();
        let mut cursor = from?;
        if backwards {
            loop {
                let &byte = bytes.get(cursor)?;
                if !keep_going(byte) {
                    return Some(cursor);
                }
                cursor = cursor.checked_sub(1)?;
            }
        } else {
            while let Some(&byte) = bytes.get(cursor) {
                if !keep_going(byte) {
                    return Some(cursor);
                }
                cursor += 1;
            }
            None
        }
    }

    /// Skip a `/* … */` comment starting exactly at `from`.
    fn skip_inline_comment(&self, from: Option<usize>) -> Option<usize> {
        let start = from?;
        if self.byte(start) == Some(b'/') && self.byte(start + 1) == Some(b'*') {
            let rest = &self.text.as_bytes()[start + 2..];
            if let Some(at) = memchr::memmem::find(rest, b"*/") {
                return Some(start + 2 + at + 2);
            }
        }
        Some(start)
    }

    /// Skip a `// …` comment starting exactly at `from`.
    fn skip_trailing_comment(&self, from: Option<usize>) -> Option<usize> {
        let start = from?;
        if self.byte(start) == Some(b'/') && self.byte(start + 1) == Some(b'/') {
            return self.skip_everything_but_newline(Some(start));
        }
        Some(start)
    }

    /// Skip one line terminator at `from`, forwards or backwards.
    fn skip_newline(&self, from: Option<usize>, backwards: bool) -> Option<usize> {
        let at = from?;
        let bytes = self.text.as_bytes();
        if backwards {
            if at >= 1 && bytes.get(at - 1) == Some(&b'\r') && bytes.get(at) == Some(&b'\n') {
                return at.checked_sub(2);
            }
            if matches!(bytes.get(at), Some(b'\n' | b'\r')) {
                return at.checked_sub(1);
            }
            if at >= 2 && is_unicode_line_terminator(bytes, at - 2) {
                return at.checked_sub(3);
            }
            return Some(at);
        }
        if bytes.get(at) == Some(&b'\r') && bytes.get(at + 1) == Some(&b'\n') {
            return Some(at + 2);
        }
        if matches!(bytes.get(at), Some(b'\n' | b'\r')) {
            return Some(at + 1);
        }
        if is_unicode_line_terminator(bytes, at) {
            return Some(at + 3);
        }
        Some(at)
    }

    /// Whether a newline separates `at` from the previous (or next, when not
    /// `backwards`) non-space byte. Prettier's `hasNewline`.
    pub fn has_newline(&self, at: usize, backwards: bool) -> bool {
        let from = if backwards {
            at.checked_sub(1)
        } else {
            Some(at)
        };
        let index = self.skip_spaces(from, backwards);
        let after = self.skip_newline(index, backwards);
        index != after
    }

    /// Whether a `\n` occurs in `start..end`.
    pub fn has_newline_in_range(&self, start: usize, end: usize) -> bool {
        let end = end.min(self.text.len());
        let start = start.min(end);
        memchr::memchr(b'\n', &self.text.as_bytes()[start..end]).is_some()
    }

    /// Whether the line before the one holding `at` is blank.
    pub fn is_previous_line_empty(&self, at: usize) -> bool {
        let index = at.checked_sub(1);
        let index = self.skip_spaces(index, true);
        let index = self.skip_newline(index, true);
        let index = self.skip_spaces(index, true);
        let after = self.skip_newline(index, true);
        index != after
    }

    /// Whether the line after the one ending the node at `at` is blank,
    /// looking past trailing separators and comments on the node's line.
    /// Prettier's `isNextLineEmpty`.
    pub fn is_next_line_empty(&self, at: usize) -> bool {
        let mut index = Some(at);
        let mut previous = None;
        while index != previous {
            previous = index;
            index = self.skip_to_line_end(index);
            index = self.skip_inline_comment(index);
            index = self.skip_spaces(index, false);
        }
        index = self.skip_trailing_comment(index);
        index = self.skip_newline(index, false);
        match index {
            Some(index) => self.has_newline(index, false),
            None => false,
        }
    }

    /// Byte offset of the next character after `at` that is neither
    /// whitespace nor inside a comment.
    pub fn next_non_space_non_comment_index(&self, at: usize) -> Option<usize> {
        let mut index = Some(at);
        let mut previous = None;
        while index != previous {
            previous = index;
            index = self.skip_spaces(index, false);
            index = self.skip_inline_comment(index);
            index = self.skip_trailing_comment(index);
            index = self.skip_newline(index, false);
        }
        index
    }

    /// The next character after `at` that is neither whitespace nor inside a
    /// comment.
    pub fn next_non_space_non_comment_character(&self, at: usize) -> Option<char> {
        let index = self.next_non_space_non_comment_index(at)?;
        self.text.get(index..)?.chars().next()
    }
}

fn is_unicode_line_terminator(bytes: &[u8], at: usize) -> bool {
    bytes.get(at) == Some(&0xe2)
        && bytes.get(at + 1) == Some(&0x80)
        && matches!(bytes.get(at + 2), Some(0xa8 | 0xa9))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(line: i32, column: i32) -> Position {
        Position { line, column }
    }

    #[test]
    fn offsets_count_columns_in_utf8_bytes() {
        let text = SourceText::new("ab\n日本c\nz");
        assert_eq!(text.offset(position(1, 1)), 1);
        assert_eq!(text.offset(position(2, 0)), 3);
        // Two three-byte characters, then `c`.
        assert_eq!(text.offset(position(2, 6)), 9);
        assert_eq!(text.offset(position(3, 0)), 11);
        assert_eq!(text.offset(position(9, 9)), text.text().len());
    }

    #[test]
    fn an_offset_inside_a_character_moves_back_to_its_start() {
        let text = SourceText::new("日本");
        assert_eq!(text.offset(position(1, 1)), 0);
        assert_eq!(text.offset(position(1, 3)), 3);
    }

    #[test]
    fn next_line_empty_looks_past_trailing_comments() {
        let text = SourceText::new("a; // c\n\nb;");
        assert!(text.is_next_line_empty(1));
        let text = SourceText::new("a; /* c\n */\n\nb;");
        assert!(text.is_next_line_empty(1));
        let text = SourceText::new("a;\nb;");
        assert!(!text.is_next_line_empty(1));
    }

    #[test]
    fn previous_line_empty_sees_the_blank_line() {
        let text = SourceText::new("a;\n\n  b;");
        assert!(text.is_previous_line_empty(6));
        let text = SourceText::new("a;\n  b;");
        assert!(!text.is_previous_line_empty(5));
    }

    #[test]
    fn has_newline_checks_both_directions() {
        let text = SourceText::new("a  \n  b");
        assert!(text.has_newline(1, false));
        assert!(text.has_newline(6, true));
        assert!(!text.has_newline(0, true));
        let text = SourceText::new("a b");
        assert!(!text.has_newline(1, false));
    }

    #[test]
    fn next_non_space_non_comment_character_skips_comments() {
        let text = SourceText::new("a /* x */ // y\n  , b");
        assert_eq!(text.next_non_space_non_comment_character(1), Some(','));
        assert_eq!(text.next_non_space_non_comment_character(20), None);
    }

    #[test]
    fn line_of_maps_offsets_back_to_lines() {
        let text = SourceText::new("ab\ncd\n");
        assert_eq!(text.line_of(0), 1);
        assert_eq!(text.line_of(2), 1);
        assert_eq!(text.line_of(3), 2);
        assert_eq!(text.line_of(6), 3);
    }
}
