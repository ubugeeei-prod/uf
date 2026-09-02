//! Bridge to the official Rust implementation shipped from `facebook/react`.
//!
//! The upstream compiler entrypoint accepts Babel's `File` AST plus Babel scope
//! information. uf keeps source-text syntax validation for lint/reporting, and
//! exposes this bridge for bundler integrations that can hand us the official
//! Babel-shaped payload.

use react_compiler::entrypoint::{CompileResult, PluginOptions, compile_program};
use react_compiler_ast::File;
use react_compiler_ast::common::from_json_str_unbounded;
use react_compiler_ast::scope::ScopeInfo;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The official upstream implementation uf is wired to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialReactCompilerCrate {
    /// Cargo package name.
    pub name: &'static str,
    /// Cargo package version compiled into uf.
    pub version: &'static str,
    /// Upstream source repository.
    pub repository: &'static str,
}

/// Return the exact official compiler crate built into this uf binary.
pub const fn official_compiler_crate() -> OfficialReactCompilerCrate {
    OfficialReactCompilerCrate {
        name: "react_compiler",
        version: "0.1.0",
        repository: "https://github.com/facebook/react",
    }
}

/// JSON-serializable result returned by the official compiler bridge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficialCompileOutput {
    /// The official compiler result, serialized in its own JS bridge shape.
    pub result: serde_json::Value,
}

/// Errors raised before or after entering the official compiler.
#[derive(Debug, Error)]
pub enum OfficialReactCompilerError {
    /// Babel AST JSON did not match the official compiler AST schema.
    #[error("failed to decode Babel File AST for the official React Compiler: {0}")]
    File(serde_json::Error),
    /// ScopeInfo JSON did not match the official compiler scope schema.
    #[error("failed to decode Babel scope info for the official React Compiler: {0}")]
    Scope(serde_json::Error),
    /// Plugin options JSON did not match the official compiler option schema.
    #[error("failed to decode official React Compiler options: {0}")]
    Options(serde_json::Error),
    /// The official compiler result could not be serialized.
    #[error("failed to encode official React Compiler result: {0}")]
    Result(serde_json::Error),
}

/// Compile a Babel AST payload through the official React Compiler.
pub fn compile_babel_ast_json(
    file_json: &str,
    scope_json: &str,
    options_json: &str,
) -> Result<OfficialCompileOutput, OfficialReactCompilerError> {
    let file: File =
        from_json_str_unbounded(file_json).map_err(OfficialReactCompilerError::File)?;
    let scope: ScopeInfo =
        from_json_str_unbounded(scope_json).map_err(OfficialReactCompilerError::Scope)?;
    let options: PluginOptions =
        serde_json::from_str(options_json).map_err(OfficialReactCompilerError::Options)?;
    compile_babel_ast(file, scope, options)
}

/// Compile typed Babel AST structures through the official React Compiler.
pub fn compile_babel_ast(
    file: File,
    scope: ScopeInfo,
    options: PluginOptions,
) -> Result<OfficialCompileOutput, OfficialReactCompilerError> {
    let result: CompileResult = compile_program(file, scope, options);
    let result = serde_json::to_value(result).map_err(OfficialReactCompilerError::Result)?;
    Ok(OfficialCompileOutput { result })
}
