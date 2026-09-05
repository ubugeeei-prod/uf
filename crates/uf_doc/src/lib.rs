#![deny(missing_docs)]
//! Flow API documentation extraction for `uf doc`.
//!
//! This crate deliberately uses uf's Flow parser instead of translating Flow
//! into another language first. Running `uf doc` is the opt-in boundary: normal
//! build, lint, test and format paths do not pay for this pass.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use thiserror::Error;
use uf_config::UniflowedConfig;
use uf_flow::ast::{self, CommentKind, function, pattern, statement};
use uf_flow::{Loc, ParseDiagnostic, ParseFailure};
use uf_project::{ProjectError, SourceKind, scan_source_files};

/// The generated API documentation report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocReport {
    /// How many JavaScript source files were parsed.
    pub files_scanned: usize,
    /// Modules with at least one documented export.
    pub modules: Vec<DocModule>,
    /// Flow parser diagnostics found while reading source files.
    pub diagnostics: Vec<DocDiagnostic>,
    /// Files discovery could not read at all, as `path: reason`.
    ///
    /// Separate from `diagnostics`, which are about Flow: a file that is not
    /// UTF-8 has no syntax to be wrong about. Reported rather than fatal —
    /// one stray byte used to stop the whole walk.
    pub unreadable: Vec<String>,
}

impl DocReport {
    /// Count documented entries across all modules.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.modules.iter().map(|module| module.entries.len()).sum()
    }

    /// Whether parsing found diagnostics that should fail the command.
    #[must_use]
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// A documented source module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocModule {
    /// Repository-relative source path.
    pub path: String,
    /// Documented exports in source order.
    pub entries: Vec<DocEntry>,
}

/// One documented export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocEntry {
    /// Public export name.
    pub name: String,
    /// Declaration kind.
    pub kind: DocKind,
    /// One-based source line where the declaration begins.
    pub line: usize,
    /// Signature rendered from the Flow source.
    pub signature: String,
    /// Free-form JSDoc description.
    pub description: String,
    /// Parsed JSDoc tags.
    pub tags: Vec<DocTag>,
}

/// The declaration kind `uf doc` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocKind {
    /// A Flow component declaration.
    Component,
    /// A class declaration.
    Class,
    /// A Flow enum declaration.
    Enum,
    /// A function declaration.
    Function,
    /// A hook declaration.
    Hook,
    /// An interface declaration.
    Interface,
    /// An opaque type alias.
    OpaqueType,
    /// A type alias.
    Type,
    /// A variable declaration.
    Variable,
    /// Any other default export expression.
    Default,
}

impl DocKind {
    fn label(self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Class => "class",
            Self::Enum => "enum",
            Self::Function => "function",
            Self::Hook => "hook",
            Self::Interface => "interface",
            Self::OpaqueType => "opaque type",
            Self::Type => "type",
            Self::Variable => "variable",
            Self::Default => "default",
        }
    }
}

/// One parsed JSDoc tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocTag {
    /// Tag name without the leading `@`.
    pub name: String,
    /// Tag value after the tag name.
    pub value: String,
}

/// One parser diagnostic in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocDiagnostic {
    /// Repository-relative source path.
    pub path: String,
    /// Parser message.
    pub message: String,
    /// One-based source line, if Flow reported one.
    pub line: Option<u32>,
    /// Zero-based source column, if Flow reported one.
    pub column: Option<u32>,
}

/// Errors raised while generating API docs.
#[derive(Debug, Error)]
pub enum DocError {
    /// Project source discovery failed.
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// The Flow parser refused a source before parsing it.
    #[error(transparent)]
    Parse(#[from] ParseFailure),
    /// Output could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that could not be written.
        path: Utf8PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Generate a documentation report from the project source files.
///
/// Only Flow JavaScript source files are parsed, and only documented exported
/// declarations become entries. Syntax diagnostics are recorded in the report
/// so callers can render them before failing the command.
pub fn generate(root: &Utf8Path, config: &UniflowedConfig) -> Result<DocReport, DocError> {
    let scan = scan_source_files(root, config)?;

    // On a thread with the stack the parser documents for its ceilings, the
    // way `uf_fmt` does. Reading a tree recurses once per level and so does
    // *freeing* it, and the free happens wherever the `Parsed` is held — a
    // main thread's 8 MiB is not enough for a source at `MAX_CHAIN_DEPTH`,
    // which `uf fmt` formats without complaint. See ubugeeei-prod/uf#155.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("uf-doc".into())
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, || document(scan))
            .map_err(|source| DocError::Write {
                path: root.to_path_buf(),
                source,
            })?
            .join()
            .unwrap_or_else(|_| {
                Err(DocError::Write {
                    path: root.to_path_buf(),
                    source: std::io::Error::other("the documentation worker panicked"),
                })
            })
    })
}

/// Every documented export in `scan`, with the syntax diagnostics found on
/// the way and the files that could not be read.
fn document(scan: uf_project::SourceScan) -> Result<DocReport, DocError> {
    let mut report = DocReport {
        unreadable: scan
            .unreadable
            .iter()
            .map(|file| format!("{}: {}", file.relative_path, file.reason))
            .collect(),
        ..DocReport::default()
    };

    for file in scan
        .files
        .into_iter()
        .filter(|file| file.kind == SourceKind::JavaScript)
    {
        report.files_scanned += 1;
        let parsed = uf_flow::parse(&file.source)?;
        if !parsed.is_ok() {
            report.diagnostics.extend(
                parsed
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic_for(&file.relative_path, diagnostic)),
            );
            continue;
        }

        let entries = extract_entries(&file.source, &parsed.program);
        if !entries.is_empty() {
            report.modules.push(DocModule {
                path: file.relative_path,
                entries,
            });
        }
    }

    Ok(report)
}

/// Render the report as one Markdown document.
#[must_use]
pub fn render_markdown(report: &DocReport) -> String {
    let mut out = String::new();
    out.push_str("# API\n\n");
    out.push_str("Generated by `uf doc` from exported Flow declarations.\n\n");

    if report.modules.is_empty() {
        out.push_str("No documented exports found.\n");
        return out;
    }

    for module in &report.modules {
        out.push_str("## ");
        out.push_str(&module.path);
        out.push_str("\n\n");

        for entry in &module.entries {
            out.push_str("### ");
            out.push_str(&entry.name);
            out.push_str("\n\n");
            out.push_str("- kind: ");
            out.push_str(entry.kind.label());
            out.push('\n');
            out.push_str("- source: `");
            out.push_str(&module.path);
            out.push(':');
            out.push_str(&entry.line.to_string());
            out.push_str("`\n\n");
            if !entry.description.is_empty() {
                out.push_str(&entry.description);
                out.push_str("\n\n");
            }
            out.push_str("```js\n");
            out.push_str(&entry.signature);
            out.push_str("\n```\n");
            if !entry.tags.is_empty() {
                out.push('\n');
                for tag in &entry.tags {
                    out.push_str("- @");
                    out.push_str(&tag.name);
                    if !tag.value.is_empty() {
                        out.push(' ');
                        out.push_str(&tag.value);
                    }
                    out.push('\n');
                }
            }
            out.push('\n');
        }
    }

    out
}

/// Write the Markdown report into `out_dir/api.md`.
pub fn write_markdown(report: &DocReport, out_dir: &Utf8Path) -> Result<Utf8PathBuf, DocError> {
    fs::create_dir_all(out_dir).map_err(|source| DocError::Write {
        path: out_dir.to_path_buf(),
        source,
    })?;
    let path = out_dir.join("api.md");
    fs::write(&path, render_markdown(report)).map_err(|source| DocError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

fn diagnostic_for(path: &str, diagnostic: &ParseDiagnostic) -> DocDiagnostic {
    DocDiagnostic {
        path: path.to_string(),
        message: diagnostic.message.clone(),
        line: diagnostic.line,
        column: diagnostic.column,
    }
}

fn extract_entries(source: &str, program: &ast::Program<Loc, Loc>) -> Vec<DocEntry> {
    let map = LineMap::new(source);
    let mut entries = Vec::new();
    for statement in program.statements.iter() {
        collect_export_entries(source, &map, &program.all_comments, statement, &mut entries);
    }
    entries
}

fn collect_export_entries(
    source: &str,
    map: &LineMap,
    comments: &[ast::Comment<Loc>],
    node: &statement::Statement<Loc, Loc>,
    entries: &mut Vec<DocEntry>,
) {
    match &**node {
        statement::StatementInner::ExportNamedDeclaration { loc, inner } => {
            if inner.source.is_some() {
                return;
            }
            let Some(declaration) = &inner.declaration else {
                return;
            };
            let Some(jsdoc) = leading_jsdoc(source, map, comments, loc) else {
                return;
            };
            collect_named_declaration(source, map, loc, declaration, &jsdoc, entries);
        }
        statement::StatementInner::ExportDefaultDeclaration { loc, inner } => {
            let Some(jsdoc) = leading_jsdoc(source, map, comments, loc) else {
                return;
            };
            collect_default_declaration(source, map, loc, inner, &jsdoc, entries);
        }
        statement::StatementInner::DeclareExportDeclaration { loc, inner } => {
            if inner.source.is_some() {
                return;
            }
            let Some(jsdoc) = leading_jsdoc(source, map, comments, loc) else {
                return;
            };
            collect_declare_export(source, map, loc, inner, &jsdoc, entries);
        }
        _ => {}
    }
}

fn collect_named_declaration(
    source: &str,
    map: &LineMap,
    export_loc: &Loc,
    declaration: &statement::Statement<Loc, Loc>,
    jsdoc: &ParsedJsdoc,
    entries: &mut Vec<DocEntry>,
) {
    let line = line_of(export_loc);
    match &**declaration {
        statement::StatementInner::ComponentDeclaration { inner, .. } => {
            push_entry(
                entries,
                &inner.id.name,
                DocKind::Component,
                line,
                signature_with_prefix(source, map, "export ", &inner.sig_loc, true),
                jsdoc,
            );
        }
        statement::StatementInner::ClassDeclaration { inner, .. } => {
            if let Some(id) = &inner.id {
                push_entry(
                    entries,
                    &id.name,
                    DocKind::Class,
                    line,
                    body_signature(source, map, export_loc),
                    jsdoc,
                );
            }
        }
        statement::StatementInner::EnumDeclaration { inner, .. } => {
            push_entry(
                entries,
                &inner.id.name,
                DocKind::Enum,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        statement::StatementInner::FunctionDeclaration { inner, .. } => {
            let Some(id) = &inner.id else {
                return;
            };
            push_entry(
                entries,
                &id.name,
                function_kind(inner),
                line,
                signature_with_prefix(source, map, "export ", &inner.sig_loc, true),
                jsdoc,
            );
        }
        statement::StatementInner::InterfaceDeclaration { inner, .. } => {
            push_entry(
                entries,
                &inner.id.name,
                DocKind::Interface,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        statement::StatementInner::OpaqueType { inner, .. } => {
            push_entry(
                entries,
                &inner.id.name,
                DocKind::OpaqueType,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        statement::StatementInner::TypeAlias { inner, .. } => {
            push_entry(
                entries,
                &inner.id.name,
                DocKind::Type,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        statement::StatementInner::VariableDeclaration { inner, .. } => {
            if let Some(name) = variable_name(inner) {
                push_entry(
                    entries,
                    name,
                    DocKind::Variable,
                    line,
                    signature(source, map, export_loc),
                    jsdoc,
                );
            }
        }
        _ => {}
    }
}

fn collect_default_declaration(
    source: &str,
    map: &LineMap,
    export_loc: &Loc,
    declaration: &statement::ExportDefaultDeclaration<Loc, Loc>,
    jsdoc: &ParsedJsdoc,
    entries: &mut Vec<DocEntry>,
) {
    use statement::export_default_declaration::Declaration;
    match &declaration.declaration {
        Declaration::Declaration(statement) => {
            let (kind, signature) = match &**statement {
                statement::StatementInner::ComponentDeclaration { inner, .. } => (
                    DocKind::Component,
                    signature_with_prefix(source, map, "export default ", &inner.sig_loc, true),
                ),
                statement::StatementInner::ClassDeclaration { .. } => {
                    (DocKind::Class, body_signature(source, map, export_loc))
                }
                statement::StatementInner::FunctionDeclaration { inner, .. } => (
                    function_kind(inner),
                    signature_with_prefix(source, map, "export default ", &inner.sig_loc, true),
                ),
                _ => (DocKind::Default, signature(source, map, export_loc)),
            };
            push_entry(
                entries,
                "default",
                kind,
                line_of(export_loc),
                signature,
                jsdoc,
            );
        }
        Declaration::Expression(_) => {
            push_entry(
                entries,
                "default",
                DocKind::Default,
                line_of(export_loc),
                body_signature(source, map, export_loc),
                jsdoc,
            );
        }
    }
}

fn collect_declare_export(
    source: &str,
    map: &LineMap,
    export_loc: &Loc,
    declaration: &statement::DeclareExportDeclaration<Loc, Loc>,
    jsdoc: &ParsedJsdoc,
    entries: &mut Vec<DocEntry>,
) {
    use statement::declare_export_declaration::Declaration;
    let Some(declaration) = &declaration.declaration else {
        return;
    };
    let line = line_of(export_loc);
    match declaration {
        Declaration::Class { declaration, .. } => {
            push_entry(
                entries,
                &declaration.id.name,
                DocKind::Class,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        Declaration::Component { declaration, .. } => {
            push_entry(
                entries,
                &declaration.id.name,
                DocKind::Component,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        Declaration::Enum { declaration, .. } => {
            push_entry(
                entries,
                &declaration.id.name,
                DocKind::Enum,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        Declaration::Function { declaration, .. } => {
            if let Some(id) = &declaration.id {
                push_entry(
                    entries,
                    &id.name,
                    declare_function_kind(declaration),
                    line,
                    signature(source, map, export_loc),
                    jsdoc,
                );
            }
        }
        Declaration::Interface { declaration, .. } => {
            push_entry(
                entries,
                &declaration.id.name,
                DocKind::Interface,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        Declaration::NamedOpaqueType { declaration, .. } => {
            push_entry(
                entries,
                &declaration.id.name,
                DocKind::OpaqueType,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        Declaration::NamedType { declaration, .. } => {
            push_entry(
                entries,
                &declaration.id.name,
                DocKind::Type,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        Declaration::Variable { declaration, .. } => {
            if let Some(name) = declare_variable_name(declaration) {
                push_entry(
                    entries,
                    name,
                    DocKind::Variable,
                    line,
                    signature(source, map, export_loc),
                    jsdoc,
                );
            }
        }
        Declaration::DefaultType { .. } => {
            push_entry(
                entries,
                "default",
                DocKind::Type,
                line,
                signature(source, map, export_loc),
                jsdoc,
            );
        }
        Declaration::Namespace { declaration, .. } => {
            if let Some(name) = namespace_name(declaration) {
                push_entry(
                    entries,
                    name,
                    DocKind::Variable,
                    line,
                    signature(source, map, export_loc),
                    jsdoc,
                );
            }
        }
    }
}

fn push_entry(
    entries: &mut Vec<DocEntry>,
    name: &str,
    kind: DocKind,
    line: usize,
    signature: String,
    jsdoc: &ParsedJsdoc,
) {
    entries.push(DocEntry {
        name: name.to_string(),
        kind,
        line,
        signature,
        description: jsdoc.description.clone(),
        tags: jsdoc.tags.clone(),
    });
}

fn function_kind(function: &function::Function<Loc, Loc>) -> DocKind {
    if matches!(function.effect_, function::Effect::Hook) {
        DocKind::Hook
    } else {
        DocKind::Function
    }
}

fn declare_function_kind(function: &statement::DeclareFunction<Loc, Loc>) -> DocKind {
    match &*function.annot.annotation {
        ast::types::TypeInner::Function { inner, .. }
            if matches!(inner.effect, function::Effect::Hook) =>
        {
            DocKind::Hook
        }
        _ => DocKind::Function,
    }
}

fn variable_name(declaration: &statement::VariableDeclaration<Loc, Loc>) -> Option<&str> {
    declaration
        .declarations
        .first()
        .and_then(|declarator| pattern_name(&declarator.id))
}

fn declare_variable_name(declaration: &statement::DeclareVariable<Loc, Loc>) -> Option<&str> {
    declaration
        .declarations
        .first()
        .and_then(|declarator| pattern_name(&declarator.id))
}

fn namespace_name(declaration: &statement::DeclareNamespace<Loc, Loc>) -> Option<&str> {
    match &declaration.id {
        statement::declare_namespace::Id::Global(id)
        | statement::declare_namespace::Id::Local(id) => Some(&id.name),
    }
}

fn pattern_name(pattern: &pattern::Pattern<Loc, Loc>) -> Option<&str> {
    match pattern {
        pattern::Pattern::Identifier { inner, .. } => Some(&inner.name.name),
        _ => None,
    }
}

fn line_of(loc: &Loc) -> usize {
    usize::try_from(loc.start.line).unwrap_or(0).max(1)
}

fn signature(source: &str, map: &LineMap, loc: &Loc) -> String {
    map.slice(source, loc).trim().to_string()
}

fn signature_with_prefix(
    source: &str,
    map: &LineMap,
    prefix: &str,
    loc: &Loc,
    has_body: bool,
) -> String {
    let mut value = String::new();
    value.push_str(prefix);
    value.push_str(map.slice(source, loc).trim());
    if has_body {
        value.push_str(" { ... }");
    }
    value
}

fn body_signature(source: &str, map: &LineMap, loc: &Loc) -> String {
    let mut value = signature(source, map, loc);
    if let Some(index) = find_top_level_body(&value) {
        value.truncate(index);
        value = value.trim_end().to_string();
        value.push_str(" { ... }");
    }
    value
}

fn find_top_level_body(value: &str) -> Option<usize> {
    let mut parens = 0usize;
    let mut brackets = 0usize;
    let mut angles = 0usize;
    let bytes = value.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' | b'`' => index = skip_string(bytes, index),
            b'/' if bytes.get(index + 1) == Some(&b'/') => index = skip_line_comment(bytes, index),
            b'/' if bytes.get(index + 1) == Some(&b'*') => index = skip_block_comment(bytes, index),
            b'(' => {
                parens += 1;
                index += 1;
            }
            b')' => {
                parens = parens.saturating_sub(1);
                index += 1;
            }
            b'[' => {
                brackets += 1;
                index += 1;
            }
            b']' => {
                brackets = brackets.saturating_sub(1);
                index += 1;
            }
            b'<' => {
                angles += 1;
                index += 1;
            }
            b'>' => {
                angles = angles.saturating_sub(1);
                index += 1;
            }
            b'{' if parens == 0 && brackets == 0 && angles == 0 => return Some(index),
            _ => index += 1,
        }
    }

    None
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |offset| start + offset + 1)
}

fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn leading_jsdoc(
    source: &str,
    map: &LineMap,
    comments: &[ast::Comment<Loc>],
    loc: &Loc,
) -> Option<ParsedJsdoc> {
    let declaration_start = map.offset(&loc.start);
    comments
        .iter()
        .rev()
        .filter(|comment| matches!(comment.kind, CommentKind::Block))
        .filter(|comment| comment.text.trim_start().starts_with('*'))
        .find_map(|comment| {
            let comment_end = map.offset(&comment.loc.end);
            if comment_end > declaration_start {
                return None;
            }
            let gap = &source[comment_end..declaration_start];
            gap.chars()
                .all(char::is_whitespace)
                .then(|| parse_jsdoc(&comment.text))
        })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ParsedJsdoc {
    description: String,
    tags: Vec<DocTag>,
}

fn parse_jsdoc(raw: &str) -> ParsedJsdoc {
    let mut description = Vec::new();
    let mut tags = Vec::<DocTag>::new();
    for line in raw.lines().map(clean_jsdoc_line) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('@') {
            let (name, value) = rest
                .split_once(char::is_whitespace)
                .map_or((rest, ""), |split| (split.0, split.1.trim()));
            tags.push(DocTag {
                name: name.to_string(),
                value: value.to_string(),
            });
        } else if let Some(tag) = tags.last_mut()
            && !trimmed.is_empty()
        {
            if !tag.value.is_empty() {
                tag.value.push('\n');
            }
            tag.value.push_str(trimmed);
        } else {
            description.push(trimmed.to_string());
        }
    }

    while description.first().is_some_and(|line| line.is_empty()) {
        description.remove(0);
    }
    while description.last().is_some_and(|line| line.is_empty()) {
        description.pop();
    }

    ParsedJsdoc {
        description: description.join("\n"),
        tags,
    }
}

fn clean_jsdoc_line(line: &str) -> String {
    let line = line.trim_start();
    let Some(line) = line.strip_prefix('*') else {
        return line.to_string();
    };
    line.strip_prefix(' ').unwrap_or(line).to_string()
}

#[derive(Debug, Clone)]
struct LineMap {
    starts: Vec<usize>,
}

impl LineMap {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        starts.push(source.len());
        Self { starts }
    }

    fn offset(&self, position: &uf_flow::Position) -> usize {
        let line = usize::try_from(position.line)
            .unwrap_or(1)
            .saturating_sub(1);
        let start = self.starts.get(line).copied().unwrap_or(0);
        let column = usize::try_from(position.column).unwrap_or(0);
        start
            .saturating_add(column)
            .min(*self.starts.last().unwrap_or(&start))
    }

    fn slice<'a>(&self, source: &'a str, loc: &Loc) -> &'a str {
        let start = self.offset(&loc.start).min(source.len());
        let end = self.offset(&loc.end).min(source.len());
        source.get(start..end).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use similar_asserts::assert_eq;

    /// A source at the parser's ceiling does not abort the process.
    ///
    /// ubugeeei-prod/uf#155. Reading a tree recurses once per level and so
    /// does freeing it, and the free happens wherever the `Parsed` is held.
    /// `generate` ran on its caller's thread, so a member-call chain at
    /// `MAX_CHAIN_DEPTH` — a source `uf fmt` formats without complaint —
    /// overflowed a main thread's 8 MiB and took the process with it.
    ///
    /// This test runs on a *test* thread, 2 MiB, which is smaller still: if
    /// the work ever moves back onto the caller's stack it fails here first.
    #[test]
    fn a_source_at_the_parsers_ceiling_is_documented() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        // Two chain levels per link — a member and a call — and one more for
        // the `=`, so this is the deepest source the parser accepts.
        let chain = ".f()".repeat(uf_flow::MAX_CHAIN_DEPTH / 2 - 1);
        fs::write(
            root.join("src/deep.js"),
            format!("// @flow\n\n/** Deep. */\nexport const deep = a{chain};\n"),
        )
        .unwrap();

        let report = generate(root, &UniflowedConfig::default()).unwrap();

        assert_eq!(report.diagnostics, Vec::new());
        assert_eq!(report.files_scanned, 1);
    }

    #[test]
    fn documents_exported_flow_declarations() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(
            root.join("src/api.js"),
            r#"
// @flow

/**
 * Stable user id.
 * @opaque public identifier
 */
export opaque type UserId = string;

/**
 * Reads a user.
 * @param id identifier to load
 * @returns maybe a user name
 */
export function readUser(id: UserId): ?{| readonly name: string |} {
  return null;
}

/** Render a user's avatar. */
export component Avatar(name: string) renders React.Node {
  return name;
}

/** Not exported. */
function localOnly(): void {}
"#,
        )
        .unwrap();

        let report = generate(root, &UniflowedConfig::default()).unwrap();

        assert_eq!(report.diagnostics, Vec::new());
        assert_eq!(report.files_scanned, 1);
        assert_eq!(report.entry_count(), 3);
        let module = report
            .modules
            .iter()
            .find(|module| module.path == "src/api.js")
            .unwrap();
        assert_eq!(
            module
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["UserId", "readUser", "Avatar"]
        );
        assert_eq!(module.entries[0].kind, DocKind::OpaqueType);
        assert_eq!(module.entries[1].kind, DocKind::Function);
        assert_eq!(
            module.entries[1].signature,
            "export function readUser(id: UserId): ?{| readonly name: string |} { ... }"
        );
        assert_eq!(module.entries[2].kind, DocKind::Component);
    }

    #[test]
    fn reports_flow_parse_diagnostics_without_writing_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(dir.path()).unwrap();
        fs::write(root.join("broken.js"), "// @flow\ntype = ;\n").unwrap();

        let report = generate(root, &UniflowedConfig::default()).unwrap();

        assert_eq!(report.modules, Vec::new());
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].path, "broken.js");
        assert_eq!(report.diagnostics[0].line, Some(2));
    }

    #[test]
    fn renders_markdown_with_flow_signatures_and_jsdoc_tags() {
        let report = DocReport {
            files_scanned: 1,
            diagnostics: Vec::new(),
            unreadable: Vec::new(),
            modules: vec![DocModule {
                path: "src/api.js".to_string(),
                entries: vec![DocEntry {
                    name: "readUser".to_string(),
                    kind: DocKind::Function,
                    line: 4,
                    signature: "export function readUser(id: UserId): ?User".to_string(),
                    description: "Reads a user.".to_string(),
                    tags: vec![DocTag {
                        name: "param".to_string(),
                        value: "id identifier to load".to_string(),
                    }],
                }],
            }],
        };

        assert_eq!(
            render_markdown(&report),
            "# API\n\nGenerated by `uf doc` from exported Flow declarations.\n\n## src/api.js\n\n### readUser\n\n- kind: function\n- source: `src/api.js:4`\n\nReads a user.\n\n```js\nexport function readUser(id: UserId): ?User\n```\n\n- @param id identifier to load\n\n"
        );
    }
}
