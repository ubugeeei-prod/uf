//! Typed violations of the React Server Components contract.
//!
//! Each way an app can break the contract is one [`RscDiagnostic`] variant
//! carrying the module, position and names a reporter needs, so a build
//! accumulates violations as data rather than as formatted strings, and
//! [`RscSeverity`] keeps the manifest spelling of how serious one is.

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

/// A violation of the React Server Components contract.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RscDiagnostic {
    /// A module in the client graph imports server-only code.
    #[error("client module `{module}` imports server-only `{specifier}` at line {line}")]
    ServerOnlyImportInClientModule {
        /// Importing module, relative to the project root.
        module: Utf8PathBuf,
        /// The specifier as written.
        specifier: CompactString,
        /// 1-based line of the import.
        line: u32,
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
            Self::Directive { issue, .. } => issue.rule(),
        }
    }

    /// Severity of the diagnostic. Every RSC contract violation is an error.
    pub fn severity(&self) -> RscSeverity {
        RscSeverity::Error
    }

    /// Module the diagnostic belongs to.
    pub fn module(&self) -> &Utf8Path {
        match self {
            Self::ServerOnlyImportInClientModule { module, .. }
            | Self::ClientOnlyApiInServerModule { module, .. }
            | Self::ServerActionNotAsync { module, .. }
            | Self::ServerActionNotFunction { module, .. }
            | Self::ImportEscapesProjectRoot { module, .. }
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
            | Self::ImportEscapesProjectRoot { line, .. } => *line,
            Self::ModulePathOutsideProject { .. } => 0,
            Self::Directive { issue, .. } => issue.line(),
        }
    }
}
