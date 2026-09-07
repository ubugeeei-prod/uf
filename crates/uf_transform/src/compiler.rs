//! The official React Compiler, in process.
//!
//! `react_compiler` is Meta's Rust implementation of the compiler that ships
//! as `babel-plugin-react-compiler`. It takes Babel's AST and the scope
//! information a front end computed, and returns the same AST with memoised
//! functions substituted. uf drives it in `syntax` mode by default: only
//! functions declared with Flow's `component` and `hook` syntax are compiled,
//! which is precisely the set an author has opted into by writing them that
//! way.
//!
//! The panic threshold is `none`: a function the compiler cannot handle is
//! left as written and reported as a diagnostic, never a failed build.

use react_compiler::entrypoint::{CompileResult, PluginOptions, compile_program};
use react_compiler_ast::File;
use react_compiler_ast::scope::ScopeInfo;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uf_infra::FxHashMap;

use crate::{TransformError, TransformOptions};

/// Which functions the React Compiler memoises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactCompilerMode {
    /// Do not run the compiler.
    Off,
    /// Only `component` and `hook` declarations. The default.
    #[default]
    Syntax,
    /// Also functions the compiler infers to be components or hooks from
    /// their name and shape, as it does for plain JavaScript projects.
    Infer,
    /// Only functions carrying a `"use memo"` directive.
    Annotation,
    /// Every function.
    All,
}

impl ReactCompilerMode {
    /// The compiler's own name for the mode.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Syntax => "syntax",
            Self::Infer => "infer",
            Self::Annotation => "annotation",
            Self::All => "all",
        }
    }
}

/// Something the compiler reported about one function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilerDiagnostic {
    /// The event kind, e.g. `CompileError` or `CompileSkip`.
    pub kind: String,
    /// What the compiler said.
    pub message: String,
    /// The function's name, under the compiler's own naming rules, when it has
    /// one: a declaration is its identifier, an arrow or function expression is
    /// the `const` it is assigned to (through a `memo`/`forwardRef` wrapper and
    /// nothing else), and anything else is anonymous.
    ///
    /// `None` is a real answer and not a gap to paper over — an anonymous
    /// function has no name a reader could search for, and a name uf invented
    /// would send them looking for text that is not in their file.
    pub function: Option<String>,
    /// 1-based line of the finding itself — the expression the compiler
    /// objected to — falling back to the line the function was declared on
    /// when the event locates only the function. Paired with
    /// [`CompilerDiagnostic::function`], which always names the enclosing
    /// function, so the two together read as "this expression, in that
    /// function" rather than repeating one position twice.
    pub line: Option<u32>,
    /// 0-based column of the finding, on [`CompilerDiagnostic::line`].
    pub column: Option<u32>,
}

/// Compile `file`, returning the (possibly rewritten) file, the compiler's
/// diagnostics, and how many functions were memoised.
///
/// # Errors
///
/// [`TransformError::Internal`] when the tree does not deserialize into the
/// compiler's AST, and [`TransformError::Compiler`] when the compiler reports
/// a fatal error.
pub fn compile(
    file: Value,
    scope: ScopeInfo,
    source: &str,
    options: &TransformOptions,
) -> Result<(Value, Vec<CompilerDiagnostic>, usize), TransformError> {
    let ast: File = serde_json::from_value(file.clone()).map_err(|error| {
        TransformError::Internal(format!("Babel AST rejected by the React Compiler: {error}"))
    })?;
    // Built from the tree as it goes in, because that is the tree the events
    // point back into: the compiler may hand back a rewritten AST, and the
    // functions it rewrote are the ones it did not report on anyway.
    let names = function_names(&file);
    let plugin_options = plugin_options(source, options)?;

    match compile_program(ast, scope, plugin_options) {
        CompileResult::Success { ast, events, .. } => {
            let events: Vec<Value> = events
                .iter()
                .map(|event| serde_json::to_value(event).unwrap_or(Value::Null))
                .collect();
            let compiled = events
                .iter()
                .filter(|event| event["kind"] == "CompileSuccess")
                .count();
            let diagnostics = events
                .iter()
                .filter_map(|event| diagnostic(event, &names))
                .collect();
            let rewritten = match ast {
                Some(ast) => serde_json::to_value(ast).map_err(|error| {
                    TransformError::Internal(format!(
                        "compiled AST could not be serialized: {error}"
                    ))
                })?,
                None => file,
            };
            Ok((rewritten, diagnostics, compiled))
        }
        CompileResult::Error { error, .. } => {
            let mut message = error.reason.clone();
            if let Some(description) = &error.description {
                message.push_str(": ");
                message.push_str(description);
            }
            Err(TransformError::Compiler(message))
        }
    }
}

fn plugin_options(
    source: &str,
    options: &TransformOptions,
) -> Result<PluginOptions, TransformError> {
    serde_json::from_value(json!({
        "shouldCompile": true,
        "enableReanimated": false,
        "isDev": options.development,
        "filename": options.filename,
        "compilationMode": options.react_compiler.as_str(),
        "panicThreshold": "none",
        "target": "19",
        "noEmit": false,
        "flowSuppressions": true,
        "ignoreUseNoForget": false,
        "environment": {},
        "__sourceCode": source,
    }))
    .map_err(|error| TransformError::Internal(format!("React Compiler options rejected: {error}")))
}

/// A diagnostic from a logger event that is not a success.
///
/// `CompileSuccess` is the only kind held back, `PipelineError` included, and
/// that is a decision rather than an oversight — the line used to read
/// `kind == "CompileSuccess" || kind == "PipelineError" && false`, where `&&`
/// binds tighter than `||` and the second clause was a constant (#372).
///
/// A `PipelineError` is the compiler failing on a function rather than the
/// function being wrong, and the argument for hiding it is that the author
/// cannot fix the compiler. But the consequence of the failure is theirs: the
/// function is left exactly as written and never memoised, which is what this
/// module's `panicThreshold: "none"` promises will be *reported* and not
/// swallowed. Hiding the only event that says so would mean a `component`
/// quietly opting out of compilation with nothing in any log. `CompileSkip`
/// and `CompileUnexpectedThrow` — the same shape, the same "not compiled, not
/// your fault" — are already reported, and holding back one of the three has
/// no principle behind it. The noise this would have controlled is controlled
/// where it belongs instead: `@uniflowed/vite` holds back a dependency's
/// findings and counts them in one line.
///
/// What the objection was actually about is the message, and that is fixed
/// here: `PipelineError` and `CompileUnexpectedThrow` carry their text in
/// `data` rather than a `detail.reason`, so reading only the latter left them
/// reporting the bare word `PipelineError`.
fn diagnostic(event: &Value, names: &FunctionNames) -> Option<CompilerDiagnostic> {
    let kind = event.get("kind")?.as_str()?;
    if kind == "CompileSuccess" {
        return None;
    }
    let message = event
        .get("detail")
        .and_then(|detail| detail.get("reason").or_else(|| detail.get("description")))
        .and_then(Value::as_str)
        .or_else(|| event.get("reason").and_then(Value::as_str))
        .or_else(|| event.get("data").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| kind.to_owned());
    let declared_at = position(event.get("fnLoc"));
    let at = reported_at(event).or(declared_at);
    Some(CompilerDiagnostic {
        kind: kind.to_owned(),
        message,
        function: declared_at.and_then(|at| names.get(&at).cloned()),
        line: at.map(|(line, _)| line),
        column: at.map(|(_, column)| column),
    })
}

/// Where in the file a finding actually points, as opposed to where the
/// function containing it was declared.
///
/// `detail.loc` is the obvious field for this and the Rust compiler never fills
/// it in: `CompilerErrorDetailInfo::loc` is `skip_serializing_if = "none"` and
/// is `None` on every event uf has seen, because the position is carried one
/// level further down, on the individual `detail.details` items. So the
/// `detail.loc` fallback that was written to make a finding precise was a
/// second read of a field that is not there — the same shape as the `&& false`
/// in [`diagnostic`] (#372), and with a visible cost: every finding fell
/// through to `fnLoc` and reported the line the `component` or `hook` keyword
/// is on. Four separate ref writes in one hook arrived as four byte-identical
/// lines, and nothing in the output said which write each was about.
///
/// The first `error` item is the one the compiler leads with and the one its
/// own formatter underlines; `hint` items are advice about the finding rather
/// than a second site for it, and carry `"loc": null` anyway. `detail.loc` is
/// still read after it, so a future compiler that populates the field is
/// believed rather than ignored.
fn reported_at(event: &Value) -> Option<(u32, u32)> {
    let detail = event.get("detail")?;
    detail
        .get("details")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("kind").and_then(Value::as_str) == Some("error"))
        .and_then(|item| position(item.get("loc")))
        .or_else(|| position(detail.get("loc")))
}

/// The 1-based line and 0-based column a logger location starts at.
fn position(loc: Option<&Value>) -> Option<(u32, u32)> {
    let start = loc?.get("start")?;
    let number = |key: &str| {
        start
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    Some((number("line")?, number("column")?))
}

/// Where each function starts, and what a reader would call it.
type FunctionNames = FxHashMap<(u32, u32), String>;

/// Index the functions in a Babel AST by the position they start at.
///
/// The compiler names a function on `CompileSuccess` and nowhere else: every
/// event that becomes a diagnostic declares `fnLoc` and no `fnName`, so
/// [`CompilerDiagnostic::function`] read a field that structurally could not be
/// there and was `None` for every diagnostic uf would ever emit (#371). The
/// location, though, is the function node's own `loc` from the tree uf handed
/// in, copied through unchanged — which makes the name recoverable by indexing
/// that tree by start position, with no source text to re-scan and no guess
/// about which declaration a position falls inside.
///
/// The naming rules are the compiler's, so that a diagnostic and a success
/// event about the same function agree: a declaration is its `id`; an arrow or
/// function expression takes the name of the `const` it is the initialiser of,
/// carried through a `memo`/`forwardRef` call and through no other call, so
/// `const A = memo(() => …)` is `A` and `const A = wrap(() => …)` is anonymous.
/// Two functions cannot begin at the same line and column, so the index cannot
/// collide.
///
/// The walk recurses on the tree and is bounded by it: `lower` has already
/// refused anything deeper than [`crate::lower::MAX_DEPTH`] by the time a file
/// reaches here, so a source cannot drive this to a stack overflow that the
/// lowering would not have rejected first.
fn function_names(file: &Value) -> FunctionNames {
    let mut names = FunctionNames::default();
    collect_names(file, None, &mut names);
    names
}

/// Walk one node, recording any function it is, and hand `hint` on to the one
/// child that is allowed to inherit it.
fn collect_names(node: &Value, hint: Option<&str>, names: &mut FunctionNames) {
    let object = match node {
        Value::Array(items) => {
            for item in items {
                collect_names(item, hint, names);
            }
            return;
        }
        Value::Object(object) => object,
        _ => return,
    };
    let named = |key: &str| object.get(key).and_then(|id| id.get("name")?.as_str());
    // The one child key that inherits a name, and the name it inherits. Every
    // other child starts again from anonymous, which is what stops a name
    // reaching a function it does not belong to.
    let (carrier, carried) = match object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        kind @ ("FunctionDeclaration" | "FunctionExpression" | "ArrowFunctionExpression") => {
            // A function expression's own `id` is deliberately ignored, as the
            // compiler ignores it: `const A = function B() {}` is reported as
            // `A`, which is the binding the call site uses.
            let name = if kind == "FunctionDeclaration" {
                named("id").or(hint)
            } else {
                hint
            };
            if let (Some(name), Some(at)) = (name, position(object.get("loc"))) {
                names.insert(at, name.to_owned());
            }
            (None, None)
        }
        "VariableDeclarator" => {
            let bound = matches!(
                object
                    .get("init")
                    .and_then(|init| init.get("type"))
                    .and_then(Value::as_str),
                Some("FunctionExpression" | "ArrowFunctionExpression" | "CallExpression")
            )
            .then(|| named("id"))
            .flatten();
            (Some("init"), bound)
        }
        "CallExpression" if is_react_wrapper(object.get("callee")) => (Some("arguments"), hint),
        _ => (None, None),
    };
    for (key, child) in object {
        let inherited = (Some(key.as_str()) == carrier).then_some(carried).flatten();
        collect_names(child, inherited, names);
    }
}

/// Whether a callee is one of the two wrappers the compiler looks through when
/// it is naming a function: `memo` and `forwardRef`, bare or on `React`.
fn is_react_wrapper(callee: Option<&Value>) -> bool {
    let Some(callee) = callee else { return false };
    let name = |node: &Value| node.get("name").and_then(Value::as_str).map(str::to_owned);
    let called = match callee.get("type").and_then(Value::as_str) {
        Some("Identifier") => name(callee),
        Some("MemberExpression") => match callee.get("object").and_then(name).as_deref() {
            Some("React") => callee.get("property").and_then(name),
            _ => None,
        },
        _ => None,
    };
    matches!(called.as_deref(), Some("memo" | "forwardRef"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;
    use crate::{lower, scope};

    fn compiled(source: &str, mode: ReactCompilerMode) -> (Value, Vec<CompilerDiagnostic>, usize) {
        let mut program = parse(source).unwrap();
        lower::lower(&mut program, source).unwrap();
        let file = crate::babel::to_babel(program, source).unwrap();
        let info = scope::analyze(&file);
        let options = TransformOptions {
            react_compiler: mode,
            ..TransformOptions::new("app.js")
        };
        compile(file, info, source, &options).unwrap()
    }

    #[test]
    fn syntax_mode_compiles_a_component_and_leaves_a_plain_function() {
        let source = "import {useState} from 'react';\nexport component App(title: string) { const [n, setN] = useState(0); return <h1 onClick={() => setN(n + 1)}>{title}{n}</h1>; }\nexport function Plain(props) { return <p>{props.x}</p>; }\n";
        let (file, diagnostics, count) = compiled(source, ReactCompilerMode::Syntax);
        assert_eq!(count, 1, "{diagnostics:?}");
        let text = file.to_string();
        assert!(
            text.contains("react/compiler-runtime"),
            "compiler runtime import missing"
        );
        let declaration = |name: &str| {
            file["program"]["body"]
                .as_array()
                .unwrap()
                .iter()
                .map(|statement| &statement["declaration"])
                .find(|declaration| declaration["id"]["name"] == name)
                .cloned()
                .unwrap_or_else(|| panic!("no declaration named {name}"))
        };
        let app = declaration("App");
        let first = &app["body"]["body"][0];
        assert_eq!(first["type"], "VariableDeclaration");
        assert_eq!(first["declarations"][0]["id"]["name"], "$");
        let plain = declaration("Plain");
        assert_eq!(plain["body"]["body"][0]["type"], "ReturnStatement");
    }

    #[test]
    fn a_module_without_components_comes_back_unchanged() {
        let source = "export const a = 1;\n";
        let (file, diagnostics, count) = compiled(source, ReactCompilerMode::Syntax);
        assert_eq!(count, 0);
        assert!(diagnostics.is_empty());
        assert_eq!(file["program"]["body"][0]["type"], "ExportNamedDeclaration");
    }

    #[test]
    fn a_hook_rules_violation_is_a_diagnostic_not_a_failure() {
        let source = "import {useState} from 'react';\ncomponent Bad(flag: boolean) { if (flag) { useState(0); } return null; }\n";
        let (_, diagnostics, count) = compiled(source, ReactCompilerMode::Syntax);
        assert_eq!(count, 0);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn a_diagnostic_names_the_declaration_it_is_about() {
        let source = "import {useState} from 'react';\nexport component Bad(flag: boolean) { if (flag) { useState(0); } return null; }\n";
        let (_, diagnostics, _) = compiled(source, ReactCompilerMode::Syntax);
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| d.function.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("Bad")],
        );
    }

    #[test]
    fn a_hook_declaration_is_named_too() {
        let source = "import {useRef} from 'react';\nhook useThing(n: number) { const r = useRef(0); r.current = n; return r.current; }\n";
        let (_, diagnostics, _) = compiled(source, ReactCompilerMode::Syntax);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .all(|d| d.function.as_deref() == Some("useThing")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn an_arrow_takes_the_name_of_the_binding_it_is_assigned_to() {
        let source = "import {useRef} from 'react';\nconst Card = () => { const r = useRef(0); r.current = 1; return <p>{r.current}</p>; };\nexport default Card;\n";
        let (_, diagnostics, _) = compiled(source, ReactCompilerMode::Infer);
        assert!(!diagnostics.is_empty(), "no diagnostic to name");
        assert!(
            diagnostics
                .iter()
                .all(|d| d.function.as_deref() == Some("Card")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn a_memo_wrapper_is_looked_through_and_another_call_is_not() {
        let wrapped = "import {memo, useRef} from 'react';\nconst Card = memo(() => { const r = useRef(0); r.current = 1; return <p>{r.current}</p>; });\nexport default Card;\n";
        let (_, diagnostics, _) = compiled(wrapped, ReactCompilerMode::Infer);
        assert!(!diagnostics.is_empty(), "no diagnostic to name");
        assert!(
            diagnostics
                .iter()
                .all(|d| d.function.as_deref() == Some("Card")),
            "{diagnostics:?}"
        );

        // `wrap` is not `memo`, so the compiler does not carry `Card` into it
        // and neither does uf: naming that function `Card` would name the
        // wrapper's argument after the wrapper's result.
        let opaque = "import {useRef} from 'react';\nimport {wrap} from './wrap.js';\nconst Card = wrap(() => { const r = useRef(0); r.current = 1; return <p>{r.current}</p>; });\nexport default Card;\n";
        let (_, diagnostics, _) = compiled(opaque, ReactCompilerMode::All);
        assert!(!diagnostics.is_empty(), "no diagnostic to name");
        assert!(
            diagnostics.iter().all(|d| d.function.is_none()),
            "{diagnostics:?}"
        );
    }

    /// The `data`-carrying kinds, which reach uf with no `detail.reason` and no
    /// `reason`, and which the dead `&& false` clause claimed to filter out.
    #[test]
    fn an_internal_failure_is_reported_and_says_what_failed() {
        let names = FunctionNames::from_iter([((7, 2), String::from("Form"))]);
        for kind in ["PipelineError", "CompileUnexpectedThrow"] {
            let event = json!({
                "kind": kind,
                "fnLoc": {"start": {"line": 7, "column": 2}, "end": {"line": 9, "column": 1}},
                "data": "Error: Invariant: unexpected terminal kind",
            });
            let reported = diagnostic(&event, &names).expect("reported");
            assert_eq!(reported.kind, kind);
            assert_eq!(
                reported.message,
                "Error: Invariant: unexpected terminal kind"
            );
            assert_eq!(reported.function.as_deref(), Some("Form"));
            assert_eq!((reported.line, reported.column), (Some(7), Some(2)));
        }
    }

    #[test]
    fn only_a_success_is_held_back() {
        let event = json!({
            "kind": "CompileSuccess",
            "fnLoc": {"start": {"line": 1, "column": 0}, "end": {"line": 1, "column": 9}},
            "fnName": "Form",
        });
        assert!(diagnostic(&event, &FunctionNames::default()).is_none());
    }

    /// Two findings in one hook have to arrive as two different lines. Both
    /// events carry the same `fnLoc`, so a position taken from `fnLoc` makes
    /// them identical; the position that tells them apart is on the `error`
    /// item inside `detail.details`, and `detail.loc` — which the old fallback
    /// read — is absent, exactly as the compiler serializes it.
    #[test]
    fn a_finding_points_at_the_expression_rather_than_the_declaration() {
        let names = FunctionNames::from_iter([((178, 7), String::from("useLongPress"))]);
        let at = |line: u32, column: u32| {
            json!({
                "kind": "CompileError",
                "fnLoc": {"start": {"line": 178, "column": 7}, "end": {"line": 225, "column": 1}},
                "detail": {
                    "category": "Immutability",
                    "reason": "This value cannot be modified",
                    "description": "Modifying a value returned from a hook is not allowed",
                    "severity": "Error",
                    "suggestions": null,
                    "details": [
                        {"kind": "error", "loc": {"start": {"line": line, "column": column}}, "message": "`pending` cannot be modified"},
                        {"kind": "hint", "loc": null, "message": "Hint: rename the variable to end in \"Ref\"."},
                    ],
                },
            })
        };
        let reported = |line, column| {
            let d = diagnostic(&at(line, column), &names).expect("reported");
            (d.function, d.line, d.column)
        };
        assert_eq!(
            reported(195, 6),
            (Some(String::from("useLongPress")), Some(195), Some(6)),
        );
        assert_ne!(reported(195, 6), reported(203, 4));
    }

    /// With no located `error` item there is still the function to point at,
    /// which is worse than a precise position and much better than none.
    #[test]
    fn a_finding_the_compiler_did_not_locate_falls_back_to_the_declaration() {
        let names = FunctionNames::from_iter([((7, 2), String::from("Form"))]);
        let event = json!({
            "kind": "CompileError",
            "fnLoc": {"start": {"line": 7, "column": 2}, "end": {"line": 9, "column": 1}},
            "detail": {
                "reason": "Cannot access refs during render",
                "details": [{"kind": "hint", "loc": null, "message": "Hint: …"}],
            },
        });
        let reported = diagnostic(&event, &names).expect("reported");
        assert_eq!(reported.function.as_deref(), Some("Form"));
        assert_eq!((reported.line, reported.column), (Some(7), Some(2)));
    }
}
