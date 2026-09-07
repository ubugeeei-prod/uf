//! What the official React Compiler did with a module's hand-written
//! memoization.
//!
//! A `useMemo` or a `useCallback` in a component uf compiles is usually noise:
//! the compiler memoizes what needs memoizing, and the hand-written one is a
//! second dependency array to keep correct. It is not *always* noise, and that
//! is the whole difficulty — a callback whose identity crosses into a function
//! the compiler declined, or a memo whose dependencies the compiler cannot
//! reproduce, is load-bearing.
//!
//! So this does not guess. It runs the compiler and looks at what came back.
//!
//! 1. Every `useMemo`/`useCallback` call in the module is recorded, with the
//!    position its name is written at.
//! 2. The module is compiled, exactly as `uf build` compiles it — the same
//!    [`plugin_options`](crate::compiler::plugin_options), so the linter cannot
//!    report a memoization the build then keeps.
//! 3. A call is redundant when it stood inside a function the compiler reported
//!    `CompileSuccess` for **and** is gone from the compiled output. The
//!    compiler's `DropManualMemoization` pass removes a manual memo only when
//!    its own reactive scopes reproduce it; where they cannot, the function
//!    fails `ValidatePreservedManualMemoization` and is not compiled at all,
//!    and the call is still standing in the output.
//!
//! Which means every case that has to stay silent stays silent for a reason
//! the compiler gave, not one uf inferred:
//!
//! | Written in | Compiler said | Reported |
//! | --- | --- | --- |
//! | a `component` or `hook` uf compiles | `CompileSuccess`, call gone | yes |
//! | a plain `function` (`syntax` mode compiles neither) | nothing | no |
//! | a function carrying `"use no memo"` | `CompileSkip` | no |
//! | a memo whose dependencies are incomplete | `CompileError` | no |
//! | a function the compiler bailed on for any other reason | not a success | no |
//!
//! # Cost
//!
//! This compiles the module, which is far more work than a lint rule usually
//! does. The caller is expected to ask only when the module's text contains
//! `useMemo` or `useCallback` at all, and [`redundant_memoization`] returns
//! early — before the compiler runs — when the tree holds neither.

use react_compiler::entrypoint::{CompileResult, LoggerEvent, compile_program};
use react_compiler_ast::File;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::compiler::plugin_options;
use crate::{TransformError, TransformOptions, scope};

/// The two calls a developer writes to memoize by hand.
///
/// `useEffect`'s dependency array is not memoization and `useRef` is not
/// either; these two are the pair the compiler's `DropManualMemoization` pass
/// removes, and this list has to match it or uf would report a call the
/// compiler left alone.
const MANUAL_MEMO: [&str; 2] = ["useCallback", "useMemo"];

/// One hand-written memoization the official React Compiler made redundant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedundantMemo {
    /// `"useMemo"` or `"useCallback"`.
    pub hook: &'static str,
    /// 1-based line the hook's name is written on.
    pub line: u32,
    /// 0-based column of the hook's name, in UTF-16 code units — the unit
    /// Babel's `loc` counts and source maps use.
    pub column: u32,
    /// Name of the compiled function the call stood in, when the compiler
    /// reported one.
    pub function: Option<String>,
}

/// Every hand-written memoization in `file` that the official React Compiler
/// removed.
///
/// `file` is the module's Babel tree, from [`crate::babel_ast`], and `source`
/// and `options` are the ones the module would be built with — the compiler
/// reads the source text for its suppression comments and the options decide
/// which functions it compiles at all.
///
/// # Errors
///
/// [`TransformError::Internal`] when the tree does not deserialize into the
/// compiler's AST or its result does not serialize back, which is a bug in uf
/// rather than in the module. A compiler error that is *about* the module is
/// not an error here: it means nothing was compiled, so nothing is redundant,
/// and the answer is an empty list.
///
/// # Call this from a thread with `uf_flow::PARSE_STACK_BYTES` of stack
///
/// The compiler recurses through the tree, and so does dropping it.
pub fn redundant_memoization(
    file: &Value,
    source: &str,
    options: &TransformOptions,
) -> Result<Vec<RedundantMemo>, TransformError> {
    let written = manual_memo_calls(file);
    if written.is_empty() {
        return Ok(Vec::new());
    }

    let ast: File = serde_json::from_value(file.clone()).map_err(|error| {
        TransformError::Internal(format!("Babel AST rejected by the React Compiler: {error}"))
    })?;
    let plugin = plugin_options(source, options)?;
    let scope = scope::analyze(file);

    let (compiled, events) = match compile_program(ast, scope, plugin) {
        CompileResult::Success { ast, events, .. } => (ast, events),
        // The compiler asked for this one to be fatal. Nothing was compiled,
        // so nothing is redundant — and the build reports it, not the linter.
        CompileResult::Error { .. } => return Ok(Vec::new()),
    };

    let succeeded: Vec<CompiledFunction> = events.iter().filter_map(compiled_function).collect();
    // No function compiled, or the compiler changed nothing: every manual memo
    // in the module is still doing its job.
    let (Some(compiled), false) = (compiled, succeeded.is_empty()) else {
        return Ok(Vec::new());
    };

    let output = serde_json::to_value(compiled).map_err(|error| {
        TransformError::Internal(format!("compiled AST could not be serialized: {error}"))
    })?;
    let kept = manual_memo_calls(&output);

    Ok(written
        .into_iter()
        .filter_map(|call| {
            let function = succeeded
                .iter()
                .find(|function| function.contains(call.at))?;
            // Still in the output: the compiler left this one alone even
            // though the function around it compiled, so it is not uf's to
            // call redundant.
            if kept.iter().any(|other| other.at == call.at) {
                return None;
            }
            Some(RedundantMemo {
                hook: call.hook,
                line: call.at.line,
                column: call.at.column,
                function: function.name.clone(),
            })
        })
        .collect())
}

/// A position in the module, as Babel's `loc` and the compiler's events both
/// spell it: a 1-based line and a 0-based UTF-16 column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    line: u32,
    column: u32,
}

impl Position {
    /// Read a `{ "line": …, "column": … }` object.
    fn from_json(value: &Value) -> Option<Self> {
        Some(Self {
            line: u32::try_from(value.get("line")?.as_u64()?).ok()?,
            column: u32::try_from(value.get("column")?.as_u64()?).ok()?,
        })
    }
}

/// One `useMemo`/`useCallback` call, identified by where its name is written.
///
/// The position is the identity: the same call has the same position in the
/// tree the compiler was handed and in the tree it gave back, because both are
/// rendered from the author's own bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManualMemo {
    hook: &'static str,
    at: Position,
}

/// A function the compiler reported `CompileSuccess` for.
struct CompiledFunction {
    name: Option<String>,
    start: Position,
    end: Position,
}

impl CompiledFunction {
    fn contains(&self, at: Position) -> bool {
        self.start <= at && at <= self.end
    }
}

/// The function a `CompileSuccess` event describes, if the event is one.
fn compiled_function(event: &LoggerEvent) -> Option<CompiledFunction> {
    let LoggerEvent::CompileSuccess {
        fn_loc: Some(location),
        fn_name,
        ..
    } = event
    else {
        return None;
    };
    Some(CompiledFunction {
        name: fn_name.clone(),
        start: Position {
            line: location.start.line,
            column: location.start.column,
        },
        end: Position {
            line: location.end.line,
            column: location.end.column,
        },
    })
}

/// Every `useMemo`/`useCallback` call in a Babel tree, in tree order.
///
/// Iterative rather than recursive on purpose: this runs over a tree whose
/// depth is bounded only by the parser's ceilings, and the walk should not be
/// the thing that needs a large stack.
///
/// Both spellings count — `useMemo(…)` and `React.useMemo(…)` — because the
/// compiler removes both, and reading only the first would leave uf silent
/// about a call the compiler had already dropped.
fn manual_memo_calls(file: &Value) -> Vec<ManualMemo> {
    let mut found = Vec::new();
    let mut pending = vec![file];

    while let Some(node) = pending.pop() {
        match node {
            Value::Array(items) => pending.extend(items),
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("CallExpression")
                    && let Some(callee) = map.get("callee")
                    && let Some((hook, name)) = manual_memo_callee(callee)
                    && let Some(at) = name.get("loc").and_then(|loc| loc.get("start"))
                    && let Some(at) = Position::from_json(at)
                {
                    found.push(ManualMemo { hook, at });
                }
                pending.extend(map.values());
            }
            _ => {}
        }
    }

    found
}

/// The hook a callee names, when it names one of [`MANUAL_MEMO`], with the
/// node that spells the name.
///
/// The name rather than the callee, because that is what a reader is looking
/// for: `React.useMemo(…)` reported at its callee underlines `React`, which
/// says nothing about which call is meant when a line holds two of them. It is
/// also a stable identity across the compile — the property keeps its position
/// in the output tree exactly as the callee would.
fn manual_memo_callee(callee: &Value) -> Option<(&'static str, &Value)> {
    let named = |value: &Value| {
        let name = value.get("name")?.as_str()?;
        MANUAL_MEMO.iter().copied().find(|hook| *hook == name)
    };
    match callee.get("type").and_then(Value::as_str)? {
        "Identifier" => Some((named(callee)?, callee)),
        // `React.useMemo(…)`, and only that: a member call on anything else is
        // a method that happens to share a name.
        "MemberExpression" if callee.get("computed") != Some(&Value::Bool(true)) => {
            let object = callee.get("object")?;
            if object.get("type").and_then(Value::as_str)? != "Identifier"
                || object.get("name").and_then(Value::as_str)? != "React"
            {
                return None;
            }
            let property = callee.get("property")?;
            Some((named(property)?, property))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ReactCompilerMode, babel_ast, transform};

    /// `source` compiled the way `uf build` compiles it.
    fn built(source: &str) -> String {
        let options = TransformOptions {
            react_compiler: ReactCompilerMode::Syntax,
            ..TransformOptions::new("app.js")
        };
        transform(source, &options)
            .expect("the module compiles")
            .code
    }

    /// What the compiler made redundant in `source`, as `(hook, line)` pairs.
    fn redundant(source: &str) -> Vec<(&'static str, u32)> {
        let (file, _) = babel_ast(source).expect("the module parses and lowers");
        let options = TransformOptions {
            react_compiler: ReactCompilerMode::Syntax,
            ..TransformOptions::new("app.js")
        };
        redundant_memoization(&file, source, &options)
            .expect("the compiler answers")
            .into_iter()
            .map(|memo| (memo.hook, memo.line))
            .collect()
    }

    #[test]
    fn a_memo_in_a_compiled_component_is_redundant() {
        assert_eq!(
            redundant(
                "import {useMemo} from 'react';\ncomponent List(items: Array<string>) {\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n"
            ),
            [("useMemo", 3)]
        );
    }

    #[test]
    fn a_callback_in_a_compiled_hook_is_redundant() {
        assert_eq!(
            redundant(
                "import {useCallback} from 'react';\nhook useHandler(id: string): () => void {\n  return useCallback(() => send(id), [id]);\n}\n"
            ),
            [("useCallback", 3)]
        );
    }

    #[test]
    fn a_memo_in_a_function_the_compiler_does_not_compile_is_left_alone() {
        // `syntax` mode compiles `component` and `hook` declarations and
        // nothing else, so this one is still the only memoization there is.
        assert_eq!(
            redundant(
                "import {useMemo} from 'react';\nexport function List(props: {items: Array<string>}) {\n  const sorted = useMemo(() => props.items.slice(), [props.items]);\n  return <ul>{sorted}</ul>;\n}\n"
            ),
            []
        );
    }

    #[test]
    fn a_memo_in_a_function_that_opted_out_is_left_alone() {
        assert_eq!(
            redundant(
                "import {useMemo} from 'react';\ncomponent List(items: Array<string>) {\n  'use no memo';\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n"
            ),
            []
        );
    }

    #[test]
    fn a_memo_the_compiler_could_not_preserve_is_left_alone() {
        // Missing dependency: the compiler refuses the function rather than
        // dropping a memo whose guarantees it cannot reproduce.
        assert_eq!(
            redundant(
                "import {useMemo} from 'react';\ncomponent Sum(a: number, b: number) {\n  const total = useMemo(() => a + b, [a]);\n  return <p>{total}</p>;\n}\n"
            ),
            []
        );
    }

    #[test]
    fn one_module_can_hold_both_answers() {
        let found = redundant(
            "import {useMemo} from 'react';\ncomponent List(items: Array<string>) {\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\nexport function Plain(props: {items: Array<string>}) {\n  const sorted = useMemo(() => props.items.slice(), [props.items]);\n  return <ul>{sorted}</ul>;\n}\n",
        );
        assert_eq!(found, [("useMemo", 3)]);
    }

    #[test]
    fn a_module_with_no_manual_memoization_never_reaches_the_compiler() {
        assert_eq!(
            redundant("component Page() {\n  return <p>hello</p>;\n}\n"),
            []
        );
    }

    #[test]
    fn the_react_namespaced_spelling_is_read_too() {
        assert_eq!(
            redundant(
                "import * as React from 'react';\ncomponent List(items: Array<string>) {\n  const sorted = React.useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n"
            ),
            [("useMemo", 3)]
        );
    }

    /// The claim the rule makes, checked rather than argued.
    ///
    /// `react/no-redundant-memo` tells a reader to delete a call. What makes
    /// that safe is not that the compiler *dropped* the call — it drops one it
    /// then reproduces — but that the module without it compiles to the same
    /// program. So compile both and compare the output, which is the only form
    /// of the claim a reader would care about: the memoization survives, the
    /// cache is the same size, and the only line that changes is the import
    /// that is now unused.
    #[test]
    fn deleting_a_reported_memo_compiles_to_the_same_program() {
        let with = "import {useMemo} from 'react';\nhook useInstant(clockAt: number): number {\n  return useMemo(() => expensive(clockAt), [clockAt]);\n}\n";
        let without =
            "hook useInstant(clockAt: number): number {\n  return expensive(clockAt);\n}\n";
        assert_eq!(redundant(with), [("useMemo", 3)]);

        let kept: Vec<String> = built(with)
            .lines()
            .filter(|line| !line.contains("from \"react\""))
            .map(str::to_owned)
            .collect();
        let dropped: Vec<String> = built(without)
            .lines()
            .filter(|line| !line.contains("from \"react\""))
            .map(str::to_owned)
            .collect();

        assert_eq!(kept, dropped);
        assert!(
            kept.iter().any(|line| line.contains("_c(2)")),
            "the compiler still memoizes: {kept:?}"
        );
    }

    #[test]
    fn a_method_that_shares_the_name_is_not_memoization() {
        assert_eq!(
            redundant(
                "component Page(cache: Cache) {\n  const value = cache.useMemo(1);\n  return <p>{value}</p>;\n}\n"
            ),
            []
        );
    }
}
