//! Expressions: the dispatcher, and the kinds simple enough not to need a
//! module of their own.

use uf_flow::Loc;
use uf_flow::ast::{expression, pattern, statement};

use super::parens::{is_call, is_jsx, is_member, same, starts_with_no_lookahead_token};
use super::{PrintArgs, Printer};
use crate::doc::{Doc, LINE, SOFTLINE};
use crate::flow::node::{Expression, NodeRef};

impl<'a> Printer<'a> {
    /// Print any expression, parenthesized where the grammar or Prettier
    /// wants it, with its comments.
    pub fn print_expression(&mut self, expression: &'a Expression) -> Doc<'a> {
        self.print_expression_with(expression, PrintArgs::default())
    }

    /// Print an expression with what its parent knows about its position:
    /// the assignment layout it is the right side of, or that it is a
    /// hugged call argument.
    pub fn print_expression_with(
        &mut self,
        expression: &'a Expression,
        args: PrintArgs,
    ) -> Doc<'a> {
        let node = NodeRef::Expression(expression);
        self.ancestors.push(node);
        let doc = self.print_expression_inner(expression, args);
        let needs_parens = self.needs_parens(expression);
        self.ancestors.pop();
        let doc = if needs_parens {
            self.concat([self.s("("), doc, self.s(")")])
        } else {
            doc
        };
        // JSX prints its own comments, inside the parentheses it adds.
        if is_jsx(expression) {
            return doc;
        }
        self.print_comments(node.key(), doc)
    }

    /// Print an expression without its comments or parentheses, for the
    /// callers that place those themselves.
    pub fn print_expression_bare(&mut self, expression: &'a Expression) -> Doc<'a> {
        let node = NodeRef::Expression(expression);
        self.with_node(node, |p| {
            p.print_expression_inner(expression, PrintArgs::default())
        })
    }

    fn print_expression_inner(&mut self, expression: &'a Expression, args: PrintArgs) -> Doc<'a> {
        use expression::ExpressionInner as E;
        match &**expression {
            E::Array { inner, .. } => self.print_array(inner, expression),
            E::ArrowFunction { inner, .. } => self.print_arrow_function(inner, expression, args),
            E::AsConstExpression { inner, .. } => {
                let value = self.print_expression(&inner.expression);
                self.concat([value, self.s(" as const")])
            }
            E::AsExpression { inner, .. } => self.print_as_expression(inner),
            E::TSSatisfies { inner, .. } => {
                let value = self.print_expression(&inner.expression);
                let annotation = self.print_type(&inner.annot.annotation);
                self.concat([value, self.s(" satisfies "), annotation])
            }
            E::Assignment { inner, .. } => self.print_assignment_expression(inner, expression),
            E::Binary { .. } | E::Logical { .. } => self.print_binaryish(expression),
            E::Call { inner, .. } => self.print_call(expression, inner, false),
            E::OptionalCall { inner, .. } => self.print_call(
                expression,
                &inner.call,
                matches!(inner.optional, expression::OptionalCallKind::Optional),
            ),
            E::Class { inner, .. } => {
                self.print_class(inner, NodeRef::Expression(expression).key())
            }
            E::Conditional { inner, .. } => self.print_conditional(inner, expression),
            E::Function { inner, .. } => {
                self.print_function(inner, NodeRef::Expression(expression).key(), args)
            }
            E::Identifier { inner, .. } => self.print_identifier(inner),
            E::Import { inner, .. } => self.print_dynamic_import(inner, expression),
            E::JSXElement { inner, .. } => self.print_jsx_element(inner, expression),
            E::JSXFragment { inner, .. } => self.print_jsx_fragment(inner, expression),
            E::StringLiteral { inner, .. } => self.print_string_literal(inner),
            E::BooleanLiteral { inner, .. } => self.print_boolean_literal(inner),
            E::NullLiteral { .. } => self.s("null"),
            E::NumberLiteral { inner, .. } => self.print_number_literal(inner),
            E::BigIntLiteral { inner, .. } => self.print_bigint_literal(inner),
            E::RegExpLiteral { inner, .. } => {
                self.text(&super::literal::print_regex(&inner.pattern, &inner.flags))
            }
            E::ModuleRefLiteral { inner, .. } => self.docs.borrowed(&inner.raw),
            E::Match { inner, .. } => self.print_match_expression(inner, expression),
            E::Member { inner, .. } => self.print_member(expression, inner, false),
            E::OptionalMember { inner, .. } => self.print_member(
                expression,
                &inner.member,
                matches!(inner.optional, expression::OptionalMemberKind::Optional),
            ),
            E::MetaProperty { inner, .. } => {
                let meta = self.print_identifier(&inner.meta);
                let property = self.print_identifier(&inner.property);
                self.concat([meta, self.s("."), property])
            }
            E::New { inner, .. } => self.print_new(expression, inner),
            E::Object { inner, .. } => self.print_object(inner, expression),
            E::Record { inner, .. } => {
                // Records are experimental in the port; print the source
                // verbatim so nothing is lost.
                let span = self.text.span(expression.loc());
                let _ = inner;
                self.replace_end_of_line(self.text.slice(span))
            }
            E::Sequence { inner, .. } => self.print_sequence(inner),
            E::Super { .. } => self.s("super"),
            E::TaggedTemplate { inner, .. } => self.print_tagged_template(inner),
            E::TemplateLiteral { inner, .. } => self.print_template_literal(inner),
            E::This { .. } => self.s("this"),
            E::TypeCast { inner, .. } => {
                let value = self.print_expression(&inner.expression);
                let annotation = self.print_type_annotation(&inner.annot);
                self.concat([self.s("("), value, annotation, self.s(")")])
            }
            E::Unary { inner, .. } => self.print_unary(inner, expression),
            E::Update { inner, .. } => {
                let argument = self.print_expression(&inner.argument);
                let operator = match inner.operator {
                    expression::UpdateOperator::Increment => self.s("++"),
                    expression::UpdateOperator::Decrement => self.s("--"),
                };
                if inner.prefix {
                    self.concat([operator, argument])
                } else {
                    self.concat([argument, operator])
                }
            }
            E::Yield { inner, .. } => {
                let mut parts = vec![self.s("yield")];
                if inner.delegate {
                    parts.push(self.s("*"));
                }
                if let Some(argument) = &inner.argument {
                    let argument = self.print_expression(argument);
                    parts.push(self.s(" "));
                    parts.push(argument);
                }
                self.docs.concat_vec(parts)
            }
        }
    }

    /// An identifier, with its comments.
    pub fn print_identifier(
        &mut self,
        identifier: &'a uf_flow::ast::Identifier<Loc, Loc>,
    ) -> Doc<'a> {
        self.print_node(NodeRef::Identifier(identifier), |p| {
            p.docs.borrowed(&identifier.name)
        })
    }

    /// A `#private` name, with its comments.
    pub fn print_private_name(&mut self, name: &'a uf_flow::ast::PrivateName<Loc>) -> Doc<'a> {
        self.print_node(NodeRef::PrivateName(name), |p| {
            p.text(&format!("#{}", name.name))
        })
    }

    fn print_unary(
        &mut self,
        unary: &'a expression::Unary<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        use expression::UnaryOperator as U;
        if matches!(unary.operator, U::Await) {
            return self.print_await(unary, expression);
        }
        let (operator, word) = match unary.operator {
            U::Minus => ("-", false),
            U::Plus => ("+", false),
            U::Not => ("!", false),
            U::BitNot => ("~", false),
            U::Typeof => ("typeof", true),
            U::Void => ("void", true),
            U::Delete => ("delete", true),
            U::Await => ("await", true),
            U::Nonnull => ("", false),
        };
        let argument = self.print_expression(&unary.argument);
        let mut parts = vec![self.s(operator)];
        if word {
            parts.push(self.s(" "));
        }
        if matches!(unary.operator, U::Nonnull) {
            return self.concat([argument, self.s("!")]);
        }
        if self.has_comment(NodeRef::Expression(&unary.argument).key()) {
            parts.push(self.group(self.concat([
                self.s("("),
                self.indent(self.concat([&SOFTLINE, argument])),
                &SOFTLINE,
                self.s(")"),
            ])));
        } else {
            parts.push(argument);
        }
        self.docs.concat_vec(parts)
    }

    /// `x as T`, or `x /*:: as T */` when that is how it was written.
    ///
    /// The comment form is the fourth of Flow's comment types and the one the
    /// corpus is full of: Relay writes every generated artifact with
    /// `v0 /*:: as any*/`, because those files ship in npm packages and are
    /// read by tools that do not strip Flow. `node --check` accepts the
    /// comment and rejects `v0 as any`, so printing it as real syntax turns a
    /// file that runs into one that does not.
    ///
    /// Like the type arguments in `print_call_type_args`, and unlike an
    /// annotation, the location is no help: it starts at the type rather than
    /// at the `/*`. What identifies the form is the block the type sits *in*,
    /// found once for the file. The operand has to be outside that block —
    /// otherwise the whole cast is inside a `/*:: … */` declaration, which
    /// belongs to whoever is printing the block.
    fn print_as_expression(
        &mut self,
        inner: &'a expression::AsExpression<Loc, Loc>,
    ) -> Doc<'a> {
        let value = self.print_expression(&inner.expression);
        if let Some(block) = self.comment_cast_block(inner) {
            let raw = self.text.slice(block);
            let printed = self.replace_end_of_line(raw);
            return self.concat([value, self.s(" "), printed]);
        }
        let annotation = self.print_type(&inner.annot.annotation);
        self.concat([value, self.s(" as "), annotation])
    }

    /// `await x`, which breaks before itself when it is the object of a
    /// member chain so `(await\n x).y` never happens.
    fn print_await(
        &mut self,
        unary: &'a expression::Unary<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let argument = self.print_expression(&unary.argument);
        let parts = self.concat([self.s("await "), argument]);
        let Some((parent, role)) = self.role_of(expression) else {
            return parts;
        };
        let in_chain = matches!(parent, NodeRef::Expression(parent_expression)
            if (is_call(parent_expression) && role == super::parens::Role::Callee)
                || (is_member(parent_expression) && role == super::parens::Role::Object));
        if !in_chain {
            return parts;
        }
        let wrapped = self.concat([self.indent(self.concat([&SOFTLINE, parts])), &SOFTLINE]);
        // Avoid printing `await (await` on one line.
        let enclosing_await =
            self.ancestors
                .iter()
                .rev()
                .skip(1)
                .find_map(|ancestor| match ancestor {
                    NodeRef::Expression(candidate) => match &***candidate {
                        expression::ExpressionInner::Unary { inner, .. }
                            if matches!(inner.operator, expression::UnaryOperator::Await) =>
                        {
                            Some(Some(&inner.argument))
                        }
                        _ => None,
                    },
                    NodeRef::Block(..) => Some(None),
                    NodeRef::Statement(statement)
                        if matches!(***statement, statement::StatementInner::Block { .. }) =>
                    {
                        Some(None)
                    }
                    _ => None,
                });
        match enclosing_await {
            Some(Some(argument))
                if starts_with_no_lookahead_token(argument, &|left| same(left, expression)) =>
            {
                wrapped
            }
            _ => self.group(wrapped),
        }
    }

    fn print_sequence(&mut self, sequence: &'a expression::Sequence<Loc, Loc>) -> Doc<'a> {
        let in_statement_or_for = matches!(self.parent(), Some(NodeRef::Statement(statement))
            if matches!(**statement, statement::StatementInner::Expression { .. } | statement::StatementInner::For { .. }));
        if in_statement_or_for {
            let mut parts = Vec::new();
            for (index, expression) in sequence.expressions.iter().enumerate() {
                let printed = self.print_expression(expression);
                if index == 0 {
                    parts.push(printed);
                } else {
                    parts.push(self.s(","));
                    parts.push(self.indent(self.concat([&LINE, printed])));
                }
            }
            return self.group(self.docs.concat_vec(parts));
        }
        let printed: Vec<Doc<'a>> = sequence
            .expressions
            .iter()
            .map(|expression| self.print_expression(expression))
            .collect();
        let separator = self.concat([self.s(","), &LINE]);
        self.group(self.join(separator, printed))
    }

    /// `a.b`, `a[b]`, `a?.b`, breaking before the `.` when the object is
    /// long and the lookup is not a trivial `a.b`.
    fn print_member(
        &mut self,
        _expression: &'a Expression,
        member: &'a expression::Member<Loc, Loc>,
        optional: bool,
    ) -> Doc<'a> {
        let object = self.print_expression(&member.object);
        let lookup = self.print_member_lookup(member, optional);
        let first_non_member_parent = self
            .ancestors
            .iter()
            .rev()
            .skip(1)
            .find(|ancestor| !matches!(ancestor, NodeRef::Expression(e) if is_member(e)))
            .copied();
        let should_inline = match first_non_member_parent {
            Some(NodeRef::Expression(parent)) => match &**parent {
                expression::ExpressionInner::New { .. } => true,
                expression::ExpressionInner::Assignment { inner, .. } => {
                    !matches!(inner.left, pattern::Pattern::Identifier { .. })
                        && !matches!(&inner.left, pattern::Pattern::Expression { inner, .. } if matches!(***inner, expression::ExpressionInner::Identifier { .. }))
                }
                _ => false,
            },
            _ => false,
        } || matches!(
            member.property,
            expression::member::Property::PropertyExpression(_)
        ) || (matches!(
            *member.object,
            expression::ExpressionInner::Identifier { .. }
        ) && matches!(
            member.property,
            expression::member::Property::PropertyIdentifier(_)
        ) && !matches!(self.parent(), Some(NodeRef::Expression(parent)) if is_member(parent)));
        let doc = if should_inline {
            self.concat([object, lookup])
        } else {
            self.concat([
                object,
                self.group(self.indent(self.concat([&SOFTLINE, lookup]))),
            ])
        };
        match crate::doc::label_of(object) {
            Some(label) => self.docs.label(label, doc),
            None => doc,
        }
    }

    /// The `.b`, `?.b`, `[b]` part of a member access.
    pub fn print_member_lookup(
        &mut self,
        member: &'a expression::Member<Loc, Loc>,
        optional: bool,
    ) -> Doc<'a> {
        let optional_token = if optional { self.s("?.") } else { self.s("") };
        match &member.property {
            expression::member::Property::PropertyIdentifier(id) => {
                let property = self.print_identifier(id);
                let dot = if optional { self.s("") } else { self.s(".") };
                self.concat([optional_token, dot, property])
            }
            expression::member::Property::PropertyPrivateName(name) => {
                let property = self.print_private_name(name);
                let dot = if optional { self.s("") } else { self.s(".") };
                self.concat([optional_token, dot, property])
            }
            expression::member::Property::PropertyExpression(property) => {
                let printed = self.print_expression(property);
                if matches!(
                    **property,
                    expression::ExpressionInner::NumberLiteral { .. }
                ) {
                    return self.concat([optional_token, self.s("["), printed, self.s("]")]);
                }
                self.group(self.concat([
                    optional_token,
                    self.s("["),
                    self.indent(self.concat([&SOFTLINE, printed])),
                    &SOFTLINE,
                    self.s("]"),
                ]))
            }
        }
    }
}
