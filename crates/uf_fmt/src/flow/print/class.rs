//! Classes and their members: Prettier's `printClass`, `printClassBody`,
//! `printClassMethod` and `printClassProperty`.

use uf_flow::Loc;
use uf_flow::ast::{class, expression, function, statement, types};

use super::Printer;
use super::assignment::Rhs;
use super::parens::is_member;
use crate::doc::{Doc, HARDLINE, LINE, SOFTLINE};
use crate::flow::comments::{Marker, Placement};
use crate::flow::node::{Class, NodeKey, NodeRef};

/// Whether a class member is a property (as opposed to a method or block).
fn is_class_property(member: &class::BodyElement<Loc, Loc>) -> bool {
    matches!(
        member,
        class::BodyElement::Property(_)
            | class::BodyElement::PrivateField(_)
            | class::BodyElement::AbstractProperty(_)
    )
}

/// The key of a member, if it has a conventional one.
fn member_key(member: &class::BodyElement<Loc, Loc>) -> Option<&expression::object::Key<Loc, Loc>> {
    match member {
        class::BodyElement::Method(method) => Some(&method.key),
        class::BodyElement::Property(property) => Some(&property.key),
        class::BodyElement::DeclareMethod(method) => Some(&method.key),
        class::BodyElement::AbstractMethod(method) => Some(&method.key),
        class::BodyElement::AbstractProperty(property) => Some(&property.key),
        _ => None,
    }
}

fn key_name(key: &expression::object::Key<Loc, Loc>) -> Option<&str> {
    match key {
        expression::object::Key::Identifier(id) => Some(&id.name),
        _ => None,
    }
}

impl<'a> Printer<'a> {
    /// `class Name<T> extends Base implements I { … }`. `key` is the node
    /// the class is (a declaration statement or class expression), which
    /// owns the dangling comments before `implements`.
    pub fn print_class(&mut self, class: &'a Class, key: NodeKey) -> Doc<'a> {
        let mut parts: Vec<Doc<'a>> = Vec::new();
        if !class.class_decorators.is_empty() {
            let decorators: Vec<Doc<'a>> = class
                .class_decorators
                .iter()
                .map(|decorator| self.print_decorator(decorator))
                .collect();
            parts.push(self.join(&HARDLINE, decorators));
            parts.push(&HARDLINE);
        }
        parts.push(self.s("class"));

        let group_mode = self.should_print_heritage_in_group(class);
        let mut parts_group: Vec<Doc<'a>> = Vec::new();
        let mut heritage: Vec<Doc<'a>> = Vec::new();
        if let Some(id) = &class.id {
            parts_group.push(self.s(" "));
            let node = NodeRef::Identifier(id);
            let leading = self.print_leading_comments(node.key());
            let printed = self.with_node(node, |p| p.docs.borrowed(&id.name));
            let trailing = self.print_trailing_comments(node.key());
            parts_group.push(leading.unwrap_or(self.s("")));
            parts_group.push(printed);
            parts_group.push(self.indent(trailing.unwrap_or(self.s(""))));
        }
        if let Some(tparams) = &class.tparams {
            let node = NodeRef::TypeParams(tparams);
            let leading = self.print_leading_comments(node.key());
            let printed = self.with_node(node, |p| p.print_type_params_inner(tparams));
            let trailing = self.print_trailing_comments(node.key());
            parts_group.push(leading.unwrap_or(self.s("")));
            parts_group.push(printed);
            parts_group.push(self.indent(trailing.unwrap_or(self.s(""))));
        }
        if let Some(extends) = &class.extends {
            let super_key = NodeRef::Expression(&extends.expr).key();
            let printed = self.print_super_class(&extends.expr);
            let targs = match &extends.targs {
                Some(targs) => self.print_type_args(targs),
                None => self.s(""),
            };
            let with_comments = self.print_comments(super_key, self.concat([printed, targs]));
            let clause = self.concat([self.s("extends "), with_comments]);
            if group_mode {
                heritage.push(&LINE);
                heritage.push(self.group(clause));
            } else {
                heritage.push(self.s(" "));
                heritage.push(clause);
            }
        }
        if let Some(implements) = &class.implements
            && !implements.interfaces.is_empty()
        {
            let printed: Vec<Doc<'a>> = implements
                .interfaces
                .iter()
                .map(|interface| {
                    self.print_node(NodeRef::ClassImplements(interface), |p| {
                        let id = p.print_generic_identifier(&interface.id);
                        let targs = match &interface.targs {
                            Some(targs) => p.print_type_args(targs),
                            None => p.s(""),
                        };
                        p.concat([id, targs])
                    })
                })
                .collect();
            let dangling = self.print_dangling_comments(key, Marker::Implements, false);
            let separator = self.concat([self.s(","), &LINE]);
            let list = self.join(separator, printed);
            let more_than_one_clause = class.extends.is_some();
            if !more_than_one_clause {
                let clause =
                    self.concat([self.s("implements "), dangling.unwrap_or(self.s("")), list]);
                if group_mode {
                    heritage.push(&LINE);
                    heritage.push(self.group(clause));
                } else {
                    heritage.push(self.s(" "));
                    heritage.push(clause);
                }
            } else {
                heritage.push(&LINE);
                if let Some(dangling) = dangling {
                    heritage.push(dangling);
                    heritage.push(&HARDLINE);
                }
                heritage.push(self.s("implements"));
                heritage.push(self.group(self.indent(self.concat([&LINE, list]))));
            }
        }

        let body_is_empty = class.body.body.is_empty()
            && !self.has_comment_placed(NodeRef::ClassBody(&class.body).key(), Placement::Dangling);
        let mut heritage_group_id = None;
        if group_mode {
            let id = self.docs.group_id();
            heritage_group_id = Some(id);
            let mut grouped = parts_group;
            grouped.push(self.indent(self.docs.concat_vec(heritage)));
            parts.push(
                self.docs
                    .group_with(self.docs.concat_vec(grouped), false, Some(id)),
            );
        } else {
            parts.extend(parts_group);
            parts.extend(heritage);
        }
        match heritage_group_id {
            Some(id) if !body_is_empty => {
                parts.push(self.docs.if_break(&HARDLINE, self.s(" "), Some(id)));
            }
            _ => parts.push(self.s(" ")),
        }
        parts.push(self.print_class_body(&class.body));
        self.docs.concat_vec(parts)
    }

    /// The superclass expression, parenthesized when the class is being
    /// assigned and the expression breaks.
    fn print_super_class(&mut self, super_class: &'a crate::flow::node::Expression) -> Doc<'a> {
        let node = NodeRef::Expression(super_class);
        let doc = self.print_expression_bare(super_class);
        let printed = if self.with_node(node, |p| p.needs_parens(super_class)) {
            self.concat([self.s("("), doc, self.s(")")])
        } else {
            doc
        };
        let parent_is_assignment = matches!(self.parent(), Some(NodeRef::Expression(parent))
            if matches!(**parent, expression::ExpressionInner::Assignment { .. }));
        if parent_is_assignment {
            let broken = self.concat([
                self.s("("),
                self.indent(self.concat([&SOFTLINE, printed])),
                &SOFTLINE,
                self.s(")"),
            ]);
            return self.group(self.if_break(broken, printed));
        }
        printed
    }

    /// Prettier's `shouldPrintHeritageClauses` group decision.
    fn should_print_heritage_in_group(&self, class: &'a Class) -> bool {
        if let Some(id) = &class.id
            && self.has_comment_placed(NodeRef::Identifier(id).key(), Placement::Trailing)
        {
            return true;
        }
        if let Some(tparams) = &class.tparams
            && self.has_comment_placed(NodeRef::TypeParams(tparams).key(), Placement::Trailing)
        {
            return true;
        }
        let clause_count = usize::from(class.extends.is_some())
            + class
                .implements
                .as_ref()
                .map_or(0, |implements| implements.interfaces.len());
        if clause_count > 1 {
            return true;
        }
        if let Some(extends) = &class.extends {
            if self.has_comment(NodeRef::Expression(&extends.expr).key()) {
                return true;
            }
            let parent_is_assignment = matches!(self.parent(), Some(NodeRef::Expression(parent))
                if matches!(**parent, expression::ExpressionInner::Assignment { .. }));
            if parent_is_assignment {
                return false;
            }
            return extends.targs.is_none() && is_member(&extends.expr);
        }
        false
    }

    /// `{ members }`, one per line.
    pub fn print_class_body(&mut self, body: &'a class::Body<Loc, Loc>) -> Doc<'a> {
        let node = NodeRef::ClassBody(body);
        self.print_node(node, |p| {
            let mut parts: Vec<Doc<'a>> = Vec::new();
            let count = body.body.len();
            for (index, member) in body.body.iter().enumerate() {
                let printed = p.print_class_member(member);
                parts.push(printed);
                let next = body.body.get(index + 1);
                if !p.options.semi
                    && is_class_property(member)
                    && p.should_print_semicolon_after_class_property(member, next)
                {
                    parts.push(p.s(";"));
                }
                if index + 1 < count {
                    parts.push(&HARDLINE);
                    if p.text
                        .is_next_line_empty(p.text.span(&NodeRef::ClassMember(member).loc()).end)
                    {
                        parts.push(&HARDLINE);
                    }
                }
            }
            if let Some(dangling) = p.print_dangling_comments(node.key(), Marker::None, false) {
                parts.push(dangling);
            }
            if parts.is_empty() {
                return p.s("{}");
            }
            let inner = p.docs.concat_vec(parts);
            p.concat([
                p.s("{"),
                p.indent(p.concat([&HARDLINE, inner])),
                &HARDLINE,
                p.s("}"),
            ])
        })
    }

    /// Without semicolons, a class property followed by a member that
    /// could be read as continuing it needs a `;`.
    fn should_print_semicolon_after_class_property(
        &self,
        member: &'a class::BodyElement<Loc, Loc>,
        next: Option<&'a class::BodyElement<Loc, Loc>>,
    ) -> bool {
        let (value_missing, key) = match member {
            class::BodyElement::Property(property) => (
                !matches!(property.value, class::property::Value::Initialized(_)),
                Some(&property.key),
            ),
            class::BodyElement::PrivateField(field) => (
                !matches!(field.value, class::property::Value::Initialized(_)),
                None,
            ),
            _ => (false, None),
        };
        let keyword_like = key.is_some_and(|key| {
            matches!(key_name(key), Some("static" | "get" | "set"))
                && !matches!(member, class::BodyElement::Property(property) if matches!(property.annot, types::AnnotationOrHint::Available(_)))
        });
        if value_missing && keyword_like {
            return true;
        }
        let Some(next) = next else {
            return false;
        };
        let next_static = match next {
            class::BodyElement::Method(method) => method.static_,
            class::BodyElement::Property(property) => property.static_,
            class::BodyElement::PrivateField(field) => field.static_,
            class::BodyElement::StaticBlock(_) => true,
            class::BodyElement::DeclareMethod(method) => method.static_,
            class::BodyElement::IndexSignature(indexer) => indexer.static_,
            _ => false,
        };
        if next_static {
            return false;
        }
        let next_key = member_key(next);
        if let Some(next_key) = next_key
            && !matches!(next_key, expression::object::Key::Computed(_))
            && matches!(key_name(next_key), Some("in" | "instanceof"))
        {
            return true;
        }
        match next {
            class::BodyElement::Property(property) => {
                (property.variance.is_some()
                    && !matches!(property.value, class::property::Value::Declared))
                    || matches!(property.key, expression::object::Key::Computed(_))
            }
            class::BodyElement::Method(method) => {
                if method.value.1.async_
                    || matches!(method.kind, class::MethodKind::Get | class::MethodKind::Set)
                {
                    return false;
                }
                matches!(method.key, expression::object::Key::Computed(_))
                    || method.value.1.generator
            }
            class::BodyElement::IndexSignature(_) => true,
            _ => false,
        }
    }

    /// One class member, with its comments.
    fn print_class_member(&mut self, member: &'a class::BodyElement<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::ClassMember(member), |p| match member {
            class::BodyElement::Method(method) => p.print_class_method(method),
            class::BodyElement::Property(property) => p.print_class_property(property),
            class::BodyElement::PrivateField(field) => p.print_private_field(field),
            class::BodyElement::StaticBlock(block) => {
                let key = NodeRef::ClassMember(member).key();
                let body = p.print_block_body(&block.body, key);
                p.concat([p.s("static "), body])
            }
            class::BodyElement::DeclareMethod(method) => {
                let mut parts = Vec::new();
                if method.static_ {
                    parts.push(p.s("static "));
                }
                match method.kind {
                    class::MethodKind::Get => parts.push(p.s("get ")),
                    class::MethodKind::Set => parts.push(p.s("set ")),
                    _ => {}
                }
                parts.push(p.print_property_key(&method.key));
                if method.optional {
                    parts.push(p.s("?"));
                }
                let annotation =
                    p.print_node(NodeRef::Annotation(&method.annot), |p| {
                        match &*method.annot.annotation {
                            types::TypeInner::Function { inner, .. } => {
                                p.print_node(NodeRef::Type(&method.annot.annotation), |p| {
                                    p.print_function_type(
                                        inner,
                                        super::types::FunctionTypeStyle::Method,
                                    )
                                })
                            }
                            _ => {
                                let ty = p.print_type(&method.annot.annotation);
                                p.concat([p.s(": "), ty])
                            }
                        }
                    });
                parts.push(annotation);
                parts.push(p.semi());
                p.docs.concat_vec(parts)
            }
            class::BodyElement::AbstractMethod(method) => {
                let key = p.print_property_key(&method.key);
                let signature = p.print_node(
                    NodeRef::FunctionType(&method.annot.0, &method.annot.1),
                    |p| {
                        p.print_function_type(
                            &method.annot.1,
                            super::types::FunctionTypeStyle::Method,
                        )
                    },
                );
                p.concat([p.s("abstract "), key, signature, p.semi()])
            }
            class::BodyElement::AbstractProperty(property) => {
                let variance = match &property.variance {
                    Some(variance) => p.print_variance(variance),
                    None => p.s(""),
                };
                let key = p.print_property_key(&property.key);
                let annotation = p.print_optional_annotation(&property.annot);
                p.concat([p.s("abstract "), variance, key, annotation, p.semi()])
            }
            class::BodyElement::IndexSignature(indexer) => {
                let printed = p.print_indexer(indexer);
                p.concat([printed, p.semi()])
            }
        })
    }

    /// Decorators on a member, on their own lines when the source had them
    /// there.
    fn print_member_decorators(&mut self, decorators: &'a [class::Decorator<Loc, Loc>]) -> Doc<'a> {
        if decorators.is_empty() {
            return self.s("");
        }
        let on_own_lines = decorators.iter().any(|decorator| {
            self.text
                .has_newline(self.text.span(&decorator.loc).end, false)
        });
        let printed: Vec<Doc<'a>> = decorators
            .iter()
            .map(|decorator| self.print_decorator(decorator))
            .collect();
        self.group(self.concat([
            self.join(&LINE, printed),
            if on_own_lines { &HARDLINE } else { &LINE },
        ]))
    }

    /// `@expression`.
    pub fn print_decorator(&mut self, decorator: &'a class::Decorator<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::Decorator(decorator), |p| {
            let expression = p.print_expression(&decorator.expression);
            p.concat([p.s("@"), expression])
        })
    }

    fn print_class_method(&mut self, method: &'a class::Method<Loc, Loc>) -> Doc<'a> {
        let mut parts = vec![self.print_member_decorators(&method.decorators)];
        if method.static_ {
            parts.push(self.s("static "));
        }
        if method.override_ {
            parts.push(self.s("override "));
        }
        let (loc, function) = &method.value;
        parts.push(self.print_method(
            &method.key,
            &method.kind,
            function,
            NodeRef::FunctionValue(loc, function),
        ));
        self.docs.concat_vec(parts)
    }

    /// `async *name(params) {}` / `get name() {}`: Prettier's
    /// `printMethod`, for classes and object literals alike.
    pub fn print_method(
        &mut self,
        key: &'a expression::object::Key<Loc, Loc>,
        kind: &'a class::MethodKind,
        function: &'a function::Function<Loc, Loc>,
        value_node: NodeRef<'a>,
    ) -> Doc<'a> {
        let mut parts = Vec::new();
        match kind {
            class::MethodKind::Get => parts.push(self.s("get ")),
            class::MethodKind::Set => parts.push(self.s("set ")),
            class::MethodKind::Method | class::MethodKind::Constructor => {
                if function.async_ {
                    parts.push(self.s("async "));
                }
            }
        }
        if function.generator {
            parts.push(self.s("*"));
        }
        parts.push(self.print_property_key(key));
        let value = self.print_node(value_node, |p| {
            p.print_method_value(function, value_node.key())
        });
        parts.push(value);
        self.docs.concat_vec(parts)
    }

    /// `static +name?: T = value;`
    pub fn print_class_property(&mut self, property: &'a class::Property<Loc, Loc>) -> Doc<'a> {
        let mut parts = vec![self.print_member_decorators(&property.decorators)];
        if matches!(property.value, class::property::Value::Declared) {
            parts.push(self.s("declare "));
        }
        if property.static_ {
            parts.push(self.s("static "));
        }
        if property.override_ {
            parts.push(self.s("override "));
        }
        if let Some(variance) = &property.variance {
            parts.push(self.print_variance(variance));
        }
        parts.push(self.print_property_key(&property.key));
        if property.optional {
            parts.push(self.s("?"));
        }
        parts.push(self.print_optional_annotation(&property.annot));
        let left = self.docs.concat_vec(parts);
        let Some(node) = self.current() else {
            return left;
        };
        let rhs = match &property.value {
            class::property::Value::Initialized(value) => Some(Rhs::Expression(value)),
            _ => None,
        };
        let assignment = self.print_assignment_like(node, left, " =", rhs);
        self.concat([assignment, self.semi()])
    }

    fn print_private_field(&mut self, field: &'a class::PrivateField<Loc, Loc>) -> Doc<'a> {
        let mut parts = vec![self.print_member_decorators(&field.decorators)];
        if matches!(field.value, class::property::Value::Declared) {
            parts.push(self.s("declare "));
        }
        if field.static_ {
            parts.push(self.s("static "));
        }
        if field.override_ {
            parts.push(self.s("override "));
        }
        if let Some(variance) = &field.variance {
            parts.push(self.print_variance(variance));
        }
        parts.push(self.print_private_name(&field.key));
        if field.optional {
            parts.push(self.s("?"));
        }
        parts.push(self.print_optional_annotation(&field.annot));
        let left = self.docs.concat_vec(parts);
        let Some(node) = self.current() else {
            return left;
        };
        let rhs = match &field.value {
            class::property::Value::Initialized(value) => Some(Rhs::Expression(value)),
            _ => None,
        };
        let assignment = self.print_assignment_like(node, left, " =", rhs);
        self.concat([assignment, self.semi()])
    }

    /// `+`, `-`, `readonly `, `in `, `out `.
    pub fn print_variance(&mut self, variance: &'a uf_flow::ast::Variance<Loc>) -> Doc<'a> {
        self.print_node(NodeRef::Variance(variance), |p| {
            p.s(match variance.kind {
                uf_flow::ast::VarianceKind::Plus => "+",
                uf_flow::ast::VarianceKind::Minus => "-",
                uf_flow::ast::VarianceKind::Readonly => "readonly ",
                uf_flow::ast::VarianceKind::Writeonly => "writeonly ",
                uf_flow::ast::VarianceKind::In => "in ",
                uf_flow::ast::VarianceKind::Out => "out ",
                uf_flow::ast::VarianceKind::InOut => "in out ",
            })
        })
    }

    /// `declare class Name<T> extends B mixins M implements I { … }`.
    pub fn print_declare_class(
        &mut self,
        class: &'a statement::DeclareClass<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let mut parts = vec![self.s("declare class ")];
        parts.push(self.print_identifier(&class.id));
        parts.push(self.print_optional_type_params(class.tparams.as_ref()));
        let mut heritage: Vec<Doc<'a>> = Vec::new();
        if let Some((_, extends)) = &class.extends {
            let printed = self.print_declare_class_extends(extends);
            heritage.push(self.concat([self.s("extends "), printed]));
        }
        if !class.mixins.is_empty() {
            let printed: Vec<Doc<'a>> = class
                .mixins
                .iter()
                .map(|mixin| self.print_interface_extends(mixin))
                .collect();
            let dangling = self.print_dangling_comments(key, Marker::Mixins, false);
            let separator = self.concat([self.s(","), &LINE]);
            heritage.push(self.concat([
                self.s("mixins "),
                dangling.unwrap_or(self.s("")),
                self.join(separator, printed),
            ]));
        }
        if let Some(implements) = &class.implements
            && !implements.interfaces.is_empty()
        {
            let printed: Vec<Doc<'a>> = implements
                .interfaces
                .iter()
                .map(|interface| {
                    self.print_node(NodeRef::ClassImplements(interface), |p| {
                        let id = p.print_generic_identifier(&interface.id);
                        let targs = match &interface.targs {
                            Some(targs) => p.print_type_args(targs),
                            None => p.s(""),
                        };
                        p.concat([id, targs])
                    })
                })
                .collect();
            let dangling = self.print_dangling_comments(key, Marker::Implements, false);
            let separator = self.concat([self.s(","), &LINE]);
            heritage.push(self.concat([
                self.s("implements "),
                dangling.unwrap_or(self.s("")),
                self.join(separator, printed),
            ]));
        }
        // More than one clause needs the group to lay them out. So does a
        // trailing comment on the class name, for a different reason: a line
        // comment ends the line it is on, and without a group there is no
        // line before `extends` for it to end. See ubugeeei-prod/uf#135.
        let group_mode = heritage.len() > 1
            || self.has_comment_placed(NodeRef::Identifier(&class.id).key(), Placement::Trailing);
        if group_mode {
            let clauses: Vec<Doc<'a>> = heritage
                .into_iter()
                .map(|clause| self.concat([&LINE, self.group(clause)]))
                .collect();
            let id = self.docs.group_id();
            // The head goes inside the group, as Prettier has it. A trailing
            // line comment on the class name breaks the group it is *in*, and
            // the line it has to end is the one before `extends` — which is
            // in this group. Left outside, the break propagated past it to
            // the statement and the heritage stayed on one line.
            let head = self.docs.concat_vec(std::mem::take(&mut parts));
            parts.push(self.docs.group_with(
                self.concat([head, self.indent(self.docs.concat_vec(clauses))]),
                false,
                Some(id),
            ));
            if class.body.1.properties.is_empty() {
                parts.push(self.s(" "));
            } else {
                parts.push(self.docs.if_break(&HARDLINE, self.s(" "), Some(id)));
            }
        } else {
            for clause in heritage {
                parts.push(self.s(" "));
                parts.push(clause);
            }
            parts.push(self.s(" "));
        }
        parts.push(self.print_object_type(
            &class.body.1,
            NodeRef::ObjectType(&class.body.0, &class.body.1),
            true,
        ));
        self.docs.concat_vec(parts)
    }

    fn print_declare_class_extends(
        &mut self,
        extends: &'a statement::DeclareClassExtends<Loc, Loc>,
    ) -> Doc<'a> {
        match extends {
            statement::DeclareClassExtends::ExtendsIdent(generic) => self.print_generic(generic),
            statement::DeclareClassExtends::ExtendsCall { callee, arg } => {
                let callee = self.print_generic(&callee.1);
                let argument = self.print_declare_class_extends(&arg.1);
                self.concat([callee, self.s("("), argument, self.s(")")])
            }
        }
    }
}
