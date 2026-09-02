//! Tests for the StyleX pass, one file per topic.

mod extract;
mod idempotency;
mod ordering;
mod plugin;
mod props;
mod rewrite;
mod robustness;
mod variables;

use crate::compile::{CompiledModule, compile_module};

/// Compile a module, failing the test with the error if it does not compile.
fn compile(source: &str) -> CompiledModule {
    compile_module(source)
        .unwrap_or_else(|error| panic!("expected the module to compile, got: {error}"))
}

/// A module that imports `stylex` and then contains `body`.
fn module(body: &str) -> String {
    format!("// @flow\nimport {{ stylex }} from \"@uniflowed/stylex\";\n{body}")
}
