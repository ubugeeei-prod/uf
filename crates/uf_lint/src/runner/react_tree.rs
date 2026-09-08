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
//! 3. The text holds no `useEffect`, `useMemo` or `useCallback` — nothing runs.
//!    A module without one of those words cannot violate either rule.
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
use uf_infra::{FxHashMap, FxHashSet};
use uf_transform::{ReactCompilerMode, TransformOptions};

use crate::scan::FileScan;
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

/// Report the rules that need the module's tree.
pub(crate) fn run_react_tree_rules(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
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
        return;
    }
    if !super::flow_syntax::is_flow_syntax_target(&scan.file.path) {
        return;
    }

    let source = &scan.file.source;
    // Both words, not just the effect: see `STATE`. Textual on purpose — the
    // point is to decide without parsing, and a module that mentions
    // `useState` in a comment costs one parse it would have paid anyway.
    let wants_effects = derived.is_some() && source.contains(EFFECT) && source.contains(STATE);
    let wants_memo =
        memo.is_some() && (source.contains("useMemo") || source.contains("useCallback"));
    if !wants_effects && !wants_memo {
        return;
    }

    // The same ceilings `uf_flow::upstream` applies before it parses. A module
    // over one of them has already been reported by `flow/syntax`; reaching
    // for the parser again would only be a slower way to overflow.
    let depths = uf_flow::depths(source);
    if source.len() > uf_flow::MAX_PARSE_BYTES
        || depths.brackets > uf_flow::MAX_NESTING_DEPTH
        || depths.chain > uf_flow::MAX_CHAIN_DEPTH
    {
        return;
    }

    let Some(found) = analyse(scan, config, wants_effects, wants_memo) else {
        return;
    };

    for finding in found {
        let rule = match finding.kind {
            FindingKind::DerivedState => DERIVED_STATE,
            FindingKind::RedundantMemo => REDUNDANT_MEMO,
        };
        let level = match finding.kind {
            FindingKind::DerivedState => derived,
            FindingKind::RedundantMemo => memo,
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
        // The AST counts columns in UTF-16 code units and a diagnostic carries
        // bytes, so the conversion reads the line out of the *unmasked* source:
        // `mask_inline_comments` replaces a comment byte for byte, which keeps
        // every offset but not every character.
        let text = source
            .get(line.offset..line.offset + line.text.len())
            .unwrap_or(line.text);
        push_at(
            diagnostics,
            scan,
            rule,
            level,
            index,
            byte_column(text, finding.column),
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
struct TreeFinding {
    kind: FindingKind,
    /// 1-based line.
    line: u32,
    /// 0-based column, in UTF-16 code units.
    column: u32,
    message: String,
}

/// Parse the module and run whichever of the two rules was asked for.
///
/// [`None`] when the module could not be parsed or lowered — `flow/syntax`
/// reports the first and the build reports the second, and a rule that cannot
/// see the tree has nothing to say about it.
fn analyse(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    wants_effects: bool,
    wants_memo: bool,
) -> Option<Vec<TreeFinding>> {
    let source = &scan.file.source;
    let options = TransformOptions {
        react_compiler: compiler_mode(config),
        ..TransformOptions::new(scan.file.path.clone())
    };

    let work = || {
        let (file, _) = uf_transform::babel_ast(source).ok()?;
        let mut found = Vec::new();
        if wants_effects {
            found.extend(derived_state_effects(&file));
        }
        // An error here is a bug in uf rather than in the module — the tree
        // did not fit the compiler's own schema — and it says nothing about
        // the effects the other rule already found, so it costs that rule
        // nothing.
        if wants_memo
            && let Ok(redundant) = uf_transform::redundant_memoization(&file, source, &options)
        {
            found.extend(redundant.into_iter().map(|memo| TreeFinding {
                kind: FindingKind::RedundantMemo,
                line: memo.line,
                column: memo.column,
                message: format!(
                    "the React Compiler memoizes this already; `{}` here is a second dependency array to keep correct",
                    memo.hook
                ),
            }));
        }
        Some(found)
    };

    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, work)
            .ok()?
            .join()
            .ok()?
    })
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
const FUNCTIONS: [&str; 5] = [
    "ArrowFunctionExpression",
    "ClassMethod",
    "FunctionDeclaration",
    "FunctionExpression",
    "ObjectMethod",
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
                    Some("ObjectProperty" | "ObjectMethod" | "ClassMethod") if key_is_a_name => {
                        "key"
                    }
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
const PURE_LITERALS: [&str; 6] = [
    "BigIntLiteral",
    "BooleanLiteral",
    "NullLiteral",
    "NumericLiteral",
    "RegExpLiteral",
    "StringLiteral",
];

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

fn span_start(node: &Value) -> Option<u64> {
    node.get("start")?.as_u64()
}

fn span_end(node: &Value) -> Option<u64> {
    node.get("end")?.as_u64()
}
