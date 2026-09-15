//! What a JSX attribute's value is, as far as the source says.
//!
//! Most accessibility questions are about a value rather than a name: whether
//! `aria-hidden` is true, whether an `href` goes anywhere, whether a `title`
//! has words in it. JSX spells one value several ways — `alt="x"`, `alt={"x"}`
//! and ``alt={`x`}`` are the same string, a bare `muted` is `muted={true}`, and
//! `href={undefined}` is no `href` at all — so a rule that read one spelling
//! would be wrong about the others. [`Scope::value`] folds them into one
//! [`Value`], and answers [`Value::Unknown`] for everything the source does not
//! settle, which is where every rule stops.
//!
//! # `undefined` is looked up, not assumed
//!
//! `href={undefined}` renders no `href` only while `undefined` is the global. A
//! parameter, an import or a `const` can take the name, and then the attribute
//! holds whatever that binding holds. The parser's tree carries no resolved
//! bindings, so [`Scope::of`] walks it once for a declaration of that name, and
//! when the module has one, every `undefined` in it is a value the source does
//! not hold. The question is asked of the whole module rather than of the
//! scopes around each use, which is wrong only for a module that both shadows
//! `undefined` and reads the global one — and wrong there in the direction of
//! saying nothing.

use uf_flow::ast::expression::{ExpressionInner, UnaryOperator};
use uf_flow::ast::jsx;
use uf_flow::ast_visitor::{self, AstVisitor};
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
    /// `href={null}`, and `href={undefined}` while `undefined` is the global:
    /// React renders no attribute at all.
    Nullish,
    /// Anything else: a value the module does not hold.
    Unknown,
}

/// How values are read in one module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Scope {
    /// Whether `undefined` in this module is the global.
    undefined_is_global: bool,
}

impl Scope {
    /// The scope of the module `parsed` holds.
    ///
    /// `look` is whether there is anything to look for. A module whose source
    /// never spells `undefined` cannot declare it, and its tree is not walked a
    /// second time.
    pub(super) fn of(parsed: &uf_flow::Parsed, look: bool) -> Self {
        // The finder stops with `Err` at the first declaration it meets.
        let undefined_is_global = !look || UndefinedBinding.program(&parsed.program).is_ok();
        Self {
            undefined_is_global,
        }
    }

    /// The value of `attribute`.
    pub(super) fn value<'a>(self, attribute: &'a jsx::Attribute<Loc, Loc>) -> Value<'a> {
        match &attribute.value {
            None => Value::Bool(true),
            Some(jsx::attribute::Value::StringLiteral((_, literal))) => Value::Text(&literal.value),
            Some(jsx::attribute::Value::ExpressionContainer((_, container))) => {
                match &container.expression {
                    jsx::expression_container::Expression::Expression(expression) => {
                        self.expression_value(expression)
                    }
                    jsx::expression_container::Expression::EmptyExpression => Value::Unknown,
                }
            }
        }
    }

    /// The value of an expression, when it is a literal.
    pub(super) fn expression_value(
        self,
        expression: &ast::expression::Expression<Loc, Loc>,
    ) -> Value<'_> {
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
            ExpressionInner::Identifier { inner, .. }
                if self.undefined_is_global && inner.name == "undefined" =>
            {
                Value::Nullish
            }
            _ => Value::Unknown,
        }
    }

    /// Whether `aria-hidden` takes the element out of the accessibility tree,
    /// or [`None`] when its value is an expression.
    ///
    /// A bare `aria-hidden` and `aria-hidden="true"` both hide it — React
    /// writes the boolean out as the string — and ARIA reads the token without
    /// regard to case. Any other value, `"false"` included, leaves the element
    /// exposed.
    pub(super) fn aria_hidden(self, opening: &jsx::Opening<Loc, Loc>) -> Option<bool> {
        let Some(hidden) = attribute(opening, "aria-hidden") else {
            return Some(false);
        };
        match self.value(hidden) {
            Value::Bool(hidden) => Some(hidden),
            Value::Text(text) => Some(text.trim().eq_ignore_ascii_case("true")),
            Value::Number(_) | Value::Nullish => Some(false),
            Value::Unknown => None,
        }
    }
}

/// Whether a `{...spread}` may decide the attribute called `name`.
///
/// JSX applies props in the order they are written, so a spread after an
/// attribute can replace it and a spread before one cannot; and when the
/// attribute is not written at all, a spread anywhere may supply it.
pub(super) fn spread_may_set(opening: &jsx::Opening<Loc, Loc>, name: &str) -> bool {
    let mut spread_since = false;
    for attribute in &*opening.attributes {
        match attribute {
            jsx::OpeningAttribute::SpreadAttribute(_) => spread_since = true,
            jsx::OpeningAttribute::Attribute(attribute) => {
                if matches!(&attribute.name, jsx::attribute::Name::Identifier(id) if &*id.name == name)
                {
                    spread_since = false;
                }
            }
        }
    }
    spread_since
}

/// Stops at the first declaration of a value called `undefined`.
struct UndefinedBinding;

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for UndefinedBinding {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    /// Every value binding reaches this hook with a `kind` — a declaration, a
    /// parameter, a `catch` clause, an import, and the name of a function, a
    /// class, a component or an enum — and an assignment target reaches it
    /// without one. A type alias is a different hook, because a type does not
    /// shadow a value.
    fn pattern_identifier(
        &mut self,
        kind: Option<ast::VariableKind>,
        ident: &'ast ast::Identifier<Loc, Loc>,
    ) -> Result<(), ()> {
        if kind.is_some() && ident.name == "undefined" {
            return Err(());
        }
        ast_visitor::pattern_identifier_default(self, kind, ident)
    }
}
