//! Calls, `new`, dynamic `import()`, and their argument lists: Prettier's
//! `printCallExpression` and `printCallArguments`.
//!
//! The argument list is where Prettier's most recognisable layout lives:
//! a last argument that is a function or object is *hugged* — printed
//! expanded while the other arguments stay on the call's line — and only
//! if that fails do all the arguments break out one per line. The hug is a
//! conditional group whose states are tried in order.

use uf_flow::Loc;
use uf_flow::ast::{expression, function, statement};

use super::Printer;
use super::assignment::PrintArgs;
use super::parens::{is_binaryish, is_call, is_call_like, is_jsx, is_member};
use crate::doc::{BREAK_PARENT, Doc, HARDLINE, LINE, LINE_SUFFIX_BOUNDARY, SOFTLINE, will_break};
use crate::flow::comments::{Marker, Placement};
use crate::flow::node::{Expression, NodeRef};

/// The expression inside an argument slot, spread or not.
pub fn argument_expression(argument: &expression::ExpressionOrSpread<Loc, Loc>) -> &Expression {
    match argument {
        expression::ExpressionOrSpread::Expression(expression) => expression,
        expression::ExpressionOrSpread::Spread(spread) => &spread.argument,
    }
}

fn is_spread(argument: &expression::ExpressionOrSpread<Loc, Loc>) -> bool {
    matches!(argument, expression::ExpressionOrSpread::Spread(_))
}

/// The node an argument is attached comments to.
pub fn argument_node<'a>(argument: &'a expression::ExpressionOrSpread<Loc, Loc>) -> NodeRef<'a> {
    match argument {
        expression::ExpressionOrSpread::Expression(expression) => NodeRef::Expression(expression),
        expression::ExpressionOrSpread::Spread(spread) => NodeRef::Spread(spread),
    }
}

/// Whether `expression` is a function or arrow function.
pub fn is_function_or_arrow(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::Function { .. }
            | expression::ExpressionInner::ArrowFunction { .. }
    )
}

fn is_function_or_arrow_with_body(expression: &Expression) -> bool {
    match &**expression {
        expression::ExpressionInner::Function { .. } => true,
        expression::ExpressionInner::ArrowFunction { inner, .. } => {
            matches!(inner.body, function::Body::BodyBlock(_))
        }
        _ => false,
    }
}

fn function_of(expression: &Expression) -> Option<&function::Function<Loc, Loc>> {
    match &**expression {
        expression::ExpressionInner::Function { inner, .. }
        | expression::ExpressionInner::ArrowFunction { inner, .. } => Some(inner),
        _ => None,
    }
}

/// How many parameters a function has, `this` and rest included.
pub fn parameter_count(function: &function::Function<Loc, Loc>) -> usize {
    function.params.params.len()
        + usize::from(function.params.rest.is_some())
        + usize::from(function.params.this_.is_some())
}

fn is_string_literal(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::StringLiteral { .. }
    )
}

fn is_object(expression: &Expression) -> bool {
    matches!(**expression, expression::ExpressionInner::Object { .. })
}

fn is_array(expression: &Expression) -> bool {
    matches!(**expression, expression::ExpressionInner::Array { .. })
}

/// Whether a template literal spans more than one line.
pub fn template_has_newlines(template: &expression::TemplateLiteral<Loc, Loc>) -> bool {
    template
        .quasis
        .iter()
        .any(|quasi| quasi.value.raw.contains('\n'))
}

/// The name of a callee like `a.b.c`, dotted, for matching against known
/// test and require patterns.
fn callee_name(expression: &Expression, out: &mut String) -> bool {
    match &**expression {
        expression::ExpressionInner::Identifier { inner, .. } => {
            out.push_str(&inner.name);
            true
        }
        expression::ExpressionInner::Member { inner, .. } => {
            let expression::member::Property::PropertyIdentifier(property) = &inner.property else {
                return false;
            };
            if !callee_name(&inner.object, out) {
                return false;
            }
            out.push('.');
            out.push_str(&property.name);
            true
        }
        expression::ExpressionInner::MetaProperty { inner, .. } => {
            out.push_str(&inner.meta.name);
            out.push('.');
            out.push_str(&inner.property.name);
            true
        }
        _ => false,
    }
}

const TEST_CALLEES: &[&str] = &[
    "it",
    "it.only",
    "it.skip",
    "describe",
    "describe.only",
    "describe.skip",
    "test",
    "test.only",
    "test.skip",
    "test.fixme",
    "test.step",
    "test.describe",
    "test.describe.only",
    "test.describe.skip",
    "test.describe.fixme",
    "test.describe.parallel",
    "test.describe.parallel.only",
    "test.describe.serial",
    "test.describe.serial.only",
    "skip",
    "xit",
    "xdescribe",
    "xtest",
    "fit",
    "fdescribe",
    "ftest",
];

fn is_angular_test_wrapper(expression: &Expression) -> bool {
    matches!(&**expression, expression::ExpressionInner::Call { inner, .. }
        if matches!(&*inner.callee, expression::ExpressionInner::Identifier { inner: callee, .. }
            if matches!(&*callee.name, "async" | "inject" | "fakeAsync" | "waitForAsync")))
}

fn is_unit_test_set_identifier(expression: &Expression) -> bool {
    matches!(&**expression, expression::ExpressionInner::Identifier { inner, .. }
        if matches!(&*inner.name, "beforeEach" | "beforeAll" | "afterEach" | "afterAll"))
}

/// Prettier's `isTestCall`: `it("name", () => {})` and friends keep their
/// arguments on one line.
pub fn is_test_call(expression: &Expression, parent: Option<&Expression>) -> bool {
    let expression::ExpressionInner::Call { inner, .. } = &**expression else {
        return false;
    };
    let arguments: Vec<&Expression> = inner
        .arguments
        .arguments
        .iter()
        .map(argument_expression)
        .collect();
    if inner.arguments.arguments.iter().any(is_spread) {
        return false;
    }
    if arguments.len() == 1 {
        if is_angular_test_wrapper(expression)
            && parent.is_some_and(|parent| is_test_call(parent, None))
        {
            return is_function_or_arrow(arguments[0]);
        }
        if is_unit_test_set_identifier(&inner.callee) {
            return is_angular_test_wrapper(arguments[0]);
        }
        return false;
    }
    if arguments.len() == 2 || arguments.len() == 3 {
        let first_ok = matches!(
            **arguments[0],
            expression::ExpressionInner::TemplateLiteral { .. }
        ) || is_string_literal(arguments[0]);
        let mut name = String::new();
        if !first_ok
            || !callee_name(&inner.callee, &mut name)
            || !TEST_CALLEES.contains(&name.as_str())
        {
            return false;
        }
        if arguments.len() == 3
            && !matches!(
                **arguments[2],
                expression::ExpressionInner::NumberLiteral { .. }
            )
        {
            return false;
        }
        let second = arguments[1];
        let callback_ok = if arguments.len() == 2 {
            is_function_or_arrow(second)
        } else {
            is_function_or_arrow_with_body(second)
                && function_of(second).is_some_and(|function| parameter_count(function) <= 1)
        };
        return callback_ok || is_angular_test_wrapper(second);
    }
    false
}

/// Prettier's `isSimpleCallArgument`, to depth 2.
pub fn is_simple_call_argument(expression: &Expression, depth: usize) -> bool {
    use expression::ExpressionInner as E;
    if depth == 0 {
        return false;
    }
    let child = |node: &Expression| is_simple_call_argument(node, depth - 1);
    match &**expression {
        E::RegExpLiteral { inner, .. } => inner.pattern.chars().count() <= 5,
        E::StringLiteral { .. }
        | E::BooleanLiteral { .. }
        | E::NullLiteral { .. }
        | E::NumberLiteral { .. }
        | E::BigIntLiteral { .. }
        | E::ModuleRefLiteral { .. }
        | E::Identifier { .. }
        | E::This { .. }
        | E::Super { .. } => true,
        E::TemplateLiteral { inner, .. } => {
            inner
                .quasis
                .iter()
                .all(|quasi| !quasi.value.raw.contains('\n'))
                && inner.expressions.iter().all(&child)
        }
        E::Object { inner, .. } => inner.properties.iter().all(|property| match property {
            expression::object::Property::NormalProperty(property) => match property {
                expression::object::NormalProperty::Init {
                    key,
                    value,
                    shorthand,
                    ..
                } => {
                    !matches!(key, expression::object::Key::Computed(_))
                        && (*shorthand || child(value))
                }
                _ => false,
            },
            expression::object::Property::SpreadProperty(spread) => child(&spread.argument),
        }),
        E::Array { inner, .. } => inner.elements.iter().all(|element| match element {
            expression::ArrayElement::Expression(e) => child(e),
            expression::ArrayElement::Spread(spread) => child(&spread.argument),
            expression::ArrayElement::Hole(_) => true,
        }),
        E::Import { inner, .. } => {
            child(&inner.argument) && inner.options.as_ref().is_none_or(&child)
        }
        E::Call { .. } | E::OptionalCall { .. } | E::New { .. } => {
            let (callee, arguments): (&Expression, &[expression::ExpressionOrSpread<Loc, Loc>]) =
                match &**expression {
                    E::Call { inner, .. } => (&inner.callee, &inner.arguments.arguments),
                    E::OptionalCall { inner, .. } => {
                        (&inner.call.callee, &inner.call.arguments.arguments)
                    }
                    E::New { inner, .. } => (
                        &inner.callee,
                        inner.arguments.as_ref().map_or(&[][..], |a| &a.arguments),
                    ),
                    _ => unreachable!(),
                };
            if is_simple_call_argument(callee, depth) {
                arguments.len() <= depth && arguments.iter().all(|a| child(argument_expression(a)))
            } else {
                false
            }
        }
        E::Member { inner, .. } => {
            is_simple_call_argument(&inner.object, depth)
                && match &inner.property {
                    expression::member::Property::PropertyExpression(property) => {
                        is_simple_call_argument(property, depth)
                    }
                    _ => true,
                }
        }
        E::OptionalMember { inner, .. } => {
            is_simple_call_argument(&inner.member.object, depth)
                && match &inner.member.property {
                    expression::member::Property::PropertyExpression(property) => {
                        is_simple_call_argument(property, depth)
                    }
                    _ => true,
                }
        }
        E::Unary { inner, .. } => {
            matches!(
                inner.operator,
                expression::UnaryOperator::Not
                    | expression::UnaryOperator::Minus
                    | expression::UnaryOperator::Plus
                    | expression::UnaryOperator::BitNot
            ) && is_simple_call_argument(&inner.argument, depth)
        }
        E::Update { inner, .. } => is_simple_call_argument(&inner.argument, depth),
        _ => false,
    }
}

impl<'a> Printer<'a> {
    /// `callee(arguments)`, on top of the ancestor stack.
    pub fn print_call(
        &mut self,
        expression: &'a Expression,
        call: &'a expression::Call<Loc, Loc>,
        optional: bool,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        let arguments = &call.arguments.arguments;
        let optional_token = if optional { self.s("?.") } else { self.s("") };

        // A template literal on its own line, `require("x")`, AMD `define`,
        // and test calls keep their arguments as they are.
        let is_template_on_own_line = arguments.len() == 1
            && self.is_template_on_its_own_line(argument_expression(&arguments[0]));
        let parent_expression = match self.parent() {
            Some(NodeRef::Expression(parent)) => Some(parent),
            _ => None,
        };
        if is_template_on_own_line
            || self.is_require_like_call(call)
            || self.is_commonjs_or_amd_call(call)
            || (!optional && is_test_call(expression, parent_expression))
        {
            let printed: Vec<Doc<'a>> = arguments
                .iter()
                .map(|argument| self.print_argument(argument, PrintArgs::default()))
                .collect();
            let callee = self.print_expression(&call.callee);
            let targs = self.print_optional_call_type_args(call.targs.as_ref());
            let separator = self.s(", ");
            return self.concat([
                callee,
                &LINE_SUFFIX_BOUNDARY,
                optional_token,
                targs,
                self.s("("),
                self.join(separator, printed),
                self.s(")"),
            ]);
        }

        // `(a?.b)()` ends its optional chain at the parentheses, so it is
        // printed as a plain call whose callee carries them, never as a
        // member chain that would spell it `a?.b()`.
        let callee_ends_optional_chain = !optional
            && matches!(
                *call.callee,
                expression::ExpressionInner::OptionalMember { .. }
                    | expression::ExpressionInner::OptionalCall { .. }
            );
        if is_member(&call.callee) && !callee_ends_optional_chain {
            return self.print_member_chain(expression);
        }

        let callee = self.print_expression(&call.callee);
        let targs = self.print_optional_call_type_args(call.targs.as_ref());
        let printed_arguments = self.print_call_arguments(&call.arguments, key, false);
        let contents = self.concat([
            callee,
            &LINE_SUFFIX_BOUNDARY,
            optional_token,
            targs,
            printed_arguments,
        ]);
        if is_call(&call.callee) {
            self.group(contents)
        } else {
            contents
        }
    }

    /// `new Callee(arguments)`.
    pub fn print_new(
        &mut self,
        expression: &'a Expression,
        new: &'a expression::New<Loc, Loc>,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        let callee = self.print_expression(&new.callee);
        let targs = self.print_optional_call_type_args(new.targs.as_ref());
        let arguments = match &new.arguments {
            Some(arguments) => self.print_call_arguments(arguments, key, false),
            None => {
                let dangling = self.print_dangling_comments(key, Marker::None, false);
                self.concat([self.s("("), dangling.unwrap_or(self.s("")), self.s(")")])
            }
        };
        self.concat([
            self.s("new "),
            callee,
            &LINE_SUFFIX_BOUNDARY,
            targs,
            arguments,
        ])
    }

    /// `import("module", options)`.
    pub fn print_dynamic_import(
        &mut self,
        import: &'a expression::Import<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        let source = self.print_expression(&import.argument);
        let mut printed = vec![source];
        if let Some(options) = &import.options {
            printed.push(self.print_expression(options));
        }
        if is_string_literal(&import.argument)
            && !self.has_comment(NodeRef::Expression(&import.argument).key())
            && import.options.is_none()
        {
            let separator = self.s(", ");
            return self.concat([
                self.s("import("),
                self.join(separator, printed),
                self.s(")"),
            ]);
        }
        let _ = key;
        let separator = self.concat([self.s(","), &LINE]);
        let contents = self.concat([
            self.s("("),
            self.indent(self.concat([&SOFTLINE, self.join(separator, printed)])),
            &SOFTLINE,
            self.s(")"),
        ]);
        self.group(self.concat([self.s("import"), contents]))
    }

    /// Type arguments on a call, or nothing.
    pub fn print_optional_call_type_args(
        &mut self,
        targs: Option<&'a expression::CallTypeArgs<Loc, Loc>>,
    ) -> Doc<'a> {
        match targs {
            Some(targs) => {
                let printed = self.print_call_type_args(targs);
                self.concat([printed, &LINE_SUFFIX_BOUNDARY])
            }
            None => self.s(""),
        }
    }

    /// One argument, spread or not.
    pub fn print_argument(
        &mut self,
        argument: &'a expression::ExpressionOrSpread<Loc, Loc>,
        args: PrintArgs,
    ) -> Doc<'a> {
        match argument {
            expression::ExpressionOrSpread::Expression(expression) => {
                self.print_expression_with(expression, args)
            }
            expression::ExpressionOrSpread::Spread(spread) => {
                self.print_node(NodeRef::Spread(spread), |p| {
                    let inner = p.print_expression_with(&spread.argument, args);
                    p.concat([p.s("..."), inner])
                })
            }
        }
    }

    fn is_template_on_its_own_line(&self, expression: &'a Expression) -> bool {
        let has_newlines = match &**expression {
            expression::ExpressionInner::TemplateLiteral { inner, .. } => {
                template_has_newlines(inner)
            }
            expression::ExpressionInner::TaggedTemplate { inner, .. } => {
                template_has_newlines(&inner.quasi.1)
            }
            _ => return false,
        };
        has_newlines
            && !self
                .text
                .has_newline(self.text.span(expression.loc()).start, true)
    }

    /// `require("x")`, `require.resolve("x")`, `import.meta.resolve("x")`
    /// with a single string argument.
    fn is_require_like_call(&self, call: &'a expression::Call<Loc, Loc>) -> bool {
        let mut name = String::new();
        if !callee_name(&call.callee, &mut name) {
            return false;
        }
        if !matches!(
            name.as_str(),
            "require" | "require.resolve" | "require.resolve.paths" | "import.meta.resolve"
        ) {
            return false;
        }
        let arguments = &call.arguments.arguments;
        arguments.len() == 1
            && !is_spread(&arguments[0])
            && is_string_literal(argument_expression(&arguments[0]))
            && !self.has_comment(argument_node(&arguments[0]).key())
    }

    fn is_commonjs_or_amd_call(&self, call: &'a expression::Call<Loc, Loc>) -> bool {
        let expression::ExpressionInner::Identifier { inner: callee, .. } = &*call.callee else {
            return false;
        };
        let arguments = &call.arguments.arguments;
        let first = arguments.first().map(argument_expression);
        match &*callee.name {
            "require" => {
                let Some(first) = first else {
                    return false;
                };
                ((arguments.len() == 1 && is_string_literal(first)) || arguments.len() > 1)
                    && !self.has_comment(argument_node(&arguments[0]).key())
            }
            "define" => {
                let in_statement = matches!(self.parent(), Some(NodeRef::Statement(statement))
                    if matches!(**statement, statement::StatementInner::Expression { .. }));
                if !in_statement {
                    return false;
                }
                match arguments.len() {
                    1 => true,
                    2 => first.is_some_and(is_array),
                    3 => {
                        first.is_some_and(is_string_literal)
                            && is_array(argument_expression(&arguments[1]))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The `(...)` of a call. `key` is the call node, which owns dangling
    /// comments in an empty list.
    pub fn print_call_arguments(
        &mut self,
        list: &'a expression::ArgList<Loc, Loc>,
        key: crate::flow::node::NodeKey,
        is_dynamic_import: bool,
    ) -> Doc<'a> {
        let arguments = &list.arguments;
        if arguments.is_empty() {
            let dangling = self.print_dangling_comments(key, Marker::None, false);
            return self.group(self.concat([
                self.s("("),
                dangling.unwrap_or(self.s("")),
                self.s(")"),
            ]));
        }

        let expressions: Vec<&'a Expression> = arguments.iter().map(argument_expression).collect();
        let last_index = arguments.len() - 1;

        // useEffect(() => { ... }, [foo, bar, baz])
        if self.is_react_hook_call_with_deps_array(arguments) {
            let mut parts = vec![self.s("(")];
            for (index, argument) in arguments.iter().enumerate() {
                parts.push(self.print_argument(argument, PrintArgs::default()));
                if index != last_index {
                    parts.push(self.s(", "));
                }
            }
            parts.push(self.s(")"));
            return self.docs.concat_vec(parts);
        }

        let mut any_arg_empty_line = false;
        let mut printed_arguments: Vec<Doc<'a>> = Vec::with_capacity(arguments.len());
        for (index, argument) in arguments.iter().enumerate() {
            let printed = self.print_argument(argument, PrintArgs::default());
            if index == last_index {
                printed_arguments.push(printed);
            } else {
                let end = self.text.span(&argument_node(argument).loc()).end;
                if self.text.is_next_line_empty(end) {
                    any_arg_empty_line = true;
                    printed_arguments.push(self.concat([
                        printed,
                        self.s(","),
                        &HARDLINE,
                        &HARDLINE,
                    ]));
                } else {
                    printed_arguments.push(self.concat([printed, self.s(","), &LINE]));
                }
            }
        }

        let trailing_comma = if is_dynamic_import {
            self.s("")
        } else {
            self.if_break(self.s(","), self.s(""))
        };
        let all_args_broken_out = |p: &mut Self, printed: &[Doc<'a>]| -> Doc<'a> {
            let inner = p.docs.concat(printed.iter().copied());
            p.docs.group_with(
                p.concat([
                    p.s("("),
                    p.indent(p.concat([&LINE, inner])),
                    trailing_comma,
                    &LINE,
                    p.s(")"),
                ]),
                true,
                None,
            )
        };

        let in_decorator = matches!(self.parent(), Some(NodeRef::Decorator(_)));
        if any_arg_empty_line || (!in_decorator && is_function_composition_args(&expressions)) {
            return all_args_broken_out(self, &printed_arguments);
        }

        if self.should_group_first(&expressions, arguments) {
            let tail: Vec<Doc<'a>> = printed_arguments[1..].to_vec();
            if tail.iter().any(|part| will_break(part)) {
                return all_args_broken_out(self, &printed_arguments);
            }
            self.expansion_bailout = false;
            let expanded = self.print_argument(
                &arguments[0],
                PrintArgs {
                    expand_first_arg: true,
                    ..PrintArgs::default()
                },
            );
            if self.expansion_bailout {
                self.expansion_bailout = false;
                return all_args_broken_out(self, &printed_arguments);
            }
            let tail_doc = self.docs.concat(tail.iter().copied());
            let hugged = self.concat([
                self.s("("),
                self.docs.group_with(expanded, true, None),
                self.s(", "),
                tail_doc,
                self.s(")"),
            ]);
            let broken = all_args_broken_out(self, &printed_arguments);
            if will_break(expanded) {
                return self.concat([
                    &BREAK_PARENT,
                    self.docs.conditional_group(&[hugged, broken], false),
                ]);
            }
            let flat = self.concat([self.s("("), expanded, self.s(", "), tail_doc, self.s(")")]);
            return self.docs.conditional_group(&[flat, hugged, broken], false);
        }

        if self.should_group_last(&expressions, arguments, &printed_arguments) {
            let head: Vec<Doc<'a>> = printed_arguments[..last_index].to_vec();
            if head.iter().any(|part| will_break(part)) {
                return all_args_broken_out(self, &printed_arguments);
            }
            self.expansion_bailout = false;
            let expanded = self.print_argument(
                &arguments[last_index],
                PrintArgs {
                    expand_last_arg: true,
                    ..PrintArgs::default()
                },
            );
            if self.expansion_bailout {
                self.expansion_bailout = false;
                return all_args_broken_out(self, &printed_arguments);
            }
            let head_doc = self.docs.concat(head.iter().copied());
            let hugged = self.concat([
                self.s("("),
                head_doc,
                self.docs.group_with(expanded, true, None),
                self.s(")"),
            ]);
            let broken = all_args_broken_out(self, &printed_arguments);
            if will_break(expanded) {
                return self.concat([
                    &BREAK_PARENT,
                    self.docs.conditional_group(&[hugged, broken], false),
                ]);
            }
            let flat = self.concat([self.s("("), head_doc, expanded, self.s(")")]);
            return self.docs.conditional_group(&[flat, hugged, broken], false);
        }

        let inner = self.docs.concat(printed_arguments.iter().copied());
        let contents = self.concat([
            self.s("("),
            self.indent(self.concat([&SOFTLINE, inner])),
            trailing_comma,
            &SOFTLINE,
            self.s(")"),
        ]);
        if self.is_long_curried_call_chain() {
            return contents;
        }
        let should_break =
            printed_arguments.iter().any(|part| will_break(part)) || any_arg_empty_line;
        self.docs.group_with(contents, should_break, None)
    }

    /// `useEffect(() => {}, [deps])`: two arguments, a parameterless arrow
    /// with a block body and an array, none with comments.
    fn is_react_hook_call_with_deps_array(
        &self,
        arguments: &'a [expression::ExpressionOrSpread<Loc, Loc>],
    ) -> bool {
        let check = |index: usize| -> bool {
            let (Some(first), Some(second)) = (arguments.get(index), arguments.get(index + 1))
            else {
                return false;
            };
            if is_spread(first) || is_spread(second) {
                return false;
            }
            let first_ok = matches!(&**argument_expression(first), expression::ExpressionInner::ArrowFunction { inner, .. }
                if parameter_count(inner) == 0 && matches!(inner.body, function::Body::BodyBlock(_)));
            first_ok
                && is_array(argument_expression(second))
                && arguments
                    .iter()
                    .all(|argument| !self.has_comment(argument_node(argument).key()))
        };
        match arguments.len() {
            2 => check(0),
            3 => {
                matches!(
                    **argument_expression(&arguments[0]),
                    expression::ExpressionInner::Identifier { .. }
                ) && check(1)
            }
            _ => false,
        }
    }

    fn should_group_first(
        &self,
        expressions: &[&'a Expression],
        arguments: &'a [expression::ExpressionOrSpread<Loc, Loc>],
    ) -> bool {
        if expressions.len() != 2 || is_spread(&arguments[0]) || is_spread(&arguments[1]) {
            return false;
        }
        let (first, second) = (expressions[0], expressions[1]);
        let first_ok = match &**first {
            expression::ExpressionInner::Function { .. } => true,
            expression::ExpressionInner::ArrowFunction { inner, .. } => {
                matches!(inner.body, function::Body::BodyBlock(_))
            }
            _ => false,
        };
        !self.has_comment(NodeRef::Expression(first).key())
            && first_ok
            && !matches!(
                **second,
                expression::ExpressionInner::Function { .. }
                    | expression::ExpressionInner::ArrowFunction { .. }
                    | expression::ExpressionInner::Conditional { .. }
            )
            && self.is_hopefully_short_call_argument(second)
            && !self.could_expand_arg(second, false)
    }

    fn should_group_last(
        &self,
        expressions: &[&'a Expression],
        arguments: &'a [expression::ExpressionOrSpread<Loc, Loc>],
        _printed: &[Doc<'a>],
    ) -> bool {
        let last_index = expressions.len() - 1;
        let last = expressions[last_index];
        let last_key = argument_node(&arguments[last_index]).key();
        let penultimate = if last_index > 0 {
            Some(expressions[last_index - 1])
        } else {
            None
        };
        !self.has_comment_placed(last_key, Placement::Leading)
            && !self.has_comment_placed(last_key, Placement::Trailing)
            && self.could_expand_arg(last, false)
            // If the last two arguments are of the same type, disregard
            // hugging neither of them.
            && penultimate.is_none_or(|penultimate| {
                std::mem::discriminant(&**penultimate) != std::mem::discriminant(&**last)
            })
            // useMemo(() => func, [foo, bar, baz])
            && (expressions.len() != 2
                || !matches!(**expressions[0], expression::ExpressionInner::ArrowFunction { .. })
                || !is_array(last))
            && !(expressions.len() > 1 && is_array(last) && self.is_concisely_printed_array(last))
    }

    /// Prettier's `couldExpandArg`.
    pub fn could_expand_arg(&self, argument: &'a Expression, arrow_chain_recursion: bool) -> bool {
        use expression::ExpressionInner as E;
        let key = NodeRef::Expression(argument).key();
        match &**argument {
            E::Object { inner, .. } => !inner.properties.is_empty() || self.has_comment(key),
            E::Array { inner, .. } => !inner.elements.is_empty() || self.has_comment(key),
            E::AsExpression { inner, .. } => self.could_expand_arg(&inner.expression, false),
            E::AsConstExpression { inner, .. } => self.could_expand_arg(&inner.expression, false),
            E::TSSatisfies { inner, .. } => self.could_expand_arg(&inner.expression, false),
            E::Function { .. } => true,
            E::ArrowFunction { inner, .. } => match &inner.body {
                function::Body::BodyBlock(_) => true,
                function::Body::BodyExpression(body) => {
                    is_jsx(body)
                        || is_object(body)
                        || is_array(body)
                        || matches!(**body, E::ArrowFunction { .. })
                            && self.could_expand_arg(body, true)
                        || (!arrow_chain_recursion
                            && (is_call(body) || matches!(**body, E::Conditional { .. })))
                }
            },
            _ => false,
        }
    }

    /// Prettier's `isHopefullyShortCallArgument`.
    fn is_hopefully_short_call_argument(&self, argument: &'a Expression) -> bool {
        use expression::ExpressionInner as E;
        match &**argument {
            E::TypeCast { inner, .. } => {
                self.is_simple_type(&inner.annot.annotation)
                    && is_simple_call_argument(&inner.expression, 1)
            }
            E::AsExpression { inner, .. } => {
                self.is_simple_type(&inner.annot.annotation)
                    && is_simple_call_argument(&inner.expression, 1)
            }
            E::AsConstExpression { inner, .. } => is_simple_call_argument(&inner.expression, 1),
            E::TSSatisfies { inner, .. } => {
                self.is_simple_type(&inner.annot.annotation)
                    && is_simple_call_argument(&inner.expression, 1)
            }
            _ => {
                if is_call_like(argument) && call_argument_count(argument) > 1 {
                    return false;
                }
                if is_binaryish(argument) {
                    let (left, right) = match &**argument {
                        E::Binary { inner, .. } => (&inner.left, &inner.right),
                        E::Logical { inner, .. } => (&inner.left, &inner.right),
                        _ => unreachable!(),
                    };
                    return is_simple_call_argument(left, 1) && is_simple_call_argument(right, 1);
                }
                matches!(**argument, E::RegExpLiteral { .. })
                    || is_simple_call_argument(argument, 2)
            }
        }
    }

    /// Whether a type is a keyword, literal, or unparameterised reference:
    /// Prettier's `isSimpleType`.
    pub fn is_simple_type(&self, ty: &'a uf_flow::ast::types::Type<Loc, Loc>) -> bool {
        use uf_flow::ast::types::TypeInner as T;
        let mut ty = ty;
        if let T::Array { inner, .. } = &**ty {
            ty = &inner.argument;
            if let T::Array { inner, .. } = &**ty {
                ty = &inner.argument;
            }
        }
        if let T::Generic { inner, .. } = &**ty
            && let Some(targs) = &inner.targs
            && targs.arguments.len() == 1
        {
            ty = &targs.arguments[0];
        }
        match &**ty {
            T::Any { .. }
            | T::Mixed { .. }
            | T::Empty { .. }
            | T::Void { .. }
            | T::Null { .. }
            | T::Number { .. }
            | T::BigInt { .. }
            | T::String { .. }
            | T::Boolean { .. }
            | T::Symbol { .. }
            | T::Exists { .. }
            | T::StringLiteral { .. }
            | T::NumberLiteral { .. }
            | T::BigIntLiteral { .. }
            | T::BooleanLiteral { .. }
            | T::Unknown { .. }
            | T::Never { .. }
            | T::Undefined { .. }
            | T::UniqueSymbol { .. } => true,
            T::Generic { inner, .. } => inner.targs.is_none(),
            _ => false,
        }
    }

    /// `a(b)(c)(d)`: a call whose callee is a call with more arguments than
    /// itself. Prettier's `isLongCurriedCallChain`.
    fn is_long_curried_call_chain(&self) -> bool {
        let Some(NodeRef::Expression(node)) = self.current() else {
            return false;
        };
        let Some(NodeRef::Expression(parent)) = self.parent() else {
            return false;
        };
        let (Some(node_count), Some(parent_count)) = (call_arguments(node), call_arguments(parent))
        else {
            return false;
        };
        let is_callee = match &**parent {
            expression::ExpressionInner::Call { inner, .. } => {
                super::parens::same(&inner.callee, node)
            }
            expression::ExpressionInner::OptionalCall { inner, .. } => {
                super::parens::same(&inner.call.callee, node)
            }
            _ => false,
        };
        is_callee && parent_count > 0 && node_count > parent_count
    }
}

/// How many arguments a call-like expression has.
pub fn argument_expression_count(expression: &Expression) -> usize {
    call_argument_count(expression)
}

/// The argument count of a call, if `expression` is one.
fn call_arguments(expression: &Expression) -> Option<usize> {
    match &**expression {
        expression::ExpressionInner::Call { inner, .. } => Some(inner.arguments.arguments.len()),
        expression::ExpressionInner::OptionalCall { inner, .. } => {
            Some(inner.call.arguments.arguments.len())
        }
        _ => None,
    }
}

fn call_argument_count(expression: &Expression) -> usize {
    match &**expression {
        expression::ExpressionInner::New { inner, .. } => {
            inner.arguments.as_ref().map_or(0, |a| a.arguments.len())
        }
        expression::ExpressionInner::Import { inner, .. } => {
            1 + usize::from(inner.options.is_some())
        }
        _ => call_arguments(expression).unwrap_or(0),
    }
}

/// Prettier's `isFunctionCompositionArgs`: more than one function
/// argument, or a call argument that itself takes a function.
fn is_function_composition_args(arguments: &[&Expression]) -> bool {
    if arguments.len() <= 1 {
        return false;
    }
    let mut count = 0;
    for argument in arguments {
        if is_function_or_arrow(argument) {
            count += 1;
            if count > 1 {
                return true;
            }
        } else if is_call_like(argument) {
            let inner_arguments: Vec<&Expression> = match &***argument {
                expression::ExpressionInner::Call { inner, .. } => inner
                    .arguments
                    .arguments
                    .iter()
                    .map(argument_expression)
                    .collect(),
                expression::ExpressionInner::OptionalCall { inner, .. } => inner
                    .call
                    .arguments
                    .arguments
                    .iter()
                    .map(argument_expression)
                    .collect(),
                expression::ExpressionInner::New { inner, .. } => inner
                    .arguments
                    .as_ref()
                    .map(|a| a.arguments.iter().map(argument_expression).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            if inner_arguments
                .iter()
                .any(|child| is_function_or_arrow(child))
            {
                return true;
            }
        }
    }
    false
}
