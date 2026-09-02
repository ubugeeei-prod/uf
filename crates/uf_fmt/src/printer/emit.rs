//! The second pass: turning the analysis into output text.
//!
//! This is the only place that knows the live output column, which is what lets
//! it explode a group the analysis marked breakable but could not measure. It
//! also owns the pending state between tokens -- queued newlines, a queued
//! significant JSX space -- so that trivia is emitted once and only once.

use uf_config::{FmtConfig, QuoteStyle};

use super::analyze::{Analysis, Anno, display_width};
use super::quote::{requote, requote_jsx};
use super::statement::{ends_a_statement, starts_a_statement};
use super::{MAX_INDENT_LEVELS, NO_MATCH};
use crate::lexer::{Punctuator, Token, TokenKind};

/// A bracketed group that is currently open in the output.
#[derive(Debug, Clone, Copy)]
struct OpenGroup {
    close: u32,
    broken: bool,
    statements: bool,
}

pub(crate) struct Emitter<'a> {
    source: &'a str,
    tokens: &'a [Token],
    analysis: &'a Analysis,
    indent_width: usize,
    line_width: u32,
    quotes: QuoteStyle,
    semicolons: bool,
    max_blank_lines: usize,
    out: String,
    indent_cache: String,
    column: u32,
    pending_newlines: usize,
    pending_jsx_space: Option<usize>,
    groups: Vec<OpenGroup>,
    /// How many currently open groups the printer exploded itself. The analysis
    /// pass could not know about those, so their indentation is added here.
    broken_depth: u16,
    prev_emitted: Option<usize>,
    prev_significant: Option<usize>,
    started: bool,
}

impl<'a> Emitter<'a> {
    pub(crate) fn new(
        source: &'a str,
        tokens: &'a [Token],
        analysis: &'a Analysis,
        config: &FmtConfig,
    ) -> Self {
        Self {
            source,
            tokens,
            analysis,
            indent_width: usize::from(config.indent_width),
            line_width: u32::from(config.line_width),
            quotes: config.quotes,
            semicolons: config.semicolons,
            max_blank_lines: usize::from(config.max_blank_lines),
            out: String::with_capacity(source.len() + source.len() / 8 + 16),
            indent_cache: String::new(),
            column: 0,
            pending_newlines: 0,
            pending_jsx_space: None,
            groups: Vec::with_capacity(16),
            broken_depth: 0,
            prev_emitted: None,
            prev_significant: None,
            started: false,
        }
    }

    pub(crate) fn run(mut self) -> String {
        for index in 0..self.tokens.len() {
            match self.tokens[index].kind {
                TokenKind::Newline => {
                    self.pending_newlines += 1;
                    self.pending_jsx_space = None;
                }
                TokenKind::Whitespace => {
                    if self.analysis.annos[index].jsx_space && self.pending_newlines == 0 {
                        self.pending_jsx_space = Some(index);
                    }
                }
                _ => self.emit(index),
            }
        }

        if self.semicolons && self.needs_semicolon(None) {
            self.out.push(';');
        }
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
        self.out
    }

    /// Append text whose printed width is already known.
    fn write_measured(&mut self, text: &str, width: u32, multiline: bool) {
        self.out.push_str(text);
        self.column = if multiline {
            width
        } else {
            self.column.saturating_add(width)
        };
    }

    fn write(&mut self, text: &str) {
        let multiline = memchr::memchr(b'\n', text.as_bytes()).is_some();
        self.write_measured(text, display_width(text), multiline);
    }

    fn write_indent(&mut self, level: u16) {
        let width = usize::from(level) * self.indent_width;
        while self.indent_cache.len() < width {
            self.indent_cache.push(' ');
        }
        self.out.push_str(&self.indent_cache[..width]);
        self.column = u32::try_from(width).unwrap_or(u32::MAX);
    }

    fn emit(&mut self, index: usize) {
        let token = self.tokens[index];

        if token.kind == TokenKind::Punctuator(Punctuator::Semicolon)
            && !self.semicolons
            && self.can_drop_semicolon(index)
        {
            return;
        }

        self.open_line(index);

        let text = token.text(self.source);
        let rewritten = match token.kind {
            TokenKind::String => requote(text, self.quotes),
            TokenKind::JsxString => requote_jsx(text, self.quotes),
            _ => None,
        };
        match rewritten {
            Some(rewritten) => self.write(&rewritten),
            None => {
                let anno = self.analysis.annos[index];
                self.write_measured(text, anno.width, anno.multiline);
            }
        }

        self.prev_emitted = Some(index);
        if !token.kind.is_comment() {
            self.prev_significant = Some(index);
        }

        self.update_groups(index);
    }

    /// Emit whatever separates this token from the previous one: newlines and
    /// indentation, a single space, or nothing at all.
    fn open_line(&mut self, index: usize) {
        let anno = self.analysis.annos[index];

        if !self.started {
            self.started = true;
            self.pending_newlines = 0;
            self.pending_jsx_space = None;
            let indent = self.indent_of(index);
            if indent > 0 {
                self.write_indent(indent);
            }
            return;
        }

        if self.pending_newlines > 0 {
            if self.semicolons && self.needs_semicolon(Some(index)) {
                self.out.push(';');
                self.column = self.column.saturating_add(1);
            }
            let mut count = self.pending_newlines.min(self.max_blank_lines + 1);
            if self.hugs_a_delimiter(index) {
                count = 1;
            }
            for _ in 0..count {
                self.out.push('\n');
            }
            self.column = 0;
            self.pending_newlines = 0;
            self.pending_jsx_space = None;
            let indent = self.indent_of(index);
            self.write_indent(indent);
            return;
        }

        if let Some(space) = self.pending_jsx_space.take() {
            let text = self.tokens[space].text(self.source);
            self.write(text);
            return;
        }

        if anno.space_before {
            self.out.push(' ');
            self.column = self.column.saturating_add(1);
        }
    }

    /// Indentation for a line starting at `index`, including the levels added by
    /// groups this printer exploded on its own.
    fn indent_of(&self, index: usize) -> u16 {
        let mut extra = self.broken_depth;
        if self
            .groups
            .last()
            .is_some_and(|group| group.broken && group.close as usize == index)
        {
            extra = extra.saturating_sub(1);
        }
        self.analysis.annos[index]
            .indent
            .saturating_add(extra)
            .min(MAX_INDENT_LEVELS)
    }

    /// Blank lines are dropped directly inside a delimiter pair.
    fn hugs_a_delimiter(&self, index: usize) -> bool {
        let opens = matches!(
            self.prev_emitted.map(|prev| self.tokens[prev].kind),
            Some(TokenKind::Punctuator(punctuator)) if punctuator.is_open_delimiter()
        );
        let closes = matches!(
            self.tokens[index].kind,
            TokenKind::Punctuator(punctuator) if punctuator.is_close_delimiter()
        );
        opens || closes
    }

    fn update_groups(&mut self, index: usize) {
        let kind = self.tokens[index].kind;
        let anno = self.analysis.annos[index];

        if matches!(kind, TokenKind::Punctuator(punctuator) if punctuator.is_close_delimiter()) {
            if self
                .groups
                .last()
                .is_some_and(|group| group.close as usize == index)
                && let Some(group) = self.groups.pop()
                && group.broken
            {
                self.broken_depth = self.broken_depth.saturating_sub(1);
            }
        } else if matches!(kind, TokenKind::Punctuator(punctuator) if punctuator.is_open_delimiter())
        {
            let broken = anno.breakable && anno.close != NO_MATCH && !self.group_fits(index, anno);
            self.groups.push(OpenGroup {
                close: anno.close,
                broken,
                statements: anno.statement_group,
            });
            if broken {
                self.broken_depth = self.broken_depth.saturating_add(1);
                self.pending_newlines = 1;
            }
        } else if matches!(
            kind,
            TokenKind::Punctuator(Punctuator::Comma | Punctuator::Semicolon)
        ) && !anno.in_angle
            && self.groups.last().is_some_and(|group| group.broken)
        {
            // Inside a group we exploded ourselves, every top-level separator
            // starts a new line.
            self.pending_newlines = 1;
        }

        if let Some(group) = self.groups.last()
            && group.broken
            && group.close != NO_MATCH
            && self.next_significant(index) == Some(group.close as usize)
        {
            self.pending_newlines = self.pending_newlines.max(1);
        }
    }

    /// Whether the group opening at `index` still fits on the current line.
    fn group_fits(&self, index: usize, anno: Anno) -> bool {
        if self.line_width == 0 {
            return true;
        }
        let close = anno.close as usize;
        let stop = usize::min(
            self.analysis.line_end[close] as usize,
            self.analysis.cost.len() - 1,
        );
        let span = self.analysis.cost[stop].saturating_sub(self.analysis.cost[index + 1]);
        self.column.saturating_add(span) <= self.line_width
    }

    fn next_significant(&self, index: usize) -> Option<usize> {
        self.tokens[index + 1..]
            .iter()
            .position(|token| !token.kind.is_trivia())
            .map(|offset| index + 1 + offset)
    }

    /// Whether a statement-terminating semicolon should be written before the
    /// token at `next`, which is `None` at the end of the file.
    ///
    /// The rule is deliberately conservative. Automatic semicolon insertion only
    /// fires when the next line cannot continue the current expression, so a next
    /// token such as `(`, `[`, `` ` `` or an operator blocks insertion: writing a
    /// semicolon there would change what the program means.
    fn needs_semicolon(&self, next: Option<usize>) -> bool {
        // Never write a semicolon after a comment; it would land outside the
        // statement it is meant to terminate.
        if self
            .prev_emitted
            .is_some_and(|index| self.tokens[index].kind.is_comment())
        {
            return false;
        }
        let Some(prev) = self.prev_significant else {
            return false;
        };
        let prev_kind = self.tokens[prev].kind;
        let ends_statement = ends_a_statement(prev_kind)
            || (prev_kind == TokenKind::Punctuator(Punctuator::CloseBrace)
                && self.analysis.annos[prev].object_close);
        if !ends_statement || self.analysis.annos[prev].in_jsx {
            return false;
        }
        if !self.in_statement_position() {
            return false;
        }
        // A comment sitting between two statements must not hide the statement
        // that follows it.
        let next = next.and_then(|index| self.next_program_token(index));
        match next {
            None => true,
            Some(index) => starts_a_statement(self.tokens[index].kind),
        }
    }

    /// The token at `index`, or the first non-comment token after it.
    fn next_program_token(&self, index: usize) -> Option<usize> {
        self.tokens[index..]
            .iter()
            .position(|token| !token.kind.is_trivia())
            .map(|offset| index + offset)
    }

    /// Whether the innermost open group holds statements: the file itself, a
    /// block, a class body or a switch body. Parenthesised groups have no
    /// statements and object literals separate their members with commas.
    fn in_statement_position(&self) -> bool {
        self.groups.last().is_none_or(|group| group.statements)
    }

    /// Whether a `;` may be removed when semicolons are switched off.
    fn can_drop_semicolon(&self, index: usize) -> bool {
        let Some(prev) = self.prev_significant else {
            return false;
        };
        let prev_kind = self.tokens[prev].kind;
        // A `}` cannot gain a synthesized semicolon, because a block must not get
        // one, but an existing `};` after an object or class may be dropped.
        if !ends_a_statement(prev_kind)
            && prev_kind != TokenKind::Punctuator(Punctuator::CloseBrace)
        {
            return false;
        }
        // `if (x);` is an empty statement: dropping the `;` would adopt the next
        // statement as the body.
        if prev_kind == TokenKind::Punctuator(Punctuator::CloseParen)
            && self.analysis.annos[prev].statement_paren
        {
            return false;
        }
        if !self.in_statement_position() {
            return false;
        }
        match self.next_significant(index) {
            None => true,
            Some(next) => starts_a_statement(self.tokens[next].kind),
        }
    }
}
