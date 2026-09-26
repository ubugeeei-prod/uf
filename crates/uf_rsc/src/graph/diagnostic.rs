//! Typed violations of the React Server Components contract.
//!
//! Each way an app can break the contract is one [`RscDiagnostic`] variant
//! carrying the module, position and names a reporter needs, so a build
//! accumulates violations as data rather than as formatted strings, and
//! [`RscSeverity`] keeps the manifest spelling of how serious one is.

use std::fmt;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::directive::DirectiveIssue;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RscSeverity {
    /// Worth fixing, but the build can continue.
    Warn,
    /// Breaks the RSC contract.
    Error,
}

impl RscSeverity {
    /// Stable identifier used in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// Where a client-only hook verdict came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientOnlyHookOrigin {
    /// A published package with declared hook metadata.
    Package(CompactString),
    /// A project module whose exported hook body was classified by the graph.
    Module(Utf8PathBuf),
}

impl fmt::Display for ClientOnlyHookOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Package(package) => write!(formatter, "`{package}`"),
            Self::Module(module) => write!(formatter, "project module `{module}`"),
        }
    }
}

/// Why a route's document is written once, which is what makes reading the
/// request in its render wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaticRouteReason {
    /// The build prerenders it: its document is written at build time.
    Prerendered,
    /// Its render states a cache lifetime and the route cache is on, so its
    /// document is written into the cache and served to every request.
    Cached {
        /// The module that imports `cacheLife`.
        module: Utf8PathBuf,
        /// 1-based line of that import.
        line: u32,
    },
}

/// How a server-only import reached the client graph, as a message ends.
///
/// Nothing for a chain of one: that module is a client boundary itself, and the
/// message already calls it the client module. Otherwise the modules from the
/// boundary down to the importer, in the order somebody would follow the
/// imports to find where to cut — which is the half of the message a reader
/// cannot work out from the importing module alone. See ubugeeei-prod/uf#252.
fn chain_suffix(chain: &[Utf8PathBuf]) -> String {
    if chain.len() < 2 {
        return String::new();
    }
    uf_infra::into_string(uf_infra::cstr!(
        ", reached from a client boundary through {}",
        arrows(chain)
    ))
}

/// A chain of modules as the report prints it: each in backticks, `→` between.
fn arrows(chain: &[Utf8PathBuf]) -> String {
    let steps: Vec<String> = chain
        .iter()
        .map(|module| uf_infra::into_string(uf_infra::cstr!("`{module}`")))
        .collect();
    steps.join(" → ")
}

/// The message for [`RscDiagnostic::RequestStateInStaticRoute`].
///
/// Three things, in the order a reader needs them: which routes, and why their
/// documents are written once; what the render reads, and the chain of imports
/// that reaches it, since the import to cut is on that path; and what to do.
fn request_state_message(
    routes: &[CompactString],
    reason: &StaticRouteReason,
    api: &str,
    module: &Utf8Path,
    line: u32,
    chain: &[Utf8PathBuf],
) -> String {
    let named: Vec<String> = routes
        .iter()
        .map(|route| uf_infra::into_string(uf_infra::cstr!("`{route}`")))
        .collect();
    let (subject, their) = match named.as_slice() {
        [one] => (uf_infra::into_string(uf_infra::cstr!("route {one}")), "its"),
        _ => (
            uf_infra::into_string(uf_infra::cstr!("routes {}", named.join(", "))),
            "their",
        ),
    };
    let plural = routes.len() != 1;
    let reached = if chain.len() < 2 {
        uf_infra::cstr!(", which `{module}` imports from `@uniflowed/server` at line {line}")
            .into_string()
    } else {
        uf_infra::cstr!(
            " through {}, where `{module}` imports it from `@uniflowed/server` at line {line}",
            arrows(chain)
        )
        .into_string()
    };
    match reason {
        StaticRouteReason::Prerendered => uf_infra::cstr!(
            "{subject} {} prerendered, and {their} render reads `{api}()`{reached}. A prerendered \
             document is written once, at build time, for no request. Export `const dynamic = \
             \"force-dynamic\"` from {} to render it for each request, or move the read out of \
             the render",
            if plural { "are" } else { "is" },
            if plural { "each page" } else { "the page" },
        )
        .into_string(),
        StaticRouteReason::Cached {
            module: stated,
            line: stated_line,
        } => uf_infra::cstr!(
            "{subject} {} a cache lifetime through `cacheLife`, which `{stated}` imports at line \
             {stated_line}, and {their} render reads `{api}()`{reached}. A render that reads the \
             request is never stored, so that lifetime is never kept. Move the read out of the \
             render, or state no lifetime for {}",
            if plural { "state" } else { "states" },
            if plural { "these routes" } else { "this route" },
        )
        .into_string(),
    }
}

/// A violation of the React Server Components contract.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RscDiagnostic {
    /// A module in the client graph imports server-only code.
    #[error(
        "client module `{module}` imports server-only `{specifier}` at line {line}{}",
        chain_suffix(.chain)
    )]
    ServerOnlyImportInClientModule {
        /// Importing module, relative to the project root.
        module: Utf8PathBuf,
        /// The specifier as written.
        specifier: CompactString,
        /// 1-based line of the import.
        line: u32,
        /// The shortest chain of imports from a client boundary to `module`,
        /// both ends included. Empty until the graph is built, and a single
        /// module when `module` is the boundary.
        chain: Vec<Utf8PathBuf>,
    },
    /// A Server Component reaches for an API that only exists in the browser.
    #[error("server module `{module}` uses client-only `{api}` at line {line}:{column}")]
    ClientOnlyApiInServerModule {
        /// The server module.
        module: Utf8PathBuf,
        /// Name of the client-only API.
        api: &'static str,
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
    },
    /// A `"use server"` export that React cannot call.
    #[error("server action `{export}` in `{module}` must be an async function")]
    ServerActionNotAsync {
        /// The `"use server"` module.
        module: Utf8PathBuf,
        /// Exported name.
        export: CompactString,
        /// 1-based line of the export.
        line: u32,
    },
    /// A `"use server"` module exporting something that is not a function at all.
    #[error("`use server` module `{module}` exports non-function `{export}`")]
    ServerActionNotFunction {
        /// The `"use server"` module.
        module: Utf8PathBuf,
        /// Exported name.
        export: CompactString,
        /// 1-based line of the export.
        line: u32,
    },
    /// An import specifier that resolves outside the project root.
    #[error("module `{module}` imports `{specifier}` from outside the project root")]
    ImportEscapesProjectRoot {
        /// The importing module.
        module: Utf8PathBuf,
        /// The specifier as written.
        specifier: CompactString,
        /// 1-based line of the import.
        line: u32,
    },
    /// A module whose own path is not inside the project root.
    #[error("module path `{module}` is not inside the project root")]
    ModulePathOutsideProject {
        /// The offending path, as supplied.
        module: Utf8PathBuf,
    },
    /// A hook a package or project module exports that only runs in the browser.
    ///
    /// The half of [`Self::UnclassifiedHookInServerModule`] that can be
    /// decided. `useMediaQuery` is not on `CLIENT_ONLY_APIS` — it is not a
    /// React API — and it reads `matchMedia` through a `useSyncExternalStore`,
    /// so a Server Component calling it fails for exactly the reason
    /// `useState` does. The registry has said so for every hook
    /// `@uniflowed/hooks` exports since before `uf_rsc` existed; this is the
    /// rule that reads it. Project hooks use the same verdict once the
    /// export-level fixpoint can prove their bodies reach the same APIs.
    ///
    /// An error rather than a warning, because unlike its unclassified
    /// neighbour this *is* a verdict: the contract has no tolerances, and a
    /// module that breaks it does not work once the split lands.
    #[error(
        "server module `{module}` calls `{hook}` at line {line}:{column}, and `{package}` exports \
         it as a hook that only runs in the browser"
    )]
    ClientOnlyHookInServerModule {
        /// The server module.
        module: Utf8PathBuf,
        /// Name of the hook as called.
        hook: CompactString,
        /// The package or project module that exports it.
        package: ClientOnlyHookOrigin,
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
    },
    /// A hook the client-only check has no answer for.
    ///
    /// Not a violation: a statement that the analysis stopped. The check
    /// matches identifiers against two name lists, so `useState` in a Server
    /// Component is caught and `useRoute` — which is built on `useContext` —
    /// is not, and neither is any hook a user writes. Deciding it properly
    /// means binding an export to the APIs its body reaches, or reading the
    /// React Compiler's purity analysis, and both are larger than the rule
    /// they fix (ubugeeei-prod/uf#388). Until one of them exists the honest
    /// output is the question rather than silence: silence reads as "checked
    /// and fine", which is the one thing it is not.
    #[error(
        "server module `{module}` calls `{hook}` at line {line}:{column}, and the client-only \
         check cannot say whether it is a client hook: it matches names, and `{hook}` is not one \
         of them"
    )]
    UnclassifiedHookInServerModule {
        /// The server module.
        module: Utf8PathBuf,
        /// Name of the hook as called.
        hook: CompactString,
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
    },
    /// A route whose document is written once reaches a read of the request.
    ///
    /// Not found while building the graph, because the graph does not know
    /// which routes are prerendered or cached: `uf build` asks
    /// [`crate::RscGraph::request_state_read`] of each route it writes once,
    /// and reports what that finds as this. The run-time refusal —
    /// `x-uf-cache: BYPASS` for a cached render that read the request — stays
    /// behind it for what an import cannot show. See ubugeeei-prod/uf#996.
    #[error(
        "{}",
        request_state_message(.routes, .reason, .api, .module, *.line, .chain)
    )]
    RequestStateInStaticRoute {
        /// The routes, by URL pattern, whose render reaches the read, sorted.
        routes: Vec<CompactString>,
        /// Why their documents are written once.
        reason: StaticRouteReason,
        /// The request API imported: `cookies`, `headers` or `draftMode`.
        api: CompactString,
        /// The module that imports it, relative to the project root.
        module: Utf8PathBuf,
        /// 1-based line of the import.
        line: u32,
        /// From the module that renders the routes down to `module`, both
        /// included.
        chain: Vec<Utf8PathBuf>,
    },
    /// Shared function data must not depend on ambient request state.
    #[error(
        "cached function `{function}` reads `{api}()` in `{module}` at line {line}; read request data outside the cached function and pass its public inputs explicitly"
    )]
    RequestStateInCachedFunction {
        /// Application-supplied cache identity.
        function: CompactString,
        /// Request API being called.
        api: CompactString,
        /// Module containing the callback and read.
        module: Utf8PathBuf,
        /// 1-based source line.
        line: u32,
    },
    /// A rejected directive, lifted from the directive pass.
    #[error("in `{module}`: {issue}")]
    Directive {
        /// The module the directive was written in.
        module: Utf8PathBuf,
        /// What was wrong with it.
        issue: DirectiveIssue,
    },
}

impl RscDiagnostic {
    /// Stable rule identifier.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::ServerOnlyImportInClientModule { .. } => "rsc/server-only-import-in-client",
            Self::ClientOnlyApiInServerModule { .. } => "rsc/client-only-api-in-server",
            Self::ServerActionNotAsync { .. } => "rsc/server-action-not-async",
            Self::ServerActionNotFunction { .. } => "rsc/server-action-not-a-function",
            Self::ImportEscapesProjectRoot { .. } => "rsc/import-escapes-project-root",
            Self::ModulePathOutsideProject { .. } => "rsc/module-outside-project-root",
            Self::UnclassifiedHookInServerModule { .. } => "rsc/unclassified-hook-in-server",
            Self::ClientOnlyHookInServerModule { .. } => "rsc/client-only-hook-in-server",
            Self::RequestStateInStaticRoute { .. } => "rsc/request-state-in-static-route",
            Self::RequestStateInCachedFunction { .. } => "rsc/request-state-in-cached-function",
            Self::Directive { issue, .. } => issue.rule(),
        }
    }

    /// Severity of the diagnostic.
    ///
    /// Every RSC contract *violation* is an error: the contract has no
    /// tolerances, and a module that breaks it does not work once the split
    /// lands. [`Self::UnclassifiedHookInServerModule`] is the one variant that
    /// is not a violation — it says the analysis could not reach a verdict —
    /// so it is a warning, and it is the first thing in this crate that has
    /// ever been one.
    pub fn severity(&self) -> RscSeverity {
        match self {
            Self::UnclassifiedHookInServerModule { .. } => RscSeverity::Warn,
            _ => RscSeverity::Error,
        }
    }

    /// Module the diagnostic belongs to.
    pub fn module(&self) -> &Utf8Path {
        match self {
            Self::ServerOnlyImportInClientModule { module, .. }
            | Self::ClientOnlyApiInServerModule { module, .. }
            | Self::ServerActionNotAsync { module, .. }
            | Self::ServerActionNotFunction { module, .. }
            | Self::ImportEscapesProjectRoot { module, .. }
            | Self::UnclassifiedHookInServerModule { module, .. }
            | Self::ClientOnlyHookInServerModule { module, .. }
            | Self::RequestStateInStaticRoute { module, .. }
            | Self::RequestStateInCachedFunction { module, .. }
            | Self::ModulePathOutsideProject { module }
            | Self::Directive { module, .. } => module,
        }
    }

    /// 1-based line the diagnostic points at, when there is one.
    pub fn line(&self) -> u32 {
        match self {
            Self::ServerOnlyImportInClientModule { line, .. }
            | Self::ClientOnlyApiInServerModule { line, .. }
            | Self::ServerActionNotAsync { line, .. }
            | Self::ServerActionNotFunction { line, .. }
            | Self::ImportEscapesProjectRoot { line, .. }
            | Self::RequestStateInStaticRoute { line, .. }
            | Self::RequestStateInCachedFunction { line, .. }
            | Self::UnclassifiedHookInServerModule { line, .. } => *line,
            Self::ClientOnlyHookInServerModule { line, .. } => *line,
            Self::ModulePathOutsideProject { .. } => 0,
            Self::Directive { issue, .. } => issue.line(),
        }
    }

    /// 1-based column the diagnostic points at.
    ///
    /// The three checks that point at an *expression* record one; the rest
    /// answer with the first column, which is where a reporter's caret goes
    /// when the whole line is at fault. A reporter cannot ask "is there a
    /// column?" and do something sensible with `None`, so it is not offered
    /// one.
    ///
    /// [`Self::UnclassifiedHookInServerModule`] has carried a column since
    /// #348 and was not answering with it, so `uf build` printed a message
    /// saying `line 122:24` above a caret under column 1 — the number in the
    /// prose and the number under the code disagreeing about the same call.
    pub fn column(&self) -> u32 {
        match self {
            Self::ClientOnlyApiInServerModule { column, .. }
            | Self::ClientOnlyHookInServerModule { column, .. }
            | Self::UnclassifiedHookInServerModule { column, .. } => *column,
            _ => 1,
        }
    }
}
