//! The node printers: Prettier's `printer-estree`, one Rust function per
//! node kind, building a [`Doc`] for [`doc::printer`](crate::doc::printer)
//! to lay out.
//!
//! Everything here mirrors Prettier's own printer closely enough that its
//! output is reproducible rule for rule — the same groups, the same
//! `ifBreak`s, the same conditional groups for call arguments and arrow
//! chains — because Prettier's layout is the specification, and a printer
//! that only resembles it drifts on every construct the tests missed.
//! Where a rule has a name in Prettier (`shouldHugTheOnlyFunctionParameter`,
//! `isMemberChain`, `shouldBreakAfterOperator`) the function here carries
//! the same name in snake case, so the two can be read side by side.
//!
//! [`Printer`] holds what every node printer needs: the doc arena, the
//! source text for the questions layout asks of it, the attached comments,
//! and the stack of ancestors a node's layout depends on.

mod array;
mod assignment;
mod binary;
mod call;
mod class;
mod expression;
mod function;
mod jsx;
mod literal;
mod match_;
mod member_chain;
mod module;
mod object;
mod parens;
mod pattern;
mod statement;
mod ternary;
mod types;

use uf_config::{FmtConfig, QuoteStyle};
use uf_flow::ast::CommentKind;
use uf_infra::FxHashMap;

use super::comments::{Comment, Comments, Marker, Placement};
use super::node::{NodeKey, NodeRef, Program};
use super::text::SourceText;
use crate::doc::{BREAK_PARENT, Doc, Docs, HARDLINE, LINE, LITERALLINE, label_of};

/// The configuration a node printer consults.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// The quote to prefer for strings.
    pub quote: QuoteStyle,
    /// Whether statements end in semicolons.
    pub semi: bool,
    /// Spaces per indentation level; template literals align to it.
    pub indent_width: usize,
    /// Columns a line may use.
    pub line_width: usize,
}

impl Options {
    /// Options from the formatter configuration.
    pub fn from_config(config: &FmtConfig) -> Self {
        Self {
            quote: config.quotes,
            semi: config.semicolons,
            indent_width: usize::from(config.indent_width),
            line_width: usize::from(config.line_width),
        }
    }
}

/// State shared by every node printer.
pub struct Printer<'a> {
    /// Where docs are allocated.
    pub docs: Docs<'a>,
    /// The source, for the questions layout asks of it.
    pub text: &'a SourceText<'a>,
    /// Every comment and the node it belongs to.
    pub comments: Comments<'a>,
    /// The configuration.
    pub options: Options,
    /// The nodes enclosing the one being printed, innermost last.
    pub ancestors: Vec<NodeRef<'a>>,
    /// The program, for the few places a printer needs the root.
    pub program: &'a Program,
    /// Set when a hugged call argument turned out to need its own line
    /// breaks: Prettier's `ArgExpansionBailout`, without the exception.
    pub expansion_bailout: bool,
    /// Arguments already printed, by node, print arguments and parent.
    ///
    /// `print_arguments` prints the argument it is considering hugging a
    /// second time, and so does every level below it, which makes the
    /// document exponential in the nesting: nineteen levels of
    /// `expect.objectContaining` is a file React Native writes and `uf fmt`
    /// did not finish on. See ubugeeei-prod/uf#125.
    ///
    /// The same node, printed with the same arguments in the same place, is
    /// the same document. Keying on the parent as well is not needed for
    /// that — a node has one parent — but it is free and it says what the
    /// entry means.
    pub argument_docs: FxHashMap<ArgumentKey, (Doc<'a>, bool)>,
}

/// What identifies a printed argument: the node, how it was asked to print,
/// and where it sat.
pub type ArgumentKey = (NodeKey, PrintArgs, Option<NodeKey>);

pub use assignment::PrintArgs;

impl<'a> Printer<'a> {
    /// A printer over `program`.
    pub fn new(
        docs: Docs<'a>,
        text: &'a SourceText<'a>,
        comments: Comments<'a>,
        options: Options,
        program: &'a Program,
    ) -> Self {
        Self {
            docs,
            text,
            comments,
            options,
            ancestors: Vec::with_capacity(64),
            program,
            expansion_bailout: false,
            argument_docs: FxHashMap::default(),
        }
    }

    /// Print the whole file: statements separated by newlines, blank lines
    /// preserved (one at most), a final newline.
    pub fn print_program(&mut self) -> Doc<'a> {
        let program = self.program;
        let root = NodeRef::Program(program);
        self.ancestors.push(root);
        let body = self.print_statement_sequence(&program.statements);
        let dangling = self.print_dangling_comments(root.key(), Marker::None, false);
        let leading = self.print_leading_comments(root.key());
        self.ancestors.pop();

        let mut parts = Vec::new();
        // A `#!` line is not a statement and has no comments of its own;
        // it is printed verbatim, keeping a blank line after it when the
        // source had one.
        if let Some((loc, _)) = &program.interpreter {
            let span = self.text.span(loc);
            parts.push(self.docs.borrowed(self.text.slice(span).trim_end()));
            parts.push(&HARDLINE);
            if self.text.is_next_line_empty(span.end) {
                parts.push(&HARDLINE);
            }
        }
        if let Some(leading) = leading {
            parts.push(leading);
        }
        let mut printed_body = false;
        if !crate::doc::is_empty(body) {
            parts.push(body);
            printed_body = true;
        }
        if let Some(dangling) = dangling {
            if printed_body {
                parts.push(&HARDLINE);
            }
            parts.push(dangling);
            printed_body = true;
        }
        if printed_body {
            parts.push(&HARDLINE);
        }
        self.docs.concat_vec(parts)
    }

    // ---- small builders ----

    /// Arena text.
    pub fn text(&self, text: &str) -> Doc<'a> {
        self.docs.text(text)
    }

    /// A literal that lives as long as the arena.
    pub fn s(&self, text: &'static str) -> Doc<'a> {
        self.docs.borrowed(text)
    }

    /// The statement terminator: `;` or nothing.
    pub fn semi(&self) -> Doc<'a> {
        if self.options.semi {
            self.s(";")
        } else {
            self.s("")
        }
    }

    /// A sequence.
    pub fn concat<I>(&self, parts: I) -> Doc<'a>
    where
        I: IntoIterator<Item = Doc<'a>>,
    {
        self.docs.concat(parts)
    }

    /// A group.
    pub fn group(&self, doc: Doc<'a>) -> Doc<'a> {
        self.docs.group(doc)
    }

    /// An indent.
    pub fn indent(&self, doc: Doc<'a>) -> Doc<'a> {
        self.docs.indent(doc)
    }

    /// `parts` joined by `separator`.
    pub fn join(&self, separator: Doc<'a>, parts: Vec<Doc<'a>>) -> Doc<'a> {
        self.docs.join(separator, parts)
    }

    // ---- the ancestor stack ----

    /// The node enclosing the one being printed.
    pub fn parent(&self) -> Option<NodeRef<'a>> {
        self.ancestors.iter().rev().nth(1).copied()
    }

    /// The node enclosing the parent.
    pub fn grandparent(&self) -> Option<NodeRef<'a>> {
        self.ancestors.iter().rev().nth(2).copied()
    }

    /// The node being printed.
    pub fn current(&self) -> Option<NodeRef<'a>> {
        self.ancestors.last().copied()
    }

    /// Run `print` with `node` pushed as the current node.
    pub fn with_node<T>(&mut self, node: NodeRef<'a>, print: impl FnOnce(&mut Self) -> T) -> T {
        self.ancestors.push(node);
        let result = print(self);
        self.ancestors.pop();
        result
    }

    // ---- comments ----

    /// Print `node` with `print`, then attach its leading and trailing
    /// comments around the result. Every attachable node goes through here.
    pub fn print_node(
        &mut self,
        node: NodeRef<'a>,
        print: impl FnOnce(&mut Self) -> Doc<'a>,
    ) -> Doc<'a> {
        let doc = self.with_node(node, print);
        self.print_comments(node.key(), doc)
    }

    /// Wrap `doc` in the leading and trailing comments attached to `key`.
    pub fn print_comments(&mut self, key: NodeKey, doc: Doc<'a>) -> Doc<'a> {
        let leading = self.print_leading_comments(key);
        let trailing = self.print_trailing_comments(key);
        if leading.is_none() && trailing.is_none() {
            return doc;
        }
        let label = label_of(doc);
        let inner = self.docs.concat(
            [leading, Some(doc), trailing]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
        );
        match label {
            Some(label) => self.docs.label(label, inner),
            None => inner,
        }
    }

    /// The text of one comment, delimiters included.
    pub fn print_comment(&mut self, index: u32) -> Doc<'a> {
        self.comments.mark_printed(index);
        let comment = self.comments.get(index);
        match comment.kind {
            CommentKind::Line => {
                let raw = self.text.slice(comment.span).trim_end();
                self.docs.borrowed(raw)
            }
            CommentKind::Block => {
                if is_indentable_block_comment(comment.text) {
                    let lines: Vec<Doc<'a>> = comment
                        .text
                        .split('\n')
                        .enumerate()
                        .map(|(index, line)| {
                            if index == 0 {
                                self.text(line.trim_end())
                            } else {
                                self.text(&format!(" {}", line.trim_start()))
                            }
                        })
                        .collect();
                    let body = self.join(&HARDLINE, lines);
                    // Every line but the last was trimmed on the right; the
                    // last keeps its trailing whitespace before `*/`.
                    self.concat([self.s("/*"), body, self.s("*/")])
                } else {
                    let body = self.replace_end_of_line(comment.text);
                    self.concat([self.s("/*"), body, self.s("*/")])
                }
            }
        }
    }

    /// `text` with each newline replaced by a literal line, so multi-line
    /// text is never re-indented.
    pub fn replace_end_of_line(&self, text: &'a str) -> Doc<'a> {
        if !text.contains('\n') {
            return self.docs.borrowed(text);
        }
        let lines: Vec<Doc<'a>> = text
            .split('\n')
            .map(|line| self.docs.borrowed(line))
            .collect();
        self.join(&LITERALLINE, lines)
    }

    /// The leading comments of `key`, each followed by the break Prettier
    /// puts after it.
    pub fn print_leading_comments(&mut self, key: NodeKey) -> Option<Doc<'a>> {
        let indices: Vec<u32> = self.comments.slots(key)?.leading.iter().copied().collect();
        if indices.is_empty() {
            return None;
        }
        let mut parts = Vec::with_capacity(indices.len() * 3);
        for index in indices {
            let comment = self.comments.get(index);
            parts.push(self.print_comment(index));
            match comment.kind {
                CommentKind::Block => {
                    let after = self.text.has_newline(comment.span.end, false);
                    let before = self.text.has_newline(comment.span.start, true);
                    parts.push(if after {
                        if before { &HARDLINE } else { &LINE }
                    } else {
                        self.s(" ")
                    });
                }
                CommentKind::Line => parts.push(&HARDLINE),
            }
            if self.blank_line_after_comment(comment) {
                parts.push(&HARDLINE);
            }
        }
        Some(self.docs.concat_vec(parts))
    }

    /// Whether the line after `comment` is blank.
    fn blank_line_after_comment(&self, comment: Comment<'a>) -> bool {
        let bytes = self.text.text().as_bytes();
        let mut at = comment.span.end;
        while matches!(bytes.get(at), Some(b' ' | b'\t')) {
            at += 1;
        }
        let at = match bytes.get(at) {
            Some(b'\n') => at + 1,
            Some(b'\r') if bytes.get(at + 1) == Some(&b'\n') => at + 2,
            Some(b'\r') => at + 1,
            _ => return false,
        };
        self.text.has_newline(at, false)
    }

    /// The trailing comments of `key`.
    pub fn print_trailing_comments(&mut self, key: NodeKey) -> Option<Doc<'a>> {
        let indices: Vec<u32> = self.comments.slots(key)?.trailing.iter().copied().collect();
        if indices.is_empty() {
            return None;
        }
        let mut parts = Vec::with_capacity(indices.len());
        let mut previous: Option<(bool, bool)> = None; // (is_block, has_line_suffix)
        for index in indices {
            let comment = self.comments.get(index);
            let printed = self.print_comment(index);
            let is_block = matches!(comment.kind, CommentKind::Block);
            let previous_line_suffix_from_line =
                previous.is_some_and(|(prev_block, suffix)| suffix && !prev_block);
            if previous_line_suffix_from_line || self.text.has_newline(comment.span.start, true) {
                let blank_before = self.text.is_previous_line_empty(comment.span.start);
                let inner = self.concat([
                    &HARDLINE,
                    if blank_before { &HARDLINE } else { self.s("") },
                    printed,
                ]);
                parts.push(self.docs.line_suffix(inner));
                previous = Some((is_block, true));
            } else if !is_block || previous.is_some_and(|(_, suffix)| suffix) {
                let inner = self.concat([self.s(" "), printed]);
                let suffix = self.docs.line_suffix(inner);
                parts.push(if is_block {
                    suffix
                } else {
                    self.concat([suffix, &BREAK_PARENT])
                });
                previous = Some((is_block, true));
            } else {
                parts.push(self.concat([self.s(" "), printed]));
                previous = Some((is_block, false));
            }
        }
        Some(self.docs.concat_vec(parts))
    }

    /// The dangling comments of `key` carrying `marker`, joined by hard
    /// lines, indented on their own line when `indent` is set.
    pub fn print_dangling_comments(
        &mut self,
        key: NodeKey,
        marker: Marker,
        indent: bool,
    ) -> Option<Doc<'a>> {
        let indices: Vec<u32> = self
            .comments
            .slots(key)?
            .dangling
            .iter()
            .copied()
            .filter(|index| self.comments.get(*index).marker == marker)
            .collect();
        if indices.is_empty() {
            return None;
        }
        let parts: Vec<Doc<'a>> = indices
            .into_iter()
            .map(|index| self.print_comment(index))
            .collect();
        let doc = self.join(&HARDLINE, parts);
        Some(if indent {
            self.indent(self.concat([&HARDLINE, doc]))
        } else {
            doc
        })
    }

    /// Whether `key` has a comment matching `placement` and `pred`.
    pub fn has_comment_where(
        &self,
        key: NodeKey,
        placement: Option<Placement>,
        pred: impl Fn(&Comment<'a>) -> bool,
    ) -> bool {
        let Some(slots) = self.comments.slots(key) else {
            return false;
        };
        let check = |indices: &[u32]| indices.iter().any(|index| pred(&self.comments.get(*index)));
        match placement {
            Some(Placement::Leading) => check(&slots.leading),
            Some(Placement::Trailing) => check(&slots.trailing),
            Some(Placement::Dangling) => check(&slots.dangling),
            None => check(&slots.leading) || check(&slots.trailing) || check(&slots.dangling),
        }
    }

    /// Whether any comment is attached to `key`.
    pub fn has_comment(&self, key: NodeKey) -> bool {
        self.comments.has_any(key)
    }

    /// Whether `key` has a comment in `placement`.
    pub fn has_comment_placed(&self, key: NodeKey, placement: Placement) -> bool {
        self.comments.has(key, placement)
    }

    /// Whether `key` has a trailing line comment (which forces a break).
    pub fn has_trailing_line_comment(&self, key: NodeKey) -> bool {
        self.has_comment_where(key, Some(Placement::Trailing), |comment| {
            matches!(comment.kind, CommentKind::Line)
        })
    }

    /// Whether `key` has a leading comment on its own line: Prettier's
    /// `hasLeadingOwnLineComment`.
    pub fn has_leading_own_line_comment(&self, key: NodeKey, is_jsx: bool) -> bool {
        if is_jsx {
            return self.has_comment_where(key, None, |comment| {
                self.text.has_newline(comment.span.end, false)
            });
        }
        self.has_comment_where(key, Some(Placement::Leading), |comment| {
            self.text.has_newline(comment.span.end, false)
        })
    }

    /// Whether `key` has a line comment anywhere.
    pub fn has_line_comment(&self, key: NodeKey, placement: Option<Placement>) -> bool {
        self.has_comment_where(key, placement, |comment| {
            matches!(comment.kind, CommentKind::Line)
        })
    }

    // ---- statements in sequence ----

    /// Statements one per line, with one blank line kept where the source
    /// had any, and empty statements dropped. Prettier's
    /// `printStatementSequence`.
    pub fn print_statement_sequence(
        &mut self,
        statements: &'a [super::node::Statement],
    ) -> Doc<'a> {
        let last = statements
            .iter()
            .rposition(|statement| !statement::is_empty_statement(statement));
        let mut parts = Vec::with_capacity(statements.len() * 2);
        for (index, statement) in statements.iter().enumerate() {
            if statement::is_empty_statement(statement) {
                continue;
            }
            let printed = self.print_statement(statement);
            if !self.options.semi && self.statement_needs_asi_protection(statement) {
                if self.has_comment_placed(NodeRef::Statement(statement).key(), Placement::Leading)
                {
                    // The `;` has to come after the comments, so print again
                    // with the guard inside.
                    let guarded = self.print_statement_with_leading_semi(statement);
                    parts.push(guarded);
                } else {
                    parts.push(self.s(";"));
                    parts.push(printed);
                }
            } else {
                parts.push(printed);
            }
            if Some(index) != last {
                parts.push(&HARDLINE);
                let end = self.text.span(statement.loc()).end;
                if self.text.is_next_line_empty(end) {
                    parts.push(&HARDLINE);
                }
            }
        }
        self.docs.concat_vec(parts)
    }

    /// `text` as a doc, or an `if_break` between two spellings.
    pub fn if_break(&self, break_doc: Doc<'a>, flat_doc: Doc<'a>) -> Doc<'a> {
        self.docs.if_break(break_doc, flat_doc, None)
    }
}

/// Whether every line of a block comment starts with `*`, so it can be
/// re-indented line by line.
fn is_indentable_block_comment(text: &str) -> bool {
    let wrapped = format!("*{text}*");
    let lines: Vec<&str> = wrapped.split('\n').collect();
    lines.len() > 1 && lines.iter().all(|line| line.trim_start().starts_with('*'))
}
