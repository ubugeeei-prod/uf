//! Conditional expressions and conditional types: Prettier's
//! `printTernary`.
//!
//! A chain of ternaries is one group, so `a ? b : c ? d : e` breaks into a
//! ladder or not at all. When JSX is involved the branches are wrapped in
//! parentheses that appear only when the chain breaks — "JSX mode".

use uf_flow::Loc;
use uf_flow::ast::{CommentKind, expression, statement, types};

use super::Printer;
use super::parens::{is_binary_cast, is_jsx, is_member, same};
use crate::doc::{BREAK_PARENT, Doc, LINE, SOFTLINE};
use crate::flow::node::{Expression, NodeRef, Type};

/// Which kind of conditional is being printed.
enum Ternary<'a> {
    Expression(&'a expression::Conditional<Loc, Loc>),
    Type(&'a types::Conditional<Loc, Loc>),
}

fn conditional_of(expression: &Expression) -> Option<&expression::Conditional<Loc, Loc>> {
    match &**expression {
        expression::ExpressionInner::Conditional { inner, .. } => Some(inner),
        _ => None,
    }
}

fn conditional_type_of(ty: &Type) -> Option<&types::Conditional<Loc, Loc>> {
    match &**ty {
        types::TypeInner::Conditional { inner, .. } => Some(inner),
        _ => None,
    }
}

/// Whether any branch of a conditional chain rooted at `expression` is
/// JSX.
fn chain_contains_jsx(expression: &Expression) -> bool {
    let mut stack = vec![expression];
    while let Some(node) = stack.pop() {
        let Some(conditional) = conditional_of(node) else {
            continue;
        };
        for child in [
            &conditional.test,
            &conditional.consequent,
            &conditional.alternate,
        ] {
            if is_jsx(child) {
                return true;
            }
            if conditional_of(child).is_some() {
                stack.push(child);
            }
        }
    }
    false
}

fn is_nil(expression: &Expression) -> bool {
    match &**expression {
        expression::ExpressionInner::NullLiteral { .. } => true,
        expression::ExpressionInner::Identifier { inner, .. } => &*inner.name == "undefined",
        _ => false,
    }
}

impl<'a> Printer<'a> {
    /// Print `test ? consequent : alternate`.
    pub fn print_conditional(
        &mut self,
        conditional: &'a expression::Conditional<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        self.print_ternary(
            Ternary::Expression(conditional),
            NodeRef::Expression(expression),
        )
    }

    /// Print `A extends B ? C : D`.
    pub fn print_conditional_type(
        &mut self,
        conditional: &'a types::Conditional<Loc, Loc>,
        ty: &'a Type,
    ) -> Doc<'a> {
        self.print_ternary(Ternary::Type(conditional), NodeRef::Type(ty))
    }

    fn print_ternary(&mut self, ternary: Ternary<'a>, node: NodeRef<'a>) -> Doc<'a> {
        let is_expression = matches!(ternary, Ternary::Expression(_));
        let parent = self.parent();

        // Is the parent a ternary of the same kind, and if so, is this node
        // its test or one of its branches?
        let (parent_is_same, is_parent_test, parent_alternate_is_node) = match (&ternary, parent) {
            (Ternary::Expression(_), Some(NodeRef::Expression(parent_expression))) => {
                match conditional_of(parent_expression) {
                    Some(parent_conditional) => {
                        let me = match node {
                            NodeRef::Expression(e) => e,
                            _ => unreachable!(),
                        };
                        (
                            true,
                            same(&parent_conditional.test, me),
                            same(&parent_conditional.alternate, me),
                        )
                    }
                    None => (false, false, false),
                }
            }
            (Ternary::Type(_), Some(NodeRef::Type(parent_type))) => {
                match conditional_type_of(parent_type) {
                    Some(parent_conditional) => {
                        let me = match node {
                            NodeRef::Type(t) => t,
                            _ => unreachable!(),
                        };
                        let is_test = std::ptr::eq(&*parent_conditional.check_type.0, &*me.0)
                            || std::ptr::eq(&*parent_conditional.extends_type.0, &*me.0);
                        (
                            true,
                            is_test,
                            std::ptr::eq(&*parent_conditional.false_type.0, &*me.0),
                        )
                    }
                    None => (false, false, false),
                }
            }
            _ => (false, false, false),
        };
        let mut force_no_indent = parent_is_same && !is_parent_test;

        // Find the outermost non-conditional parent and the outermost
        // conditional of the chain.
        let mut chain_root = node;
        let mut first_non_conditional_parent: Option<NodeRef<'a>> = None;
        for ancestor in self.ancestors.iter().rev().skip(1) {
            let continues = match (chain_root, ancestor) {
                (NodeRef::Expression(child), NodeRef::Expression(candidate)) => {
                    match conditional_of(candidate) {
                        Some(conditional) => !same(&conditional.test, child),
                        None => false,
                    }
                }
                (NodeRef::Type(child), NodeRef::Type(candidate)) => {
                    match conditional_type_of(candidate) {
                        Some(conditional) => {
                            !std::ptr::eq(&*conditional.check_type.0, &*child.0)
                                && !std::ptr::eq(&*conditional.extends_type.0, &*child.0)
                        }
                        None => false,
                    }
                }
                _ => false,
            };
            if continues {
                chain_root = *ancestor;
            } else {
                first_non_conditional_parent = Some(*ancestor);
                break;
            }
        }
        let first_non_conditional_parent = first_non_conditional_parent.or(parent);

        let mut jsx_mode = false;
        let mut parts: Vec<Doc<'a>> = Vec::new();
        let (consequent_doc, alternate_doc, test_doc);
        let should_break;
        match ternary {
            Ternary::Expression(conditional) => {
                let root_expression = match chain_root {
                    NodeRef::Expression(e) => e,
                    _ => unreachable!(),
                };
                if is_jsx(&conditional.test)
                    || is_jsx(&conditional.consequent)
                    || is_jsx(&conditional.alternate)
                    || chain_contains_jsx(root_expression)
                {
                    jsx_mode = true;
                    force_no_indent = true;
                    let consequent = self.print_expression(&conditional.consequent);
                    let alternate = self.print_expression(&conditional.alternate);
                    let consequent = if is_nil(&conditional.consequent) {
                        consequent
                    } else {
                        self.wrap_jsx_branch(consequent)
                    };
                    let alternate = if conditional_of(&conditional.alternate).is_some()
                        || is_nil(&conditional.alternate)
                    {
                        alternate
                    } else {
                        self.wrap_jsx_branch(alternate)
                    };
                    parts.push(self.s(" ? "));
                    parts.push(consequent);
                    parts.push(self.s(" : "));
                    parts.push(alternate);
                } else {
                    let consequent = self.print_expression(&conditional.consequent);
                    let alternate = self.print_expression(&conditional.alternate);
                    let consequent_is_conditional =
                        conditional_of(&conditional.consequent).is_some();
                    let part = self.concat([
                        &LINE,
                        self.s("? "),
                        if consequent_is_conditional {
                            self.if_break(self.s(""), self.s("("))
                        } else {
                            self.s("")
                        },
                        self.docs.align(2, consequent),
                        if consequent_is_conditional {
                            self.if_break(self.s(""), self.s(")"))
                        } else {
                            self.s("")
                        },
                        &LINE,
                        self.s(": "),
                        self.docs.align(2, alternate),
                    ]);
                    parts.push(
                        if !parent_is_same || parent_alternate_is_node || is_parent_test {
                            part
                        } else {
                            self.docs.align(
                                u16::try_from(self.options.indent_width.saturating_sub(2))
                                    .unwrap_or(0),
                                part,
                            )
                        },
                    );
                }
                consequent_doc = ();
                alternate_doc = ();
                should_break = [
                    &conditional.consequent,
                    &conditional.alternate,
                    &conditional.test,
                ]
                .into_iter()
                .any(|child| self.has_multiline_block_comment(NodeRef::Expression(child).key()));
                let printed_test = self.print_expression(&conditional.test);
                test_doc = if parent_is_same && parent_alternate_is_node {
                    self.docs.align(2, printed_test)
                } else {
                    printed_test
                };
            }
            Ternary::Type(conditional) => {
                let true_type = self.print_type(&conditional.true_type);
                let false_type = self.print_type(&conditional.false_type);
                let true_is_conditional = conditional_type_of(&conditional.true_type).is_some();
                let part = self.concat([
                    &LINE,
                    self.s("? "),
                    if true_is_conditional {
                        self.if_break(self.s(""), self.s("("))
                    } else {
                        self.s("")
                    },
                    self.docs.align(2, true_type),
                    if true_is_conditional {
                        self.if_break(self.s(""), self.s(")"))
                    } else {
                        self.s("")
                    },
                    &LINE,
                    self.s(": "),
                    self.docs.align(2, false_type),
                ]);
                parts.push(
                    if !parent_is_same || parent_alternate_is_node || is_parent_test {
                        part
                    } else {
                        self.docs.align(
                            u16::try_from(self.options.indent_width.saturating_sub(2)).unwrap_or(0),
                            part,
                        )
                    },
                );
                consequent_doc = ();
                alternate_doc = ();
                should_break = [
                    &conditional.true_type,
                    &conditional.false_type,
                    &conditional.check_type,
                    &conditional.extends_type,
                ]
                .into_iter()
                .any(|child| self.has_multiline_block_comment(NodeRef::Type(child).key()));
                let check = self.print_type(&conditional.check_type);
                let extends = self.print_type(&conditional.extends_type);
                let printed_test = self.concat([check, self.s(" extends "), extends]);
                test_doc = if parent_is_same && parent_alternate_is_node {
                    self.docs.align(2, printed_test)
                } else {
                    printed_test
                };
            }
        }
        let _ = (consequent_doc, alternate_doc);

        // Break the closing paren to keep the chain right after it:
        // `(a ? b : c).call()`.
        let break_closing_paren = !jsx_mode
            && matches!(parent, Some(NodeRef::Expression(parent_expression))
                if is_member(parent_expression)
                    && !matches!(&**parent_expression,
                        expression::ExpressionInner::Member { inner, .. }
                            if matches!(inner.property, expression::member::Property::PropertyExpression(_)))
                    && !matches!(&**parent_expression,
                        expression::ExpressionInner::OptionalMember { inner, .. }
                            if matches!(inner.member.property, expression::member::Property::PropertyExpression(_))));
        let should_extra_indent = is_expression && self.should_extra_indent_for_conditional(node);

        let body = self.docs.concat_vec(parts);
        let body = if force_no_indent {
            body
        } else {
            self.indent(body)
        };
        let closing = if is_expression && break_closing_paren && !should_extra_indent {
            &SOFTLINE
        } else {
            self.s("")
        };
        let contents = self.concat([test_doc, body, closing]);
        let result = if parent.map(|p| p.key()) == first_non_conditional_parent.map(|p| p.key()) {
            self.docs.group_with(contents, should_break, None)
        } else if should_break {
            self.concat([contents, &BREAK_PARENT])
        } else {
            contents
        };
        if is_parent_test || should_extra_indent {
            self.group(self.concat([self.indent(self.concat([&SOFTLINE, result])), &SOFTLINE]))
        } else {
            result
        }
    }

    /// `(` and `)` that appear only when the JSX-mode ternary breaks.
    fn wrap_jsx_branch(&self, doc: Doc<'a>) -> Doc<'a> {
        self.concat([
            self.if_break(self.s("("), self.s("")),
            self.indent(self.concat([&SOFTLINE, doc])),
            &SOFTLINE,
            self.if_break(self.s(")"), self.s("")),
        ])
    }

    fn has_multiline_block_comment(&self, key: crate::flow::node::NodeKey) -> bool {
        self.has_comment_where(key, None, |comment| {
            matches!(comment.kind, CommentKind::Block)
                && self
                    .text
                    .has_newline_in_range(comment.span.start, comment.span.end)
        })
    }

    /// Prettier's `shouldExtraIndentForConditionalExpression`: a ternary at
    /// the head of a member chain that is itself the whole right side of an
    /// assignment, return, or similar gets one more level of indentation.
    fn should_extra_indent_for_conditional(&self, node: NodeRef<'a>) -> bool {
        let NodeRef::Expression(conditional) = node else {
            return false;
        };
        let mut child: &'a Expression = conditional;
        let mut chain_parent: Option<NodeRef<'a>> = None;
        let ancestors: Vec<NodeRef<'a>> = self.ancestors.iter().rev().skip(1).copied().collect();
        let mut index = 0;
        while chain_parent.is_none() {
            let Some(ancestor) = ancestors.get(index).copied() else {
                return false;
            };
            index += 1;
            if let NodeRef::Expression(ancestor_expression) = ancestor {
                let continues = match &**ancestor_expression {
                    expression::ExpressionInner::Call { inner, .. } => same(&inner.callee, child),
                    expression::ExpressionInner::OptionalCall { inner, .. } => {
                        same(&inner.call.callee, child)
                    }
                    expression::ExpressionInner::Member { inner, .. } => same(&inner.object, child),
                    expression::ExpressionInner::OptionalMember { inner, .. } => {
                        same(&inner.member.object, child)
                    }
                    _ => false,
                };
                if continues {
                    child = ancestor_expression;
                    continue;
                }
                let is_root_wrapper = match &**ancestor_expression {
                    expression::ExpressionInner::New { inner, .. } => same(&inner.callee, child),
                    _ => {
                        is_binary_cast(ancestor_expression)
                            && super::parens::left_side(ancestor_expression)
                                .is_some_and(|e| same(e, child))
                    }
                };
                if is_root_wrapper {
                    chain_parent = ancestors.get(index).copied();
                    child = ancestor_expression;
                    if chain_parent.is_none() {
                        return false;
                    }
                    continue;
                }
            }
            chain_parent = Some(ancestor);
        }
        if same(child, conditional) {
            return false;
        }
        let Some(chain_parent) = chain_parent else {
            return false;
        };
        match chain_parent {
            NodeRef::Expression(parent_expression) => match &**parent_expression {
                expression::ExpressionInner::Assignment { inner, .. } => same(&inner.right, child),
                expression::ExpressionInner::Unary { inner, .. } => same(&inner.argument, child),
                expression::ExpressionInner::Yield { inner, .. } => inner
                    .argument
                    .as_ref()
                    .is_some_and(|argument| same(argument, child)),
                _ => false,
            },
            NodeRef::Declarator(declarator) => declarator
                .init
                .as_ref()
                .is_some_and(|init| same(init, child)),
            NodeRef::Statement(statement) => match &**statement {
                statement::StatementInner::Return { inner, .. } => inner
                    .argument
                    .as_ref()
                    .is_some_and(|argument| same(argument, child)),
                statement::StatementInner::Throw { inner, .. } => same(&inner.argument, child),
                _ => false,
            },
            _ => false,
        }
    }
}
