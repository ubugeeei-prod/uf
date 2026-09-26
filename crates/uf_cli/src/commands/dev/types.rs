//! Types in the editor: type errors, hover, go to definition, go to type
//! definition, completion, find references, document highlights, rename and
//! the document outline, answered by Flow's own inference and services.
//!
//! Every answer here comes from a [`uf_check::Session`] — the project checked
//! the way `uf check` checks it, kept warm for the life of the server. The
//! session owns the port's queries; this module owns only the protocol around
//! them: which document a URI is, how a UTF-16 position becomes a byte column,
//! and how a location in another file becomes a `Location`.
//!
//! # When the project is read
//!
//! The batch is assembled on a thread of its own when the client says
//! `initialized` — which every editor does right after `initialize` — so that
//! scanning the project, walking its imports and merging the builtins happen
//! while the editor is still opening its first file. A question that arrives
//! before it is ready waits for it; a client that never sends `initialized`
//! has it assembled by its first question instead.
//!
//! The batch is the whole project and every module it imports, as `uf check`
//! with no paths would check it, so any file the editor opens is already in
//! it. What the editor holds unsaved is laid over it: an open document's text
//! replaces the file's, and a document the scan did not find joins the batch.
//!
//! # Type errors
//!
//! Every document the editor changes is checked again, and so is every other
//! open document, since any of them may import it — but not at once. The
//! server's loop asks [`Types::settle`] for one document at a time, and only
//! while the pipe is quiet, so a request is never queued behind a check. Most
//! of those re-checks cost nothing: the session keeps each file's diagnostics
//! until something it imports changes. They are the same diagnostics
//! `uf check` prints for the file, published beside the linter's with
//! `source: "flow"`, Flow's error code, and every location the message
//! refers to as related information.
//!
//! # What is not answered
//!
//! * **A file that does not parse**, or says `@noflow`: there is no inference
//!   to ask. The answer is `null`, which an editor shows as nothing.
//! * **Flow's own library definitions** as a definition target. `core.js` and
//!   `react.js` are compiled into the checker and have no file an editor could
//!   open, so a definition that lands only there is `null`.
//! * **A package installed after the batch was assembled.** The batch is read
//!   once; a restart picks the new package up.
//! * **References inside `node_modules`.** A search covers the project's own
//!   files; a package's uses of a project name are not the project's to find
//!   or rename. A rename of a name a package or a library definition declares
//!   is refused rather than half made.
//! * **A property reached only through an object literal** checked against an
//!   annotated type: see [`uf_check::Session::references`].

use std::sync::mpsc;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Value, json};
use uf_check::{
    CheckLimits, Definition, MessageSegment, Origin, OwnedSource, Position, Rename, Session,
    Severity, Span, Symbol, TypeDiagnostic,
};
use uf_config::UniflowedConfig;
use uf_infra::FxHashMap;

use super::{Document, LineIndex, byte_column, character, document_path};

/// The extensions whose documents are asked about: Flow's.
const FLOW_EXTENSIONS: [&str; 5] = ["js", "jsx", "mjs", "cjs", "flow"];

/// The project's session, and what the protocol needs to talk about it.
pub(super) struct Types {
    root: Utf8PathBuf,
    /// The root as the file system resolves it, when that is a different
    /// spelling: macOS's `/var` is `/private/var`, and an editor may name a
    /// document either way.
    canonical_root: Option<Utf8PathBuf>,
    config: UniflowedConfig,
    state: State,
    /// Documents whose type errors are owed to the editor, the one edited
    /// last first.
    pending: Vec<String>,
}

enum State {
    /// Nothing asked for yet.
    Idle,
    /// Being assembled on another thread.
    Preparing(mpsc::Receiver<Result<Prepared, String>>),
    Ready(Prepared),
    /// It could not be assembled; said once on stderr, and then every answer
    /// is `null` rather than a retry per keystroke.
    Failed,
}

struct Prepared {
    session: Session,
    /// The batch as loaded, for a reload that adds a document to it.
    batch: Vec<OwnedSource>,
}

impl Types {
    pub(super) fn new(root: Utf8PathBuf, config: UniflowedConfig) -> Self {
        let canonical_root = root
            .canonicalize_utf8()
            .ok()
            .filter(|canonical| *canonical != root);
        Self {
            root,
            canonical_root,
            config,
            state: State::Idle,
            pending: Vec::new(),
        }
    }

    /// Start assembling the batch, unless that has already started.
    pub(super) fn prepare(&mut self) {
        if !matches!(self.state, State::Idle) {
            return;
        }
        let root = self.root.clone();
        let config = self.config.clone();
        let (done, waiting) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("uf-lsp-types".to_owned())
            .spawn(move || {
                let _ = done.send(assemble(&root, &config));
            });
        self.state = match started {
            Ok(_) => State::Preparing(waiting),
            Err(error) => {
                eprintln!("uf lsp: types are unavailable: {error}");
                State::Failed
            }
        };
    }

    /// The session, once it is ready, with every open document laid over it.
    fn session(&mut self, documents: &FxHashMap<String, Document>) -> Option<&Session> {
        self.prepare();
        if let State::Preparing(waiting) = &self.state {
            let prepared = waiting
                .recv()
                .unwrap_or_else(|_| Err(String::from("the thread assembling it stopped")));
            self.finish(prepared, documents);
        }
        self.ready()
    }

    /// The session if it is ready, without waiting for it: what the pauses
    /// between keystrokes use, since a pause is no reason to block the loop
    /// on a project still being read.
    fn poll(&mut self, documents: &FxHashMap<String, Document>) -> Option<&Session> {
        if let State::Preparing(waiting) = &self.state {
            match waiting.try_recv() {
                Ok(prepared) => self.finish(prepared, documents),
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => self.finish(
                    Err(String::from("the thread assembling it stopped")),
                    documents,
                ),
            }
        }
        self.ready()
    }

    /// The batch has been assembled, or could not be.
    fn finish(
        &mut self,
        prepared: Result<Prepared, String>,
        documents: &FxHashMap<String, Document>,
    ) {
        self.state = match prepared {
            Ok(prepared) => State::Ready(prepared),
            Err(error) => {
                eprintln!("uf lsp: types are unavailable: {error}");
                self.pending.clear();
                State::Failed
            }
        };
        // Everything the editor opened while the batch was being read.
        let mut uris: Vec<&String> = documents.keys().collect();
        uris.sort();
        for uri in uris {
            self.changed(uri, &documents[uri].text);
        }
    }

    fn ready(&self) -> Option<&Session> {
        match &self.state {
            State::Ready(prepared) => Some(&prepared.session),
            State::Idle | State::Preparing(_) | State::Failed => None,
        }
    }

    /// Tell the session what the editor now holds for `uri`.
    ///
    /// Nothing happens before the session is ready: the editor's documents are
    /// laid over the batch when it becomes ready, so a change made while it is
    /// being assembled is not lost.
    ///
    /// Either way the document's type errors are owed again, first in line.
    pub(super) fn changed(&mut self, uri: &str, text: &str) {
        let Some(path) = self.path_of(uri) else {
            return;
        };
        if !matches!(self.state, State::Failed) {
            self.pending.retain(|pending| pending != uri);
            self.pending.insert(0, uri.to_owned());
        }
        let State::Ready(prepared) = &mut self.state else {
            return;
        };
        match prepared.session.edit(&path, text.to_owned()) {
            Ok(true) => {}
            // A file the scan did not find — new and unsaved, or outside what
            // the project collects. It joins the batch, which is a reload.
            Ok(false) => {
                prepared.batch.push(OwnedSource::new(path, text));
                if let Err(error) = prepared.session.load(prepared.batch.clone()) {
                    eprintln!("uf lsp: types: {error}");
                }
            }
            Err(error) => eprintln!("uf lsp: types: {error}"),
        }
    }

    /// Owe the editor the type errors of other open documents too, behind
    /// whatever is already owed: an edit may have changed what they import.
    pub(super) fn recheck<'a>(&mut self, open: impl Iterator<Item = &'a String>) {
        if matches!(self.state, State::Failed) {
            return;
        }
        let mut open: Vec<&String> = open
            .filter(|uri| self.path_of(uri).is_some())
            .filter(|uri| !self.pending.contains(uri))
            .collect();
        open.sort();
        self.pending.extend(open.into_iter().cloned());
    }

    /// Whether type errors are owed and can be worked out: the loop's cue to
    /// wait for a pause rather than for the next message.
    ///
    /// Not before the client has said `initialized` or asked a typed
    /// question — nothing is read before then, and a pause is no reason to
    /// start.
    pub(super) fn waiting(&self) -> bool {
        !self.pending.is_empty() && matches!(self.state, State::Preparing(_) | State::Ready(_))
    }

    /// In a pause: the type errors of the next document owed them, as the
    /// protocol's `Diagnostic`s.
    ///
    /// [`None`] when nothing could be said this time — the project is still
    /// being read, the document was closed, or the check failed, which is
    /// said on stderr once rather than published as a diagnostic about the
    /// user's code. A document that does not parse gets an empty list: its
    /// syntax error is the linter's, and type errors about a previous text
    /// would point at code that is no longer there.
    pub(super) fn settle(
        &mut self,
        documents: &FxHashMap<String, Document>,
    ) -> Option<(String, Vec<Value>)> {
        self.poll(documents)?;
        let uri = loop {
            let uri = self.pending.first()?.clone();
            self.pending.remove(0);
            if documents.contains_key(&uri) {
                break uri;
            }
        };
        let path = self.path_of(&uri)?;
        let found = match self.ready()?.diagnostics(&path) {
            Ok(found) => found.unwrap_or_default(),
            Err(error) => {
                eprintln!("uf lsp: type errors: {error}");
                return None;
            }
        };
        let text = &documents[&uri].text;
        let encoded = found
            .iter()
            .map(|diagnostic| self.encode(documents, text, diagnostic))
            .collect();
        Some((uri, encoded))
    }

    /// One type error as the protocol's `Diagnostic`.
    ///
    /// The message is Flow's, on one line, with each reference numbered as
    /// `uf check` numbers it; `relatedInformation` is where each number
    /// points, labelled with what the message said about it, so an editor can
    /// list them under the error and jump to each.
    fn encode(
        &self,
        documents: &FxHashMap<String, Document>,
        text: &str,
        diagnostic: &TypeDiagnostic,
    ) -> Value {
        let related: Vec<Value> = diagnostic
            .related
            .iter()
            .filter_map(|related| {
                let label = diagnostic
                    .message
                    .iter()
                    .find_map(|segment| match segment {
                        MessageSegment::Reference { text, id } if *id == related.id => {
                            Some(text.as_str())
                        }
                        _ => None,
                    })?;
                Some(json!({
                    "location": self.span_location(documents, &related.span)?,
                    "message": uf_infra::cstr!("[{}] {label}", related.id).into_string(),
                }))
            })
            .collect();
        let mut encoded = json!({
            "range": range_in(text, &diagnostic.primary),
            "severity": match diagnostic.severity {
                Severity::Error => 1,
                Severity::Warning => 2,
            },
            "source": "flow",
            "code": diagnostic.code.unwrap_or(diagnostic.kind.as_str()),
            "message": diagnostic.message_text(),
        });
        if !related.is_empty() {
            encoded["relatedInformation"] = Value::Array(related);
        }
        encoded
    }

    /// The editor closed `uri`: what is on disk is the file again.
    pub(super) fn closed(&mut self, uri: &str) {
        self.pending.retain(|pending| pending != uri);
        if !matches!(self.state, State::Ready(_)) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(document_path(uri)) else {
            return;
        };
        self.changed(uri, &text);
    }

    /// The batch path a document URI names: relative to the root when it is
    /// under it, which is how the scan spells every other file, and absolute
    /// otherwise.
    ///
    /// [`None`] for a document that is not Flow, which is never asked about.
    fn path_of(&self, uri: &str) -> Option<String> {
        let path = Utf8PathBuf::from(document_path(uri));
        if !FLOW_EXTENSIONS.contains(&path.extension()?) {
            return None;
        }
        let roots = || std::iter::once(&self.root).chain(&self.canonical_root);
        if let Some(relative) = roots().find_map(|root| path.strip_prefix(root).ok()) {
            return Some(relative.as_str().to_owned());
        }
        // Under the root through a link the editor did not resolve.
        if let Ok(canonical) = path.canonicalize_utf8()
            && let Some(relative) = roots().find_map(|root| canonical.strip_prefix(root).ok())
        {
            return Some(relative.as_str().to_owned());
        }
        Some(path.into_string())
    }

    /// The document at `uri` and a protocol position in it, as the session's
    /// path and position, with the text the position is in.
    fn locate<'a>(
        &self,
        documents: &'a FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
    ) -> Option<(String, Position, &'a str)> {
        let document = documents.get(uri)?;
        let path = self.path_of(uri)?;
        let text = LineIndex::new(&document.text).line(line);
        let position = Position {
            line: u32::try_from(line + 1).ok()?,
            column: u32::try_from(byte_column(text, requested) + 1).ok()?,
        };
        Some((path, position, &document.text))
    }

    /// `textDocument/hover`: the type under the cursor, printed as Flow.
    pub(super) fn hover(
        &mut self,
        documents: &FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
    ) -> Option<Value> {
        let (path, position, text) = self.locate(documents, uri, line, requested)?;
        let found = match self.session(documents)?.type_at(&path, position) {
            Ok(found) => found?,
            Err(error) => {
                eprintln!("uf lsp: hover: {error}");
                return None;
            }
        };
        Some(json!({
            "contents": {
                "kind": "markdown",
                "value": uf_infra::cstr!("```flow\n{}\n```", found.printed).into_string(),
            },
            "range": range_in(text, &found.span),
        }))
    }

    /// `textDocument/definition`, or `textDocument/typeDefinition` when
    /// `of_type` is set: `Location`s, or `null` when there is none an editor
    /// could open.
    pub(super) fn definition(
        &mut self,
        documents: &FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
        of_type: bool,
    ) -> Value {
        let Some((path, position, _)) = self.locate(documents, uri, line, requested) else {
            return Value::Null;
        };
        let Some(session) = self.session(documents) else {
            return Value::Null;
        };
        let found = match of_type {
            true => session.type_definition(&path, position),
            false => session.definition(&path, position),
        };
        let found = match found {
            Ok(found) => found,
            Err(error) => {
                eprintln!("uf lsp: definition: {error}");
                return Value::Null;
            }
        };
        let locations: Vec<Value> = found
            .iter()
            .filter_map(|definition| self.location(documents, definition))
            .collect();
        match locations.is_empty() {
            true => Value::Null,
            false => Value::Array(locations),
        }
    }

    /// A definition as a `Location`, if it is in a file an editor can open.
    fn location(
        &self,
        documents: &FxHashMap<String, Document>,
        definition: &Definition,
    ) -> Option<Value> {
        if definition.origin == Origin::Builtin {
            return None;
        }
        self.span_location(documents, &definition.span)
    }

    /// A span as a `Location`, if it is in a file an editor can open: one it
    /// holds, or one on disk. A name the checker gave one of its own library
    /// definitions is neither.
    fn span_location(&self, documents: &FxHashMap<String, Document>, span: &Span) -> Option<Value> {
        let (uri, text) = self.file_text(documents, span.path.as_str())?;
        Some(json!({ "uri": uri, "range": range_in(&text, span) }))
    }

    /// The URI of the batch path `path`, and the text positions in it are
    /// counted in: what the editor holds when it has the file open, and the
    /// file on disk otherwise.
    fn file_text(
        &self,
        documents: &FxHashMap<String, Document>,
        path: &str,
    ) -> Option<(String, String)> {
        let path = Utf8Path::new(path);
        let absolute = match path.is_absolute() {
            true => path.to_path_buf(),
            false => self.root.join(path),
        };
        let open = documents.iter().find_map(|(open, document)| {
            (document_path(open) == absolute.as_str()).then_some(document.text.clone())
        });
        let text = open.or_else(|| std::fs::read_to_string(&absolute).ok())?;
        Some((file_uri(&absolute), text))
    }

    /// `textDocument/references`: `Location`s across the project, or `null`
    /// when nothing under the cursor has a definition.
    pub(super) fn references(
        &mut self,
        documents: &FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
        include_declaration: bool,
    ) -> Value {
        let Some((path, position, _)) = self.locate(documents, uri, line, requested) else {
            return Value::Null;
        };
        let Some(session) = self.session(documents) else {
            return Value::Null;
        };
        let found = match session.references(&path, position) {
            Ok(Some(found)) => found,
            Ok(None) => return Value::Null,
            Err(error) => {
                eprintln!("uf lsp: references: {error}");
                return Value::Null;
            }
        };
        Value::Array(
            found
                .spans
                .iter()
                .filter(|span| include_declaration || !found.declarations.contains(span))
                .filter_map(|span| self.span_location(documents, span))
                .collect(),
        )
    }

    /// `textDocument/documentHighlight`: every place in this document the
    /// name under the cursor is written.
    pub(super) fn highlights(
        &mut self,
        documents: &FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
    ) -> Value {
        let Some((path, position, text)) = self.locate(documents, uri, line, requested) else {
            return Value::Null;
        };
        let Some(session) = self.session(documents) else {
            return Value::Null;
        };
        match session.highlights(&path, position) {
            // `kind: 1` is `Text`: Flow's service does not tell a read from a
            // write, and saying which would be a guess.
            Ok(found) => Value::Array(
                found
                    .iter()
                    .map(|span| json!({ "range": range_in(text, span), "kind": 1 }))
                    .collect(),
            ),
            Err(error) => {
                eprintln!("uf lsp: document highlight: {error}");
                Value::Null
            }
        }
    }

    /// `textDocument/prepareRename`: the identifier a rename would replace,
    /// or `null` when the cursor is not on one.
    pub(super) fn prepare_rename(
        &mut self,
        documents: &FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
    ) -> Value {
        let Some((path, position, text)) = self.locate(documents, uri, line, requested) else {
            return Value::Null;
        };
        let Some(session) = self.session(documents) else {
            return Value::Null;
        };
        match session.rename_range(&path, position) {
            Ok(Some(span)) => range_in(text, &span),
            Ok(None) => Value::Null,
            Err(error) => {
                eprintln!("uf lsp: prepare rename: {error}");
                Value::Null
            }
        }
    }

    /// `textDocument/rename`: a `WorkspaceEdit` over every file that writes
    /// the name, `null` when there is nothing to rename, or the reason the
    /// rename is refused.
    pub(super) fn rename(
        &mut self,
        documents: &FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
        new_name: &str,
    ) -> Result<Value, String> {
        if !is_identifier(new_name) {
            return Err(uf_infra::cstr!("`{new_name}` is not an identifier").into_string());
        }
        let Some((path, position, _)) = self.locate(documents, uri, line, requested) else {
            return Ok(Value::Null);
        };
        let Some(session) = self.session(documents) else {
            return Ok(Value::Null);
        };
        let edits = match session.rename(&path, position, new_name) {
            Ok(Rename::Edits(edits)) => edits,
            Ok(Rename::Nothing) => return Ok(Value::Null),
            Ok(Rename::Outside(span)) => {
                return Err(uf_infra::cstr!(
                    "the name is written in {}, which a rename does not edit",
                    span.path,
                )
                .into_string());
            }
            Err(error) => return Err(error.to_string()),
        };
        let mut changes = serde_json::Map::new();
        for edit in &edits {
            let Some((uri, text)) = self.file_text(documents, edit.span.path.as_str()) else {
                return Err(uf_infra::cstr!("{} could not be read", edit.span.path).into_string());
            };
            let entry = changes
                .entry(uri)
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(list) = entry {
                list.push(json!({
                    "range": range_in(&text, &edit.span),
                    "newText": edit.new_text,
                }));
            }
        }
        Ok(json!({ "changes": changes }))
    }

    /// `textDocument/documentSymbol`: the document's outline, nested as it is
    /// written, or `null` for a document that is not Flow.
    pub(super) fn symbols(&mut self, documents: &FxHashMap<String, Document>, uri: &str) -> Value {
        let Some(document) = documents.get(uri) else {
            return Value::Null;
        };
        let Some(path) = self.path_of(uri) else {
            return Value::Null;
        };
        let Some(session) = self.session(documents) else {
            return Value::Null;
        };
        match session.symbols(&path) {
            Ok(Some(found)) => Value::Array(
                found
                    .iter()
                    .map(|symbol| encode_symbol(&document.text, symbol))
                    .collect(),
            ),
            Ok(None) => Value::Null,
            Err(error) => {
                eprintln!("uf lsp: document symbols: {error}");
                Value::Null
            }
        }
    }

    /// `textDocument/completion` in a Flow document: after `value.`, the
    /// members of `value`'s type with their types; elsewhere, the names in
    /// scope.
    pub(super) fn completion(
        &mut self,
        documents: &FxHashMap<String, Document>,
        uri: &str,
        line: usize,
        requested: usize,
    ) -> Value {
        let Some((path, position, text)) = self.locate(documents, uri, line, requested) else {
            return Value::Null;
        };
        let Some(session) = self.session(documents) else {
            return Value::Null;
        };
        let found = match session.completion(&path, position) {
            Ok(Some(found)) => found,
            Ok(None) => return Value::Null,
            Err(error) => {
                eprintln!("uf lsp: completion: {error}");
                return Value::Null;
            }
        };
        let items: Vec<Value> = found
            .items
            .iter()
            .map(|item| {
                let mut encoded = json!({ "label": item.label });
                if let Some(kind) = item.kind {
                    encoded["kind"] = json!(kind);
                }
                if let Some(detail) = &item.detail {
                    encoded["detail"] = json!(detail);
                }
                if let Some(sort) = &item.sort_text {
                    encoded["sortText"] = json!(sort);
                }
                if let Some(edit) = &item.edit {
                    encoded["textEdit"] = json!({
                        "range": range_in(text, &edit.replace),
                        "newText": edit.new_text,
                    });
                }
                encoded
            })
            .collect();
        json!({ "isIncomplete": found.incomplete, "items": items })
    }
}

/// Assemble the project's batch and load a session with it.
///
/// The same scan `uf lint` makes and the same batch `uf check` checks, so a
/// hover is answered against exactly the modules a check is.
fn assemble(root: &Utf8Path, config: &UniflowedConfig) -> Result<Prepared, String> {
    let scanned =
        crate::commands::lint::project_sources(root, config).map_err(|error| error.to_string())?;
    let project =
        crate::commands::check::project_batch(root, &scanned).map_err(|error| error.to_string())?;
    let owned = |files: Vec<uf_lint::SourceFile>| -> Vec<OwnedSource> {
        files
            .into_iter()
            .map(|file| OwnedSource::new(file.path, file.source))
            .collect()
    };
    let session = Session::start(owned(project.libs), CheckLimits::default())
        .map_err(|error| error.to_string())?;
    let batch = owned(project.sources);
    session
        .load(batch.clone())
        .map_err(|error| error.to_string())?;
    Ok(Prepared { session, batch })
}

/// One outline entry as the protocol's `DocumentSymbol`.
fn encode_symbol(text: &str, symbol: &Symbol) -> Value {
    let mut encoded = json!({
        "name": symbol.name,
        "kind": symbol.kind,
        "range": range_in(text, &symbol.span),
        "selectionRange": range_in(text, &symbol.selection),
    });
    if let Some(detail) = &symbol.detail {
        encoded["detail"] = json!(detail);
    }
    if !symbol.children.is_empty() {
        encoded["children"] = symbol
            .children
            .iter()
            .map(|child| encode_symbol(text, child))
            .collect();
    }
    encoded
}

/// Whether `name` can be written where a binding's name is: an identifier,
/// and not a word the language reserves.
///
/// Asked before the rename is, because the service renames to whatever it is
/// handed, and `const class = 1` is a syntax error in every file it touches.
fn is_identifier(name: &str) -> bool {
    const RESERVED: [&str; 38] = [
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "new",
        "null",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
    ];
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '$' || first == '_')
        && chars.all(|next| {
            next.is_alphanumeric() || matches!(next, '$' | '_' | '\u{200c}' | '\u{200d}')
        })
        && !RESERVED.contains(&name)
}

/// A span as a protocol range over `text`: one-based byte columns become
/// zero-based UTF-16 characters.
fn range_in(text: &str, span: &Span) -> Value {
    let index = LineIndex::new(text);
    let position = |at: Position| {
        let line = at.line.saturating_sub(1) as usize;
        json!({
            "line": line,
            "character": character(index.line(line), at.column as usize),
        })
    };
    json!({ "start": position(span.start), "end": position(span.end) })
}

/// A `file://` URI for an absolute path, escaping what a URI path may not
/// hold as written.
fn file_uri(path: &Utf8Path) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut uri = String::from("file://");
    for &byte in path.as_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(byte));
            }
            _ => {
                uri.push('%');
                uri.push(char::from(HEX[usize::from(byte >> 4)]));
                uri.push(char::from(HEX[usize::from(byte & 0xf)]));
            }
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_becomes_a_uri_the_editor_can_decode_back() {
        let path = Utf8Path::new("/My Project/src/π.js");
        let uri = file_uri(path);

        assert_eq!(uri, "file:///My%20Project/src/%CF%80.js");
        assert_eq!(document_path(&uri), path.as_str());
    }

    #[test]
    fn a_span_becomes_a_range_in_utf16_units() {
        // `🦀` is four bytes and two UTF-16 units.
        let text = "// @flow\nconst s = \"🦀\"; const n = 1;\n";
        let span = Span {
            path: "a.js".into(),
            start: Position {
                line: 2,
                column: 25,
            },
            end: Position {
                line: 2,
                column: 26,
            },
        };

        assert_eq!(
            range_in(text, &span),
            json!({
                "start": { "line": 1, "character": 22 },
                "end": { "line": 1, "character": 23 },
            })
        );
    }

    #[test]
    fn a_rename_is_to_an_identifier_and_not_to_a_reserved_word() {
        for name in ["count", "_private", "$el", "π", "camelCase2"] {
            assert!(is_identifier(name), "{name}");
        }
        for name in ["", "2fast", "class", "a-b", "a b", "#x"] {
            assert!(!is_identifier(name), "{name}");
        }
    }

    #[test]
    fn a_document_under_the_root_is_asked_about_by_its_project_path() {
        let types = Types::new(Utf8PathBuf::from("/project"), UniflowedConfig::default());

        assert_eq!(
            types.path_of("file:///project/src/a.js").as_deref(),
            Some("src/a.js")
        );
        assert_eq!(
            types.path_of("file:///elsewhere/b.js").as_deref(),
            Some("/elsewhere/b.js")
        );
        // Not Flow: never asked about.
        assert_eq!(types.path_of("file:///project/styles.css"), None);
    }
}
