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
pub(crate) mod owner;

pub(crate) use client_api::{client_api_uses_from_tokens, hook_calls_from_tokens};
pub(crate) use exports::exports_from_tokens;
pub(crate) use imports::imports_from_tokens;
pub use lexer::{Token, TokenKind, matching_close, matching_open, starts_statement, tokenize};
pub(crate) use owner::owner_spans;

/// Inline list of import specifiers belonging to one module.
pub type ImportList = InlineVec<ImportSpecifier, 8>;

/// Inline list of the bindings one import clause introduces.
pub type ImportBindingList = InlineVec<ImportBinding, 4>;

/// Inline list of exported bindings belonging to one module.
pub type ExportList = InlineVec<ModuleExport, 8>;

/// Inline list of client-only API uses found in one module.
pub type ClientApiUseList = InlineVec<ClientApiUse, 4>;

/// Inline list of calls to hooks the name lists do not know.
pub type HookCallList = InlineVec<HookCall, 4>;

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

/// What an import specifier names in the module it points at.
///
/// Three shapes rather than one string, because `*` is not a name a module can
/// export and `default` is: `import { default as x }` and `import x` bind the
/// same export and are the same variant here, while a namespace import binds
/// the module itself and has no exported name behind it at all.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportedName {
    /// The default export: `import x from "m"`, `import { default as x } from "m"`.
    Default,
    /// The module itself: `import * as x from "m"`.
    Namespace,
    /// A named export: `import { imported as local } from "m"`.
    Named(CompactString),
}

impl ImportedName {
    /// The exported name behind the binding, when there is one.
    ///
    /// [`None`] for a namespace import, which binds no single export — a
    /// caller asking "which export of `@uniflowed/hooks` is this" has to get
    /// nothing rather than a name it can compare against a table.
    pub fn as_export_name(&self) -> Option<&str> {
        match self {
            Self::Default => Some("default"),
            Self::Namespace => None,
            Self::Named(name) => Some(name.as_str()),
        }
    }
}

/// One `{ imported, local }` pair of an import clause.
///
/// The pair rather than either half alone. Collapsing them loses the aliased
/// import — `import { useState as useS } from "react"` binds `useS` in this
/// module and names `useState` in React's — so a scanner that keeps only the
/// local name cannot look a hook up in a package's table, and one that keeps
/// only the imported name cannot connect a call site to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBinding {
    /// The name in the module the specifier points at.
    pub imported: ImportedName,
    /// The name this module binds it under.
    ///
    /// A module-scope binding for every [`ImportKind`] but
    /// [`ImportKind::ReExport`], where `export { a as b } from "m"` introduces
    /// no local binding at all and `b` is the name this module *exports* it
    /// as. Both answer the same question — which name here is that name there
    /// — so both are recorded, and the kind says which reading applies.
    pub local: CompactString,
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
    /// The names the clause binds, in source order.
    ///
    /// Empty when the form binds nothing the scanner can name: a bare
    /// `import "m"`, an `export * from "m"`, a dynamic `import("m")` or a
    /// `require("m")` — the last two because what they bind is decided by the
    /// expression around them and not by a clause. Flow's inline type
    /// specifiers are dropped with the rest of the types: `import { type T, a }`
    /// binds only `a` at runtime.
    pub bindings: ImportBindingList,
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
    /// Local declaration name when it differs from the exported name.
    ///
    /// `export default function Page()` exports `default`, but the body owner
    /// recorded for hooks and client APIs is `Page`. `export { useTheme as
    /// useAppTheme }` has the same split. Consumers that match a body back to an
    /// export must use this when it is present, while import resolution must keep
    /// using [`name`](Self::name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<CompactString>,
    /// Shape of the exported binding.
    pub kind: ExportKind,
    /// 1-based line the export was declared on.
    pub line: u32,
}

/// One call to a hook the client-only name lists do not know.
///
/// Not a violation and not evidence of one: it is the record of a question the
/// scanner asked and could not answer, because `useRoute` is client-only if
/// and only if something in its body is, and its body is in another module.
/// The alternative to recording it is a graph that silently reports nothing
/// for every hook anybody wrote — see ubugeeei-prod/uf#348.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCall {
    /// The name as written.
    pub name: CompactString,
    /// 1-based line of the call.
    pub line: u32,
    /// 1-based column of the call.
    pub column: u32,
    /// The module-level declaration whose body the call sits in.
    ///
    /// Same rule and same caveats as [`ClientApiUse::owner`]. It is the second
    /// half of the fixpoint's edge set: a wrapper is client-only when it
    /// reaches a client-only API *or* calls a hook that does, and without an
    /// owner here the second clause has no left-hand side.
    pub owner: Option<CompactString>,
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
    /// The module-level declaration whose body the use sits in.
    ///
    /// A line and a column say where a `useState` is; this says whose it is,
    /// which is what turns "this module reaches a client-only API" into "this
    /// *export* does" — the edge an export-graph fixpoint propagates along.
    /// See ubugeeei-prod/uf#388.
    ///
    /// [`None`] for a use no module-level body contains: module top-level
    /// code, a method of a class or an object literal, and an anonymous body.
    /// Not a fallback to the nearest enclosing name — an owner that might be
    /// wrong is worse than none, because the fixpoint would propagate it.
    pub owner: Option<CompactString>,
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
    let owners = owner_spans(source, &tokens);
    client_api_uses_from_tokens(source, &tokens, &index, &owners)
}

/// Collect calls to hooks that [`CLIENT_ONLY_APIS`] does not name.
pub fn scan_hook_calls(source: &str) -> HookCallList {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    let owners = owner_spans(source, &tokens);
    hook_calls_from_tokens(source, &tokens, &index, &owners)
}

/// Clamp a `usize` position into the `u32` used by diagnostics.
pub(crate) fn clamp_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
