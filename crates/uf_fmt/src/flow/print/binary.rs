//! Binary and logical expressions: Prettier's `printBinaryishExpression`.
//!
//! Operators of the same precedence are flattened into one list so they
//! break together — `a && b && c` is three lines or one, never two — and
//! the list is indented under its first operand except where the parent
//! already indents it (a `return`, an arrow body, a variable initializer).

use uf_flow::ast::{expression, statement};

use super::Printer;
use super::parens::{
    binaryish_operator, is_binaryish, is_call, is_jsx, is_member, logical_in_logical_needs_parens,
    same, should_flatten,
};
use crate::doc::{Doc, DocKind, LINE};
use crate::flow::comments::Placement;
use crate::flow::node::{Expression, NodeRef};

/// Whether a logical expression's right side is an object, array or JSX
/// that should hug the operator rather than break after it.
///
/// The right side here is the *last operand of the chain*, which is not
/// `inner.right` when the source parenthesized to the right: `a && (b && {})`
/// prints as `a && b && {}`, and what hugs the operator is the object. See
/// {@link flattens_into_parent}.
pub fn should_inline_logical_expression(expression: &Expression) -> bool {
    let expression::ExpressionInner::Logical { inner, .. } = &**expression else {
        return false;
    };
    let mut right = &inner.right;
    while flattens_into_parent(expression, right) {
        let expression::ExpressionInner::Logical { inner, .. } = &**right else {
            break;
        };
        right = &inner.right;
    }
    match &**right {
        expression::ExpressionInner::Object { inner: object, .. } => !object.properties.is_empty(),
        expression::ExpressionInner::Array { inner: array, .. } => !array.elements.is_empty(),
        _ => is_jsx(right),
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

/// Whether `right` joins its parent's chain instead of printing as a group
/// of its own.
///
/// Prettier flattens the left spine only, and says why: `1 + 2 - 3` parses
/// as `((1 + 2) - 3)`, so the left is where the rest of the expression is,
/// and anything on the right had a different precedence and deserves its own
/// group.
///
/// That reasoning holds for every tree built from source without
/// parentheses, and `a && (b && c)` is the case it does not cover. The
/// parentheses are redundant — see
/// [`logical_in_logical_needs_parens`](super::parens::logical_in_logical_needs_parens)
/// — so they are not printed, and what comes out is `a && b && c`. Reading
/// that back gives a *left*-nested tree, which lays out as one chain. Unless
/// the right-nested one lays out the same way, `uf fmt` twice is not
/// `uf fmt` once:
///
/// ```text
/// - descriptor.get === undefined && descriptor.set === undefined;
/// + descriptor.get === undefined &&
/// +   descriptor.set === undefined;
/// ```
///
/// The condition is the paren rule, not a second copy of it: an operand
/// that keeps its parentheses — `a + (b + c)`, `a && (b || c)` — is a group,
/// exactly as before. See ubugeeei-prod/uf#133.
fn flattens_into_parent(expression: &Expression, right: &Expression) -> bool {
    let expression::ExpressionInner::Logical { inner: parent, .. } = &**expression else {
        return false;
    };
    let expression::ExpressionInner::Logical { inner: child, .. } = &**right else {
        return false;
    };
    !logical_in_logical_needs_parens(&parent.operator, &child.operator)
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
                // A ternary's test, when what encloses the ternary is not
                // an argument list. `new Error(…)` and `import(…)` are not
                // argument lists for this purpose — only `f(…)` and
                // `f?.(…)` are, which `is_call` is and `is_call_like` is
                // not. react-native's `Touchable.js` is where the
                // difference shows: the `===` in
                // `new Error("…" + signal + … === "number" ? a : b)` keeps
                // its right operand at the ternary's own indentation, and
                // reading `new` as a call put `"number"` two columns in,
                // under the `+` chain it is not part of.
                expression::ExpressionInner::Conditional { .. } => match grandparent {
                    Some(NodeRef::Statement(statement)) => !matches!(
                        &**statement,
                        statement::StatementInner::Return { .. }
                            | statement::StatementInner::Throw { .. }
                    ),
                    Some(NodeRef::Expression(grand)) => {
                        !is_call(grand)
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
        // "Is there another link of the same precedence in this chain?" —
        // Prettier asks it of the left because a chain written without
        // parentheses is left-nested. A flattened right is the same chain
        // read from the other end, and answers the same question.
        let same_precedence_sub_expression = (is_binaryish(left)
            && should_flatten(operator, binaryish_operator(left).unwrap_or("")))
            || flattens_into_parent(expression, right);

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
        // A right operand that prints without parentheses continues this
        // chain rather than starting a group. Its parts are spliced in, so
        // the operands break together or not at all — and the hug of a
        // trailing object is decided by the innermost link, which is where
        // that object actually sits.
        let flattened_right = flattens_into_parent(expression, right).then(|| {
            let node = NodeRef::Expression(right);
            self.ancestors.push(node);
            let parts = self.binaryish_parts(right, true, is_inside_parenthesis);
            self.ancestors.pop();
            parts
        });
        let right_doc = match &flattened_right {
            Some(parts) => {
                let tail = self.docs.concat(parts.iter().copied());
                self.concat([self.s(operator), &LINE, tail])
            }
            None => {
                let printed_right = self.print_expression(right);
                if should_inline {
                    self.concat([self.s(operator), self.s(" "), printed_right])
                } else {
                    self.concat([self.s(operator), &LINE, printed_right])
                }
            }
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
        //
        // Spliced into the parts rather than wrapped around them, because
        // the caller separates the leftmost operand from the rest and
        // indents the rest, and it finds the leftmost operand by looking for
        // the first part that is a group. Wrapping puts a concat in front of
        // it, so the search runs on to the *next* group — the last operand's
        // — and every part up to that lands in the head, unindented:
        //
        // ```text
        // uf:        2 + // [rendererID, rootFiberID]
        //            1 + // [stringTableLength]
        //            pendingStringTableLength +
        //              (numUnmountSuspenseIDs > 0 ? … : 0) +
        // prettier:  2 + // [rendererID, rootFiberID]
        //              1 + // [stringTableLength]
        //              pendingStringTableLength +
        //              (numUnmountSuspenseIDs > 0 ? … : 0) +
        // ```
        //
        // from react-devtools' `renderer.js`. A leading comment goes in
        // front of the operand it leads, which is where the head wants it;
        // a trailing one is a line suffix and prints at the end of the line
        // it is already on, so neither moves.
        let key = NodeRef::Expression(expression).key();
        if is_nested && self.has_comment(key) {
            let leading = self.print_leading_comments(key);
            let trailing = self.print_trailing_comments(key);
            let mut spliced = Vec::with_capacity(parts.len() + 2);
            spliced.extend(leading);
            spliced.append(&mut parts);
            spliced.extend(trailing);
            return spliced;
        }
        parts
    }
}
