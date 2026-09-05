//! Statements: control flow, declarations, and the block that holds them.

use uf_flow::Loc;
use uf_flow::ast::{expression, statement};

use super::Printer;

/// What an empty block prints as.
///
/// Prettier decides this from the parent, and the split is between a body
/// that is *expected* to be empty sometimes and a block whose emptiness is
/// worth seeing on its own line:
///
/// ```js
/// function noop() {}          // a body that does nothing, said in one line
/// while (poll()) {}           // a loop whose work is the condition
/// try {
/// } catch (error) {}          // the `try` is empty by accident
/// ```
///
/// Which is not a matter of taste: `if (a) {}` reads as a mistake and
/// `if (a) {\n}` reads as a hole somebody left, and the second is what the
/// author wrote.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EmptyBlock {
    /// `{}` — function, method, arrow, component and static bodies, and the
    /// bodies of `for (;;)`, `while` and `do … while`. A `catch` too, unless
    /// the statement also has a `finally`.
    Collapsed,
    /// `{` and `}` on two lines — `if`, `else`, `try`, `finally`, `with`,
    /// `for … in`, `for … of`, a labelled block, a `case` block, a block on
    /// its own, and a `declare module` or `declare namespace` body.
    Open,
}
use crate::doc::{BREAK_PARENT, Doc, HARDLINE, LINE, SOFTLINE};
use crate::flow::comments::{Marker, Placement};
use crate::flow::node::{Expression, NodeRef, Statement};

/// Whether `statement` is a lone `;`, which is never printed.
pub fn is_empty_statement(statement: &Statement) -> bool {
    matches!(**statement, statement::StatementInner::Empty { .. })
}

/// Whether `statement` is a `{ … }` block.
pub fn is_block_statement(statement: &Statement) -> bool {
    matches!(**statement, statement::StatementInner::Block { .. })
}

impl<'a> Printer<'a> {
    /// Print any statement, with its comments.
    pub fn print_statement(&mut self, statement: &'a Statement) -> Doc<'a> {
        self.print_node(NodeRef::Statement(statement), |p| {
            p.print_statement_inner(statement)
        })
    }

    /// A statement that needs a leading `;` in no-semicolon mode, printed
    /// with the guard after its leading comments.
    pub fn print_statement_with_leading_semi(&mut self, statement: &'a Statement) -> Doc<'a> {
        let node = NodeRef::Statement(statement);
        let inner = self.with_node(node, |p| p.print_statement_inner(statement));
        let guarded = self.concat([self.s(";"), inner]);
        self.print_comments(node.key(), guarded)
    }

    fn print_statement_inner(&mut self, statement: &'a Statement) -> Doc<'a> {
        use statement::StatementInner as S;
        let key = NodeRef::Statement(statement).key();
        match &**statement {
            S::Block { inner, .. } => {
                let empty = self.empty_block_of_statement();
                self.print_block_body(&inner.body, key, empty)
            }
            S::Break { inner, .. } => {
                let label = match &inner.label {
                    Some(label) => {
                        let printed = self.print_identifier(label);
                        self.concat([self.s(" "), printed])
                    }
                    None => self.s(""),
                };
                self.concat([self.s("break"), label, self.semi()])
            }
            S::Continue { inner, .. } => {
                let label = match &inner.label {
                    Some(label) => {
                        let printed = self.print_identifier(label);
                        self.concat([self.s(" "), printed])
                    }
                    None => self.s(""),
                };
                self.concat([self.s("continue"), label, self.semi()])
            }
            S::ClassDeclaration { inner, .. } => self.print_class(inner, key),
            S::ComponentDeclaration { inner, .. } => self.print_component_declaration(inner, key),
            S::Debugger { .. } => self.concat([self.s("debugger"), self.semi()]),
            S::DeclareClass { inner, .. } => self.print_declare_class(inner, key),
            S::DeclareComponent { inner, .. } => self.print_declare_component(inner),
            S::DeclareEnum { inner, .. } => {
                let printed = self.print_enum(inner, key);
                self.concat([self.s("declare "), printed])
            }
            S::DeclareExportDeclaration { inner, .. } => self.print_declare_export(inner, key),
            S::DeclareFunction { inner, .. } => self.print_declare_function(inner, key),
            S::DeclareInterface { inner, .. } => {
                let printed = self.print_interface(inner, key);
                self.concat([self.s("declare "), printed])
            }
            S::DeclareModule { inner, .. } => self.print_declare_module(inner, key),
            S::DeclareModuleExports { inner, .. } => {
                let annotation = self.print_type_annotation(&inner.annot);
                self.concat([self.s("declare module.exports"), annotation, self.semi()])
            }
            S::DeclareNamespace { inner, .. } => self.print_declare_namespace(inner),
            S::DeclareTypeAlias { inner, .. } => {
                let printed = self.print_type_alias(inner, key);
                self.concat([self.s("declare "), printed])
            }
            S::DeclareOpaqueType { inner, .. } => {
                let printed = self.print_opaque_type(inner, key);
                self.concat([self.s("declare "), printed])
            }
            S::DeclareVariable { inner, .. } => self.print_declare_variable(inner, key),
            S::DoWhile { inner, .. } => {
                let printed_body = self.print_statement(&inner.body);
                let clause = self.adjust_clause(&inner.body, printed_body, false);
                let do_body = self.group(self.concat([self.s("do"), clause]));
                let separator = if is_block_statement(&inner.body) {
                    self.s(" ")
                } else {
                    &HARDLINE
                };
                let test = self.print_expression(&inner.test);
                self.concat([
                    do_body,
                    separator,
                    self.s("while ("),
                    self.group(
                        self.concat([self.indent(self.concat([&SOFTLINE, test])), &SOFTLINE]),
                    ),
                    self.s(")"),
                    self.semi(),
                ])
            }
            S::Empty { .. } => self.s(""),
            S::EnumDeclaration { inner, .. } => self.print_enum(inner, key),
            S::ExportDefaultDeclaration { inner, .. } => self.print_export_default(inner, key),
            S::ExportNamedDeclaration { inner, .. } => self.print_export_named(inner, key),
            S::ExportAssignment { inner, .. } => {
                // `export = x`, a TypeScript form the port accepts.
                let rhs = match &inner.rhs {
                    statement::ExportAssignmentRhs::Expression(expression) => {
                        self.print_expression(expression)
                    }
                    statement::ExportAssignmentRhs::DeclareFunction(loc, declaration) => self
                        .print_node(NodeRef::DeclareFunction(loc, declaration), |p| {
                            p.print_declare_function(
                                declaration,
                                NodeRef::DeclareFunction(loc, declaration).key(),
                            )
                        }),
                };
                self.concat([self.s("export = "), rhs, self.semi()])
            }
            S::NamespaceExportDeclaration { inner, .. } => {
                let id = self.print_identifier(&inner.id);
                self.concat([self.s("export as namespace "), id, self.semi()])
            }
            S::Expression { inner, .. } => self.print_expression_statement(inner, key),
            S::For { inner, .. } => self.print_for(inner, key),
            S::ForIn { inner, .. } => {
                let left = match &inner.left {
                    statement::for_in::Left::LeftDeclaration((loc, declaration)) => {
                        self.print_variable_declaration(loc, declaration, true)
                    }
                    statement::for_in::Left::LeftPattern(pattern) => self.print_pattern(pattern),
                };
                let right = self.print_expression(&inner.right);
                let printed_body = self.print_statement(&inner.body);
                let body = self.adjust_clause(&inner.body, printed_body, false);
                self.group(self.concat([
                    self.s("for ("),
                    left,
                    self.s(" in "),
                    right,
                    self.s(")"),
                    body,
                ]))
            }
            S::ForOf { inner, .. } => {
                let left = match &inner.left {
                    statement::for_of::Left::LeftDeclaration((loc, declaration)) => {
                        self.print_variable_declaration(loc, declaration, true)
                    }
                    statement::for_of::Left::LeftPattern(pattern) => self.print_pattern(pattern),
                };
                let right = self.print_expression(&inner.right);
                let printed_body = self.print_statement(&inner.body);
                let body = self.adjust_clause(&inner.body, printed_body, false);
                self.group(self.concat([
                    self.s("for"),
                    if inner.await_ {
                        self.s(" await")
                    } else {
                        self.s("")
                    },
                    self.s(" ("),
                    left,
                    self.s(" of "),
                    right,
                    self.s(")"),
                    body,
                ]))
            }
            S::FunctionDeclaration { inner, .. } => {
                self.print_function(inner, key, super::PrintArgs::default())
            }
            S::If { inner, .. } => self.print_if(inner, key),
            S::ImportDeclaration { inner, .. } => self.print_import(inner, key),
            S::ImportEqualsDeclaration { inner, .. } => {
                let reference = match &inner.module_reference {
                    statement::import_equals_declaration::ModuleReference::ExternalModuleReference(loc, literal) => {
                        let source = self.print_node(NodeRef::StringLiteral(loc, literal), |p| p.print_string_literal(literal));
                        self.concat([self.s("require("), source, self.s(")")])
                    }
                    statement::import_equals_declaration::ModuleReference::Identifier(id) => {
                        self.print_generic_identifier(id)
                    }
                };
                let id = self.print_identifier(&inner.id);
                self.concat([
                    if inner.is_export {
                        self.s("export ")
                    } else {
                        self.s("")
                    },
                    self.s("import "),
                    match inner.import_kind {
                        statement::ImportKind::ImportType => self.s("type "),
                        statement::ImportKind::ImportTypeof => self.s("typeof "),
                        statement::ImportKind::ImportValue => self.s(""),
                    },
                    id,
                    self.s(" = "),
                    reference,
                    self.semi(),
                ])
            }
            S::InterfaceDeclaration { inner, .. } => self.print_interface(inner, key),
            S::Labeled { inner, .. } => {
                let label = self.print_identifier(&inner.label);
                if is_empty_statement(&inner.body) {
                    return self.concat([label, self.s(":;")]);
                }
                let body = self.print_statement(&inner.body);
                self.concat([label, self.s(": "), body])
            }
            S::Match { inner, .. } => self.print_match_statement(inner, key),
            S::RecordDeclaration { .. } => {
                // Records are an experiment the port parses but Flow does not
                // ship; print the source verbatim so nothing is lost.
                let span = self.text.span(statement.loc());
                self.replace_end_of_line(self.text.slice(span))
            }
            S::Return { inner, .. } => {
                let argument = self.print_return_or_throw_argument(inner.argument.as_ref(), key);
                self.concat([self.s("return"), argument])
            }
            S::Throw { inner, .. } => {
                let argument = self.print_return_or_throw_argument(Some(&inner.argument), key);
                self.concat([self.s("throw"), argument])
            }
            S::Switch { inner, .. } => self.print_switch(inner),
            S::Try { inner, .. } => {
                let block = self.print_block(&inner.block.0, &inner.block.1, EmptyBlock::Open);
                let handler = inner.handler.as_ref().map(|handler| {
                    let printed = self.print_catch_clause(handler, inner.finalizer.is_some());
                    self.concat([self.s(" "), printed])
                });
                let finalizer = inner.finalizer.as_ref().map(|(loc, block)| {
                    let printed = self.print_block(loc, block, EmptyBlock::Open);
                    self.concat([self.s(" finally "), printed])
                });
                self.concat([
                    self.s("try "),
                    block,
                    handler.unwrap_or(self.s("")),
                    finalizer.unwrap_or(self.s("")),
                ])
            }
            S::TypeAlias { inner, .. } => self.print_type_alias(inner, key),
            S::OpaqueType { inner, .. } => self.print_opaque_type(inner, key),
            S::VariableDeclaration { inner, loc } => {
                self.print_variable_declaration(loc, inner, false)
            }
            S::While { inner, .. } => {
                let test = self.print_expression(&inner.test);
                let printed_body = self.print_statement(&inner.body);
                let body = self.adjust_clause(&inner.body, printed_body, false);
                self.group(self.concat([
                    self.s("while ("),
                    self.group(
                        self.concat([self.indent(self.concat([&SOFTLINE, test])), &SOFTLINE]),
                    ),
                    self.s(")"),
                    body,
                ]))
            }
            S::With { inner, .. } => {
                let object = self.print_expression(&inner.object);
                let printed_body = self.print_statement(&inner.body);
                let body = self.adjust_clause(&inner.body, printed_body, false);
                self.group(self.concat([self.s("with ("), object, self.s(")"), body]))
            }
        }
    }

    /// The body of a control statement: a block on the same line, a bare
    /// statement indented on the next, an empty statement as `;`.
    pub fn adjust_clause(
        &self,
        body: &'a Statement,
        clause: Doc<'a>,
        force_space: bool,
    ) -> Doc<'a> {
        if is_empty_statement(body) {
            return self.s(";");
        }
        if is_block_statement(body) || force_space {
            return self.concat([self.s(" "), clause]);
        }
        self.indent(self.concat([&LINE, clause]))
    }

    fn print_expression_statement(
        &mut self,
        inner: &'a statement::Expression<Loc, Loc>,
        key: crate::flow::node::NodeKey,
    ) -> Doc<'a> {
        let expression = if inner.directive.is_some() {
            self.print_directive(&inner.expression)
        } else {
            self.print_expression(&inner.expression)
        };
        // A comment kept on this line because the `if` it belongs to has
        // a bare consequent: it prints after the semicolon, and a line
        // comment there breaks every group it sits in, exactly as a
        // trailing comment would.
        let dangling = match self.print_dangling_comments(key, Marker::IfElseSameLine, false) {
            Some(dangling) => {
                let suffix = self.docs.line_suffix(self.concat([self.s(" "), dangling]));
                if self.has_line_comment(key, Some(Placement::Dangling)) {
                    self.concat([suffix, &BREAK_PARENT])
                } else {
                    suffix
                }
            }
            None => self.s(""),
        };
        self.concat([expression, self.semi(), dangling])
    }

    /// A directive keeps its quotes unless it holds none, because its
    /// meaning is its exact code units.
    fn print_directive(&mut self, expression: &'a Expression) -> Doc<'a> {
        let raw: Option<&'a str> = match &**expression {
            expression::ExpressionInner::StringLiteral { inner, .. } => Some(&inner.raw),
            _ => None,
        };
        match raw {
            Some(raw) if raw.len() >= 2 => {
                let content = &raw[1..raw.len() - 1];
                if content.contains('"') || content.contains('\'') {
                    let node = NodeRef::Expression(expression);
                    return self.print_node(node, |p| p.docs.borrowed(raw));
                }
                let quote = match self.options.quote {
                    uf_config::QuoteStyle::Single => "'",
                    uf_config::QuoteStyle::Double => "\"",
                };
                let node = NodeRef::Expression(expression);
                self.print_node(node, |p| p.text(&format!("{quote}{content}{quote}")))
            }
            _ => self.print_expression(expression),
        }
    }

    fn print_if(
        &mut self,
        inner: &'a statement::If<Loc, Loc>,
        key: crate::flow::node::NodeKey,
    ) -> Doc<'a> {
        let test = self.print_expression(&inner.test);
        let consequent_doc = self.print_statement(&inner.consequent);
        let consequent = self.adjust_clause(&inner.consequent, consequent_doc, false);
        let opening = self.group(self.concat([
            self.s("if ("),
            self.group(self.concat([self.indent(self.concat([&SOFTLINE, test])), &SOFTLINE])),
            self.s(")"),
            consequent,
        ]));
        let mut parts = vec![opening];
        if let Some(alternate) = &inner.alternate {
            let consequent_key = NodeRef::Statement(&inner.consequent).key();
            let comment_on_own_line = self.has_trailing_line_comment(consequent_key)
                || self.needs_hardline_after_dangling_comment(key);
            let else_on_same_line = is_block_statement(&inner.consequent) && !comment_on_own_line;
            parts.push(if else_on_same_line {
                self.s(" ")
            } else {
                &HARDLINE
            });
            if let Some(dangling) = self.print_dangling_comments(key, Marker::None, false) {
                parts.push(dangling);
                parts.push(if comment_on_own_line {
                    &HARDLINE
                } else {
                    self.s(" ")
                });
            }
            let alternate_node = NodeRef::Alternate(alternate);
            let is_else_if = matches!(*alternate.body, statement::StatementInner::If { .. });
            let alternate_doc = self.print_node(alternate_node, |p| {
                let body = p.print_statement(&alternate.body);
                p.adjust_clause(&alternate.body, body, is_else_if)
            });
            parts.push(self.s("else"));
            parts.push(self.group(alternate_doc));
        }
        self.docs.concat_vec(parts)
    }

    /// Whether the last dangling comment on `key` is a line comment, which
    /// must be followed by a newline rather than a space.
    fn needs_hardline_after_dangling_comment(&self, key: crate::flow::node::NodeKey) -> bool {
        let Some(slots) = self.comments.slots(key) else {
            return false;
        };
        let Some(last) = slots.dangling.last() else {
            return false;
        };
        let comment = self.comments.get(*last);
        matches!(comment.kind, uf_flow::ast::CommentKind::Line) && comment.marker == Marker::None
    }

    fn print_for(
        &mut self,
        inner: &'a statement::For<Loc, Loc>,
        key: crate::flow::node::NodeKey,
    ) -> Doc<'a> {
        let printed_body = self.print_statement(&inner.body);
        let body = self.adjust_clause(&inner.body, printed_body, false);
        let dangling = self.print_dangling_comments(key, Marker::None, false);
        let printed_comments =
            dangling.map_or(self.s(""), |dangling| self.concat([dangling, &SOFTLINE]));
        if inner.init.is_none() && inner.test.is_none() && inner.update.is_none() {
            return self.concat([
                printed_comments,
                self.group(self.concat([self.s("for (;;)"), body])),
            ]);
        }
        let init = match &inner.init {
            Some(statement::for_::Init::InitDeclaration((loc, declaration))) => {
                self.print_variable_declaration(loc, declaration, true)
            }
            Some(statement::for_::Init::InitExpression(expression)) => {
                self.print_expression(expression)
            }
            None => self.s(""),
        };
        let test = inner
            .test
            .as_ref()
            .map_or(self.s(""), |test| self.print_expression(test));
        let update = inner
            .update
            .as_ref()
            .map_or(self.s(""), |update| self.print_expression(update));
        self.concat([
            printed_comments,
            self.group(self.concat([
                self.s("for ("),
                self.group(self.concat([
                    self.indent(self.concat([
                        &SOFTLINE,
                        init,
                        self.s(";"),
                        &LINE,
                        test,
                        self.s(";"),
                        &LINE,
                        update,
                    ])),
                    &SOFTLINE,
                ])),
                self.s(")"),
                body,
            ])),
        ])
    }

    fn print_switch(&mut self, inner: &'a statement::Switch<Loc, Loc>) -> Doc<'a> {
        let discriminant = self.print_expression(&inner.discriminant);
        let head = self.group(self.concat([
            self.s("switch ("),
            self.indent(self.concat([&SOFTLINE, discriminant])),
            &SOFTLINE,
            self.s(")"),
        ]));
        let mut cases = Vec::with_capacity(inner.cases.len() * 2);
        let last = inner.cases.len().saturating_sub(1);
        for (index, case) in inner.cases.iter().enumerate() {
            let printed = self.print_node(NodeRef::SwitchCase(case), |p| p.print_switch_case(case));
            cases.push(printed);
            if index != last {
                cases.push(&HARDLINE);
                if self.text.is_next_line_empty(self.text.span(&case.loc).end) {
                    cases.push(&HARDLINE);
                }
            }
        }
        let body = if cases.is_empty() {
            self.s("")
        } else {
            self.indent(self.concat([&HARDLINE, self.docs.concat_vec(cases)]))
        };
        self.concat([head, self.s(" {"), body, &HARDLINE, self.s("}")])
    }

    fn print_switch_case(&mut self, case: &'a statement::switch::Case<Loc, Loc>) -> Doc<'a> {
        let key = NodeRef::SwitchCase(case).key();
        let mut parts = Vec::new();
        match &case.test {
            Some(test) => {
                let test = self.print_expression(test);
                parts.push(self.s("case "));
                parts.push(test);
                parts.push(self.s(":"));
            }
            None => parts.push(self.s("default:")),
        }
        if let Some(dangling) = self.print_dangling_comments(key, Marker::None, false) {
            parts.push(self.s(" "));
            parts.push(dangling);
        }
        let consequent: Vec<&'a Statement> = case
            .consequent
            .iter()
            .filter(|statement| !is_empty_statement(statement))
            .collect();
        if !consequent.is_empty() {
            let body = self.print_statement_sequence(&case.consequent);
            if consequent.len() == 1 && is_block_statement(consequent[0]) {
                parts.push(self.s(" "));
                parts.push(body);
            } else {
                parts.push(self.indent(self.concat([&HARDLINE, body])));
            }
        }
        self.docs.concat_vec(parts)
    }

    /// `catch (error) { … }`.
    ///
    /// `has_finally` because an empty handler collapses to `catch {}` only
    /// when it is the end of the statement. With a `finally` after it the
    /// three braces line up, and Prettier keeps the handler open so they do.
    fn print_catch_clause(
        &mut self,
        clause: &'a statement::try_::CatchClause<Loc, Loc>,
        has_finally: bool,
    ) -> Doc<'a> {
        let empty = if has_finally {
            EmptyBlock::Open
        } else {
            EmptyBlock::Collapsed
        };
        self.print_node(NodeRef::CatchClause(clause), |p| {
            let body = p.print_block(&clause.body.0, &clause.body.1, empty);
            let Some(param) = &clause.param else {
                return p.concat([p.s("catch "), body]);
            };
            let param_key = NodeRef::Pattern(param).key();
            let param_has_comments = p.has_comment_where(param_key, None, |comment| {
                !matches!(comment.kind, uf_flow::ast::CommentKind::Block)
                    || (p.comments.slots(param_key).is_some_and(|slots| {
                        slots
                            .leading
                            .iter()
                            .any(|index| p.comments.get(*index).span == comment.span)
                    }) && p.text.has_newline(comment.span.end, false))
                    || (p.comments.slots(param_key).is_some_and(|slots| {
                        slots
                            .trailing
                            .iter()
                            .any(|index| p.comments.get(*index).span == comment.span)
                    }) && p.text.has_newline(comment.span.start, true))
            });
            let param = p.print_pattern(param);
            let head = if param_has_comments {
                p.concat([
                    p.s("("),
                    p.indent(p.concat([&SOFTLINE, param])),
                    &SOFTLINE,
                    p.s(") "),
                ])
            } else {
                p.concat([p.s("("), param, p.s(") ")])
            };
            p.concat([p.s("catch "), head, body])
        })
    }

    /// `return x` / `throw x`, with the argument wrapped in parentheses when
    /// it is a binary expression that breaks, and comments kept in order.
    fn print_return_or_throw_argument(
        &mut self,
        argument: Option<&'a Expression>,
        key: crate::flow::node::NodeKey,
    ) -> Doc<'a> {
        let mut parts = Vec::new();
        if let Some(argument) = argument {
            if self.return_argument_has_leading_comment(argument) {
                let printed = self.print_expression(argument);
                parts.push(self.concat([
                    self.s(" ("),
                    self.indent(self.concat([&HARDLINE, printed])),
                    &HARDLINE,
                    self.s(")"),
                ]));
            } else if super::parens::is_binaryish(argument)
                || matches!(**argument, expression::ExpressionInner::Sequence { .. })
            {
                let printed = self.print_expression(argument);
                parts.push(self.group(self.concat([
                    self.s(" "),
                    self.if_break(self.s("("), self.s("")),
                    self.indent(self.concat([&SOFTLINE, printed])),
                    &SOFTLINE,
                    self.if_break(self.s(")"), self.s("")),
                ])));
            } else {
                let printed = self.print_expression(argument);
                parts.push(self.s(" "));
                parts.push(printed);
            }
        }
        let last_is_line = self
            .comments
            .slots(key)
            .and_then(|slots| {
                slots
                    .dangling
                    .last()
                    .or(slots.trailing.last())
                    .map(|index| {
                        matches!(
                            self.comments.get(*index).kind,
                            uf_flow::ast::CommentKind::Line
                        )
                    })
            })
            .unwrap_or(false);
        if last_is_line {
            parts.push(self.semi());
        }
        if let Some(dangling) = self.print_dangling_comments(key, Marker::None, false) {
            parts.push(self.s(" "));
            parts.push(dangling);
        }
        if !last_is_line {
            parts.push(self.semi());
        }
        self.docs.concat_vec(parts)
    }

    /// Whether a returned expression, or the left-most expression it
    /// starts with, has a comment on its own line before it.
    fn return_argument_has_leading_comment(&self, argument: &'a Expression) -> bool {
        if self.has_leading_own_line_comment(
            NodeRef::Expression(argument).key(),
            super::parens::is_jsx(argument),
        ) {
            return true;
        }
        if super::parens::has_naked_left_side(argument) {
            let mut left_most = argument;
            while let Some(next) = super::parens::left_side(left_most) {
                left_most = next;
                if self.has_leading_own_line_comment(
                    NodeRef::Expression(left_most).key(),
                    super::parens::is_jsx(left_most),
                ) {
                    return true;
                }
            }
        }
        false
    }

    /// How an empty block *statement* prints, from what encloses it.
    ///
    /// A block statement is the same node whether it is a loop body, an `if`
    /// branch, a `case` body or a block somebody wrote on its own, so this is
    /// the one place the parent has to be asked. Everywhere else the caller
    /// knows what it is printing and says so.
    ///
    /// The block is on top of the stack while its statement is being printed,
    /// so the parent is what is under it.
    fn empty_block_of_statement(&self) -> EmptyBlock {
        use statement::StatementInner as S;
        match self.parent() {
            // `for … in` and `for … of` are deliberately not here: Prettier
            // keeps those open and collapses only the three-part `for`.
            Some(NodeRef::Statement(parent)) => match &**parent {
                S::For { .. } | S::While { .. } | S::DoWhile { .. } => EmptyBlock::Collapsed,
                _ => EmptyBlock::Open,
            },
            // A `match` arm is a body like a function's, not a branch like an
            // `if`: `_ => {}` is how an arm says it does nothing.
            Some(NodeRef::MatchStatementCase(_)) => EmptyBlock::Collapsed,
            _ => EmptyBlock::Open,
        }
    }

    /// A `{ … }` block that is not itself a statement.
    pub fn print_block(
        &mut self,
        loc: &'a Loc,
        block: &'a statement::Block<Loc, Loc>,
        empty: EmptyBlock,
    ) -> Doc<'a> {
        let node = NodeRef::Block(loc, block);
        self.print_node(node, |p| p.print_block_body(&block.body, node.key(), empty))
    }

    /// `{`, the statements, `}`; `{}` or `{`/`}` on two lines when there is
    /// nothing to print, as [`EmptyBlock`] says.
    pub fn print_block_body(
        &mut self,
        body: &'a [Statement],
        key: crate::flow::node::NodeKey,
        empty: EmptyBlock,
    ) -> Doc<'a> {
        let has_body = body.iter().any(|statement| !is_empty_statement(statement));
        let dangling = self.print_dangling_comments(key, Marker::None, false);
        if !has_body && dangling.is_none() {
            return match empty {
                EmptyBlock::Collapsed => self.s("{}"),
                EmptyBlock::Open => self.concat([self.s("{"), &HARDLINE, self.s("}")]),
            };
        }
        let mut parts = Vec::new();
        if has_body {
            parts.push(self.print_statement_sequence(body));
        }
        if let Some(dangling) = dangling {
            parts.push(dangling);
        }
        let inner = self.docs.concat_vec(parts);
        self.concat([
            self.s("{"),
            self.indent(self.concat([&HARDLINE, inner])),
            &HARDLINE,
            self.s("}"),
        ])
    }

    /// `const a = 1, b = 2`, breaking one declarator per line when any has
    /// an initializer.
    pub fn print_variable_declaration(
        &mut self,
        _loc: &'a Loc,
        declaration: &'a statement::VariableDeclaration<Loc, Loc>,
        in_loop_head: bool,
    ) -> Doc<'a> {
        let printed: Vec<Doc<'a>> = declaration
            .declarations
            .iter()
            .map(|declarator| self.print_declarator(declarator))
            .collect();
        let has_value = declaration
            .declarations
            .iter()
            .any(|declarator| declarator.init.is_some());
        let first = match printed.first() {
            Some(first)
                if printed.len() == 1
                    && !self
                        .has_comment(NodeRef::Declarator(&declaration.declarations[0]).key()) =>
            {
                Some(*first)
            }
            Some(first) => Some(self.indent(first)),
            None => None,
        };
        let separator = if has_value && !in_loop_head {
            &HARDLINE
        } else {
            &LINE
        };
        let rest: Vec<Doc<'a>> = printed
            .iter()
            .skip(1)
            .map(|declarator| self.concat([self.s(","), separator, *declarator]))
            .collect();
        let mut parts = vec![self.s(declaration.kind.as_str())];
        if let Some(first) = first {
            parts.push(self.s(" "));
            parts.push(first);
        }
        parts.push(self.indent(self.docs.concat_vec(rest)));
        if !in_loop_head {
            parts.push(self.semi());
        }
        self.group(self.docs.concat_vec(parts))
    }

    fn print_declarator(
        &mut self,
        declarator: &'a statement::variable::Declarator<Loc, Loc>,
    ) -> Doc<'a> {
        self.print_node(NodeRef::Declarator(declarator), |p| {
            let left = p.print_pattern(&declarator.id);
            p.print_assignment_like(
                NodeRef::Declarator(declarator),
                left,
                " =",
                declarator
                    .init
                    .as_ref()
                    .map(super::assignment::Rhs::Expression),
            )
        })
    }

    /// Whether, without semicolons, `statement` must be guarded by a
    /// leading `;` because it starts with a token that would otherwise
    /// continue the previous line.
    pub fn statement_needs_asi_protection(&mut self, statement: &'a Statement) -> bool {
        let statement::StatementInner::Expression { inner, .. } = &**statement else {
            return false;
        };
        self.with_node(NodeRef::Statement(statement), |p| {
            p.expression_needs_asi_protection(&inner.expression)
        })
    }

    fn expression_needs_asi_protection(&mut self, expression: &'a Expression) -> bool {
        use expression::ExpressionInner as E;
        let node = NodeRef::Expression(expression);
        let starts_dangerously = match &**expression {
            E::TypeCast { .. }
            | E::Array { .. }
            | E::TemplateLiteral { .. }
            | E::RegExpLiteral { .. } => true,
            E::ArrowFunction { .. } => true,
            E::Unary { inner, .. } => matches!(
                inner.operator,
                expression::UnaryOperator::Plus | expression::UnaryOperator::Minus
            ),
            E::JSXElement { .. } | E::JSXFragment { .. } => true,
            _ => false,
        };
        if starts_dangerously {
            return true;
        }
        if self.with_node(node, |p| p.needs_parens(expression)) {
            return true;
        }
        if !super::parens::has_naked_left_side(expression) {
            return false;
        }
        match super::parens::left_side(expression) {
            Some(left) => self.with_node(node, |p| p.expression_needs_asi_protection(left)),
            None => false,
        }
    }
}
