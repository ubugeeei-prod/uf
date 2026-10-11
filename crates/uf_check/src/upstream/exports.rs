//! The names a project's own files export, for completion's auto-imports.
//!
//! Flow's autocomplete service will insert an import when a search of this
//! index says the name lives in another file. The search is Flow's. What it
//! does not have, outside a running Flow server, is the index, so this walks
//! each file's parsed program and records the exported names.
//!
//! A package under `node_modules` and a library definition under `flow-typed`
//! are not recorded: importing either is not a relative path this walk can
//! honestly write. `export *` and a name bound by destructuring are skipped
//! too — the first has no names of its own, and the second's binding is not
//! the exported name a one-line import can name.

use std::rc::Rc;

use compact_str::CompactString;
use dupe::Dupe;
use flow_common::options::Options;
use flow_parser::ast::{self, pattern::Pattern};
use flow_parser::file_key::{FileKey, FileKeyInner};
use flow_parser::loc::Loc;
use flow_services_export::export_index::{self, Kind, Source};
use flow_services_export::export_search::{self, ExportSearch};
use uf_infra::FxHashMap;

use super::parse;

/// One exported name, the kind it was exported as, and how many times that
/// pair was written.
struct RecordedExport {
    name: CompactString,
    kind: Kind,
    count: i32,
}

/// One file's exported names, kept so an edit re-parses only that file.
///
/// The assembled [`ExportSearch`] is cheap to rebuild from these records.
/// Parsing every file again is not, and a completion follows most keystrokes.
pub(super) struct FileExports {
    records: FxHashMap<usize, Rc<[RecordedExport]>>,
}

impl FileExports {
    pub(super) fn new() -> Self {
        Self {
            records: FxHashMap::default(),
        }
    }

    pub(super) fn clear(&mut self) {
        self.records.clear();
    }

    pub(super) fn invalidate(&mut self, index: usize) {
        self.records.remove(&index);
    }

    #[cfg(test)]
    pub(super) fn has(&self, index: usize) -> bool {
        self.records.contains_key(&index)
    }

    /// Records for every file, parsing only the ones [`Self::invalidate`]
    /// dropped, then one search over all of them.
    pub(super) fn search<'a>(
        &mut self,
        files: impl Iterator<Item = (usize, &'a str, &'a str)>,
        options: &Options,
    ) -> ExportSearch {
        let mut index = export_index::empty();
        for (at, path, text) in files {
            let records = match self.records.get(&at) {
                Some(cached) => Rc::clone(cached),
                None => {
                    let recorded: Rc<[RecordedExport]> = file_records(path, text, options).into();
                    self.records.insert(at, Rc::clone(&recorded));
                    recorded
                }
            };
            contribute(path, &records, &mut index);
        }
        export_search::init(index)
    }
}

/// One search over the names `sources` export.
///
/// `sources` is `(path, text)` in the batch's own path spelling. A file that
/// does not parse, or that opted out of Flow, contributes nothing: there is
/// no export list to trust then.
#[cfg(test)]
fn project_search<'a>(
    sources: impl IntoIterator<Item = (&'a str, &'a str)>,
    options: &Options,
) -> ExportSearch {
    let mut index = export_index::empty();
    for (path, text) in sources {
        contribute(path, &file_records(path, text, options), &mut index);
    }
    export_search::init(index)
}

/// The names `path` exports: the exported spelling, its kind, and how many
/// times that pair was written.
///
/// Empty when the file is not a project source, does not parse, or opted out
/// of Flow. The path is not stored here; [`contribute`] attaches it, so a
/// cached list stays valid until this file's own text changes.
fn file_records(path: &str, text: &str, options: &Options) -> Vec<RecordedExport> {
    if !is_indexed_source(path) {
        return Vec::new();
    }
    let file_key = FileKey::new(FileKeyInner::SourceFile(path.to_owned()));
    let parsed = parse::parse_file(file_key.dupe(), text, options, false);
    if !parsed.is_parseable() || !parsed.is_checked() {
        return Vec::new();
    }
    let mut index = export_index::empty();
    let source = Source::FileKey(file_key);
    for statement in parsed.ast.statements.iter() {
        record_statement(statement, &source, &mut index);
    }
    let mut found = Vec::new();
    for (name, exports) in index.iter() {
        for (export_index::Export(_source, kind), count) in exports.iter() {
            found.push(RecordedExport {
                name: CompactString::from(name.as_str()),
                kind: kind.clone(),
                count: *count,
            });
        }
    }
    found
}

/// Add `records` to `index` as exports of `path`.
fn contribute(path: &str, records: &[RecordedExport], index: &mut export_index::ExportIndex) {
    if records.is_empty() {
        return;
    }
    let source = Source::FileKey(FileKey::new(FileKeyInner::SourceFile(path.to_owned())));
    for record in records {
        for _ in 0..record.count {
            export_index::add(&record.name, source.clone(), record.kind.clone(), index);
        }
    }
}

/// Write `.js` or `.jsx` back onto a relative specifier in `text`.
///
/// Flow's import edit strips a trailing `.js` and a trailing `/index.js`.
/// uf's modules import with the extension written out, so a specifier that
/// names one of `files` is rewritten to that file. A package specifier, and a
/// specifier that already names an extension, are left as Flow wrote them.
pub(super) fn restore_explicit_extensions(
    text: &str,
    importer: &str,
    files: &[CompactString],
) -> String {
    let mut rest = text;
    let mut out = String::with_capacity(text.len());
    while let Some(at) = specifier_quote(rest) {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        let quote = tail.as_bytes()[0];
        let quoted = &tail[1..];
        let Some(end) = quoted.as_bytes().iter().position(|byte| *byte == quote) else {
            out.push_str(tail);
            return out;
        };
        let specifier = &quoted[..end];
        out.push(char::from(quote));
        match explicit_specifier(specifier, importer, files) {
            Some(rewritten) => out.push_str(&rewritten),
            None => out.push_str(specifier),
        }
        out.push(char::from(quote));
        rest = &quoted[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A project source whose exports are offered as a relative import.
fn is_indexed_source(path: &str) -> bool {
    (path.ends_with(".js") || path.ends_with(".jsx"))
        && path
            .split('/')
            .all(|part| part != "node_modules" && part != "flow-typed")
}

/// The byte index of the opening quote after a `from` keyword.
fn specifier_quote(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 5 < bytes.len() {
        if bytes[index..].starts_with(b"from") {
            let word = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
            let mut cursor = index + 4;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if word && cursor < bytes.len() && (bytes[cursor] == b'"' || bytes[cursor] == b'\'') {
                return Some(cursor);
            }
        }
        index += 1;
    }
    None
}

/// The specifier with its extension restored, when `files` has exactly the
/// file it names.
fn explicit_specifier(specifier: &str, importer: &str, files: &[CompactString]) -> Option<String> {
    if !specifier.starts_with('.') || has_explicit_extension(specifier) {
        return None;
    }
    let dir = parent_dir(importer);
    let resolved = resolve(dir, specifier);
    let candidates = [
        suffixed(&resolved, ".js"),
        suffixed(&resolved, ".jsx"),
        suffixed(&resolved, "/index.js"),
        suffixed(&resolved, "/index.jsx"),
    ];
    candidates.into_iter().find_map(|candidate| {
        files
            .iter()
            .any(|file| file.as_str() == candidate)
            .then(|| relative_specifier(dir, &candidate))
    })
}

fn has_explicit_extension(specifier: &str) -> bool {
    let segment = specifier.rsplit('/').next().unwrap_or(specifier);
    segment.ends_with(".js")
        || segment.ends_with(".jsx")
        || segment.ends_with(".mjs")
        || segment.ends_with(".cjs")
        || segment.ends_with(".json")
}

fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

fn suffixed(path: &str, suffix: &str) -> String {
    let mut out = String::with_capacity(path.len() + suffix.len());
    out.push_str(path);
    out.push_str(suffix);
    out
}

fn resolve(dir: &str, specifier: &str) -> String {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').filter(|part| !part.is_empty()).collect()
    };
    for segment in specifier.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn relative_specifier(from_dir: &str, target: &str) -> String {
    let from: Vec<&str> = from_dir
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
    let mut shared = 0;
    while shared < from.len() && shared < to.len() && from[shared] == to[shared] {
        shared += 1;
    }
    let mut out = String::new();
    let ups = from.len() - shared;
    if ups == 0 {
        out.push_str("./");
    } else {
        out.push_str("..");
        for _ in 1..ups {
            out.push_str("/..");
        }
        if shared < to.len() {
            out.push('/');
        }
    }
    for (index, part) in to[shared..].iter().enumerate() {
        if index > 0 {
            out.push('/');
        }
        out.push_str(part);
    }
    out
}

fn record(index: &mut export_index::ExportIndex, name: &str, source: &Source, kind: Kind) {
    if name.is_empty() {
        return;
    }
    export_index::add(name, source.clone(), kind, index);
}

fn record_statement(
    statement: &ast::statement::Statement<Loc, Loc>,
    source: &Source,
    index: &mut export_index::ExportIndex,
) {
    use ast::statement::ExportKind;
    use ast::statement::StatementInner;
    match &**statement {
        StatementInner::ExportNamedDeclaration { inner, .. } => {
            if let Some(specifiers) = &inner.specifiers {
                record_specifiers(specifiers, source, index);
            }
            if let Some(declaration) = &inner.declaration {
                let kind = match inner.export_kind {
                    ExportKind::ExportType => Kind::NamedType,
                    ExportKind::ExportValue => Kind::Named,
                };
                record_bindings(declaration, kind, source, index);
            }
        }
        StatementInner::ExportDefaultDeclaration { inner, .. } => {
            let ast::statement::export_default_declaration::Declaration::Declaration(declaration) =
                &inner.declaration
            else {
                return;
            };
            record_bindings(declaration, default_kind(declaration), source, index);
        }
        StatementInner::DeclareExportDeclaration { inner, .. } => {
            if let Some(specifiers) = &inner.specifiers {
                record_specifiers(specifiers, source, index);
            }
            if let Some(declaration) = &inner.declaration {
                record_declared(declaration, inner.default.is_some(), source, index);
            }
        }
        _ => {}
    }
}

fn record_specifiers(
    specifiers: &ast::statement::export_named_declaration::Specifier<Loc, Loc>,
    source: &Source,
    index: &mut export_index::ExportIndex,
) {
    use ast::statement::ExportKind;
    use ast::statement::export_named_declaration::Specifier;
    let Specifier::ExportSpecifiers(specifiers) = specifiers else {
        return;
    };
    for specifier in specifiers {
        let name = specifier.exported.as_ref().unwrap_or(&specifier.local);
        let kind = match specifier.export_kind {
            ExportKind::ExportType => Kind::NamedType,
            ExportKind::ExportValue => Kind::Named,
        };
        record(index, name.name.as_str(), source, kind);
    }
}

fn default_kind(statement: &ast::statement::Statement<Loc, Loc>) -> Kind {
    use ast::statement::StatementInner;
    match &**statement {
        StatementInner::TypeAlias { .. }
        | StatementInner::OpaqueType { .. }
        | StatementInner::InterfaceDeclaration { .. }
        | StatementInner::DeclareTypeAlias { .. }
        | StatementInner::DeclareOpaqueType { .. }
        | StatementInner::DeclareInterface { .. } => Kind::DefaultType,
        _ => Kind::Default,
    }
}

fn record_named(
    index: &mut export_index::ExportIndex,
    id: &ast::Identifier<Loc, Loc>,
    source: &Source,
    kind: Kind,
) {
    record(index, id.name.as_str(), source, kind);
}

fn record_bindings(
    statement: &ast::statement::Statement<Loc, Loc>,
    kind: Kind,
    source: &Source,
    index: &mut export_index::ExportIndex,
) {
    use ast::statement::StatementInner;
    match &**statement {
        StatementInner::FunctionDeclaration { inner, .. } => {
            if let Some(id) = &inner.id {
                record_named(index, id, source, kind);
            }
        }
        StatementInner::DeclareFunction { inner, .. } => {
            if let Some(id) = &inner.id {
                record_named(index, id, source, kind);
            }
        }
        StatementInner::ClassDeclaration { inner, .. } => {
            if let Some(id) = &inner.id {
                record_named(index, id, source, kind);
            }
        }
        StatementInner::ComponentDeclaration { inner, .. } => {
            record_named(index, &inner.id, source, kind);
        }
        StatementInner::DeclareComponent { inner, .. } => {
            record_named(index, &inner.id, source, kind);
        }
        StatementInner::DeclareClass { inner, .. } => {
            record_named(index, &inner.id, source, kind);
        }
        StatementInner::TypeAlias { inner, .. }
        | StatementInner::DeclareTypeAlias { inner, .. } => {
            record_named(index, &inner.id, source, kind);
        }
        StatementInner::OpaqueType { inner, .. }
        | StatementInner::DeclareOpaqueType { inner, .. } => {
            record_named(index, &inner.id, source, kind);
        }
        StatementInner::InterfaceDeclaration { inner, .. }
        | StatementInner::DeclareInterface { inner, .. } => {
            record_named(index, &inner.id, source, kind);
        }
        StatementInner::EnumDeclaration { inner, .. }
        | StatementInner::DeclareEnum { inner, .. } => {
            record_named(index, &inner.id, source, kind);
        }
        StatementInner::VariableDeclaration { inner, .. } => {
            record_declarators(&inner.declarations, kind, source, index);
        }
        StatementInner::DeclareVariable { inner, .. } => {
            record_declarators(&inner.declarations, kind, source, index);
        }
        _ => {}
    }
}

fn record_declarators(
    declarations: &[ast::statement::variable::Declarator<Loc, Loc>],
    kind: Kind,
    source: &Source,
    index: &mut export_index::ExportIndex,
) {
    for declarator in declarations {
        let Pattern::Identifier { inner, .. } = &declarator.id else {
            continue;
        };
        record(index, inner.name.name.as_str(), source, kind.clone());
    }
}

fn record_declared(
    declaration: &ast::statement::declare_export_declaration::Declaration<Loc, Loc>,
    defaulted: bool,
    source: &Source,
    index: &mut export_index::ExportIndex,
) {
    use ast::statement::declare_export_declaration::Declaration;
    let typed = matches!(
        declaration,
        Declaration::DefaultType { .. }
            | Declaration::NamedType { .. }
            | Declaration::NamedOpaqueType { .. }
            | Declaration::Interface { .. }
    );
    let kind = match (defaulted, typed) {
        (true, true) => Kind::DefaultType,
        (true, false) => Kind::Default,
        (false, true) => Kind::NamedType,
        (false, false) => Kind::Named,
    };
    match declaration {
        Declaration::Variable { declaration, .. } => {
            record_declarators(&declaration.declarations, kind, source, index);
        }
        Declaration::Function { declaration, .. } => {
            if let Some(id) = &declaration.id {
                record(index, id.name.as_str(), source, kind);
            }
        }
        Declaration::Class { declaration, .. } => {
            record(index, declaration.id.name.as_str(), source, kind);
        }
        Declaration::Component { declaration, .. } => {
            record(index, declaration.id.name.as_str(), source, kind);
        }
        Declaration::NamedType { declaration, .. } => {
            record(index, declaration.id.name.as_str(), source, kind);
        }
        Declaration::NamedOpaqueType { declaration, .. } => {
            record(index, declaration.id.name.as_str(), source, kind);
        }
        Declaration::Interface { declaration, .. } => {
            record(index, declaration.id.name.as_str(), source, kind);
        }
        Declaration::Enum { declaration, .. } => {
            record(index, declaration.id.name.as_str(), source, kind);
        }
        Declaration::DefaultType { .. } | Declaration::Namespace { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::CheckLimits;
    use flow_services_export::export_search::{search_types, search_values};

    fn search(sources: &[(&str, &str)]) -> ExportSearch {
        let options = super::super::options::options_with(&CheckLimits::default(), &[]);
        project_search(sources.iter().copied(), &options)
    }

    fn file_of(source: &Source) -> &str {
        match source {
            Source::FileKey(key) => key.as_str(),
            Source::Global => "global",
            Source::Builtin(_) => "builtin",
        }
    }

    #[test]
    fn named_exports_are_searchable_and_the_rest_are_not() {
        let found = search(&[
            (
                "model.js",
                "// @flow\nexport type User = { name: string };\nexport function greet(user: User): string { return user.name; }\nexport const answer: number = 1;\nexport { answer as reply };\nexport default function Widget(): string { return \"\"; }\n",
            ),
            (
                "barrel.js",
                "// @flow\nexport * from './model.js';\nexport const { left } = { left: 1 };\n",
            ),
            ("plain.js", "export const hidden: number = 1;\n"),
            (
                "node_modules/clock/index.js",
                "// @flow\nexport function tick(): number { return 0; }\n",
            ),
            (
                "opted.js",
                "// @noflow\nexport function secret(): number { return 1; }\n",
            ),
            (
                "script.js",
                "\"use js\";\nexport function scripted(): number { return 1; }\n",
            ),
        ]);

        let values = search_values(None, "greet", &found);
        assert_eq!(values.results.len(), 1);
        assert_eq!(file_of(&values.results[0].search_result.source), "model.js");
        assert!(matches!(values.results[0].search_result.kind, Kind::Named));

        let renamed = search_values(None, "reply", &found);
        assert_eq!(renamed.results.len(), 1);
        assert!(matches!(renamed.results[0].search_result.kind, Kind::Named));

        let widget = search_values(None, "Widget", &found);
        assert_eq!(widget.results.len(), 1);
        assert!(matches!(
            widget.results[0].search_result.kind,
            Kind::Default
        ));

        let user = search_types(None, "User", &found);
        assert_eq!(user.results.len(), 1);
        assert!(matches!(
            user.results[0].search_result.kind,
            Kind::NamedType
        ));
        assert!(search_values(None, "User", &found).results.is_empty());

        assert!(search_values(None, "left", &found).results.is_empty());
        // A file with no pragma is still checked, so its export is offered.
        assert_eq!(search_values(None, "hidden", &found).results.len(), 1);
        assert!(search_values(None, "tick", &found).results.is_empty());
        assert!(search_values(None, "secret", &found).results.is_empty());
        assert!(search_values(None, "scripted", &found).results.is_empty());
    }

    #[test]
    fn a_relative_specifier_gets_its_extension_back() {
        let files = [
            CompactString::const_new("src/app.js"),
            CompactString::const_new("src/model.js"),
            CompactString::const_new("src/ui/index.js"),
            CompactString::const_new("src/view.jsx"),
        ];
        let restored = restore_explicit_extensions(
            "import { greet } from \"./model\";\nimport type { User } from './model';\nimport { tick } from \"clock\";\nimport View from \"./view\";\nimport { Button } from \"./ui\";\nimport { greet } from \"./model.js\";\n",
            "src/app.js",
            &files,
        );
        assert_eq!(
            restored,
            "import { greet } from \"./model.js\";\nimport type { User } from './model.js';\nimport { tick } from \"clock\";\nimport View from \"./view.jsx\";\nimport { Button } from \"./ui/index.js\";\nimport { greet } from \"./model.js\";\n"
        );
    }
}
