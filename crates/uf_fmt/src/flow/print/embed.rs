//! Embedded languages: which templates hold one, and how the formatted
//! result is spliced back into the JavaScript.
//!
//! This is Prettier's `embed` for the estree printer, restricted to the one
//! language uf implements. Prettier reads `graphql`, `css`, `html`, `sql`
//! and `markdown` templates; uf reads GraphQL, because it is the one this
//! toolchain has a reason to know (`@uniflowed/graphql`, `@uniflowed/relay`)
//! and because "a formatter that mangles a template" is a much worse
//! outcome than "a formatter that leaves it alone". Every other template
//! goes out byte for byte, as before.
//!
//! Everything here is a rule established by running Prettier, not by
//! reading its documentation. The three that are easy to guess wrong:
//!
//! * **The tags.** `graphql`, `gql`, `graphql.experimental`, a template
//!   passed to a call of `graphql(…)`, and a template preceded by a
//!   `/* GraphQL */` block comment. Not `Relay.QL`, not `gql.experimental`,
//!   not `GraphQL` — the match is exact and case sensitive.
//! * **`${…}`.** Prettier does not treat the template as one document with
//!   holes in it. It parses *each quasi separately* as a whole GraphQL
//!   document, so `gql`…`${Fragment}`` works — the text before the hole is a
//!   complete document and the text after it is only whitespace — and
//!   `gql`… ...${name} …`` does not, because `… ...` is not a document. When
//!   any quasi fails, the whole template is left alone.
//! * **Indentation.** The contents are indented one level from the line the
//!   template starts on, and the closing backtick returns to it, whatever
//!   the author wrote. That is the visible half of the feature: GraphQL that
//!   keeps its old indentation after the code around it moves is GraphQL at
//!   the wrong indentation.

use uf_flow::Loc;
use uf_flow::ast::{CommentKind, expression};

use super::Printer;
use crate::doc::{Doc, EMPTY, HARDLINE, Label};
use crate::flow::comments::Placement;
use crate::flow::node::{Expression, NodeRef};

/// The block comment that marks a template as GraphQL, spelled exactly.
const LANGUAGE_COMMENT: &str = " GraphQL ";

impl<'a> Printer<'a> {
    /// Whether a tagged template's tag names GraphQL.
    pub(super) fn tag_is_graphql(tag: &Expression) -> bool {
        use expression::ExpressionInner as E;
        match &**tag {
            E::Identifier { inner, .. } => inner.name == "graphql" || inner.name == "gql",
            E::Member { inner, .. } => {
                let object_is_graphql = matches!(
                    &*inner.object,
                    E::Identifier { inner, .. } if inner.name == "graphql"
                );
                let property_is_experimental = matches!(
                    &inner.property,
                    expression::member::Property::PropertyIdentifier(name)
                        if name.name == "experimental"
                );
                object_is_graphql && property_is_experimental
            }
            _ => false,
        }
    }

    /// Whether an untagged template literal is GraphQL because of where it
    /// sits: an argument of `graphql(…)`, or behind a `/* GraphQL */`.
    ///
    /// Called while the template literal is the current node, so its parent
    /// is the enclosing expression.
    pub(super) fn template_is_graphql(&self, template: &'a Expression) -> bool {
        if self.has_language_comment(NodeRef::Expression(template).key()) {
            return true;
        }
        // Prettier also reads the comment off an `x as const` or an
        // expression statement wrapping the template, because that is where
        // its own comment attachment puts it in those two shapes.
        let Some(parent) = self.parent() else {
            return false;
        };
        match parent {
            NodeRef::Expression(parent) => {
                use expression::ExpressionInner as E;
                match &**parent {
                    E::Call { inner, .. } => matches!(
                        &*inner.callee,
                        E::Identifier { inner, .. } if inner.name == "graphql"
                    ),
                    E::AsConstExpression { .. } => {
                        self.has_language_comment(NodeRef::Expression(parent).key())
                    }
                    _ => false,
                }
            }
            NodeRef::Statement(statement) => {
                use uf_flow::ast::statement::StatementInner as S;
                matches!(&**statement, S::Expression { .. })
                    && self.has_language_comment(NodeRef::Statement(statement).key())
            }
            _ => false,
        }
    }

    fn has_language_comment(&self, key: crate::flow::node::NodeKey) -> bool {
        if !self.comments.has(key, Placement::Leading) {
            return false;
        }
        let Some(slots) = self.comments.slots(key) else {
            return false;
        };
        slots.leading.iter().any(|index| {
            let comment = self.comments.get(*index);
            comment.kind == CommentKind::Block && comment.text == LANGUAGE_COMMENT
        })
    }

    /// Print `template` with its contents formatted as GraphQL, or [`None`]
    /// to leave the template exactly as it was written.
    ///
    /// Prettier's `printEmbedGraphQL`. Every quasi is a document of its own;
    /// a quasi that is nothing but blank lines and `#` comments takes a
    /// short path that never reaches the GraphQL parser, which is why a
    /// template whose only content is a comment still formats while a
    /// document with a comment *in* it does not.
    pub(super) fn print_graphql_template(
        &mut self,
        template: &'a expression::TemplateLiteral<Loc, Loc>,
    ) -> Option<Doc<'a>> {
        let quasis = &template.quasis;
        let count = quasis.len();
        // A recognised template with nothing in it collapses to an empty
        // one, whatever whitespace the author left between the backticks.
        // Prettier decides this before it reaches the GraphQL parser, and
        // the result carries no embed label.
        if count == 1 && quasis[0].value.raw.trim().is_empty() {
            return Some(self.s("``"));
        }
        let mut parts: Vec<Doc<'a>> = Vec::new();
        let mut indent_size = 0;

        for (index, quasi) in quasis.iter().enumerate() {
            let is_first = index == 0;
            let is_last = index + 1 == count;
            let text: &'a str = &quasi.value.cooked;
            let lines: Vec<&str> = text.split('\n').collect();
            let last_line = lines.len() - 1;

            // A hole on a line that already holds a `#` would be commented
            // out by it. Prettier declines the whole template rather than
            // decide where the comment ends.
            if !is_last && lines[last_line].contains('#') {
                return None;
            }

            let starts_with_blank_line =
                lines.len() > 2 && lines[0].trim().is_empty() && lines[1].trim().is_empty();
            let ends_with_blank_line = lines.len() > 2
                && lines[last_line].trim().is_empty()
                && lines[last_line - 1].trim().is_empty();

            let printed = if lines.iter().all(|line| is_blank_or_comment(line)) {
                self.print_graphql_comment_lines(&lines)
            } else {
                // The annotation pins the doc to the arena's lifetime.
                // Without it the borrow of `self.docs` is inferred as the
                // shorter `&mut self` region, and the doc it returns then
                // cannot outlive this loop iteration.
                let formatted: Result<Doc<'a>, crate::graphql::Declined> =
                    crate::graphql::format(&self.docs, text);
                match formatted {
                    Ok(doc) => Some(doc),
                    Err(_) => return None,
                }
            };

            match printed {
                Some(doc) => {
                    if !is_first && starts_with_blank_line {
                        parts.push(&EMPTY);
                    }
                    parts.push(doc);
                    if !is_last && ends_with_blank_line {
                        parts.push(&EMPTY);
                    }
                }
                None => {
                    if !is_first && !is_last && starts_with_blank_line {
                        parts.push(&EMPTY);
                    }
                }
            }

            if let Some(expression) = template.expressions.get(index) {
                if text.contains('\n') {
                    indent_size = super::literal::indent_size_of(text, self.options.indent_width);
                }
                parts.push(self.print_template_expression(
                    expression,
                    quasi,
                    quasis.get(index + 1),
                    indent_size,
                ));
            }
        }

        let body = self.docs.join(&HARDLINE, parts);
        let printed = self.concat([
            self.s("`"),
            self.indent(self.concat([&HARDLINE, body])),
            &HARDLINE,
            self.s("`"),
        ]);
        Some(self.docs.label(Label::Embed, printed))
    }

    /// Prettier's `printGraphqlComments`: the non-blank lines, trimmed, with
    /// one blank line kept between groups of them.
    ///
    /// [`None`] when there is nothing left, which is the ordinary case — a
    /// quasi that is only the newline and indentation around a `${…}`.
    fn print_graphql_comment_lines(&self, lines: &[&str]) -> Option<Doc<'a>> {
        let trimmed: Vec<&str> = lines.iter().map(|line| line.trim()).collect();
        let mut parts: Vec<Doc<'a>> = Vec::new();
        let mut seen = false;
        for (index, line) in trimmed.iter().enumerate() {
            if line.is_empty() {
                continue;
            }
            // A `#` comment can hold a backtick or a `${` as easily as a
            // string value can, and writing one back unescaped would end
            // the template early.
            let line = self.text(&crate::graphql::escape_template_characters(line));
            if seen && index > 0 && trimmed[index - 1].is_empty() {
                parts.push(self.concat([&HARDLINE, line]));
            } else {
                parts.push(line);
            }
            seen = true;
        }
        if parts.is_empty() {
            None
        } else {
            Some(self.docs.join(&HARDLINE, parts))
        }
    }
}

/// Prettier's `/^\s*(?:#[^\n\r]*)?$/`: a line that is blank, or blank up to
/// a `#` comment.
fn is_blank_or_comment(line: &str) -> bool {
    let rest = line.trim_start();
    rest.is_empty() || rest.starts_with('#')
}
