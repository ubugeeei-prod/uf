//! Arrays, array patterns and tuple types: Prettier's `printArray`.
//!
//! Arrays of numbers are *filled* — as many per line as fit — and an
//! array whose elements are all non-trivial objects or arrays breaks one
//! per line regardless of width.

use uf_flow::Loc;
use uf_flow::ast::{expression, pattern, types};

use super::Printer;
use crate::doc::{Doc, HARDLINE, LINE, SOFTLINE};
use crate::flow::comments::{Marker, Placement};
use crate::flow::node::{Expression, NodeKey, NodeRef};

/// What kind of bracketed list is being printed.
#[derive(Clone, Copy)]
pub enum ArrayKind<'a> {
    /// An array literal.
    Expression(&'a expression::Array<Loc, Loc>),
    /// A destructuring pattern.
    Pattern(&'a pattern::Array<Loc, Loc>),
    /// A tuple type.
    Tuple(&'a types::Tuple<Loc, Loc>),
}

/// One element to print, or a hole.
enum Element<'a> {
    Node(NodeRef<'a>),
    Hole,
}

fn is_signed_number(expression: &Expression) -> bool {
    matches!(&**expression, expression::ExpressionInner::Unary { inner, .. }
        if matches!(inner.operator, expression::UnaryOperator::Plus | expression::UnaryOperator::Minus)
            && matches!(*inner.argument, expression::ExpressionInner::NumberLiteral { .. }))
}

impl<'a> Printer<'a> {
    /// `[a, b, ...c]`.
    pub fn print_array(
        &mut self,
        array: &'a expression::Array<Loc, Loc>,
        expression: &'a Expression,
    ) -> Doc<'a> {
        let key = NodeRef::Expression(expression).key();
        self.print_array_like(ArrayKind::Expression(array), key)
    }

    /// `[a, , b = 1, ...rest]: T`.
    pub fn print_array_pattern(
        &mut self,
        array: &'a pattern::Array<Loc, Loc>,
        pattern: &'a pattern::Pattern<Loc, Loc>,
    ) -> Doc<'a> {
        let key = NodeRef::Pattern(pattern).key();
        let printed = self.print_array_like(ArrayKind::Pattern(array), key);
        let optional = if array.optional {
            self.s("?")
        } else {
            self.s("")
        };
        let annotation = self.print_optional_annotation(&array.annot);
        self.concat([printed, optional, annotation])
    }

    /// `[A, b: B, ...C]`.
    pub fn print_tuple_type(
        &mut self,
        tuple: &'a types::Tuple<Loc, Loc>,
        ty: &'a types::Type<Loc, Loc>,
    ) -> Doc<'a> {
        let key = NodeRef::Type(ty).key();
        self.print_array_like(ArrayKind::Tuple(tuple), key)
    }

    fn elements_of(&self, kind: ArrayKind<'a>) -> Vec<Element<'a>> {
        match kind {
            ArrayKind::Expression(array) => array
                .elements
                .iter()
                .map(|element| match element {
                    expression::ArrayElement::Expression(expression) => {
                        Element::Node(NodeRef::Expression(expression))
                    }
                    expression::ArrayElement::Spread(spread) => {
                        Element::Node(NodeRef::Spread(spread))
                    }
                    expression::ArrayElement::Hole(_) => Element::Hole,
                })
                .collect(),
            ArrayKind::Pattern(array) => array
                .elements
                .iter()
                .map(|element| match element {
                    pattern::array::Element::NormalElement(element)
                        if element.default.is_some() =>
                    {
                        Element::Node(NodeRef::PatternElement(element))
                    }
                    pattern::array::Element::NormalElement(element) => {
                        Element::Node(NodeRef::Pattern(&element.argument))
                    }
                    pattern::array::Element::RestElement(rest) => {
                        Element::Node(NodeRef::PatternRest(rest))
                    }
                    pattern::array::Element::Hole(_) => Element::Hole,
                })
                .collect(),
            ArrayKind::Tuple(tuple) => tuple
                .elements
                .iter()
                .map(|element| match element {
                    types::tuple::Element::UnlabeledElement { annot, .. } => {
                        Element::Node(NodeRef::Type(annot))
                    }
                    _ => Element::Node(NodeRef::TupleElement(element)),
                })
                .collect(),
        }
    }

    fn print_element(
        &mut self,
        kind: ArrayKind<'a>,
        index: usize,
        element: &Element<'a>,
    ) -> Doc<'a> {
        match (kind, element) {
            (_, Element::Hole) => self.s(""),
            (ArrayKind::Expression(_), Element::Node(NodeRef::Expression(expression))) => {
                self.print_expression(expression)
            }
            (ArrayKind::Expression(_), Element::Node(NodeRef::Spread(spread))) => {
                self.print_node(NodeRef::Spread(spread), |p| {
                    let argument = p.print_expression(&spread.argument);
                    p.concat([p.s("..."), argument])
                })
            }
            (ArrayKind::Pattern(_), Element::Node(NodeRef::Pattern(pattern))) => {
                self.print_pattern(pattern)
            }
            (ArrayKind::Pattern(_), Element::Node(NodeRef::PatternElement(element))) => self
                .print_node(NodeRef::PatternElement(element), |p| {
                    let argument = p.print_pattern(&element.argument);
                    match &element.default {
                        Some(default) => {
                            let value = p.print_expression(default);
                            p.concat([argument, p.s(" = "), value])
                        }
                        None => argument,
                    }
                }),
            (ArrayKind::Pattern(_), Element::Node(NodeRef::PatternRest(rest))) => {
                self.print_node(NodeRef::PatternRest(rest), |p| {
                    let argument = p.print_pattern(&rest.argument);
                    p.concat([p.s("..."), argument])
                })
            }
            (ArrayKind::Tuple(tuple), Element::Node(NodeRef::Type(ty))) => {
                let printed = self.print_type(ty);
                let optional = matches!(
                    tuple.elements.get(index),
                    Some(types::tuple::Element::UnlabeledElement { optional: true, .. })
                );
                if optional {
                    self.concat([printed, self.s("?")])
                } else {
                    printed
                }
            }
            (ArrayKind::Tuple(_), Element::Node(NodeRef::TupleElement(element))) => {
                self.print_tuple_element(element)
            }
            _ => self.s(""),
        }
    }

    fn print_array_like(&mut self, kind: ArrayKind<'a>, key: NodeKey) -> Doc<'a> {
        let elements = self.elements_of(kind);
        let inexact = matches!(kind, ArrayKind::Tuple(tuple) if tuple.inexact);
        if elements.is_empty() && !inexact {
            // As for objects, a comment inside empty brackets breaks them.
            return match self.print_dangling_comments(key, Marker::None, false) {
                Some(dangling) => self.concat([
                    self.s("["),
                    self.indent(self.concat([&HARDLINE, dangling])),
                    &HARDLINE,
                    self.s("]"),
                ]),
                None => self.s("[]"),
            };
        }

        // Only a *pattern's* rest: `const [a, ...rest,] = xs` is a syntax
        // error, so nothing may follow it. A spread anywhere else is an
        // ordinary last element and takes the comma like one — `[...a, ...b,]`
        // in an array literal, and `[...[number, number],]` in a tuple type,
        // which is how StyleX writes `Matrix3d`'s sixteen arguments.
        let last_is_rest = matches!(
            elements.last(),
            Some(Element::Node(NodeRef::PatternRest(_)))
        );
        let can_have_trailing_comma = !last_is_rest && !inexact;
        let last_is_hole = matches!(elements.last(), Some(Element::Hole));
        let group_id = self.docs.group_id();

        // An array of objects or arrays, all with more than one entry, is
        // always expanded.
        let should_break = elements.len() > 1
            && elements.iter().all(|element| {
                let Element::Node(NodeRef::Expression(expression)) = element else {
                    return false;
                };
                match &***expression {
                    expression::ExpressionInner::Object { inner, .. } => inner.properties.len() > 1,
                    expression::ExpressionInner::Array { inner, .. } => inner.elements.len() > 1,
                    _ => false,
                }
            })
            && {
                let first = match elements.first() {
                    Some(Element::Node(NodeRef::Expression(expression))) => {
                        Some(std::mem::discriminant(&***expression))
                    }
                    _ => None,
                };
                elements.iter().all(|element| matches!(element, Element::Node(NodeRef::Expression(expression)) if Some(std::mem::discriminant(&***expression)) == first))
            }
            || self.has_line_comment(key, Some(Placement::Dangling));

        let concise = matches!(kind, ArrayKind::Expression(array) if self.is_concisely_printed_array_elements(array));
        let trailing_comma = if !can_have_trailing_comma {
            self.s("")
        } else if last_is_hole {
            self.s(",")
        } else if concise {
            self.docs.if_break(self.s(","), self.s(""), Some(group_id))
        } else {
            self.if_break(self.s(","), self.s(""))
        };

        let items = if concise {
            self.print_array_items_concisely(kind, &elements, trailing_comma)
        } else {
            let items = self.print_array_items(kind, &elements, inexact);
            self.concat([items, trailing_comma])
        };
        let dangling = self.print_dangling_comments(key, Marker::None, false);
        let content = self.concat([
            self.s("["),
            self.indent(self.concat([&SOFTLINE, items, dangling.unwrap_or(self.s(""))])),
            &SOFTLINE,
            self.s("]"),
        ]);
        self.docs.group_with(content, should_break, Some(group_id))
    }

    /// Whether a blank line follows the element (looking past its comma).
    fn is_next_line_empty_after_element(&self, node: NodeRef<'a>) -> bool {
        let span = self.text.span(&node.loc());
        if span.end == span.start {
            return false;
        }
        let bytes = self.text.text().as_bytes();
        let mut at = span.end;
        while at < bytes.len() && bytes[at] != b',' {
            let Some(next) = self.text.next_non_space_non_comment_index(at + 1) else {
                return false;
            };
            if next <= at {
                break;
            }
            at = next;
        }
        self.text.is_next_line_empty(at)
    }

    fn print_array_items(
        &mut self,
        kind: ArrayKind<'a>,
        elements: &[Element<'a>],
        inexact: bool,
    ) -> Doc<'a> {
        let mut parts = Vec::with_capacity(elements.len() * 2);
        let last = elements.len().saturating_sub(1);
        for (index, element) in elements.iter().enumerate() {
            let printed = self.print_element(kind, index, element);
            parts.push(match element {
                Element::Hole => self.s(""),
                Element::Node(_) => self.group(printed),
            });
            if index != last || inexact {
                let blank = match element {
                    Element::Node(node) if self.is_next_line_empty_after_element(*node) => {
                        &SOFTLINE
                    }
                    _ => self.s(""),
                };
                parts.push(self.concat([self.s(","), &LINE, blank]));
            }
        }
        if inexact {
            parts.push(self.s("..."));
        }
        self.docs.concat_vec(parts)
    }

    fn print_array_items_concisely(
        &mut self,
        kind: ArrayKind<'a>,
        elements: &[Element<'a>],
        trailing_comma: Doc<'a>,
    ) -> Doc<'a> {
        let mut parts = Vec::with_capacity(elements.len() * 2);
        let last = elements.len().saturating_sub(1);
        for (index, element) in elements.iter().enumerate() {
            let printed = self.print_element(kind, index, element);
            let is_last = index == last;
            parts.push(self.concat([printed, if is_last { trailing_comma } else { self.s(",") }]));
            if !is_last {
                let node = match element {
                    Element::Node(node) => Some(*node),
                    Element::Hole => None,
                };
                let next_has_line_comment = match elements.get(index + 1) {
                    Some(Element::Node(next)) => {
                        self.has_comment_where(next.key(), Some(Placement::Leading), |comment| {
                            matches!(comment.kind, uf_flow::ast::CommentKind::Line)
                        })
                    }
                    _ => false,
                };
                parts.push(
                    if node.is_some_and(|node| self.is_next_line_empty_after_element(node)) {
                        self.concat([&HARDLINE, &HARDLINE])
                    } else if next_has_line_comment {
                        &HARDLINE
                    } else {
                        &LINE
                    },
                );
            }
        }
        self.docs.fill(parts)
    }

    /// Whether every element is a number (or a signed number) without a
    /// trailing line comment, so the array is filled rather than broken.
    pub fn is_concisely_printed_array(&self, expression: &'a Expression) -> bool {
        match &**expression {
            expression::ExpressionInner::Array { inner, .. } => {
                self.is_concisely_printed_array_elements(inner)
            }
            _ => false,
        }
    }

    fn is_concisely_printed_array_elements(&self, array: &'a expression::Array<Loc, Loc>) -> bool {
        !array.elements.is_empty()
            && array.elements.iter().all(|element| {
                let expression::ArrayElement::Expression(expression) = element else {
                    return false;
                };
                let is_number = matches!(
                    **expression,
                    expression::ExpressionInner::NumberLiteral { .. }
                ) || (is_signed_number(expression)
                    && !matches!(&**expression, expression::ExpressionInner::Unary { inner, .. }
                            if self.has_comment(NodeRef::Expression(&inner.argument).key())));
                is_number
                    && !self.has_comment_where(
                        NodeRef::Expression(expression).key(),
                        Some(Placement::Trailing),
                        |comment| {
                            matches!(comment.kind, uf_flow::ast::CommentKind::Line)
                                && !self.text.has_newline(comment.span.start, true)
                        },
                    )
            })
    }

    /// `name?: T`, `...name: T`, `+name: T` in a tuple type.
    fn print_tuple_element(&mut self, element: &'a types::tuple::Element<Loc, Loc>) -> Doc<'a> {
        self.print_node(NodeRef::TupleElement(element), |p| match element {
            types::tuple::Element::UnlabeledElement {
                annot, optional, ..
            } => {
                let printed = p.print_type(annot);
                if *optional {
                    p.concat([printed, p.s("?")])
                } else {
                    printed
                }
            }
            types::tuple::Element::LabeledElement { element, .. } => {
                let variance = match &element.variance {
                    Some(variance) => p.print_variance(variance),
                    None => p.s(""),
                };
                let name = p.print_identifier(&element.name);
                let annot = p.print_type(&element.annot);
                p.concat([
                    variance,
                    name,
                    if element.optional { p.s("?") } else { p.s("") },
                    p.s(": "),
                    annot,
                ])
            }
            types::tuple::Element::SpreadElement { element, .. } => {
                let annot = p.print_type(&element.annot);
                match &element.name {
                    Some(name) => {
                        let name = p.print_identifier(name);
                        p.concat([p.s("..."), name, p.s(": "), annot])
                    }
                    None => p.concat([p.s("..."), annot]),
                }
            }
        })
    }
}
