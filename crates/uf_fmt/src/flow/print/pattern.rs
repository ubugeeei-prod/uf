//! Binding and assignment patterns.

use uf_flow::ast::pattern;

use super::Printer;
use crate::doc::Doc;
use crate::flow::node::{NodeRef, Pattern};

impl<'a> Printer<'a> {
    /// Any pattern, with its comments.
    pub fn print_pattern(&mut self, pattern: &'a Pattern) -> Doc<'a> {
        self.print_node(NodeRef::Pattern(pattern), |p| match pattern {
            pattern::Pattern::Object { inner, .. } => p.print_object_pattern(inner, pattern),
            pattern::Pattern::Array { inner, .. } => p.print_array_pattern(inner, pattern),
            pattern::Pattern::Identifier { inner, .. } => {
                let name = p.print_identifier(&inner.name);
                let optional = if inner.optional { p.s("?") } else { p.s("") };
                let annotation = p.print_optional_annotation(&inner.annot);
                p.concat([name, optional, annotation])
            }
            pattern::Pattern::Expression { inner, .. } => p.print_expression(inner),
        })
    }
}
