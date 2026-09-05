//! Flow types: every `TypeInner` variant, type parameters and arguments,
//! type aliases, opaque types, interfaces, and type guards.
//!
//! The layout rules that matter: a union that breaks puts `| ` at the
//! start of every line and indents under the `=`, an intersection of
//! objects hugs its `&`, a function type prints as `(a: T) => R` except
//! where the grammar reads it as a method (`m(): R`), and a single
//! object-type argument hugs its angle brackets.

use uf_flow::Loc;
use uf_flow::ast::{expression, function, statement, types};

use super::Printer;
use super::assignment::Rhs;
use super::call::is_test_call;
use super::function::is_object_type;
use crate::doc::{Doc, LINE, LINE_SUFFIX_BOUNDARY, SOFTLINE};
use crate::flow::comments::Placement;
use crate::flow::node::{NodeKey, NodeRef, Type};

/// How a function type's return is introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionTypeStyle {
    /// `(a: T) => R`.
    Arrow,
    /// `(a: T): R` as a method or call property of an object type.
    Method,
    /// `(a: T): R` after `declare function f`.
    Declaration,
}

fn is_union(ty: &Type) -> bool {
    matches!(**ty, types::TypeInner::Union { .. })
}

fn is_intersection(ty: &Type) -> bool {
    matches!(**ty, types::TypeInner::Intersection { .. })
}

/// The members of a union or intersection, flattened out of the port's
/// `(first, second, rest)` triple.
pub fn members_of(ty: &Type) -> Vec<&Type> {
    match &**ty {
        types::TypeInner::Union { inner, .. } => {
            let mut members = vec![&inner.types.0, &inner.types.1];
            members.extend(inner.types.2.iter());
            members
        }
        types::TypeInner::Intersection { inner, .. } => {
            let mut members = vec![&inner.types.0, &inner.types.1];
            members.extend(inner.types.2.iter());
            members
        }
        _ => Vec::new(),
    }
}

fn same_type(a: &Type, b: &Type) -> bool {
    std::ptr::eq(&*a.0, &*b.0)
}

impl<'a> Printer<'a> {
    /// Any type, parenthesized where the grammar needs it, with its
    /// comments.
    pub fn print_type(&mut self, ty: &'a Type) -> Doc<'a> {
        let node = NodeRef::Type(ty);
        self.ancestors.push(node);
        let doc = self.print_type_inner(ty);
        let needs_parens = self.type_needs_parens(ty);
        let prints_own_comments = self.type_prints_own_comments(ty);
        self.ancestors.pop();
        let doc = if needs_parens {
            self.concat([self.s("("), doc, self.s(")")])
        } else {
            doc
        };
        if prints_own_comments {
            return doc;
        }
        self.print_comments(node.key(), doc)
    }

    /// Union members print their comments through the union printer, so
    /// the comment lands on the member's line.
    fn type_prints_own_comments(&self, ty: &'a Type) -> bool {
        let _ = ty;
        matches!(self.parent(), Some(NodeRef::Type(parent)) if is_union(parent))
    }

    fn is_in_multi_element_tuple(&self) -> bool {
        match self.parent() {
            Some(NodeRef::Type(parent)) => {
                matches!(&**parent, types::TypeInner::Tuple { inner, .. } if inner.elements.len() > 1)
            }
            Some(NodeRef::TupleElement(_)) => {
                matches!(self.grandparent(), Some(NodeRef::Type(grand))
                if matches!(&**grand, types::TypeInner::Tuple { inner, .. } if inner.elements.len() > 1))
            }
            _ => false,
        }
    }

    fn type_needs_parens(&self, ty: &'a Type) -> bool {
        use types::TypeInner as T;
        let Some(parent) = self.parent() else {
            return false;
        };
        let parent_type = match parent {
            NodeRef::Type(parent) => Some(&**parent),
            _ => None,
        };
        // The role of a nullable's argument is decided by the nullable's
        // own parent.
        let (effective_parent, effective_parent_type) = match parent_type {
            Some(T::Nullable { .. }) => match self.grandparent() {
                Some(grand @ NodeRef::Type(grand_type)) => (Some(grand), Some(&**grand_type)),
                other => (other, None),
            },
            _ => (Some(parent), parent_type),
        };
        let is_object_type_of_indexed = |p: Option<&types::TypeInner<Loc, Loc>>| match p {
            Some(T::IndexedAccess { inner, .. }) => same_type(&inner.object, ty),
            Some(T::OptionalIndexedAccess { inner, .. }) => {
                same_type(&inner.indexed_access.object, ty)
            }
            _ => false,
        };
        let is_check_type = |p: Option<&types::TypeInner<Loc, Loc>>| match p {
            Some(T::Conditional { inner, .. }) => same_type(&inner.check_type, ty),
            _ => false,
        };
        let is_extends_type = |p: Option<&types::TypeInner<Loc, Loc>>| match p {
            Some(T::Conditional { inner, .. }) => same_type(&inner.extends_type, ty),
            _ => false,
        };
        let in_operator = |p: Option<&types::TypeInner<Loc, Loc>>| {
            matches!(
                p,
                Some(T::Keyof { .. }) | Some(T::Renders { .. }) | Some(T::ReadOnly { .. })
            )
        };

        match &**ty {
            T::Function { .. } | T::ConstructorType { .. } | T::Component { .. } => {
                // An arrow's return type that is a function type.
                if let NodeRef::Annotation(_) = parent
                    && let Some(NodeRef::Expression(grand)) = self.grandparent()
                    && let expression::ExpressionInner::ArrowFunction { inner, .. } = &**grand
                    && matches!(&inner.return_, function::ReturnAnnot::Available(annotation) if same_type(&annotation.annotation, ty))
                {
                    return true;
                }
                matches!(
                    effective_parent_type,
                    Some(T::Union { .. }) | Some(T::Intersection { .. }) | Some(T::Array { .. })
                ) || is_object_type_of_indexed(effective_parent_type)
                    || is_check_type(effective_parent_type)
                    || in_operator(effective_parent_type)
                    || (is_extends_type(effective_parent_type)
                        && matches!(&**ty, T::Function { inner, .. }
                            if matches!(&inner.return_, types::function::ReturnAnnotation::Available(annotation)
                                if matches!(&*annotation.annotation, T::Infer { inner, .. }
                                    if matches!(inner.tparam.bound, types::AnnotationOrHint::Available(_))))))
                    || self.is_anonymous_function_type_param_with_nullable(ty, effective_parent)
            }
            T::Nullable { .. } => {
                matches!(parent_type, Some(T::Array { .. }))
                    || is_object_type_of_indexed(parent_type)
            }
            T::Conditional { .. } => {
                if is_extends_type(parent_type) || is_check_type(parent_type) {
                    return true;
                }
                matches!(
                    parent_type,
                    Some(T::Array { .. })
                        | Some(T::Nullable { .. })
                        | Some(T::Intersection { .. })
                        | Some(T::Union { .. })
                ) || is_object_type_of_indexed(parent_type)
                    || in_operator(parent_type)
            }
            T::Union { .. } | T::Intersection { .. } => {
                matches!(
                    parent_type,
                    Some(T::Array { .. })
                        | Some(T::Nullable { .. })
                        | Some(T::Intersection { .. })
                        | Some(T::Union { .. })
                ) || is_object_type_of_indexed(parent_type)
                    || is_check_type(parent_type)
                    || is_extends_type(parent_type)
                    || in_operator(parent_type)
            }
            T::Infer { .. } => {
                matches!(parent_type, Some(T::Array { .. }))
                    || is_object_type_of_indexed(parent_type)
            }
            T::Keyof { .. } | T::Renders { .. } | T::ReadOnly { .. } => {
                matches!(parent_type, Some(T::Array { .. }))
                    || is_object_type_of_indexed(parent_type)
            }
            _ => false,
        }
    }

    /// `type F = (?string) => void` needs `((?string) => void)`? No — but a
    /// function type as the anonymous parameter of another function type
    /// with a nullable parameter does. Prettier's rule, kept verbatim.
    fn is_anonymous_function_type_param_with_nullable(
        &self,
        ty: &'a Type,
        parent: Option<NodeRef<'a>>,
    ) -> bool {
        let Some(NodeRef::FunctionTypeParam(param)) = parent else {
            return false;
        };
        if !matches!(param.param, types::function::ParamKind::Anonymous(_)) {
            return false;
        }
        let types::TypeInner::Function { inner, .. } = &**ty else {
            return false;
        };
        inner.params.params.iter().any(|param| match &param.param {
            types::function::ParamKind::Anonymous(annot)
            | types::function::ParamKind::Labeled { annot, .. } => {
                matches!(**annot, types::TypeInner::Nullable { .. })
            }
            types::function::ParamKind::Destructuring(_) => false,
        })
    }

    fn print_type_inner(&mut self, ty: &'a Type) -> Doc<'a> {
        use types::TypeInner as T;
        let key = NodeRef::Type(ty).key();
        match &**ty {
            T::Any { .. } => self.s("any"),
            T::Mixed { .. } => self.s("mixed"),
            T::Empty { .. } => self.s("empty"),
            T::Void { .. } => self.s("void"),
            T::Null { .. } => self.s("null"),
            T::Number { .. } => self.s("number"),
            T::BigInt { .. } => self.s("bigint"),
            T::String { .. } => self.s("string"),
            T::Boolean { .. } => self.s("boolean"),
            T::Symbol { .. } => self.s("symbol"),
            T::Exists { .. } => self.s("*"),
            T::Unknown { .. } => self.s("unknown"),
            T::Never { .. } => self.s("never"),
            T::Undefined { .. } => self.s("undefined"),
            T::UniqueSymbol { .. } => self.s("unique symbol"),
            T::Nullable { inner, .. } => {
                let argument = self.print_type(&inner.argument);
                self.concat([self.s("?"), argument])
            }
            T::Function { inner, .. } => {
                self.print_function_type(inner, self.function_type_style())
            }
            T::ConstructorType {
                inner, abstract_, ..
            } => {
                let printed = self.print_function_type(inner, FunctionTypeStyle::Arrow);
                self.concat([
                    if *abstract_ {
                        self.s("abstract ")
                    } else {
                        self.s("")
                    },
                    self.s("new "),
                    printed,
                ])
            }
            T::Component { inner, .. } => self.print_component_type(inner, key),
            T::Object { inner, .. } => self.print_object_type(inner, NodeRef::Type(ty), false),
            T::Interface { inner, .. } => {
                // An `interface { … }` type has no name, so the only thing
                // that can make its heritage start a line is having more
                // than one target.
                let extends = self.print_interface_extends_list(&inner.extends, key);
                // An object type, not an interface body. `interface { … }` in
                // a type position separates its members with `,` and stays on
                // one line when it fits; the `;` and the line per member
                // belong to the *declaration*. See ubugeeei-prod/uf#151.
                let body = self.print_object_type(
                    &inner.body.1,
                    NodeRef::ObjectType(&inner.body.0, &inner.body.1),
                    false,
                );
                // An `interface { … }` type has no name to hang a comment
                // on, so heritage is the only thing that can make it group.
                let head = if inner.extends.is_empty() {
                    self.s("interface")
                } else {
                    self.group(self.indent(self.concat([self.s("interface"), extends])))
                };
                self.concat([head, self.s(" "), body])
            }
            T::Array { inner, .. } => {
                let argument = self.print_type(&inner.argument);
                self.concat([argument, self.s("[]")])
            }
            T::Conditional { inner, .. } => self.print_conditional_type(inner, ty),
            T::Infer { inner, .. } => {
                let param = self.print_type_param(&inner.tparam);
                self.concat([self.s("infer "), param])
            }
            T::Generic { inner, .. } => self.print_generic(inner),
            T::IndexedAccess { inner, .. } => {
                let object = self.print_type(&inner.object);
                let index = self.print_type(&inner.index);
                self.concat([object, self.s("["), index, self.s("]")])
            }
            T::OptionalIndexedAccess { inner, .. } => {
                let object = self.print_type(&inner.indexed_access.object);
                let index = self.print_type(&inner.indexed_access.index);
                let optional = if inner.optional {
                    self.s("?.")
                } else {
                    self.s("")
                };
                self.concat([object, optional, self.s("["), index, self.s("]")])
            }
            T::Union { .. } => self.print_union_type(ty),
            T::Intersection { .. } => self.print_intersection_type(ty),
            T::Typeof { inner, .. } => {
                let argument = self.print_typeof_target(&inner.argument);
                let targs = match &inner.targs {
                    Some(targs) => self.print_type_args(targs),
                    None => self.s(""),
                };
                self.concat([self.s("typeof "), argument, targs])
            }
            T::Keyof { inner, .. } => {
                let argument = self.print_type(&inner.argument);
                self.concat([self.s("keyof "), argument])
            }
            T::Renders { inner, .. } => self.print_renders(inner),
            T::ReadOnly { inner, .. } => {
                let argument = self.print_type(&inner.argument);
                self.concat([self.s("readonly "), argument])
            }
            T::Tuple { inner, .. } => self.print_tuple_type(inner, ty),
            T::StringLiteral { literal, .. } => self.print_string_literal(literal),
            T::NumberLiteral { literal, .. } => self.print_number_literal(literal),
            T::BigIntLiteral { literal, .. } => self.print_bigint_literal(literal),
            T::BooleanLiteral { literal, .. } => self.print_boolean_literal(literal),
            T::TemplateLiteral { inner, .. } => {
                let mut parts = vec![self.s("`")];
                for (index, quasi) in inner.quasis.iter().enumerate() {
                    parts.push(self.replace_end_of_line(&quasi.value.raw));
                    if let Some(ty) = inner.types.get(index) {
                        let printed = self.print_type(ty);
                        parts.push(self.concat([self.s("${"), printed, self.s("}")]));
                    }
                }
                parts.push(self.s("`"));
                self.docs.concat_vec(parts)
            }
        }
    }

    /// Whether the function type on top of the stack prints `=> R` or
    /// `: R`: Prettier's `needsArrow` for `FunctionTypeAnnotation`.
    fn function_type_style(&self) -> FunctionTypeStyle {
        match self.parent() {
            Some(NodeRef::ObjectTypeProperty(types::object::Property::NormalProperty(
                property,
            ))) => {
                if property.method && property.variance.is_none() && !property.optional {
                    FunctionTypeStyle::Method
                } else {
                    FunctionTypeStyle::Arrow
                }
            }
            Some(NodeRef::ObjectTypeProperty(types::object::Property::InternalSlot(slot))) => {
                if slot.method && !slot.optional {
                    FunctionTypeStyle::Method
                } else {
                    FunctionTypeStyle::Arrow
                }
            }
            Some(NodeRef::ObjectTypeProperty(types::object::Property::CallProperty(_))) => {
                FunctionTypeStyle::Method
            }
            Some(NodeRef::Annotation(_)) => match self.grandparent() {
                Some(NodeRef::Statement(statement))
                    if matches!(
                        **statement,
                        statement::StatementInner::DeclareFunction { .. }
                    ) =>
                {
                    FunctionTypeStyle::Declaration
                }
                Some(NodeRef::DeclareFunction(..)) => FunctionTypeStyle::Declaration,
                _ => FunctionTypeStyle::Arrow,
            },
            _ => FunctionTypeStyle::Arrow,
        }
    }

    /// `(a: T, ...rest: R) => Ret`, or the method form. The function type
    /// must be on top of the ancestor stack (as a `Type` or
    /// `FunctionType` node).
    pub fn print_function_type(
        &mut self,
        function: &'a types::Function<Loc, Loc>,
        style: FunctionTypeStyle,
    ) -> Doc<'a> {
        let is_hook = matches!(function.effect, function::Effect::Hook);
        let Some(current) = self.current() else {
            return self.s("");
        };
        let key = current.key();
        let type_params = self.print_optional_type_params(function.tparams.as_ref());
        let parameters = self.print_function_type_params(function, key);
        let return_doc = match &function.return_ {
            types::function::ReturnAnnotation::Missing(_) => self.s(""),
            types::function::ReturnAnnotation::Available(annotation) => {
                let printed = self.print_node(NodeRef::Annotation(annotation), |p| {
                    p.print_type(&annotation.annotation)
                });
                let separator = match style {
                    FunctionTypeStyle::Arrow => " => ",
                    FunctionTypeStyle::Method | FunctionTypeStyle::Declaration => ": ",
                };
                self.concat([self.s(separator), printed])
            }
            types::function::ReturnAnnotation::TypeGuard(guard) => {
                let printed = self.print_type_guard(guard);
                let separator = match style {
                    FunctionTypeStyle::Arrow => " => ",
                    FunctionTypeStyle::Method | FunctionTypeStyle::Declaration => ": ",
                };
                self.concat([self.s(separator), printed])
            }
        };
        let should_group = self.should_group_function_type_params(function, return_doc);
        let parameters = if should_group {
            self.group(parameters)
        } else {
            parameters
        };
        let prefix = if is_hook && style == FunctionTypeStyle::Arrow {
            self.s("hook ")
        } else {
            self.s("")
        };
        self.group(self.concat([prefix, type_params, parameters, return_doc]))
    }

    fn should_group_function_type_params(
        &self,
        function: &'a types::Function<Loc, Loc>,
        return_doc: Doc<'a>,
    ) -> bool {
        let return_type = match &function.return_ {
            types::function::ReturnAnnotation::Available(annotation) => &annotation.annotation,
            _ => return false,
        };
        if let Some(tparams) = &function.tparams {
            if tparams.params.len() > 1 {
                return false;
            }
            if let [param] = &*tparams.params
                && (matches!(param.bound, types::AnnotationOrHint::Available(_))
                    || param.default.is_some())
            {
                return false;
            }
        }
        let count = function.params.params.len()
            + usize::from(function.params.rest.is_some())
            + usize::from(function.params.this.is_some());
        count == 1 && (is_object_type(return_type) || crate::doc::will_break(return_doc))
    }

    /// The `(...)` of a function type.
    fn print_function_type_params(
        &mut self,
        function: &'a types::Function<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let count = function.params.params.len()
            + usize::from(function.params.rest.is_some())
            + usize::from(function.params.this.is_some());
        if count == 0 {
            let dangling = self.print_dangling_comments_where(key, |p, comment| {
                p.text
                    .next_non_space_non_comment_character(comment.span.end)
                    == Some(')')
            });
            return self.concat([self.s("("), dangling.unwrap_or(self.s("")), self.s(")")]);
        }
        let parent_is_test_call = match self.grandparent() {
            Some(NodeRef::Expression(grand)) => is_test_call(grand, None),
            _ => false,
        };
        let should_hug = self.should_hug_the_only_type_param(function);
        let mut printed: Vec<Doc<'a>> = Vec::new();
        let mut index = 0usize;
        let push_separator = |p: &mut Self, printed: &mut Vec<Doc<'a>>, end: usize| {
            printed.push(p.s(","));
            if parent_is_test_call || should_hug {
                printed.push(p.s(" "));
            } else if p.text.is_next_line_empty(end) {
                printed.push(&crate::doc::HARDLINE);
                printed.push(&crate::doc::HARDLINE);
            } else {
                printed.push(&LINE);
            }
        };
        if let Some(this) = &function.params.this {
            let doc = self.print_node(NodeRef::FunctionTypeThis(this), |p| {
                let annotation = p.print_type_annotation(&this.annot);
                p.concat([p.s("this"), annotation])
            });
            printed.push(doc);
            index += 1;
            if index < count {
                push_separator(self, &mut printed, self.text.span(&this.loc).end);
            }
        }
        for param in function.params.params.iter() {
            let doc = self.print_function_type_param(param);
            printed.push(doc);
            index += 1;
            if index < count {
                push_separator(self, &mut printed, self.text.span(&param.loc).end);
            }
        }
        if let Some(rest) = &function.params.rest {
            let doc = self.print_node(NodeRef::FunctionTypeRest(rest), |p| {
                let argument = p.print_function_type_param(&rest.argument);
                p.concat([p.s("..."), argument])
            });
            printed.push(doc);
        }
        let printed_doc = self.docs.concat_vec(printed);
        if should_hug || parent_is_test_call {
            return self.concat([self.s("("), printed_doc, self.s(")")]);
        }
        if self.is_flow_shorthand_with_one_arg(function) {
            return self.concat([self.s("("), printed_doc, self.s(")")]);
        }
        let trailing_comma = if function.params.rest.is_some() {
            self.s("")
        } else {
            self.if_break(self.s(","), self.s(""))
        };
        self.concat([
            self.s("("),
            self.indent(self.concat([&SOFTLINE, printed_doc])),
            trailing_comma,
            &SOFTLINE,
            self.s(")"),
        ])
    }

    /// `(string) => void` in a type alias, union, or object property keeps
    /// its single simple parameter unindented.
    fn is_flow_shorthand_with_one_arg(&self, function: &'a types::Function<Loc, Loc>) -> bool {
        let parent_ok = match self.parent() {
            Some(NodeRef::ObjectTypeProperty(types::object::Property::NormalProperty(
                property,
            ))) => !property.static_ && !property.method,
            Some(NodeRef::Annotation(_)) => true,
            Some(NodeRef::Statement(statement)) => matches!(
                **statement,
                statement::StatementInner::TypeAlias { .. }
                    | statement::StatementInner::DeclareTypeAlias { .. }
            ),
            Some(NodeRef::Type(parent)) => {
                is_union(parent)
                    || is_intersection(parent)
                    || matches!(&**parent, types::TypeInner::Function { inner, .. }
                        if matches!(&inner.return_, types::function::ReturnAnnotation::Available(annotation)
                            if matches!(self.current(), Some(NodeRef::Type(me)) if same_type(&annotation.annotation, me))))
            }
            _ => false,
        };
        parent_ok
            && function.params.params.len() == 1
            && function.params.rest.is_none()
            && function.params.this.is_none()
            && function.tparams.is_none()
            && matches!(&function.params.params[0].param, types::function::ParamKind::Anonymous(annot) if self.is_simple_type(annot))
    }

    fn should_hug_the_only_type_param(&self, function: &'a types::Function<Loc, Loc>) -> bool {
        if function.params.params.len() != 1
            || function.params.rest.is_some()
            || function.params.this.is_some()
        {
            return false;
        }
        let param = &function.params.params[0];
        if self.has_comment(NodeRef::FunctionTypeParam(param).key()) {
            return false;
        }
        match &param.param {
            types::function::ParamKind::Anonymous(annot)
            | types::function::ParamKind::Labeled { annot, .. } => is_object_type(annot),
            types::function::ParamKind::Destructuring(pattern) => {
                matches!(
                    pattern,
                    uf_flow::ast::pattern::Pattern::Object { .. }
                        | uf_flow::ast::pattern::Pattern::Array { .. }
                )
            }
        }
    }

    /// `name?: T` or `T` in a function type's parameters.
    fn print_function_type_param(
        &mut self,
        param: &'a types::function::Param<Loc, Loc>,
    ) -> Doc<'a> {
        self.print_node(NodeRef::FunctionTypeParam(param), |p| match &param.param {
            types::function::ParamKind::Anonymous(annot) => p.print_type(annot),
            types::function::ParamKind::Labeled {
                name,
                annot,
                optional,
            } => {
                let name = p.print_identifier(name);
                let annot = p.print_type(annot);
                p.concat([
                    name,
                    if *optional { p.s("?") } else { p.s("") },
                    p.s(": "),
                    annot,
                ])
            }
            types::function::ParamKind::Destructuring(pattern) => p.print_pattern(pattern),
        })
    }

    /// `component(a: T) renders R` as a type.
    fn print_component_type(
        &mut self,
        component: &'a types::Component<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let type_params = self.print_optional_type_params(component.tparams.as_ref());
        let count = component.params.params.len() + usize::from(component.params.rest.is_some());
        let parameters = if count == 0 {
            let dangling = self.print_dangling_comments_where(key, |p, comment| {
                p.text
                    .next_non_space_non_comment_character(comment.span.end)
                    == Some(')')
            });
            self.concat([self.s("("), dangling.unwrap_or(self.s("")), self.s(")")])
        } else {
            let mut printed: Vec<Doc<'a>> = Vec::new();
            for (index, param) in component.params.params.iter().enumerate() {
                let doc = self.print_node(NodeRef::ComponentTypeParam(param), |p| {
                    let name = match &param.name {
                        statement::component_params::ParamName::Identifier(id) => {
                            p.print_identifier(id)
                        }
                        statement::component_params::ParamName::StringLiteral((loc, literal)) => {
                            p.print_string_node(loc, literal)
                        }
                    };
                    let annotation = p.print_type_annotation(&param.annot);
                    p.concat([
                        name,
                        if param.optional { p.s("?") } else { p.s("") },
                        annotation,
                    ])
                });
                printed.push(doc);
                if index + 1 < count {
                    printed.push(self.s(","));
                    printed.push(&LINE);
                }
            }
            if let Some(rest) = &component.params.rest {
                let doc = self.print_node(NodeRef::ComponentTypeRest(rest), |p| {
                    let annot = p.print_type(&rest.annot);
                    match &rest.argument {
                        Some(name) => {
                            let name = p.print_identifier(name);
                            p.concat([
                                p.s("..."),
                                name,
                                if rest.optional { p.s("?") } else { p.s("") },
                                p.s(": "),
                                annot,
                            ])
                        }
                        None => p.concat([p.s("..."), annot]),
                    }
                });
                printed.push(doc);
            }
            let trailing_comma = if component.params.rest.is_some() {
                self.s("")
            } else {
                self.if_break(self.s(","), self.s(""))
            };
            let inner = self.docs.concat_vec(printed);
            self.concat([
                self.s("("),
                self.indent(self.concat([&SOFTLINE, inner])),
                trailing_comma,
                &SOFTLINE,
                self.s(")"),
            ])
        };
        let renders = self.print_renders_annotation(&component.renders);
        self.group(self.concat([self.s("component"), type_params, parameters, renders]))
    }

    /// `x is T`, `asserts x`, `implies x is T`.
    pub fn print_type_guard(&mut self, guard: &'a types::TypeGuard<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::TypeGuard(guard), |p| {
            let kind = match guard.kind {
                types::TypeGuardKind::Default => p.s(""),
                types::TypeGuardKind::Asserts => p.s("asserts "),
                types::TypeGuardKind::Implies => p.s("implies "),
            };
            let name = p.print_identifier(&guard.guard.0);
            let ty = match &guard.guard.1 {
                Some(ty) => {
                    let printed = p.print_type(ty);
                    p.concat([p.s(" is "), printed])
                }
                None => p.s(""),
            };
            p.concat([kind, name, ty])
        })
    }

    /// Prettier's `shouldHugType`: a union of one object or generic type
    /// with only `null` and `void` beside it hugs.
    pub fn should_hug_type(&self, ty: &'a Type) -> bool {
        let members = members_of(ty);
        if members.is_empty() {
            return false;
        }
        if members
            .iter()
            .any(|member| self.has_comment(NodeRef::Type(member).key()))
        {
            return false;
        }
        let Some(object) = members.iter().copied().find(|member| {
            matches!(
                ***member,
                types::TypeInner::Object { .. } | types::TypeInner::Generic { .. }
            )
        }) else {
            return false;
        };
        members.iter().copied().all(|member| {
            same_type(member, object)
                || matches!(
                    **member,
                    types::TypeInner::Void { .. } | types::TypeInner::Null { .. }
                )
        })
    }

    fn print_union_type(&mut self, ty: &'a Type) -> Doc<'a> {
        let key = NodeRef::Type(ty).key();
        let members = members_of(ty);
        let should_hug = self.should_hug_type(ty);
        let should_indent =
            self.union_should_indent() && !self.union_in_alias_with_own_line_comment(key);
        // Members print their own comments here, so a comment lands beside
        // its member rather than outside the `| ` it belongs to.
        let printed: Vec<Doc<'a>> = members
            .iter()
            .map(|member| {
                let doc = self.print_type(member);
                let doc = if should_hug {
                    doc
                } else {
                    self.docs.align(2, doc)
                };
                self.print_comments(NodeRef::Type(member).key(), doc)
            })
            .collect();
        if should_hug {
            return self.join(self.s(" | "), printed);
        }
        let should_add_start_line = should_indent && !self.has_leading_own_line_comment(key, false);
        let separator = self.concat([&LINE, self.s("| ")]);
        let code = self.concat([
            self.if_break(
                self.concat([
                    if should_add_start_line {
                        &LINE
                    } else {
                        self.s("")
                    },
                    self.s("| "),
                ]),
                self.s(""),
            ),
            self.join(separator, printed),
        ]);
        if self.type_needs_parens(ty) {
            return self.group(self.concat([self.indent(code), &SOFTLINE]));
        }
        if self.is_in_multi_element_tuple() {
            return self.group(self.concat([
                self.indent(self.concat([
                    self.if_break(self.concat([self.s("("), &SOFTLINE]), self.s("")),
                    code,
                ])),
                &SOFTLINE,
                self.if_break(self.s(")"), self.s("")),
            ]));
        }
        if should_indent {
            self.group(self.indent(code))
        } else {
            self.group(code)
        }
    }

    /// A union that is the right side of a type alias or declarator and has
    /// a comment on its own line before it is not indented.
    fn union_in_alias_with_own_line_comment(&self, key: NodeKey) -> bool {
        let parent_is_alias = match self.parent() {
            Some(NodeRef::Statement(statement)) => matches!(
                **statement,
                statement::StatementInner::TypeAlias { .. }
                    | statement::StatementInner::DeclareTypeAlias { .. }
            ),
            Some(NodeRef::Declarator(_)) => true,
            _ => false,
        };
        parent_is_alias && self.has_leading_own_line_comment(key, false)
    }

    /// Prettier's `shouldIndent` for unions: not inside type arguments,
    /// tuples, conditional branches, or anonymous function type params.
    fn union_should_indent(&self) -> bool {
        let Some(me) = self.current() else {
            return true;
        };
        let NodeRef::Type(me) = me else {
            return true;
        };
        match self.parent() {
            Some(NodeRef::TypeArgs(_)) | Some(NodeRef::CallTypeArgs(_)) => false,
            Some(NodeRef::TupleElement(_)) => false,
            Some(NodeRef::Type(parent)) => match &**parent {
                types::TypeInner::Tuple { .. } => false,
                types::TypeInner::Conditional { inner, .. } => {
                    !(same_type(&inner.true_type, me) || same_type(&inner.false_type, me))
                }
                _ => true,
            },
            Some(NodeRef::FunctionTypeParam(param)) => {
                !matches!(param.param, types::function::ParamKind::Anonymous(_))
                    || self.is_param_of_object_property_function()
            }
            _ => true,
        }
    }

    fn is_param_of_object_property_function(&self) -> bool {
        let ancestors: Vec<NodeRef<'a>> = self.ancestors.iter().rev().copied().collect();
        matches!(
            (ancestors.get(1), ancestors.get(2), ancestors.get(3)),
            (Some(NodeRef::FunctionTypeParam(_)), Some(NodeRef::Type(_)), Some(NodeRef::ObjectTypeProperty(types::object::Property::NormalProperty(property))))
                if !property.method && !property.static_
        )
    }

    fn print_intersection_type(&mut self, ty: &'a Type) -> Doc<'a> {
        let members = members_of(ty);
        let mut parts: Vec<Doc<'a>> = Vec::with_capacity(members.len() * 2);
        let mut was_indented = false;
        for (index, member) in members.iter().enumerate() {
            let printed = self.print_type(member);
            if index == 0 {
                parts.push(printed);
                continue;
            }
            let is_object = is_object_type(member);
            let previous_is_object = is_object_type(members[index - 1]);
            let has_own_line_comment =
                self.has_leading_own_line_comment(NodeRef::Type(member).key(), false);
            if previous_is_object && is_object && !has_own_line_comment {
                parts.push(self.concat([
                    self.s(" & "),
                    if was_indented {
                        self.indent(printed)
                    } else {
                        printed
                    },
                ]));
            } else if (!previous_is_object && !is_object) || has_own_line_comment {
                parts.push(self.indent(self.concat([self.s(" &"), &LINE, printed])));
            } else {
                if index > 1 {
                    was_indented = true;
                }
                parts.push(self.s(" & "));
                parts.push(if index > 1 {
                    self.indent(printed)
                } else {
                    printed
                });
            }
        }
        self.group(self.docs.concat_vec(parts))
    }

    /// `Name<Args>`.
    pub fn print_generic(&mut self, generic: &'a types::Generic<Loc, Loc>) -> Doc<'a> {
        let id = self.print_generic_identifier(&generic.id);
        let targs = match &generic.targs {
            Some(targs) => self.print_type_args(targs),
            None => self.s(""),
        };
        self.concat([id, targs])
    }

    /// `Name`, `A.B`, or `import("m")`.
    pub fn print_generic_identifier(
        &mut self,
        id: &'a types::generic::Identifier<Loc, Loc>,
    ) -> Doc<'a> {
        match id {
            types::generic::Identifier::Unqualified(id) => self.print_identifier(id),
            types::generic::Identifier::Qualified(qualified) => {
                self.print_node(NodeRef::QualifiedType(qualified), |p| {
                    let qualification = p.print_generic_identifier(&qualified.qualification);
                    let id = p.print_identifier(&qualified.id);
                    p.concat([qualification, p.s("."), id])
                })
            }
            types::generic::Identifier::ImportTypeAnnot(import) => self.print_import_type(import),
        }
    }

    fn print_import_type(&mut self, import: &'a types::generic::ImportType<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::ImportType(import), |p| {
            let source = p.print_string_node(&import.argument.0, &import.argument.1);
            p.concat([p.s("import("), source, p.s(")")])
        })
    }

    fn print_typeof_target(&mut self, target: &'a types::typeof_::Target<Loc, Loc>) -> Doc<'a> {
        match target {
            types::typeof_::Target::Unqualified(id) => self.print_identifier(id),
            types::typeof_::Target::Qualified(qualified) => {
                self.print_node(NodeRef::QualifiedTypeof(qualified), |p| {
                    let qualification = p.print_typeof_target(&qualified.qualification);
                    let id = p.print_identifier(&qualified.id);
                    p.concat([qualification, p.s("."), id])
                })
            }
            types::typeof_::Target::Import(import) => self.print_import_type(import),
        }
    }

    /// `: T`, or `/*: T */` when that is how it was written, with the
    /// annotation node's comments.
    pub fn print_type_annotation(
        &mut self,
        annotation: &'a types::Annotation<Loc, Loc>,
    ) -> Doc<'a> {
        let node = NodeRef::Annotation(annotation);
        let has_leading = self.has_comment_placed(node.key(), Placement::Leading);
        let comment_form = self.comment_type_source(&annotation.loc);
        let printed = self.print_node(node, |p| match comment_form {
            Some(raw) => p.replace_end_of_line(raw),
            None => {
                let ty = p.print_type(&annotation.annotation);
                p.concat([p.s(": "), ty])
            }
        });
        if has_leading || comment_form.is_some() {
            self.concat([self.s(" "), printed])
        } else {
            printed
        }
    }

    /// The source of an annotation written as one of Flow's comment types,
    /// `/*` to `*/`, or [`None`] when it was written as ordinary syntax.
    ///
    /// Flow's parser lexes `/*: string */` as an ordinary annotation and the
    /// tree records nothing to say where it came from — but the *location*
    /// does: an annotation written in a comment starts at the `/*` rather
    /// than at the `:`. That is exact, and it is where Prettier decides the
    /// same thing.
    ///
    /// The bytes come back untouched, which is also what Prettier does:
    /// `/*: {[string]: string} */` keeps its spacing where a real annotation
    /// would be re-printed as `{ [string]: string }`. Reformatting inside the
    /// comment would be a second, quieter way of the same bug — the text is a
    /// comment to every tool that is not Flow.
    ///
    /// Preserving the form at all matters because it is the whole point of
    /// the syntax: a file can carry annotations *and* run under bare `node`.
    /// React Native has a build script that does, and after `uf fmt` it
    /// needed a compiler. See ubugeeei-prod/uf#126.
    fn comment_type_source(&self, loc: &Loc) -> Option<&'a str> {
        let span = self.text.span(loc);
        let source = self.text.text();
        if !source.get(span.start..)?.starts_with("/*") {
            return None;
        }
        // The location stops before the closing delimiter. A block comment
        // does not nest, so the next `*/` is this one's.
        let close = source.get(span.end..)?.find("*/")? + span.end + 2;
        source.get(span.start..close)
    }

    /// `<T, U>` on a declaration, or nothing.
    pub fn print_optional_type_params(
        &mut self,
        tparams: Option<&'a types::TypeParams<Loc, Loc>>,
    ) -> Doc<'a> {
        match tparams {
            Some(tparams) => self.print_node(NodeRef::TypeParams(tparams), |p| {
                p.print_type_params_inner(tparams)
            }),
            None => self.s(""),
        }
    }

    /// The `<…>` of a type parameter declaration, without its comments.
    pub fn print_type_params_inner(&mut self, tparams: &'a types::TypeParams<Loc, Loc>) -> Doc<'a> {
        let key = NodeRef::TypeParams(tparams).key();
        let printed: Vec<Doc<'a>> = tparams
            .params
            .iter()
            .map(|param| self.print_type_param(param))
            .collect();
        let simple = tparams.params.len() == 1
            && matches!(tparams.params[0].bound, types::AnnotationOrHint::Missing(_))
            && tparams.params[0].default.is_none()
            && tparams.params[0].variance.is_none();
        self.print_angle_list(
            key,
            printed,
            simple,
            &tparams
                .params
                .iter()
                .map(NodeRef::TypeParam)
                .collect::<Vec<_>>(),
        )
    }

    /// `<A, B>` on a type reference.
    pub fn print_type_args(&mut self, targs: &'a types::TypeArgs<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::TypeArgs(targs), |p| {
            let key = NodeRef::TypeArgs(targs).key();
            let printed: Vec<Doc<'a>> = targs
                .arguments
                .iter()
                .map(|arg| p.print_type(arg))
                .collect();
            let grandparent_is_test_call = match p.grandparent() {
                Some(NodeRef::Expression(grand)) => is_test_call(grand, None),
                _ => false,
            };
            let simple = grandparent_is_test_call
                || (targs.arguments.len() == 1
                    && (matches!(*targs.arguments[0], types::TypeInner::Nullable { .. })
                        || p.is_simple_type(&targs.arguments[0])
                        || is_object_type(&targs.arguments[0])
                        || (is_union(&targs.arguments[0])
                            && p.should_hug_type(&targs.arguments[0]))));
            let nodes: Vec<NodeRef<'a>> = targs.arguments.iter().map(NodeRef::Type).collect();
            p.print_angle_list(key, printed, simple, &nodes)
        })
    }

    /// `<A, B>` on a call or JSX element.
    pub fn print_call_type_args(
        &mut self,
        targs: &'a expression::CallTypeArgs<Loc, Loc>,
    ) -> Doc<'a> {
        // `new Map /*:: <string, T> */()` is the third of Flow's comment
        // forms, and the one that breaks a file most plainly: the angle
        // brackets are a syntax error to anything that is not Flow.
        //
        // Unlike an annotation, the location here starts at the `<` and not
        // at the `/*` — so it is the block the arguments sit *in* that is
        // printed, from the spans found once for the file. See
        // ubugeeei-prod/uf#126.
        if let Some(block) = self.comment_type_around(self.text.span(&targs.loc)) {
            let raw = self.text.slice(block);
            return self.print_node(NodeRef::CallTypeArgs(targs), |p| {
                let printed = p.replace_end_of_line(raw);
                p.concat([p.s(" "), printed])
            });
        }
        self.print_node(NodeRef::CallTypeArgs(targs), |p| {
            let key = NodeRef::CallTypeArgs(targs).key();
            let printed: Vec<Doc<'a>> = targs
                .arguments
                .iter()
                .map(|arg| match arg {
                    expression::CallTypeArg::Explicit(ty) => p.print_type(ty),
                    expression::CallTypeArg::Implicit(implicit) => {
                        p.print_node(NodeRef::ImplicitTypeArg(implicit), |p| p.s("_"))
                    }
                })
                .collect();
            let grandparent_is_test_call = match p.grandparent() {
                Some(NodeRef::Expression(grand)) => is_test_call(grand, None),
                _ => false,
            };
            let simple = grandparent_is_test_call
                || (targs.arguments.len() == 1
                    && match &targs.arguments[0] {
                        expression::CallTypeArg::Explicit(ty) => {
                            matches!(**ty, types::TypeInner::Nullable { .. })
                                || p.is_simple_type(ty)
                                || is_object_type(ty)
                                || (is_union(ty) && p.should_hug_type(ty))
                        }
                        expression::CallTypeArg::Implicit(_) => true,
                    });
            let nodes: Vec<NodeRef<'a>> = targs
                .arguments
                .iter()
                .map(|arg| match arg {
                    expression::CallTypeArg::Explicit(ty) => NodeRef::Type(ty),
                    expression::CallTypeArg::Implicit(implicit) => {
                        NodeRef::ImplicitTypeArg(implicit)
                    }
                })
                .collect();
            p.print_angle_list(key, printed, simple, &nodes)
        })
    }

    /// `<a, b>`, flat when `simple` and no member carries a line comment,
    /// otherwise a breakable group.
    fn print_angle_list(
        &mut self,
        key: NodeKey,
        printed: Vec<Doc<'a>>,
        simple: bool,
        nodes: &[NodeRef<'a>],
    ) -> Doc<'a> {
        let has_awkward_comment = nodes.iter().any(|node| {
            let node_key = node.key();
            self.has_line_comment(node_key, None)
                || self.has_comment_where(node_key, Some(Placement::Trailing), |comment| {
                    self.text.has_newline(comment.span.end, false)
                })
        });
        if printed.is_empty() || (simple && !has_awkward_comment) {
            let dangling =
                self.print_dangling_comments(key, crate::flow::comments::Marker::None, false);
            let separator = self.s(", ");
            let dangling = match dangling {
                Some(dangling) if self.has_line_comment(key, Some(Placement::Dangling)) => self
                    .concat([
                        self.indent(self.concat([&crate::doc::HARDLINE, dangling])),
                        &crate::doc::HARDLINE,
                    ]),
                Some(dangling) => dangling,
                None => self.s(""),
            };
            return self.concat([
                self.s("<"),
                self.join(separator, printed),
                dangling,
                self.s(">"),
            ]);
        }
        let separator = self.concat([self.s(","), &LINE]);
        self.group(self.concat([
            self.s("<"),
            self.indent(self.concat([&SOFTLINE, self.join(separator, printed)])),
            self.if_break(self.s(","), self.s("")),
            &SOFTLINE,
            self.s(">"),
        ]))
    }

    /// `+T: Bound = Default`.
    pub fn print_type_param(&mut self, param: &'a types::TypeParam<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::TypeParam(param), |p| {
            let mut parts = Vec::new();
            if param.const_.is_some() {
                parts.push(p.s("const "));
            }
            if let Some(variance) = &param.variance {
                parts.push(p.print_variance(variance));
            }
            parts.push(p.print_identifier(&param.name));
            if let types::AnnotationOrHint::Available(bound) = &param.bound {
                match param.bound_kind {
                    types::type_param::BoundKind::Colon => {
                        parts.push(p.print_type_annotation(bound))
                    }
                    types::type_param::BoundKind::Extends => {
                        let printed = p.print_node(NodeRef::Annotation(bound), |p| {
                            p.print_type(&bound.annotation)
                        });
                        parts.push(p.s(" extends "));
                        parts.push(printed);
                    }
                }
            }
            if let Some(default) = &param.default {
                let group_id = p.docs.group_id();
                let printed = p.print_type(default);
                parts.push(p.s(" ="));
                parts.push(p.docs.group_with(p.indent(&LINE), false, Some(group_id)));
                parts.push(&LINE_SUFFIX_BOUNDARY);
                parts.push(p.docs.indent_if_break(printed, group_id, false));
            }
            p.group(p.docs.concat_vec(parts))
        })
    }

    /// `type Name<T> = Type;`
    pub fn print_type_alias(
        &mut self,
        alias: &'a statement::TypeAlias<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let _ = key;
        let id = self.print_identifier(&alias.id);
        let tparams = self.print_optional_type_params(alias.tparams.as_ref());
        let left = self.concat([self.s("type "), id, tparams]);
        let node = self.current().unwrap_or(NodeRef::Identifier(&alias.id));
        let assignment =
            self.print_assignment_like(node, left, " =", Some(Rhs::Type(&alias.right)));
        self.concat([assignment, self.semi()])
    }

    /// `opaque type Name<T>: Super = Impl;`
    pub fn print_opaque_type(
        &mut self,
        opaque: &'a statement::OpaqueType<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let _ = key;
        let mut parts = vec![self.s("opaque type ")];
        parts.push(self.print_identifier(&opaque.id));
        parts.push(self.print_optional_type_params(opaque.tparams.as_ref()));
        if let Some(bound) = &opaque.legacy_upper_bound {
            let printed = self.print_type(bound);
            parts.push(self.s(": "));
            parts.push(printed);
        }
        if opaque.lower_bound.is_some() || opaque.upper_bound.is_some() {
            let mut bounds = Vec::new();
            if let Some(bound) = &opaque.lower_bound {
                let printed = self.print_type(bound);
                bounds.push(self.indent(self.concat([&LINE, self.s("super "), printed])));
            }
            if let Some(bound) = &opaque.upper_bound {
                let printed = self.print_type(bound);
                bounds.push(self.indent(self.concat([&LINE, self.s("extends "), printed])));
            }
            parts.push(self.group(self.docs.concat_vec(bounds)));
        }
        if let Some(impl_type) = &opaque.impl_type {
            let printed = self.print_type(impl_type);
            parts.push(self.s(" = "));
            parts.push(printed);
        }
        parts.push(self.semi());
        self.docs.concat_vec(parts)
    }

    /// `interface Name<T> extends A, B { … }`
    pub fn print_interface(
        &mut self,
        interface: &'a statement::Interface<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let mut parts = vec![self.s("interface")];
        let id = self.print_identifier(&interface.id);
        parts.push(self.s(" "));
        parts.push(id);
        parts.push(self.print_optional_type_params(interface.tparams.as_ref()));
        // Any heritage at all, or a trailing comment on the name. That is
        // the hermes plugin's condition, and the hermes plugin is what every
        // expectation in `tests/fixtures` is generated with — Prettier's own
        // estree printer answers this differently, and the two disagree
        // about a long `extends` list. See ubugeeei-prod/uf#143.
        let group_mode = !interface.extends.is_empty()
            || self.has_comment_placed(
                NodeRef::Identifier(&interface.id).key(),
                Placement::Trailing,
            );
        let extends = self.print_interface_extends_list(&interface.extends, key);
        if group_mode {
            let head = self.docs.concat_vec(std::mem::take(&mut parts));
            // The head is indented *with* the clause rather than beside it.
            // `interface Several` stays whole — the space after `interface`
            // is a space and not a line — and what the indent reaches is the
            // line before `extends`.
            parts.push(self.group(self.indent(self.concat([head, extends]))));
        } else {
            parts.push(extends);
        }
        parts.push(self.s(" "));
        parts.push(self.print_object_type(
            &interface.body.1,
            NodeRef::ObjectType(&interface.body.0, &interface.body.1),
            true,
        ));
        self.docs.concat_vec(parts)
    }

    /// ` extends A, B`, or nothing.
    ///
    /// One target and several are the same doc but for where the indent
    /// goes: `extends ` keeps its first target, and only the ones after it
    /// are indented under it.
    ///
    /// ```text
    /// interface Several
    ///   extends AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA,
    ///     BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB {
    /// ```
    fn print_interface_extends_list(
        &mut self,
        extends: &'a [(Loc, types::Generic<Loc, Loc>)],
        key: NodeKey,
    ) -> Doc<'a> {
        if extends.is_empty() {
            return self.s("");
        }
        let printed: Vec<Doc<'a>> = extends
            .iter()
            .map(|item| self.print_interface_extends(item))
            .collect();
        let dangling =
            self.print_dangling_comments(key, crate::flow::comments::Marker::Extends, false);
        let separator = self.concat([self.s(","), &LINE]);
        let list = self.join(separator, printed);
        let targets = if extends.len() > 1 {
            self.indent(list)
        } else {
            list
        };
        // A line rather than a space, so a trailing line comment on the name
        // has one to end. Without it the comment was a line suffix with
        // nothing before the body to flush at, and it came out after the `{`
        // — five levels from where it was written. See ubugeeei-prod/uf#135.
        self.concat([
            &LINE,
            dangling.unwrap_or(self.s("")),
            self.s("extends "),
            targets,
        ])
    }

    /// One `extends` target of an interface or declared class.
    pub fn print_interface_extends(
        &mut self,
        item: &'a (Loc, types::Generic<Loc, Loc>),
    ) -> Doc<'a> {
        self.print_node(NodeRef::InterfaceExtends(item), |p| {
            p.print_generic(&item.1)
        })
    }
}
