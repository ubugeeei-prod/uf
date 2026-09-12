//! The two `react/*` rules that read the module's tree rather than its lines.
//!
//! Every other rule in this crate answers its question from [`FileScan`] — a
//! line, its code, the brace depth it opens at. These two cannot:
//!
//! * `react/no-derived-state-effect` has to know that a name is a `useState`
//!   setter declared in the same render function, that an effect's body is one
//!   statement and not two, and that every name in the value it stores is in
//!   the effect's dependency array. A line scanner can see none of that.
//! * `react/no-redundant-memo` has to know what the **official React Compiler**
//!   did with the module, which is not a question about the source text at all.
//!   [`uf_transform::memo`] holds that answer; this runner reports it.
//!
//! So both go through `uf_transform`: the official Flow parser, Flow's own
//! lowering rules, and Babel's AST shape — the same three stages `uf build`
//! runs, so the tree the linter reads is the tree the compiler is given.
//!
//! # What this costs, and when it is paid
//!
//! Parsing a module again is far more work than a line scan, and compiling one
//! is more again. Four gates stand in front of it, in order of cheapness:
//!
//! 1. Neither rule is enabled — nothing runs.
//! 2. The path is not Flow source — nothing runs.
//! 3. The text holds no call-shaped `useEffect`, `useMemo` or `useCallback` —
//!    nothing runs. A module without one of those calls cannot violate either
//!    rule.
//! 4. The module is nested or chained past [`uf_flow`]'s parser ceilings —
//!    nothing runs, because `flow/syntax` has already refused it and a linter
//!    must not be the thing that overflows a stack on a minified bundle.
//!
//! Past those, the parse happens on a thread of
//! [`PARSE_STACK_BYTES`](uf_flow::PARSE_STACK_BYTES), for the reason
//! `uf_flow::upstream` spells out: the port's frames are large, the tree is
//! freed where it was built, and a lint worker's own stack is not enough.

use serde_json::Value;
use uf_config::UniflowedConfig;
use uf_flow::ast_visitor::{self, AstVisitor};
use uf_flow::{Loc, ast};
use uf_infra::{FxHashMap, FxHashSet};
use uf_profiler::profile_span;
use uf_transform::{ReactCompilerMode, TransformOptions};

use crate::scan::{FileScan, find_words, next_non_space};
use crate::{Diagnostic, push_at, severity};

/// `react/no-derived-state-effect`.
const DERIVED_STATE: &str = "react/no-derived-state-effect";

/// `react/no-redundant-memo`.
const REDUNDANT_MEMO: &str = "react/no-redundant-memo";

/// The effect hooks this rule reads.
///
/// `useEffect` only. `useLayoutEffect` is where measuring the DOM belongs, and
/// a measurement is exactly the shape this rule must not move into render.
const EFFECT: &str = "useEffect";

/// A module that does not mention `useState` cannot produce a finding.
///
/// The rule reports an effect whose only job is to store a value derived from
/// its dependencies, and it finds those stores through the **setters**
/// `declared_state` collects from `useState` destructuring. No `useState`, no
/// setter, no finding — and building a Babel AST to discover that is 790,000
/// allocations and 65 MiB for a 111 KiB module, which is 94% of what `uf lint`
/// spends on it. See ubugeeei-prod/uf#668.
///
/// # What a textual gate cannot see
///
/// An identifier may be written with escapes: `useSt\u0061te` is `useState` to
/// the parser and is not this string. Such a module is skipped and its finding
/// is missed — a false negative, never a false positive, which is the right
/// direction for a rule at `error`.
///
/// The same has always been true of [`EFFECT`], so this is a property of
/// deciding without parsing rather than one this constant introduced. Closing
/// it means unescaping the source before the test, which is the work the gate
/// exists to avoid, on every module, to catch a spelling nobody writes.
const STATE: &str = "useState";

/// What these rules want out of a parse, or [`None`] when they want none.
///
/// Split from the analysis so that one parse can serve every runner that
/// needs the module's tree — see [`super::module_tree`], which owns it.
pub(super) struct ReactWork {
    derived: Option<crate::Severity>,
    memo: Option<crate::Severity>,
    wants_effects: bool,
    wants_memo: bool,
}

/// Whether these rules want this module read at all.
pub(super) fn wanted(scan: &FileScan<'_>, config: &UniflowedConfig) -> Option<ReactWork> {
    let derived = severity(config, DERIVED_STATE);
    // A project that has turned the compiler off gets no report: without it,
    // a hand-written `useMemo` is the only memoization there is.
    let memo = config
        .app
        .builtins
        .react_compiler
        .enabled
        .then(|| severity(config, REDUNDANT_MEMO))
        .flatten();
    if derived.is_none() && memo.is_none() {
        return None;
    }
    if !super::flow_syntax::is_flow_syntax_target(&scan.file.path) {
        return None;
    }

    // Both calls, not just the effect: see `STATE`. Textual on purpose — the
    // point is to decide without parsing. Read the scanner's code slices rather
    // than the whole source so a comment, prose string or import-only mention
    // of a hook does not pay for the module-tree path.
    let wants_effects =
        derived.is_some() && mentions_code_call(scan, EFFECT) && mentions_code_call(scan, STATE);
    let wants_memo = memo.is_some()
        && (mentions_code_call(scan, "useMemo") || mentions_code_call(scan, "useCallback"));
    if !wants_effects && !wants_memo {
        return None;
    }
    Some(ReactWork {
        derived,
        memo,
        wants_effects,
        wants_memo,
    })
}

/// Whether a hook-ish name is present where code can call it.
///
/// This is still a cheap textual gate, not binding analysis. It is deliberately
/// narrower than `source.contains`: comments, string prose and imports cannot
/// be hook calls, and those false positives are exactly what make `uf lint`
/// enter the allocation-heavy tree path for modules that cannot report
/// anything here.
fn mentions_code_call(scan: &FileScan<'_>, word: &str) -> bool {
    scan.lines.iter().any(|line| {
        let code = line.code();
        find_words(code, word)
            .any(|at| !line.in_string(at) && call_follows_name(code, at + word.len()))
    })
}

/// Whether the next non-space token after a name can still be the same call.
///
/// `useMemo<T>(...)` is a call too, so `<` is accepted with `(`. The gate stays
/// local to the line: if somebody splits a hook callee from its argument list
/// across lines, this errs toward skipping the expensive optional rule rather
/// than paying the #668 path for import-only modules.
fn call_follows_name(code: &str, after: usize) -> bool {
    next_non_space(code, after).is_some_and(|(_, byte)| matches!(byte, b'(' | b'<'))
}

/// Run whichever of the two rules was asked for, over a tree somebody else
/// parsed.
///
/// # Call this on the thread that built `parsed`
///
/// The lowering recurses over the tree; [`super::module_tree`] is the caller
/// and is on a thread with `uf_flow::PARSE_STACK_BYTES`.
pub(super) fn analyse_parsed(
    parsed: &uf_flow::Parsed,
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    work: &ReactWork,
) -> Vec<TreeFinding> {
    profile_span!("run_react_tree_rules");
    let source = &scan.file.source;
    let options = TransformOptions {
        react_compiler: compiler_mode(config),
        ..TransformOptions::new(scan.file.path.clone())
    };

    if work.wants_effects && !work.wants_memo {
        return derived_state_effects_native(parsed);
    }

    // Lowered, not raw: `component` and `match` are Flow's own syntax, and
    // this rule reads functions and calls. After the lowering a component
    // *is* a `FunctionDeclaration`, which is the tree the rule was always
    // written against.
    //
    // From the tree the caller already holds rather than from the source:
    // `uf lint` used to hand the text back to `estree::parse` here and have
    // the module parsed a second time. See ubugeeei-prod/uf#668.
    let Ok((program, _)) = uf_transform::lowered_from_parsed(&parsed.program, source) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if work.wants_effects {
        found.extend(derived_state_effects(&program));
    }
    // An error here is a bug in uf rather than in the module — the tree did
    // not fit the compiler's own schema — and it says nothing about the
    // effects the other rule already found, so it costs that rule nothing.
    if work.wants_memo
        && let Ok(file) = uf_transform::babel_from_lowered(program, source)
        && let Ok(redundant) = uf_transform::redundant_memoization(&file, source, &options)
    {
        found.extend(redundant.into_iter().map(|memo| TreeFinding {
            kind: FindingKind::RedundantMemo,
            line: memo.line,
            column: memo.column,
            column_unit: ColumnUnit::Utf16,
            message: format!(
                "the React Compiler memoizes this already; `{}` here is a second dependency array to keep correct",
                memo.hook
            ),
        }));
    }
    found
}

/// Turn what the analysis found into diagnostics.
pub(super) fn report(
    scan: &FileScan<'_>,
    work: &ReactWork,
    found: Vec<TreeFinding>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let source = &scan.file.source;
    for finding in found {
        let rule = match finding.kind {
            FindingKind::DerivedState => DERIVED_STATE,
            FindingKind::RedundantMemo => REDUNDANT_MEMO,
        };
        let level = match finding.kind {
            FindingKind::DerivedState => work.derived,
            FindingKind::RedundantMemo => work.memo,
        };
        let Some(index) = usize::try_from(finding.line)
            .ok()
            .and_then(|line| line.checked_sub(1))
        else {
            continue;
        };
        let (Some(level), Some(line)) = (level, scan.lines.get(index)) else {
            continue;
        };
        // A diagnostic carries bytes, and the conversion reads the line out of
        // the *unmasked* source: `mask_inline_comments` replaces a comment byte
        // for byte, which keeps every offset but not every character.
        let text = source
            .get(line.offset..line.offset + line.text.len())
            .unwrap_or(line.text);
        // The two rules read two trees now, and the trees count columns
        // differently: the Flow translator's `loc` counts code points, and
        // `babel::finalize` recomputes them as UTF-16 code units. Both are the
        // same number until a line holds an astral character, and then they are
        // not — so the unit is chosen by which tree the finding came from
        // rather than assumed to be one of them.
        let column = match finding.column_unit {
            ColumnUnit::Byte => usize::try_from(finding.column).unwrap_or(0).min(text.len()),
            ColumnUnit::CodePoint => byte_column_of_code_point(text, finding.column),
            ColumnUnit::Utf16 => byte_column(text, finding.column),
        };
        push_at(
            diagnostics,
            scan,
            rule,
            level,
            index,
            column,
            finding.message,
        );
    }
}

/// Which rule a finding belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FindingKind {
    DerivedState,
    RedundantMemo,
}

/// One finding, positioned the way the tree positions things.
pub(super) struct TreeFinding {
    kind: FindingKind,
    /// 1-based line.
    line: u32,
    /// 0-based column, counted according to [`column_unit`].
    column: u32,
    column_unit: ColumnUnit,
    message: String,
}

/// Which unit a finding's column is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColumnUnit {
    Byte,
    CodePoint,
    Utf16,
}

/// The mode uf would compile this project's modules in.
///
/// A `match` rather than a default, so that a mode added to `uf.config.js`
/// cannot reach the compiler here under the wrong name: `uf lint` and
/// `uf build` have to ask the compiler the same question.
fn compiler_mode(config: &UniflowedConfig) -> ReactCompilerMode {
    match config.app.builtins.react_compiler.mode {
        uf_config::ReactCompilerMode::Syntax => ReactCompilerMode::Syntax,
    }
}

/// Byte offset within `line` of the `column`-th UTF-16 code unit.
///
/// What `react/no-redundant-memo` needs: it reads the Babel tree, whose `loc`
/// `babel::finalize` recomputed in UTF-16 units.
fn byte_column(line: &str, column: u32) -> usize {
    let mut units = 0u32;
    for (offset, character) in line.char_indices() {
        if units >= column {
            return offset;
        }
        units += u32::try_from(character.len_utf16()).unwrap_or(u32::MAX);
    }
    line.len()
}

/// Byte offset within `line` of the `column`-th code point.
///
/// What `react/no-derived-state-effect` needs: it reads the lowered ESTree
/// tree, and the Flow translator's `loc` columns count code points. The two
/// agree on every line that is entirely BMP and part company on one that is
/// not — an emoji before an effect would put the caret one column early.
fn byte_column_of_code_point(line: &str, column: u32) -> usize {
    line.char_indices()
        .nth(column as usize)
        .map_or(line.len(), |(offset, _)| offset)
}

// --- react/no-derived-state-effect -----------------------------------------

/// Every effect in `file` whose whole body is one write of state derived from
/// its own dependencies.
///
/// # What is reported
///
/// All of these have to hold, and the list is short because each item removes
/// a way the rule could be wrong:
///
/// * the call is `useEffect`, with exactly two arguments;
/// * the first is an arrow with no parameters and no `async`;
/// * its body is exactly one expression statement — so there is no cleanup to
///   return, and nothing else the effect also does;
/// * that expression calls a setter destructured from a `useState` in an
///   enclosing function, with exactly one argument;
/// * the second argument is a non-empty array of plain identifiers;
/// * the argument to the setter is built from those identifiers, literals and
///   operators, and mentions at least one of them.
///
/// The last condition is what makes the value computable during render: an
/// expression over the effect's own dependencies, with no call and no property
/// read in it, is total and pure, and React's ["You Might Not Need an
/// Effect"](https://react.dev/learn/you-might-not-need-an-effect) is about
/// exactly that shape.
///
/// # What is deliberately not reported
///
/// * **A property read.** `setItems(data.items)` and `setWidth(box.offsetWidth)`
///   are the same three tokens, and the first belongs in render while the
///   second must not go there — a layout read during render is a bug uf's own
///   `react/no-render-side-effects` exists to catch. The source text does not
///   say which is which, so the rule stops here rather than guessing.
/// * **A call.** `setSorted(items.slice().sort())` may be pure and may not be;
///   nothing in the tree says.
/// * **A reset to a constant.** `setSelection(null)` on `[items]` is the
///   article's *other* shape, and its fix — a `key`, or a comparison against
///   the previous prop — is a redesign rather than moving one expression, so
///   it is not something a rule at `error` should insist on.
/// * **An effect with no dependency array, or an empty one.** Neither derives
///   anything from anything.
/// * **`useLayoutEffect`.** That is where measuring belongs.
/// * **A setter that is not a `useState` binding.** `onChange(value)` is a
///   prop being called, which is what an effect is *for*.
/// * **A value the state itself feeds.** When a dependency is computed, through
///   however many bindings, from the state this effect writes, the render-time
///   rewrite is `const x = f(x)` — a circular definition, not a simpler
///   program. `@uniflowed/hooks`' `useTimeAgo` is one: the schedule it stores
///   chooses the clock whose tick decides the next schedule, and the effect is
///   what breaks that cycle across renders. See [`SetterScope::feeds_back`].
fn derived_state_effects(file: &Value) -> Vec<TreeFinding> {
    let scopes = setter_scopes(file);
    let mut found = Vec::new();
    let mut pending = vec![file];

    while let Some(node) = pending.pop() {
        match node {
            Value::Array(items) => pending.extend(items),
            Value::Object(map) => {
                if let Some(at) = derived_state_effect(node, &scopes) {
                    found.push(TreeFinding {
                        kind: FindingKind::DerivedState,
                        line: at.0,
                        column: at.1,
                        column_unit: ColumnUnit::CodePoint,
                        message: String::from(
                            "this effect only stores a value derived from its dependencies; compute it during render instead",
                        ),
                    });
                }
                pending.extend(map.values());
            }
            _ => {}
        }
    }

    found.sort_by_key(|finding| (finding.line, finding.column));
    found
}

/// Native Flow-AST implementation of `react/no-derived-state-effect`.
///
/// The derived-state rule does not need Babel's AST shape, and on a module
/// that does not ask for `react/no-redundant-memo` there is no reason to render
/// the parsed tree as `serde_json::Value` only to walk it again.
fn derived_state_effects_native(parsed: &uf_flow::Parsed) -> Vec<TreeFinding> {
    let mut tree = NativeDerivedState {
        scopes: Vec::new(),
        found: Vec::new(),
    };
    let _ = tree.program(&parsed.program);
    let mut found = tree.found;
    found.sort_by_key(|finding| (finding.line, finding.column));
    found
}

struct NativeDerivedState {
    scopes: Vec<NativeSetterScope>,
    found: Vec<TreeFinding>,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for NativeDerivedState {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn function_(
        &mut self,
        loc: &'ast Loc,
        function: &'ast ast::function::Function<Loc, Loc>,
    ) -> Result<(), ()> {
        if let Some(scope) = native_scope_for_function(loc, function) {
            self.scopes.push(scope);
        }
        ast_visitor::function_default(self, loc, function)
    }

    fn component_declaration(
        &mut self,
        loc: &'ast Loc,
        component: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
    ) -> Result<(), ()> {
        if let Some((_, body)) = component.body.as_ref()
            && let Some(scope) = native_scope(loc, body)
        {
            self.scopes.push(scope);
        }
        ast_visitor::component_declaration_default(self, loc, component)
    }

    fn call(
        &mut self,
        loc: &'ast Loc,
        call: &'ast ast::expression::Call<Loc, Loc>,
    ) -> Result<(), ()> {
        if let Some((line, column)) = derived_state_effect_native(loc, call, &self.scopes) {
            self.found.push(TreeFinding {
                kind: FindingKind::DerivedState,
                line,
                column,
                column_unit: ColumnUnit::Byte,
                message: String::from(
                    "this effect only stores a value derived from its dependencies; compute it during render instead",
                ),
            });
        }
        ast_visitor::call_default(self, loc, call)
    }
}

fn native_scope_for_function(
    loc: &Loc,
    function: &ast::function::Function<Loc, Loc>,
) -> Option<NativeSetterScope> {
    match &function.body {
        ast::function::Body::BodyBlock((_, body)) => native_scope(loc, body),
        ast::function::Body::BodyExpression(_) => None,
    }
}

fn native_scope(loc: &Loc, body: &ast::statement::Block<Loc, Loc>) -> Option<NativeSetterScope> {
    let state = native_declared_state(body);
    (!state.is_empty()).then(|| NativeSetterScope {
        span: NativeSpan::from_loc(loc),
        state,
        reads: native_declared_reads(body),
    })
}

#[derive(Clone, Copy)]
struct NativeSpan {
    start_line: i32,
    start_column: i32,
    end_line: i32,
    end_column: i32,
}

impl NativeSpan {
    fn from_loc(loc: &Loc) -> Self {
        Self {
            start_line: loc.start.line,
            start_column: loc.start.column,
            end_line: loc.end.line,
            end_column: loc.end.column,
        }
    }

    fn contains(self, loc: &Loc) -> bool {
        let start = (self.start_line, self.start_column);
        let end = (self.end_line, self.end_column);
        let at = (loc.start.line, loc.start.column);
        start <= at && at <= end
    }

    fn width_key(self) -> (i32, i32) {
        (
            self.end_line.saturating_sub(self.start_line),
            self.end_column.saturating_sub(self.start_column),
        )
    }
}

struct NativeSetterScope {
    span: NativeSpan,
    state: Vec<StateBinding>,
    reads: FxHashMap<String, Vec<String>>,
}

impl NativeSetterScope {
    fn binding(&self, setter: &str) -> Option<&StateBinding> {
        self.state.iter().find(|held| held.setter == setter)
    }

    fn feeds_back<'a>(&'a self, deps: &[&'a str], state: &str) -> bool {
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        let mut pending: Vec<&str> = deps.to_vec();

        while let Some(name) = pending.pop() {
            if name == state {
                return true;
            }
            if !seen.insert(name) {
                continue;
            }
            if let Some(reads) = self.reads.get(name) {
                pending.extend(reads.iter().map(String::as_str));
            }
        }

        false
    }
}

fn native_declared_state(body: &ast::statement::Block<Loc, Loc>) -> Vec<StateBinding> {
    let mut state = Vec::new();
    for statement in body.body.iter() {
        let ast::statement::StatementInner::VariableDeclaration { inner, .. } = &**statement else {
            continue;
        };
        for declarator in inner.declarations.iter() {
            if !calls_hook_native(declarator.init.as_ref(), STATE) {
                continue;
            }
            let ast::pattern::Pattern::Array { inner: pattern, .. } = &declarator.id else {
                continue;
            };
            if let Some(setter) = native_pattern_array_identifier(pattern, 1) {
                state.push(StateBinding {
                    value: native_pattern_array_identifier(pattern, 0).map(str::to_owned),
                    setter: setter.to_owned(),
                });
            }
        }
    }
    state
}

fn native_pattern_array_identifier(
    pattern: &ast::pattern::Array<Loc, Loc>,
    at: usize,
) -> Option<&str> {
    match pattern.elements.get(at)? {
        ast::pattern::array::Element::NormalElement(element) => {
            native_pattern_identifier(&element.argument)
        }
        ast::pattern::array::Element::RestElement(_) | ast::pattern::array::Element::Hole(_) => {
            None
        }
    }
}

fn native_pattern_identifier(pattern: &ast::pattern::Pattern<Loc, Loc>) -> Option<&str> {
    let ast::pattern::Pattern::Identifier { inner, .. } = pattern else {
        return None;
    };
    Some(inner.name.name.as_str())
}

fn native_declared_reads(body: &ast::statement::Block<Loc, Loc>) -> FxHashMap<String, Vec<String>> {
    let mut collector = NativeDeclaredReads {
        reads: FxHashMap::default(),
    };
    for statement in body.body.iter() {
        let _ = collector.statement(statement);
    }
    collector.reads
}

struct NativeDeclaredReads {
    reads: FxHashMap<String, Vec<String>>,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for NativeDeclaredReads {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn variable_declarator(
        &mut self,
        kind: ast::VariableKind,
        declarator: &'ast ast::statement::variable::Declarator<Loc, Loc>,
    ) -> Result<(), ()> {
        let bound = native_pattern_identifiers(&declarator.id);
        let read = declarator
            .init
            .as_ref()
            .map(native_identifiers_read_in)
            .unwrap_or_default();
        for name in bound {
            self.reads
                .entry(name)
                .or_default()
                .extend(read.iter().cloned());
        }
        ast_visitor::variable_declarator_default(self, kind, declarator)
    }
}

fn native_pattern_identifiers(pattern: &ast::pattern::Pattern<Loc, Loc>) -> Vec<String> {
    let mut collector = NativeIdentifierCollector { names: Vec::new() };
    let _ = collector.pattern(None, pattern);
    collector.names
}

fn native_identifiers_read_in(expression: &ast::expression::Expression<Loc, Loc>) -> Vec<String> {
    let mut collector = NativeIdentifierCollector { names: Vec::new() };
    let _ = collector.expression(expression);
    collector.names
}

struct NativeIdentifierCollector {
    names: Vec<String>,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for NativeIdentifierCollector {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    fn identifier(&mut self, id: &'ast ast::Identifier<Loc, Loc>) -> Result<(), ()> {
        self.names.push(id.name.as_str().to_owned());
        Ok(())
    }

    fn member(
        &mut self,
        _loc: &'ast Loc,
        member: &'ast ast::expression::Member<Loc, Loc>,
    ) -> Result<(), ()> {
        self.expression(&member.object)?;
        if let ast::expression::member::Property::PropertyExpression(property) = &member.property {
            self.expression(property)?;
        }
        Ok(())
    }

    fn object_property(
        &mut self,
        property: &'ast ast::expression::object::NormalProperty<Loc, Loc>,
    ) -> Result<(), ()> {
        match property {
            ast::expression::object::NormalProperty::Init { key, value, .. } => {
                self.object_key(key)?;
                self.expression(value)?;
            }
            ast::expression::object::NormalProperty::Method { key, value, .. }
            | ast::expression::object::NormalProperty::Get { key, value, .. }
            | ast::expression::object::NormalProperty::Set { key, value, .. } => {
                self.object_key(key)?;
                let (loc, function) = value;
                self.function_expression_or_method(loc, function)?;
            }
        }
        Ok(())
    }

    fn object_key(&mut self, key: &'ast ast::expression::object::Key<Loc, Loc>) -> Result<(), ()> {
        if let ast::expression::object::Key::Computed(computed) = key {
            self.expression(&computed.expression)?;
        }
        Ok(())
    }
}

fn derived_state_effect_native(
    loc: &Loc,
    call: &ast::expression::Call<Loc, Loc>,
    scopes: &[NativeSetterScope],
) -> Option<(u32, u32)> {
    if !calls_hook_call_native(call, EFFECT) {
        return None;
    }
    let [effect, dependencies] = call.arguments.arguments.as_ref() else {
        return None;
    };
    let (
        ast::expression::ExpressionOrSpread::Expression(effect),
        ast::expression::ExpressionOrSpread::Expression(dependencies),
    ) = (effect, dependencies)
    else {
        return None;
    };

    let ast::expression::ExpressionInner::ArrowFunction { inner: effect, .. } = &**effect else {
        return None;
    };
    if !effect.params.params.is_empty() || effect.params.rest.is_some() || effect.async_ {
        return None;
    }

    let deps = native_dependency_names(dependencies)?;
    let value_call = native_effect_body(&effect.body)?;
    let ast::expression::ExpressionInner::Call {
        inner: value_call, ..
    } = &**value_call
    else {
        return None;
    };
    let setter = native_identifier_name(&value_call.callee)?;
    let scope = scopes
        .iter()
        .filter(|scope| scope.span.contains(loc) && scope.binding(setter).is_some())
        .min_by_key(|scope| scope.span.width_key())?;
    let [value] = value_call.arguments.arguments.as_ref() else {
        return None;
    };
    let ast::expression::ExpressionOrSpread::Expression(value) = value else {
        return None;
    };
    if !native_derives_from(value, &deps) {
        return None;
    }
    if let Some(state) = scope.binding(setter).and_then(|held| held.value.as_deref())
        && scope.feeds_back(&deps, state)
    {
        return None;
    }

    let at = native_expression_loc(&call.callee);
    Some((
        u32::try_from(at.start.line).ok()?,
        u32::try_from(at.start.column).ok()?,
    ))
}

fn native_effect_body(
    body: &ast::function::Body<Loc, Loc>,
) -> Option<&ast::expression::Expression<Loc, Loc>> {
    match body {
        ast::function::Body::BodyExpression(expression) => Some(expression),
        ast::function::Body::BodyBlock((_, block)) => {
            let [statement] = block.body.as_ref() else {
                return None;
            };
            let ast::statement::StatementInner::Expression { inner, .. } = &**statement else {
                return None;
            };
            Some(&inner.expression)
        }
    }
}

fn native_dependency_names(
    expression: &ast::expression::Expression<Loc, Loc>,
) -> Option<Vec<&str>> {
    let ast::expression::ExpressionInner::Array { inner, .. } = &**expression else {
        return None;
    };
    if inner.elements.is_empty() {
        return None;
    }
    inner
        .elements
        .iter()
        .map(|element| match element {
            ast::expression::ArrayElement::Expression(expression) => {
                native_identifier_name(expression)
            }
            ast::expression::ArrayElement::Spread(_) | ast::expression::ArrayElement::Hole(_) => {
                None
            }
        })
        .collect()
}

fn native_derives_from(expression: &ast::expression::Expression<Loc, Loc>, deps: &[&str]) -> bool {
    let mut mentions = false;
    let mut pending = vec![expression];

    while let Some(expression) = pending.pop() {
        match &**expression {
            ast::expression::ExpressionInner::Identifier { inner, .. } => {
                if deps.contains(&inner.name.as_str()) {
                    mentions = true;
                } else {
                    return false;
                }
            }
            ast::expression::ExpressionInner::StringLiteral { .. }
            | ast::expression::ExpressionInner::BooleanLiteral { .. }
            | ast::expression::ExpressionInner::NullLiteral { .. }
            | ast::expression::ExpressionInner::NumberLiteral { .. }
            | ast::expression::ExpressionInner::BigIntLiteral { .. }
            | ast::expression::ExpressionInner::RegExpLiteral { .. } => {}
            ast::expression::ExpressionInner::TemplateLiteral { inner, .. } => {
                pending.extend(inner.expressions.iter());
            }
            ast::expression::ExpressionInner::Binary { inner, .. } => {
                pending.extend([&inner.left, &inner.right]);
            }
            ast::expression::ExpressionInner::Logical { inner, .. } => {
                pending.extend([&inner.left, &inner.right]);
            }
            ast::expression::ExpressionInner::Conditional { inner, .. } => {
                pending.extend([&inner.test, &inner.consequent, &inner.alternate]);
            }
            ast::expression::ExpressionInner::Unary { inner, .. }
                if native_pure_unary(inner.operator) =>
            {
                pending.push(&inner.argument);
            }
            _ => return false,
        }
    }

    mentions
}

fn native_pure_unary(operator: ast::expression::UnaryOperator) -> bool {
    matches!(
        operator,
        ast::expression::UnaryOperator::Minus
            | ast::expression::UnaryOperator::Plus
            | ast::expression::UnaryOperator::Not
            | ast::expression::UnaryOperator::BitNot
            | ast::expression::UnaryOperator::Typeof
            | ast::expression::UnaryOperator::Void
    )
}

fn calls_hook_native(
    expression: Option<&ast::expression::Expression<Loc, Loc>>,
    hook: &str,
) -> bool {
    let Some(ast::expression::ExpressionInner::Call { inner, .. }) =
        expression.map(|expression| &**expression)
    else {
        return false;
    };
    calls_hook_call_native(inner, hook)
}

fn calls_hook_call_native(call: &ast::expression::Call<Loc, Loc>, hook: &str) -> bool {
    match &*call.callee {
        ast::expression::ExpressionInner::Identifier { inner, .. } => inner.name == hook,
        ast::expression::ExpressionInner::Member { inner, .. } => {
            native_identifier_name(&inner.object) == Some("React")
                && matches!(
                    &inner.property,
                    ast::expression::member::Property::PropertyIdentifier(identifier)
                        if identifier.name == hook
                )
        }
        _ => false,
    }
}

fn native_identifier_name(expression: &ast::expression::Expression<Loc, Loc>) -> Option<&str> {
    match &**expression {
        ast::expression::ExpressionInner::Identifier { inner, .. } => Some(inner.name.as_str()),
        _ => None,
    }
}

fn native_expression_loc(expression: &ast::expression::Expression<Loc, Loc>) -> &Loc {
    expression.0.loc()
}

/// One `useState` binding: `const [value, setValue] = useState(…)`.
///
/// `value` is [`None`] when the first element is not a plain identifier —
/// `const [, setValue]` is written on purpose when nothing reads the state, and
/// the feedback test below has nothing to look for then.
struct StateBinding {
    value: Option<String>,
    setter: String,
}

/// One function body: the `useState` bindings declared directly in it, and what
/// every binding declared anywhere in it reads.
struct SetterScope {
    start: u64,
    end: u64,
    state: Vec<StateBinding>,
    /// Name of a binding declared in this body, to the names its initialiser
    /// reads. The edges of the graph [`SetterScope::feeds_back`] searches.
    reads: FxHashMap<String, Vec<String>>,
}

impl SetterScope {
    /// The binding `setter` writes, when this body declares one.
    fn binding(&self, setter: &str) -> Option<&StateBinding> {
        self.state.iter().find(|held| held.setter == setter)
    }

    /// Whether `state` is what any of `deps` is ultimately computed from.
    ///
    /// A walk over [`SetterScope::reads`] rather than a name comparison,
    /// because the dependency is rarely the state itself: `@uniflowed/hooks`'
    /// `useTimeAgo` stores a schedule, the schedule chooses a tick, the tick
    /// drives a clock, and the clock's reading is what the effect stores back.
    /// Only the last of those four is the effect's dependency, and the cycle is
    /// the reason the effect has to exist — moving the expression into render
    /// would define the value in terms of itself.
    ///
    /// Over-connected on purpose: an identifier that merely *appears* in an
    /// initialiser counts as read, so a spurious edge costs a report the rule
    /// would otherwise make and never a report it should not. That is the right
    /// direction for a rule at `error`.
    fn feeds_back<'a>(&'a self, deps: &[&'a str], state: &str) -> bool {
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        let mut pending: Vec<&str> = deps.to_vec();

        while let Some(name) = pending.pop() {
            if name == state {
                return true;
            }
            if !seen.insert(name) {
                continue;
            }
            if let Some(reads) = self.reads.get(name) {
                pending.extend(reads.iter().map(String::as_str));
            }
        }

        false
    }
}

/// Node types whose `body` is a function body.
///
/// Three, not Babel's five: ESTree has no `ClassMethod` or `ObjectMethod`. A
/// class method is a `MethodDefinition` and an object method a `Property`, and
/// in both the function itself is the `FunctionExpression` underneath — which
/// is the node that carries the body, and is already here.
const FUNCTIONS: [&str; 3] = [
    "ArrowFunctionExpression",
    "FunctionDeclaration",
    "FunctionExpression",
];

/// Every function in `file` that declares a `useState` setter, with the span
/// the setter is in scope over.
///
/// The `useState` bindings are read from the statements *directly* in the body,
/// which is not a shortcut: the rules of hooks put every `useState` there, and
/// a `setX` bound anywhere else is not a state setter this rule may assume
/// anything about. The reads graph is collected from the whole body, because a
/// binding declared inside a block still closes the cycle the graph is there to
/// find.
fn setter_scopes(file: &Value) -> Vec<SetterScope> {
    let mut scopes = Vec::new();
    let mut pending = vec![file];

    while let Some(node) = pending.pop() {
        match node {
            Value::Array(items) => pending.extend(items),
            Value::Object(map) => {
                if let Some(node_type) = map.get("type").and_then(Value::as_str)
                    && FUNCTIONS.contains(&node_type)
                    && let (Some(start), Some(end)) = (span_start(node), span_end(node))
                {
                    let body = map.get("body");
                    let state = declared_state(body);
                    if !state.is_empty() {
                        scopes.push(SetterScope {
                            start,
                            end,
                            state,
                            reads: declared_reads(body),
                        });
                    }
                }
                pending.extend(map.values());
            }
            _ => {}
        }
    }

    scopes
}

/// The `useState` bindings a block statement's own declarations make.
fn declared_state(body: Option<&Value>) -> Vec<StateBinding> {
    let mut state = Vec::new();
    let Some(statements) = body
        .filter(|body| body.get("type").and_then(Value::as_str) == Some("BlockStatement"))
        .and_then(|body| body.get("body"))
        .and_then(Value::as_array)
    else {
        return state;
    };

    for statement in statements {
        if statement.get("type").and_then(Value::as_str) != Some("VariableDeclaration") {
            continue;
        }
        let Some(declarators) = statement.get("declarations").and_then(Value::as_array) else {
            continue;
        };
        for declarator in declarators {
            if !calls_hook(declarator.get("init"), "useState") {
                continue;
            }
            let id = declarator.get("id");
            if id.and_then(|id| id.get("type")).and_then(Value::as_str) != Some("ArrayPattern") {
                continue;
            }
            // `const [value, setValue] = useState(…)`: the setter is the
            // second element, and only the second.
            let element = |at: usize| {
                id.and_then(|id| id.get("elements"))
                    .and_then(Value::as_array)
                    .and_then(|elements| elements.get(at))
                    .and_then(identifier_name)
            };
            if let Some(setter) = element(1) {
                state.push(StateBinding {
                    value: element(0).map(str::to_owned),
                    setter: setter.to_owned(),
                });
            }
        }
    }

    state
}

/// Every binding declared anywhere in `body`, and the names its initialiser
/// reads.
///
/// A name declared twice keeps the union of both initialisers' reads, which is
/// the conservative answer: shadowing is not tracked, so a scope that redeclares
/// a name is read as if either initialiser could be the one that matters.
fn declared_reads(body: Option<&Value>) -> FxHashMap<String, Vec<String>> {
    let mut reads: FxHashMap<String, Vec<String>> = FxHashMap::default();
    let mut pending = match body {
        Some(body) => vec![body],
        None => return reads,
    };

    while let Some(node) = pending.pop() {
        match node {
            Value::Array(items) => pending.extend(items),
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("VariableDeclarator")
                    && let Some(id) = map.get("id")
                {
                    let mut bound = Vec::new();
                    identifiers_in(id, &mut bound);
                    let mut initialiser = Vec::new();
                    if let Some(init) = map.get("init") {
                        identifiers_in(init, &mut initialiser);
                    }
                    for name in bound {
                        reads
                            .entry(name.to_owned())
                            .or_default()
                            .extend(initialiser.iter().map(|read| (*read).to_owned()));
                    }
                }
                pending.extend(map.values());
            }
            _ => {}
        }
    }

    reads
}

/// Every identifier `node` reads, appended to `out`.
///
/// Property names are not reads: `now.getTime()` reads `now`, and counting
/// `getTime` would join two bindings that only share a method name. Object
/// literal keys are skipped for the same reason.
///
/// Iterative, like the other walks here, because the depth of the tree is the
/// module author's to choose.
fn identifiers_in<'a>(node: &'a Value, out: &mut Vec<&'a str>) {
    let mut pending = vec![node];

    while let Some(node) = pending.pop() {
        match node {
            Value::Array(items) => pending.extend(items),
            Value::Object(map) => {
                if let Some(name) = identifier_name(node) {
                    out.push(name);
                    continue;
                }
                let key_is_a_name = map.get("computed") != Some(&Value::Bool(true));
                let skipped = match map.get("type").and_then(Value::as_str) {
                    Some("MemberExpression" | "OptionalMemberExpression") if key_is_a_name => {
                        "property"
                    }
                    // `Property` and `MethodDefinition` are ESTree's spelling
                    // of what Babel calls `ObjectProperty`, `ObjectMethod` and
                    // `ClassMethod`.
                    Some("Property" | "MethodDefinition") if key_is_a_name => "key",
                    _ => "",
                };
                pending.extend(
                    map.iter()
                        .filter(|(field, _)| field.as_str() != skipped)
                        .map(|(_, child)| child),
                );
            }
            _ => {}
        }
    }
}

/// Whether `node` is a call to `hook`, written bare or under `React.`.
fn calls_hook(node: Option<&Value>, hook: &str) -> bool {
    let Some(node) =
        node.filter(|node| node.get("type").and_then(Value::as_str) == Some("CallExpression"))
    else {
        return false;
    };
    let Some(callee) = node.get("callee") else {
        return false;
    };
    match callee.get("type").and_then(Value::as_str) {
        Some("Identifier") => identifier_name(callee) == Some(hook),
        Some("MemberExpression") if callee.get("computed") != Some(&Value::Bool(true)) => {
            callee.get("object").and_then(identifier_name) == Some("React")
                && callee.get("property").and_then(identifier_name) == Some(hook)
        }
        _ => false,
    }
}

/// The position of a derived-state effect's `useEffect`, when `node` is one.
fn derived_state_effect(node: &Value, scopes: &[SetterScope]) -> Option<(u32, u32)> {
    if node.get("type").and_then(Value::as_str)? != "CallExpression" {
        return None;
    }
    if !calls_hook(Some(node), EFFECT) {
        return None;
    }
    let arguments = node.get("arguments").and_then(Value::as_array)?;
    let [effect, dependencies] = arguments.as_slice() else {
        return None;
    };

    // `() => …`, and nothing that takes a parameter or awaits.
    if effect.get("type").and_then(Value::as_str)? != "ArrowFunctionExpression"
        || !effect
            .get("params")
            .and_then(Value::as_array)
            .is_some_and(|params| params.is_empty())
        || effect.get("async") == Some(&Value::Bool(true))
    {
        return None;
    }

    let deps = dependency_names(dependencies)?;
    let call = effect_body(effect.get("body")?)?;
    if call.get("type").and_then(Value::as_str)? != "CallExpression" {
        return None;
    }

    // The setter has to be a `useState` binding of a function this call
    // stands inside. A name that is merely spelled `setX` is not one: it may
    // be a prop, a store action, or a helper, and calling one of those from an
    // effect is what an effect is for.
    //
    // The innermost enclosing declaration wins, because that is the one the
    // name resolves to: two components in a module may each bind `setValue`,
    // and only one of them holds this effect.
    let setter = call.get("callee").and_then(identifier_name)?;
    let start = span_start(node)?;
    let scope = scopes
        .iter()
        .filter(|scope| {
            scope.start <= start && start <= scope.end && scope.binding(setter).is_some()
        })
        .min_by_key(|scope| scope.end - scope.start)?;

    let [value] = call.get("arguments").and_then(Value::as_array)?.as_slice() else {
        return None;
    };
    if !derives_from(value, &deps) {
        return None;
    }

    // A dependency the state itself feeds: the render-time rewrite would be
    // circular, so the effect is the design rather than a mistake.
    if let Some(state) = scope.binding(setter).and_then(|held| held.value.as_deref())
        && scope.feeds_back(&deps, state)
    {
        return None;
    }

    let at = node.get("callee")?.get("loc")?.get("start")?;
    Some((
        u32::try_from(at.get("line")?.as_u64()?).ok()?,
        u32::try_from(at.get("column")?.as_u64()?).ok()?,
    ))
}

/// The single expression an effect's body evaluates, if that is all it does.
///
/// A block of one expression statement, or an arrow with a concise body.
/// Anything longer is an effect that also does something else, and anything
/// that returns is an effect with a cleanup.
fn effect_body(body: &Value) -> Option<&Value> {
    if body.get("type").and_then(Value::as_str)? != "BlockStatement" {
        return Some(body);
    }
    let [statement] = body.get("body").and_then(Value::as_array)?.as_slice() else {
        return None;
    };
    if statement.get("type").and_then(Value::as_str)? != "ExpressionStatement" {
        return None;
    }
    statement.get("expression")
}

/// The names in a dependency array, when every element is a plain identifier.
///
/// [`None`] for an empty array, a missing one, or one holding anything else —
/// `[data.id]` says the effect re-runs on the *field*, which is a different
/// claim from "derived from `data`", and the rule does not make claims it
/// cannot check.
fn dependency_names(node: &Value) -> Option<Vec<&str>> {
    if node.get("type").and_then(Value::as_str)? != "ArrayExpression" {
        return None;
    }
    let elements = node.get("elements").and_then(Value::as_array)?;
    if elements.is_empty() {
        return None;
    }
    elements.iter().map(identifier_name).collect()
}

/// Expression node types that are pure however they are combined.
///
/// One name, because ESTree has one: a string, a number, a boolean, `null`, a
/// regular expression and a bigint are all `Literal`, and it is `to_babel`
/// that splits them into the six Babel spells them with.
const PURE_LITERALS: [&str; 1] = ["Literal"];

/// Unary operators that read their operand and nothing else.
///
/// `delete` is not here: it writes.
const PURE_UNARY: [&str; 6] = ["!", "+", "-", "typeof", "void", "~"];

/// Whether `value` is an expression over `deps` that render could compute.
///
/// True only when every leaf is either a literal or one of `deps`, every node
/// between them is an operator, and at least one dependency is actually read —
/// a constant is not derived from anything.
fn derives_from(value: &Value, deps: &[&str]) -> bool {
    let mut mentions = false;
    let mut pending = vec![value];

    while let Some(node) = pending.pop() {
        let Some(node_type) = node.get("type").and_then(Value::as_str) else {
            return false;
        };
        if PURE_LITERALS.contains(&node_type) {
            continue;
        }
        match node_type {
            "Identifier" => match identifier_name(node) {
                Some(name) if deps.contains(&name) => mentions = true,
                _ => return false,
            },
            "TemplateLiteral" => match node.get("expressions").and_then(Value::as_array) {
                Some(expressions) => pending.extend(expressions),
                None => return false,
            },
            "BinaryExpression" | "LogicalExpression" => {
                match (node.get("left"), node.get("right")) {
                    (Some(left), Some(right)) => pending.extend([left, right]),
                    _ => return false,
                }
            }
            "ConditionalExpression" => {
                match (
                    node.get("test"),
                    node.get("consequent"),
                    node.get("alternate"),
                ) {
                    (Some(test), Some(consequent), Some(alternate)) => {
                        pending.extend([test, consequent, alternate]);
                    }
                    _ => return false,
                }
            }
            "UnaryExpression" => {
                let operator = node.get("operator").and_then(Value::as_str);
                match (
                    operator.is_some_and(|op| PURE_UNARY.contains(&op)),
                    node.get("argument"),
                ) {
                    (true, Some(argument)) => pending.push(argument),
                    _ => return false,
                }
            }
            "ParenthesizedExpression" => match node.get("expression") {
                Some(expression) => pending.push(expression),
                None => return false,
            },
            _ => return false,
        }
    }

    mentions
}

/// The name of an `Identifier` node.
fn identifier_name(node: &Value) -> Option<&str> {
    if node.get("type").and_then(Value::as_str)? != "Identifier" {
        return None;
    }
    node.get("name")?.as_str()
}

/// The span's start, from the ESTree `range` pair.
///
/// `start` and `end` as their own fields are Babel's; `babel::finalize` splits
/// `range` into them. This tree has not been through it.
fn span_start(node: &Value) -> Option<u64> {
    node.get("range")?.as_array()?.first()?.as_u64()
}

/// The span's end, from the same pair.
fn span_end(node: &Value) -> Option<u64> {
    node.get("range")?.as_array()?.get(1)?.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceFile;
    use crate::scan::mask_inline_comments;
    use uf_config::{RuleLevel, UniflowedConfig};
    use uf_infra::CompactString;

    fn only(rule: &str) -> UniflowedConfig {
        let mut config = UniflowedConfig::default();
        config.lint.rules.clear();
        config
            .lint
            .rules
            .insert(CompactString::from(rule), RuleLevel::Error);
        config
    }

    fn wants(rule: &str, source: &str) -> bool {
        let file = SourceFile {
            path: "app/page.js".to_owned(),
            source: source.to_owned(),
        };
        let masked = mask_inline_comments(&file.source);
        let scan = FileScan::new(&file, &masked);
        wanted(&scan, &only(rule)).is_some()
    }

    #[test]
    fn comments_and_strings_do_not_request_the_react_tree_path() {
        let source = r#"// @flow
// useEffect useState useMemo useCallback
const prose = "useEffect useState useMemo useCallback";
component Page() {
  return <main />;
}
"#;

        assert!(!wants(DERIVED_STATE, source));
        assert!(!wants(REDUNDANT_MEMO, source));
    }

    #[test]
    fn import_only_hook_names_do_not_request_the_react_tree_path() {
        let source = r#"// @flow
import { useEffect, useMemo, useState } from "react";
component Page() {
  return <main />;
}
"#;

        assert!(!wants(DERIVED_STATE, source));
        assert!(!wants(REDUNDANT_MEMO, source));
    }

    #[test]
    fn hook_calls_still_request_the_react_tree_path() {
        let derived = r#"// @flow
import { useEffect, useState } from "react";
component Page() {
  const [value, setValue] = useState(0);
  useEffect(() => setValue(value + 1), [value]);
  return <main />;
}
"#;
        let memo = r#"// @flow
import { useMemo } from "react";
component Page() {
  const value = useMemo(() => 1, []);
  return <main />;
}
"#;
        let react_member = r#"// @flow
import * as React from "react";
component Page() {
  const value = React.useMemo(() => 1, []);
  return <main />;
}
"#;

        assert!(wants(DERIVED_STATE, derived));
        assert!(wants(REDUNDANT_MEMO, memo));
        assert!(wants(REDUNDANT_MEMO, react_member));
    }

    #[test]
    fn generic_hook_calls_still_request_the_react_tree_path() {
        let source = r#"// @flow
import { useMemo } from "react";
component Page() {
  const value = useMemo<number>(() => 1, []);
  return <main>{value}</main>;
}
"#;

        assert!(wants(REDUNDANT_MEMO, source));
    }
}
