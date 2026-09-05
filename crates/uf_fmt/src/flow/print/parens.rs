//! Which expressions need parentheses where they stand: Prettier's
//! `needs-parens.js`.
//!
//! The parser threw the source's parentheses away, so every pair in the
//! output is decided here from the shape of the tree alone. That is what
//! keeps the printer honest: a pair that the grammar requires is added, one
//! that only the author liked is not, and either way the printed program
//! parses back to the same tree. The rules are precedence for binary
//! operators, the statement-start tokens (`{`, `function`, `class`, `let[`)
//! that would be read as something else, and a set of readability
//! parentheses Prettier adds on purpose (`(a ?? b) || c`, `a % (b + c)`,
//! `(await x).y`).

use uf_flow::ast::{expression, statement};

use super::Printer;
use crate::flow::node::{Expression, NodeRef};

/// The slot a child expression occupies in its parent, where the slot
/// changes whether parentheses are needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The callee of a call or `new`.
    Callee,
    /// The object of a member access.
    Object,
    /// The left operand of a binary or logical expression.
    Left,
    /// The right operand of a binary or logical expression.
    Right,
    /// The test of a conditional.
    Test,
    /// A branch of a conditional.
    Branch,
    /// The body of an arrow function.
    ArrowBody,
    /// The expression of an expression statement.
    Statement,
    /// The superclass of a class.
    SuperClass,
    /// The declaration of `export default`.
    ExportDefault,
    /// The `init` or `update` of a `for` head.
    ForHead,
    /// The expression of a decorator.
    Decorator,
    /// The left side of an assignment.
    AssignmentLeft,
    /// The argument of a unary, spread, await, or similar.
    Argument,
    /// Anywhere else.
    Other,
}

/// Whether two expressions are the same node.
pub fn same(a: &Expression, b: &Expression) -> bool {
    std::ptr::eq(&**a, &**b)
}

/// Whether `expression` is a binary or logical expression.
pub fn is_binaryish(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::Binary { .. } | expression::ExpressionInner::Logical { .. }
    )
}

/// Whether `expression` is a call, optional or not, or `new`.
pub fn is_call_like(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::Call { .. }
            | expression::ExpressionInner::OptionalCall { .. }
            | expression::ExpressionInner::New { .. }
            | expression::ExpressionInner::Import { .. }
    )
}

/// Whether `expression` is a member access, optional or not.
pub fn is_member(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::Member { .. }
            | expression::ExpressionInner::OptionalMember { .. }
    )
}

/// Whether `expression` is a call, optional or not, but not `new`.
pub fn is_call(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::Call { .. } | expression::ExpressionInner::OptionalCall { .. }
    )
}

/// Whether `expression` is an `as` or `satisfies` cast.
pub fn is_binary_cast(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::AsExpression { .. }
            | expression::ExpressionInner::AsConstExpression { .. }
            | expression::ExpressionInner::TSSatisfies { .. }
    )
}

/// Whether `expression` is a JSX element or fragment.
pub fn is_jsx(expression: &Expression) -> bool {
    matches!(
        **expression,
        expression::ExpressionInner::JSXElement { .. }
            | expression::ExpressionInner::JSXFragment { .. }
    )
}

/// Whether `expression` starts with something other than its own token:
/// Prettier's `hasNakedLeftSide`.
pub fn has_naked_left_side(expression: &Expression) -> bool {
    use expression::ExpressionInner as E;
    match &**expression {
        E::Assignment { .. }
        | E::Binary { .. }
        | E::Logical { .. }
        | E::Conditional { .. }
        | E::Call { .. }
        | E::OptionalCall { .. }
        | E::New { .. }
        | E::Member { .. }
        | E::OptionalMember { .. }
        | E::Sequence { .. }
        | E::TaggedTemplate { .. }
        | E::AsExpression { .. }
        | E::AsConstExpression { .. }
        | E::TSSatisfies { .. } => true,
        E::Update { inner, .. } => !inner.prefix,
        _ => false,
    }
}

/// The expression `expression` starts with: Prettier's `getLeftSide`.
pub fn left_side(expression: &Expression) -> Option<&Expression> {
    use expression::ExpressionInner as E;
    match &**expression {
        E::Sequence { inner, .. } => inner.expressions.first(),
        E::Assignment { inner, .. } => match &inner.left {
            uf_flow::ast::pattern::Pattern::Expression { inner, .. } => Some(inner),
            _ => None,
        },
        E::Binary { inner, .. } => Some(&inner.left),
        E::Logical { inner, .. } => Some(&inner.left),
        E::Conditional { inner, .. } => Some(&inner.test),
        E::Call { inner, .. } => Some(&inner.callee),
        E::OptionalCall { inner, .. } => Some(&inner.call.callee),
        E::New { inner, .. } => Some(&inner.callee),
        E::Member { inner, .. } => Some(&inner.object),
        E::OptionalMember { inner, .. } => Some(&inner.member.object),
        E::TaggedTemplate { inner, .. } => Some(&inner.tag),
        E::Update { inner, .. } if !inner.prefix => Some(&inner.argument),
        E::AsExpression { inner, .. } => Some(&inner.expression),
        E::AsConstExpression { inner, .. } => Some(&inner.expression),
        E::TSSatisfies { inner, .. } => Some(&inner.expression),
        _ => None,
    }
}

/// Identifiers that begin a declaration when they begin a statement.
///
/// Every one of them is an ordinary identifier anywhere else, which is why
/// the check has to be about position rather than spelling.
const STATEMENT_KEYWORDS: [&str; 9] = [
    "await",
    "component",
    "declare",
    "hook",
    "interface",
    "let",
    "module",
    "type",
    "using",
];

/// Whether the first token of `expression` is one `forbidden` rejects:
/// Prettier's `startsWithNoLookaheadToken`.
pub fn starts_with_no_lookahead_token(
    expression: &Expression,
    forbidden: &dyn Fn(&Expression) -> bool,
) -> bool {
    use expression::ExpressionInner as E;
    match &**expression {
        E::Binary { inner, .. } => starts_with_no_lookahead_token(&inner.left, forbidden),
        E::Logical { inner, .. } => starts_with_no_lookahead_token(&inner.left, forbidden),
        E::Assignment { inner, .. } => match &inner.left {
            uf_flow::ast::pattern::Pattern::Expression { inner, .. } => {
                starts_with_no_lookahead_token(inner, forbidden)
            }
            _ => false,
        },
        E::Member { inner, .. } => starts_with_no_lookahead_token(&inner.object, forbidden),
        E::OptionalMember { inner, .. } => {
            starts_with_no_lookahead_token(&inner.member.object, forbidden)
        }
        E::TaggedTemplate { inner, .. } => {
            if matches!(*inner.tag, E::Function { .. }) {
                return false;
            }
            starts_with_no_lookahead_token(&inner.tag, forbidden)
        }
        E::Call { inner, .. } => {
            if matches!(*inner.callee, E::Function { .. }) {
                return false;
            }
            starts_with_no_lookahead_token(&inner.callee, forbidden)
        }
        E::OptionalCall { inner, .. } => {
            if matches!(*inner.call.callee, E::Function { .. }) {
                return false;
            }
            starts_with_no_lookahead_token(&inner.call.callee, forbidden)
        }
        E::Conditional { inner, .. } => starts_with_no_lookahead_token(&inner.test, forbidden),
        E::Update { inner, .. } => {
            !inner.prefix && starts_with_no_lookahead_token(&inner.argument, forbidden)
        }
        E::Sequence { inner, .. } => inner
            .expressions
            .first()
            .is_some_and(|first| starts_with_no_lookahead_token(first, forbidden)),
        E::AsExpression { inner, .. } => {
            starts_with_no_lookahead_token(&inner.expression, forbidden)
        }
        E::AsConstExpression { inner, .. } => {
            starts_with_no_lookahead_token(&inner.expression, forbidden)
        }
        E::TSSatisfies { inner, .. } => {
            starts_with_no_lookahead_token(&inner.expression, forbidden)
        }
        _ => forbidden(expression),
    }
}

/// Binding strength of a binary or logical operator; higher binds tighter.
pub fn precedence(operator: &str) -> u8 {
    match operator {
        "|>" => 0,
        "??" => 1,
        "||" => 2,
        "&&" => 3,
        "|" => 4,
        "^" => 5,
        "&" => 6,
        "==" | "===" | "!=" | "!==" => 7,
        "<" | ">" | "<=" | ">=" | "in" | "instanceof" => 8,
        ">>" | "<<" | ">>>" => 9,
        "+" | "-" => 10,
        "*" | "/" | "%" => 11,
        "**" => 12,
        _ => 13,
    }
}

fn is_equality(operator: &str) -> bool {
    matches!(operator, "==" | "===" | "!=" | "!==")
}

fn is_multiplicative(operator: &str) -> bool {
    matches!(operator, "*" | "/" | "%")
}

fn is_bitshift(operator: &str) -> bool {
    matches!(operator, ">>" | ">>>" | "<<")
}

/// Whether `operator` is a bitwise operator, which Prettier always
/// parenthesizes inside another operator for readability.
pub fn is_bitwise(operator: &str) -> bool {
    is_bitshift(operator) || matches!(operator, "|" | "^" | "&")
}

/// Whether `a op b op c` can be printed as one flat chain: Prettier's
/// `shouldFlatten`.
pub fn should_flatten(parent_operator: &str, child_operator: &str) -> bool {
    if precedence(child_operator) != precedence(parent_operator) {
        return false;
    }
    if parent_operator == "**" {
        return false;
    }
    if is_equality(parent_operator) && is_equality(child_operator) {
        return false;
    }
    if (child_operator == "%" && is_multiplicative(parent_operator))
        || (parent_operator == "%" && is_multiplicative(child_operator))
    {
        return false;
    }
    if child_operator != parent_operator
        && is_multiplicative(child_operator)
        && is_multiplicative(parent_operator)
    {
        return false;
    }
    if is_bitshift(parent_operator) && is_bitshift(child_operator) {
        return false;
    }
    true
}

/// The operator of a binary or logical expression.
pub fn binaryish_operator(expression: &Expression) -> Option<&'static str> {
    match &**expression {
        expression::ExpressionInner::Binary { inner, .. } => Some(inner.operator.as_str()),
        expression::ExpressionInner::Logical { inner, .. } => {
            Some(logical_operator(&inner.operator))
        }
        _ => None,
    }
}

/// The spelling of a logical operator.
pub fn logical_operator(operator: &expression::LogicalOperator) -> &'static str {
    match operator {
        expression::LogicalOperator::Or => "||",
        expression::LogicalOperator::And => "&&",
        expression::LogicalOperator::NullishCoalesce => "??",
    }
}

impl<'a> Printer<'a> {
    /// The parent of `child` and the slot it occupies there. `child` must
    /// be the node on top of the ancestor stack.
    pub fn role_of(&self, child: &'a Expression) -> Option<(NodeRef<'a>, Role)> {
        use expression::ExpressionInner as E;
        let parent = self.parent()?;
        let role = match parent {
            NodeRef::Expression(parent_expression) => match &**parent_expression {
                E::Call { inner, .. } if same(&inner.callee, child) => Role::Callee,
                E::OptionalCall { inner, .. } if same(&inner.call.callee, child) => Role::Callee,
                E::New { inner, .. } if same(&inner.callee, child) => Role::Callee,
                E::Member { inner, .. } if same(&inner.object, child) => Role::Object,
                E::OptionalMember { inner, .. } if same(&inner.member.object, child) => {
                    Role::Object
                }
                E::Binary { inner, .. } => {
                    if same(&inner.left, child) {
                        Role::Left
                    } else {
                        Role::Right
                    }
                }
                E::Logical { inner, .. } => {
                    if same(&inner.left, child) {
                        Role::Left
                    } else {
                        Role::Right
                    }
                }
                E::Conditional { inner, .. } => {
                    if same(&inner.test, child) {
                        Role::Test
                    } else {
                        Role::Branch
                    }
                }
                E::ArrowFunction { inner, .. } => match &inner.body {
                    uf_flow::ast::function::Body::BodyExpression(body) if same(body, child) => {
                        Role::ArrowBody
                    }
                    _ => Role::Other,
                },
                E::Class { inner, .. }
                    if inner
                        .extends
                        .as_ref()
                        .is_some_and(|extends| same(&extends.expr, child)) =>
                {
                    Role::SuperClass
                }
                E::Assignment { inner, .. } => match &inner.left {
                    uf_flow::ast::pattern::Pattern::Expression { inner: left, .. }
                        if same(left, child) =>
                    {
                        Role::AssignmentLeft
                    }
                    _ => Role::Other,
                },
                E::Unary { .. } | E::Update { .. } | E::Yield { .. } => Role::Argument,
                E::TaggedTemplate { inner, .. } if same(&inner.tag, child) => Role::Callee,
                _ => Role::Other,
            },
            NodeRef::Statement(parent_statement) => match &**parent_statement {
                statement::StatementInner::Expression { inner, .. }
                    if same(&inner.expression, child) =>
                {
                    Role::Statement
                }
                statement::StatementInner::ExportDefaultDeclaration { inner, .. } => {
                    match &inner.declaration {
                        statement::export_default_declaration::Declaration::Expression(e)
                            if same(e, child) =>
                        {
                            Role::ExportDefault
                        }
                        _ => Role::Other,
                    }
                }
                statement::StatementInner::ClassDeclaration { inner, .. }
                    if inner
                        .extends
                        .as_ref()
                        .is_some_and(|extends| same(&extends.expr, child)) =>
                {
                    Role::SuperClass
                }
                statement::StatementInner::For { inner, .. } => {
                    let is_init = matches!(&inner.init, Some(statement::for_::Init::InitExpression(e)) if same(e, child));
                    let is_update = inner.update.as_ref().is_some_and(|e| same(e, child));
                    if is_init || is_update {
                        Role::ForHead
                    } else {
                        Role::Other
                    }
                }
                _ => Role::Other,
            },
            NodeRef::Spread(_) | NodeRef::JsxSpreadAttribute(_) => Role::Argument,
            NodeRef::Decorator(_) => Role::Decorator,
            _ => Role::Other,
        };
        Some((parent, role))
    }

    /// Whether `expression`, on top of the ancestor stack, must be
    /// parenthesized.
    pub fn needs_parens(&self, expression: &'a Expression) -> bool {
        use expression::ExpressionInner as E;
        let Some((parent, role)) = self.role_of(expression) else {
            return false;
        };

        // A bare identifier never needs parentheses, except when it is one
        // of Flow's contextual keywords, it is the first token of an
        // expression statement, and the token after it is another
        // identifier.
        //
        // `type` is an ordinary name — `obj.type`, `{type: 'x'}` — right up
        // until it opens a statement *and is followed by a name*, where the
        // parser commits to a type alias and wants a `=`. So this, from
        // React's devtools:
        //
        //     (type) as empty;
        //
        // stops parsing the moment the parentheses are dropped.
        //
        // The `as` is the whole reason it can happen. Every other way an
        // expression continues — `hook.renderers`, `type(x)`, `let = 1` —
        // puts punctuation after the identifier, and the parser gives up on
        // the declaration and reads an expression. `as`, `as const` and
        // `satisfies` are the only operators spelled as identifiers, so
        // they are the only ones that can be mistaken for the name in
        // `type Name = …`.
        //
        // Asking only "is it leftmost" was too broad and CI caught it:
        // `hook.renderers.forEach(…)` in this repository's own
        // `refresh-runtime.js` became `(hook).renderers.forEach(…)`.
        //
        // `role == Role::Statement` is excluded for the opposite reason:
        // the identifier is the whole statement, `type;` is unambiguous,
        // and parenthesizing it would be noise.
        if let E::Identifier { inner, .. } = &**expression {
            return STATEMENT_KEYWORDS.contains(&&*inner.name)
                && matches!(
                    parent,
                    NodeRef::Expression(parent)
                        if matches!(
                            &**parent,
                            E::AsExpression { .. }
                                | E::AsConstExpression { .. }
                                | E::TSSatisfies { .. }
                        )
                )
                && self.is_leftmost_of_expression_statement();
        }

        // Rules keyed on the parent.
        match (parent, role) {
            (_, Role::SuperClass) => {
                if matches!(
                    &**expression,
                    E::ArrowFunction { .. }
                        | E::Assignment { .. }
                        | E::Binary { .. }
                        | E::Conditional { .. }
                        | E::Logical { .. }
                        | E::New { .. }
                        | E::Object { .. }
                        | E::Sequence { .. }
                        | E::TaggedTemplate { .. }
                        | E::Unary { .. }
                        | E::Update { .. }
                        | E::Yield { .. }
                ) {
                    return true;
                }
                if let E::Class { inner, .. } = &**expression
                    && !inner.class_decorators.is_empty()
                {
                    return true;
                }
            }
            (_, Role::ExportDefault) => {
                return self.should_wrap_function_for_export_default(expression)
                    || matches!(**expression, E::Sequence { .. });
            }
            (_, Role::Decorator) => {
                return decorator_needs_parens(expression);
            }
            (_, Role::Statement) => {
                // Handled by `is_leftmost_of_expression_statement` below,
                // which parenthesizes the offending token itself rather
                // than the whole statement.
            }
            (_, Role::ArrowBody) => {
                if !matches!(**expression, E::Sequence { .. })
                    && starts_with_no_lookahead_token(expression, &|node| {
                        matches!(**node, E::Object { .. })
                    })
                {
                    return true;
                }
            }
            (NodeRef::Expression(parent_expression), Role::Left) => {
                if let E::Binary { inner, .. } = &**parent_expression
                    && matches!(
                        inner.operator,
                        expression::BinaryOperator::In | expression::BinaryOperator::Instanceof
                    )
                    && matches!(**expression, E::Unary { .. })
                {
                    return true;
                }
            }
            _ => {}
        }

        let parent_expression = match parent {
            NodeRef::Expression(parent_expression) => Some(&**parent_expression),
            _ => None,
        };

        // `{`, `function` and `class` cannot open an expression
        // statement, so whichever of them is the first token gets the
        // parentheses.
        if matches!(
            **expression,
            E::Object { .. } | E::Function { .. } | E::Class { .. }
        ) && self.is_leftmost_of_expression_statement()
        {
            return true;
        }

        // `(a?.b)()` is not `a?.b()`: the parentheses end the optional
        // chain, and dropping them would change what the program does.
        if matches!(
            **expression,
            E::OptionalMember { .. } | E::OptionalCall { .. }
        ) && matches!(role, Role::Callee | Role::Object)
            && matches!(parent, NodeRef::Expression(parent_expression)
                if matches!(**parent_expression, E::Call { .. } | E::Member { .. } | E::New { .. }))
        {
            return true;
        }

        match &**expression {
            E::Update { inner, .. } => {
                if let Some(E::Unary {
                    inner: parent_unary,
                    ..
                }) = parent_expression
                {
                    let same_sign = matches!(
                        (&inner.operator, &parent_unary.operator),
                        (
                            expression::UpdateOperator::Increment,
                            expression::UnaryOperator::Plus
                        ) | (
                            expression::UpdateOperator::Decrement,
                            expression::UnaryOperator::Minus
                        )
                    );
                    return inner.prefix && same_sign;
                }
                unary_needs_parens(parent_expression, role, None)
            }
            E::Unary { inner, .. }
                if !matches!(inner.operator, expression::UnaryOperator::Await) =>
            {
                if let Some(E::Unary {
                    inner: parent_unary,
                    ..
                }) = parent_expression
                {
                    let same = std::mem::discriminant(&inner.operator)
                        == std::mem::discriminant(&parent_unary.operator);
                    return same
                        && matches!(
                            inner.operator,
                            expression::UnaryOperator::Plus | expression::UnaryOperator::Minus
                        );
                }
                unary_needs_parens(parent_expression, role, Some(&inner.operator))
            }
            E::Binary { inner, .. } => {
                if matches!(parent_expression, Some(E::Update { .. })) {
                    return true;
                }
                if matches!(inner.operator, expression::BinaryOperator::In)
                    && self.is_in_for_statement_initializer()
                {
                    return true;
                }
                self.binaryish_needs_parens(expression, parent_expression, role)
            }
            E::Logical { .. }
            | E::AsExpression { .. }
            | E::AsConstExpression { .. }
            | E::TSSatisfies { .. } => {
                self.binaryish_needs_parens(expression, parent_expression, role)
            }
            E::Sequence { .. } => match parent {
                NodeRef::Statement(statement) => match &**statement {
                    statement::StatementInner::Return { .. }
                    | statement::StatementInner::For { .. } => false,
                    statement::StatementInner::Expression { .. } => role != Role::Statement,
                    _ => true,
                },
                NodeRef::Expression(parent_expression) => {
                    if let E::ArrowFunction { .. } = &**parent_expression {
                        role != Role::ArrowBody
                    } else {
                        true
                    }
                }
                _ => true,
            },
            E::Yield { .. } | E::Unary { .. } => {
                // `yield` and `await`.
                if matches!(**expression, E::Yield { .. }) && is_await_parent(parent_expression) {
                    return true;
                }
                match parent_expression {
                    Some(E::TaggedTemplate { .. })
                    | Some(E::Unary { .. })
                    | Some(E::Logical { .. })
                    | Some(E::AsExpression { .. })
                    | Some(E::AsConstExpression { .. })
                    | Some(E::TSSatisfies { .. }) => true,
                    Some(E::Member { .. }) | Some(E::OptionalMember { .. }) => role == Role::Object,
                    Some(E::New { .. }) | Some(E::Call { .. }) | Some(E::OptionalCall { .. }) => {
                        role == Role::Callee
                    }
                    Some(E::Conditional { .. }) => role == Role::Test,
                    Some(E::Binary { .. }) => true,
                    _ => matches!(parent, NodeRef::Spread(_) | NodeRef::JsxSpreadAttribute(_)),
                }
            }
            E::StringLiteral { .. } => {
                // A string at the start of a statement would become a
                // directive.
                if role == Role::Statement {
                    let is_directive = matches!(parent, NodeRef::Statement(statement) if matches!(&**statement, statement::StatementInner::Expression { inner, .. } if inner.directive.is_some()));
                    if !is_directive {
                        return matches!(
                            self.grandparent(),
                            Some(NodeRef::Program(_)) | Some(NodeRef::Block(..))
                        ) || matches!(self.grandparent(), Some(NodeRef::Statement(s)) if matches!(**s, statement::StatementInner::Block { .. }));
                    }
                }
                false
            }
            E::NumberLiteral { .. } => role == Role::Object,
            E::Assignment { inner, .. } => {
                if role == Role::ArrowBody {
                    return true;
                }
                if role == Role::ForHead {
                    return false;
                }
                if role == Role::Statement {
                    return matches!(inner.left, uf_flow::ast::pattern::Pattern::Object { .. });
                }
                match parent {
                    NodeRef::Expression(parent_expression) => match &**parent_expression {
                        E::Assignment { .. } => false,
                        E::Sequence { .. } => !matches!(
                            self.grandparent(),
                            Some(NodeRef::Statement(statement))
                                if matches!(**statement, statement::StatementInner::For { .. })
                        ),
                        _ => true,
                    },
                    // A default value in a destructuring pattern.
                    NodeRef::PatternProperty(_)
                    | NodeRef::PatternElement(_)
                    | NodeRef::Param(_) => false,
                    NodeRef::ComponentParam(_) => false,
                    _ => true,
                }
            }
            E::Conditional { .. } => match parent_expression {
                Some(E::TaggedTemplate { .. })
                | Some(E::Unary { .. })
                | Some(E::Binary { .. })
                | Some(E::Logical { .. })
                | Some(E::TypeCast { .. })
                | Some(E::AsExpression { .. })
                | Some(E::AsConstExpression { .. })
                | Some(E::TSSatisfies { .. }) => true,
                Some(E::New { .. }) | Some(E::Call { .. }) | Some(E::OptionalCall { .. }) => {
                    role == Role::Callee
                }
                Some(E::Conditional { .. }) => role == Role::Test,
                Some(E::Member { .. }) | Some(E::OptionalMember { .. }) => role == Role::Object,
                _ => {
                    matches!(parent, NodeRef::Spread(_) | NodeRef::JsxSpreadAttribute(_))
                        || role == Role::ExportDefault
                }
            },
            E::Function { .. } => match parent_expression {
                Some(E::Call { .. }) | Some(E::OptionalCall { .. }) | Some(E::New { .. }) => {
                    role == Role::Callee
                }
                Some(E::TaggedTemplate { .. }) => true,
                _ => false,
            },
            E::ArrowFunction { .. } => match parent_expression {
                Some(E::Binary { .. }) => true,
                Some(E::New { .. }) | Some(E::Call { .. }) | Some(E::OptionalCall { .. }) => {
                    role == Role::Callee
                }
                Some(E::Member { .. }) | Some(E::OptionalMember { .. }) => role == Role::Object,
                Some(E::AsExpression { .. })
                | Some(E::AsConstExpression { .. })
                | Some(E::TSSatisfies { .. })
                | Some(E::TaggedTemplate { .. })
                | Some(E::Unary { .. })
                | Some(E::Logical { .. }) => true,
                Some(E::Conditional { .. }) => role == Role::Test,
                _ => false,
            },
            E::Class { inner, .. } => {
                if !inner.class_decorators.is_empty() {
                    return true;
                }
                matches!(parent_expression, Some(E::New { .. })) && role == Role::Callee
            }
            E::OptionalMember { .. }
            | E::OptionalCall { .. }
            | E::Call { .. }
            | E::Member { .. }
            | E::TaggedTemplate { .. } => {
                if role == Role::Callee && matches!(parent_expression, Some(E::New { .. })) {
                    let mut object = expression;
                    loop {
                        match &**object {
                            E::Call { .. } | E::OptionalCall { .. } => return true,
                            E::Member { inner, .. } => object = &inner.object,
                            E::OptionalMember { inner, .. } => object = &inner.member.object,
                            E::TaggedTemplate { inner, .. } => object = &inner.tag,
                            _ => return false,
                        }
                    }
                }
                false
            }
            E::JSXFragment { .. } | E::JSXElement { .. } => {
                if role == Role::Callee {
                    return true;
                }
                if role == Role::Left
                    && matches!(parent_expression, Some(E::Binary { inner, .. }) if matches!(inner.operator, expression::BinaryOperator::LessThan))
                {
                    return true;
                }
                match parent {
                    NodeRef::Expression(parent_expression) => !matches!(
                        &**parent_expression,
                        E::Array { .. }
                            | E::ArrowFunction { .. }
                            | E::Assignment { .. }
                            | E::Binary { .. }
                            | E::New { .. }
                            | E::Conditional { .. }
                            | E::JSXElement { .. }
                            | E::JSXFragment { .. }
                            | E::Logical { .. }
                            | E::Call { .. }
                            | E::OptionalCall { .. }
                            | E::TypeCast { .. }
                            | E::Yield { .. }
                            | E::Match { .. }
                    ),
                    NodeRef::Statement(statement) => !matches!(
                        &**statement,
                        statement::StatementInner::Expression { .. }
                            | statement::StatementInner::Return { .. }
                            | statement::StatementInner::Throw { .. }
                    ),
                    NodeRef::Spread(_)
                    | NodeRef::ObjectProperty(_)
                    | NodeRef::Declarator(_)
                    | NodeRef::JsxAttribute(_)
                    | NodeRef::JsxExpressionContainer(..)
                    | NodeRef::JsxChild(_)
                    | NodeRef::Param(_)
                    | NodeRef::PatternProperty(_)
                    | NodeRef::PatternElement(_)
                    | NodeRef::MatchExpressionCase(_)
                    | NodeRef::ClassMember(_)
                    | NodeRef::ComponentParam(_) => false,
                    _ => true,
                }
            }
            _ => false,
        }
    }

    /// Prettier's `shouldWrapFunctionForExportDefault`: `export default`
    /// followed by an expression that *starts* with a function or class
    /// must be parenthesized, or the keyword would claim the declaration.
    ///
    /// The exception is a function that is already parenthesized by its own
    /// position — `export default (function () {})();` — so the walk down
    /// the left side asks, at each step, whether the node it came from puts
    /// parentheses around a function there.
    fn should_wrap_function_for_export_default(&self, expression: &'a Expression) -> bool {
        use expression::ExpressionInner as E;
        let mut node = expression;
        let mut parent: Option<&'a Expression> = None;
        loop {
            if matches!(**node, E::Function { .. } | E::Class { .. }) {
                return match parent {
                    None => true,
                    Some(parent) => !parenthesizes_function_child(parent, node),
                };
            }
            if !has_naked_left_side(node) {
                return false;
            }
            match left_side(node) {
                Some(left) => {
                    parent = Some(node);
                    node = left;
                }
                None => return false,
            }
        }
    }

    fn binaryish_needs_parens(
        &self,
        expression: &'a Expression,
        parent_expression: Option<&'a expression::ExpressionInner<uf_flow::Loc, uf_flow::Loc>>,
        role: Role,
    ) -> bool {
        use expression::ExpressionInner as E;
        let Some(parent_expression) = parent_expression else {
            return matches!(
                self.parent(),
                Some(NodeRef::Spread(_)) | Some(NodeRef::JsxSpreadAttribute(_))
            );
        };
        match parent_expression {
            E::AsExpression { .. } | E::AsConstExpression { .. } | E::TSSatisfies { .. } => {
                !is_binary_cast(expression)
            }
            E::Conditional { .. } => is_binary_cast(expression),
            E::Call { .. } | E::New { .. } | E::OptionalCall { .. } => role == Role::Callee,
            E::Class { .. } => role == Role::SuperClass,
            E::TaggedTemplate { .. } | E::Unary { .. } | E::Update { .. } => true,
            E::Member { .. } | E::OptionalMember { .. } => role == Role::Object,
            E::Assignment { .. } => role == Role::AssignmentLeft && is_binary_cast(expression),
            E::Logical {
                inner: parent_logical,
                ..
            } => {
                // `a && b || c` is printed `(a && b) || c`: a logical
                // operator inside a different one is always parenthesized,
                // however the precedences fall.
                if let E::Logical { inner, .. } = &**expression {
                    return std::mem::discriminant(&parent_logical.operator)
                        != std::mem::discriminant(&inner.operator);
                }
                self.binaryish_in_binaryish(
                    expression,
                    logical_operator(&parent_logical.operator),
                    role,
                )
            }
            E::Binary {
                inner: parent_binary,
                ..
            } => self.binaryish_in_binaryish(expression, parent_binary.operator.as_str(), role),
            _ => false,
        }
    }

    fn binaryish_in_binaryish(
        &self,
        expression: &'a Expression,
        parent_operator: &str,
        role: Role,
    ) -> bool {
        let Some(operator) = binaryish_operator(expression) else {
            // A cast inside a binary expression.
            return true;
        };
        self.operator_needs_parens(parent_operator, operator, role)
    }

    fn operator_needs_parens(&self, parent_operator: &str, operator: &str, role: Role) -> bool {
        let precedence_here = precedence(operator);
        let parent_precedence = precedence(parent_operator);
        if parent_precedence > precedence_here {
            return true;
        }
        if role == Role::Right && parent_precedence == precedence_here {
            return true;
        }
        if parent_precedence == precedence_here && !should_flatten(parent_operator, operator) {
            return true;
        }
        if parent_precedence < precedence_here && operator == "%" {
            return matches!(parent_operator, "+" | "-");
        }
        if is_bitwise(parent_operator) {
            return true;
        }
        false
    }

    /// Whether the expression on top of the stack is the first token of
    /// an expression statement, following the left side of every
    /// expression it is nested in.
    fn is_leftmost_of_expression_statement(&self) -> bool {
        let mut child = match self.current() {
            Some(NodeRef::Expression(expression)) => expression,
            _ => return false,
        };
        for ancestor in self.ancestors.iter().rev().skip(1) {
            match ancestor {
                // The member-chain printer pushes a call twice.
                NodeRef::Expression(parent) if same(parent, child) => {}
                NodeRef::Expression(parent) => match left_side(parent) {
                    Some(left) if same(left, child) => child = parent,
                    _ => return false,
                },
                NodeRef::Statement(statement) => {
                    return matches!(&***statement, statement::StatementInner::Expression { inner, .. }
                        if same(&inner.expression, child));
                }
                _ => return false,
            }
        }
        false
    }

    /// Whether the current expression sits in the `init` of a `for`
    /// statement, where an `in` operator must be parenthesized.
    fn is_in_for_statement_initializer(&self) -> bool {
        let mut child: Option<NodeRef<'a>> = self.current();
        for ancestor in self.ancestors.iter().rev().skip(1) {
            match ancestor {
                NodeRef::Statement(statement) => {
                    if let statement::StatementInner::For { inner, .. } = &***statement {
                        let in_init = match (&inner.init, child) {
                            (
                                Some(statement::for_::Init::InitExpression(init)),
                                Some(NodeRef::Expression(c)),
                            ) => same(init, c),
                            (
                                Some(statement::for_::Init::InitDeclaration(_)),
                                Some(NodeRef::Declarator(_)),
                            ) => true,
                            _ => false,
                        };
                        return in_init;
                    }
                    return false;
                }
                NodeRef::Expression(_)
                | NodeRef::Declarator(_)
                | NodeRef::Pattern(_)
                | NodeRef::Spread(_) => {
                    child = Some(*ancestor);
                }
                _ => return false,
            }
        }
        false
    }
}

fn is_await_parent(
    parent: Option<&expression::ExpressionInner<uf_flow::Loc, uf_flow::Loc>>,
) -> bool {
    matches!(parent, Some(expression::ExpressionInner::Unary { inner, .. }) if matches!(inner.operator, expression::UnaryOperator::Await))
}

fn unary_needs_parens(
    parent: Option<&expression::ExpressionInner<uf_flow::Loc, uf_flow::Loc>>,
    role: Role,
    _operator: Option<&expression::UnaryOperator>,
) -> bool {
    use expression::ExpressionInner as E;
    match parent {
        Some(E::Member { .. }) | Some(E::OptionalMember { .. }) => role == Role::Object,
        Some(E::TaggedTemplate { .. }) => true,
        Some(E::New { .. }) | Some(E::Call { .. }) | Some(E::OptionalCall { .. }) => {
            role == Role::Callee
        }
        Some(E::Binary { inner, .. }) => {
            role == Role::Left && matches!(inner.operator, expression::BinaryOperator::Exp)
        }
        _ => false,
    }
}

/// A decorator expression needs parentheses unless it is a plain chain of
/// member accesses ending in at most one call.
fn decorator_needs_parens(expression: &Expression) -> bool {
    use expression::ExpressionInner as E;
    let mut has_call = false;
    let mut has_member = false;
    let mut current = expression;
    loop {
        match &**current {
            E::Member { inner, .. } => {
                has_member = true;
                current = &inner.object;
            }
            E::Call { inner, .. } => {
                if has_member || has_call {
                    return true;
                }
                has_call = true;
                current = &inner.callee;
            }
            E::Identifier { .. } => return false,
            _ => return true,
        }
    }
}

/// Whether `parent` puts parentheses around a function or class in the
/// position `child` occupies: the callee of a call or `new`, or the tag of
/// a tagged template.
fn parenthesizes_function_child(parent: &Expression, child: &Expression) -> bool {
    use expression::ExpressionInner as E;
    match &**parent {
        E::Call { inner, .. } => same(&inner.callee, child),
        E::OptionalCall { inner, .. } => same(&inner.call.callee, child),
        E::New { inner, .. } => same(&inner.callee, child),
        E::TaggedTemplate { inner, .. } => same(&inner.tag, child),
        _ => false,
    }
}
