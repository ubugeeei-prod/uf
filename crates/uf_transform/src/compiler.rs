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
            let diagnostics = events
                .iter()
                .filter_map(|event| diagnostic(event, source))
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
/// # Which events are reported
///
/// Everything but `CompileSuccess`, and that includes `PipelineError` — the
/// compiler's own internal failure, carrying a `data` string rather than a
/// reason written for an application author.
///
/// This line used to read `kind == "CompileSuccess" || kind == "PipelineError"
/// && false`, which is `kind == "CompileSuccess"` with a decoration: `&&` binds
/// tighter than `||`, so the second clause was a constant `false` and
/// `PipelineError` was reported anyway. Somebody meant to suppress it and
/// stopped half way (ubugeeei-prod/uf#372).
///
/// It is reported, deliberately. A `PipelineError` means the React Compiler
/// gave up on that function: the code still runs, and it is *not memoised*.
/// That is the author's consequence whether or not the message is written for
/// them, and a compiler that silently stops optimising your component is worse
/// than one that says something you have to look up. The message is jargon;
/// silence would be a lie.
fn diagnostic(event: &Value, source: &str) -> Option<CompilerDiagnostic> {
    let kind = event.get("kind")?.as_str()?;
    if kind == "CompileSuccess" {
        return None;
    }
    let message = event
        .get("detail")
        .and_then(|detail| detail.get("reason").or_else(|| detail.get("description")))
        .and_then(Value::as_str)
        .or_else(|| event.get("reason").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| kind.to_owned());
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
    // Read out of the source rather than out of the event. `fnName` is declared
    // on exactly one `LoggerEvent` variant — `CompileSuccess` — and that one
    // never becomes a diagnostic, so this field was `None` for every diagnostic
    // uf could ever emit: a promise the type made and the code could not keep
    // (ubugeeei-prod/uf#371). What the events *do* carry is `fnLoc`, and uf
    // already holds the source those positions are into.
    let function = number("line")
        .zip(number("column"))
        .and_then(|(line, column)| declared_name(source, line, column));
    Some(CompilerDiagnostic {
        kind: kind.to_owned(),
        message,
        function,
        line: number("line"),
        column: number("column"),
    })
}

/// The name of the function declared at `line`:`column`, when it has one.
///
/// # What it reads
///
/// The declaration the compiler pointed at, in the four shapes that carry a
/// name a reader would use:
///
/// * `function Name(`, `component Name(`, `hook useName(` — the identifier
///   after the keyword, `async` and `export`/`export default` skipped.
/// * `const Name = …`, and `let`/`var` — the binding's name, which is the name
///   an arrow function is known by even though the arrow has none.
///
/// `None` for everything else, which is the honest answer for an arrow passed
/// straight to a call: it has no name, and inventing one from the surrounding
/// expression would put a name in a diagnostic that appears nowhere in the
/// file.
///
/// Text rather than an AST because that is all this needs and the AST at this
/// point is Babel's JSON. A declaration keyword followed by an identifier is
/// not a shape that needs parsing to recognise, and the failure mode of getting
/// it wrong is a missing parenthetical rather than a wrong compilation.
fn declared_name(source: &str, line: u32, column: u32) -> Option<String> {
    let start = offset_of(source, line, column)?;
    let mut rest = source.get(start..)?;

    // The compiler points at the declaration, which may open with modifiers.
    loop {
        rest = rest.trim_start();
        let stripped = ["export default ", "export ", "async "]
            .iter()
            .find_map(|prefix| rest.strip_prefix(*prefix));
        match stripped {
            Some(next) => rest = next,
            None => break,
        }
    }

    let after = ["function ", "component ", "hook ", "const ", "let ", "var "]
        .iter()
        .find_map(|keyword| rest.strip_prefix(*keyword))?;
    let name: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The byte offset of a 1-based line and 0-based column.
///
/// The column is in characters, which is what the compiler's positions are, so
/// it is walked rather than added: a line with an emoji in it before the
/// declaration would otherwise land mid-character and the name would come back
/// truncated or empty.
fn offset_of(source: &str, line: u32, column: u32) -> Option<usize> {
    let mut offset = 0usize;
    for _ in 1..line {
        offset += source.get(offset..)?.find('\n')? + 1;
    }
    let rest = source.get(offset..)?;
    let extra = rest
        .char_indices()
        .nth(usize::try_from(column).ok()?)
        .map(|(index, _)| index)
        .unwrap_or(rest.len());
    Some(offset + extra)
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

    /// ubugeeei-prod/uf#371, end to end: a real finding from the real compiler
    /// names the function it is about.
    ///
    /// `CompilerDiagnostic.function` used to be read from the event's `fnName`,
    /// which only `CompileSuccess` carries and which never becomes a
    /// diagnostic — so this was `None` for every finding uf could produce, and
    /// `packages/vite/index.js` printed its `?? "a function"` fallback every
    /// time.
    #[test]
    fn a_finding_names_the_function_it_is_about() {
        let source = "import {useState} from 'react';\ncomponent Bad(flag: boolean) { if (flag) { useState(0); } return null; }\n";
        let (_, diagnostics, _) = compiled(source, ReactCompilerMode::Syntax);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.function.as_deref() == Some("Bad")),
            "no diagnostic named the component: {diagnostics:?}"
        );
    }
}

#[cfg(test)]
mod name_tests {
    use super::{declared_name, offset_of};

    /// ubugeeei-prod/uf#371: `CompilerDiagnostic.function` was `None` for every
    /// diagnostic uf could emit, because the name is not in the event.
    #[test]
    fn a_declaration_gives_up_the_name_a_reader_would_use() {
        for (source, name) in [
            ("function Form() {}", "Form"),
            ("component Form() {}", "Form"),
            ("hook useForm() {}", "useForm"),
            ("const Form = () => {};", "Form"),
            ("let Form = function () {};", "Form"),
            ("var Form = () => {};", "Form"),
            ("async function Form() {}", "Form"),
            ("export function Form() {}", "Form"),
            ("export default function Form() {}", "Form"),
            ("export const Form = () => {};", "Form"),
            ("export default async function Form() {}", "Form"),
            ("const $form_2 = () => {};", "$form_2"),
        ] {
            assert_eq!(
                declared_name(source, 1, 0).as_deref(),
                Some(name),
                "{source}"
            );
        }
    }

    /// And nothing where there is nothing. Inventing a name from the
    /// surrounding expression would put a word in a diagnostic that appears
    /// nowhere in the file.
    #[test]
    fn an_anonymous_function_has_no_name_to_report() {
        for source in [
            "useMemo(() => compute(), [])",
            "items.map(function () {})",
            "export default () => {};",
            "",
        ] {
            assert_eq!(declared_name(source, 1, 0), None, "{source:?}");
        }
    }

    #[test]
    fn the_position_is_the_one_the_compiler_pointed_at() {
        let source = "// a header
import x from 'y';

export function Form() {}
";

        assert_eq!(declared_name(source, 4, 0).as_deref(), Some("Form"));
        // A line the compiler never points at has no declaration on it.
        assert_eq!(declared_name(source, 2, 0), None);
        // Past the end is not a panic.
        assert_eq!(declared_name(source, 99, 0), None);
    }

    /// The column is in characters and the offset is in bytes, and a line with
    /// an emoji in front of the declaration is where the two part company.
    #[test]
    fn a_column_is_characters_rather_than_bytes() {
        let source = "const emoji = \"🙂\"; function Form() {}\n";
        let column = source.chars().position(|c| c == 'f').unwrap();

        assert_eq!(offset_of(source, 1, 0), Some(0));
        assert_eq!(
            declared_name(source, 1, u32::try_from(column).unwrap()).as_deref(),
            Some("Form")
        );
    }
}
