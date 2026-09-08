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
pub mod memo;
pub mod print;
pub mod scope;

use serde_json::Value;
use thiserror::Error;

pub use crate::compiler::{CompilerDiagnostic, ReactCompilerMode};
pub use crate::estree::MAX_SOURCE_BYTES;
pub use crate::memo::{RedundantMemo, redundant_memoization};

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
    /// Whether `import.meta.uf.test` reaches uf's test API.
    ///
    /// True only for the hosts `uf test` starts. Everywhere else the marker is
    /// substituted with `void 0`, which is what makes an in-source test block
    /// something the bundler removes rather than something a production build
    /// has to be trusted not to run. See [`print::IN_SOURCE_TESTS_PRESENT`].
    ///
    /// This is the one option that makes `uf test` and the Vite plugin produce
    /// *different* modules from the same source, against the promise in
    /// [`emit`]'s header. Deliberately: the difference is the feature, and it
    /// is confined to one expression that has no meaning until a host answers
    /// for it. The two hosts still agree byte for byte on every module that
    /// does not write the marker, which is every module that is not a test.
    pub in_source_tests: bool,
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
            in_source_tests: false,
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
    // uf's own working directory: the bundled server, the deploy adapters, and
    // the asset module `uf build --compile` packs a site into are all output
    // uf wrote as plain JavaScript. Parsing them as Flow was waste at every
    // size and a failure at one — see `isFlowModule` in
    // `packages/host/transform.js`, which this mirrors, and #679.
    if path.starts_with(".uf/") || path.contains("/.uf/") {
        return false;
    }
    match path.rfind("/node_modules/") {
        Some(at) => path[at..].starts_with("/node_modules/@uniflowed/"),
        None => true,
    }
}

/// File extensions uf treats as Flow source.
pub const FLOW_EXTENSIONS: [&str; 4] = [".js", ".jsx", ".mjs", ".cjs"];

/// The module as the React Compiler sees it: parsed with the official Flow
/// parser, lowered, and rendered in Babel's AST shape.
///
/// Stages one to three of [`transform`], with the JavaScript left unprinted.
/// It is a public entry point because a caller can want the tree and nothing
/// else: `uf lint` asks the compiler what it did with a module's hand-written
/// memoization (see [`memo`]) and never emits a byte of code.
///
/// The [`lower::Lowered`] beside it carries the one fact the later stages need
/// and the tree does not say: whether the module declares anything the
/// compiler would compile in `syntax` mode.
///
/// # Errors
///
/// [`TransformError::SourceTooLarge`], [`TransformError::Syntax`] and
/// [`TransformError::Lowering`], exactly as [`transform`] raises them.
///
/// # Call this from a thread with `uf_flow::PARSE_STACK_BYTES` of stack
///
/// The Flow parser is recursive descent and freeing its tree recurses too;
/// [`uf_flow::parse`] says how much that costs and why the ceiling is where it
/// is.
pub fn babel_ast(source: &str) -> Result<(Value, lower::Lowered), TransformError> {
    let (program, lowered) = lowered_ast(source)?;
    let file = babel::to_babel(program, source)?;
    Ok((file, lowered))
}

/// [`babel_ast`] stopping one stage early: parsed and lowered, not converted.
///
/// The tree is ESTree as the Flow parser renders it, with Flow's own syntax
/// already desugared — a `component` is a function here, and a `match` is a
/// conditional — but with ESTree's names rather than Babel's: literals are
/// `Literal` and not `StringLiteral`, an object's entries are `Property` and
/// not `ObjectProperty`, a span is `range` and not `start`/`end`, and a `loc`
/// column counts code points where Babel's counts UTF-16 units.
///
/// It exists because the conversion is a third of the cost of `babel_ast` and
/// not every caller needs what it buys. `uf_lint`'s `react/no-derived-state-
/// effect` asks about effects, setters and the expressions between them, all
/// of which this tree already answers; only `react/no-redundant-memo` needs
/// Babel's shape, because the official React Compiler crate reads it. See
/// ubugeeei-prod/uf#668.
///
/// # Errors
///
/// [`TransformError::SourceTooLarge`], [`TransformError::Syntax`] and
/// [`TransformError::Lowering`], exactly as [`babel_ast`] raises them.
///
/// # Call this from a thread with `uf_flow::PARSE_STACK_BYTES` of stack
///
/// For the reason [`babel_ast`] gives.
pub fn lowered_ast(source: &str) -> Result<(Value, lower::Lowered), TransformError> {
    let mut program = estree::parse(source)?;
    let lowered = lower::lower(&mut program, source)?;
    Ok((program, lowered))
}

/// The rest of [`babel_ast`], for a caller that already has [`lowered_ast`].
///
/// # Errors
///
/// [`TransformError::Lowering`] when the tree does not fit Babel's schema.
pub fn babel_from_lowered(program: Value, source: &str) -> Result<Value, TransformError> {
    babel::to_babel(program, source)
}

/// Transform one Flow module to JavaScript.
///
/// # Errors
///
/// See [`TransformError`].
pub fn transform(source: &str, options: &TransformOptions) -> Result<Transformed, TransformError> {
    let (file, lowered) = babel_ast(source)?;

    let (file, compiler_diagnostics, compiled_functions) =
        if options.react_compiler == ReactCompilerMode::Off || !lowered.may_compile {
            (file, Vec::new(), 0)
        } else {
            let scope = scope::analyze(&file);
            compiler::compile(file, scope, source, options)?
        };

    let printed = print::print(
        &file,
        print::PrintOptions {
            in_source_tests: options.in_source_tests,
        },
    )?;
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

    /// uf's own working directory is output, not source.
    ///
    /// The rows are `tests/library/flow-modules.test.js`'s, because the two
    /// answers have to be one answer — `isFlowModule` there says so in its
    /// documentation and this function's job is to mirror it.
    ///
    /// `assets.js` is why it matters. `uf build --compile` writes the whole
    /// site into it as one base64 string literal, so it grows with the
    /// project, and past eight megabytes the Flow parser refused the file uf
    /// had written a moment earlier — `source is 8442536 bytes, over the
    /// 8388608 byte ceiling`, from a ceiling that exists to distrust input.
    /// See ubugeeei-prod/uf#679.
    #[test]
    fn ufs_own_build_directory_is_not_flow() {
        assert!(!is_flow_module("/p/.uf/build/compile/assets.js"));
        assert!(!is_flow_module("/p/.uf/build/compile/server.js"));
        assert!(!is_flow_module("/p/.uf/deploy/node/handler.js"));
        assert!(!is_flow_module(".uf/build/compile/assets.js"));
        assert!(!is_flow_module("/p/node_modules/@uniflowed/core/.uf/x.js"));

        // A directory whose name merely contains it, and a file merely named
        // for it, are ordinary source.
        assert!(is_flow_module("/p/.uf-notes/x.js"));
        assert!(is_flow_module("/p/my.uf/x.js"));
        assert!(is_flow_module("/p/app.uf.js"));
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
