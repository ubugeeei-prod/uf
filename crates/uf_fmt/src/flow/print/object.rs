//! Object literals, object patterns and object types: Prettier's
//! `printObject`.
//!
//! One rule here is the one people notice most: an object whose first
//! property started on a new line in the source stays expanded, however
//! short it is. Prettier calls it `objectWrap: "preserve"`, and it is the
//! only place the source's own line breaks decide a layout.

use uf_flow::Loc;
use uf_flow::ast::{class, expression, pattern, types};

use super::Printer;
use super::assignment::Rhs;
use super::literal::{is_identifier_name, print_string};
use crate::doc::{Doc, HARDLINE, LINE, SOFTLINE};
use crate::flow::comments::{Marker, Placement};
use crate::flow::node::{Expression, NodeKey, NodeRef};

/// What kind of object is being printed, since the three share a layout.
#[derive(Clone, Copy)]
pub enum ObjectKind<'a> {
    /// An object literal.
    Expression(&'a expression::Object<Loc, Loc>),
    /// A destructuring pattern.
    Pattern(&'a pattern::Object<Loc, Loc>),
}

impl<'a> Printer<'a> {
    /// `{ a: 1, b }`.
    pub fn print_object(
        &mut self,
        object: &'a expression::Object<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        let start = self.text.span(expression.loc()).start;
        self.print_object_like(ObjectKind::Expression(object), key, start)
    }

    /// `{ a, b: c = 1, ...rest }: T`.
    pub fn print_object_pattern(
        &mut self,
        object: &'a pattern::Object<Loc, Loc>,
        pattern: &'a pattern::Pattern<Loc, Loc>,
    ) -> Doc<'a> {
        let key = NodeRef::Pattern(pattern).key();
        let start = self.text.span(pattern.loc()).start;
        let printed = self.print_object_like(ObjectKind::Pattern(object), key, start);
        let optional = if object.optional {
            self.s("?")
        } else {
            self.s("")
        };
        let annotation = self.print_optional_annotation(&object.annot);
        self.concat([printed, optional, annotation])
    }

    fn print_object_like(&mut self, kind: ObjectKind<'a>, key: NodeKey, start: usize) -> Doc<'a> {
        let members: Vec<(NodeRef<'a>, usize)> = match kind {
            ObjectKind::Expression(object) => object
                .properties
                .iter()
                .map(|property| {
                    let node = NodeRef::ObjectProperty(property);
                    (node, self.text.span(&node.loc()).end)
                })
                .collect(),
            ObjectKind::Pattern(object) => object
                .properties
                .iter()
                .map(|property| {
                    let node = NodeRef::PatternProperty(property);
                    (node, self.text.span(&node.loc()).end)
                })
                .collect(),
        };

        let parent = self.parent();
        let should_break = match kind {
            ObjectKind::Pattern(object) => {
                let parent_is_function_like = matches!(
                    parent,
                    Some(NodeRef::Param(_))
                        | Some(NodeRef::CatchClause(_))
                        | Some(NodeRef::RestParam(_))
                        | Some(NodeRef::PatternElement(_))
                        | Some(NodeRef::ComponentParam(_))
                        | Some(NodeRef::ComponentRest(_))
                ) || matches!(parent, Some(NodeRef::PatternProperty(pattern::object::Property::NormalProperty(property))) if property.default.is_some());
                !parent_is_function_like
                    && object.properties.iter().any(|property| match property {
                        pattern::object::Property::NormalProperty(property) => {
                            property.default.is_none()
                                && matches!(
                                    property.pattern,
                                    pattern::Pattern::Object { .. }
                                        | pattern::Pattern::Array { .. }
                                )
                        }
                        pattern::object::Property::RestElement(_) => false,
                    })
            }
            ObjectKind::Expression(_) => members.first().is_some_and(|(first, _)| {
                self.text
                    .has_newline_in_range(start, self.text.span(&first.loc()).start)
            }),
        };

        let mut printed: Vec<Doc<'a>> = Vec::with_capacity(members.len() * 3);
        for (index, (member, end)) in members.iter().enumerate() {
            let doc = match (kind, member) {
                (ObjectKind::Expression(_), NodeRef::ObjectProperty(property)) => {
                    self.print_object_property(property)
                }
                (ObjectKind::Pattern(_), NodeRef::PatternProperty(property)) => {
                    self.print_pattern_property(property)
                }
                _ => self.s(""),
            };
            if index > 0 {
                printed.push(self.s(","));
                printed.push(&LINE);
                let previous_end = members[index - 1].1;
                if self.text.is_next_line_empty(previous_end) {
                    printed.push(&HARDLINE);
                }
            }
            printed.push(doc);
            let _ = end;
        }

        let last_is_rest = match kind {
            ObjectKind::Pattern(object) => matches!(
                object.properties.last(),
                Some(pattern::object::Property::RestElement(_))
            ),
            ObjectKind::Expression(_) => false,
        };

        let content = if printed.is_empty() {
            // An empty object with a comment inside always breaks, so the
            // comment keeps its own line.
            match self.print_dangling_comments(key, Marker::None, false) {
                Some(dangling) => self.concat([
                    self.s("{"),
                    self.indent(self.concat([&HARDLINE, dangling])),
                    &HARDLINE,
                    self.s("}"),
                ]),
                None => self.s("{}"),
            }
        } else {
            let inner = self.docs.concat_vec(printed);
            let trailing_comma = if last_is_rest {
                self.s("")
            } else {
                self.if_break(self.s(","), self.s(""))
            };
            self.concat([
                self.s("{"),
                self.indent(self.concat([&LINE, inner])),
                trailing_comma,
                &LINE,
                self.s("}"),
            ])
        };

        // If we inline the object as the only parameter of a function, we
        // don't want to create another group so that the object breaks
        // before the parent breaks.
        let hugged = matches!(kind, ObjectKind::Pattern(_)) && self.is_hugged_only_parameter();
        let in_assignment_left = matches!(kind, ObjectKind::Pattern(_))
            && !should_break
            && matches!(parent, Some(NodeRef::Declarator(_)))
            || matches!(parent, Some(NodeRef::Expression(parent)) if matches!(**parent, expression::ExpressionInner::Assignment { .. }) && matches!(kind, ObjectKind::Pattern(_)) && !should_break);
        if hugged || in_assignment_left {
            return content;
        }
        self.docs.group_with(content, should_break, None)
    }

    /// `key: value`, `key`, `method() {}`, `get key() {}`, `...spread`.
    fn print_object_property(
        &mut self,
        property: &'a expression::object::Property<Loc, Loc>,
    ) -> Doc<'a> {
        let node = NodeRef::ObjectProperty(property);
        self.print_node(node, |p| match property {
            expression::object::Property::NormalProperty(normal) => match normal {
                expression::object::NormalProperty::Init {
                    key,
                    value,
                    shorthand,
                    ..
                } => {
                    if *shorthand {
                        return p.print_expression(value);
                    }
                    let printed_key = p.print_property_key(key);
                    p.print_assignment_like(node, printed_key, ":", Some(Rhs::Expression(value)))
                }
                expression::object::NormalProperty::Method { key, value, .. } => p.print_method(
                    key,
                    &class::MethodKind::Method,
                    &value.1,
                    NodeRef::FunctionValue(&value.0, &value.1),
                ),
                expression::object::NormalProperty::Get { key, value, .. } => p.print_method(
                    key,
                    &class::MethodKind::Get,
                    &value.1,
                    NodeRef::FunctionValue(&value.0, &value.1),
                ),
                expression::object::NormalProperty::Set { key, value, .. } => p.print_method(
                    key,
                    &class::MethodKind::Set,
                    &value.1,
                    NodeRef::FunctionValue(&value.0, &value.1),
                ),
            },
            expression::object::Property::SpreadProperty(spread) => {
                let argument = p.print_expression(&spread.argument);
                p.concat([p.s("..."), argument])
            }
        })
    }

    /// `key: pattern = default`, `key`, `...rest` in an object pattern.
    fn print_pattern_property(
        &mut self,
        property: &'a pattern::object::Property<Loc, Loc>,
    ) -> Doc<'a> {
        let node = NodeRef::PatternProperty(property);
        self.print_node(node, |p| match property {
            pattern::object::Property::NormalProperty(normal) => {
                let value = p.print_pattern(&normal.pattern);
                let value = match &normal.default {
                    Some(default) => {
                        let printed = p.print_expression(default);
                        p.concat([value, p.s(" = "), printed])
                    }
                    None => value,
                };
                if normal.shorthand {
                    return value;
                }
                let key = p.print_pattern_key(&normal.key);
                p.concat([key, p.s(": "), value])
            }
            pattern::object::Property::RestElement(rest) => {
                let argument = p.print_pattern(&rest.argument);
                p.concat([p.s("..."), argument])
            }
        })
    }

    /// A property key: quoted only when it has to be.
    pub fn print_property_key(&mut self, key: &'a expression::object::Key<Loc, Loc>) -> Doc<'a> {
        use expression::object::Key;
        match key {
            Key::Identifier(id) => self.print_identifier(id),
            Key::PrivateName(name) => self.print_private_name(name),
            Key::StringLiteral((loc, literal)) => self
                .print_node(NodeRef::StringLiteral(loc, literal), |p| {
                    p.print_string_key(literal)
                }),
            Key::NumberLiteral((loc, literal)) => self
                .print_node(NodeRef::NumberLiteral(loc, literal), |p| {
                    p.print_number_literal(literal)
                }),
            Key::BigIntLiteral((loc, literal)) => self
                .print_node(NodeRef::BigIntLiteral(loc, literal), |p| {
                    p.print_bigint_literal(literal)
                }),
            Key::Computed(computed) => {
                let expression = self.print_expression(&computed.expression);
                self.concat([self.s("["), expression, self.s("]")])
            }
        }
    }

    /// A key in an object pattern.
    pub fn print_pattern_key(&mut self, key: &'a pattern::object::Key<Loc, Loc>) -> Doc<'a> {
        use pattern::object::Key;
        match key {
            Key::Identifier(id) => self.print_identifier(id),
            Key::StringLiteral((loc, literal)) => self
                .print_node(NodeRef::StringLiteral(loc, literal), |p| {
                    p.print_string_key(literal)
                }),
            Key::NumberLiteral((loc, literal)) => self
                .print_node(NodeRef::NumberLiteral(loc, literal), |p| {
                    p.print_number_literal(literal)
                }),
            Key::BigIntLiteral((loc, literal)) => self
                .print_node(NodeRef::BigIntLiteral(loc, literal), |p| {
                    p.print_bigint_literal(literal)
                }),
            Key::Computed(computed) => {
                let expression = self.print_expression(&computed.expression);
                self.concat([self.s("["), expression, self.s("]")])
            }
        }
    }

    /// A quoted key, unquoted when it is a plain identifier whose spelling
    /// the quotes did not change.
    pub fn print_string_key(&self, literal: &'a uf_flow::ast::StringLiteral<Loc>) -> Doc<'a> {
        let printed = print_string(&literal.raw, self.options.quote);
        let unchanged = printed.len() >= 2 && printed[1..printed.len() - 1] == *literal.value;
        if unchanged && is_identifier_name(&literal.value) {
            return self.docs.borrowed(&literal.value);
        }
        self.text(&printed)
    }

    /// `: T` after a pattern, or nothing.
    pub fn print_optional_annotation(
        &mut self,
        annotation: &'a types::AnnotationOrHint<Loc, Loc>,
    ) -> Doc<'a> {
        match annotation {
            types::AnnotationOrHint::Available(annotation) => {
                self.print_type_annotation(annotation)
            }
            types::AnnotationOrHint::Missing(_) => self.s(""),
        }
    }

    /// An object type `{ a: T, ... }`, or the body of an interface or
    /// declared class when `is_interface_body`, whose members sit one per
    /// line ended with `;`.
    pub fn print_object_type(
        &mut self,
        object: &'a types::Object<Loc, Loc>,
        node: NodeRef<'a>,
        is_interface_body: bool,
    ) -> Doc<'a> {
        let key = node.key();
        let (open, close) = if object.exact {
            ("{|", "|}")
        } else {
            ("{", "}")
        };
        let separator: Doc<'a> = if is_interface_body { &HARDLINE } else { &LINE };
        let count = object.properties.len();
        let mut parts: Vec<Doc<'a>> = Vec::with_capacity(count * 3);
        let mut first_start: Option<usize> = None;
        for (index, member) in object.properties.iter().enumerate() {
            let member_node = NodeRef::ObjectTypeProperty(member);
            let span = self.text.span(&member_node.loc());
            first_start.get_or_insert(span.start);
            let printed = self.print_node(member_node, |p| p.print_object_type_member(member));
            parts.push(printed);
            let is_last = index + 1 == count;
            if !is_interface_body {
                if object.inexact || !is_last {
                    parts.push(self.s(","));
                } else {
                    parts.push(self.if_break(self.s(","), self.s("")));
                }
            } else {
                parts.push(self.s(";"));
            }
            if !is_last {
                parts.push(separator);
                if self.text.is_next_line_empty(span.end) {
                    parts.push(&HARDLINE);
                }
            }
        }
        let has_dangling = self.has_comment_placed(key, Placement::Dangling);
        if let Some(dangling) = self.print_dangling_comments(key, Marker::None, false) {
            if !parts.is_empty() {
                parts.push(separator);
            }
            parts.push(dangling);
        }
        if object.inexact {
            if has_dangling {
                let line_comment = self.has_line_comment(key, Some(Placement::Dangling));
                parts.push(if line_comment { &HARDLINE } else { &LINE });
            } else if first_start.is_some() {
                parts.push(&LINE);
            }
            parts.push(self.s("..."));
        }

        if is_interface_body {
            if parts.is_empty() {
                return self.concat([self.s(open), self.s(close)]);
            }
            let inner = self.docs.concat_vec(parts);
            return self.concat([
                self.s(open),
                self.indent(self.concat([&HARDLINE, inner])),
                &HARDLINE,
                self.s(close),
            ]);
        }

        let should_break = self.has_line_comment(key, Some(Placement::Dangling))
            || first_start.is_some_and(|first| {
                self.text
                    .has_newline_in_range(self.text.span(&node.loc()).start, first)
            });
        let content = if parts.is_empty() {
            self.concat([self.s(open), self.s(close)])
        } else {
            let no_members = first_start.is_none() && !object.inexact;
            let bracket_space: Doc<'a> = if no_members && !should_break {
                &SOFTLINE
            } else {
                &LINE
            };
            let inner = self.docs.concat_vec(parts);
            self.concat([
                self.s(open),
                self.indent(self.concat([bracket_space, inner])),
                bracket_space,
                self.s(close),
            ])
        };
        if self.is_annotation_of_hugged_parameter() || self.is_function_type_param_of_hugged() {
            return content;
        }
        self.docs.group_with(content, should_break, None)
    }

    /// Whether the type on top of the stack annotates a function type
    /// parameter whose function hugs it.
    fn is_function_type_param_of_hugged(&self) -> bool {
        let ancestors: Vec<NodeRef<'a>> = self.ancestors.iter().rev().copied().collect();
        let (Some(NodeRef::FunctionTypeParam(param)), Some(function_type)) =
            (ancestors.get(1), ancestors.get(2))
        else {
            return false;
        };
        let function = match function_type {
            NodeRef::Type(ty) => match &***ty {
                types::TypeInner::Function { inner, .. } => &**inner,
                _ => return false,
            },
            NodeRef::FunctionType(_, function) => *function,
            _ => return false,
        };
        function.params.params.len() == 1
            && function.params.rest.is_none()
            && function.params.this.is_none()
            && std::ptr::eq(&function.params.params[0], *param)
            && matches!(&param.param, types::function::ParamKind::Labeled { annot, .. } if super::function::is_object_type(annot))
    }

    /// One member of an object type.
    fn print_object_type_member(
        &mut self,
        member: &'a types::object::Property<Loc, Loc>,
    ) -> Doc<'a> {
        use types::object::Property;
        match member {
            Property::NormalProperty(property) => self.print_object_type_property(property),
            Property::SpreadProperty(spread) => {
                let argument = self.print_type(&spread.argument);
                self.concat([self.s("..."), argument])
            }
            Property::Indexer(indexer) => self.print_indexer(indexer),
            Property::CallProperty(call) => {
                let value =
                    self.print_node(NodeRef::FunctionType(&call.value.0, &call.value.1), |p| {
                        p.print_function_type(
                            &call.value.1,
                            super::types::FunctionTypeStyle::Method,
                        )
                    });
                self.concat([
                    if call.static_ {
                        self.s("static ")
                    } else {
                        self.s("")
                    },
                    value,
                ])
            }
            Property::InternalSlot(slot) => {
                let id = self.print_identifier(&slot.id);
                let value = if slot.method {
                    match &*slot.value {
                        types::TypeInner::Function { inner, .. } => {
                            self.print_node(NodeRef::Type(&slot.value), |p| {
                                p.print_function_type(
                                    inner,
                                    super::types::FunctionTypeStyle::Method,
                                )
                            })
                        }
                        _ => {
                            let ty = self.print_type(&slot.value);
                            self.concat([self.s(": "), ty])
                        }
                    }
                } else {
                    let ty = self.print_type(&slot.value);
                    self.concat([self.s(": "), ty])
                };
                self.concat([
                    if slot.static_ {
                        self.s("static ")
                    } else {
                        self.s("")
                    },
                    self.s("[["),
                    id,
                    self.s("]]"),
                    if slot.optional {
                        self.s("?")
                    } else {
                        self.s("")
                    },
                    value,
                ])
            }
            Property::MappedType(mapped) => self.print_mapped_type(mapped),
            Property::PrivateField(field) => self.print_private_name(&field.key),
        }
    }

    fn print_object_type_property(
        &mut self,
        property: &'a types::object::NormalProperty<Loc, Loc>,
    ) -> Doc<'a> {
        let mut parts = Vec::new();
        if property.static_ {
            parts.push(self.s("static "));
        }
        if property.proto {
            parts.push(self.s("proto "));
        }
        if let Some(variance) = &property.variance {
            parts.push(self.print_variance(variance));
        }
        match &property.value {
            types::object::PropertyValue::Get(loc, function) => {
                parts.push(self.s("get "));
                parts.push(self.print_property_key(&property.key));
                let printed = self.print_node(NodeRef::FunctionType(loc, function), |p| {
                    p.print_function_type(function, super::types::FunctionTypeStyle::Method)
                });
                parts.push(printed);
            }
            types::object::PropertyValue::Set(loc, function) => {
                parts.push(self.s("set "));
                parts.push(self.print_property_key(&property.key));
                let printed = self.print_node(NodeRef::FunctionType(loc, function), |p| {
                    p.print_function_type(function, super::types::FunctionTypeStyle::Method)
                });
                parts.push(printed);
            }
            types::object::PropertyValue::Init(value) => {
                let key = self.print_property_key(&property.key);
                parts.push(key);
                if property.optional {
                    parts.push(self.s("?"));
                }
                match value {
                    Some(ty) if property.method => {
                        let printed = match &**ty {
                            types::TypeInner::Function { inner, .. } => {
                                self.print_node(NodeRef::Type(ty), |p| {
                                    p.print_function_type(
                                        inner,
                                        super::types::FunctionTypeStyle::Method,
                                    )
                                })
                            }
                            _ => {
                                let printed = self.print_type(ty);
                                self.concat([self.s(": "), printed])
                            }
                        };
                        parts.push(printed);
                    }
                    Some(ty) => {
                        let left = self.docs.concat_vec(std::mem::take(&mut parts));
                        let Some(node) = self.current() else {
                            let printed = self.print_type(ty);
                            return self.concat([left, self.s(": "), printed]);
                        };
                        return self.print_assignment_like(node, left, ":", Some(Rhs::Type(ty)));
                    }
                    None => {}
                }
            }
        }
        self.docs.concat_vec(parts)
    }

    /// `[key: K]: V` / `[name: K]: V`.
    pub fn print_indexer(&mut self, indexer: &'a types::object::Indexer<Loc, Loc>) -> Doc<'a> {
        let mut parts = Vec::new();
        if indexer.static_ {
            parts.push(self.s("static "));
        }
        if let Some(variance) = &indexer.variance {
            parts.push(self.print_variance(variance));
        }
        parts.push(self.s("["));
        if let Some(id) = &indexer.id {
            parts.push(self.print_identifier(id));
            parts.push(self.s(": "));
        }
        parts.push(self.print_type(&indexer.key));
        parts.push(self.s("]"));
        if indexer.optional {
            parts.push(self.s("?"));
        }
        parts.push(self.s(": "));
        parts.push(self.print_type(&indexer.value));
        self.docs.concat_vec(parts)
    }

    /// `+[K in keyof O]?: T`.
    fn print_mapped_type(&mut self, mapped: &'a types::object::MappedType<Loc, Loc>) -> Doc<'a> {
        let mut parts = Vec::new();
        if let Some(variance) = &mapped.variance {
            parts.push(self.print_variance(variance));
        }
        parts.push(self.s("["));
        parts.push(self.print_identifier(&mapped.key_tparam.name));
        parts.push(self.s(" in "));
        parts.push(self.print_type(&mapped.source_type));
        if let Some(name) = &mapped.name_type {
            parts.push(self.s(" as "));
            parts.push(self.print_type(name));
        }
        parts.push(self.s("]"));
        parts.push(match mapped.optional {
            types::object::MappedTypeOptionalFlag::PlusOptional => self.s("+?"),
            types::object::MappedTypeOptionalFlag::MinusOptional => self.s("-?"),
            types::object::MappedTypeOptionalFlag::Optional => self.s("?"),
            types::object::MappedTypeOptionalFlag::NoOptionalFlag => self.s(""),
        });
        parts.push(self.s(": "));
        parts.push(self.print_type(&mapped.prop_type));
        self.docs.concat_vec(parts)
    }
}
