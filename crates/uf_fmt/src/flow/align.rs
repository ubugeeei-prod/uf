//! Optional, token-based column alignment after the ordinary Flow printer.
//!
//! The printer decides every line break first. This pass only inserts spaces
//! before tokens that share an AST container. A hook call or a match arm may
//! continue onto later lines; the `=` or `=>` on its first line still takes
//! the column. Re-parsing the printed text gives the inserted spaces stable
//! byte positions; the normal printer removes them on the next run and this
//! pass reapplies the same columns, so alignment stays idempotent.

use uf_flow::Loc;
use uf_flow::ast::{self, expression, function, pattern, statement, types};
use uf_flow::ast_visitor::{self, AstVisitor};

use crate::doc::printer::text_width;
use crate::flow::text::{SourceText, Span};

#[derive(Clone, Copy)]
struct Stop {
    at: usize,
    entry_end: usize,
    line_start: usize,
    line_end: usize,
    column: usize,
}

#[derive(Clone, Copy)]
struct Edit {
    at: usize,
    spaces: usize,
}

#[derive(Clone, Copy)]
struct UseEntry {
    equal: Stop,
    second_binding: Option<Stop>,
    /// First byte of the line after this statement. A call that continues
    /// past the `=` still joins the next hook.
    next_line: usize,
}

struct Collector<'a> {
    text: SourceText<'a>,
    line_width: usize,
    edits: Vec<Edit>,
}

/// Align the printed source, or return it unchanged when the printed tree
/// cannot be read. The ordinary printer has already validated the input.
pub(super) fn align(source: String, line_width: usize) -> String {
    let Ok(parsed) = uf_flow::parse(&source) else {
        return source;
    };
    if !parsed.is_ok() {
        return source;
    }
    let mut collector = Collector {
        text: SourceText::new(&source),
        line_width,
        edits: Vec::new(),
    };
    let walked: Result<(), ()> = collector.program(&parsed.program);
    debug_assert!(walked.is_ok());
    let mut edits = collector.edits;
    if edits.is_empty() {
        return source;
    }
    edits.sort_by_key(|edit| edit.at);
    let mut output =
        String::with_capacity(source.len() + edits.iter().map(|edit| edit.spaces).sum::<usize>());
    let mut copied = 0;
    for edit in edits {
        output.push_str(&source[copied..edit.at]);
        output.extend(std::iter::repeat_n(' ', edit.spaces));
        copied = edit.at;
    }
    output.push_str(&source[copied..]);
    output
}

impl Collector<'_> {
    fn stop(&self, span: Span, at: usize) -> Option<Stop> {
        let source = self.text.text();
        if at < span.start || at >= span.end || !source.is_char_boundary(at) {
            return None;
        }
        let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
        let line_end = source[at..]
            .find('\n')
            .map_or(source.len(), |offset| at + offset);
        if span.start < line_start || span.end > line_end {
            return None;
        }
        let prefix = &source[line_start..at];
        if prefix.contains("//") || prefix.contains("/*") {
            return None;
        }
        Some(Stop {
            at,
            entry_end: span.end,
            line_start,
            line_end,
            column: text_width(prefix),
        })
    }

    /// [`Self::stop`] for a token whose entry continues onto later lines.
    ///
    /// The column is the token's own line. Object values stay single-line,
    /// because a value that breaks should not share a column with its
    /// neighbours; hooks and match arrows opt in here.
    fn stop_on_its_line(&self, span: Span, at: usize) -> Option<Stop> {
        let source = self.text.text();
        if at < span.start || at >= span.end || !source.is_char_boundary(at) {
            return None;
        }
        let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
        let line_end = source[at..]
            .find('\n')
            .map_or(source.len(), |offset| at + offset);
        self.stop(
            Span {
                start: line_start.max(span.start),
                end: line_end.min(span.end),
            },
            at,
        )
    }

    /// The first byte of the line after the one containing `end - 1`.
    fn line_after(&self, end: usize) -> usize {
        let source = self.text.text();
        if end == 0 || end > source.len() {
            return source.len();
        }
        let last = end - 1;
        let line_end = source[last..]
            .find('\n')
            .map_or(source.len(), |offset| last + offset);
        if line_end < source.len() {
            line_end + 1
        } else {
            source.len()
        }
    }

    fn trailing_comment(&self, stop: &Stop) -> bool {
        let suffix = &self.text.text()[stop.entry_end..stop.line_end];
        suffix.contains("//") || suffix.contains("/*")
    }

    fn align_stops(&mut self, stops: &[Option<Stop>]) {
        let mut run = Vec::new();
        for item in stops.iter().copied().chain(std::iter::once(None)) {
            match item {
                Some(stop)
                    if run.last().is_none_or(|previous: &Stop| {
                        previous.line_end + 1 == stop.line_start && !self.trailing_comment(previous)
                    }) =>
                {
                    run.push(stop);
                }
                Some(stop) => {
                    self.align_stop_run(&run);
                    run.clear();
                    run.push(stop);
                }
                None => {
                    self.align_stop_run(&run);
                    run.clear();
                }
            }
        }
    }

    fn align_stop_run(&mut self, run: &[Stop]) {
        if run.len() < 2 {
            return;
        }
        let max_column = run.iter().map(|stop| stop.column).max().unwrap_or(0);
        if run.iter().any(|stop| {
            text_width(&self.text.text()[stop.line_start..stop.line_end])
                + max_column.saturating_sub(stop.column)
                > self.line_width
        }) {
            return;
        }
        for stop in run {
            let spaces = max_column - stop.column;
            if spaces > 0 {
                self.edits.push(Edit {
                    at: stop.at,
                    spaces,
                });
            }
        }
    }

    fn case_arrow_stop(
        &self,
        loc: &Loc,
        pattern: &ast::match_pattern::MatchPattern<Loc, Loc>,
        guard: Option<&expression::Expression<Loc, Loc>>,
        body: &Loc,
    ) -> Option<Stop> {
        let case = self.text.span(loc);
        let after_head = guard.map_or_else(
            || self.text.span(pattern.loc()).end,
            |guard| self.text.span(guard.loc()).end,
        );
        let before_body = self.text.span(body).start;
        let gap = self.text.text().get(after_head..before_body)?;
        let relative = gap.rfind("=>")?;
        let at = after_head + relative;
        if !gap[relative + 2..].trim().is_empty() {
            return None;
        }
        self.stop_on_its_line(case, at)
    }

    /// Every arm of one match, including arms whose body continues below the
    /// `=>`. The arrow column has to sit inside the line width; the body after
    /// it may run past that width, which is the line the printer already chose.
    fn align_arrows(&mut self, stops: &[Option<Stop>]) {
        let run: Vec<Stop> = stops.iter().copied().flatten().collect();
        if run.len() < 2 {
            return;
        }
        let max_column = run.iter().map(|stop| stop.column).max().unwrap_or(0);
        if max_column + 2 > self.line_width {
            return;
        }
        for stop in &run {
            let spaces = max_column - stop.column;
            if spaces > 0 {
                self.edits.push(Edit {
                    at: stop.at,
                    spaces,
                });
            }
        }
    }

    fn object_stops(&self, object: &expression::Object<Loc, Loc>) -> Vec<Option<Stop>> {
        object
            .properties
            .iter()
            .map(|property| {
                let expression::object::Property::NormalProperty(
                    expression::object::NormalProperty::Init {
                        loc,
                        value,
                        shorthand: false,
                        ..
                    },
                ) = property
                else {
                    return None;
                };
                let property_span = self.text.span(loc);
                let before_value = self.text.span(value.loc()).start;
                let gap = self.text.text().get(property_span.start..before_value)?;
                let at = property_span.start + gap.rfind(':')?;
                if !self.text.text()[at + 1..before_value].trim().is_empty() {
                    return None;
                }
                self.stop(property_span, at)
            })
            .collect()
    }

    fn object_type_stops(&self, object: &types::Object<Loc, Loc>) -> Vec<Option<Stop>> {
        object
            .properties
            .iter()
            .map(|property| {
                let types::object::Property::NormalProperty(property) = property else {
                    return None;
                };
                let types::object::PropertyValue::Init(Some(value)) = &property.value else {
                    return None;
                };
                let property_span = self.text.span(&property.loc);
                let before_value = self.text.span(value.loc()).start;
                let gap = self.text.text().get(property_span.start..before_value)?;
                let at = property_span.start + gap.rfind(':')?;
                if !self.text.text()[at + 1..before_value].trim().is_empty() {
                    return None;
                }
                self.stop(property_span, at)
            })
            .collect()
    }

    fn type_alias_stop(&self, statement: &statement::Statement<Loc, Loc>) -> Option<Stop> {
        let alias = match &**statement {
            statement::StatementInner::TypeAlias { inner, .. } => inner,
            statement::StatementInner::ExportNamedDeclaration { inner, .. } => {
                let declaration = inner.declaration.as_ref()?;
                let statement::StatementInner::TypeAlias { inner, .. } = &**declaration else {
                    return None;
                };
                inner
            }
            _ => return None,
        };
        let after_name = alias.tparams.as_ref().map_or_else(
            || self.text.span(&alias.id.loc).end,
            |params| self.text.span(&params.loc).end,
        );
        let before_value = self.text.span(alias.right.loc()).start;
        let gap = self.text.text().get(after_name..before_value)?;
        let relative = gap.find('=')?;
        if !gap[..relative].trim().is_empty() || !gap[relative + 1..].trim().is_empty() {
            return None;
        }
        self.stop(self.text.span(statement.loc()), after_name + relative)
    }

    fn function_param_stops(&self, params: &function::Params<Loc, Loc>) -> Vec<Option<Stop>> {
        params
            .params
            .iter()
            .map(|param| {
                let function::Param::RegularParam {
                    loc,
                    argument: pattern::Pattern::Identifier { inner, .. },
                    ..
                } = param
                else {
                    return None;
                };
                let types::AnnotationOrHint::Available(annotation) = &inner.annot else {
                    return None;
                };
                let at = self.text.span(&annotation.loc).start;
                (self.text.text().as_bytes().get(at) == Some(&b':'))
                    .then(|| self.stop(self.text.span(loc), at))?
            })
            .collect()
    }

    fn component_param_stops(
        &self,
        params: &statement::component_params::Params<Loc, Loc>,
    ) -> Vec<Option<Stop>> {
        params
            .params
            .iter()
            .map(|param| {
                let pattern::Pattern::Identifier { inner, .. } = &param.local else {
                    return None;
                };
                let types::AnnotationOrHint::Available(annotation) = &inner.annot else {
                    return None;
                };
                let at = self.text.span(&annotation.loc).start;
                (self.text.text().as_bytes().get(at) == Some(&b':'))
                    .then(|| self.stop(self.text.span(&param.loc), at))?
            })
            .collect()
    }

    fn function_type_param_stops(&self, function: &types::Function<Loc, Loc>) -> Vec<Option<Stop>> {
        function
            .params
            .params
            .iter()
            .map(|param| {
                let types::function::ParamKind::Labeled { name, annot, .. } = &param.param else {
                    return None;
                };
                let after_name = self.text.span(&name.loc).end;
                let before_type = self.text.span(annot.loc()).start;
                let gap = self.text.text().get(after_name..before_type)?;
                let relative = gap.rfind(':')?;
                if !gap[relative + 1..].trim().is_empty() {
                    return None;
                }
                self.stop(self.text.span(&param.loc), after_name + relative)
            })
            .collect()
    }

    fn use_entry(&self, statement: &statement::Statement<Loc, Loc>) -> Option<UseEntry> {
        let statement::StatementInner::VariableDeclaration { inner, .. } = &**statement else {
            return None;
        };
        if inner.kind != ast::VariableKind::Const || inner.declarations.len() != 1 {
            return None;
        }
        let declarator = &inner.declarations[0];
        let init = declarator.init.as_ref()?;
        let init_span = self.text.span(init.loc());
        let init_source = self.text.slice(init_span);
        let hook = init_source.strip_prefix("use")?;
        if !hook
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        {
            return None;
        }
        let statement_span = self.text.span(statement.loc());
        let after_id = self.text.span(declarator.id.loc()).end;
        let gap = self.text.text().get(after_id..init_span.start)?;
        let at = after_id + gap.rfind('=')?;
        if !gap[..at - after_id].trim().is_empty() || !gap[at - after_id + 1..].trim().is_empty() {
            return None;
        }
        let equal = self.stop_on_its_line(statement_span, at)?;
        let second_binding = self.second_binding(&declarator.id, statement_span);
        Some(UseEntry {
            equal,
            second_binding,
            next_line: self.line_after(statement_span.end),
        })
    }

    fn second_binding(&self, id: &pattern::Pattern<Loc, Loc>, statement: Span) -> Option<Stop> {
        let pattern::Pattern::Array { inner, .. } = id else {
            return None;
        };
        let [
            pattern::array::Element::NormalElement(first),
            pattern::array::Element::NormalElement(second),
        ] = inner.elements.as_ref()
        else {
            return None;
        };
        if !matches!(first.argument, pattern::Pattern::Identifier { .. })
            || !matches!(second.argument, pattern::Pattern::Identifier { .. })
        {
            return None;
        }
        let first_end = self.text.span(first.argument.loc()).end;
        let second_start = self.text.span(second.argument.loc()).start;
        let between = self.text.text().get(first_end..second_start)?;
        let comma = between.find(',')?;
        if !between[comma + 1..].trim().is_empty() {
            return None;
        }
        self.stop_on_its_line(statement, second_start)
    }

    fn align_uses(&mut self, statements: &[statement::Statement<Loc, Loc>]) {
        let mut run = Vec::new();
        for statement in statements {
            match self.use_entry(statement) {
                Some(entry)
                    if run.last().is_none_or(|previous: &UseEntry| {
                        previous.next_line == entry.equal.line_start
                            && !self.trailing_comment(&previous.equal)
                    }) =>
                {
                    run.push(entry);
                }
                Some(entry) => {
                    self.align_use_run(&run);
                    run.clear();
                    run.push(entry);
                }
                None => {
                    self.align_use_run(&run);
                    run.clear();
                }
            }
        }
        self.align_use_run(&run);
    }

    fn align_use_run(&mut self, run: &[UseEntry]) {
        if run.len() < 2 {
            return;
        }
        let max_second = run
            .iter()
            .filter_map(|entry| entry.second_binding.map(|stop| stop.column))
            .max();
        let second_count = run
            .iter()
            .filter(|entry| entry.second_binding.is_some())
            .count();
        let adjusted_equal = |entry: &UseEntry| {
            entry.equal.column
                + if second_count >= 2 {
                    entry
                        .second_binding
                        .map_or(0, |stop| max_second.unwrap_or(0) - stop.column)
                } else {
                    0
                }
        };
        let max_equal = run.iter().map(adjusted_equal).max().unwrap_or(0);
        if run.iter().any(|entry| {
            text_width(&self.text.text()[entry.equal.line_start..entry.equal.line_end]) + max_equal
                - entry.equal.column
                > self.line_width
        }) {
            return;
        }
        for entry in run {
            if second_count >= 2
                && let Some(second) = entry.second_binding
            {
                let spaces = max_second.unwrap_or(0) - second.column;
                if spaces > 0 {
                    self.edits.push(Edit {
                        at: second.at,
                        spaces,
                    });
                }
            }
            let spaces = max_equal - adjusted_equal(entry);
            if spaces > 0 {
                self.edits.push(Edit {
                    at: entry.equal.at,
                    spaces,
                });
            }
        }
    }
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for Collector<'_> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn statement_list(
        &mut self,
        statements: &'ast [statement::Statement<Loc, Loc>],
    ) -> Result<(), ()> {
        let stops = statements
            .iter()
            .map(|statement| self.type_alias_stop(statement))
            .collect::<Vec<_>>();
        self.align_stops(&stops);
        self.align_uses(statements);
        ast_visitor::statement_list_default(self, statements)
    }

    fn function_params(&mut self, params: &'ast function::Params<Loc, Loc>) -> Result<(), ()> {
        let stops = self.function_param_stops(params);
        self.align_stops(&stops);
        ast_visitor::function_params_default(self, params)
    }

    fn component_params(
        &mut self,
        params: &'ast statement::component_params::Params<Loc, Loc>,
    ) -> Result<(), ()> {
        let stops = self.component_param_stops(params);
        self.align_stops(&stops);
        ast_visitor::component_params_default(self, params)
    }

    fn function_type(&mut self, function: &'ast types::Function<Loc, Loc>) -> Result<(), ()> {
        let stops = self.function_type_param_stops(function);
        self.align_stops(&stops);
        ast_visitor::function_type_default(self, function)
    }

    fn object_type(&mut self, object: &'ast types::Object<Loc, Loc>) -> Result<(), ()> {
        let stops = self.object_type_stops(object);
        self.align_stops(&stops);
        ast_visitor::object_type_default(self, object)
    }

    fn match_expression(
        &mut self,
        loc: &'ast Loc,
        m: &'ast expression::MatchExpression<Loc, Loc>,
    ) -> Result<(), ()> {
        let stops = m
            .cases
            .iter()
            .map(|case| {
                self.case_arrow_stop(
                    &case.loc,
                    &case.pattern,
                    case.guard.as_ref(),
                    case.body.loc(),
                )
            })
            .collect::<Vec<_>>();
        self.align_arrows(&stops);
        ast_visitor::match_expression_default(self, loc, m)
    }

    fn match_statement(
        &mut self,
        loc: &'ast Loc,
        m: &'ast statement::MatchStatement<Loc, Loc>,
    ) -> Result<(), ()> {
        let stops = m
            .cases
            .iter()
            .map(|case| {
                self.case_arrow_stop(
                    &case.loc,
                    &case.pattern,
                    case.guard.as_ref(),
                    case.body.loc(),
                )
            })
            .collect::<Vec<_>>();
        self.align_arrows(&stops);
        ast_visitor::match_statement_default(self, loc, m)
    }

    fn object(
        &mut self,
        loc: &'ast Loc,
        object: &'ast expression::Object<Loc, Loc>,
    ) -> Result<(), ()> {
        let stops = self.object_stops(object);
        self.align_stops(&stops);
        ast_visitor::object_default(self, loc, object)
    }
}
