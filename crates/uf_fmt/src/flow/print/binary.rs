//! Binary and logical expressions: Prettier's `printBinaryishExpression`.
//!
//! Operators of the same precedence are flattened into one list so they
//! break together — `a && b && c` is three lines or one, never two — and
//! the list is indented under its first operand except where the parent
//! already indents it (a `return`, an arrow body, a variable initializer).

use uf_flow::ast::{expression, statement};

use super::Printer;
use super::parens::{
    binaryish_operator, is_binaryish, is_call_like, is_jsx, is_member, same, should_flatten,
};
use crate::doc::{Doc, DocKind, LINE};
use crate::flow::comments::Placement;
use crate::flow::node::{Expression, NodeRef};

/// Whether a logical expression's right side is an object, array or JSX
/// that should hug the operator rather than break after it.
pub fn should_inline_logical_expression(expression: &Expression) -> bool {
    let expression::ExpressionInner::Logical { inner, .. } = &**expression else {
        return false;
    };
    match &*inner.right {
        expression::ExpressionInner::Object { inner: object, .. } => !object.properties.is_empty(),
        expression::ExpressionInner::Array { inner: array, .. } => !array.elements.is_empty(),
        _ => is_jsx(&inner.right),
    }
}

/// The two operands of a binary or logical expression.
fn operands(expression: &Expression) -> Option<(&Expression, &Expression)> {
    match &**expression {
        expression::ExpressionInner::Binary { inner, .. } => Some((&inner.left, &inner.right)),
        expression::ExpressionInner::Logical { inner, .. } => Some((&inner.left, &inner.right)),
        _ => None,
    }
}

/// Whether two expressions are both binary or both logical.
fn same_kind(a: &Expression, b: &Expression) -> bool {
    matches!(
        (&**a, &**b),
        (
            expression::ExpressionInner::Binary { .. },
            expression::ExpressionInner::Binary { .. }
        ) | (
            expression::ExpressionInner::Logical { .. },
            expression::ExpressionInner::Logical { .. }
        )
    )
}

impl<'a> Printer<'a> {
    /// Print a binary or logical expression, which is on top of the
    /// ancestor stack.
    pub fn print_binaryish(&mut self, expression: &'a Expression) -> Doc<'a> {
        let parent = self.parent();
        let is_inside_parenthesis = match parent {
            Some(NodeRef::Statement(statement)) => match &**statement {
                statement::StatementInner::If { inner, .. } => same(&inner.test, expression),
                statement::StatementInner::While { inner, .. } => same(&inner.test, expression),
                statement::StatementInner::DoWhile { inner, .. } => same(&inner.test, expression),
                statement::StatementInner::Switch { inner, .. } => {
                    same(&inner.discriminant, expression)
                }
                _ => false,
            },
            _ => false,
        };
        let parts = self.binaryish_parts(expression, false, is_inside_parenthesis);

        if is_inside_parenthesis {
            return self.docs.concat_vec(parts);
        }

        // Break between the parens in unaries or in a member or specific
        // call expression: `(\n  a &&\n  b\n).call()`.
        if let Some(NodeRef::Expression(parent_expression)) = parent {
            let wraps = match &**parent_expression {
                expression::ExpressionInner::Call { inner, .. } => same(&inner.callee, expression),
                expression::ExpressionInner::New { inner, .. } => same(&inner.callee, expression),
                expression::ExpressionInner::Unary { .. } => true,
                _ => is_member(parent_expression),
            };
            if wraps {
                let inner = self.docs.concat_vec(parts);
                return self.group(self.concat([
                    self.indent(self.concat([&crate::doc::SOFTLINE, inner])),
                    &crate::doc::SOFTLINE,
                ]));
            }
        }

        let grandparent = self.grandparent();
        let should_not_indent = match parent {
            Some(NodeRef::Statement(statement)) => match &**statement {
                statement::StatementInner::Return { .. }
                | statement::StatementInner::Throw { .. } => true,
                // A `for` body is a statement, so an expression whose
                // parent is a `for` is always in its head.
                statement::StatementInner::For { .. } => true,
                _ => false,
            },
            Some(NodeRef::JsxExpressionContainer(..)) => {
                matches!(grandparent, Some(NodeRef::JsxAttribute(_)))
            }
            Some(NodeRef::Expression(parent_expression)) => match &**parent_expression {
                expression::ExpressionInner::ArrowFunction { inner, .. } => matches!(
                    &inner.body,
                    uf_flow::ast::function::Body::BodyExpression(body) if same(body, expression)
                ),
                expression::ExpressionInner::Conditional { .. } => match grandparent {
                    Some(NodeRef::Statement(statement)) => !matches!(
                        &**statement,
                        statement::StatementInner::Return { .. }
                            | statement::StatementInner::Throw { .. }
                    ),
                    Some(NodeRef::Expression(grand)) => {
                        !is_call_like(grand)
                            && !matches!(**grand, expression::ExpressionInner::MetaProperty { .. })
                    }
                    _ => true,
                },
                expression::ExpressionInner::TemplateLiteral { .. } => true,
                _ => false,
            },
            _ => false,
        };

        let should_indent_if_inlining = match parent {
            Some(NodeRef::Expression(parent_expression)) => {
                matches!(
                    **parent_expression,
                    expression::ExpressionInner::Assignment { .. }
                )
            }
            Some(NodeRef::Declarator(_)) | Some(NodeRef::ObjectProperty(_)) => true,
            Some(NodeRef::ClassMember(member)) => matches!(
                member,
                uf_flow::ast::class::BodyElement::Property(_)
                    | uf_flow::ast::class::BodyElement::PrivateField(_)
            ),
            _ => false,
        };

        let (left, right) = operands(expression).expect("binaryish");
        let operator = binaryish_operator(expression).unwrap_or("");
        let same_precedence_sub_expression =
            is_binaryish(left) && should_flatten(operator, binaryish_operator(left).unwrap_or(""));

        if should_not_indent
            || (should_inline_logical_expression(expression) && !same_precedence_sub_expression)
            || (!should_inline_logical_expression(expression) && should_indent_if_inlining)
        {
            return self.group(self.docs.concat_vec(parts));
        }

        if parts.is_empty() {
            return self.s("");
        }

        // If the right part is a JSX node, we include it in a separate group
        // to make sure it gets the proper treatment.
        let has_jsx = is_jsx(right);
        let first_group_index = parts
            .iter()
            .position(|part| matches!(part.kind, DocKind::Group { .. }));
        let head_len = first_group_index.map_or(1, |index| index + 1);
        let (head, rest) = parts.split_at(head_len.min(parts.len()));
        let rest_end = if has_jsx {
            rest.len().saturating_sub(1)
        } else {
            rest.len()
        };
        let rest_doc = self.docs.concat(rest[..rest_end].iter().copied());
        let group_id = self.docs.group_id();
        let mut chain_parts: Vec<Doc<'a>> = head.to_vec();
        chain_parts.push(self.indent(rest_doc));
        let chain = self
            .docs
            .group_with(self.docs.concat_vec(chain_parts), false, Some(group_id));
        if !has_jsx {
            return chain;
        }
        let jsx_part = parts[parts.len() - 1];
        self.group(self.concat([chain, self.docs.indent_if_break(jsx_part, group_id, false)]))
    }

    /// The flattened parts of a binaryish expression: operands and
    /// operators of the same precedence in one list. `expression` is on top
    /// of the ancestor stack.
    fn binaryish_parts(
        &mut self,
        expression: &'a Expression,
        is_nested: bool,
        is_inside_parenthesis: bool,
    ) -> Vec<Doc<'a>> {
        let Some((left, right)) = operands(expression) else {
            let printed = self.print_expression(expression);
            return vec![self.group(printed)];
        };
        let operator = binaryish_operator(expression).unwrap_or("");
        let mut parts: Vec<Doc<'a>> = Vec::new();

        // Put all operators with the same precedence level in the same
        // group. The reason we only need to do this with the `left`
        // expression is because given an expression like `1 + 2 - 3`, it is
        // always parsed like `((1 + 2) - 3)`, meaning the `left` side is
        // where the rest of the expression will exist.
        let flatten_left =
            is_binaryish(left) && should_flatten(operator, binaryish_operator(left).unwrap_or(""));
        if flatten_left {
            let node = NodeRef::Expression(left);
            self.ancestors.push(node);
            parts = self.binaryish_parts(left, true, is_inside_parenthesis);
            self.ancestors.pop();
        } else {
            let printed = self.print_expression(left);
            parts.push(self.group(printed));
        }

        let should_inline = should_inline_logical_expression(expression);
        let printed_right = self.print_expression(right);
        let right_doc = if should_inline {
            self.concat([self.s(operator), self.s(" "), printed_right])
        } else {
            self.concat([self.s(operator), &LINE, printed_right])
        };

        // If there's only a single binary expression, we want to create a
        // group in order to avoid having a small right part like -1 be on
        // its own line.
        let parent_same_kind = match self.parent() {
            Some(NodeRef::Expression(parent)) => same_kind(parent, expression),
            _ => false,
        };
        let should_break = self.has_comment_where(
            NodeRef::Expression(left).key(),
            Some(Placement::Trailing),
            |comment| matches!(comment.kind, uf_flow::ast::CommentKind::Line),
        );
        let is_logical = matches!(**expression, expression::ExpressionInner::Logical { .. });
        let should_group = should_break
            || (!(is_inside_parenthesis && is_logical)
                && !parent_same_kind
                && !same_kind(left, expression)
                && !same_kind(right, expression));

        parts.push(self.s(" "));
        parts.push(if should_group {
            self.docs.group_with(right_doc, should_break, None)
        } else {
            right_doc
        });

        // The root comments are already printed, but we need to manually
        // print the other ones since we don't call the normal print on
        // nested binary expressions.
        if is_nested && self.has_comment(NodeRef::Expression(expression).key()) {
            let inner = self.docs.concat_vec(parts);
            let with_comments = self.print_comments(NodeRef::Expression(expression).key(), inner);
            return vec![with_comments];
        }
        parts
    }
}
