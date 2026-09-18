//! Static import rules that do not need the project module graph.

use std::path::{Component, Path, PathBuf};

use uf_config::UniflowedConfig;
use uf_flow::scan::{Token, TokenKind, starts_statement, tokenize};
use uf_infra::FxHashMap;

use crate::scan::FileScan;
use crate::{Diagnostic, LintContext, Severity, SourceFile, push, severity};

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

pub(crate) fn run_import_no_cycle(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    context: &LintContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = "import/no-cycle";
    let Some(severity) = severity(config, rule) else {
        return;
    };
    let Some(cycle) = context.import_graph().cycle_from(&scan.file.path) else {
        return;
    };

    push_import(
        diagnostics,
        scan,
        rule,
        severity,
        cycle.source_at,
        format!(
            "this import creates a cycle through `{}`",
            cycle.target_path
        ),
    );
}

pub(crate) fn run_import_no_extraneous_dependencies(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    context: &LintContext,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = "import/no-extraneous-dependencies";
    let Some(severity) = severity(config, rule) else {
        return;
    };
    let Some(manifest) = context.nearest_package(&scan.file.path) else {
        return;
    };

    for import in static_imports(&scan.file.source) {
        let Some(package) = imported_package_name(import.source) else {
            continue;
        };
        if manifest.name.as_deref() == Some(package)
            || manifest.declared.contains(package)
            || is_generated_router_dependency(&scan.file.path, package, config)
        {
            continue;
        }
        push_import(
            diagnostics,
            scan,
            rule,
            severity,
            import.source_at,
            format!("`{package}` must be declared in the nearest package.json"),
        );
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

pub(crate) fn run_import_no_relative_packages(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = "import/no-relative-packages";
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for import in static_imports(&scan.file.source) {
        if let Some(target) = relative_workspace_package(&scan.file.path, import.source) {
            push_import(
                diagnostics,
                scan,
                rule,
                severity,
                import.source_at,
                format!("use the `{target}` package name instead of a relative path into it"),
            );
        }
    }
}

pub(crate) fn run_import_no_useless_path_segments(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = "import/no-useless-path-segments";
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for import in static_imports(&scan.file.source) {
        if let Some(shorter) = shorter_import_path(import.source) {
            push_import(
                diagnostics,
                scan,
                rule,
                severity,
                import.source_at,
                format!("`{}` can be written as `{shorter}`", import.source),
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

fn imported_package_name(source: &str) -> Option<&str> {
    let (source, _) = split_import_suffix(source);
    if is_relative_import(source)
        || is_absolute_import(source)
        || source.starts_with('#')
        || source.starts_with("@/")
        || source.contains(':')
    {
        return None;
    }

    let package = if let Some(rest) = source.strip_prefix('@') {
        let (scope, rest) = rest.split_once('/')?;
        let (name, _) = rest.split_once('/').unwrap_or((rest, ""));
        if scope.is_empty() || name.is_empty() {
            return None;
        }
        &source[..1 + scope.len() + 1 + name.len()]
    } else {
        let (name, _) = source.split_once('/').unwrap_or((source, ""));
        if !name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        {
            return None;
        }
        name
    };

    (!is_node_builtin_package(package)).then_some(package)
}

fn is_generated_router_dependency(file: &str, package: &str, config: &UniflowedConfig) -> bool {
    package == "@uniflowed/router"
        && is_router_manifest_file(file, config.app.router.manifest.as_str())
}

fn is_router_manifest_file(file: &str, manifest: &str) -> bool {
    if file == manifest {
        return true;
    }

    let (directory, name) = manifest
        .rsplit_once('/')
        .map_or(("", manifest), |(directory, name)| (directory, name));
    let (stem, extension) = name.rsplit_once('.').unwrap_or((name, "js"));
    ["native", "ios", "android"].iter().any(|platform| {
        let native = if directory.is_empty() {
            format!("{stem}.{platform}.{extension}")
        } else {
            format!("{directory}/{stem}.{platform}.{extension}")
        };
        file == native
    })
}

struct Cycle {
    source_at: usize,
    target_path: String,
}

/// Relative import graph over the source batch available to lint rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ImportGraph {
    paths: Vec<String>,
    edges: Vec<Vec<ImportEdge>>,
    by_path: FxHashMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportEdge {
    to: usize,
    source_at: usize,
}

impl ImportGraph {
    /// Build the graph from the files the caller made available.
    pub(crate) fn new(files: &[SourceFile]) -> Self {
        let mut candidates = files.iter().filter(|file| is_graph_module_path(&file.path));
        let Some(first) = candidates.next() else {
            return Self::default();
        };
        let Some(second) = candidates.next() else {
            return Self::default();
        };
        let mut files = Vec::with_capacity(2);
        files.push(first);
        files.push(second);
        files.extend(candidates);

        let mut paths = Vec::new();
        let mut by_path = FxHashMap::default();
        for file in &files {
            let path = normalize_graph_path(&file.path);
            if by_path.contains_key(&path) {
                continue;
            }
            let index = paths.len();
            by_path.insert(path.clone(), index);
            paths.push(path);
        }

        let mut graph = Self {
            edges: (0..paths.len()).map(|_| Vec::new()).collect(),
            paths,
            by_path,
        };
        for file in files {
            graph.add_edges(file);
        }
        graph
    }

    fn add_edges(&mut self, file: &SourceFile) {
        let importer = normalize_graph_path(&file.path);
        let Some(from) = self.by_path.get(&importer).copied() else {
            return;
        };
        let imports = static_imports(&file.source)
            .into_iter()
            .filter_map(|import| {
                let (source, suffix) = split_import_suffix(import.source);
                if suffix.is_empty() {
                    self.resolve(source, &importer).map(|to| ImportEdge {
                        to,
                        source_at: import.source_at,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        self.edges[from].extend(imports);
    }

    fn cycle_from(&self, file: &str) -> Option<Cycle> {
        if self.paths.is_empty() {
            return None;
        }
        let start = *self.by_path.get(&normalize_graph_path(file))?;
        self.edges[start].iter().find_map(|edge| {
            if edge.to != start && self.reaches(edge.to, start, &mut vec![false; self.paths.len()])
            {
                Some(Cycle {
                    source_at: edge.source_at,
                    target_path: self.paths[edge.to].clone(),
                })
            } else {
                None
            }
        })
    }

    fn reaches(&self, current: usize, target: usize, seen: &mut [bool]) -> bool {
        if current == target {
            return true;
        }
        if seen[current] {
            return false;
        }
        seen[current] = true;
        self.edges[current]
            .iter()
            .any(|edge| self.reaches(edge.to, target, seen))
    }

    fn resolve(&self, specifier: &str, importer: &str) -> Option<usize> {
        if !is_relative_import(specifier) {
            return None;
        }
        let base = graph_join(importer, specifier)?;
        self.lookup(&base).or_else(|| {
            let extensions = ["js", "mjs", "cjs", "jsx"];
            extensions
                .iter()
                .find_map(|extension| self.lookup(&format!("{base}.{extension}")))
                .or_else(|| {
                    extensions
                        .iter()
                        .find_map(|extension| self.lookup(&format!("{base}/index.{extension}")))
                })
        })
    }

    fn lookup(&self, path: &str) -> Option<usize> {
        self.by_path
            .get(path)
            .or_else(|| self.by_path.get(&format!("{path}.flow")))
            .copied()
    }
}

fn is_graph_module_path(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("js" | "jsx" | "mjs" | "cjs" | "flow")
    )
}

fn graph_join(importer: &str, specifier: &str) -> Option<String> {
    let directory = importer.rsplit_once('/').map_or("", |(head, _)| head);
    let mut segments = Vec::new();
    for segment in directory
        .split('/')
        .chain(specifier.split('/'))
        .filter(|segment| !segment.is_empty() && *segment != ".")
    {
        if segment == ".." {
            segments.pop()?;
        } else {
            segments.push(segment);
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

fn normalize_graph_path(path: &str) -> String {
    let mut segments = Vec::new();
    for segment in path
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
    {
        if segment == ".." {
            if segments.pop().is_none() {
                return path.to_owned();
            }
        } else {
            segments.push(segment);
        }
    }
    segments.join("/")
}

fn is_node_builtin_package(package: &str) -> bool {
    matches!(
        package,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "stream"
            | "string_decoder"
            | "sys"
            | "timers"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
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

fn relative_workspace_package(file: &str, source: &str) -> Option<String> {
    let (source, _) = split_import_suffix(source);
    if !is_relative_import(source) {
        return None;
    }

    let file = normalize_path(Path::new(file));
    let parent = file.parent()?;
    let target = normalize_path(&parent.join(source));

    let current = workspace_package(&file)?;
    let target = workspace_package(&target)?;
    (current.root == target.root && current.name != target.name).then_some(target.name)
}

#[derive(Debug, PartialEq, Eq)]
struct WorkspacePackage {
    root: PathBuf,
    name: String,
}

fn workspace_package(path: &Path) -> Option<WorkspacePackage> {
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        if component.as_os_str() != "packages" {
            continue;
        }
        let Some(Component::Normal(name)) = components.get(index + 1) else {
            continue;
        };
        let mut root = PathBuf::new();
        for component in &components[..index] {
            root.push(component.as_os_str());
        }
        return Some(WorkspacePackage {
            root,
            name: name.to_string_lossy().into_owned(),
        });
    }
    None
}

fn is_relative_import(source: &str) -> bool {
    source == "." || source == ".." || source.starts_with("./") || source.starts_with("../")
}

fn shorter_import_path(source: &str) -> Option<String> {
    let (path, suffix) = split_import_suffix(source);
    if !is_relative_import(path) {
        return None;
    }

    let normalized = normalize_relative_specifier(path);
    (normalized != path).then(|| format!("{normalized}{suffix}"))
}

fn split_import_suffix(source: &str) -> (&str, &str) {
    let query = source.find('?').unwrap_or(source.len());
    let fragment = source.find('#').unwrap_or(source.len());
    let at = query.min(fragment);
    source.split_at(at)
}

fn normalize_relative_specifier(source: &str) -> String {
    let mut segments = Vec::new();
    for segment in source.split('/') {
        match segment {
            "" | "." => {}
            ".." if segments.last().is_some_and(|last| *last != "..") => {
                segments.pop();
            }
            ".." => segments.push(".."),
            segment => segments.push(segment),
        }
    }
    format_relative_segments(&segments)
}

fn format_relative_segments(segments: &[&str]) -> String {
    if segments.is_empty() {
        return ".".to_owned();
    }
    if segments[0] == ".." {
        return segments.join("/");
    }
    format!("./{}", segments.join("/"))
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
