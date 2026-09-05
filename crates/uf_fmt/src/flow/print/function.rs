//! Functions, arrow functions, parameters and return types, components
//! and hooks: Prettier's `printFunction`, `printArrowFunction`,
//! `printFunctionParameters` and their helpers.
//!
//! Two rules do most of the work. A single object-pattern parameter *hugs*
//! the parentheses — `function f({ a, b }) {}` breaks inside the braces,
//! not before them. And an arrow chain `(a) => (b) => (c) => …` is printed
//! as one group so it breaks after the arrows together.

use uf_flow::Loc;
use uf_flow::ast::{expression, function, pattern, statement, types};

use super::Printer;
use super::statement::EmptyBlock;
use super::assignment::{Layout, PrintArgs};
use super::call::{is_test_call, parameter_count};
use super::parens::{is_binaryish, is_call_like, is_jsx, starts_with_no_lookahead_token};
use crate::doc::{Doc, HARDLINE, LINE, SOFTLINE, will_break};
use crate::flow::comments::Marker;
use crate::flow::node::{Expression, Function, NodeKey, NodeRef};

/// Whether `ty` is an object type, which hugs where it can.
pub fn is_object_type(ty: &types::Type<Loc, Loc>) -> bool {
    matches!(**ty, types::TypeInner::Object { .. })
}

impl<'a> Printer<'a> {
    /// `function name(params): ret {}` or `hook name(params): ret {}`.
    /// `key` is the node the function is: a declaration statement, a
    /// function expression, or a method value.
    pub fn print_function(
        &mut self,
        function: &'a Function,
        key: NodeKey,
        args: PrintArgs,
    ) -> Doc<'a> {
        let is_hook = matches!(function.effect_, function::Effect::Hook);
        let mut expand_arg = false;
        if args.expand_last_arg
            && !is_hook
            && let Some(NodeRef::Expression(parent)) = self.parent()
            && is_call_like(parent)
        {
            let argument_count = super::call::argument_expression_count(parent);
            let all_plain = function.params.params.iter().all(|param| {
                matches!(param, function::Param::RegularParam { argument, default: None, .. }
                        if matches!(argument, pattern::Pattern::Identifier { inner, .. }
                            if matches!(inner.annot, types::AnnotationOrHint::Missing(_))))
            }) && function.params.rest.is_none()
                && function.params.this_.is_none();
            if argument_count > 1 || all_plain {
                expand_arg = true;
            }
        }

        let mut parts = Vec::new();
        if function.async_ {
            parts.push(self.s("async "));
        }
        if is_hook {
            parts.push(self.s("hook "));
        } else if function.generator {
            parts.push(self.s("function* "));
        } else {
            parts.push(self.s("function "));
        }
        if let Some(id) = &function.id {
            parts.push(self.print_identifier(id));
        }

        let parameters = self.print_function_params(function, key, expand_arg, false);
        let return_type = self.print_return_type(function);
        let should_group = self.should_group_function_parameters(function, return_type);
        let type_params = self.print_optional_type_params(function.tparams.as_ref());
        parts.push(type_params);
        parts.push(self.group(self.concat([
            if should_group {
                self.group(parameters)
            } else {
                parameters
            },
            return_type,
        ])));
        match &function.body {
            function::Body::BodyBlock((loc, block)) => {
                let body = self.print_block(loc, block, EmptyBlock::Collapsed);
                parts.push(self.s(" "));
                parts.push(body);
            }
            function::Body::BodyExpression(body) => {
                // Not valid for a `function`, but the tree allows it.
                let body = self.print_expression(body);
                parts.push(self.s(" "));
                parts.push(body);
            }
        }
        self.docs.concat_vec(parts)
    }

    /// A method's `(params): ret { body }`, after its name.
    pub fn print_method_value(&mut self, function: &'a Function, key: NodeKey) -> Doc<'a> {
        let parameters = self.print_function_params(function, key, false, false);
        let return_type = self.print_return_type(function);
        let should_break = self.should_break_function_parameters(function);
        let should_group = self.should_group_function_parameters(function, return_type);
        let type_params = self.print_optional_type_params(function.tparams.as_ref());
        let parameters = if should_break {
            self.docs.group_with(parameters, true, None)
        } else if should_group {
            self.group(parameters)
        } else {
            parameters
        };
        let mut parts = vec![
            type_params,
            self.group(self.concat([parameters, return_type])),
        ];
        match &function.body {
            function::Body::BodyBlock((loc, block)) => {
                let body = self.print_block(loc, block, EmptyBlock::Collapsed);
                parts.push(self.s(" "));
                parts.push(body);
            }
            function::Body::BodyExpression(body) => {
                let body = self.print_expression(body);
                parts.push(self.s(" "));
                parts.push(body);
            }
        }
        self.docs.concat_vec(parts)
    }

    /// Whether a method's parameters must break: several of them, and one
    /// carries a default or an object type.
    fn should_break_function_parameters(&self, function: &'a Function) -> bool {
        let count = parameter_count(function);
        if count <= 1 {
            return false;
        }
        function.params.params.iter().any(|param| match param {
            function::Param::RegularParam {
                default: Some(_), ..
            } => false,
            function::Param::ParamProperty { .. } => true,
            function::Param::RegularParam { .. } => false,
        })
    }

    /// The `: T` (and `%checks`) after a function's parameters.
    pub fn print_return_type(&mut self, function: &'a Function) -> Doc<'a> {
        let mut parts = Vec::new();
        match &function.return_ {
            function::ReturnAnnot::Missing(_) => {}
            function::ReturnAnnot::Available(annotation) => {
                parts.push(self.print_type_annotation(annotation));
            }
            function::ReturnAnnot::TypeGuard(guard) => {
                let printed = self.print_type_guard(&guard.guard);
                parts.push(self.concat([self.s(": "), printed]));
            }
        }
        if let Some(predicate) = &function.predicate {
            // An *inferred* predicate stands where the return type would
            // have been, so it brings the colon with it: `function f(x):
            // %checks {}` is the whole annotation and `function f(x)
            // %checks {}` does not parse. When a return type is present it
            // has already written the colon, and `%checks` follows it as a
            // second word.
            if matches!(function.return_, function::ReturnAnnot::Missing(_)) {
                parts.push(self.s(":"));
            }
            parts.push(self.print_predicate(predicate));
        }
        self.docs.concat_vec(parts)
    }

    /// `%checks` or `%checks(expr)`.
    ///
    /// Always with a leading space, so it reads as a word after whatever
    /// precedes it — a return type, or the bare colon
    /// [`print_return_type`](Self::print_return_type) writes when there is
    /// none.
    pub fn print_predicate(&mut self, predicate: &'a types::Predicate<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::Predicate(predicate), |p| match &predicate.kind {
            types::PredicateKind::Inferred => p.s(" %checks"),
            types::PredicateKind::Declared(expression) => {
                let printed = p.print_expression(expression);
                p.concat([p.s(" %checks("), printed, p.s(")")])
            }
        })
    }

    /// The return type node, for deciding whether parameters group.
    fn return_type_node(function: &'a Function) -> Option<&'a types::Type<Loc, Loc>> {
        match &function.return_ {
            function::ReturnAnnot::Available(annotation) => Some(&annotation.annotation),
            _ => None,
        }
    }

    /// Prettier's `shouldGroupFunctionParameters`: one parameter, a
    /// return type that is an object or breaks, and no complex type
    /// parameters.
    pub fn should_group_function_parameters(
        &self,
        function: &'a Function,
        return_type: Doc<'a>,
    ) -> bool {
        let Some(return_node) = Self::return_type_node(function) else {
            return false;
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
        parameter_count(function) == 1 && (is_object_type(return_node) || will_break(return_type))
    }

    /// Prettier's `shouldHugTheOnlyFunctionParameter`.
    pub fn should_hug_the_only_function_parameter(&self, function: &'a Function) -> bool {
        if parameter_count(function) != 1 {
            return false;
        }
        let Some(param) = function.params.params.first() else {
            return false;
        };
        let function::Param::RegularParam {
            argument, default, ..
        } = param
        else {
            return false;
        };
        if self.has_comment(NodeRef::Param(param).key())
            || self.has_comment(NodeRef::Pattern(argument).key())
        {
            return false;
        }
        match default {
            None => match argument {
                pattern::Pattern::Object { .. } | pattern::Pattern::Array { .. } => true,
                pattern::Pattern::Identifier { inner, .. } => match &inner.annot {
                    types::AnnotationOrHint::Available(annotation) => {
                        is_object_type(&annotation.annotation)
                    }
                    types::AnnotationOrHint::Missing(_) => false,
                },
                pattern::Pattern::Expression { .. } => false,
            },
            Some(default) => {
                matches!(
                    argument,
                    pattern::Pattern::Object { .. } | pattern::Pattern::Array { .. }
                ) && match &**default {
                    expression::ExpressionInner::Identifier { .. } => true,
                    expression::ExpressionInner::Object { inner, .. } => {
                        inner.properties.is_empty()
                    }
                    expression::ExpressionInner::Array { inner, .. } => inner.elements.is_empty(),
                    _ => false,
                }
            }
        }
    }

    /// Whether the pattern on top of the ancestor stack is the hugged only
    /// parameter of its function.
    pub fn is_hugged_only_parameter(&self) -> bool {
        let (Some(NodeRef::Param(param)), Some(function_node)) =
            (self.parent(), self.grandparent())
        else {
            return false;
        };
        if !matches!(param, function::Param::RegularParam { default: None, .. }) {
            return false;
        }
        match function_of_node(function_node) {
            Some(function) => self.should_hug_the_only_function_parameter(function),
            None => false,
        }
    }

    /// Whether the type on top of the ancestor stack is the annotation of
    /// the hugged only parameter of its function.
    pub fn is_annotation_of_hugged_parameter(&self) -> bool {
        let ancestors: Vec<NodeRef<'a>> = self.ancestors.iter().rev().copied().collect();
        let (
            Some(NodeRef::Annotation(_)),
            Some(NodeRef::Pattern(_)),
            Some(NodeRef::Param(_)),
            Some(function_node),
        ) = (
            ancestors.get(1),
            ancestors.get(2),
            ancestors.get(3),
            ancestors.get(4),
        )
        else {
            return false;
        };
        match function_of_node(*function_node) {
            Some(function) => self.should_hug_the_only_function_parameter(function),
            None => false,
        }
    }

    /// `(a, b, ...rest)` with the hugging and breaking rules. `key` is the
    /// function node, which owns dangling comments in empty parentheses.
    pub fn print_function_params(
        &mut self,
        function: &'a Function,
        key: NodeKey,
        expand_arg: bool,
        _print_type_params: bool,
    ) -> Doc<'a> {
        let count = parameter_count(function);
        if count == 0 {
            let dangling = self.print_dangling_comments_where(key, |p, comment| {
                p.text
                    .next_non_space_non_comment_character(comment.span.end)
                    == Some(')')
            });
            return self.concat([self.s("("), dangling.unwrap_or(self.s("")), self.s(")")]);
        }

        let parent_is_test_call = match self.parent() {
            Some(NodeRef::Expression(parent)) => is_test_call(parent, None),
            _ => false,
        };
        let should_hug = self.should_hug_the_only_function_parameter(function);

        let mut printed: Vec<Doc<'a>> = Vec::with_capacity(count * 3);
        let mut index = 0usize;
        let push_separator = |p: &mut Self, printed: &mut Vec<Doc<'a>>, end: usize| {
            printed.push(p.s(","));
            if parent_is_test_call || should_hug {
                printed.push(p.s(" "));
            } else if p.text.is_next_line_empty(end) {
                printed.push(&HARDLINE);
                printed.push(&HARDLINE);
            } else {
                printed.push(&LINE);
            }
        };
        if let Some(this) = &function.params.this_ {
            let doc = self.print_node(NodeRef::ThisParam(this), |p| {
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
            let doc = self.print_param(param);
            printed.push(doc);
            index += 1;
            if index < count {
                push_separator(
                    self,
                    &mut printed,
                    self.text.span(&NodeRef::Param(param).loc()).end,
                );
            }
        }
        if let Some(rest) = &function.params.rest {
            let doc = self.print_node(NodeRef::RestParam(rest), |p| {
                let argument = p.print_pattern(&rest.argument);
                p.concat([p.s("..."), argument])
            });
            printed.push(doc);
        }

        if expand_arg && !self.is_arrow_in_curried_test_call() {
            let printed_doc = self.docs.concat_vec(printed);
            if will_break(printed_doc) {
                self.expansion_bailout = true;
            }
            let flat = self.docs.remove_lines(printed_doc);
            return self.group(self.concat([self.s("("), flat, self.s(")")]));
        }

        let printed_doc = self.docs.concat_vec(printed);
        if should_hug || parent_is_test_call {
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

    /// Prettier's exception to the hugged-argument bailout: an arrow with a
    /// block body that is the only argument of a curried call, `f(a)(() =>
    /// {})`.
    fn is_arrow_in_curried_test_call(&self) -> bool {
        let Some(NodeRef::Expression(arrow)) = self.current() else {
            return false;
        };
        let expression::ExpressionInner::ArrowFunction { inner, .. } = &**arrow else {
            return false;
        };
        if !matches!(inner.body, function::Body::BodyBlock(_)) {
            return false;
        }
        let Some(NodeRef::Expression(parent)) = self.parent() else {
            return false;
        };
        let expression::ExpressionInner::Call { inner: call, .. } = &**parent else {
            return false;
        };
        if call.arguments.arguments.len() != 1 {
            return false;
        }
        let expression::ExpressionInner::Call { inner: outer, .. } = &*call.callee else {
            return false;
        };
        match &*outer.callee {
            expression::ExpressionInner::Identifier { .. } => true,
            expression::ExpressionInner::Member { inner: member, .. } => {
                matches!(
                    *member.object,
                    expression::ExpressionInner::Identifier { .. }
                ) && matches!(
                    member.property,
                    expression::member::Property::PropertyIdentifier(_)
                )
            }
            _ => false,
        }
    }

    /// One parameter: a pattern, with its default if it has one.
    pub fn print_param(&mut self, param: &'a function::Param<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::Param(param), |p| match param {
            function::Param::RegularParam {
                argument, default, ..
            } => {
                let pattern = p.print_pattern(argument);
                match default {
                    Some(default) => {
                        let value = p.print_expression(default);
                        p.concat([pattern, p.s(" = "), value])
                    }
                    None => pattern,
                }
            }
            function::Param::ParamProperty { property, .. } => p.print_class_property(property),
        })
    }

    /// Dangling comments on `key` that `filter` accepts.
    pub fn print_dangling_comments_where(
        &mut self,
        key: NodeKey,
        filter: impl Fn(&Self, &crate::flow::comments::Comment<'a>) -> bool,
    ) -> Option<Doc<'a>> {
        let indices: Vec<u32> = self
            .comments
            .slots(key)?
            .dangling
            .iter()
            .copied()
            .filter(|index| {
                let comment = self.comments.get(*index);
                comment.marker == Marker::None && filter(self, &comment)
            })
            .collect();
        if indices.is_empty() {
            return None;
        }
        let parts: Vec<Doc<'a>> = indices
            .into_iter()
            .map(|index| self.print_comment(index))
            .collect();
        Some(self.join(&HARDLINE, parts))
    }

    // ---- arrow functions ----

    /// `(params) => body`, and chains of them.
    pub fn print_arrow_function(
        &mut self,
        function: &'a Function,
        expression: &'a Expression,
        args: PrintArgs,
    ) -> Doc<'a> {
        let mut signatures: Vec<Doc<'a>> = Vec::new();
        let mut body_comments: Vec<Doc<'a>> = Vec::new();
        let mut should_break_chain = false;
        let should_print_as_chain = !args.expand_last_arg
            && matches!(&function.body, function::Body::BodyExpression(body) if matches!(**body, expression::ExpressionInner::ArrowFunction { .. }));

        // Walk down the chain of arrows, collecting each signature.
        let mut current_function = function;
        let mut current_expression = expression;
        let mut pushed = 0usize;
        let (body_doc, function_body) = loop {
            let node = NodeRef::Expression(current_expression);
            let is_root = std::ptr::eq(current_function, function);
            if !is_root {
                self.ancestors.push(node);
                pushed += 1;
            }
            let signature = self.print_arrow_signature(current_function, node.key(), args);
            if signatures.is_empty() {
                signatures.push(signature);
            } else {
                let leading = self.print_leading_comments(node.key());
                let trailing = self.print_trailing_comments(node.key());
                signatures.push(self.concat([leading.unwrap_or(self.s("")), signature]));
                if let Some(trailing) = trailing {
                    body_comments.insert(0, trailing);
                }
            }
            if should_print_as_chain {
                should_break_chain = should_break_chain
                    || (!matches!(current_function.return_, function::ReturnAnnot::Missing(_))
                        && parameter_count(current_function) > 0)
                    || current_function.tparams.is_some()
                    || current_function.params.params.iter().any(|param| {
                        !matches!(
                            param,
                            function::Param::RegularParam {
                                argument: pattern::Pattern::Identifier { .. },
                                default: None,
                                ..
                            }
                        )
                    })
                    || current_function.params.rest.is_some();
            }
            match &current_function.body {
                function::Body::BodyExpression(body)
                    if should_print_as_chain
                        && matches!(**body, expression::ExpressionInner::ArrowFunction { .. }) =>
                {
                    let expression::ExpressionInner::ArrowFunction { inner, .. } = &**body else {
                        unreachable!();
                    };
                    current_function = inner;
                    current_expression = body;
                }
                function::Body::BodyExpression(body) => {
                    let printed = self.print_expression_with(body, args);
                    break (printed, ArrowBody::Expression(body));
                }
                function::Body::BodyBlock((loc, block)) => {
                    let printed = self.print_block(loc, block, EmptyBlock::Collapsed);
                    break (printed, ArrowBody::Block);
                }
            }
        };
        for _ in 0..pushed {
            self.ancestors.pop();
        }

        let should_put_body_on_same_line = match function_body {
            ArrowBody::Block => true,
            ArrowBody::Expression(body) => {
                !self.has_leading_own_line_comment(NodeRef::Expression(body).key(), is_jsx(body))
                    && (matches!(**body, expression::ExpressionInner::Sequence { .. })
                        || self.arrow_body_hugs(body)
                        || (!should_break_chain && should_add_parens_if_not_break(body)))
            }
        };

        let is_callee = matches!(self.role_of(expression), Some((NodeRef::Expression(parent), super::parens::Role::Callee)) if is_call_like(parent));
        let group_id = self.docs.group_id();
        let signatures_doc =
            self.print_arrow_chain_signatures(&signatures, should_break_chain, is_callee, args);

        let mut should_break_before_chain = false;
        let mut is_chain = false;
        let mut add_soft_line = false;
        if should_print_as_chain && (is_callee || args.assignment_layout.is_some()) {
            is_chain = true;
            add_soft_line = !self.has_comment(NodeRef::Expression(expression).key());
            should_break_before_chain = args.assignment_layout == Some(Layout::ChainTailArrowChain)
                || (is_callee && !should_put_body_on_same_line);
        }

        let body = self.print_arrow_body(
            expression,
            body_doc,
            body_comments,
            function_body,
            should_put_body_on_same_line,
            args,
        );

        let head = if is_chain {
            self.indent(self.concat([
                if add_soft_line { &SOFTLINE } else { self.s("") },
                signatures_doc,
            ]))
        } else {
            signatures_doc
        };
        self.group(
            self.concat([
                self.docs
                    .group_with(head, should_break_before_chain, Some(group_id)),
                self.s(" =>"),
                if should_print_as_chain {
                    self.docs.indent_if_break(body, group_id, false)
                } else {
                    self.group(body)
                },
                if should_print_as_chain && is_callee {
                    self.docs.if_break(&SOFTLINE, self.s(""), Some(group_id))
                } else {
                    self.s("")
                },
            ]),
        )
    }

    /// Whether an arrow body can sit on the same line as the `=>`.
    fn arrow_body_hugs(&self, body: &'a Expression) -> bool {
        use expression::ExpressionInner as E;
        matches!(
            **body,
            E::Array { .. } | E::Object { .. } | E::ArrowFunction { .. }
        ) || is_jsx(body)
            || self.is_template_on_own_line(body)
    }

    fn is_template_on_own_line(&self, expression: &'a Expression) -> bool {
        let has_newlines = match &**expression {
            expression::ExpressionInner::TemplateLiteral { inner, .. } => {
                super::call::template_has_newlines(inner)
            }
            expression::ExpressionInner::TaggedTemplate { inner, .. } => {
                super::call::template_has_newlines(&inner.quasi.1)
            }
            _ => return false,
        };
        has_newlines
            && !self
                .text
                .has_newline(self.text.span(expression.loc()).start, true)
    }

    /// `async (params): ret`, with the dangling comment before `=>`.
    fn print_arrow_signature(
        &mut self,
        function: &'a Function,
        key: NodeKey,
        args: PrintArgs,
    ) -> Doc<'a> {
        let mut parts = Vec::new();
        if function.async_ {
            parts.push(self.s("async "));
        }
        let expand_arg = args.expand_last_arg || args.expand_first_arg;
        let mut return_type = self.print_return_type(function);
        if expand_arg {
            if will_break(return_type) {
                self.expansion_bailout = true;
            }
            return_type = self.group(self.docs.remove_lines(return_type));
        }
        let type_params = self.print_optional_type_params(function.tparams.as_ref());
        let parameters = self.print_function_params(function, key, expand_arg, true);
        parts.push(self.group(self.concat([type_params, parameters, return_type])));
        if let Some(dangling) = self.print_dangling_comments(key, Marker::CommentBeforeArrow, false)
        {
            parts.push(self.s(" "));
            parts.push(dangling);
        }
        self.docs.concat_vec(parts)
    }

    /// The signatures of an arrow chain joined by ` =>`.
    fn print_arrow_chain_signatures(
        &mut self,
        signatures: &[Doc<'a>],
        should_break: bool,
        is_callee: bool,
        args: PrintArgs,
    ) -> Doc<'a> {
        if signatures.len() == 1 {
            return signatures[0];
        }
        let parent_is_call =
            matches!(self.parent(), Some(NodeRef::Expression(parent)) if is_call_like(parent));
        let parent_is_binaryish =
            matches!(self.parent(), Some(NodeRef::Expression(parent)) if is_binaryish(parent));
        let separator = self.concat([self.s(" =>"), &LINE]);
        if (!is_callee && parent_is_call) || parent_is_binaryish {
            let rest = self.join(separator, signatures[1..].to_vec());
            return self.docs.group_with(
                self.concat([
                    signatures[0],
                    self.s(" =>"),
                    self.indent(self.concat([&LINE, rest])),
                ]),
                should_break,
                None,
            );
        }
        let joined = self.join(separator, signatures.to_vec());
        if (is_callee && parent_is_call) || args.assignment_layout.is_some() {
            return self.docs.group_with(joined, should_break, None);
        }
        self.docs
            .group_with(self.indent(joined), should_break, None)
    }

    fn print_arrow_body(
        &mut self,
        expression: &'a Expression,
        body_doc: Doc<'a>,
        body_comments: Vec<Doc<'a>>,
        function_body: ArrowBody<'a>,
        should_put_body_on_same_line: bool,
        args: PrintArgs,
    ) -> Doc<'a> {
        let trailing_comma = if args.expand_last_arg {
            self.if_break(self.s(","), self.s(""))
        } else {
            self.s("")
        };
        let in_jsx_container = matches!(self.parent(), Some(NodeRef::JsxExpressionContainer(..)));
        let trailing_space = if (args.expand_last_arg || in_jsx_container)
            && !self.has_comment(NodeRef::Expression(expression).key())
        {
            &SOFTLINE
        } else {
            self.s("")
        };
        let body_comments = self.docs.concat_vec(body_comments);
        // A sequence body always gets parentheses: `(x) => (a, b)`.
        let body_doc = match function_body {
            ArrowBody::Expression(body)
                if matches!(**body, expression::ExpressionInner::Sequence { .. }) =>
            {
                self.group(self.concat([
                    self.s("("),
                    self.indent(self.concat([&SOFTLINE, body_doc])),
                    &SOFTLINE,
                    self.s(")"),
                ]))
            }
            _ => body_doc,
        };
        if should_put_body_on_same_line
            && matches!(function_body, ArrowBody::Expression(body) if should_add_parens_if_not_break(body))
        {
            return self.concat([
                self.s(" "),
                self.group(self.concat([
                    self.if_break(self.s(""), self.s("(")),
                    self.indent(self.concat([&SOFTLINE, body_doc])),
                    self.if_break(self.s(""), self.s(")")),
                    trailing_comma,
                    trailing_space,
                ])),
                body_comments,
            ]);
        }
        if should_put_body_on_same_line {
            return self.concat([self.s(" "), body_doc, body_comments]);
        }
        self.concat([
            self.indent(self.concat([&LINE, body_doc, body_comments])),
            trailing_comma,
            trailing_space,
        ])
    }

    // ---- components and hooks ----

    /// `component Name(params) renders T { body }`.
    pub fn print_component_declaration(
        &mut self,
        component: &'a statement::ComponentDeclaration<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let mut parts = Vec::new();
        if component.async_ {
            parts.push(self.s("async "));
        }
        parts.push(self.s("component "));
        parts.push(self.print_identifier(&component.id));
        parts.push(self.print_optional_type_params(component.tparams.as_ref()));
        let parameters = self.print_component_params(&component.params, key);
        let renders = self.print_renders_annotation(&component.renders);
        parts.push(self.group(self.concat([parameters, renders])));
        if let Some((loc, body)) = &component.body {
            let body = self.print_block(loc, body, EmptyBlock::Collapsed);
            parts.push(self.s(" "));
            parts.push(body);
        }
        self.docs.concat_vec(parts)
    }

    /// `declare component Name(params) renders T;`
    pub fn print_declare_component(
        &mut self,
        component: &'a statement::DeclareComponent<Loc, Loc>,
    ) -> Doc<'a> {
        let key = self
            .current()
            .map_or(NodeRef::Identifier(&component.id).key(), |node| node.key());
        let id = self.print_identifier(&component.id);
        let type_params = self.print_optional_type_params(component.tparams.as_ref());
        let parameters = self.print_component_params(&component.params, key);
        let renders = self.print_renders_annotation(&component.renders);
        self.concat([
            self.s("declare component "),
            id,
            type_params,
            self.group(self.concat([parameters, renders])),
            self.semi(),
        ])
    }

    /// ` renders T`, or nothing.
    pub fn print_renders_annotation(
        &mut self,
        renders: &'a types::ComponentRendersAnnotation<Loc, Loc>,
    ) -> Doc<'a> {
        match renders {
            types::ComponentRendersAnnotation::MissingRenders(_) => self.s(""),
            types::ComponentRendersAnnotation::AvailableRenders(loc, renders) => {
                let printed =
                    self.print_node(NodeRef::Renders(loc, renders), |p| p.print_renders(renders));
                self.concat([self.s(" "), printed])
            }
        }
    }

    /// `renders T`, `renders? T`, `renders* T`.
    pub fn print_renders(&mut self, renders: &'a types::Renders<Loc, Loc>) -> Doc<'a> {
        let keyword = match renders.variant {
            types::RendersVariant::Normal => "renders ",
            types::RendersVariant::Maybe => "renders? ",
            types::RendersVariant::Star => "renders* ",
        };
        let argument = self.print_type(&renders.argument);
        self.concat([self.s(keyword), argument])
    }

    /// `(a: T, 'x' as y: U = 1, ...rest: R)` for a component declaration.
    fn print_component_params(
        &mut self,
        params: &'a statement::component_params::Params<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        let count = params.params.len() + usize::from(params.rest.is_some());
        if count == 0 {
            let dangling = self.print_dangling_comments_where(key, |p, comment| {
                p.text
                    .next_non_space_non_comment_character(comment.span.end)
                    == Some(')')
            });
            return self.concat([self.s("("), dangling.unwrap_or(self.s("")), self.s(")")]);
        }
        let mut printed: Vec<Doc<'a>> = Vec::new();
        for (index, param) in params.params.iter().enumerate() {
            let doc = self.print_node(NodeRef::ComponentParam(param), |p| {
                p.print_component_param(param)
            });
            printed.push(doc);
            if index + 1 < count {
                printed.push(self.s(","));
                if self.text.is_next_line_empty(self.text.span(&param.loc).end) {
                    printed.push(&HARDLINE);
                    printed.push(&HARDLINE);
                } else {
                    printed.push(&LINE);
                }
            }
        }
        if let Some(rest) = &params.rest {
            let doc = self.print_node(NodeRef::ComponentRest(rest), |p| {
                let argument = p.print_pattern(&rest.argument);
                p.concat([p.s("..."), argument])
            });
            printed.push(doc);
        }
        let trailing_comma = if params.rest.is_some() {
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
    }

    fn print_component_param(
        &mut self,
        param: &'a statement::component_params::Param<Loc, Loc>,
    ) -> Doc<'a> {
        let local = self.print_pattern(&param.local);
        let local = match &param.default {
            Some(default) => {
                let value = self.print_expression(default);
                self.concat([local, self.s(" = "), value])
            }
            None => local,
        };
        if param.shorthand {
            return local;
        }
        let name = match &param.name {
            statement::component_params::ParamName::Identifier(id) => self.print_identifier(id),
            statement::component_params::ParamName::StringLiteral((loc, literal)) => {
                self.print_string_node(loc, literal)
            }
        };
        self.concat([name, self.s(" as "), local])
    }

    /// `declare function f(params): T;`
    pub fn print_declare_function(
        &mut self,
        declaration: &'a statement::DeclareFunction<Loc, Loc>,
        key: NodeKey,
    ) -> Doc<'a> {
        self.print_declare_function_inner(declaration, key, !declaration.implicit_declare)
    }

    /// `declare function f(params): T;`, with or without the `declare`
    /// (which `declare export function` spells once, up front).
    pub fn print_declare_function_inner(
        &mut self,
        declaration: &'a statement::DeclareFunction<Loc, Loc>,
        key: NodeKey,
        with_declare: bool,
    ) -> Doc<'a> {
        let _ = key;
        let mut parts = Vec::new();
        if with_declare {
            parts.push(self.s("declare "));
        }
        let is_hook = matches!(&*declaration.annot.annotation, types::TypeInner::Function { inner, .. } if matches!(inner.effect, function::Effect::Hook));
        parts.push(if is_hook {
            self.s("hook ")
        } else {
            self.s("function ")
        });
        if let Some(id) = &declaration.id {
            parts.push(self.print_identifier(id));
        }
        let annotation =
            self.print_node(
                NodeRef::Annotation(&declaration.annot),
                |p| match &*declaration.annot.annotation {
                    types::TypeInner::Function { inner, .. } => {
                        p.print_node(NodeRef::Type(&declaration.annot.annotation), |p| {
                            p.print_function_type(
                                inner,
                                super::types::FunctionTypeStyle::Declaration,
                            )
                        })
                    }
                    _ => {
                        let ty = p.print_type(&declaration.annot.annotation);
                        p.concat([p.s(": "), ty])
                    }
                },
            );
        parts.push(annotation);
        if let Some(predicate) = &declaration.predicate {
            parts.push(self.print_predicate(predicate));
        }
        parts.push(self.semi());
        self.docs.concat_vec(parts)
    }
}

/// What an arrow's body is, once the chain has been walked.
#[derive(Clone, Copy)]
enum ArrowBody<'a> {
    Block,
    Expression(&'a Expression),
}

/// A conditional body gets parentheses when it stays flat:
/// `(a) => (b ? c : d)`.
fn should_add_parens_if_not_break(body: &Expression) -> bool {
    matches!(**body, expression::ExpressionInner::Conditional { .. })
        && !starts_with_no_lookahead_token(body, &|node| {
            matches!(**node, expression::ExpressionInner::Object { .. })
        })
}

/// The function a node is, if it is one.
pub fn function_of_node<'a>(node: NodeRef<'a>) -> Option<&'a Function> {
    match node {
        NodeRef::Statement(statement) => match &**statement {
            statement::StatementInner::FunctionDeclaration { inner, .. } => Some(inner),
            _ => None,
        },
        NodeRef::Expression(expression) => match &**expression {
            expression::ExpressionInner::Function { inner, .. }
            | expression::ExpressionInner::ArrowFunction { inner, .. } => Some(inner),
            _ => None,
        },
        NodeRef::FunctionValue(_, function) => Some(function),
        _ => None,
    }
}
