//! What one module imports, what it exports, and which bytes say so.
//!
//! The RSC analysis already answers "which modules does this reach" from
//! [`uf_rsc::scan`], and the graph uses it for exactly that. Emission needs one
//! thing more: the *byte span* of every import and export statement, and the
//! bindings each one introduces, because a chunk is many modules in one file
//! and their `import` statements cannot survive into a function body.
//!
//! So this module reads the same token stream — [`uf_flow::scan`], the one
//! scanner uf has for its own syntax — and produces a record with spans.
//! Rewriting is always a *blank*, never a delete: an erased statement is
//! overwritten with spaces and keeps its line terminators, so a module's line
//! *n* is still line *n* afterwards and the chunk's source map stays a
//! per-line table.

use compact_str::CompactString;
use smallvec::SmallVec;
use uf_flow::scan::{Token, TokenKind, starts_statement, tokenize};

mod parse;

#[cfg(test)]
mod tests;

use parse::{read_dynamic, read_export, read_import};

pub use parse::DEFAULT_LOCAL;

/// How a module named another one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImportForm {
    /// `import … from "…"`, or a bare `import "…"`.
    Static,
    /// `export … from "…"`.
    ReExport,
    /// `import("…")`, left in place for the runtime to resolve.
    Dynamic,
    /// `require("…")`, left in place for the runtime to resolve.
    Require,
}

impl ImportForm {
    /// Whether the bundler rewrites the statement that produced this import.
    ///
    /// Static forms are linked at build time; dynamic ones are expressions that
    /// stay in the emitted code exactly as the author wrote them.
    #[must_use]
    pub const fn is_linked(self) -> bool {
        matches!(self, Self::Static | Self::ReExport)
    }
}

/// One binding an import statement introduces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportBinding {
    /// `import local from "…"`.
    Default {
        /// The local name.
        local: CompactString,
    },
    /// `import { imported as local } from "…"`.
    Named {
        /// The name the other module exports.
        imported: CompactString,
        /// The local name.
        local: CompactString,
    },
    /// `import * as local from "…"`.
    Namespace {
        /// The local name.
        local: CompactString,
    },
}

impl ImportBinding {
    /// The local name the binding introduces.
    #[must_use]
    pub fn local(&self) -> &str {
        match self {
            Self::Default { local } | Self::Namespace { local } => local,
            Self::Named { local, .. } => local,
        }
    }
}

/// One import statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    /// The specifier exactly as written, without quotes.
    pub specifier: CompactString,
    /// The syntactic form it appeared in.
    pub form: ImportForm,
    /// The bindings it introduces. Empty for `import "…"` and `export * from`.
    pub bindings: SmallVec<[ImportBinding; 4]>,
}

/// Where an exported name gets its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportSource {
    /// A binding declared in this module.
    Local {
        /// The local name.
        local: CompactString,
    },
    /// A name taken straight from another module.
    Reexport {
        /// Index into [`ModuleRecord::imports`].
        import: usize,
        /// The name that module exports.
        imported: CompactString,
    },
}

/// One exported name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRecord {
    /// The name importers use; `default` for a default export.
    pub exported: CompactString,
    /// Where its value comes from.
    pub source: ExportSource,
}

/// What replaces a rewritten span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchText {
    /// Overwrite with spaces, keeping line terminators.
    Blank,
    /// Overwrite with fixed text that holds no line terminator.
    Text(&'static str),
}

/// One rewrite the emitter applies to a module's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patch {
    /// Byte offset of the first byte replaced.
    pub start: usize,
    /// Byte offset one past the last byte replaced.
    pub end: usize,
    /// What goes in its place.
    pub text: PatchText,
}

/// Whether a module does anything when it is merely evaluated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SideEffectKind {
    /// Every top-level statement only declares.
    #[default]
    None,
    /// A top-level statement runs code, so the module cannot be dropped.
    Present,
}

/// Everything the emitter needs to know about one module's syntax.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleRecord {
    /// Import statements, in source order.
    pub imports: Vec<ImportRecord>,
    /// Exported names, in source order.
    pub exports: Vec<ExportRecord>,
    /// Indices into [`Self::imports`] of `export * from "…"` statements.
    pub star_reexports: Vec<usize>,
    /// Rewrites to apply before the module goes into a chunk, ordered.
    pub patches: Vec<Patch>,
    /// Whether evaluating the module does anything.
    pub side_effects: SideEffectKind,
}

impl ModuleRecord {
    /// Whether the module exports `name`.
    #[must_use]
    pub fn exports_name(&self, name: &str) -> bool {
        self.exports.iter().any(|export| export.exported == name)
    }
}

/// Read one module's imports, exports and rewrite spans.
#[must_use]
pub fn scan_module(source: &str) -> ModuleRecord {
    let tokens = tokenize(source);
    let mut record = ModuleRecord::default();

    let mut at = 0usize;
    while at < tokens.len() {
        let next = if tokens[at].kind == TokenKind::Ident {
            match tokens[at].text(source) {
                "import" if starts_statement(&tokens, at) => {
                    read_import(source, &tokens, at, &mut record)
                }
                "import" => read_dynamic(source, &tokens, at, ImportForm::Dynamic, &mut record),
                "require" => read_dynamic(source, &tokens, at, ImportForm::Require, &mut record),
                "export" if starts_statement(&tokens, at) => {
                    read_export(source, &tokens, at, &mut record)
                }
                _ => None,
            }
        } else {
            None
        };
        at = next.unwrap_or(at + 1);
    }

    record.patches.sort_by_key(|patch| (patch.start, patch.end));
    record.side_effects = side_effects(source, &tokens, &record.patches);
    record
}

/// Whether any top-level statement does something other than declare.
///
/// Tokens inside an import or export statement are skipped: the `}` of an
/// `import { a } from "…"` clause makes the `from` after it look like the start
/// of a statement to a lexer, and treating that as a call would mark every
/// module with a named import as unshakeable.
fn side_effects(source: &str, tokens: &[Token], patches: &[Patch]) -> SideEffectKind {
    let mut depth: usize = 0;
    let mut patch = 0usize;

    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Punct(b'{' | b'(' | b'[') => {
                depth += 1;
                continue;
            }
            TokenKind::Punct(b'}' | b')' | b']') => {
                depth = depth.saturating_sub(1);
                continue;
            }
            _ => {}
        }
        while patch < patches.len() && patches[patch].end <= token.start {
            patch += 1;
        }
        if patches
            .get(patch)
            .is_some_and(|span| span.start <= token.start && token.start < span.end)
        {
            continue;
        }
        if depth > 0 || !starts_statement(tokens, index) {
            continue;
        }
        match token.kind {
            // A directive prologue, and nothing else, may be a bare string.
            TokenKind::String => {}
            TokenKind::Punct(b';') => {}
            TokenKind::Ident
                if matches!(
                    token.text(source),
                    "import" | "export" | "const" | "let" | "var" | "function" | "class" | "async"
                ) => {}
            _ => return SideEffectKind::Present,
        }
    }

    SideEffectKind::None
}
