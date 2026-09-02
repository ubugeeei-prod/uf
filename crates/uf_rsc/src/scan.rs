//! Single-pass token scanning for `.js` sources.
//!
//! The RSC analyses need four things out of a module: its directive prologue,
//! its import specifiers, its exported bindings, and whether a server module
//! reaches for a client-only API. All four are answered from one token vector
//! produced by [`tokenize`], so a module is scanned exactly once.
//!
//! The scanner is deliberately a lexer and not a parser: it understands string,
//! template, regular-expression and comment boundaries (so a `"use client"`
//! inside a comment or a string is never mistaken for a directive) but it does
//! not build an AST. Every place where that costs precision is documented at the
//! function that pays the cost.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use uf_infra::{InlineVec, LineIndex};

mod client_api;
mod exports;
mod imports;
pub mod lexer;

pub(crate) use client_api::client_api_uses_from_tokens;
pub(crate) use exports::exports_from_tokens;
pub(crate) use imports::imports_from_tokens;
pub use lexer::{Token, TokenKind, matching_close, matching_open, starts_statement, tokenize};

/// Inline list of import specifiers belonging to one module.
pub type ImportList = InlineVec<ImportSpecifier, 8>;

/// Inline list of exported bindings belonging to one module.
pub type ExportList = InlineVec<ModuleExport, 8>;

/// Inline list of client-only API uses found in one module.
pub type ClientApiUseList = InlineVec<ClientApiUse, 4>;

/// Longest source accepted by the scanner, in bytes.
///
/// Guards against unbounded allocation when a hostile or generated file is fed
/// to `uf build`; larger files are reported as unscannable rather than parsed.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// React APIs that only exist inside the client bundle.
///
/// A Server Component that calls one of these fails at render time, which is the
/// error class Next.js surfaces as "This React hook only works in a client
/// component". The list is sorted so lookups can binary search it.
pub const CLIENT_ONLY_APIS: &[&str] = &[
    "createContext",
    "useActionState",
    "useCallback",
    "useContext",
    "useDebugValue",
    "useDeferredValue",
    "useEffect",
    "useFormStatus",
    "useImperativeHandle",
    "useInsertionEffect",
    "useLayoutEffect",
    "useMemo",
    "useOptimistic",
    "useReducer",
    "useRef",
    "useState",
    "useSyncExternalStore",
    "useTransition",
];

/// Browser globals that only exist inside the client bundle.
///
/// Sorted for binary search.
pub const CLIENT_ONLY_GLOBALS: &[&str] = &[
    "alert",
    "document",
    "localStorage",
    "navigator",
    "sessionStorage",
    "window",
];

/// How a module reached another module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportKind {
    /// `import x from "..."` or a bare `import "..."`.
    Static,
    /// `export { x } from "..."` or `export * from "..."`.
    ReExport,
    /// `import("...")`.
    Dynamic,
    /// `require("...")`.
    Require,
}

/// One import specifier as written in the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSpecifier {
    /// The specifier text exactly as written, without quotes.
    pub specifier: CompactString,
    /// Syntactic form the specifier appeared in.
    pub kind: ImportKind,
    /// 1-based line the specifier appeared on.
    pub line: u32,
}

/// Shape of an exported binding, as far as a lexer can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportKind {
    /// `export async function f()`, `export const f = async () => {}`.
    AsyncFunction,
    /// `export function f()`, `export const f = () => {}`, `export hook useF()`.
    SyncFunction,
    /// `export class C {}`.
    Class,
    /// Any other initializer: a literal, an object, a call result.
    Value,
    /// `export { x } from "./other.js"`, where the shape lives in another module.
    ReExport,
}

impl ExportKind {
    /// Whether the export is callable at all.
    pub fn is_function(self) -> bool {
        matches!(self, Self::AsyncFunction | Self::SyncFunction)
    }
}

/// One exported binding of a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleExport {
    /// Exported name; `default` for a default export.
    pub name: CompactString,
    /// Shape of the exported binding.
    pub kind: ExportKind,
    /// 1-based line the export was declared on.
    pub line: u32,
}

/// One use of a client-only API inside a module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientApiUse {
    /// The API name, borrowed from [`CLIENT_ONLY_APIS`] or [`CLIENT_ONLY_GLOBALS`].
    pub api: &'static str,
    /// 1-based line of the use.
    pub line: u32,
    /// 1-based column of the use.
    pub column: u32,
}

/// Collect the import specifiers of a module.
pub fn scan_imports(source: &str) -> ImportList {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    imports_from_tokens(source, &tokens, &index)
}

/// Collect the exported bindings of a module.
pub fn scan_exports(source: &str) -> ExportList {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    exports_from_tokens(source, &tokens, &index)
}

/// Collect client-only API uses inside a module.
pub fn scan_client_api_uses(source: &str) -> ClientApiUseList {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    client_api_uses_from_tokens(source, &tokens, &index)
}

/// Clamp a `usize` position into the `u32` used by diagnostics.
pub(crate) fn clamp_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
