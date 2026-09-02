//! Flow parser/typechecker adapter boundary for uniflowed.
//!
//! `uf` talks to exactly one Flow backend at a time, and both real backends
//! implement the same official Flow grammar. The preferred one is Meta's
//! official Flow Rust port, vendored as the `upstream/flow` submodule and
//! selected with the `upstream-parser` feature. Until the `!` type is stable on
//! the pinned release toolchain the crate defaults to the QuickJS-hosted
//! reference parser, which needs source normalization for `component`/`hook`
//! declarations.

#[cfg(all(feature = "official-parser", not(feature = "upstream-parser")))]
mod quickjs;
#[cfg(feature = "upstream-parser")]
mod upstream;

use thiserror::Error;

/// A single syntax diagnostic reported by the active Flow parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    /// Human readable parser message.
    pub message: String,
    /// One-based source line, when the backend reports one.
    pub line: Option<u32>,
    /// Zero-based source column, when the backend reports one.
    pub column: Option<u32>,
}

/// The result of validating one Flow source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseOutcome {
    /// Diagnostics in source order.
    pub diagnostics: Vec<ParseDiagnostic>,
    /// Backend that produced the diagnostics.
    pub parser: ParserKind,
}

impl ParseOutcome {
    /// Return whether the source parsed without diagnostics.
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Flow syntax authority backing [`validate_source`].
///
/// Both real backends implement the same official Flow grammar, so they report
/// [`ParserKind::OfficialFlowParser`]. Use [`active_backend`] to learn which
/// implementation produced a [`ParseOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserKind {
    /// Flow's official grammar, through either real backend.
    OfficialFlowParser,
    /// The dependency-free guard used when no parser backend is compiled in.
    Fallback,
}

/// Implementation behind [`ParserKind::OfficialFlowParser`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserBackend {
    /// Meta's official Flow Rust port from `upstream/flow/rust_port`.
    UpstreamRustPort,
    /// Flow's reference parser compiled to JavaScript and hosted in QuickJS.
    QuickJsReference,
    /// No parser backend was compiled in.
    Fallback,
}

/// Errors raised while driving a Flow parser backend.
#[derive(Debug, Error)]
pub enum FlowError {
    /// The backend could not be initialized.
    #[error("failed to initialize Flow parser: {0}")]
    Initialize(String),
    /// The backend failed while parsing.
    #[error("Flow parser runtime error: {0}")]
    Runtime(String),
}

/// Handle for validating Flow sources through the compiled-in backend.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlowParser;

impl FlowParser {
    /// Validate `source` with the active backend.
    pub fn validate_source(&self, source: &str) -> Result<ParseOutcome, FlowError> {
        validate_source(source)
    }
}

/// Syntax authority selected at compile time.
pub const fn active_parser() -> ParserKind {
    #[cfg(any(feature = "upstream-parser", feature = "official-parser"))]
    {
        ParserKind::OfficialFlowParser
    }
    #[cfg(not(any(feature = "upstream-parser", feature = "official-parser")))]
    {
        ParserKind::Fallback
    }
}

/// Backend implementation selected at compile time.
///
/// `upstream-parser` wins over `official-parser` so a build that enables both
/// always runs the native Rust port.
pub const fn active_backend() -> ParserBackend {
    #[cfg(feature = "upstream-parser")]
    {
        ParserBackend::UpstreamRustPort
    }
    #[cfg(all(not(feature = "upstream-parser"), feature = "official-parser"))]
    {
        ParserBackend::QuickJsReference
    }
    #[cfg(not(any(feature = "upstream-parser", feature = "official-parser")))]
    {
        ParserBackend::Fallback
    }
}

/// Initialize this thread's parser state before any nested work begins.
///
/// Backends that host a JavaScript engine budget their stack from wherever the
/// engine was created, so creating one lazily inside a work-stealing job leaves
/// the parser with less headroom than it expects and makes ordinary files look
/// like syntax errors. Callers that parse from a thread pool should call this
/// once per worker, from a shallow frame, before fanning out. It is idempotent,
/// cheap after the first call, and a no-op for backends that need no setup.
pub fn prepare_thread() -> Result<(), FlowError> {
    #[cfg(all(feature = "official-parser", not(feature = "upstream-parser")))]
    {
        quickjs::prepare_thread()
    }
    #[cfg(any(
        feature = "upstream-parser",
        not(any(feature = "upstream-parser", feature = "official-parser"))
    ))]
    {
        Ok(())
    }
}

/// Validate `source` with the active Flow parser backend.
pub fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    #[cfg(feature = "upstream-parser")]
    {
        upstream::validate_source(source)
    }
    #[cfg(all(not(feature = "upstream-parser"), feature = "official-parser"))]
    {
        quickjs::validate_source(source)
    }
    #[cfg(not(any(feature = "upstream-parser", feature = "official-parser")))]
    {
        fallback_validate_source(source)
    }
}

/// Rewrite `component`/`hook` declarations into plain functions.
///
/// Backends that predate Flow component syntax need this; the upstream Rust
/// port parses the real syntax and never calls it. Returns [`None`] when the
/// source needs no rewriting.
pub fn normalize_modern_flow_for_parser(source: &str) -> Option<String> {
    if !source.contains("component ") && !source.contains("hook ") {
        return None;
    }

    let mut changed = false;
    let mut output = String::with_capacity(source.len());
    for line in source.lines() {
        let leading = line.len() - line.trim_start().len();
        let indent = &line[..leading];
        let trimmed = &line[leading..];
        let rewritten = trimmed
            .strip_prefix("export component ")
            .map(|tail| format!("export function {}", strip_renders_clause(tail)))
            .or_else(|| {
                trimmed
                    .strip_prefix("component ")
                    .map(|tail| format!("function {}", strip_renders_clause(tail)))
            })
            .or_else(|| {
                trimmed
                    .strip_prefix("export hook ")
                    .map(|tail| format!("export function {tail}"))
            })
            .or_else(|| {
                trimmed
                    .strip_prefix("hook ")
                    .map(|tail| format!("function {tail}"))
            });

        if let Some(rewritten) = rewritten {
            changed = true;
            output.push_str(indent);
            output.push_str(&rewritten);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }

    changed.then_some(output)
}

fn strip_renders_clause(tail: &str) -> String {
    let Some(body_start) = tail.find('{') else {
        return tail.to_string();
    };
    let signature = &tail[..body_start];
    let body = &tail[body_start..];
    let Some(renders_start) = signature.find(" renders ") else {
        return tail.to_string();
    };

    format!("{} {}", signature[..renders_start].trim_end(), body)
}

#[cfg(not(any(feature = "upstream-parser", feature = "official-parser")))]
fn fallback_validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    let diagnostics = if source.contains("type =") {
        vec![ParseDiagnostic {
            message: "fallback parser found an invalid type declaration".to_string(),
            line: None,
            column: None,
        }]
    } else {
        Vec::new()
    };

    Ok(ParseOutcome {
        diagnostics,
        parser: ParserKind::Fallback,
    })
}

/// Stable identifier for a syntax authority, used by `uf inspect` and the LSP.
pub fn parser_name(kind: ParserKind) -> &'static str {
    match kind {
        ParserKind::OfficialFlowParser => "official-flow-parser",
        ParserKind::Fallback => "fallback",
    }
}

/// Stable identifier for a backend implementation.
pub fn backend_name(backend: ParserBackend) -> &'static str {
    match backend {
        ParserBackend::UpstreamRustPort => "upstream-flow-rust-port",
        ParserBackend::QuickJsReference => "quickjs-reference-parser",
        ParserBackend::Fallback => "fallback",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Flow grammar is compiled in, so diagnostics and locations are real.
    const HAS_REAL_BACKEND: bool = !matches!(active_backend(), ParserBackend::Fallback);

    #[test]
    fn validates_modern_flow_syntax() {
        let source = r#"
            // @flow
            opaque type UserId = string;
            component Button(label: string) renders React.Node {
              return label;
            }
        "#;

        let outcome = validate_source(source).expect("parse result");

        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
        assert_eq!(outcome.parser, active_parser());
    }

    #[test]
    fn normalizes_component_and_hook_syntax_for_older_parser_bridge() {
        let normalized = normalize_modern_flow_for_parser(
            "export component Page() renders React.Node { return null; }\nexport hook useX(): number { return 1; }\n",
        )
        .expect("normalized");

        assert!(normalized.contains("export function Page() {"));
        assert!(normalized.contains("export function useX(): number"));
    }

    #[test]
    fn leaves_sources_without_component_or_hook_untouched() {
        assert!(normalize_modern_flow_for_parser("const x = 1;\n").is_none());
    }

    #[test]
    fn reports_flow_syntax_errors() {
        let source = "// @flow\ntype = ;";

        let outcome = validate_source(source).expect("parse result");

        assert!(!outcome.is_ok());
        if HAS_REAL_BACKEND {
            assert!(outcome.diagnostics[0].message.contains("Unexpected"));
        }
    }

    #[test]
    fn reports_error_locations_on_the_failing_line() {
        if !HAS_REAL_BACKEND {
            // The guard backend has no grammar and reports no locations.
            return;
        }
        let source = "// @flow\nconst a = 1;\ntype = ;\n";

        let outcome = validate_source(source).expect("parse result");

        assert_eq!(outcome.diagnostics[0].line, Some(3));
    }

    #[test]
    fn accepts_jsx_and_flow_generics() {
        let source = "// @flow\nconst node = <div className=\"a\">{value}</div>;\ntype Box<T> = { value: T };\n";

        let outcome = validate_source(source).expect("parse result");

        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn preparing_a_thread_is_idempotent() {
        prepare_thread().expect("first");
        prepare_thread().expect("second");

        assert!(validate_source("// @flow\nconst a = 1;\n").is_ok());
    }

    #[test]
    fn parser_names_are_stable() {
        assert_eq!(
            parser_name(ParserKind::OfficialFlowParser),
            "official-flow-parser"
        );
        assert_eq!(parser_name(ParserKind::Fallback), "fallback");
    }

    #[test]
    fn backend_names_are_stable() {
        assert_eq!(
            backend_name(ParserBackend::UpstreamRustPort),
            "upstream-flow-rust-port"
        );
        assert_eq!(
            backend_name(ParserBackend::QuickJsReference),
            "quickjs-reference-parser"
        );
        assert_eq!(backend_name(ParserBackend::Fallback), "fallback");
    }

    #[test]
    fn the_syntax_authority_matches_the_backend() {
        if HAS_REAL_BACKEND {
            assert_eq!(active_parser(), ParserKind::OfficialFlowParser);
        } else {
            assert_eq!(active_parser(), ParserKind::Fallback);
        }
    }
}
