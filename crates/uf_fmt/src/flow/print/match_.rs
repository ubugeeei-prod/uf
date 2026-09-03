//! `match` expressions and statements with every pattern kind, and Flow
//! enums.

use uf_flow::Loc;
use uf_flow::ast::{match_, match_pattern, statement};

use super::Printer;
use crate::doc::{Doc, HARDLINE, LINE, SOFTLINE};
use crate::flow::comments::Marker;
use crate::flow::node::{
    Expression, MatchExpressionCase, MatchPattern, MatchStatementCase, NodeKey, NodeRef,
};

impl<'a> Printer<'a> {
    /// `match (arg) { pattern => expression, … }`.
    pub fn print_match_expression(
        &mut self,
        m: &'a match_::Match<Loc, Loc, Expression>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        let arg = self.print_expression(&m.arg);
        let mut cases: Vec<Doc<'a>> = Vec::with_capacity(m.cases.len() * 2);
        let last = m.cases.len().saturating_sub(1);
        for (index, case) in m.cases.iter().enumerate() {
            let printed = self.print_node(NodeRef::MatchExpressionCase(case), |p| {
                p.print_match_expression_case(case)
            });
            cases.push(self.concat([printed, self.s(",")]));
            if index != last {
                cases.push(&HARDLINE);
                if self.text.is_next_line_empty(self.text.span(&case.loc).end) {
                    cases.push(&HARDLINE);
                }
            }
        }
        self.print_match_body(arg, cases, key)
    }

    /// `match (arg) { pattern => { … } … }`.
    pub fn print_match_statement(
        &mut self,
        m: &'a match_::Match<Loc, Loc, crate::flow::node::Statement>,
        key: NodeKey,
    ) -> Doc<'a> {
        let arg = self.print_expression(&m.arg);
        let mut cases: Vec<Doc<'a>> = Vec::with_capacity(m.cases.len() * 2);
        let last = m.cases.len().saturating_sub(1);
        for (index, case) in m.cases.iter().enumerate() {
            let printed = self.print_node(NodeRef::MatchStatementCase(case), |p| {
                p.print_match_statement_case(case)
            });
            cases.push(printed);
            if index != last {
                cases.push(&HARDLINE);
                if self.text.is_next_line_empty(self.text.span(&case.loc).end) {
                    cases.push(&HARDLINE);
                }
            }
        }
        self.print_match_body(arg, cases, key)
    }

    fn print_match_body(&mut self, arg: Doc<'a>, cases: Vec<Doc<'a>>, key: NodeKey) -> Doc<'a> {
        let head = self.group(self.concat([
            self.s("match ("),
            self.indent(self.concat([&SOFTLINE, arg])),
            &SOFTLINE,
            self.s(")"),
        ]));
        let dangling = self.print_dangling_comments(key, Marker::None, false);
        if cases.is_empty() {
            let inner = match dangling {
                Some(dangling) => {
                    self.concat([self.indent(self.concat([&HARDLINE, dangling])), &HARDLINE])
                }
                None => self.s(""),
            };
            return self.concat([head, self.s(" {"), inner, self.s("}")]);
        }
        let mut body = self.docs.concat_vec(cases);
        if let Some(dangling) = dangling {
            body = self.concat([body, &HARDLINE, dangling]);
        }
        self.concat([
            head,
            self.s(" {"),
            self.indent(self.concat([&HARDLINE, body])),
            &HARDLINE,
            self.s("}"),
        ])
    }

    fn print_match_expression_case(&mut self, case: &'a MatchExpressionCase) -> Doc<'a> {
        let pattern = self.print_match_pattern(&case.pattern);
        let guard = self.print_match_guard(case.guard.as_ref());
        let body = self.print_expression(&case.body);
        self.group(self.concat([
            pattern,
            guard,
            self.s(" =>"),
            self.group(self.indent(self.concat([&LINE, body]))),
        ]))
    }

    fn print_match_statement_case(&mut self, case: &'a MatchStatementCase) -> Doc<'a> {
        let pattern = self.print_match_pattern(&case.pattern);
        let guard = self.print_match_guard(case.guard.as_ref());
        let body = self.print_statement(&case.body);
        self.concat([pattern, guard, self.s(" => "), body])
    }

    fn print_match_guard(&mut self, guard: Option<&'a Expression>) -> Doc<'a> {
        match guard {
            Some(guard) => {
                let printed = self.print_expression(guard);
                self.concat([self.s(" if ("), printed, self.s(")")])
            }
            None => self.s(""),
        }
    }

    /// Any match pattern, with its comments.
    pub fn print_match_pattern(&mut self, pattern: &'a MatchPattern) -> Doc<'a> {
        self.print_node(NodeRef::MatchPattern(pattern), |p| {
            p.print_match_pattern_inner(pattern)
        })
    }

    fn print_match_pattern_inner(&mut self, pattern: &'a MatchPattern) -> Doc<'a> {
        use match_pattern::MatchPattern as P;
        match pattern {
            P::WildcardPattern { .. } => self.s("_"),
            P::NumberPattern { inner, .. } => self.print_number_literal(inner),
            P::BigIntPattern { inner, .. } => self.print_bigint_literal(inner),
            P::StringPattern { inner, .. } => self.print_string_literal(inner),
            P::BooleanPattern { inner, .. } => self.print_boolean_literal(inner),
            P::NullPattern { .. } => self.s("null"),
            P::UnaryPattern { inner, .. } => {
                let operator = match inner.operator {
                    match_pattern::unary_pattern::Operator::Plus => self.s("+"),
                    match_pattern::unary_pattern::Operator::Minus => self.s("-"),
                };
                let argument = match &inner.argument.1 {
                    match_pattern::unary_pattern::Argument::NumberLiteral(literal) => {
                        self.print_number_literal(literal)
                    }
                    match_pattern::unary_pattern::Argument::BigIntLiteral(literal) => {
                        self.print_bigint_literal(literal)
                    }
                };
                self.concat([operator, argument])
            }
            P::BindingPattern { inner, .. } => self.print_match_binding(inner),
            P::IdentifierPattern { inner, .. } => self.print_identifier(inner),
            P::MemberPattern { inner, .. } => self.print_match_member(inner),
            P::ObjectPattern { inner, .. } => {
                self.print_match_object_pattern(inner, NodeRef::MatchPattern(pattern).key())
            }
            P::ArrayPattern { inner, .. } => {
                let key = NodeRef::MatchPattern(pattern).key();
                let mut printed: Vec<Doc<'a>> = inner
                    .elements
                    .iter()
                    .map(|element| self.print_match_pattern(&element.pattern))
                    .collect();
                if let Some(rest) = &inner.rest {
                    printed.push(self.print_match_rest(rest));
                }
                if printed.is_empty() {
                    let dangling = self.print_dangling_comments(key, Marker::None, false);
                    return self.concat([self.s("["), dangling.unwrap_or(self.s("")), self.s("]")]);
                }
                let separator = self.concat([self.s(","), &LINE]);
                self.group(self.concat([
                    self.s("["),
                    self.indent(self.concat([&SOFTLINE, self.join(separator, printed)])),
                    if inner.rest.is_some() {
                        self.s("")
                    } else {
                        self.if_break(self.s(","), self.s(""))
                    },
                    &SOFTLINE,
                    self.s("]"),
                ]))
            }
            P::OrPattern { inner, .. } => {
                let printed: Vec<Doc<'a>> = inner
                    .patterns
                    .iter()
                    .map(|pattern| self.print_match_pattern(pattern))
                    .collect();
                let separator = self.concat([&LINE, self.s("| ")]);
                self.group(self.indent(self.join(separator, printed)))
            }
            P::AsPattern { inner, .. } => {
                let pattern = self.print_match_pattern(&inner.pattern);
                let target = match &inner.target {
                    match_pattern::as_pattern::Target::Identifier(id) => self.print_identifier(id),
                    match_pattern::as_pattern::Target::Binding { loc, pattern } => self
                        .print_node(NodeRef::MatchBinding(loc, pattern), |p| {
                            p.print_match_binding(pattern)
                        }),
                };
                self.concat([pattern, self.s(" as "), target])
            }
            P::InstancePattern { inner, .. } => {
                let constructor = match &inner.constructor {
                    match_pattern::InstancePatternConstructor::IdentifierConstructor(id) => {
                        self.print_identifier(id)
                    }
                    match_pattern::InstancePatternConstructor::MemberConstructor(member) => self
                        .print_node(NodeRef::MatchMember(member), |p| {
                            p.print_match_member(member)
                        }),
                };
                let properties = self.print_match_object_pattern(
                    &inner.properties.1,
                    NodeRef::MatchPattern(pattern).key(),
                );
                self.concat([constructor, self.s(" "), properties])
            }
        }
    }

    fn print_match_binding(
        &mut self,
        binding: &'a match_pattern::BindingPattern<Loc, Loc>,
    ) -> Doc<'a> {
        let id = self.print_identifier(&binding.id);
        self.concat([self.s(binding.kind.as_str()), self.s(" "), id])
    }

    fn print_match_member(
        &mut self,
        member: &'a match_pattern::MemberPattern<Loc, Loc>,
    ) -> Doc<'a> {
        let base = match &member.base {
            match_pattern::member_pattern::Base::BaseIdentifier(id) => self.print_identifier(id),
            match_pattern::member_pattern::Base::BaseMember(inner) => {
                self.print_node(NodeRef::MatchMember(inner), |p| p.print_match_member(inner))
            }
        };
        match &member.property {
            match_pattern::member_pattern::Property::PropertyIdentifier(id) => {
                let id = self.print_identifier(id);
                self.concat([base, self.s("."), id])
            }
            match_pattern::member_pattern::Property::PropertyString { loc, literal } => {
                let printed = self.print_string_node(loc, literal);
                self.concat([base, self.s("["), printed, self.s("]")])
            }
            match_pattern::member_pattern::Property::PropertyNumber { loc, literal } => {
                let printed = self.print_node(NodeRef::NumberLiteral(loc, literal), |p| {
                    p.print_number_literal(literal)
                });
                self.concat([base, self.s("["), printed, self.s("]")])
            }
            match_pattern::member_pattern::Property::PropertyBigInt { loc, literal } => {
                let printed = self.print_node(NodeRef::BigIntLiteral(loc, literal), |p| {
                    p.print_bigint_literal(literal)
                });
                self.concat([base, self.s("["), printed, self.s("]")])
            }
        }
    }

    fn print_match_rest(&mut self, rest: &'a match_pattern::RestPattern<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::MatchRest(rest), |p| match &rest.argument {
            Some((loc, binding)) => {
                let printed = p.print_node(NodeRef::MatchBinding(loc, binding), |p| {
                    p.print_match_binding(binding)
                });
                p.concat([p.s("..."), printed])
            }
            None => p.s("..."),
        })
    }

    /// `{key: pattern, shorthand, ...rest}` — Prettier prints match object
    /// patterns without bracket spacing.
    fn print_match_object_pattern(
        &mut self,
        object: &'a match_pattern::ObjectPattern<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let mut printed: Vec<Doc<'a>> = object
            .properties
            .iter()
            .map(|property| {
                self.print_node(NodeRef::MatchProperty(property), |p| match property {
                    match_pattern::object_pattern::Property::Valid { property, .. } => {
                        let value = p.print_match_pattern(&property.pattern);
                        if property.shorthand {
                            return value;
                        }
                        let key = match &property.key {
                            match_pattern::object_pattern::Key::Identifier(id) => {
                                p.print_identifier(id)
                            }
                            match_pattern::object_pattern::Key::StringLiteral((loc, literal)) => {
                                p.print_string_node(loc, literal)
                            }
                            match_pattern::object_pattern::Key::NumberLiteral((loc, literal)) => p
                                .print_node(NodeRef::NumberLiteral(loc, literal), |p| {
                                    p.print_number_literal(literal)
                                }),
                            match_pattern::object_pattern::Key::BigIntLiteral((loc, literal)) => p
                                .print_node(NodeRef::BigIntLiteral(loc, literal), |p| {
                                    p.print_bigint_literal(literal)
                                }),
                        };
                        p.concat([key, p.s(": "), value])
                    }
                    match_pattern::object_pattern::Property::InvalidShorthand {
                        identifier,
                        ..
                    } => p.print_identifier(identifier),
                })
            })
            .collect();
        if let Some(rest) = &object.rest {
            printed.push(self.print_match_rest(rest));
        }
        if printed.is_empty() {
            let dangling = self.print_dangling_comments(key, Marker::None, false);
            return self.concat([self.s("{"), dangling.unwrap_or(self.s("")), self.s("}")]);
        }
        let separator = self.concat([self.s(","), &LINE]);
        self.group(self.concat([
            self.s("{"),
            self.indent(self.concat([&SOFTLINE, self.join(separator, printed)])),
            if object.rest.is_some() {
                self.s("")
            } else {
                self.if_break(self.s(","), self.s(""))
            },
            &SOFTLINE,
            self.s("}"),
        ]))
    }

    /// `enum Name of type { A, B = 1, ... }`, always one member per line.
    pub fn print_enum(
        &mut self,
        declaration: &'a statement::EnumDeclaration<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let _ = key;
        let mut parts = Vec::new();
        if declaration.const_ {
            parts.push(self.s("const "));
        }
        parts.push(self.s("enum "));
        parts.push(self.print_identifier(&declaration.id));
        let body = &declaration.body;
        if let Some((_, explicit)) = &body.explicit_type {
            parts.push(self.s(" of "));
            parts.push(self.s(explicit.as_str()));
        }
        parts.push(self.s(" "));
        let body_doc = self.print_node(NodeRef::EnumBody(body), |p| {
            let key = NodeRef::EnumBody(body).key();
            let mut members: Vec<Doc<'a>> = Vec::with_capacity(body.members.len() * 2);
            for member in body.members.iter() {
                let printed =
                    p.print_node(NodeRef::EnumMember(member), |p| p.print_enum_member(member));
                members.push(p.concat([printed, p.s(",")]));
                members.push(&HARDLINE);
            }
            if body.has_unknown_members.is_some() {
                members.push(p.s("..."));
                members.push(&HARDLINE);
            }
            if let Some(dangling) = p.print_dangling_comments(key, Marker::None, false) {
                members.push(dangling);
                members.push(&HARDLINE);
            }
            if members.is_empty() {
                return p.s("{}");
            }
            // Drop the last hardline; the closing brace adds its own.
            members.pop();
            let inner = p.docs.concat_vec(members);
            p.concat([
                p.s("{"),
                p.indent(p.concat([&HARDLINE, inner])),
                &HARDLINE,
                p.s("}"),
            ])
        });
        parts.push(body_doc);
        self.docs.concat_vec(parts)
    }

    fn print_enum_member(
        &mut self,
        member: &'a statement::enum_declaration::Member<Loc>,
    ) -> Doc<'a> {
        use statement::enum_declaration::{Member, MemberName};
        let name = |p: &mut Self, name: &'a MemberName<Loc>| match name {
            MemberName::Identifier(id) => p.print_identifier(id),
            MemberName::StringLiteral(loc, literal) => p.print_string_node(loc, literal),
        };
        match member {
            Member::BooleanMember(member) => {
                let id = name(self, &member.id);
                let init = self.print_node(
                    NodeRef::BooleanLiteral(&member.init.0, &member.init.1),
                    |p| p.print_boolean_literal(&member.init.1),
                );
                self.concat([id, self.s(" = "), init])
            }
            Member::NumberMember(member) => {
                let id = name(self, &member.id);
                let init = self.print_node(
                    NodeRef::NumberLiteral(&member.init.0, &member.init.1),
                    |p| p.print_number_literal(&member.init.1),
                );
                self.concat([id, self.s(" = "), init])
            }
            Member::StringMember(member) => {
                let id = name(self, &member.id);
                let init = self.print_string_node(&member.init.0, &member.init.1);
                self.concat([id, self.s(" = "), init])
            }
            Member::BigIntMember(member) => {
                let id = name(self, &member.id);
                let init = self.print_node(
                    NodeRef::BigIntLiteral(&member.init.0, &member.init.1),
                    |p| p.print_bigint_literal(&member.init.1),
                );
                self.concat([id, self.s(" = "), init])
            }
            Member::DefaultedMember(member) => name(self, &member.id),
        }
    }
}
