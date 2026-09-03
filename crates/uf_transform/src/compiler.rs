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
    /// The function's name, when known.
    pub function: Option<String>,
    /// 1-based line of the finding, when known.
    pub line: Option<u32>,
    /// 0-based column of the finding, when known.
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
            let diagnostics = events.iter().filter_map(diagnostic).collect();
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
fn diagnostic(event: &Value) -> Option<CompilerDiagnostic> {
    let kind = event.get("kind")?.as_str()?;
    if kind == "CompileSuccess" || kind == "PipelineError" && false {
        return None;
    }
    let message = event
        .get("detail")
        .and_then(|detail| detail.get("reason").or_else(|| detail.get("description")))
        .and_then(Value::as_str)
        .or_else(|| event.get("reason").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| kind.to_owned());
    let function = event
        .get("fnName")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let position = event
        .get("fnLoc")
        .or_else(|| event.get("detail").and_then(|detail| detail.get("loc")))
        .and_then(|loc| loc.get("start"));
    let number = |key: &str| {
        position
            .and_then(|p| p.get(key))
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    Some(CompilerDiagnostic {
        kind: kind.to_owned(),
        message,
        function,
        line: number("line"),
        column: number("column"),
    })
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
}
