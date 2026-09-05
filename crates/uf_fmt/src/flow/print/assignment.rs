//! Assignments, declarators, properties and type aliases: Prettier's
//! `printAssignment` and its layout chooser.
//!
//! Everything of the shape `left = right` goes through one function that
//! first decides a *layout* — break after the operator, never break after
//! it, break the left side, format as a chain — from what the two sides
//! are, and then builds the doc for that layout. The chooser is the part
//! worth reading: it is a list of judgement calls Prettier accumulated
//! about which side of an assignment should give way first.

use uf_flow::Loc;
use uf_flow::ast::{expression, pattern, statement, types};

use super::Printer;
use super::binary::should_inline_logical_expression;
use super::parens::{is_binaryish, is_member};
use crate::doc::{Doc, LINE, LINE_SUFFIX_BOUNDARY, can_break, text_of};
use crate::flow::node::{Expression, NodeRef, Type};

/// How an assignment lays out its two sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// First break after the operator, then the sides are broken
    /// independently on their own lines.
    BreakAfterOperator,
    /// First break the right side, then the left.
    NeverBreakAfterOperator,
    /// First break the right side, then after the operator.
    Fluid,
    /// Break the left side first.
    BreakLhs,
    /// A link of an assignment chain, which breaks as a whole.
    Chain,
    /// The last link of an assignment chain.
    ChainTail,
    /// The last link of an assignment chain whose value is an arrow chain.
    ChainTailArrowChain,
    /// No right side.
    OnlyLeft,
}

/// What Prettier passes down to the print of one child: the layout its
/// assignment chose, or that it is the hugged first or last argument of a
/// call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct PrintArgs {
    /// The layout of the assignment this node is the right side of.
    pub assignment_layout: Option<Layout>,
    /// This node is the first argument of a call and is being tried hugged.
    pub expand_first_arg: bool,
    /// This node is the last argument of a call and is being tried hugged.
    pub expand_last_arg: bool,
}

/// The right side of an assignment-like construct.
#[derive(Clone, Copy)]
pub enum Rhs<'a> {
    /// An expression.
    Expression(&'a Expression),
    /// A type, for type aliases.
    Type(&'a Type),
}

fn is_assignment(expression: &Expression) -> bool {
    matches!(**expression, expression::ExpressionInner::Assignment { .. })
}

/// Whether `node` is an assignment expression or a variable declarator.
fn is_assignment_or_declarator(node: NodeRef<'_>) -> bool {
    match node {
        NodeRef::Declarator(_) => true,
        NodeRef::Expression(expression) => is_assignment(expression),
        _ => false,
    }
}

impl<'a> Printer<'a> {
    /// `left = right` as an expression, on top of the ancestor stack.
    pub fn print_assignment_expression(
        &mut self,
        assignment: &'a expression::Assignment<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let left = self.print_pattern(&assignment.left);
        let operator: &'static str = match &assignment.operator {
            Some(operator) => match operator.as_str() {
                "+=" => " +=",
                "-=" => " -=",
                "*=" => " *=",
                "**=" => " **=",
                "/=" => " /=",
                "%=" => " %=",
                "<<=" => " <<=",
                ">>=" => " >>=",
                ">>>=" => " >>>=",
                "|=" => " |=",
                "^=" => " ^=",
                "&=" => " &=",
                "??=" => " ??=",
                "&&=" => " &&=",
                "||=" => " ||=",
                _ => " =",
            },
            None => " =",
        };
        self.print_assignment_like(
            NodeRef::Expression(expression),
            left,
            operator,
            Some(Rhs::Expression(&assignment.right)),
        )
    }

    /// Print `left operator right` in whichever layout suits the two
    /// sides. `node` is the assignment-like node, which must be on top of
    /// the ancestor stack.
    pub fn print_assignment_like(
        &mut self,
        node: NodeRef<'a>,
        left: Doc<'a>,
        operator: &'static str,
        rhs: Option<Rhs<'a>>,
    ) -> Doc<'a> {
        let layout = self.choose_layout(node, left, rhs);
        let right = match rhs {
            Some(Rhs::Expression(expression)) => self.print_expression_with(
                expression,
                PrintArgs {
                    assignment_layout: Some(layout),
                    ..PrintArgs::default()
                },
            ),
            Some(Rhs::Type(ty)) => self.print_type(ty),
            None => self.s(""),
        };
        let operator = self.s(operator);
        match layout {
            Layout::BreakAfterOperator => self.group(self.concat([
                self.group(left),
                operator,
                self.group(self.indent(self.concat([&LINE, right]))),
            ])),
            Layout::NeverBreakAfterOperator => {
                self.group(self.concat([self.group(left), operator, self.s(" "), right]))
            }
            Layout::Fluid => {
                let group_id = self.docs.group_id();
                let after_operator =
                    self.docs
                        .group_with(self.indent(&LINE), false, Some(group_id));
                self.group(self.concat([
                    self.group(left),
                    operator,
                    after_operator,
                    &LINE_SUFFIX_BOUNDARY,
                    self.docs.indent_if_break(right, group_id, false),
                ]))
            }
            Layout::BreakLhs => {
                self.group(self.concat([left, operator, self.s(" "), self.group(right)]))
            }
            Layout::Chain => self.concat([self.group(left), operator, &LINE, right]),
            Layout::ChainTail => self.concat([
                self.group(left),
                operator,
                self.indent(self.concat([&LINE, right])),
            ]),
            Layout::ChainTailArrowChain => self.concat([self.group(left), operator, right]),
            Layout::OnlyLeft => left,
        }
    }

    fn choose_layout(&mut self, node: NodeRef<'a>, left: Doc<'a>, rhs: Option<Rhs<'a>>) -> Layout {
        let Some(rhs) = rhs else {
            return Layout::OnlyLeft;
        };
        let right_expression = match rhs {
            Rhs::Expression(expression) => Some(expression),
            Rhs::Type(_) => None,
        };

        // Short assignment chains (only 2 segments) are NOT formatted as
        // chains: `a = b = c;` and `const a = b = c;`.
        let is_tail = !right_expression.is_some_and(is_assignment);
        let parent = self.parent();
        let grandparent = self.grandparent();
        let should_use_chain_formatting = is_assignment_or_declarator(node)
            && matches!(node, NodeRef::Expression(_))
            && parent.is_some_and(is_assignment_or_declarator)
            && (!is_tail
                || !matches!(grandparent, Some(NodeRef::Statement(statement))
                    if matches!(**statement, statement::StatementInner::Expression { .. } | statement::StatementInner::VariableDeclaration { .. })));
        if should_use_chain_formatting {
            let is_arrow_chain_tail = is_tail
                && right_expression.is_some_and(|right| {
                    matches!(&**right, expression::ExpressionInner::ArrowFunction { inner, .. }
                        if matches!(&inner.body, uf_flow::ast::function::Body::BodyExpression(body)
                            if matches!(**body, expression::ExpressionInner::ArrowFunction { .. })))
                });
            return if !is_tail {
                Layout::Chain
            } else if is_arrow_chain_tail {
                Layout::ChainTailArrowChain
            } else {
                Layout::ChainTail
            };
        }

        let right_key = match rhs {
            Rhs::Expression(expression) => NodeRef::Expression(expression).key(),
            Rhs::Type(ty) => NodeRef::Type(ty).key(),
        };
        let right_is_nested_assignment = right_expression.is_some_and(|right| {
            !is_tail
                && matches!(&**right, expression::ExpressionInner::Assignment { inner, .. } if is_assignment(&inner.right))
        });
        if right_is_nested_assignment || self.has_leading_own_line_comment(right_key, false) {
            return Layout::BreakAfterOperator;
        }

        if matches!(node, NodeRef::ImportAttribute(_))
            || right_expression.is_some_and(is_require_call)
        {
            return Layout::NeverBreakAfterOperator;
        }

        let left_can_break = can_break(left);
        if self.is_complex_destructuring(node)
            || self.has_complex_type_annotation(node)
            || (is_arrow_function_declarator(node) && left_can_break)
        {
            return Layout::BreakLhs;
        }

        // Wrapping object properties with very short keys usually doesn't
        // add much value.
        let has_short_key = self.is_object_property_with_short_key(node, left);
        if let Some(right) = right_expression
            && self.should_break_after_operator(right, has_short_key)
        {
            return Layout::BreakAfterOperator;
        }
        if let Rhs::Type(ty) = rhs
            && self.type_should_break_after_operator(ty)
        {
            return Layout::BreakAfterOperator;
        }

        if self.is_complex_type_alias_params(node) {
            return Layout::BreakLhs;
        }

        let right_never_breaks = match rhs {
            Rhs::Expression(right) => matches!(
                **right,
                expression::ExpressionInner::TemplateLiteral { .. }
                    | expression::ExpressionInner::TaggedTemplate { .. }
                    | expression::ExpressionInner::BooleanLiteral { .. }
                    | expression::ExpressionInner::NumberLiteral { .. }
                    | expression::ExpressionInner::Class { .. }
            ),
            Rhs::Type(_) => false,
        };
        if !left_can_break && (has_short_key || right_never_breaks) {
            return Layout::NeverBreakAfterOperator;
        }
        Layout::Fluid
    }

    /// Prettier's `shouldBreakAfterOperator` for an expression right side.
    fn should_break_after_operator(&mut self, right: &'a Expression, has_short_key: bool) -> bool {
        use expression::ExpressionInner as E;
        if is_binaryish(right) && !should_inline_logical_expression(right) {
            return true;
        }
        match &**right {
            E::Sequence { .. } => return true,
            E::Conditional { inner, .. } => {
                return is_binaryish(&inner.test) && !should_inline_logical_expression(&inner.test);
            }
            E::Class { inner, .. } => return !inner.class_decorators.is_empty(),
            _ => {}
        }
        if has_short_key {
            return false;
        }
        let mut node = right;
        loop {
            match &**node {
                E::Unary { inner, .. } => node = &inner.argument,
                E::Yield { inner, .. } if inner.argument.is_some() => {
                    node = inner.argument.as_ref().expect("checked");
                }
                _ => break,
            }
        }
        if matches!(**node, E::StringLiteral { .. }) {
            return true;
        }
        self.is_poorly_breakable_member_or_call_chain(node, false)
    }

    /// Prettier's `shouldBreakAfterOperator` for a type right side.
    fn type_should_break_after_operator(&self, ty: &'a Type) -> bool {
        match &**ty {
            types::TypeInner::StringLiteral { .. } => true,
            types::TypeInner::Conditional { inner, .. } => {
                self.type_is_function_like(&inner.check_type)
                    || self.type_is_function_like(&inner.extends_type)
            }
            _ => false,
        }
    }

    fn type_is_function_like(&self, ty: &'a Type) -> bool {
        matches!(
            **ty,
            types::TypeInner::Function { .. } | types::TypeInner::ConstructorType { .. }
        )
    }

    /// A chain with no calls at all, or with calls with no arguments or
    /// lone short arguments, that would not break well after the operator.
    fn is_poorly_breakable_member_or_call_chain(
        &mut self,
        node: &'a Expression,
        deep: bool,
    ) -> bool {
        use expression::ExpressionInner as E;
        match &**node {
            E::Member { inner, .. } => {
                self.is_poorly_breakable_member_or_call_chain(&inner.object, true)
            }
            E::OptionalMember { inner, .. } => {
                self.is_poorly_breakable_member_or_call_chain(&inner.member.object, true)
            }
            E::Call { .. } | E::OptionalCall { .. } => {
                let (callee, arguments, targs) = match &**node {
                    E::Call { inner, .. } => {
                        (&inner.callee, &inner.arguments, inner.targs.as_ref())
                    }
                    E::OptionalCall { inner, .. } => (
                        &inner.call.callee,
                        &inner.call.arguments,
                        inner.call.targs.as_ref(),
                    ),
                    _ => unreachable!(),
                };
                if is_member(callee) && self.is_member_chain_printed_as_chain(node) {
                    return false;
                }
                let is_poorly_breakable_call = arguments.arguments.is_empty()
                    || (arguments.arguments.len() == 1
                        && matches!(&arguments.arguments[0], expression::ExpressionOrSpread::Expression(argument)
                            if self.is_lone_short_argument(argument)));
                if !is_poorly_breakable_call {
                    return false;
                }
                if targs.is_some_and(|targs| self.is_complex_call_type_args(targs)) {
                    return false;
                }
                self.is_poorly_breakable_member_or_call_chain(callee, true)
            }
            E::Identifier { .. } | E::This { .. } => deep,
            _ => false,
        }
    }

    /// Whether printing `call` produces a member chain doc (one that the
    /// chain printer labelled), which Prettier treats as breakable.
    fn is_member_chain_printed_as_chain(&mut self, call: &'a Expression) -> bool {
        let printed = self.print_expression(call);
        crate::doc::label_of(printed) == Some(crate::doc::Label::MemberChain)
    }

    fn is_complex_call_type_args(&mut self, targs: &'a expression::CallTypeArgs<Loc, Loc>) -> bool {
        if targs.arguments.len() > 1 {
            return true;
        }
        if let Some(expression::CallTypeArg::Explicit(ty)) = targs.arguments.first()
            && matches!(
                **ty,
                types::TypeInner::Union { .. }
                    | types::TypeInner::Intersection { .. }
                    | types::TypeInner::Object { .. }
            )
        {
            return true;
        }
        let printed = self.print_call_type_args(targs);
        crate::doc::will_break(printed)
    }

    /// Prettier's `isLoneShortArgument`.
    pub fn is_lone_short_argument(&self, argument: &'a Expression) -> bool {
        use expression::ExpressionInner as E;
        if self.has_comment(NodeRef::Expression(argument).key()) {
            return false;
        }
        let threshold = self.options.line_width / 4;
        match &**argument {
            E::This { .. } => true,
            E::Identifier { inner, .. } => inner.name.len() <= threshold,
            E::Unary { inner, .. }
                if matches!(
                    inner.operator,
                    expression::UnaryOperator::Minus | expression::UnaryOperator::Plus
                ) && matches!(*inner.argument, E::NumberLiteral { .. }) =>
            {
                !self.has_comment(NodeRef::Expression(&inner.argument).key())
            }
            E::RegExpLiteral { inner, .. } => inner.pattern.len() <= threshold,
            E::StringLiteral { inner, .. } => {
                super::literal::print_string(&inner.raw, self.options.quote).len() <= threshold
            }
            E::TemplateLiteral { inner, .. } => {
                inner.expressions.is_empty()
                    && inner.quasis.first().is_some_and(|quasi| {
                        quasi.value.raw.len() <= threshold && !quasi.value.raw.contains('\n')
                    })
            }
            E::Unary { inner, .. } => self.is_lone_short_argument(&inner.argument),
            E::Call { inner, .. } => {
                inner.arguments.arguments.is_empty()
                    && matches!(&*inner.callee, E::Identifier { inner: callee, .. } if callee.name.len() + 2 <= threshold)
            }
            E::NumberLiteral { .. }
            | E::BigIntLiteral { .. }
            | E::BooleanLiteral { .. }
            | E::NullLiteral { .. } => true,
            _ => false,
        }
    }

    /// An object pattern with more than two properties, some of them
    /// renamed or defaulted, on the left of an assignment or declarator.
    fn is_complex_destructuring(&self, node: NodeRef<'a>) -> bool {
        let left: Option<&'a pattern::Pattern<Loc, Loc>> = match node {
            NodeRef::Declarator(declarator) => Some(&declarator.id),
            NodeRef::Expression(expression) => match &**expression {
                expression::ExpressionInner::Assignment { inner, .. } => Some(&inner.left),
                _ => None,
            },
            _ => None,
        };
        let Some(pattern::Pattern::Object { inner, .. }) = left else {
            return false;
        };
        inner.properties.len() > 2
            && inner.properties.iter().any(|property| match property {
                pattern::object::Property::NormalProperty(property) => {
                    !property.shorthand || property.default.is_some()
                }
                pattern::object::Property::RestElement(_) => false,
            })
    }

    /// A type alias with several type parameters, some bounded or
    /// defaulted.
    fn is_complex_type_alias_params(&self, node: NodeRef<'a>) -> bool {
        let tparams = match node {
            NodeRef::Statement(statement) => match &**statement {
                statement::StatementInner::TypeAlias { inner, .. }
                | statement::StatementInner::DeclareTypeAlias { inner, .. } => {
                    inner.tparams.as_ref()
                }
                _ => None,
            },
            _ => None,
        };
        let Some(tparams) = tparams else {
            return false;
        };
        tparams.params.len() > 1
            && tparams.params.iter().any(|param| {
                matches!(param.bound, types::AnnotationOrHint::Available(_))
                    || param.default.is_some()
            })
    }

    /// A declarator whose type annotation has several type arguments, some
    /// themselves generic or conditional.
    fn has_complex_type_annotation(&self, node: NodeRef<'a>) -> bool {
        let NodeRef::Declarator(declarator) = node else {
            return false;
        };
        let annotation = match &declarator.id {
            pattern::Pattern::Identifier { inner, .. } => &inner.annot,
            pattern::Pattern::Object { inner, .. } => &inner.annot,
            pattern::Pattern::Array { inner, .. } => &inner.annot,
            pattern::Pattern::Expression { .. } => return false,
        };
        let types::AnnotationOrHint::Available(annotation) = annotation else {
            return false;
        };
        let Some(args) = type_args_of(&annotation.annotation) else {
            return false;
        };
        args.len() > 1
            && args.iter().any(|arg| {
                type_args_of(arg).is_some_and(|inner| !inner.is_empty())
                    || matches!(**arg, types::TypeInner::Conditional { .. })
            })
    }

    /// An object property whose key is too short for breaking after the
    /// colon to be worth it.
    fn is_object_property_with_short_key(&self, node: NodeRef<'a>, left: Doc<'a>) -> bool {
        if !matches!(
            node,
            NodeRef::ObjectProperty(_) | NodeRef::ObjectTypeProperty(_)
        ) {
            return false;
        }
        match text_of(left) {
            Some(text) => crate::doc::printer::text_width(text) < self.options.indent_width + 3,
            None => false,
        }
    }
}

/// The type arguments of a generic type reference.
fn type_args_of(ty: &Type) -> Option<&[Type]> {
    match &**ty {
        types::TypeInner::Generic { inner, .. } => {
            inner.targs.as_ref().map(|targs| &*targs.arguments)
        }
        _ => None,
    }
}

fn is_arrow_function_declarator(node: NodeRef<'_>) -> bool {
    let NodeRef::Declarator(declarator) = node else {
        return false;
    };
    matches!(declarator.init.as_deref(),
        Some(expression::ExpressionInner::ArrowFunction { inner, .. }) if inner.tparams.is_some())
}

fn is_require_call(expression: &Expression) -> bool {
    matches!(&**expression, expression::ExpressionInner::Call { inner, .. }
        if matches!(&*inner.callee, expression::ExpressionInner::Identifier { inner: callee, .. } if &*callee.name == "require"))
}
