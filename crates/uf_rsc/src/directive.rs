//! Directive detection for React Server Components.
//!
//! Every module in a `uf` app runs on the server unless it opts out. A module
//! joins the client bundle with a file-level `"use client"` directive, and a
//! module exposes server actions with a file-level `"use server"` directive.
//!
//! # What counts as a directive
//!
//! The rules follow the ECMAScript *directive prologue*, because that is what
//! React and the bundlers implement:
//!
//! * a UTF-8 BOM, a `#!` hashbang line, comments and blank lines may precede it;
//! * it must be a plain single- or double-quoted string literal;
//! * it must be terminated by `;`, by a line break (ASI), by `}`, or by the end
//!   of the file;
//! * the string content is compared to the *raw* source text, so
//!   `"use  client"` (two spaces) and `"use client"` are not directives —
//!   exactly as `"use strict"` behaves in the language;
//! * it may only appear in the prologue. `"use client"` further down the file,
//!   built by concatenation, or written as a template literal is rejected with a
//!   typed [`DirectiveIssue`] instead of being silently ignored.
//!
//! Silently ignoring one of those cases is how a module that its author believed
//! was a Client Component ends up rendered on the server (or the reverse), which
//! is the bug class this module exists to prevent.
//!
//! A `"use server"` directive at the top of a *function body* is a different,
//! supported construct: it marks that single closure as a server action rather
//! than changing the module environment. Those are collected separately in
//! [`DirectiveScan::function_directives`].

use std::fmt;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uf_infra::{InlineVec, LineIndex};

use crate::scan::{Token, TokenKind, clamp_u32, tokenize};

mod function;
mod prologue;

use function::scan_function_directives;
use prologue::{scan_misplaced_directives, scan_prologue};

/// Inline list of function-level directives found in one module.
pub type FunctionDirectiveList = InlineVec<FunctionDirective, 4>;

/// Inline list of directive issues found in one module.
pub type DirectiveIssueList = InlineVec<DirectiveIssue, 2>;

/// Which environment a module executes in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ModuleEnvironment {
    /// The default: a Server Component, never shipped to the browser.
    #[default]
    Server,
    /// A Client Component: a `"use client"` module and a client bundle root.
    Client,
    /// A `"use server"` module: every export is a server action.
    ServerActions,
}

impl ModuleEnvironment {
    /// Environment implied by a file-level directive, if any.
    pub fn from_file_directive(directive: Option<DirectiveKind>) -> Self {
        match directive {
            None => Self::Server,
            Some(DirectiveKind::UseClient) => Self::Client,
            Some(DirectiveKind::UseServer) => Self::ServerActions,
        }
    }

    /// Whether the module executes on the server.
    pub fn runs_on_server(self) -> bool {
        matches!(self, Self::Server | Self::ServerActions)
    }

    /// Whether the module is bundled for the browser.
    pub fn runs_on_client(self) -> bool {
        matches!(self, Self::Client)
    }
}

/// The two RSC directives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DirectiveKind {
    /// `"use client"`.
    UseClient,
    /// `"use server"`.
    UseServer,
}

impl DirectiveKind {
    /// Match the raw text between the quotes against the two directives.
    ///
    /// The comparison is byte-exact on the *source* text, which is what makes
    /// `"use  client"` and `"use client"` ordinary string statements.
    pub fn from_content(content: &str) -> Option<Self> {
        match content {
            "use client" => Some(Self::UseClient),
            "use server" => Some(Self::UseServer),
            _ => None,
        }
    }

    /// The directive text without quotes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UseClient => "use client",
            Self::UseServer => "use server",
        }
    }
}

impl fmt::Display for DirectiveKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The accepted file-level directive of a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDirective {
    /// Which directive was found.
    pub kind: DirectiveKind,
    /// 1-based line of the directive.
    pub line: u32,
    /// 1-based column of the directive.
    pub column: u32,
}

/// The function a function-level `"use server"` directive belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FunctionOwner {
    /// A binding the scanner could name.
    Named(CompactString),
    /// An inline closure, identified by its source order within the module.
    Anonymous {
        /// 0-based index among the anonymous inline actions of this module.
        ordinal: u32,
    },
}

impl fmt::Display for FunctionOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(name) => formatter.write_str(name),
            Self::Anonymous { ordinal } => write!(formatter, "<anonymous#{ordinal}>"),
        }
    }
}

/// A `"use server"` directive at the top of a function body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDirective {
    /// The function the directive applies to.
    pub owner: FunctionOwner,
    /// 1-based line of the directive.
    pub line: u32,
    /// 1-based column of the directive.
    pub column: u32,
}

/// A directive-shaped construct that is not a valid directive.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "issue")]
pub enum DirectiveIssue {
    /// A `"use client"` or `"use server"` string outside the module prologue.
    #[error("`{kind}` at line {line}:{column} is not the first statement of the module")]
    NotInPrologue {
        /// Which directive was written.
        kind: DirectiveKind,
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
    },
    /// A directive built from a template literal or a concatenation.
    #[error("`{kind}` at line {line}:{column} must be a plain string literal")]
    NotAStringLiteral {
        /// Which directive was intended.
        kind: DirectiveKind,
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
    },
    /// A module declaring both `"use client"` and `"use server"`.
    #[error("conflicting `use client` and `use server` directives at line {line}:{column}")]
    Conflicting {
        /// 1-based line of the second directive.
        line: u32,
        /// 1-based column of the second directive.
        column: u32,
    },
    /// A `"use client"` directive inside a function body, which React does not support.
    #[error("`use client` at line {line}:{column} is only valid at the top of a module")]
    ClientDirectiveInFunction {
        /// 1-based line.
        line: u32,
        /// 1-based column.
        column: u32,
    },
}

impl DirectiveIssue {
    /// Stable rule identifier for reporting.
    pub fn rule(&self) -> &'static str {
        match self {
            Self::NotInPrologue { .. } => "rsc/directive-not-in-prologue",
            Self::NotAStringLiteral { .. } => "rsc/directive-not-a-string-literal",
            Self::Conflicting { .. } => "rsc/conflicting-directives",
            Self::ClientDirectiveInFunction { .. } => "rsc/client-directive-in-function",
        }
    }

    /// 1-based line the issue was found on.
    pub fn line(&self) -> u32 {
        match self {
            Self::NotInPrologue { line, .. }
            | Self::NotAStringLiteral { line, .. }
            | Self::Conflicting { line, .. }
            | Self::ClientDirectiveInFunction { line, .. } => *line,
        }
    }
}

/// Everything the directive pass learned about one module.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirectiveScan {
    /// Environment the module executes in.
    pub environment: ModuleEnvironment,
    /// The accepted file-level directive, when the module has one.
    pub file_directive: Option<FileDirective>,
    /// Function-level `"use server"` closures, in source order.
    pub function_directives: FunctionDirectiveList,
    /// Rejected directive-shaped constructs.
    pub issues: DirectiveIssueList,
}

/// Classify a module by its file-level directive.
///
/// Modules without a valid file-level directive are [`ModuleEnvironment::Server`],
/// which is the `uf` default.
pub fn module_environment(source: &str) -> ModuleEnvironment {
    scan_directives(source).environment
}

/// Run the full directive pass over a module.
pub fn scan_directives(source: &str) -> DirectiveScan {
    let tokens = tokenize(source);
    let index = LineIndex::new(source);
    scan_directive_tokens(source, &tokens, &index)
}

pub(crate) fn scan_directive_tokens(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
) -> DirectiveScan {
    let mut scan = DirectiveScan::default();
    let mut consumed = vec![false; tokens.len()];

    let prologue_end = scan_prologue(source, tokens, index, &mut consumed, &mut scan);
    scan_function_directives(source, tokens, index, &mut consumed, &mut scan);
    scan_misplaced_directives(source, tokens, index, &consumed, prologue_end, &mut scan);

    scan.environment =
        ModuleEnvironment::from_file_directive(scan.file_directive.map(|found| found.kind));
    scan
}

fn line_column(index: &LineIndex, token: &Token) -> (u32, u32) {
    let position = index.line_col(token.start);
    (clamp_u32(position.line), clamp_u32(position.column))
}

/// Whether the statement starting at `position` ends there.
///
/// A directive ends at `;`, at a line break, at the `}` closing its block, or at
/// the end of the file. It does *not* end when the next token can continue the
/// expression, which is what makes `"use client" + ""` an ordinary statement.
fn terminates_statement(tokens: &[Token], position: usize) -> bool {
    let Some(next) = tokens.get(position + 1) else {
        return true;
    };
    if next.is_punct(b';') || next.is_punct(b'}') {
        return true;
    }
    if continues_expression(next) {
        return false;
    }
    next.newline_before
}

/// Whether a token can continue the expression started by the previous token.
///
/// Automatic semicolon insertion does not fire before these, so a line break in
/// front of one of them does not end the statement either.
fn continues_expression(token: &Token) -> bool {
    match token.kind {
        TokenKind::Arrow | TokenKind::Template => true,
        TokenKind::Punct(byte) => matches!(
            byte,
            b'+' | b'-'
                | b'*'
                | b'/'
                | b'%'
                | b','
                | b'.'
                | b'?'
                | b':'
                | b'='
                | b'('
                | b'['
                | b'<'
                | b'>'
                | b'&'
                | b'|'
                | b'^'
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
