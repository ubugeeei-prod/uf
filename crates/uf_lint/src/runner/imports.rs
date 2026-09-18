//! Static import rules that do not need the project module graph.

use std::path::{Component, Path, PathBuf};

use uf_config::UniflowedConfig;
use uf_flow::scan::{Token, TokenKind, starts_statement, tokenize};

use crate::scan::FileScan;
use crate::{Diagnostic, Severity, push, severity};

pub(crate) fn run_import_no_absolute_path(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = "import/no-absolute-path";
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for import in static_imports(&scan.file.source) {
        if is_absolute_import(import.source) {
            push_import(
                diagnostics,
                scan,
                rule,
                severity,
                import.source_at,
                "import paths must be portable; use a package name or a relative path",
            );
        }
    }
}

pub(crate) fn run_import_no_duplicates(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = "import/no-duplicates";
    let Some(severity) = severity(config, rule) else {
        return;
    };

    let mut seen: Vec<StaticImport<'_>> = Vec::new();
    for import in static_imports(&scan.file.source) {
        if seen.iter().any(|seen| duplicates(*seen, import)) {
            push_import(
                diagnostics,
                scan,
                rule,
                severity,
                import.source_at,
                format!("`{}` is already imported in this file", import.source),
            );
        } else {
            seen.push(import);
        }
    }
}

pub(crate) fn run_import_no_self_import(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = "import/no-self-import";
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for import in static_imports(&scan.file.source) {
        if is_self_import(&scan.file.path, import.source) {
            push_import(
                diagnostics,
                scan,
                rule,
                severity,
                import.source_at,
                "a module must not import itself; move shared code to another module",
            );
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StaticImport<'a> {
    source: &'a str,
    source_at: usize,
    kind: ImportKind,
    shape: ImportShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportKind {
    Value,
    Type,
    Typeof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportShape {
    SideEffect,
    Namespace,
    NamedOrDefault,
}

fn static_imports(source: &str) -> Vec<StaticImport<'_>> {
    let tokens = tokenize(source);
    let mut imports = Vec::new();
    let mut at = 0usize;

    while let Some(token) = tokens.get(at) {
        if !token.is_ident(source, "import") || !starts_statement(&tokens, at) {
            at += 1;
            continue;
        }

        if let Some(import) = static_import(source, &tokens, at) {
            imports.push(import);
        }
        at += 1;
    }

    imports
}

fn static_import<'a>(
    source: &'a str,
    tokens: &[Token],
    import_at: usize,
) -> Option<StaticImport<'a>> {
    let next = tokens.get(import_at + 1)?;
    if next.is_punct(b'.') || next.is_punct(b'(') {
        return None;
    }
    let kind = if next.is_ident(source, "type") {
        ImportKind::Type
    } else if next.is_ident(source, "typeof") {
        ImportKind::Typeof
    } else {
        ImportKind::Value
    };

    let clause = import_at + 1 + usize::from(matches!(kind, ImportKind::Type | ImportKind::Typeof));

    if next.kind == TokenKind::String {
        return import_source(source, next, kind, ImportShape::SideEffect);
    }
    let shape = if tokens.get(clause).is_some_and(|token| token.is_punct(b'*')) {
        ImportShape::Namespace
    } else {
        ImportShape::NamedOrDefault
    };

    for pair in tokens[import_at + 1..].windows(2) {
        let [first, second] = pair else {
            unreachable!();
        };
        if first.is_punct(b';') {
            return None;
        }
        if first.is_ident(source, "from") && second.kind == TokenKind::String {
            return import_source(source, second, kind, shape);
        }
    }

    None
}

fn import_source<'a>(
    source: &'a str,
    token: &Token,
    kind: ImportKind,
    shape: ImportShape,
) -> Option<StaticImport<'a>> {
    let text = token.text(source);
    let quote = text.as_bytes().first().copied()?;
    if !matches!(quote, b'\'' | b'"') || text.as_bytes().last().copied() != Some(quote) {
        return None;
    }
    Some(StaticImport {
        source: &text[1..text.len().saturating_sub(1)],
        source_at: token.start,
        kind,
        shape,
    })
}

fn duplicates(first: StaticImport<'_>, second: StaticImport<'_>) -> bool {
    if first.source != second.source || first.kind != second.kind {
        return false;
    }
    if first.shape == ImportShape::Namespace || second.shape == ImportShape::Namespace {
        return first.shape == second.shape;
    }
    true
}

fn is_absolute_import(source: &str) -> bool {
    if source.starts_with('/') || source.starts_with("\\\\") {
        return true;
    }

    let bytes = source.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn is_self_import(file: &str, source: &str) -> bool {
    if !is_relative_import(source) {
        return false;
    }

    let file = Path::new(file);
    let Some(parent) = file.parent() else {
        return false;
    };

    let current = normalize_path(file);
    let target = normalize_path(&parent.join(source));
    if target == current {
        return true;
    }

    if target.extension().is_none()
        && current
            .file_stem()
            .map(|stem| target == normalize_path(&current.with_file_name(stem)))
            .unwrap_or(false)
    {
        return true;
    }

    current.file_stem().is_some_and(|stem| stem == "index") && target == normalize_path(parent)
}

fn is_relative_import(source: &str) -> bool {
    source == "." || source == ".." || source.starts_with("./") || source.starts_with("../")
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if normalized.pop() => {}
            Component::ParentDir => normalized.push(".."),
            Component::Normal(segment) => normalized.push(segment),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn push_import(
    diagnostics: &mut Vec<Diagnostic>,
    scan: &FileScan<'_>,
    rule: &'static str,
    severity: Severity,
    offset: usize,
    message: impl Into<String>,
) {
    let position = scan.index.line_col(offset);
    push(
        diagnostics,
        scan.file,
        rule,
        severity,
        position.line,
        position.column,
        message,
    );
}
