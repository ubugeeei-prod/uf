//! JSX: Prettier's `printJsxElement`, `printJsxChildren` and friends.
//!
//! Whitespace in JSX text is significant, so children are printed as a
//! *fill* of words and separators, where a separator that must survive a
//! line break becomes `{" "}`. The rules for which whitespace matters, and
//! for when an element breaks its children one per line, are Prettier's.

use uf_flow::Loc;
use uf_flow::ast::{expression, function, jsx, statement};

use super::Printer;
use super::literal::choose_quote;
use super::parens::{is_binaryish, is_call, is_jsx, same};
use crate::doc::{Doc, DocKind, HARDLINE, LINE, LINE_SUFFIX_BOUNDARY, SOFTLINE, will_break};
use crate::flow::comments::{Marker, Placement};
use crate::flow::node::{Expression, NodeKey, NodeRef};

/// The characters JSX treats as whitespace between words.
fn is_jsx_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\n' | '\r' | '\t')
}

/// Whether text contributes something to print: non-whitespace, or
/// whitespace without a newline (which is a meaningful space).
fn is_meaningful_text(raw: &str) -> bool {
    raw.chars().any(|ch| !is_jsx_whitespace(ch)) || !raw.contains('\n')
}

fn child_text(child: &jsx::Child<Loc, Loc>) -> Option<&str> {
    match child {
        jsx::Child::Text { inner, .. } => Some(&inner.raw),
        _ => None,
    }
}

fn is_meaningful_child(child: &jsx::Child<Loc, Loc>) -> bool {
    child_text(child).is_some_and(is_meaningful_text)
}

fn is_element_child(child: &jsx::Child<Loc, Loc>) -> bool {
    matches!(
        child,
        jsx::Child::Element { .. } | jsx::Child::Fragment { .. }
    )
}

/// Whether a child is a self-closing-less element, which Prettier treats
/// as separable by a hard line: `isJsxElementWithoutClosing`... in fact the
/// check is "element without a closing tag", i.e. self-closing.
fn is_self_closing_element(child: &jsx::Child<Loc, Loc>) -> bool {
    matches!(child, jsx::Child::Element { inner, .. } if inner.closing_element.is_none())
}

/// `{" "}` in the source: a whitespace expression container.
fn is_jsx_whitespace_expression(child: &jsx::Child<Loc, Loc>) -> bool {
    match child {
        jsx::Child::ExpressionContainer { inner, .. } => match &inner.expression {
            jsx::expression_container::Expression::Expression(expression) => {
                matches!(&**expression, expression::ExpressionInner::StringLiteral { inner, .. } if &*inner.value == " ")
            }
            jsx::expression_container::Expression::EmptyExpression => false,
        },
        _ => false,
    }
}

/// Split JSX text into words, capturing the whitespace runs between them,
/// starting and ending with (possibly empty) whitespace-side entries the
/// way Prettier's `split(text, true)` does.
fn split_words(text: &str) -> Vec<&str> {
    // JavaScript's `text.split(/([ \n\r\t]+)/)`: words and the whitespace
    // runs between them alternate, and the list starts and ends with a
    // word, which is empty when the text starts or ends with whitespace.
    let mut parts: Vec<&str> = Vec::new();
    let mut run_start = 0;
    let mut current_is_space: Option<bool> = None;
    for (index, ch) in text.char_indices() {
        let space = is_jsx_whitespace(ch);
        match current_is_space {
            None => {
                current_is_space = Some(space);
                run_start = index;
                if space {
                    parts.push("");
                }
            }
            Some(previous) if previous == space => {}
            Some(_) => {
                parts.push(&text[run_start..index]);
                run_start = index;
                current_is_space = Some(space);
            }
        }
    }
    match current_is_space {
        None => parts.push(""),
        Some(space) => {
            parts.push(&text[run_start..]);
            if space {
                parts.push("");
            }
        }
    }
    parts
}

/// A child in Prettier's alternating list: content and separators.
#[derive(Clone, Copy)]
enum Part<'a> {
    Empty,
    Word(Doc<'a>),
    Doc(Doc<'a>),
    Line,
    Softline,
    Hardline,
    JsxWhitespace,
}

impl<'a> Part<'a> {
    fn is_line_like(self) -> bool {
        matches!(
            self,
            Part::Empty | Part::Softline | Part::Hardline | Part::Line
        )
    }
}

impl<'a> Printer<'a> {
    /// A JSX element, with its own comments and the parentheses it needs.
    pub fn print_jsx_element(
        &mut self,
        element: &'a jsx::Element<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        let printed =
            self.print_jsx_element_internal(Some(element), None, &element.children.1, key);
        let with_comments = self.print_comments(key, printed);
        self.maybe_wrap_jsx_in_parens(with_comments, expression)
    }

    /// A fragment, likewise.
    pub fn print_jsx_fragment(
        &mut self,
        fragment: &'a jsx::Fragment<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        let printed =
            self.print_jsx_element_internal(None, Some(fragment), &fragment.frag_children.1, key);
        let with_comments = self.print_comments(key, printed);
        self.maybe_wrap_jsx_in_parens(with_comments, expression)
    }

    /// Prettier's `maybeWrapJsxElementInParens`.
    fn maybe_wrap_jsx_in_parens(&mut self, doc: Doc<'a>, expression: &'a Expression) -> Doc<'a> {
        let no_wrap = match self.parent() {
            Some(NodeRef::Expression(parent)) => matches!(
                &**parent,
                expression::ExpressionInner::Array { .. }
                    | expression::ExpressionInner::JSXElement { .. }
                    | expression::ExpressionInner::JSXFragment { .. }
                    | expression::ExpressionInner::New { .. }
                    | expression::ExpressionInner::Call { .. }
                    | expression::ExpressionInner::OptionalCall { .. }
                    | expression::ExpressionInner::Conditional { .. }
            ),
            Some(NodeRef::Statement(statement)) => {
                matches!(**statement, statement::StatementInner::Expression { .. })
            }
            Some(NodeRef::JsxAttribute(_))
            | Some(NodeRef::JsxExpressionContainer(..))
            | Some(NodeRef::JsxChild(_))
            | Some(NodeRef::Spread(_))
            | Some(NodeRef::MatchExpressionCase(_)) => true,
            None => true,
            _ => false,
        };
        if no_wrap {
            return doc;
        }
        let should_break = self.jsx_in_arrow_body_in_call_in_container(expression);
        self.docs.group_with(
            self.concat([
                self.if_break(self.s("("), self.s("")),
                self.indent(self.concat([&SOFTLINE, doc])),
                &SOFTLINE,
                self.if_break(self.s(")"), self.s("")),
            ]),
            should_break,
            None,
        )
    }

    /// `{items.map((item) => <li />)}`: the arrow body breaks so the
    /// element sits on its own line.
    fn jsx_in_arrow_body_in_call_in_container(&self, expression: &'a Expression) -> bool {
        // The member-chain printer pushes a call twice, so the walk skips
        // repeats rather than counting fixed positions.
        let mut frames = self.ancestors.iter().rev().skip(1);
        let Some(NodeRef::Expression(arrow)) = frames.next() else {
            return false;
        };
        let expression::ExpressionInner::ArrowFunction { inner, .. } = &***arrow else {
            return false;
        };
        if !matches!(&inner.body, function::Body::BodyExpression(body) if same(body, expression)) {
            return false;
        }
        let mut seen_call = false;
        for frame in frames {
            match frame {
                NodeRef::Expression(candidate) if is_call(candidate) => seen_call = true,
                NodeRef::Expression(_) if seen_call => {}
                NodeRef::JsxExpressionContainer(..) | NodeRef::JsxChild(_) => return seen_call,
                _ => return false,
            }
        }
        false
    }

    fn print_jsx_element_internal(
        &mut self,
        element: Option<&'a jsx::Element<Loc, Loc>>,
        fragment: Option<&'a jsx::Fragment<Loc, Loc>>,
        children: &'a [jsx::Child<Loc, Loc>],
        key: NodeKey,
    ) -> Doc<'a> {
        // An empty element: `<div></div>`, `<div> </div>`.
        let is_empty = children.is_empty()
            || (children.len() == 1
                && child_text(&children[0]).is_some_and(|text| !is_meaningful_text(text)));
        if let Some(element) = element
            && is_empty
        {
            let opening = self.print_jsx_opening(&element.opening_element);
            let closing = match &element.closing_element {
                Some(closing) => self.print_jsx_closing(closing),
                None => self.s(""),
            };
            return self.concat([opening, closing]);
        }

        let opening = match (element, fragment) {
            (Some(element), _) => self.print_jsx_opening(&element.opening_element),
            (None, Some(fragment)) => self.print_jsx_fragment_tag(fragment, key, true),
            _ => self.s(""),
        };
        let closing = match (element, fragment) {
            (Some(element), _) => match &element.closing_element {
                Some(closing) => self.print_jsx_closing(closing),
                None => self.s(""),
            },
            (None, Some(fragment)) => self.print_jsx_fragment_tag(fragment, key, false),
            _ => self.s(""),
        };

        // A lone template literal child keeps the element on one line.
        if children.len() == 1
            && let jsx::Child::ExpressionContainer { inner, .. } = &children[0]
            && let jsx::expression_container::Expression::Expression(expression) = &inner.expression
            && matches!(
                **expression,
                expression::ExpressionInner::TemplateLiteral { .. }
                    | expression::ExpressionInner::TaggedTemplate { .. }
            )
        {
            let child = self.print_jsx_child(&children[0]);
            return self.concat([opening, child, closing]);
        }

        let contains_tag = children.iter().any(is_element_child);
        // `{" "}` counts as text rather than as an expression, so a line
        // that only spaces out its words does not force a break.
        let contains_multiple_expressions = children
            .iter()
            .filter(|child| {
                matches!(child, jsx::Child::ExpressionContainer { .. })
                    && !is_jsx_whitespace_expression(child)
            })
            .count()
            > 1;
        let contains_multiple_attributes =
            element.is_some_and(|element| element.opening_element.attributes.len() > 1);
        let mut forced_break = will_break(opening)
            || contains_tag
            || contains_multiple_attributes
            || contains_multiple_expressions;

        let raw_jsx_whitespace = match self.options.quote {
            uf_config::QuoteStyle::Single => "{' '}",
            uf_config::QuoteStyle::Double => "{\" \"}",
        };
        let jsx_whitespace = self.if_break(
            self.concat([self.s(raw_jsx_whitespace), &SOFTLINE]),
            self.s(" "),
        );
        let is_fbt = element.is_some_and(|element| matches!(&element.opening_element.name, jsx::Name::Identifier(id) if &*id.name == "fbt"));

        let mut parts = self.print_jsx_children(children, is_fbt);
        let contains_text = children.iter().any(is_meaningful_child);

        // Remove multiple whitespace elements and lines before JSX
        // whitespace, so `{" "}` never doubles up with a break.
        let mut i = parts.len().saturating_sub(2);
        loop {
            if parts.len() < 2 {
                break;
            }
            let at = |index: usize| parts.get(index).copied();
            let a = at(i);
            let b = at(i + 1);
            let c = at(i + 2);
            let is_pair_of_empty = matches!((a, b), (Some(Part::Empty), Some(Part::Empty)));
            let is_pair_of_hardlines = matches!(
                (a, b, c),
                (
                    Some(Part::Hardline),
                    Some(Part::Empty),
                    Some(Part::Hardline)
                )
            );
            let is_line_followed_by_whitespace = matches!(
                (a, b, c),
                (
                    Some(Part::Hardline | Part::Softline),
                    Some(Part::Empty),
                    Some(Part::JsxWhitespace)
                )
            );
            let is_whitespace_followed_by_line = matches!(
                (a, b, c),
                (
                    Some(Part::JsxWhitespace),
                    Some(Part::Empty),
                    Some(Part::Hardline | Part::Softline)
                )
            );
            let is_double_whitespace = matches!(
                (a, b, c),
                (
                    Some(Part::JsxWhitespace),
                    Some(Part::Empty),
                    Some(Part::JsxWhitespace)
                )
            );
            let is_pair_of_hard_or_soft = matches!(
                (a, b, c),
                (
                    Some(Part::Softline),
                    Some(Part::Empty),
                    Some(Part::Hardline)
                ) | (
                    Some(Part::Hardline),
                    Some(Part::Empty),
                    Some(Part::Softline)
                )
            );
            if (is_pair_of_hardlines && contains_text)
                || is_pair_of_empty
                || is_line_followed_by_whitespace
                || is_double_whitespace
                || is_pair_of_hard_or_soft
            {
                parts.drain(i..i + 2);
            } else if is_whitespace_followed_by_line {
                parts.drain(i + 1..i + 3);
            }
            if i == 0 {
                break;
            }
            i -= 1;
        }
        // Trim trailing lines (and empty strings).
        while parts.last().is_some_and(|part| part.is_line_like()) {
            parts.pop();
        }
        // Trim leading lines (and empty strings).
        while parts.len() > 1 && parts[0].is_line_like() && parts[1].is_line_like() {
            parts.drain(0..2);
        }

        // Group the children into content/separator pairs for the fill,
        // turning whitespace separators that must survive into `{" "}`.
        let mut multiline: Vec<Doc<'a>> = vec![self.s("")];
        let raw_ws = self.s(raw_jsx_whitespace);
        let parts_len = parts.len();
        for (index, part) in parts.iter().copied().enumerate() {
            if let Part::JsxWhitespace = part {
                if index == 1 && matches!(parts[0], Part::Empty) {
                    if parts_len == 2 {
                        let last = multiline.pop().unwrap_or(self.s(""));
                        multiline.push(self.concat([last, raw_ws]));
                        continue;
                    }
                    multiline.push(self.concat([raw_ws, &HARDLINE]));
                    multiline.push(self.s(""));
                    continue;
                }
                if index == parts_len - 1 {
                    let last = multiline.pop().unwrap_or(self.s(""));
                    multiline.push(self.concat([last, raw_ws]));
                    continue;
                }
                if index >= 2
                    && matches!(parts[index - 1], Part::Empty)
                    && matches!(parts[index - 2], Part::Hardline)
                {
                    let last = multiline.pop().unwrap_or(self.s(""));
                    multiline.push(self.concat([last, raw_ws]));
                    continue;
                }
            }
            let doc = self.part_doc(part, jsx_whitespace);
            if index % 2 == 0 {
                let last = multiline.pop().unwrap_or(self.s(""));
                multiline.push(self.concat([last, doc]));
            } else {
                multiline.push(doc);
                multiline.push(self.s(""));
            }
            if will_break(doc) {
                forced_break = true;
            }
        }

        let content = if contains_text {
            self.docs.fill(multiline)
        } else {
            self.docs
                .group_with(self.docs.concat_vec(multiline), true, None)
        };

        let multi_line = self.group(self.concat([
            opening,
            self.indent(self.concat([&HARDLINE, content])),
            &HARDLINE,
            closing,
        ]));
        if forced_break {
            return multi_line;
        }
        let flat_children: Vec<Doc<'a>> = parts
            .iter()
            .map(|part| self.part_doc(*part, jsx_whitespace))
            .collect();
        let flat = self.group(self.concat([opening, self.docs.concat_vec(flat_children), closing]));
        self.docs.conditional_group(&[flat, multi_line], false)
    }

    fn part_doc(&self, part: Part<'a>, jsx_whitespace: Doc<'a>) -> Doc<'a> {
        match part {
            Part::Empty => self.s(""),
            Part::Word(doc) | Part::Doc(doc) => doc,
            Part::Line => &LINE,
            Part::Softline => &SOFTLINE,
            Part::Hardline => &HARDLINE,
            Part::JsxWhitespace => jsx_whitespace,
        }
    }

    /// Prettier's `printJsxChildren`: an alternating list of content and
    /// separators.
    fn print_jsx_children(
        &mut self,
        children: &'a [jsx::Child<Loc, Loc>],
        is_fbt: bool,
    ) -> Vec<Part<'a>> {
        let mut parts: Vec<Part<'a>> = vec![Part::Empty];
        let push_word = |p: &Self, parts: &mut Vec<Part<'a>>, doc: Part<'a>| {
            // Append to the current content slot, joining onto whatever is
            // already there.
            match parts.last_mut() {
                Some(last @ Part::Empty) => *last = doc,
                Some(Part::Word(existing)) | Some(Part::Doc(existing)) => {
                    let joined = match doc {
                        Part::Word(new) | Part::Doc(new) => p.docs.pair(existing, new),
                        _ => *existing,
                    };
                    *existing = joined;
                }
                Some(_) | None => parts.push(doc),
            }
        };
        let push_separator = |parts: &mut Vec<Part<'a>>, separator: Part<'a>| {
            if !matches!(separator, Part::Empty) {
                parts.push(separator);
                parts.push(Part::Empty);
            }
        };
        let child_count = children.len();
        for (index, child) in children.iter().enumerate() {
            let next = children.get(index + 1);
            // `{" "}` is a text node holding a space.
            let text = if is_jsx_whitespace_expression(child) {
                Some(" ")
            } else {
                child_text(child)
            };
            if let Some(text) = text {
                if is_meaningful_text(text) {
                    let mut words = split_words(text);
                    let mut current_word: &str = "";
                    if words.first() == Some(&"") {
                        words.remove(0);
                        let whitespace = words.first().copied().unwrap_or("");
                        if whitespace.contains('\n') {
                            let separator = self.separator_with_whitespace(
                                is_fbt,
                                words.get(1).copied().unwrap_or(""),
                                child,
                                next,
                            );
                            push_separator(&mut parts, separator);
                        } else {
                            push_separator(&mut parts, Part::JsxWhitespace);
                        }
                        if !words.is_empty() {
                            words.remove(0);
                        }
                    }
                    let mut end_whitespace: Option<&str> = None;
                    if words.last() == Some(&"") {
                        words.pop();
                        end_whitespace = words.pop();
                    }
                    if words.is_empty() {
                        continue;
                    }
                    for (word_index, word) in words.iter().enumerate() {
                        if word_index % 2 == 1 {
                            push_separator(&mut parts, Part::Line);
                        } else {
                            current_word = word;
                            let doc = self.docs.borrowed(word);
                            push_word(self, &mut parts, Part::Word(doc));
                        }
                    }
                    match end_whitespace {
                        Some(whitespace) if whitespace.contains('\n') => {
                            let separator =
                                self.separator_with_whitespace(is_fbt, current_word, child, next);
                            push_separator(&mut parts, separator);
                        }
                        Some(_) => push_separator(&mut parts, Part::JsxWhitespace),
                        None => {
                            let separator =
                                self.separator_no_whitespace(is_fbt, current_word, child, next);
                            push_separator(&mut parts, separator);
                        }
                    }
                } else if text.contains('\n') {
                    if text.matches('\n').count() > 1 {
                        push_separator(&mut parts, Part::Hardline);
                    }
                } else {
                    push_separator(&mut parts, Part::JsxWhitespace);
                }
            } else {
                let printed = self.print_jsx_child(child);
                push_word(self, &mut parts, Part::Doc(printed));
                match next {
                    Some(next_child) if is_meaningful_child(next_child) => {
                        let raw = child_text(next_child).unwrap_or("");
                        let trimmed = raw.trim_matches(is_jsx_whitespace);
                        let first_word = trimmed.split(is_jsx_whitespace).next().unwrap_or("");
                        let separator =
                            self.separator_no_whitespace(is_fbt, first_word, child, next);
                        push_separator(&mut parts, separator);
                    }
                    _ => push_separator(&mut parts, Part::Hardline),
                }
            }
            let _ = child_count;
        }
        parts
    }

    fn separator_no_whitespace(
        &self,
        is_fbt: bool,
        word: &str,
        child: &jsx::Child<Loc, Loc>,
        next: Option<&jsx::Child<Loc, Loc>>,
    ) -> Part<'a> {
        if is_fbt {
            return Part::Empty;
        }
        if is_self_closing_element(child) || next.is_some_and(is_self_closing_element) {
            return if word.chars().count() == 1 {
                Part::Softline
            } else {
                Part::Hardline
            };
        }
        Part::Softline
    }

    fn separator_with_whitespace(
        &self,
        is_fbt: bool,
        word: &str,
        child: &jsx::Child<Loc, Loc>,
        next: Option<&jsx::Child<Loc, Loc>>,
    ) -> Part<'a> {
        if is_fbt {
            return Part::Hardline;
        }
        if word.chars().count() == 1 {
            return if is_self_closing_element(child) || next.is_some_and(is_self_closing_element) {
                Part::Hardline
            } else {
                Part::Softline
            };
        }
        Part::Hardline
    }

    /// One non-text child.
    fn print_jsx_child(&mut self, child: &'a jsx::Child<Loc, Loc>) -> Doc<'a> {
        let node = NodeRef::JsxChild(child);
        self.print_node(node, |p| match child {
            jsx::Child::Element { inner, .. } => {
                p.print_jsx_element_internal(Some(inner), None, &inner.children.1, node.key())
            }
            jsx::Child::Fragment { inner, .. } => {
                p.print_jsx_element_internal(None, Some(inner), &inner.frag_children.1, node.key())
            }
            jsx::Child::ExpressionContainer { inner, .. } => {
                p.print_jsx_expression_container(inner, node.key(), true)
            }
            jsx::Child::SpreadChild { inner, .. } => {
                let expression = p.print_expression(&inner.expression);
                let inner_doc = p.concat([p.s("..."), expression]);
                let has_comment = p.has_comment(NodeRef::Expression(&inner.expression).key());
                if has_comment {
                    p.concat([
                        p.s("{"),
                        p.indent(p.concat([&SOFTLINE, inner_doc])),
                        &SOFTLINE,
                        p.s("}"),
                    ])
                } else {
                    p.concat([p.s("{"), inner_doc, p.s("}")])
                }
            }
            jsx::Child::Text { inner, .. } => p.docs.borrowed(&inner.raw),
        })
    }

    /// `{expression}`, hugging when the expression is one Prettier lets
    /// sit against the braces.
    fn print_jsx_expression_container(
        &mut self,
        container: &'a jsx::ExpressionContainer<Loc, Loc>,
        key: NodeKey,
        in_children: bool,
    ) -> Doc<'a> {
        match &container.expression {
            jsx::expression_container::Expression::EmptyExpression => {
                let has_line = self.has_line_comment(key, Some(Placement::Dangling));
                let dangling = self.print_dangling_comments(key, Marker::None, has_line);
                self.concat([
                    self.s("{"),
                    dangling.unwrap_or(self.s("")),
                    if has_line { &HARDLINE } else { self.s("") },
                    self.s("}"),
                ])
            }
            jsx::expression_container::Expression::Expression(expression) => {
                let should_inline = self.should_inline_jsx_expression(expression, in_children);
                let printed = self.print_expression(expression);
                if should_inline {
                    self.group(self.concat([
                        self.s("{"),
                        printed,
                        &LINE_SUFFIX_BOUNDARY,
                        self.s("}"),
                    ]))
                } else {
                    self.group(self.concat([
                        self.s("{"),
                        self.indent(self.concat([&SOFTLINE, printed])),
                        &SOFTLINE,
                        &LINE_SUFFIX_BOUNDARY,
                        self.s("}"),
                    ]))
                }
            }
        }
    }

    fn should_inline_jsx_expression(&self, expression: &'a Expression, in_children: bool) -> bool {
        use expression::ExpressionInner as E;
        if self.has_comment(NodeRef::Expression(expression).key()) {
            return false;
        }
        match &**expression {
            E::Array { .. } | E::Object { .. } | E::ArrowFunction { .. } | E::Function { .. } => {
                true
            }
            E::Unary { inner, .. }
                if matches!(inner.operator, expression::UnaryOperator::Await) =>
            {
                self.should_inline_jsx_expression(&inner.argument, in_children)
                    || is_jsx(&inner.argument)
            }
            E::Call { .. } | E::OptionalCall { .. } => true,
            E::TemplateLiteral { .. } | E::TaggedTemplate { .. } => true,
            E::Conditional { .. } => in_children,
            _ => in_children && is_binaryish(expression),
        }
    }

    fn print_jsx_opening(&mut self, opening: &'a jsx::Opening<Loc, Loc>) -> Doc<'a> {
        let node = NodeRef::JsxOpening(opening);
        self.print_node(node, |p| {
            let name = p.print_jsx_name(&opening.name);
            let targs = match &opening.targs {
                Some(targs) => p.print_call_type_args(targs),
                None => p.s(""),
            };
            let name_has_comment = opening
                .targs
                .as_ref()
                .is_some_and(|targs| p.has_comment(NodeRef::CallTypeArgs(targs).key()));
            if opening.self_closing && opening.attributes.is_empty() && !name_has_comment {
                return p.concat([p.s("<"), name, targs, p.s(" />")]);
            }
            // One string attribute without a newline stays on the tag's line.
            if opening.attributes.len() == 1
                && let jsx::OpeningAttribute::Attribute(attribute) = &opening.attributes[0]
                && let Some(jsx::attribute::Value::StringLiteral((_, literal))) = &attribute.value
                && !literal.raw.contains('\n')
                && !name_has_comment
                && !p.has_comment(NodeRef::JsxAttribute(attribute).key())
            {
                let printed = p.print_jsx_attribute(&opening.attributes[0]);
                return p.group(p.concat([
                    p.s("<"),
                    name,
                    targs,
                    p.s(" "),
                    printed,
                    if opening.self_closing { p.s(" />") } else { p.s(">") },
                ]));
            }
            let should_break = opening.attributes.iter().any(|attribute| {
                matches!(attribute, jsx::OpeningAttribute::Attribute(attribute)
                    if matches!(&attribute.value, Some(jsx::attribute::Value::StringLiteral((_, literal))) if literal.raw.contains('\n')))
            });
            let mut attributes: Vec<Doc<'a>> = Vec::with_capacity(opening.attributes.len() * 2);
            for (index, attribute) in opening.attributes.iter().enumerate() {
                if index == 0 {
                    attributes.push(&LINE);
                } else {
                    let previous_end = p.text.span(&attribute_loc(&opening.attributes[index - 1])).end;
                    if p.text.is_next_line_empty(previous_end) {
                        attributes.push(&HARDLINE);
                        attributes.push(&HARDLINE);
                    } else {
                        attributes.push(&LINE);
                    }
                }
                attributes.push(p.print_jsx_attribute(attribute));
            }
            let last_has_trailing_comment = opening
                .attributes
                .last()
                .is_some_and(|attribute| p.has_comment_placed(attribute_node(attribute).key(), Placement::Trailing));
            let bracket_same_line = opening.attributes.is_empty() && !name_has_comment;
            let _ = last_has_trailing_comment;
            let end = if opening.self_closing {
                p.concat([&LINE, p.s("/>")])
            } else if bracket_same_line {
                p.s(">")
            } else {
                p.concat([&SOFTLINE, p.s(">")])
            };
            p.docs.group_with(
                p.concat([p.s("<"), name, targs, p.indent(p.docs.concat_vec(attributes)), end]),
                should_break,
                None,
            )
        })
    }

    fn print_jsx_closing(&mut self, closing: &'a jsx::Closing<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::JsxClosing(closing), |p| {
            let name = p.print_jsx_name(&closing.name);
            p.concat([p.s("</"), name, p.s(">")])
        })
    }

    fn print_jsx_fragment_tag(
        &mut self,
        _fragment: &'a jsx::Fragment<Loc, Loc>,
        key: NodeKey,
        opening: bool,
    ) -> Doc<'a> {
        if !opening {
            return self.s("</>");
        }
        let has_dangling = self.has_comment_placed(key, Placement::Dangling);
        let has_line = self.has_line_comment(key, Some(Placement::Dangling));
        let dangling = self.print_dangling_comments(key, Marker::None, false);
        self.concat([
            self.s("<"),
            self.indent(self.concat([
                if has_line {
                    &HARDLINE
                } else if has_dangling {
                    self.s(" ")
                } else {
                    self.s("")
                },
                dangling.unwrap_or(self.s("")),
            ])),
            if has_line { &HARDLINE } else { self.s("") },
            self.s(">"),
        ])
    }

    fn print_jsx_name(&mut self, name: &'a jsx::Name<Loc, Loc>) -> Doc<'a> {
        match name {
            jsx::Name::Identifier(id) => self.docs.borrowed(&id.name),
            jsx::Name::NamespacedName(ns) => {
                self.text(&format!("{}:{}", ns.namespace.name, ns.name.name))
            }
            jsx::Name::MemberExpression(member) => self.print_jsx_member_name(member),
        }
    }

    fn print_jsx_member_name(&mut self, member: &'a jsx::MemberExpression<Loc, Loc>) -> Doc<'a> {
        let object = match &member.object {
            jsx::member_expression::Object::Identifier(id) => self.docs.borrowed(&id.name),
            jsx::member_expression::Object::MemberExpression(inner) => {
                self.print_jsx_member_name(inner)
            }
        };
        let property = self.docs.borrowed(&member.property.name);
        self.concat([object, self.s("."), property])
    }

    fn print_jsx_attribute(&mut self, attribute: &'a jsx::OpeningAttribute<Loc, Loc>) -> Doc<'a> {
        match attribute {
            jsx::OpeningAttribute::Attribute(attribute) => {
                self.print_node(NodeRef::JsxAttribute(attribute), |p| {
                    let name = match &attribute.name {
                        jsx::attribute::Name::Identifier(id) => p.docs.borrowed(&id.name),
                        jsx::attribute::Name::NamespacedName(ns) => {
                            p.text(&format!("{}:{}", ns.namespace.name, ns.name.name))
                        }
                    };
                    let Some(value) = &attribute.value else {
                        return name;
                    };
                    let value = match value {
                        jsx::attribute::Value::StringLiteral((loc, literal)) => {
                            let node = NodeRef::StringLiteral(loc, literal);
                            let printed = p.print_jsx_attribute_string(&literal.raw);
                            p.print_node(node, |_| printed)
                        }
                        jsx::attribute::Value::ExpressionContainer((loc, container)) => {
                            let node = NodeRef::JsxExpressionContainer(loc, container);
                            p.print_node(node, |p| {
                                p.print_jsx_expression_container(container, node.key(), false)
                            })
                        }
                    };
                    p.concat([name, p.s("="), value])
                })
            }
            jsx::OpeningAttribute::SpreadAttribute(spread) => {
                self.print_node(NodeRef::JsxSpreadAttribute(spread), |p| {
                    let argument = p.print_expression(&spread.argument);
                    let inner = p.concat([p.s("..."), argument]);
                    if p.has_comment(NodeRef::Expression(&spread.argument).key()) {
                        p.concat([
                            p.s("{"),
                            p.indent(p.concat([&SOFTLINE, inner])),
                            &SOFTLINE,
                            p.s("}"),
                        ])
                    } else {
                        p.concat([p.s("{"), inner, p.s("}")])
                    }
                })
            }
        }
    }

    /// A JSX attribute string: double quotes unless the value holds more
    /// double quotes than single, with the quote entities normalised.
    fn print_jsx_attribute_string(&self, raw: &str) -> Doc<'a> {
        if raw.len() < 2 {
            return self.text(raw);
        }
        let content = raw[1..raw.len() - 1]
            .replace("&apos;", "'")
            .replace("&quot;", "\"");
        let quote = choose_quote(&content, '"');
        let escaped = if quote == '"' {
            content.replace('"', "&quot;")
        } else {
            content.replace('\'', "&apos;")
        };
        let text = format!("{quote}{escaped}{quote}");
        let doc = self.text(&text);
        if text.contains('\n')
            && let DocKind::Text(owned) = doc.kind
        {
            return self.replace_end_of_line(owned);
        }
        doc
    }
}

fn attribute_loc(attribute: &jsx::OpeningAttribute<Loc, Loc>) -> Loc {
    match attribute {
        jsx::OpeningAttribute::Attribute(attribute) => attribute.loc.clone(),
        jsx::OpeningAttribute::SpreadAttribute(spread) => spread.loc.clone(),
    }
}

fn attribute_node(attribute: &jsx::OpeningAttribute<Loc, Loc>) -> NodeRef<'_> {
    match attribute {
        jsx::OpeningAttribute::Attribute(attribute) => NodeRef::JsxAttribute(attribute),
        jsx::OpeningAttribute::SpreadAttribute(spread) => NodeRef::JsxSpreadAttribute(spread),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_split_with_captured_whitespace() {
        assert_eq!(split_words("a b"), vec!["a", " ", "b"]);
        assert_eq!(split_words(" a"), vec!["", " ", "a"]);
        assert_eq!(split_words("a "), vec!["a", " ", ""]);
        assert_eq!(
            split_words("\n  hello world\n"),
            vec!["", "\n  ", "hello", " ", "world", "\n", ""]
        );
    }
}
