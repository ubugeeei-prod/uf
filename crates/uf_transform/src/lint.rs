//! The official React Compiler, run as a linter.
//!
//! `uf lint`'s `react-compiler/*` rules report what `react_compiler` — the
//! compiler `uf build` runs, published from `facebook/react` — says about a
//! module. Nothing here decides whether code breaks a rule of React: this
//! module runs the compiler the way the compiler's own lint integration runs
//! it, and hands back what the compiler reported.
//!
//! That integration is `eslint-plugin-react-hooks`, published from the same
//! repository, and its `RunReactCompiler` module makes four choices. uf makes
//! each of them the same way, checked against the plugin's 7.1.1 build:
//!
//! 1. **The options** — [`lint_plugin_options`](crate::compiler): `outputMode:
//!    "lint"`, `panicThreshold: "none"`, `flowSuppressions: false`, the
//!    plugin's `validate*` flags, and the compiler's default `compilationMode`,
//!    `infer`. So a plain `function useThing()` or `function Card()` that calls
//!    a hook or returns JSX is checked, and so is every `component` and `hook`
//!    declaration. The one validation the plugin ships switched off and lets a
//!    project switch on is a [`LintSwitches`] field.
//! 2. **Which modules** — [`may_contain_react_code`], the plugin's own test of
//!    a module's top-level statements. A module it rejects is not compiled.
//! 3. **Which events, and where** — every `CompileError` event is a finding,
//!    placed at the diagnostic's primary location: the first `error` detail's
//!    location, or the location of a diagnostic that has only one. An event
//!    with no location is dropped, as the plugin drops it.
//! 4. **Flow suppressions** — a finding on the line below a comment carrying
//!    `$FlowFixMe[react-rule-hook]` or `$FlowFixMe[react-rule-unsafe-ref]` is
//!    dropped, because Flow reported the same thing already.
//!
//! What uf adds is presentation, and all of it is in `uf_lint`: the rule a
//! category is filed under, the code frame, and `uf-lint-disable` comments.

use std::sync::{LazyLock, Mutex, PoisonError};

use flow_parser::ast::Program;
use flow_parser::ast::expression::{Expression, ExpressionInner};
use flow_parser::ast::function::Effect;
use flow_parser::ast::pattern::Pattern;
use flow_parser::ast::statement::export_default_declaration::Declaration;
use flow_parser::ast::statement::{Statement, StatementInner};
use flow_parser::loc::Loc;
use react_compiler::entrypoint::LoggerEvent;
use react_compiler_ast::scope::ScopeInfo;
pub use react_compiler_diagnostics::ErrorCategory;
use serde::Deserialize;
use serde_json::Value;
use uf_profiler::profile_span;

use crate::TransformError;
use crate::compiler::{Compiled, compile_with_options, lint_plugin_options, reported_at};

/// The Flow suppression codes `eslint-plugin-react-hooks` treats as Flow having
/// reported a finding already.
const FLOW_SUPPRESSION_CODES: [&str; 2] = ["react-rule-hook", "react-rule-unsafe-ref"];

/// The validations `eslint-plugin-react-hooks` ships switched off and lets a
/// project switch on through its rule options.
///
/// uf's rules stand in for those options: `uf_lint` sets a switch when the
/// rule that reports what the validation finds is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LintSwitches {
    /// Check effect dependency arrays for missing and extra values
    /// (`validateExhaustiveEffectDependencies: "all"`), which the compiler
    /// reports as [`ErrorCategory::EffectExhaustiveDependencies`].
    pub effect_dependencies: bool,
}

/// How many modules [`cached`] remembers.
///
/// A module's findings are kept against its path and its exact text, so they
/// are reused only when the compiler would be asked the very same question
/// again: when `uf lint --fix` reports on the files it has just fixed, which it
/// linted a moment earlier with that text, or when an editor asks again about
/// a document it has already asked about. This is a bound, not an eviction
/// policy — past it the table is emptied, which costs one compile per module
/// the next time each is asked about.
const CACHED_MODULES: usize = 512;

/// What [`lint`] reported for one module, and the question it answered.
struct Remembered {
    source: Box<str>,
    switches: LintSwitches,
    found: Box<[LintDiagnostic]>,
}

/// Every module [`lint`] has answered in this process, by path.
static REMEMBERED: LazyLock<Mutex<uf_infra::FxHashMap<Box<str>, Remembered>>> =
    LazyLock::new(Mutex::default);

/// What [`lint`] reported for the module at `filename` the last time it was
/// asked about it with exactly this text and these switches, if it was.
///
/// The options [`lint`] runs the compiler with depend on the path and the
/// switches and nothing else, and the tree it is handed is a function of the
/// text, so the three are the whole of the question. A caller that gets an
/// answer here skips building that tree at all, which is most of the cost.
/// The answer is a copy: a module's findings are a handful of short strings,
/// and copying them costs nothing next to the compile they stand in for.
#[must_use]
pub fn cached(filename: &str, source: &str, switches: LintSwitches) -> Option<Vec<LintDiagnostic>> {
    let table = REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner);
    let entry = table.get(filename)?;
    (*entry.source == *source && entry.switches == switches).then(|| entry.found.to_vec())
}

/// Keep what [`lint`] found for [`cached`] to answer with.
fn remember(filename: &str, source: &str, switches: LintSwitches, found: &[LintDiagnostic]) {
    let mut table = REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner);
    if table.len() >= CACHED_MODULES && !table.contains_key(filename) {
        table.clear();
    }
    table.insert(
        filename.into(),
        Remembered {
            source: source.into(),
            switches,
            found: found.into(),
        },
    );
}

/// One diagnostic the official React Compiler reported about a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintDiagnostic {
    /// The compiler's category, which decides the rule the finding is filed
    /// under.
    pub category: ErrorCategory,
    /// The compiler's own words: its reason, then its description, then any
    /// hints, on one line.
    pub message: String,
    /// 1-based line of the diagnostic's primary location.
    pub line: u32,
    /// 0-based column of the primary location, in UTF-16 code units — the unit
    /// Babel's `loc` counts.
    pub column: u32,
}

/// Whether `eslint-plugin-react-hooks` would hand this module to the compiler.
///
/// A port of the plugin's `mayContainReactCode`, reading the Flow parser's
/// tree where the plugin reads ESLint's. It is true when a top-level statement
/// is any of:
///
/// * a `component` or `hook` declaration, exported or not;
/// * `export default` of a function or arrow expression, or of a function
///   declaration with no name;
/// * a function declaration named like a component (an ASCII capital first) or
///   a hook (`use`, then a capital or a digit);
/// * a variable declaration binding such a name to a function or an arrow.
///
/// A module that has none of those is not compiled, so a component made only
/// by `memo(…)` or declared inside another statement is not checked. That is
/// the plugin's answer as well; uf does not widen or narrow it.
#[must_use]
pub fn may_contain_react_code(program: &Program<Loc, Loc>) -> bool {
    program.statements.iter().any(top_level_may_be_react)
}

/// The plugin's `checkTopLevelNode`.
fn top_level_may_be_react(node: &Statement<Loc, Loc>) -> bool {
    match &**node {
        StatementInner::ComponentDeclaration { .. } => true,
        StatementInner::ExportNamedDeclaration { inner, .. } => inner
            .declaration
            .as_ref()
            .is_some_and(top_level_may_be_react),
        StatementInner::ExportDefaultDeclaration { inner, .. } => match &inner.declaration {
            Declaration::Expression(expression) => is_function_expression(expression),
            Declaration::Declaration(statement) => match &**statement {
                StatementInner::FunctionDeclaration { inner, .. } if inner.id.is_none() => true,
                _ => top_level_may_be_react(statement),
            },
        },
        // A `hook` declaration is a function declaration with the hook effect in
        // this tree, and a `HookDeclaration` node in ESLint's.
        StatementInner::FunctionDeclaration { inner, .. } => {
            inner.effect_ == Effect::Hook
                || inner
                    .id
                    .as_ref()
                    .is_some_and(|id| is_component_or_hook_name(&id.name))
        }
        StatementInner::VariableDeclaration { inner, .. } => {
            inner.declarations.iter().any(|declarator| {
                let Pattern::Identifier { inner: binding, .. } = &declarator.id else {
                    return false;
                };
                declarator.init.as_ref().is_some_and(is_function_expression)
                    && is_component_or_hook_name(&binding.name.name)
            })
        }
        _ => false,
    }
}

fn is_function_expression(expression: &Expression<Loc, Loc>) -> bool {
    matches!(
        &**expression,
        ExpressionInner::ArrowFunction { .. } | ExpressionInner::Function { .. }
    )
}

/// The plugin's `COMPONENT_NAME_PATTERN` (`/^[A-Z]/`) or `HOOK_NAME_PATTERN`
/// (`/^use[A-Z0-9]/`).
fn is_component_or_hook_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_uppercase)
        || (bytes.starts_with(b"use")
            && bytes
                .get(3)
                .is_some_and(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()))
}

/// Everything the official React Compiler reports about one module, where
/// `eslint-plugin-react-hooks` would report it.
///
/// `file` is the module's Babel tree ([`crate::babel_ast`]), `scope` is
/// [`crate::scope::analyze`] of that tree, `filename` is the module's path, and
/// `switches` are the validations the project turned on beyond the plugin's
/// defaults. Call [`may_contain_react_code`] first: a module it rejects is one
/// the plugin never compiles.
///
/// # Errors
///
/// [`TransformError::Internal`] when the tree does not fit the compiler's AST
/// or the options do not fit the compiler's schema — a bug in uf rather than in
/// the module. What the compiler says *about* the module is never an error
/// here; it is the list this returns.
///
/// # Call this from a thread with `uf_flow::PARSE_STACK_BYTES` of stack
///
/// The compiler recurses through the tree, and so does dropping it.
pub fn lint(
    file: &Value,
    scope: ScopeInfo,
    source: &str,
    filename: &str,
    switches: LintSwitches,
) -> Result<Vec<LintDiagnostic>, TransformError> {
    profile_span!("lint::lint");
    let options = lint_plugin_options(source, filename, switches)?;
    // A fatal result still carries the events logged before it, and the
    // plugin reports those too: its logger collects them as they happen.
    let events = match compile_with_options(file, scope, options, None)? {
        Compiled::Ran { events, .. } | Compiled::Fatal { events, .. } => events,
    };

    let suppressed = flow_suppression_lines(file);
    let mut found = Vec::new();
    for event in &events {
        if !matches!(
            event,
            LoggerEvent::CompileError { .. } | LoggerEvent::CompileErrorWithLoc { .. }
        ) {
            continue;
        }
        // Serialized so the location is read by the code `uf build` reads it
        // with, rather than by a second copy of the same rule.
        let event = serde_json::to_value(event).map_err(|error| {
            TransformError::Internal(format!("compiler event could not be serialized: {error}"))
        })?;
        let Some(detail) = event.get("detail") else {
            continue;
        };
        let Some(category) = detail
            .get("category")
            .and_then(|category| ErrorCategory::deserialize(category).ok())
        else {
            continue;
        };
        let Some((line, column)) = reported_at(&event) else {
            continue;
        };
        if suppressed
            .iter()
            .any(|comment| comment.saturating_add(1) == line)
        {
            continue;
        }
        found.push(LintDiagnostic {
            category,
            message: message(detail),
            line,
            column,
        });
    }
    remember(filename, source, switches, &found);
    Ok(found)
}

/// The compiler's words for one diagnostic, on one line.
///
/// The text `eslint-plugin-react-hooks` prints — the reason, the description
/// and each hint — without the category heading, which is a severity `uf lint`
/// shows in its own place, and without the code frame, which `uf lint` draws
/// at the same location. The plugin separates those parts with blank lines; a
/// lint message is one line, so they are separated by a sentence break.
fn message(detail: &Value) -> String {
    let mut message = detail
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_end()
        .to_owned();
    let description = detail.get("description").and_then(Value::as_str);
    let hints = detail
        .get("details")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("kind").and_then(Value::as_str) == Some("hint"))
        .filter_map(|item| item.get("message").and_then(Value::as_str));
    for part in description.into_iter().chain(hints) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if !message.is_empty() {
            if !message.ends_with(['.', '!', '?']) {
                message.push('.');
            }
            message.push(' ');
        }
        message.push_str(part);
    }
    if !message.is_empty() && description.is_some() && !message.ends_with(['.', '!', '?']) {
        message.push('.');
    }
    message
}

/// The line each comment carrying one of [`FLOW_SUPPRESSION_CODES`] ends on.
///
/// The plugin's `getFlowSuppressions`, which reads every comment in the module
/// and records the line its location ends on.
fn flow_suppression_lines(file: &Value) -> Vec<u32> {
    file.get("comments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|comment| {
            comment
                .get("value")
                .and_then(Value::as_str)
                .is_some_and(|text| {
                    flow_fixme_codes(text)
                        .iter()
                        .any(|code| FLOW_SUPPRESSION_CODES.contains(code))
                })
        })
        .filter_map(|comment| {
            let line = comment.get("loc")?.get("end")?.get("line")?.as_u64()?;
            u32::try_from(line).ok()
        })
        .collect()
}

/// The code in each `$FlowFixMe[code]`, found the way the plugin's
/// `/\$FlowFixMe\[([^\]]*)\]/g` finds them: left to right, each match resuming
/// after the `]` that closed the one before.
fn flow_fixme_codes(text: &str) -> Vec<&str> {
    const MARKER: &str = "$FlowFixMe[";
    let mut codes = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(MARKER) {
        let after = &rest[at + MARKER.len()..];
        // No `]` after this marker means none after any later marker either,
        // so the expression matches nothing more.
        let Some(end) = after.find(']') else {
            break;
        };
        codes.push(&after[..end]);
        rest = &after[end + 1..];
    }
    codes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{babel_ast, scope};

    /// `source`'s findings with the plugin's default switches.
    fn linted(source: &str) -> Vec<LintDiagnostic> {
        linted_with(source, "app/page.js", LintSwitches::default())
    }

    fn linted_with(source: &str, path: &str, switches: LintSwitches) -> Vec<LintDiagnostic> {
        on_parse_stack(|| {
            let (file, _) = babel_ast(source).expect("the module parses and lowers");
            let info = scope::analyze(&file);
            lint(&file, info, source, path, switches).expect("the compiler answers")
        })
    }

    /// `source`'s findings, as `(category, line, column)`.
    fn found(source: &str) -> Vec<(ErrorCategory, u32, u32)> {
        linted(source)
            .into_iter()
            .map(|finding| (finding.category, finding.line, finding.column))
            .collect()
    }

    fn may_contain(source: &str) -> bool {
        let source = source.to_owned();
        on_parse_stack(move || {
            let parsed = uf_flow::parse(&source).expect("the module parses");
            may_contain_react_code(&parsed.program)
        })
    }

    fn on_parse_stack<T: Send>(work: impl FnOnce() -> T + Send) -> T {
        std::thread::scope(|thread_scope| {
            std::thread::Builder::new()
                .name("uf-lint-test".to_owned())
                .stack_size(uf_flow::PARSE_STACK_BYTES)
                .spawn_scoped(thread_scope, work)
                .expect("test thread starts")
                .join()
                .expect("test thread completes")
        })
    }

    #[test]
    fn the_options_are_the_eslint_plugins() {
        use react_compiler::entrypoint::CompilerOutputMode;

        let options =
            lint_plugin_options("", "app/page.js", LintSwitches::default()).expect("options");
        assert_eq!(
            CompilerOutputMode::from_opts(&options),
            CompilerOutputMode::Lint
        );
        assert_eq!(options.compilation_mode, "infer");
        assert_eq!(options.panic_threshold, "none");
        assert!(!options.flow_suppressions);
        assert!(options.should_compile);

        // `EnvironmentConfig` ignores a key it does not know, so a misspelt
        // flag would be silently off. Read every one back.
        let environment = &options.environment;
        assert!(environment.validate_ref_access_during_render);
        assert!(environment.validate_no_set_state_in_render);
        assert!(environment.validate_no_set_state_in_effects);
        assert!(environment.validate_no_jsx_in_try_statements);
        assert!(environment.validate_no_impure_functions_in_render);
        assert!(environment.validate_static_components);
        assert!(environment.validate_no_freezing_known_mutable_functions);
        assert!(environment.validate_no_void_use_memo);
        assert_eq!(environment.validate_no_capitalized_calls, Some(Vec::new()));
        assert!(environment.validate_hooks_usage);
        assert!(environment.validate_no_derived_computations_in_effects);
        assert!(!environment.enable_use_keyed_state);
        assert!(!environment.enable_verbose_no_set_state_in_effect);
        let effect_dependencies = |options: &react_compiler::entrypoint::PluginOptions| {
            serde_json::to_value(options.environment.validate_exhaustive_effect_dependencies)
                .expect("serializes")
        };
        assert_eq!(effect_dependencies(&options), "off");

        // The one switch, turned on the way the plugin's rule options turn it on.
        let switched = lint_plugin_options(
            "",
            "app/page.js",
            LintSwitches {
                effect_dependencies: true,
            },
        )
        .expect("options");
        assert_eq!(effect_dependencies(&switched), "all");

        // The plugin's `sources` default: a dependency is never compiled.
        let dependency = lint_plugin_options(
            "",
            "node_modules/some-package/index.js",
            LintSwitches::default(),
        )
        .expect("options");
        assert!(!dependency.should_compile);
    }

    #[test]
    fn a_hook_called_conditionally_in_a_component_declaration_is_a_hooks_finding() {
        assert_eq!(
            found(
                "import {useState} from 'react';\nexport component Toggle(flag: boolean) {\n  if (flag) {\n    const [on] = useState(false);\n  }\n  return null;\n}\n"
            ),
            [(ErrorCategory::Hooks, 4, 17)]
        );
    }

    #[test]
    fn a_hook_called_in_a_loop_in_a_hook_declaration_is_a_hooks_finding() {
        let findings = found(
            "import {useState} from 'react';\nexport hook useAll(items: Array<string>): void {\n  for (const item of items) {\n    useState(item);\n  }\n}\n",
        );
        assert_eq!(findings, [(ErrorCategory::Hooks, 4, 4)]);
    }

    /// `infer`, the plugin's mode: a plain function named like a hook is
    /// compiled once it calls a hook, so it is checked.
    #[test]
    fn a_plain_function_named_like_a_hook_is_checked() {
        let findings = found(
            "import {useEffect} from 'react';\nexport function useThing(flag: boolean) {\n  if (flag) {\n    useEffect(() => {});\n  }\n}\n",
        );
        assert_eq!(findings, [(ErrorCategory::Hooks, 4, 4)]);
    }

    /// And a function named like neither is not compiled, so a hook called
    /// there is not something the compiler reports.
    #[test]
    fn a_function_named_like_neither_is_not_compiled() {
        assert_eq!(
            found(
                "import {useState} from 'react';\nexport function helper() {\n  return useState(0);\n}\nexport component Page() {\n  return <p>{helper()[0]}</p>;\n}\n"
            ),
            []
        );
    }

    #[test]
    fn purity_and_globals_carry_the_compilers_own_words() {
        let findings = linted(
            "let renders = 0;\nexport component Clock() {\n  renders = renders + 1;\n  return <p>{Date.now()}</p>;\n}\n",
        );
        let categories: Vec<ErrorCategory> =
            findings.iter().map(|finding| finding.category).collect();
        assert!(
            categories.contains(&ErrorCategory::Globals)
                && categories.contains(&ErrorCategory::Purity),
            "{findings:#?}"
        );
        for finding in &findings {
            let reason = match finding.category {
                ErrorCategory::Globals => {
                    "Cannot reassign variables declared outside of the component/hook"
                }
                ErrorCategory::Purity => "Cannot call impure function during render",
                _ => continue,
            };
            assert!(finding.message.starts_with(reason), "{finding:#?}");
            assert!(
                finding.message.len() > reason.len(),
                "the description is part of the message: {finding:#?}"
            );
        }
    }

    /// The plugin ships effect dependency checking switched off, and so does
    /// uf: the compiler reports a missing dependency only once it is asked to.
    #[test]
    fn effect_dependencies_are_checked_only_when_switched_on() {
        let source = "import {useEffect} from 'react';\nexport component Logger(id: string, value: string) {\n  useEffect(() => {\n    console.log(id, value);\n  }, [id]);\n  return null;\n}\n";
        let effect_findings = |switches: LintSwitches| {
            linted_with(source, "app/effect-dependencies.js", switches)
                .into_iter()
                .filter(|finding| finding.category == ErrorCategory::EffectExhaustiveDependencies)
                .map(|finding| (finding.line, finding.column))
                .collect::<Vec<_>>()
        };
        assert_eq!(effect_findings(LintSwitches::default()), []);
        assert_eq!(
            effect_findings(LintSwitches {
                effect_dependencies: true
            }),
            [(4, 20)]
        );
    }

    /// `@uniflowed/react` is React re-exported, and the compiler has to be
    /// told, or a ref from it is an ordinary value.
    #[test]
    fn a_ref_from_the_react_facade_is_a_ref() {
        let source = |from: &str| {
            format!(
                "import {{useRef}} from '{from}';\nexport component Count() {{\n  const box = useRef(0);\n  return <p>{{box.current}}</p>;\n}}\n"
            )
        };
        let refs = |from: &str| {
            found(&source(from))
                .into_iter()
                .filter(|(category, ..)| *category == ErrorCategory::Refs)
                .count()
        };
        assert_eq!(refs("react"), 1);
        assert_eq!(refs("@uniflowed/react"), 1);
    }

    #[test]
    fn a_flow_suppression_on_the_line_above_drops_the_finding() {
        let with = |comment: &str| {
            found(&format!(
                "import {{useState}} from 'react';\nexport component Toggle(flag: boolean) {{\n  if (flag) {{\n    {comment}\n    const [on] = useState(false);\n  }}\n  return null;\n}}\n"
            ))
        };
        assert_eq!(with("// $FlowFixMe[react-rule-hook]"), []);
        assert_eq!(with("/* $FlowFixMe[react-rule-unsafe-ref] */"), []);
        // Only those two codes, and only `$FlowFixMe`.
        assert_eq!(
            with("// $FlowFixMe[incompatible-type]"),
            [(ErrorCategory::Hooks, 5, 17)]
        );
        assert_eq!(
            with("// $FlowExpectedError[react-rule-hook]"),
            [(ErrorCategory::Hooks, 5, 17)]
        );
    }

    /// The table is process-wide and other tests fill it too, so this one
    /// uses paths nothing else lints.
    #[test]
    fn a_module_asked_about_again_with_the_same_text_is_answered_from_memory() {
        let path = "app/remembered-by-this-test-only.js";
        let source = "import {useState} from 'react';\nexport component Toggle(flag: boolean) {\n  if (flag) {\n    const [on] = useState(false);\n  }\n  return null;\n}\n";
        let plain = LintSwitches::default();
        assert!(cached(path, source, plain).is_none());

        let first = linted_with(source, path, plain);
        assert_eq!(first.len(), 1, "{first:#?}");

        assert_eq!(
            cached(path, source, plain).as_deref(),
            Some(first.as_slice())
        );
        // A different text, path or switch is a different question.
        assert!(cached(path, &format!("{source}\n"), plain).is_none());
        assert!(cached("app/never-linted-by-this-test.js", source, plain).is_none());
        let switched = LintSwitches {
            effect_dependencies: true,
        };
        assert!(cached(path, source, switched).is_none());
    }

    #[test]
    fn a_flow_fixme_code_is_read_the_way_the_plugins_expression_reads_it() {
        assert_eq!(
            flow_fixme_codes("$FlowFixMe[react-rule-hook]"),
            ["react-rule-hook"]
        );
        assert_eq!(
            flow_fixme_codes("$FlowFixMe[a] then $FlowFixMe[b]"),
            ["a", "b"]
        );
        assert_eq!(flow_fixme_codes("$FlowFixMe[]"), [""]);
        assert!(flow_fixme_codes("$FlowFixMe[unclosed").is_empty());
        // The first match swallows the second marker, and matching resumes
        // after its `]`.
        assert_eq!(
            flow_fixme_codes("$FlowFixMe[$FlowFixMe[x] y]"),
            ["$FlowFixMe[x"]
        );
    }

    /// The plugin's `mayContainReactCode`, statement shape by statement shape.
    #[test]
    fn which_modules_are_compiled_is_the_plugins_answer() {
        for (source, expected) in [
            ("component A() { return null; }", true),
            ("export component A() { return null; }", true),
            ("hook useA() { return null; }", true),
            ("export default hook useA() { return null; }", true),
            ("export default function () { return null; }", true),
            ("export default () => null;", true),
            ("export default function named() { return null; }", false),
            ("export default function Named() { return null; }", true),
            ("function Card() { return null; }", true),
            ("function useThing() { return null; }", true),
            ("function use1() { return null; }", true),
            ("function user() { return null; }", false),
            ("function card() { return null; }", false),
            ("export const Card = () => null;", true),
            ("const useThing = function () { return null; };", true),
            ("const Card = memo(() => null);", false),
            ("const CARD = 1;", false),
            ("const [Card] = [() => null];", false),
            ("class Card {}", false),
            ("if (true) { function Card() { return null; } }", false),
            ("const helpers = { Card() { return null; } };", false),
            ("export const value = 1;", false),
        ] {
            assert_eq!(may_contain(source), expected, "{source}");
        }
    }
}
