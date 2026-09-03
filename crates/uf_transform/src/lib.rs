#![deny(missing_docs)]
//! Flow → JavaScript, with nothing but upstream code deciding what the language means.
//!
//! `uf` projects are written in Flow — `component` and `hook` declarations,
//! `match`, enums, and type annotations — and every host runs JavaScript. This
//! crate is the one place that turns one into the other, and it is deliberately
//! assembled from the implementations that *own* each step:
//!
//! 1. **Parse** with Meta's official Flow parser (`flow_parser`, vendored from
//!    `upstream/flow`), and take its ESTree rendering ([`estree`]).
//! 2. **Lower** Flow-only syntax exactly the way Flow's own toolchain does
//!    ([`lower`]): the rules are ported from `hermes-parser`'s
//!    `TransformComponentSyntax`, `TransformMatchSyntax`, `TransformEnumSyntax`
//!    and `StripFlowTypes`, so a `match` compiles to the same conditions Flow
//!    documents and a `component` to the same destructured function.
//! 3. **Convert** the ESTree to Babel's AST shape ([`babel`]) and analyse its
//!    scopes ([`scope`]), which is the contract the official React Compiler
//!    consumes.
//! 4. **Compile** with the official React Compiler's Rust implementation
//!    ([`compiler`]) in `syntax` mode: only `component`/`hook` declarations are
//!    memoised, which is the mode Flow's syntax exists for.
//! 5. **Print** the compiled program back to JavaScript with a source map
//!    ([`print`]), then hand it to oxc — the engine inside Vite and Rolldown —
//!    for the JSX automatic runtime, React Fast Refresh registration and code
//!    generation ([`emit`]).
//!
//! There is no Babel anywhere in this pipeline, and no grammar uf invented.
//!
//! ```
//! use uf_transform::{TransformOptions, transform};
//!
//! let source = "// @flow\nexport component Hello(name: string) { return <p>{name}</p>; }\n";
//! let out = transform(source, &TransformOptions::new("hello.js")).expect("transforms");
//! assert!(out.code.contains("function Hello"));
//! assert!(!out.code.contains(": string"));
//! assert!(out.code.contains("jsx"));
//! ```

pub mod babel;
pub mod compiler;
pub mod emit;
pub mod estree;
pub mod lower;
pub mod print;
pub mod scope;

use thiserror::Error;

pub use crate::compiler::{CompilerDiagnostic, ReactCompilerMode};
pub use crate::estree::MAX_SOURCE_BYTES;

/// How one module is transformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformOptions {
    /// The module's path, used in source maps and in diagnostics.
    pub filename: String,
    /// Development output: `jsxDEV`, readable code, and Fast Refresh when
    /// [`TransformOptions::refresh`] is set.
    pub development: bool,
    /// Add React Fast Refresh registrations (`$RefreshReg$`/`$RefreshSig$`).
    /// Only meaningful in development.
    pub refresh: bool,
    /// Which functions the React Compiler memoises.
    pub react_compiler: ReactCompilerMode,
    /// Where the automatic JSX runtime is imported from.
    pub jsx_import_source: String,
    /// Produce a source map.
    pub source_map: bool,
}

impl TransformOptions {
    /// Production options for a module at `filename`.
    #[must_use]
    pub fn new(filename: impl Into<String>) -> Self {
        Self {
            filename: filename.into(),
            development: false,
            refresh: false,
            react_compiler: ReactCompilerMode::Syntax,
            jsx_import_source: String::from("react"),
            source_map: true,
        }
    }
}

/// A transformed module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transformed {
    /// The JavaScript.
    pub code: String,
    /// A source map (JSON) back to the Flow source, when one was asked for.
    pub map: Option<String>,
    /// What the React Compiler reported: functions it declined, and why.
    pub compiler_diagnostics: Vec<CompilerDiagnostic>,
    /// How many functions the React Compiler memoised.
    pub compiled_functions: usize,
}

/// Why a module could not be transformed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransformError {
    /// The source is larger than [`MAX_SOURCE_BYTES`].
    #[error("source is {bytes} bytes, over the {limit} byte ceiling")]
    SourceTooLarge {
        /// Size of the rejected source.
        bytes: usize,
        /// The ceiling.
        limit: usize,
    },
    /// The official parser rejected the source.
    #[error("{message}")]
    Syntax {
        /// What the parser said.
        message: String,
        /// 1-based line.
        line: u32,
        /// 0-based column, in UTF-16 code units, as editors count.
        column: u32,
    },
    /// A construct the lowering rules refuse, such as a `var` binding in a
    /// `match` pattern.
    #[error("{message}")]
    Lowering {
        /// What was refused.
        message: String,
        /// 1-based line, when known.
        line: Option<u32>,
        /// 0-based column, when known.
        column: Option<u32>,
    },
    /// The React Compiler failed in a way it asked to be fatal.
    #[error("React Compiler: {0}")]
    Compiler(String),
    /// The AST produced along the way did not match the shape the next stage
    /// expects. This is a bug in uf, never in the user's source.
    #[error("internal transform error: {0}")]
    Internal(String),
}

/// Whether uf is responsible for transforming the module at `id`.
///
/// Two things are excluded and one deliberately is not. A build driver
/// synthesises modules of its own — Vite's client, a bundler's shims, ids
/// starting with a NUL byte — and a third-party dependency ships JavaScript
/// that is already JavaScript; handing either to a Flow transform turns a
/// file nobody wrote into a syntax error. `@uniflowed/*` under `node_modules`
/// is *not* excluded: those packages ship Flow source, because that is what
/// uf tells everyone to write.
///
/// `@uniflowed/vite` applies the same policy in JavaScript before it asks,
/// and must keep doing so: a `uf dev` session and a `uf test` run that
/// disagree about which files are Flow disagree about what the code is.
#[must_use]
pub fn is_flow_module(id: &str) -> bool {
    if id.starts_with('\0') {
        return false;
    }
    let path = id.split_once('?').map_or(id, |(path, _)| path);
    if !FLOW_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
    {
        return false;
    }
    match path.rfind("/node_modules/") {
        Some(at) => path[at..].starts_with("/node_modules/@uniflowed/"),
        None => true,
    }
}

/// File extensions uf treats as Flow source.
pub const FLOW_EXTENSIONS: [&str; 4] = [".js", ".jsx", ".mjs", ".cjs"];

/// Transform one Flow module to JavaScript.
///
/// # Errors
///
/// See [`TransformError`].
pub fn transform(source: &str, options: &TransformOptions) -> Result<Transformed, TransformError> {
    let mut program = estree::parse(source)?;
    let lowered = lower::lower(&mut program, source)?;
    let file = babel::to_babel(program, source)?;

    let (file, compiler_diagnostics, compiled_functions) =
        if options.react_compiler == ReactCompilerMode::Off || !lowered.may_compile {
            (file, Vec::new(), 0)
        } else {
            let scope = scope::analyze(&file);
            compiler::compile(file, scope, source, options)?
        };

    let printed = print::print(&file)?;
    let emitted = emit::emit(&printed, source, options)?;

    Ok(Transformed {
        code: emitted.code,
        map: emitted.map,
        compiler_diagnostics,
        compiled_functions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_driver_s_own_modules_are_not_flow_modules() {
        assert!(is_flow_module("/app/main.js"));
        assert!(is_flow_module("/app/main.js?v=1"));
        assert!(!is_flow_module("\0vite/client"));
        assert!(!is_flow_module("/app/node_modules/react/index.js"));
        assert!(is_flow_module(
            "/app/node_modules/@uniflowed/react/index.js"
        ));
        assert!(!is_flow_module("/app/styles.css"));
        assert!(!is_flow_module("/app/page.mdx"));
    }

    #[test]
    fn the_whole_pipeline_produces_a_module_with_a_map() {
        let source = "// @flow\nimport {useState} from 'react';\nenum Mode { On, Off }\nexport component Toggle(label: string) {\n  const [mode, setMode] = useState<Mode>(Mode.On);\n  const text = match (mode) { Mode.On => 'on', Mode.Off => 'off' };\n  return <button onClick={() => setMode(Mode.Off)}>{label}: {text}</button>;\n}\n";
        let options = TransformOptions {
            development: true,
            refresh: true,
            ..TransformOptions::new("Toggle.js")
        };
        let out = transform(source, &options).expect("transforms");
        assert!(out.code.contains("function Toggle"), "{}", out.code);
        assert!(out.code.contains("$$ufEnumMirrored"), "{}", out.code);
        assert!(out.code.contains("jsxDEV"), "{}", out.code);
        assert!(out.code.contains("$RefreshReg$"), "{}", out.code);
        assert!(out.code.contains("react/compiler-runtime"), "{}", out.code);
        assert!(!out.code.contains("match ("), "{}", out.code);
        assert_eq!(out.compiled_functions, 1, "{:?}", out.compiler_diagnostics);
        assert!(out.map.is_some());
    }

    #[test]
    fn a_syntax_error_names_its_position() {
        let error = transform("const a = ;\n", &TransformOptions::new("x.js")).unwrap_err();
        assert!(
            matches!(error, TransformError::Syntax { line: 1, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn the_compiler_can_be_turned_off() {
        let source = "export component A() { return <p />; }\n";
        let options = TransformOptions {
            react_compiler: ReactCompilerMode::Off,
            ..TransformOptions::new("a.js")
        };
        let out = transform(source, &options).unwrap();
        assert!(!out.code.contains("compiler-runtime"));
        assert_eq!(out.compiled_functions, 0);
    }
}
