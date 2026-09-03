//! Member chains — `a.b().c().d()` — Prettier's `printMemberChain`.
//!
//! The chain is flattened into a list of printed nodes (the head, then
//! each `.name` and each `(args)`), the list is cut into *groups* at the
//! calls, and the groups are printed either on one line or one per line
//! indented under the head. The rules for what goes into the first group
//! (`this.x`, `Factory.create`, short identifiers in expression
//! statements) and for when the chain must break (more than two calls with
//! non-trivial arguments, a comment inside) are Prettier's, verbatim.

use uf_flow::Loc;
use uf_flow::ast::{expression, statement};

use super::Printer;
use super::call::{is_function_or_arrow, is_simple_call_argument};
use super::parens::{is_call, is_member};
use crate::doc::{BREAK_PARENT, Doc, HARDLINE, Label, will_break};
use crate::flow::comments::Placement;
use crate::flow::node::{Expression, NodeRef};

/// One link of a flattened chain.
struct PrintedNode<'a> {
    node: &'a Expression,
    printed: Doc<'a>,
    has_trailing_empty_line: bool,
}

/// Whether a member access is `a[0]`: computed with a number.
fn is_computed_number_access(expression: &Expression) -> bool {
    let property = match &**expression {
        expression::ExpressionInner::Member { inner, .. } => &inner.property,
        expression::ExpressionInner::OptionalMember { inner, .. } => &inner.member.property,
        _ => return false,
    };
    matches!(property, expression::member::Property::PropertyExpression(property)
        if matches!(**property, expression::ExpressionInner::NumberLiteral { .. }))
}

fn is_computed(expression: &Expression) -> bool {
    let property = match &**expression {
        expression::ExpressionInner::Member { inner, .. } => &inner.property,
        expression::ExpressionInner::OptionalMember { inner, .. } => &inner.member.property,
        _ => return false,
    };
    matches!(
        property,
        expression::member::Property::PropertyExpression(_)
    )
}

/// Whether `name` looks like a factory: capitalised, or only `_`/`$`.
fn is_factory(name: &str) -> bool {
    name.starts_with(|ch: char| ch.is_ascii_uppercase())
        || (!name.is_empty() && name.chars().all(|ch| ch == '_' || ch == '$'))
}

impl<'a> Printer<'a> {
    /// Print the call on top of the ancestor stack as a member chain.
    pub fn print_member_chain(&mut self, expression: &'a Expression) -> Doc<'a> {
        let parent = self.parent();
        let is_expression_statement = matches!(parent, Some(NodeRef::Statement(statement))
            if matches!(**statement, statement::StatementInner::Expression { .. }));

        // Flatten the chain: the head first, then each link.
        let mut printed_nodes: Vec<PrintedNode<'a>> = Vec::new();
        self.flatten_chain(expression, &mut printed_nodes, true);

        // Once we have a linear list of printed nodes, we want to create
        // groups out of it.
        //
        //   a().b.c().d().e
        //
        // will be grouped as
        //
        //   [
        //     [Identifier, CallExpression],
        //     [MemberExpression, MemberExpression, CallExpression],
        //     [MemberExpression, CallExpression],
        //     [MemberExpression],
        //   ]
        //
        // so that we can print it as
        //
        //   a()
        //     .b.c()
        //     .d()
        //     .e
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = vec![0];
        let mut i = 1;
        // The first group is the first node, followed by as many
        // call expressions as possible (`fn()()()`), then as many array
        // accessors as possible (`fn()[0][1]`).
        while i < printed_nodes.len()
            && (is_call(printed_nodes[i].node) || is_computed_number_access(printed_nodes[i].node))
        {
            current.push(i);
            i += 1;
        }
        // Then, as many member expressions as possible, when the head is
        // not itself a call: `this.items.toArray()`.
        if !is_call(printed_nodes[0].node) {
            while i + 1 < printed_nodes.len()
                && is_member(printed_nodes[i].node)
                && is_member(printed_nodes[i + 1].node)
            {
                current.push(i);
                i += 1;
            }
        }
        groups.push(std::mem::take(&mut current));

        // Each following group is a run of members up to and including the
        // next call (plus any `[0]` accesses after it).
        let mut has_seen_call = false;
        while i < printed_nodes.len() {
            if has_seen_call && is_member(printed_nodes[i].node) {
                if is_computed_number_access(printed_nodes[i].node) {
                    current.push(i);
                    i += 1;
                    continue;
                }
                groups.push(std::mem::take(&mut current));
                has_seen_call = false;
            }
            if is_call(printed_nodes[i].node)
                || matches!(
                    **printed_nodes[i].node,
                    expression::ExpressionInner::Import { .. }
                )
            {
                has_seen_call = true;
            }
            current.push(i);
            if self.has_comment_placed(
                NodeRef::Expression(printed_nodes[i].node).key(),
                Placement::Trailing,
            ) {
                groups.push(std::mem::take(&mut current));
                has_seen_call = false;
            }
            i += 1;
        }
        if !current.is_empty() {
            groups.push(current);
        }

        // There are cases like `Object.keys()`, `Observable.of()`,
        // `_.values()` where they are the subject of all the chained calls
        // and therefore should be kept on the same line.
        let should_merge = groups.len() >= 2
            && !groups[1].is_empty()
            && !self.has_comment(NodeRef::Expression(printed_nodes[groups[1][0]].node).key())
            && self.should_not_wrap(&groups, &printed_nodes, is_expression_statement);

        let print_group = |p: &mut Self, group: &[usize]| -> Doc<'a> {
            p.docs
                .concat(group.iter().map(|index| printed_nodes[*index].printed))
        };
        let printed_groups: Vec<Doc<'a>> = groups
            .iter()
            .map(|group| print_group(self, group))
            .collect();
        let one_line = self.docs.concat(printed_groups.iter().copied());

        let cutoff = if should_merge { 3 } else { 2 };
        let flat: Vec<usize> = groups.iter().flatten().copied().collect();
        let node_has_comment = flat[1..flat.len().saturating_sub(1).max(1)]
            .iter()
            .any(|index| {
                self.has_comment_placed(
                    NodeRef::Expression(printed_nodes[*index].node).key(),
                    Placement::Leading,
                )
            })
            || flat[..flat.len().saturating_sub(1)].iter().any(|index| {
                self.has_comment_placed(
                    NodeRef::Expression(printed_nodes[*index].node).key(),
                    Placement::Trailing,
                )
            })
            || groups.get(cutoff).is_some_and(|group| {
                group.first().is_some_and(|index| {
                    self.has_comment_placed(
                        NodeRef::Expression(printed_nodes[*index].node).key(),
                        Placement::Leading,
                    )
                })
            });

        // We don't want to print in one line if the chain has a comment,
        // non-trivial arguments, or any group but the last one breaks.
        if groups.len() <= cutoff
            && !node_has_comment
            && groups.iter().all(|group| {
                group
                    .last()
                    .is_none_or(|index| !printed_nodes[*index].has_trailing_empty_line)
            })
        {
            let result = if self.is_long_curried_call_chain_here(expression) {
                one_line
            } else {
                self.group(one_line)
            };
            return self.docs.label(Label::MemberChain, result);
        }

        // If any group breaks (e.g. because of a comment or a trailing
        // empty line) the whole chain is printed expanded.
        let last_of_head = groups[if should_merge { 1 } else { 0 }]
            .last()
            .copied()
            .unwrap_or(0);
        let last_head_node = printed_nodes[last_of_head].node;
        let has_empty_line_after_head =
            !is_call(last_head_node) && self.should_insert_empty_line_after(last_head_node);

        let rest_groups: Vec<Doc<'a>> = printed_groups[if should_merge { 2 } else { 1 }..].to_vec();
        let indented_rest = if rest_groups.is_empty() {
            self.s("")
        } else {
            self.indent(self.concat([&HARDLINE, self.join(&HARDLINE, rest_groups)]))
        };
        let expanded = self.concat([
            printed_groups[0],
            if should_merge {
                printed_groups[1]
            } else {
                self.s("")
            },
            if has_empty_line_after_head {
                &HARDLINE
            } else {
                self.s("")
            },
            indented_rest,
        ]);

        let call_expressions: Vec<&'a Expression> = printed_nodes
            .iter()
            .map(|printed| printed.node)
            .filter(|node| is_call(node))
            .collect();

        let last_group_will_break_and_other_calls_are_function_arguments = {
            let last_node = groups
                .last()
                .and_then(|group| group.last())
                .map(|index| printed_nodes[*index].node);
            let last_doc = printed_groups.last().copied();
            match (last_node, last_doc) {
                (Some(last_node), Some(last_doc)) => {
                    is_call(last_node)
                        && will_break(last_doc)
                        && call_expressions[..call_expressions.len().saturating_sub(1)]
                            .iter()
                            .any(|call| {
                                call_arguments_of(call)
                                    .iter()
                                    .any(|argument| is_function_or_arrow(argument))
                            })
                }
                _ => false,
            }
        };

        let result = if node_has_comment
            || (call_expressions.len() > 2
                && call_expressions.iter().any(|call| {
                    !call_arguments_of(call)
                        .iter()
                        .all(|argument| is_simple_call_argument(argument, 2))
                }))
            || printed_groups[..printed_groups.len() - 1]
                .iter()
                .any(|group| will_break(group))
            || last_group_will_break_and_other_calls_are_function_arguments
        {
            self.group(expanded)
        } else {
            let leading = if will_break(one_line) || has_empty_line_after_head {
                &BREAK_PARENT
            } else {
                self.s("")
            };
            self.concat([
                leading,
                self.docs.conditional_group(&[one_line, expanded], false),
            ])
        };
        self.docs.label(Label::MemberChain, result)
    }

    /// Walk down the chain, pushing printed links front to back.
    fn flatten_chain(
        &mut self,
        expression: &'a Expression,
        out: &mut Vec<PrintedNode<'a>>,
        is_root: bool,
    ) {
        use expression::ExpressionInner as E;
        let node = NodeRef::Expression(expression);
        match &**expression {
            E::Call { .. } | E::OptionalCall { .. }
                if is_root || {
                    let callee = match &**expression {
                        E::Call { inner, .. } => &inner.callee,
                        E::OptionalCall { inner, .. } => &inner.call.callee,
                        _ => return,
                    };
                    is_member(callee) || is_call(callee)
                } =>
            {
                let (call, optional) = match &**expression {
                    E::Call { inner, .. } => (&**inner, false),
                    E::OptionalCall { inner, .. } => (
                        &inner.call,
                        matches!(inner.optional, expression::OptionalCallKind::Optional),
                    ),
                    _ => return,
                };
                let has_trailing_empty_line =
                    !is_root && self.should_insert_empty_line_after(expression);
                let printed = self.with_node(node, |p| {
                    let optional_token = if optional { p.s("?.") } else { p.s("") };
                    let targs = p.print_optional_call_type_args(call.targs.as_ref());
                    let arguments = p.print_call_arguments(&call.arguments, node.key(), false);
                    p.concat([optional_token, targs, arguments])
                });
                let printed = if is_root {
                    printed
                } else {
                    let with_comments = self.print_comments(node.key(), printed);
                    if has_trailing_empty_line {
                        self.concat([with_comments, &HARDLINE])
                    } else {
                        with_comments
                    }
                };
                self.ancestors.push(node);
                self.flatten_chain(&call.callee, out, false);
                self.ancestors.pop();
                out.push(PrintedNode {
                    node: expression,
                    printed,
                    has_trailing_empty_line,
                });
            }
            E::Member { inner, .. } => {
                let printed = self.with_node(node, |p| p.print_member_lookup(inner, false));
                let printed = self.print_comments(node.key(), printed);
                self.ancestors.push(node);
                self.flatten_chain(&inner.object, out, false);
                self.ancestors.pop();
                out.push(PrintedNode {
                    node: expression,
                    printed,
                    has_trailing_empty_line: false,
                });
            }
            E::OptionalMember { inner, .. } => {
                let optional = matches!(inner.optional, expression::OptionalMemberKind::Optional);
                let printed =
                    self.with_node(node, |p| p.print_member_lookup(&inner.member, optional));
                let printed = self.print_comments(node.key(), printed);
                self.ancestors.push(node);
                self.flatten_chain(&inner.member.object, out, false);
                self.ancestors.pop();
                out.push(PrintedNode {
                    node: expression,
                    printed,
                    has_trailing_empty_line: false,
                });
            }
            _ => {
                let printed = self.print_expression(expression);
                out.push(PrintedNode {
                    node: expression,
                    printed,
                    has_trailing_empty_line: false,
                });
            }
        }
    }

    /// Whether a blank line follows `node` in the source, looking past a
    /// closing parenthesis.
    fn should_insert_empty_line_after(&self, node: &'a Expression) -> bool {
        let end = self.text.span(node.loc()).end;
        match self.text.next_non_space_non_comment_index(end) {
            Some(index) if self.text.text().as_bytes().get(index) == Some(&b')') => {
                self.text.is_next_line_empty(index + 1)
            }
            _ => self.text.is_next_line_empty(end),
        }
    }

    /// Prettier's `shouldNotWrap`: the first group stays with the second
    /// when the head is `this`, a factory, or (in a statement) short.
    fn should_not_wrap(
        &self,
        groups: &[Vec<usize>],
        printed_nodes: &[PrintedNode<'a>],
        is_expression_statement: bool,
    ) -> bool {
        let has_computed = groups
            .get(1)
            .and_then(|group| group.first())
            .is_some_and(|index| is_computed(printed_nodes[*index].node));
        if groups[0].len() == 1 {
            let first = printed_nodes[groups[0][0]].node;
            return match &**first {
                expression::ExpressionInner::This { .. } => true,
                expression::ExpressionInner::Identifier { inner, .. } => {
                    is_factory(&inner.name)
                        || (is_expression_statement
                            && inner.name.len() <= self.options.indent_width)
                        || has_computed
                }
                _ => false,
            };
        }
        let last = printed_nodes[*groups[0].last().expect("non-empty")].node;
        let property = match &**last {
            expression::ExpressionInner::Member { inner, .. } => &inner.property,
            expression::ExpressionInner::OptionalMember { inner, .. } => &inner.member.property,
            _ => return false,
        };
        matches!(property, expression::member::Property::PropertyIdentifier(id)
            if is_factory(&id.name) || has_computed)
    }

    fn is_long_curried_call_chain_here(&self, expression: &'a Expression) -> bool {
        let Some(NodeRef::Expression(parent)) = self.parent() else {
            return false;
        };
        let node_count = call_arguments_of(expression).len();
        let (parent_callee, parent_count) = match &**parent {
            expression::ExpressionInner::Call { inner, .. } => {
                (&inner.callee, inner.arguments.arguments.len())
            }
            expression::ExpressionInner::OptionalCall { inner, .. } => {
                (&inner.call.callee, inner.call.arguments.arguments.len())
            }
            _ => return false,
        };
        super::parens::same(parent_callee, expression)
            && parent_count > 0
            && node_count > parent_count
    }
}

/// The argument expressions of a call.
fn call_arguments_of(expression: &Expression) -> Vec<&Expression> {
    let list = match &**expression {
        expression::ExpressionInner::Call { inner, .. } => &inner.arguments.arguments,
        expression::ExpressionInner::OptionalCall { inner, .. } => &inner.call.arguments.arguments,
        _ => return Vec::new(),
    };
    list.iter().map(super::call::argument_expression).collect()
}

#[allow(dead_code)]
fn unused(_: &Loc) {}
