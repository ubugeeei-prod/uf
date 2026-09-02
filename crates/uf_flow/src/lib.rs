//! Flow syntax for uniflowed: the parser boundary, a token scanner, and type
//! erasure.
//!
//! The parser is Meta's official Flow Rust port, vendored as the
//! `upstream/flow` submodule. It is the only backend: a second one that spoke a
//! different dialect would make `uf lint` and `uf check` disagree with the
//! grammar uf documents, which is what happened while a QuickJS-hosted build of
//! Flow's JavaScript parser stood in for it on stable toolchains.
//!
//! Two things sit beside that boundary, and both are here rather than in the
//! crates that use them because this is the crate that owns Flow syntax:
//!
//! * [`scan`] is the byte-level token scanner — one scanner for uf source, used
//!   by the eraser below and by anything else that rewrites a module;
//! * [`strip`] erases Flow types, which is what turns a `// @flow` module into
//!   the JavaScript a browser runs.

pub mod scan;
pub mod strip;
mod upstream;

use thiserror::Error;

pub use strip::{MAX_STRIP_BYTES, StripError, Stripped, strip_types};

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
}

impl ParseOutcome {
    /// Return whether the source parsed without diagnostics.
    pub fn is_ok(&self) -> bool {
        self.diagnostics.is_empty()
    }
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

/// Handle for validating Flow sources.
#[derive(Debug, Default, Clone, Copy)]
pub struct FlowParser;

impl FlowParser {
    /// Validate `source` against the official Flow grammar.
    pub fn validate_source(&self, source: &str) -> Result<ParseOutcome, FlowError> {
        validate_source(source)
    }
}

/// Validate `source` against the official Flow grammar.
pub fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    upstream::validate_source(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

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
    }

    #[test]
    fn reports_flow_syntax_errors() {
        let source = "// @flow\ntype = ;";

        let outcome = validate_source(source).expect("parse result");

        assert!(!outcome.is_ok());
        assert!(outcome.diagnostics[0].message.contains("Unexpected"));
    }

    #[test]
    fn reports_error_locations_on_the_failing_line() {
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
    fn the_official_port_parses_syntax_the_quickjs_bridge_had_to_be_fed_rewritten() {
        // `component`/`hook` used to be rewritten into plain functions before
        // parsing, which moved every diagnostic in the file. The port reads them.
        let source = "// @flow\nexport component Page() renders React.Node {\n  return null;\n}\nexport hook useX(): number {\n  return 1;\n}\n";

        let outcome = validate_source(source).expect("parse result");

        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }

    #[test]
    fn the_official_port_accepts_modern_variance_and_bounds() {
        // `readonly` crashed the QuickJS bridge's AST deserialization, and it is
        // the only spelling Flow now accepts for a read-only property.
        for source in [
            "// @flow\nexport type A = {| readonly n: string |};\n",
            "// @flow\nexport type B<P extends string> = P;\n",
            "// @flow\nexport type C<out P> = (P) => void;\n",
            "// @flow\nexport type D<in P> = (P) => void;\n",
        ] {
            let outcome = validate_source(source).expect("parse result");
            assert!(outcome.is_ok(), "{source}: {:?}", outcome.diagnostics);
        }
    }

    #[test]
    fn validates_shipped_uniflowed_package_sources() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
        let mut modules = Vec::new();
        collect_js_modules(&root, &mut modules);
        modules.sort();

        for module in modules {
            let source = fs::read_to_string(&module).unwrap_or_else(|error| {
                panic!("failed to read {}: {error}", module.display());
            });
            let outcome = validate_source(&source).unwrap_or_else(|error| {
                panic!("failed to parse {}: {error}", module.display());
            });

            assert!(
                outcome.is_ok(),
                "{} must parse as Flow: {:?}",
                module.display(),
                outcome.diagnostics
            );
        }
    }

    fn collect_js_modules(path: &Path, modules: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(path).unwrap_or_else(|error| {
            panic!("failed to read {}: {error}", path.display());
        });
        for entry in entries {
            let entry = entry.unwrap_or_else(|error| {
                panic!("failed to read entry under {}: {error}", path.display());
            });
            let path = entry.path();
            let file_type = entry.file_type().unwrap_or_else(|error| {
                panic!("failed to stat {}: {error}", path.display());
            });
            if file_type.is_dir() {
                collect_js_modules(&path, modules);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("js") {
                modules.push(path);
            }
        }
    }
}
