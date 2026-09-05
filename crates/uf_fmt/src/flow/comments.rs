//! Deciding which node each comment belongs to: Prettier's `attachComments`.
//!
//! A comment has no place in the syntax tree, but the printer builds output
//! from the tree, so before printing every comment is attached to a node as
//! *leading* (printed before it), *trailing* (printed after it) or
//! *dangling* (printed inside it, when the node has nothing else to print).
//! The choice follows Prettier exactly, because it is what decides that a
//! comment on its own line above a property stays above the property, that
//! a comment after a `,` stays on that line, and that a comment inside `{}`
//! stays inside.
//!
//! The rules are in three layers, as in Prettier's `main/comments/attach.js`
//! and the estree printer's `handle-comments.js`:
//!
//! 1. [`decorate`] finds the comment's *enclosing* node (the deepest node
//!    whose span contains it) and, among that node's children, the
//!    *preceding* and *following* nodes.
//! 2. The comment is classified by the newlines around it — own line, end
//!    of line, or remaining — and a list of handlers for that class gets the
//!    first say; the handlers are the special cases where the default would
//!    print the comment somewhere surprising (`if (a /* c */)`, a comment
//!    before `else`, a comment in an empty argument list).
//! 3. Otherwise the defaults apply: own-line comments lead the following
//!    node, end-of-line comments trail the preceding one, and a comment with
//!    code on both sides becomes a *tie* resolved by the whitespace between
//!    it and the following node.
//!
//! Every comment ends up in exactly one slot. The printer checks that every
//! slot was printed, so a comment that the printer forgets is an error
//! rather than a silent loss.

use uf_flow::ast::{self, CommentKind, statement};
use uf_flow::ast::{expression, pattern, types};
use uf_infra::{FxHashMap, SmallVec};

use super::node::{NodeKey, NodeRef, Program};
use super::text::{SourceText, Span};

/// Where a comment prints relative to its node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Before the node.
    Leading,
    /// After the node.
    Trailing,
    /// Inside the node, which has no children to attach to.
    Dangling,
}

/// One comment, ready to print.
#[derive(Debug, Clone, Copy)]
pub struct Comment<'a> {
    /// Bytes of the whole comment, delimiters included.
    pub span: Span,
    /// Line or block.
    pub kind: CommentKind,
    /// The text between the delimiters.
    pub text: &'a str,
    /// How the comment was classified: on its own line, at the end of a
    /// line, or with code on both sides.
    pub placement_class: PlacementClass,
    /// The tag a handler can leave so the printer knows which of a node's
    /// dangling slots the comment belongs in.
    pub marker: Marker,
}

/// Which of Prettier's three classes a comment fell in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementClass {
    /// Nothing but whitespace before it on its line.
    OwnLine,
    /// Nothing but whitespace after it on its line.
    EndOfLine,
    /// Code on both sides.
    Remaining,
}

/// A tag on a dangling comment naming the position it prints in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Marker {
    /// Wherever the node prints its dangling comments.
    #[default]
    None,
    /// Before the `else` of an `if` whose consequent is a bare statement on
    /// the same line.
    IfElseSameLine,
    /// Before the `implements` of a class.
    Implements,
    /// Before the `extends` of a class or interface.
    Extends,
    /// Before the `mixins` of a declared class.
    Mixins,
    /// Between an arrow function's parameters and its `=>`.
    CommentBeforeArrow,
}

/// The comments attached to one node.
#[derive(Debug, Default, Clone)]
pub struct Slots {
    /// Printed before the node, in source order.
    pub leading: SmallVec<[u32; 2]>,
    /// Printed after the node, in source order.
    pub trailing: SmallVec<[u32; 2]>,
    /// Printed inside the node, in source order.
    pub dangling: SmallVec<[u32; 2]>,
}

/// Every comment in a file and the node each is attached to.
pub struct Comments<'a> {
    comments: Vec<Comment<'a>>,
    slots: FxHashMap<NodeKey, Slots>,
    /// Which comments have been printed, so the printer can check none was
    /// lost. A `Cell` would let this be `&self`; the printer holds `&mut`.
    printed: Vec<bool>,
}

impl<'a> Comments<'a> {
    /// Attach every comment in `program`.
    pub fn attach(program: &'a Program, text: &SourceText<'a>) -> Self {
        let comments: Vec<Comment<'a>> = program
            .all_comments
            .iter()
            .map(|comment| Comment {
                span: text.span(&comment.loc),
                kind: comment.kind,
                text: &comment.text,
                placement_class: PlacementClass::Remaining,
                marker: Marker::None,
            })
            .collect();
        let mut attacher = Attacher {
            text,
            program,
            comments,
            slots: FxHashMap::default(),
            ties: Vec::new(),
        };
        attacher.run();
        let printed = vec![false; attacher.comments.len()];
        Comments {
            comments: attacher.comments,
            slots: attacher.slots,
            printed,
        }
    }

    /// The comment with index `index`.
    pub fn get(&self, index: u32) -> Comment<'a> {
        self.comments[index as usize]
    }

    /// The slots for `key`, if any comment is attached there.
    pub fn slots(&self, key: NodeKey) -> Option<&Slots> {
        self.slots.get(&key)
    }

    /// Whether any comment is attached to `key` in `placement`.
    pub fn has(&self, key: NodeKey, placement: Placement) -> bool {
        self.slots.get(&key).is_some_and(|slots| match placement {
            Placement::Leading => !slots.leading.is_empty(),
            Placement::Trailing => !slots.trailing.is_empty(),
            Placement::Dangling => !slots.dangling.is_empty(),
        })
    }

    /// Whether any comment at all is attached to `key`.
    pub fn has_any(&self, key: NodeKey) -> bool {
        self.slots.contains_key(&key)
    }

    /// Record that comment `index` was printed.
    pub fn mark_printed(&mut self, index: u32) {
        if let Some(slot) = self.printed.get_mut(index as usize) {
            *slot = true;
        }
    }

    /// The comments that were never printed. Empty is the only acceptable
    /// answer.
    pub fn unprinted(&self) -> Vec<Comment<'a>> {
        self.printed
            .iter()
            .zip(&self.comments)
            .filter(|(printed, _)| !**printed)
            .map(|(_, comment)| *comment)
            .collect()
    }
}

/// Where a comment sits relative to the nodes around it.
#[derive(Clone, Copy)]
struct Context<'a> {
    index: usize,
    enclosing: Option<NodeRef<'a>>,
    preceding: Option<NodeRef<'a>>,
    following: Option<NodeRef<'a>>,
}

struct Attacher<'a, 't> {
    text: &'t SourceText<'a>,
    program: &'a Program,
    comments: Vec<Comment<'a>>,
    slots: FxHashMap<NodeKey, Slots>,
    ties: Vec<Context<'a>>,
}

impl<'a> Attacher<'a, '_> {
    fn run(&mut self) {
        if self.comments.is_empty() {
            return;
        }
        let root = NodeRef::Program(self.program);
        let contexts: Vec<Context<'a>> = (0..self.comments.len())
            .map(|index| {
                let (enclosing, preceding, following) =
                    self.decorate(root, self.comments[index].span, None);
                Context {
                    index,
                    enclosing,
                    preceding,
                    following,
                }
            })
            .collect();

        for (position, context) in contexts.iter().enumerate() {
            let comment = self.comments[context.index];
            let is_last = position + 1 == contexts.len();
            if self.is_own_line_comment(&contexts, position) {
                self.comments[context.index].placement_class = PlacementClass::OwnLine;
                if self.handle_own_line(*context) {
                    continue;
                }
                match (context.following, context.preceding, context.enclosing) {
                    (Some(following), _, _) => {
                        self.add(following, Placement::Leading, comment, context.index)
                    }
                    (None, Some(preceding), _) => {
                        self.add(preceding, Placement::Trailing, comment, context.index)
                    }
                    (None, None, Some(enclosing)) => {
                        self.add(enclosing, Placement::Dangling, comment, context.index)
                    }
                    (None, None, None) => {
                        self.add(root, Placement::Dangling, comment, context.index)
                    }
                }
            } else if self.is_end_of_line_comment(&contexts, position) {
                self.comments[context.index].placement_class = PlacementClass::EndOfLine;
                if self.handle_end_of_line(*context) {
                    continue;
                }
                match (context.preceding, context.following, context.enclosing) {
                    (Some(preceding), _, _) => {
                        self.add(preceding, Placement::Trailing, comment, context.index)
                    }
                    (None, Some(following), _) => {
                        self.add(following, Placement::Leading, comment, context.index)
                    }
                    (None, None, Some(enclosing)) => {
                        self.add(enclosing, Placement::Dangling, comment, context.index)
                    }
                    (None, None, None) => {
                        self.add(root, Placement::Dangling, comment, context.index)
                    }
                }
            } else {
                self.comments[context.index].placement_class = PlacementClass::Remaining;
                if self.handle_remaining(*context, is_last) {
                    continue;
                }
                match (context.preceding, context.following, context.enclosing) {
                    (Some(_), Some(following), _) => {
                        if let Some(last) = self.ties.last()
                            && last.following.map(|node| node.key()) != Some(following.key())
                        {
                            self.break_ties();
                        }
                        self.ties.push(*context);
                    }
                    (Some(preceding), None, _) => {
                        self.add(preceding, Placement::Trailing, comment, context.index)
                    }
                    (None, Some(following), _) => {
                        self.add(following, Placement::Leading, comment, context.index)
                    }
                    (None, None, Some(enclosing)) => {
                        self.add(enclosing, Placement::Dangling, comment, context.index)
                    }
                    (None, None, None) => {
                        self.add(root, Placement::Dangling, comment, context.index)
                    }
                }
            }
        }
        self.break_ties();
        for slots in self.slots.values_mut() {
            let by_start = |a: &u32, b: &u32| {
                self.comments[*a as usize]
                    .span
                    .start
                    .cmp(&self.comments[*b as usize].span.start)
            };
            slots.leading.sort_by(by_start);
            slots.trailing.sort_by(by_start);
            slots.dangling.sort_by(by_start);
        }
    }

    fn add(
        &mut self,
        node: NodeRef<'a>,
        placement: Placement,
        _comment: Comment<'a>,
        index: usize,
    ) {
        let slots = self.slots.entry(node.key()).or_default();
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        match placement {
            Placement::Leading => slots.leading.push(index),
            Placement::Trailing => slots.trailing.push(index),
            Placement::Dangling => slots.dangling.push(index),
        }
    }

    fn add_marked(&mut self, node: NodeRef<'a>, index: usize, marker: Marker) {
        self.comments[index].marker = marker;
        let comment = self.comments[index];
        self.add(node, Placement::Dangling, comment, index);
    }

    fn span(&self, node: NodeRef<'a>) -> Span {
        self.text.span(&node.loc())
    }

    /// Sorted children of `node`, with the ones that cannot carry comments
    /// replaced by their own children.
    fn sorted_children(&self, node: NodeRef<'a>) -> Vec<(Span, NodeRef<'a>)> {
        let mut raw = Vec::new();
        node.children(&mut raw);
        let mut children: Vec<(Span, NodeRef<'a>)> = raw
            .into_iter()
            .map(|child| (self.span(child), child))
            .collect();
        children.sort_by_key(|(span, _)| (span.start, span.end));
        children
    }

    /// Prettier's `decorateComment`: the enclosing, preceding and following
    /// nodes of a comment, found by descending into whichever child
    /// contains it.
    fn decorate(
        &self,
        node: NodeRef<'a>,
        comment: Span,
        enclosing: Option<NodeRef<'a>>,
    ) -> (
        Option<NodeRef<'a>>,
        Option<NodeRef<'a>>,
        Option<NodeRef<'a>>,
    ) {
        let mut node = node;
        let mut enclosing = enclosing;
        // Iterative rather than recursive: the descent is one child per
        // level, and the tree can be as deep as the nesting ceiling allows.
        loop {
            let children = self.sorted_children(node);
            let mut preceding = None;
            let mut following = None;
            let mut left = 0usize;
            let mut right = children.len();
            let mut descend = None;
            while left < right {
                let middle = (left + right) / 2;
                let (span, child) = children[middle];
                if span.start <= comment.start && comment.end <= span.end {
                    descend = Some(child);
                    break;
                }
                if span.end <= comment.start {
                    preceding = Some(child);
                    left = middle + 1;
                    continue;
                }
                if comment.end <= span.start {
                    following = Some(child);
                    right = middle;
                    continue;
                }
                // The comment overlaps a node boundary, which the parser
                // does not produce; treat the node as preceding.
                preceding = Some(child);
                left = middle + 1;
            }
            match descend {
                Some(child) => {
                    enclosing = Some(child);
                    node = child;
                }
                None => {
                    if let Some(NodeRef::Expression(expression)) = enclosing
                        && let expression::ExpressionInner::TemplateLiteral { inner, .. } =
                            &**expression
                    {
                        // Comments inside one `${}` must not move to another.
                        let slot = self.template_slot(inner, comment);
                        if let Some(p) = preceding
                            && self.template_slot(inner, self.span(p)) != slot
                        {
                            preceding = None;
                        }
                        if let Some(f) = following
                            && self.template_slot(inner, self.span(f)) != slot
                        {
                            following = None;
                        }
                    }
                    return (enclosing, preceding, following);
                }
            }
        }
    }

    /// Which `${}` of a template literal `span` falls in: the number of
    /// quasis that end before it.
    fn template_slot(
        &self,
        template: &expression::TemplateLiteral<uf_flow::Loc, uf_flow::Loc>,
        span: Span,
    ) -> usize {
        template
            .quasis
            .iter()
            .take_while(|quasi| self.text.span(&quasi.loc).end <= span.start)
            .count()
    }

    fn is_own_line_comment(&self, contexts: &[Context<'a>], position: usize) -> bool {
        let context = contexts[position];
        let mut start = self.comments[context.index].span.start;
        if let Some(preceding) = context.preceding {
            let key = preceding.key();
            for earlier in contexts[..position].iter().rev() {
                let comment = self.comments[earlier.index];
                if earlier.preceding.map(|node| node.key()) != Some(key)
                    || !self.is_all_empty_and_no_line_break(comment.span.end, start)
                {
                    break;
                }
                start = comment.span.start;
            }
        }
        self.text.has_newline(start, true)
    }

    fn is_end_of_line_comment(&self, contexts: &[Context<'a>], position: usize) -> bool {
        let context = contexts[position];
        let mut end = self.comments[context.index].span.end;
        if let Some(following) = context.following {
            let key = following.key();
            for later in &contexts[position + 1..] {
                let comment = self.comments[later.index];
                if later.following.map(|node| node.key()) != Some(key)
                    || !self.is_all_empty_and_no_line_break(end, comment.span.start)
                {
                    break;
                }
                end = comment.span.end;
            }
        }
        self.text.has_newline(end, false)
    }

    fn is_all_empty_and_no_line_break(&self, start: usize, end: usize) -> bool {
        self.text
            .slice(Span { start, end })
            .chars()
            .all(|ch| ch.is_whitespace() && ch != '\n' && ch != '\u{2028}' && ch != '\u{2029}')
    }

    /// Prettier's `breakTies`: a run of comments with code on both sides,
    /// all between the same two nodes, is split so that the ones separated
    /// from the following node only by whitespace lead it and the rest
    /// trail the preceding node.
    fn break_ties(&mut self) {
        let Some(first) = self.ties.first().copied() else {
            return;
        };
        let (Some(preceding), Some(following)) = (first.preceding, first.following) else {
            self.ties.clear();
            return;
        };
        let mut gap_end = self.span(following).start;
        let mut first_leading = self.ties.len();
        while first_leading > 0 {
            let comment = self.comments[self.ties[first_leading - 1].index];
            let gap = self.text.slice(Span {
                start: comment.span.end,
                end: gap_end,
            });
            if is_gap(gap) {
                gap_end = comment.span.start;
                first_leading -= 1;
            } else {
                break;
            }
        }
        let ties = std::mem::take(&mut self.ties);
        for (position, tie) in ties.into_iter().enumerate() {
            let comment = self.comments[tie.index];
            if position < first_leading {
                self.add(preceding, Placement::Trailing, comment, tie.index);
            } else {
                self.add(following, Placement::Leading, comment, tie.index);
            }
        }
    }

    // ---- handlers: the estree printer's special cases ----

    fn handle_own_line(&mut self, context: Context<'a>) -> bool {
        self.handle_last_function_arg(context)
            || self.handle_member_expression(context)
            || self.handle_if_statement(context)
            || self.handle_while(context)
            || self.handle_try_statement(context)
            || self.handle_class(context)
            || self.handle_for(context)
            || self.handle_union_type(context)
            || self.handle_only_comments(context, false)
            || self.handle_module_specifiers(context)
            || self.handle_assignment_pattern(context)
            || self.handle_method_name(context)
            || self.handle_labeled_statement(context)
            || self.handle_break_and_continue(context)
    }

    fn handle_end_of_line(&mut self, context: Context<'a>) -> bool {
        self.handle_last_function_arg(context)
            || self.handle_conditional_expression(context)
            || self.handle_module_specifiers(context)
            || self.handle_if_statement(context)
            || self.handle_while(context)
            || self.handle_try_statement(context)
            || self.handle_class(context)
            || self.handle_labeled_statement(context)
            || self.handle_call_expression(context)
            || self.handle_property(context)
            || self.handle_only_comments(context, false)
            || self.handle_variable_declarator(context)
            || self.handle_break_and_continue(context)
            || self.handle_switch_default_case(context)
            || self.handle_last_union_element(context)
    }

    fn handle_remaining(&mut self, context: Context<'a>, is_last: bool) -> bool {
        self.handle_if_statement(context)
            || self.handle_while(context)
            || self.handle_object_property(context)
            || self.handle_comment_in_empty_parens(context)
            || self.handle_method_name(context)
            || self.handle_only_comments(context, is_last)
            || self.handle_comment_after_arrow_params(context)
            || self.handle_break_and_continue(context)
    }

    fn comment(&self, context: Context<'a>) -> Comment<'a> {
        self.comments[context.index]
    }

    fn next_character_after(&self, context: Context<'a>) -> Option<char> {
        self.text
            .next_non_space_non_comment_character(self.comment(context).span.end)
    }

    /// A block's first non-empty statement takes the comment as leading;
    /// an empty block takes it as dangling. Prettier's
    /// `addBlockStatementFirstComment`.
    fn add_block_first(&mut self, block: NodeRef<'a>, context: Context<'a>) {
        let comment = self.comment(context);
        let mut children = Vec::new();
        block.children(&mut children);
        let first = children.into_iter().find(|child| {
            !matches!(child, NodeRef::Statement(statement) if matches!(***statement, statement::StatementInner::Empty { .. }))
        });
        match first {
            Some(first) => self.add(first, Placement::Leading, comment, context.index),
            None => self.add(block, Placement::Dangling, comment, context.index),
        }
    }

    fn add_block_or_not(&mut self, node: NodeRef<'a>, context: Context<'a>) {
        if is_block(node) {
            self.add_block_first(node, context);
        } else {
            let comment = self.comment(context);
            self.add(node, Placement::Leading, comment, context.index);
        }
    }

    fn handle_if_statement(&mut self, context: Context<'a>) -> bool {
        let (Some(NodeRef::Statement(enclosing)), Some(following)) =
            (context.enclosing, context.following)
        else {
            return false;
        };
        let statement::StatementInner::If { inner, .. } = &**enclosing else {
            return false;
        };
        let comment = self.comment(context);
        if self.next_character_after(context) == Some(')')
            && let Some(preceding) = context.preceding
        {
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        let consequent = NodeRef::Statement(&inner.consequent);
        if let (Some(preceding), Some(alternate)) = (context.preceding, &inner.alternate)
            && preceding.key() == consequent.key()
            && following.key() == NodeRef::Alternate(alternate).key()
        {
            if is_block(preceding) {
                self.add(preceding, Placement::Trailing, comment, context.index);
            } else {
                let single_line = matches!(comment.kind, CommentKind::Line)
                    || self.text.line_of(comment.span.start) == self.text.line_of(comment.span.end);
                let same_line = self.text.line_of(comment.span.start)
                    == self.text.line_of(self.span(preceding).start);
                let is_expression_statement = matches!(
                    &*inner.consequent,
                    statement::StatementInner::Expression { .. }
                );
                // A comment on the same line as a bare consequent stays
                // on that line, printed after its semicolon; one on its own
                // line goes above the `else`, at statement indentation.
                if same_line && single_line && is_expression_statement {
                    self.add_marked(preceding, context.index, Marker::IfElseSameLine);
                } else {
                    self.add(
                        NodeRef::Statement(enclosing),
                        Placement::Dangling,
                        comment,
                        context.index,
                    );
                }
            }
            return true;
        }
        if is_block(following) {
            self.add_block_first(following, context);
            return true;
        }
        if let NodeRef::Statement(statement) = following
            && let statement::StatementInner::If { inner: nested, .. } = &**statement
        {
            self.add_block_or_not(NodeRef::Statement(&nested.consequent), context);
            return true;
        }
        if following.key() == consequent.key() {
            self.add(following, Placement::Leading, comment, context.index);
            return true;
        }
        false
    }

    fn handle_while(&mut self, context: Context<'a>) -> bool {
        let (Some(NodeRef::Statement(enclosing)), Some(following)) =
            (context.enclosing, context.following)
        else {
            return false;
        };
        let statement::StatementInner::While { inner, .. } = &**enclosing else {
            return false;
        };
        let comment = self.comment(context);
        if self.next_character_after(context) == Some(')')
            && let Some(preceding) = context.preceding
        {
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        if is_block(following) {
            self.add_block_first(following, context);
            return true;
        }
        if following.key() == NodeRef::Statement(&inner.body).key() {
            self.add(following, Placement::Leading, comment, context.index);
            return true;
        }
        false
    }

    fn handle_try_statement(&mut self, context: Context<'a>) -> bool {
        let (Some(enclosing), Some(following)) = (context.enclosing, context.following) else {
            return false;
        };
        let comment = self.comment(context);
        match enclosing {
            NodeRef::Statement(statement)
                if matches!(&**statement, statement::StatementInner::Try { .. }) => {}
            NodeRef::CatchClause(_) => {
                if let Some(preceding) = context.preceding {
                    self.add(preceding, Placement::Trailing, comment, context.index);
                    return true;
                }
            }
            _ => return false,
        }
        if is_block(following) {
            self.add_block_first(following, context);
            return true;
        }
        if let NodeRef::CatchClause(clause) = following {
            self.add_block_first(NodeRef::Block(&clause.body.0, &clause.body.1), context);
            return true;
        }
        false
    }

    fn handle_member_expression(&mut self, context: Context<'a>) -> bool {
        let (Some(NodeRef::Expression(enclosing)), Some(NodeRef::Identifier(_))) =
            (context.enclosing, context.following)
        else {
            return false;
        };
        if matches!(
            &**enclosing,
            expression::ExpressionInner::Member { .. }
                | expression::ExpressionInner::OptionalMember { .. }
        ) {
            let comment = self.comment(context);
            self.add(
                NodeRef::Expression(enclosing),
                Placement::Leading,
                comment,
                context.index,
            );
            return true;
        }
        false
    }

    fn handle_conditional_expression(&mut self, context: Context<'a>) -> bool {
        let Some(following) = context.following else {
            return false;
        };
        let comment = self.comment(context);
        let same_line_as_preceding = context.preceding.is_some_and(|preceding| {
            !self
                .text
                .has_newline_in_range(self.span(preceding).end, comment.span.start)
        });
        let is_conditional = match context.enclosing {
            Some(NodeRef::Expression(expression)) => {
                matches!(
                    &**expression,
                    expression::ExpressionInner::Conditional { .. }
                )
            }
            Some(NodeRef::Type(ty)) => matches!(&**ty, types::TypeInner::Conditional { .. }),
            _ => false,
        };
        if (context.preceding.is_none() || !same_line_as_preceding) && is_conditional {
            self.add(following, Placement::Leading, comment, context.index);
            return true;
        }
        false
    }

    fn handle_object_property(&mut self, context: Context<'a>) -> bool {
        // `{ a /* c */ = 1 }` in a pattern: the comment trails the key.
        let (Some(NodeRef::PatternProperty(property)), Some(preceding)) =
            (context.enclosing, context.preceding)
        else {
            return false;
        };
        let pattern::object::Property::NormalProperty(property) = property else {
            return false;
        };
        if property.shorthand
            && property.default.is_some()
            && let pattern::object::Key::Identifier(key) = &property.key
            && NodeRef::Identifier(key).key() == preceding.key()
        {
            let comment = self.comment(context);
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        false
    }

    fn handle_class(&mut self, context: Context<'a>) -> bool {
        let Some(enclosing) = context.enclosing else {
            return false;
        };
        let comment = self.comment(context);
        let class = match enclosing {
            NodeRef::Statement(statement) => match &**statement {
                statement::StatementInner::ClassDeclaration { inner, .. } => Some(&**inner),
                _ => None,
            },
            NodeRef::Expression(expression) => match &**expression {
                expression::ExpressionInner::Class { inner, .. } => Some(&**inner),
                _ => None,
            },
            _ => None,
        };
        if let Some(class) = class {
            if let Some(last) = class.class_decorators.last()
                && !matches!(context.following, Some(NodeRef::Decorator(_)))
            {
                self.add(
                    NodeRef::Decorator(last),
                    Placement::Trailing,
                    comment,
                    context.index,
                );
                return true;
            }
            let body = NodeRef::ClassBody(&class.body);
            if let Some(following) = context.following {
                if following.key() == body.key() {
                    self.add_block_first(body, context);
                    return true;
                }
                let id_or_tparams = |node: NodeRef<'a>| {
                    class
                        .id
                        .as_ref()
                        .is_some_and(|id| NodeRef::Identifier(id).key() == node.key())
                        || class
                            .tparams
                            .as_ref()
                            .is_some_and(|tparams| NodeRef::TypeParams(tparams).key() == node.key())
                };
                if let Some(extends) = &class.extends
                    && following.key() == NodeRef::Expression(&extends.expr).key()
                    && let Some(preceding) = context.preceding
                    && id_or_tparams(preceding)
                {
                    self.add(preceding, Placement::Trailing, comment, context.index);
                    return true;
                }
                if let Some(implements) = &class.implements
                    && let Some(first) = implements.interfaces.first()
                    && following.key() == NodeRef::ClassImplements(first).key()
                {
                    let is_super = class.extends.as_ref().zip(context.preceding).is_some_and(
                        |(extends, preceding)| {
                            NodeRef::Expression(&extends.expr).key() == preceding.key()
                        },
                    );
                    match context.preceding {
                        Some(preceding) if id_or_tparams(preceding) || is_super => {
                            self.add(preceding, Placement::Trailing, comment, context.index);
                        }
                        _ => self.add_marked(enclosing, context.index, Marker::Implements),
                    }
                    return true;
                }
            }
            return false;
        }

        // Declared classes and interfaces: comments before the body lead its
        // first property; comments before `extends`/`mixins` are marked.
        let (id, tparams, body, extends_first, mixins_first) = match enclosing {
            NodeRef::Statement(statement) => match &**statement {
                statement::StatementInner::DeclareClass { inner, .. } => (
                    &inner.id,
                    inner.tparams.as_ref(),
                    NodeRef::ObjectType(&inner.body.0, &inner.body.1),
                    // `declare class C extends B` has no node for the
                    // `extends` clause — its children are spliced into the
                    // declaration's, so `B` arrives as a bare identifier and
                    // there is no `InterfaceExtends` key to compare against
                    // the way an interface gives one. The first child that is
                    // neither the name nor its type parameters is the head of
                    // the heritage; the body is handled before this is used,
                    // so a declaration with no heritage cannot reach it.
                    self.first_child_after_name(enclosing, &inner.id, inner.tparams.as_ref()),
                    inner.mixins.first().map(NodeRef::InterfaceExtends),
                ),
                statement::StatementInner::DeclareInterface { inner, .. }
                | statement::StatementInner::InterfaceDeclaration { inner, .. } => (
                    &inner.id,
                    inner.tparams.as_ref(),
                    NodeRef::ObjectType(&inner.body.0, &inner.body.1),
                    inner.extends.first().map(NodeRef::InterfaceExtends),
                    None,
                ),
                _ => return false,
            },
            _ => return false,
        };
        let Some(following) = context.following else {
            return false;
        };
        let id_or_tparams = |node: NodeRef<'a>| {
            NodeRef::Identifier(id).key() == node.key()
                || tparams.is_some_and(|tparams| NodeRef::TypeParams(tparams).key() == node.key())
        };

        if following.key() == body.key() {
            self.add_block_first(body, context);
            return true;
        }
        for (first, marker) in [
            (extends_first, Marker::Extends),
            (mixins_first, Marker::Mixins),
        ] {
            if let Some(first) = first
                && following.key() == first.key()
            {
                match context.preceding {
                    Some(preceding) if id_or_tparams(preceding) => {
                        self.add(preceding, Placement::Trailing, comment, context.index);
                    }
                    _ => self.add_marked(enclosing, context.index, marker),
                }
                return true;
            }
        }
        false
    }

    /// The first child of `node` that is neither `id` nor `tparams`.
    ///
    /// For a declaration whose heritage clause is not a node of its own, and
    /// so gives nothing to compare a key against. See the `DeclareClass` arm
    /// of [`handle_class`](Self::handle_class).
    fn first_child_after_name(
        &self,
        node: NodeRef<'a>,
        id: &'a ast::Identifier<uf_flow::Loc, uf_flow::Loc>,
        tparams: Option<&'a types::TypeParams<uf_flow::Loc, uf_flow::Loc>>,
    ) -> Option<NodeRef<'a>> {
        self.sorted_children(node)
            .into_iter()
            .map(|(_, child)| child)
            .find(|child| {
                NodeRef::Identifier(id).key() != child.key()
                    && !tparams
                        .is_some_and(|tparams| NodeRef::TypeParams(tparams).key() == child.key())
            })
    }

    fn handle_method_name(&mut self, context: Context<'a>) -> bool {
        let (Some(enclosing), Some(preceding)) = (context.enclosing, context.preceding) else {
            return false;
        };
        let comment = self.comment(context);
        // `obj = { fn /* c */() {} }`: the comment trails the name.
        let is_name = matches!(preceding, NodeRef::Identifier(_) | NodeRef::PrivateName(_));
        let key_of_enclosing: Option<NodeKey> = match enclosing {
            NodeRef::ObjectProperty(expression::object::Property::NormalProperty(property)) => {
                object_key_node(match property {
                    expression::object::NormalProperty::Init { key, .. }
                    | expression::object::NormalProperty::Method { key, .. }
                    | expression::object::NormalProperty::Get { key, .. }
                    | expression::object::NormalProperty::Set { key, .. } => key,
                })
                .map(|node| node.key())
            }
            NodeRef::ClassMember(member) => match member {
                ast::class::BodyElement::Method(method) => {
                    object_key_node(&method.key).map(|node| node.key())
                }
                ast::class::BodyElement::DeclareMethod(method) => {
                    object_key_node(&method.key).map(|node| node.key())
                }
                ast::class::BodyElement::AbstractMethod(method) => {
                    object_key_node(&method.key).map(|node| node.key())
                }
                _ => None,
            },
            _ => None,
        };
        if is_name
            && key_of_enclosing == Some(preceding.key())
            && self.next_character_after(context) == Some('(')
            && self
                .text
                .next_non_space_non_comment_character(self.span(preceding).end)
                != Some(':')
        {
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        // `@dec /* c */ method() {}`: the comment trails the decorator.
        if let NodeRef::Decorator(_) = preceding
            && matches!(enclosing, NodeRef::ClassMember(_) | NodeRef::Param(_))
            && (matches!(comment.kind, CommentKind::Line)
                || comment.placement_class == PlacementClass::OwnLine)
        {
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        false
    }

    fn handle_comment_after_arrow_params(&mut self, context: Context<'a>) -> bool {
        let Some(NodeRef::Expression(enclosing)) = context.enclosing else {
            return false;
        };
        if !matches!(
            &**enclosing,
            expression::ExpressionInner::ArrowFunction { .. }
        ) {
            return false;
        }
        let comment = self.comment(context);
        let Some(index) = self.text.next_non_space_non_comment_index(comment.span.end) else {
            return false;
        };
        if self
            .text
            .text()
            .get(index..)
            .is_some_and(|rest| rest.starts_with("=>"))
        {
            self.add_marked(
                NodeRef::Expression(enclosing),
                context.index,
                Marker::CommentBeforeArrow,
            );
            return true;
        }
        false
    }

    fn handle_comment_in_empty_parens(&mut self, context: Context<'a>) -> bool {
        if self.next_character_after(context) != Some(')') {
            return false;
        }
        let Some(enclosing) = context.enclosing else {
            return false;
        };
        let comment = self.comment(context);
        let target = match enclosing {
            NodeRef::Statement(statement) => match &**statement {
                statement::StatementInner::FunctionDeclaration { inner, .. }
                    if function_has_no_params(inner) =>
                {
                    Some(enclosing)
                }
                _ => None,
            },
            NodeRef::Expression(expression) => match &**expression {
                expression::ExpressionInner::Function { inner, .. }
                | expression::ExpressionInner::ArrowFunction { inner, .. }
                    if function_has_no_params(inner) =>
                {
                    Some(enclosing)
                }
                expression::ExpressionInner::Call { inner, .. }
                    if inner.arguments.arguments.is_empty() =>
                {
                    Some(enclosing)
                }
                expression::ExpressionInner::OptionalCall { inner, .. }
                    if inner.call.arguments.arguments.is_empty() =>
                {
                    Some(enclosing)
                }
                expression::ExpressionInner::New { inner, .. }
                    if inner
                        .arguments
                        .as_ref()
                        .is_none_or(|arguments| arguments.arguments.is_empty()) =>
                {
                    Some(enclosing)
                }
                _ => None,
            },
            NodeRef::FunctionValue(_, function) if function_has_no_params(function) => {
                Some(enclosing)
            }
            NodeRef::ClassMember(ast::class::BodyElement::Method(method))
                if function_has_no_params(&method.value.1) =>
            {
                Some(NodeRef::FunctionValue(&method.value.0, &method.value.1))
            }
            NodeRef::ObjectProperty(expression::object::Property::NormalProperty(
                expression::object::NormalProperty::Method { value, .. }
                | expression::object::NormalProperty::Get { value, .. }
                | expression::object::NormalProperty::Set { value, .. },
            )) if function_has_no_params(&value.1) => {
                Some(NodeRef::FunctionValue(&value.0, &value.1))
            }
            _ => None,
        };
        match target {
            Some(target) => {
                self.add(target, Placement::Dangling, comment, context.index);
                true
            }
            None => false,
        }
    }

    fn handle_last_function_arg(&mut self, context: Context<'a>) -> bool {
        let comment = self.comment(context);
        // Function type parameters: `(a: T /* c */) => void`.
        if let (Some(preceding @ NodeRef::FunctionTypeParam(_)), Some(enclosing)) =
            (context.preceding, context.enclosing)
            && is_function_type(enclosing)
            && !matches!(context.following, Some(NodeRef::FunctionTypeParam(_)))
        {
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        // Real functions: a comment after the last parameter, before `)`.
        if let (Some(preceding), Some(enclosing)) = (context.preceding, context.enclosing)
            && matches!(
                preceding,
                NodeRef::Param(_)
                    | NodeRef::RestParam(_)
                    | NodeRef::ThisParam(_)
                    | NodeRef::Pattern(_)
            )
            && is_real_function(enclosing)
            && self.next_character_after(context) == Some(')')
        {
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        // `function f() /* c */ {}`: into the body.
        if let (Some(NodeRef::Statement(statement)), Some(following)) =
            (context.enclosing, context.following)
            && let statement::StatementInner::FunctionDeclaration { inner, .. } = &**statement
            && is_block(following)
        {
            let right_paren = self.function_right_paren(inner);
            if right_paren.is_some_and(|index| comment.span.start > index) {
                self.add_block_first(following, context);
                return true;
            }
        }
        false
    }

    /// Byte offset of the `)` closing a function's parameter list.
    fn function_right_paren(&self, function: &'a super::node::Function) -> Option<usize> {
        let last_param_end = function
            .params
            .rest
            .as_ref()
            .map(|rest| self.text.span(&rest.loc).end)
            .or_else(|| {
                function
                    .params
                    .params
                    .last()
                    .map(|param| self.span(NodeRef::Param(param)).end)
            });
        match last_param_end {
            Some(end) => self.text.next_non_space_non_comment_index(end),
            None => {
                let id_end = self.text.span(&function.id.as_ref()?.loc).end;
                let left = self.text.next_non_space_non_comment_index(id_end)?;
                self.text.next_non_space_non_comment_index(left + 1)
            }
        }
    }

    fn handle_labeled_statement(&mut self, context: Context<'a>) -> bool {
        if let Some(NodeRef::Statement(statement)) = context.enclosing
            && matches!(&**statement, statement::StatementInner::Labeled { .. })
        {
            let comment = self.comment(context);
            self.add(
                NodeRef::Statement(statement),
                Placement::Leading,
                comment,
                context.index,
            );
            return true;
        }
        false
    }

    fn handle_break_and_continue(&mut self, context: Context<'a>) -> bool {
        if let Some(NodeRef::Statement(statement)) = context.enclosing {
            let unlabeled = match &**statement {
                statement::StatementInner::Break { inner, .. } => inner.label.is_none(),
                statement::StatementInner::Continue { inner, .. } => inner.label.is_none(),
                _ => false,
            };
            if unlabeled {
                let comment = self.comment(context);
                self.add(
                    NodeRef::Statement(statement),
                    Placement::Trailing,
                    comment,
                    context.index,
                );
                return true;
            }
        }
        false
    }

    fn handle_call_expression(&mut self, context: Context<'a>) -> bool {
        let (Some(NodeRef::Expression(enclosing)), Some(preceding)) =
            (context.enclosing, context.preceding)
        else {
            return false;
        };
        let (callee, arguments) = match &**enclosing {
            expression::ExpressionInner::Call { inner, .. } => {
                (&inner.callee, inner.arguments.arguments.first())
            }
            expression::ExpressionInner::OptionalCall { inner, .. } => {
                (&inner.call.callee, inner.call.arguments.arguments.first())
            }
            expression::ExpressionInner::New { inner, .. } => (
                &inner.callee,
                inner
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.arguments.first()),
            ),
            _ => return false,
        };
        if let Some(first) = arguments
            && NodeRef::Expression(callee).key() == preceding.key()
        {
            let comment = self.comment(context);
            let first = match first {
                expression::ExpressionOrSpread::Expression(expression) => {
                    NodeRef::Expression(expression)
                }
                expression::ExpressionOrSpread::Spread(spread) => NodeRef::Spread(spread),
            };
            self.add(first, Placement::Leading, comment, context.index);
            return true;
        }
        false
    }

    fn handle_union_type(&mut self, context: Context<'a>) -> bool {
        if let Some(NodeRef::Type(ty)) = context.enclosing
            && matches!(&**ty, types::TypeInner::Union { .. })
            && let Some(preceding) = context.preceding
        {
            let comment = self.comment(context);
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        false
    }

    fn handle_last_union_element(&mut self, context: Context<'a>) -> bool {
        // `(A | B /* c */)[]`: the comment trails the last member.
        if context.following.is_none()
            && let Some(NodeRef::Type(enclosing)) = context.enclosing
            && matches!(&**enclosing, types::TypeInner::Array { .. })
            && let Some(NodeRef::Type(preceding)) = context.preceding
            && let types::TypeInner::Union { inner, .. } = &**preceding
        {
            let last = inner.types.2.last().unwrap_or(&inner.types.1);
            let comment = self.comment(context);
            self.add(
                NodeRef::Type(last),
                Placement::Trailing,
                comment,
                context.index,
            );
            return true;
        }
        false
    }

    fn handle_property(&mut self, context: Context<'a>) -> bool {
        if let Some(
            enclosing @ NodeRef::ObjectProperty(expression::object::Property::NormalProperty(_)),
        ) = context.enclosing
        {
            let comment = self.comment(context);
            self.add(enclosing, Placement::Leading, comment, context.index);
            return true;
        }
        false
    }

    fn handle_only_comments(&mut self, context: Context<'a>, is_last: bool) -> bool {
        let root = NodeRef::Program(self.program);
        let program_is_empty = self.program.statements.is_empty();
        let comment = self.comment(context);
        if program_is_empty {
            if is_last {
                self.add(root, Placement::Dangling, comment, context.index);
            } else {
                self.add(root, Placement::Leading, comment, context.index);
            }
            return true;
        }
        false
    }

    fn handle_for(&mut self, context: Context<'a>) -> bool {
        if let Some(NodeRef::Statement(statement)) = context.enclosing
            && matches!(
                &**statement,
                statement::StatementInner::ForIn { .. } | statement::StatementInner::ForOf { .. }
            )
        {
            let comment = self.comment(context);
            self.add(
                NodeRef::Statement(statement),
                Placement::Leading,
                comment,
                context.index,
            );
            return true;
        }
        false
    }

    fn handle_module_specifiers(&mut self, context: Context<'a>) -> bool {
        let comment = self.comment(context);
        if let Some(enclosing @ (NodeRef::ImportSpecifier(_) | NodeRef::ExportSpecifier(_))) =
            context.enclosing
        {
            self.add(enclosing, Placement::Leading, comment, context.index);
            return true;
        }
        let in_declaration = match (context.preceding, context.enclosing) {
            (Some(NodeRef::ImportSpecifier(_)), Some(NodeRef::Statement(statement))) => {
                matches!(
                    &**statement,
                    statement::StatementInner::ImportDeclaration { .. }
                )
            }
            (Some(NodeRef::ExportSpecifier(_)), Some(NodeRef::Statement(statement))) => {
                matches!(
                    &**statement,
                    statement::StatementInner::ExportNamedDeclaration { .. }
                )
            }
            _ => false,
        };
        if in_declaration
            && self.text.has_newline(comment.span.end, false)
            && let Some(preceding) = context.preceding
        {
            self.add(preceding, Placement::Trailing, comment, context.index);
            return true;
        }
        false
    }

    fn handle_assignment_pattern(&mut self, context: Context<'a>) -> bool {
        // A parameter with a default, `(a /* c */ = 1) => {}`: the comment
        // leads the whole parameter.
        if let Some(enclosing @ (NodeRef::Param(_) | NodeRef::PatternElement(_))) =
            context.enclosing
        {
            let has_default = match enclosing {
                NodeRef::Param(ast::function::Param::RegularParam { default, .. }) => {
                    default.is_some()
                }
                NodeRef::PatternElement(_) => true,
                _ => false,
            };
            if has_default {
                let comment = self.comment(context);
                self.add(enclosing, Placement::Leading, comment, context.index);
                return true;
            }
        }
        false
    }

    fn handle_variable_declarator(&mut self, context: Context<'a>) -> bool {
        let Some(following) = context.following else {
            return false;
        };
        let assignment_like = match context.enclosing {
            Some(NodeRef::Declarator(_)) => true,
            Some(NodeRef::Expression(expression)) => {
                matches!(
                    &**expression,
                    expression::ExpressionInner::Assignment { .. }
                )
            }
            Some(NodeRef::Statement(statement)) => matches!(
                &**statement,
                statement::StatementInner::TypeAlias { .. }
                    | statement::StatementInner::DeclareTypeAlias { .. }
            ),
            _ => false,
        };
        if !assignment_like {
            return false;
        }
        let comment = self.comment(context);
        let complex = match following {
            NodeRef::Expression(expression) => matches!(
                &**expression,
                expression::ExpressionInner::Object { .. }
                    | expression::ExpressionInner::Record { .. }
                    | expression::ExpressionInner::Array { .. }
                    | expression::ExpressionInner::TemplateLiteral { .. }
                    | expression::ExpressionInner::TaggedTemplate { .. }
            ),
            NodeRef::Type(ty) => matches!(&**ty, types::TypeInner::Object { .. }),
            _ => false,
        };
        if complex || matches!(comment.kind, CommentKind::Block) {
            self.add(following, Placement::Leading, comment, context.index);
            return true;
        }
        false
    }

    fn handle_switch_default_case(&mut self, context: Context<'a>) -> bool {
        let (Some(NodeRef::SwitchCase(case)), Some(following)) =
            (context.enclosing, context.following)
        else {
            return false;
        };
        if case.test.is_some() {
            return false;
        }
        let Some(first) = case.consequent.first() else {
            return false;
        };
        if following.key() != NodeRef::Statement(first).key() {
            return false;
        }
        let comment = self.comment(context);
        if is_block(following) && matches!(comment.kind, CommentKind::Line) {
            self.add_block_first(following, context);
        } else {
            self.add(
                NodeRef::SwitchCase(case),
                Placement::Dangling,
                comment,
                context.index,
            );
        }
        true
    }
}

/// Prettier's `isGap` for Flow: whitespace and `(` between a tied comment
/// and the following node do not break the tie, and neither does the start
/// of a Flow comment type.
fn is_gap(text: &str) -> bool {
    let compact: String = text
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '(')
        .collect();
    compact.is_empty() || compact == "/*" || compact == "/*::"
}

/// Whether `node` is a block: a block statement or a bare block body.
pub fn is_block(node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::Block(..) => true,
        NodeRef::Statement(statement) => {
            matches!(&**statement, statement::StatementInner::Block { .. })
        }
        _ => false,
    }
}

fn is_function_type(node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::FunctionType(..) => true,
        NodeRef::Type(ty) => matches!(
            &**ty,
            types::TypeInner::Function { .. } | types::TypeInner::ConstructorType { .. }
        ),
        _ => false,
    }
}

fn is_real_function(node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::Statement(statement) => matches!(
            &**statement,
            statement::StatementInner::FunctionDeclaration { .. }
        ),
        NodeRef::Expression(expression) => matches!(
            &**expression,
            expression::ExpressionInner::Function { .. }
                | expression::ExpressionInner::ArrowFunction { .. }
        ),
        NodeRef::FunctionValue(..) => true,
        NodeRef::ClassMember(ast::class::BodyElement::Method(_)) => true,
        _ => false,
    }
}

fn function_has_no_params(function: &super::node::Function) -> bool {
    function.params.params.is_empty()
        && function.params.rest.is_none()
        && function.params.this_.is_none()
}

/// The attachable node for an object key.
fn object_key_node<'a>(
    key: &'a expression::object::Key<uf_flow::Loc, uf_flow::Loc>,
) -> Option<NodeRef<'a>> {
    use expression::object::Key;
    Some(match key {
        Key::Identifier(id) => NodeRef::Identifier(id),
        Key::PrivateName(name) => NodeRef::PrivateName(name),
        Key::StringLiteral((loc, literal)) => NodeRef::StringLiteral(loc, literal),
        Key::NumberLiteral((loc, literal)) => NodeRef::NumberLiteral(loc, literal),
        Key::BigIntLiteral((loc, literal)) => NodeRef::BigIntLiteral(loc, literal),
        Key::Computed(_) => return None,
    })
}
