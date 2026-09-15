//! What a JSX attribute's value is, as far as the source says.
//!
//! Most accessibility questions are about a value rather than a name: whether
//! `aria-hidden` is true, whether an `href` goes anywhere, whether a `title`
//! has words in it. JSX spells one value several ways — `alt="x"`, `alt={"x"}`
//! and ``alt={`x`}`` are the same string, a bare `muted` is `muted={true}`, and
//! `href={undefined}` is no `href` at all — so a rule that read one spelling
//! would be wrong about the others. [`value`] folds them into one [`Value`],
//! and answers [`Value::Unknown`] for everything the source does not settle,
//! which is where every rule stops.

use uf_flow::ast::expression::{ExpressionInner, UnaryOperator};
use uf_flow::ast::jsx;
use uf_flow::{Loc, ast};

use super::attribute;

/// An attribute's value, when the source settles it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Value<'a> {
    /// `muted`, `muted={true}`, `muted={false}`.
    Bool(bool),
    /// `alt="x"`, `alt={"x"}`, ``alt={`x`}``.
    Text(&'a str),
    /// `tabIndex={0}`, `tabIndex={-1}`.
    Number(f64),
    /// `href={null}`, `href={undefined}`: React renders no attribute at all.
    Nullish,
    /// Anything else: a value the module does not hold.
    Unknown,
}

/// The value of `attribute`.
pub(super) fn value<'a>(attribute: &'a jsx::Attribute<Loc, Loc>) -> Value<'a> {
    match &attribute.value {
        None => Value::Bool(true),
        Some(jsx::attribute::Value::StringLiteral((_, literal))) => Value::Text(&literal.value),
        Some(jsx::attribute::Value::ExpressionContainer((_, container))) => {
            match &container.expression {
                jsx::expression_container::Expression::Expression(expression) => {
                    expression_value(expression)
                }
                jsx::expression_container::Expression::EmptyExpression => Value::Unknown,
            }
        }
    }
}

/// The value of an expression, when it is a literal.
pub(super) fn expression_value(expression: &ast::expression::Expression<Loc, Loc>) -> Value<'_> {
    match &**expression {
        ExpressionInner::StringLiteral { inner, .. } => Value::Text(&inner.value),
        ExpressionInner::TemplateLiteral { inner, .. } if inner.expressions.is_empty() => {
            match &*inner.quasis {
                [only] => Value::Text(&only.value.cooked),
                _ => Value::Unknown,
            }
        }
        ExpressionInner::BooleanLiteral { inner, .. } => Value::Bool(inner.value),
        ExpressionInner::NumberLiteral { inner, .. } => Value::Number(inner.value),
        ExpressionInner::Unary { inner, .. } if inner.operator == UnaryOperator::Minus => {
            match &*inner.argument {
                ExpressionInner::NumberLiteral { inner, .. } => Value::Number(-inner.value),
                _ => Value::Unknown,
            }
        }
        ExpressionInner::NullLiteral { .. } => Value::Nullish,
        ExpressionInner::Identifier { inner, .. } if inner.name == "undefined" => Value::Nullish,
        _ => Value::Unknown,
    }
}

/// Whether `aria-hidden` takes the element out of the accessibility tree, or
/// [`None`] when its value is an expression.
///
/// A bare `aria-hidden` and `aria-hidden="true"` both hide it — React writes
/// the boolean out as the string — and ARIA reads the token without regard to
/// case. Any other value, `"false"` included, leaves the element exposed.
pub(super) fn aria_hidden(opening: &jsx::Opening<Loc, Loc>) -> Option<bool> {
    let Some(hidden) = attribute(opening, "aria-hidden") else {
        return Some(false);
    };
    match value(hidden) {
        Value::Bool(hidden) => Some(hidden),
        Value::Text(text) => Some(text.trim().eq_ignore_ascii_case("true")),
        Value::Number(_) | Value::Nullish => Some(false),
        Value::Unknown => None,
    }
}
