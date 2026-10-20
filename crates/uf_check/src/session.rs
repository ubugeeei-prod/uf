//! Positional questions over a type environment that stays warm.
//!
//! [`crate::check_sources`] answers "do the types hold" for a batch and then
//! drops everything it built: the merged dependencies, each file's context,
//! the typed AST. An editor asks a different kind of question — *what* is the
//! type under the cursor, *where* is this defined, *what* can follow this dot
//! — and asks it on mouse-move, so rebuilding that state per question would
//! make every hover a whole check.
//!
//! A [`Session`] keeps it. It holds one batch, the builtin environment it is
//! checked in, every dependency merged so far, and the inference of the few
//! files most recently asked about. A question about a file already inferred
//! costs only the query; one about a new file costs that file's inference
//! against dependencies that are already merged.
//!
//! # What an edit costs
//!
//! [`Session::edit`] replaces one file's text and invalidates exactly what was
//! derived from it: the edited file, and every file that reached it through an
//! import, transitively. Their merged contexts are torn down and rebuilt the
//! next time something imports them; nothing else is touched. So the first
//! question after an edit re-infers the edited file and nothing more — its
//! dependents are re-inferred only when somebody asks about them.
//!
//! # What answers the questions
//!
//! Nothing here reimplements a query. Each is the port's own service, the one
//! `flow type-at-pos`, Flow's language server and `flow_dot_js_wasm` call:
//!
//! | question | upstream |
//! | --- | --- |
//! | [`Session::type_at`] | `flow_typing::query_types::type_at_pos_type`, printed by `ty_printer::string_of_type_at_pos_result` |
//! | [`Session::definition`] | `flow_services_get_def::get_def_js::get_def` |
//! | [`Session::type_definition`] | the symbols `type_at_pos_type` reports for the type it printed |
//! | [`Session::completion`] | `flow_services_autocomplete::autocomplete_service_js`, over an index of the project's own exports so a name from another file can be imported |
//! | [`Session::signature_help`] | `flow_services_type_info::signature_help::find_signatures` |
//! | [`Session::quick_fixes`] | `flow_services_code_action::code_action_service::ast_transforms_of_error`, for a type error whose rewrite stays in the file |
//! | [`Session::diagnostics`] | the context's own errors, filtered and printed as `uf check` does |
//! | [`Session::references`] | `flow_services_references::find_refs_js`, over the file asked about and every file that reaches the definition through an import |
//! | [`Session::highlights`] | `find_refs_js::find_local_refs`, over the one file |
//! | [`Session::rename_range`] | `flow_services_references::prepare_rename_searcher` |
//! | [`Session::rename`] | the references, then `rename_mapper` and Flow's AST differ and printer |
//! | [`Session::symbols`] | `flow_lsp_server::document_symbol_provider`, over the parse alone |
//! | [`Session::workspace_symbols`] | the same outline, flattened across the project's own files |
//! | [`Session::organize_imports`] | `flow_services_code_action::autofix_imports::organize_imports` |
//! | [`Session::selection_range`] | `flow_lsp_server::selection_range_provider`, over the parse alone |
//! | [`Session::linked_editing`] | `flow_services_jsx::linked_editing_jsx::get_linked_locs` |
//! | [`Session::folds`] | the multi-line spans of [`Session::symbols`] |
//!
//! # Threads
//!
//! The port's state is `Rc` all the way down and cannot cross a thread, and it
//! recurses once per level of user-controlled nesting. So a session owns one
//! worker thread, with the stack every check runs on ([`CHECK_STACK_BYTES`]),
//! and every question is sent to it and answered back. [`Session`] itself is
//! `Send`: the caller may live on any thread, and only the worker ever touches
//! the port.
//!
//! In a build that unwinds, a panic inside the port while answering a question
//! is reported as [`CheckError::Worker`] and the session drops its batch rather
//! than keep state that a half-finished query may have left inconsistent; the
//! caller [`Session::load`]s again. The release profiles abort on a panic
//! instead, for a session exactly as for [`crate::check_sources`].
//!
//! [`CHECK_STACK_BYTES`]: crate::CHECK_STACK_BYTES

use std::sync::mpsc;
use std::thread::JoinHandle;

use crate::{CheckError, CheckLimits, Position, Span, TypeDiagnostic};

/// A source the session owns: a project-relative path and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedSource {
    /// The path imports are resolved against, as [`crate::Source::path`].
    pub path: String,
    /// The file's text.
    pub source: String,
}

impl OwnedSource {
    /// A source from its path and text.
    pub fn new(path: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            source: source.into(),
        }
    }
}

/// The type of what is under a position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAt {
    /// The expression, binding or annotation the type is about.
    pub span: Span,
    /// The type, printed as Flow prints it for `flow type-at-pos`: framed as a
    /// declaration (`const n: number`) when the position is on a binding's
    /// name, and bare otherwise.
    pub printed: String,
}

/// Where a definition was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A file in the batch, including a package read from `node_modules`.
    Source,
    /// One of the project's own library definitions, which the session was
    /// started with. [`Span::path`] is the path it was handed in under.
    Library,
    /// A library definition baked into the checker — Flow's `core.js`,
    /// `react.js` and the like. [`Span::path`] is the name the checker gave
    /// it, and there is no file on disk behind it.
    Builtin,
}

/// A definition a name or a type leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// Where it is.
    pub span: Span,
    /// What kind of file that is.
    pub origin: Origin,
}

/// One completion at a position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// What is offered: a property, a binding, a keyword.
    pub label: String,
    /// Its type, printed as Flow, when it has one.
    pub detail: Option<String>,
    /// Its kind, in the Language Server Protocol's `CompletionItemKind`
    /// numbering, which is the vocabulary Flow's service answers in.
    pub kind: Option<u32>,
    /// The edit that inserts it, when the service computed one.
    pub edit: Option<CompletionEdit>,
    /// The key an editor sorts by, when the service ranked the items.
    pub sort_text: Option<String>,
    /// Edits besides the insertion. The import that brings a project export
    /// into this file, when the name is not already in scope.
    pub additional_edits: Vec<TextEdit>,
}

/// The text a completion inserts and the range it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionEdit {
    /// The text to write.
    pub new_text: String,
    /// The range to insert over: the part of the word before the cursor.
    pub insert: Span,
    /// The range to replace over: the whole word the cursor is in.
    pub replace: Span,
}

/// What may be written at a position.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Completions {
    /// The items, in the service's order.
    pub items: Vec<Completion>,
    /// Whether the service says the list is not everything, so that an editor
    /// should ask again as the word grows rather than filter this one.
    pub incomplete: bool,
}

/// The call or component the cursor is inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureHelp {
    /// Each signature the callee can be called with, in the service's order.
    pub signatures: Vec<Signature>,
    /// Which signature [`Self::signatures`] is showing. Flow's service
    /// reports one callee, so this is that callee's first signature.
    pub active_signature: u32,
    /// Which parameter of the active signature the cursor is in, counting
    /// from zero.
    pub active_parameter: u32,
}

/// One way to call the callee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// The whole signature, printed as Flow: `(user: User): string`.
    ///
    /// Each [`Parameter::label`] is a substring of this, which is what the
    /// protocol uses to highlight the active parameter.
    pub label: String,
    /// The documentation the declaration carried, when it had any.
    pub documentation: Option<String>,
    /// The parameters, in source order. A rest parameter is the last one.
    pub parameters: Vec<Parameter>,
}

/// One parameter of a [`Signature`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// `name: Type`, exactly as it appears inside [`Signature::label`].
    pub label: String,
    /// The documentation that parameter's declaration carried, when it had any.
    pub documentation: Option<String>,
}

/// Every place a name is written: its declarations and each use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct References {
    /// Each place, declarations included, in file and then source order.
    pub spans: Vec<Span>,
    /// Which of [`Self::spans`] declare the name, for an editor asked to
    /// leave declarations out.
    pub declarations: Vec<Span>,
}

/// One replacement a rename or a type-error quick fix makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// The text replaced.
    pub span: Span,
    /// What replaces it. Not always the new name alone: renaming the binding
    /// behind a shorthand property `{ user }` writes `{ user: account }`, so
    /// the property keeps its name.
    pub new_text: String,
}

/// A rewrite Flow can make for one type error in the file that was asked about.
///
/// The title is Flow's. [`Self::safe`] is Flow's judgement that the rewrite
/// fixes the error and can run on save. A caller that applies fixes unattended
/// uses that bit; this session does not apply anything itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickFix {
    /// What an editor shows on the lightbulb, in Flow's words.
    pub title: String,
    /// The replacements, in the order Flow's printer emitted them. Every span
    /// is in the file that was asked about.
    pub edits: Vec<TextEdit>,
    /// Flow's `WillFixErrorAndSafeForRunningOnSave`.
    pub safe: bool,
}

/// What renaming the name under a position would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rename {
    /// There is no name there with a definition to rename.
    Nothing,
    /// The edits, across every file in the batch that writes the name.
    Edits(Vec<TextEdit>),
    /// The name is declared, or written, somewhere a rename must not edit: a
    /// library definition, or a package under `node_modules`. This span is
    /// the first such place.
    Outside(Span),
}

/// One entry of a file's outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// What is declared: a function, a class, a variable, a property.
    pub name: String,
    /// What the provider says about it beyond its name, when anything.
    pub detail: Option<String>,
    /// Its kind, in the Language Server Protocol's `SymbolKind` numbering,
    /// which is the vocabulary Flow's provider answers in.
    pub kind: u32,
    /// The whole declaration.
    pub span: Span,
    /// The part an editor selects when the entry is picked: usually the name.
    pub selection: Span,
    /// What is declared inside it: a class's members, an object's properties.
    pub children: Vec<Symbol>,
}

/// One symbol in the project, flattened out of the file that declares it.
///
/// The location is the whole declaration, the same span a document outline
/// gives it. [`Self::container`] is the symbol it is nested in, which is what
/// the protocol calls `containerName`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSymbol {
    /// What is declared.
    pub name: String,
    /// Its kind, in the Language Server Protocol's `SymbolKind` numbering,
    /// which is the vocabulary Flow's provider answers in.
    pub kind: u32,
    /// The symbol it is nested in, when it is nested in one.
    pub container: Option<String>,
    /// The whole declaration.
    pub span: Span,
}

/// One range a cursor can grow to, innermost first.
///
/// [`Self::parent`] is the next range out, the same nesting the protocol's
/// `SelectionRange` uses, so an editor can widen the selection one step at a
/// time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// This range.
    pub span: Span,
    /// The range this one sits inside.
    pub parent: Option<Box<Selection>>,
}

/// A type environment kept warm across questions. See the module header.
pub struct Session {
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<JoinHandle<()>>,
}

#[cfg(feature = "upstream-typecheck")]
type Job = crate::upstream::session::Job;
#[cfg(not(feature = "upstream-typecheck"))]
type Job = ();

impl Session {
    /// Start a session whose batches are checked against `libs`, the
    /// project's own library definitions, in declaration order — the same
    /// order and meaning as [`crate::prepare_builtins`]' argument.
    ///
    /// The builtin environment is merged on the worker when the first batch
    /// is loaded, not here.
    ///
    /// # Errors
    ///
    /// [`CheckError::Unavailable`] without a checker, and
    /// [`CheckError::Worker`] when the worker thread cannot be started.
    pub fn start(libs: Vec<OwnedSource>, limits: CheckLimits) -> Result<Self, CheckError> {
        Self::start_with_lints(libs, limits, Vec::new())
    }

    /// [`Self::start`], running `lints` during inference.
    ///
    /// The same levels [`crate::check_sources_with_lints`] takes, so an editor
    /// and `uf check` report one project's Flow lints.
    ///
    /// # Errors
    ///
    /// [`CheckError::Unavailable`] without a checker, and
    /// [`CheckError::Worker`] when the worker thread cannot be started.
    pub fn start_with_lints(
        libs: Vec<OwnedSource>,
        limits: CheckLimits,
        lints: Vec<crate::FlowLint>,
    ) -> Result<Self, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let (jobs, worker) = crate::upstream::session::spawn(libs, limits, lints)?;
            Ok(Self {
                jobs: Some(jobs),
                worker: Some(worker),
            })
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (libs, limits, lints);
            Err(CheckError::Unavailable)
        }
    }

    /// Make `batch` the set of files questions are answered over, dropping
    /// whatever the session held before.
    ///
    /// The batch is what [`crate::check_sources`] takes: every file a question
    /// may be about, and every file those import — [`crate::module_closure`]
    /// is how a caller assembles one.
    ///
    /// # Errors
    ///
    /// As [`crate::check_sources`], for a problem with the batch as a whole:
    /// a file over [`CheckLimits::max_source_bytes`], or library definitions
    /// that do not merge.
    pub fn load(&self, batch: Vec<OwnedSource>) -> Result<(), CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            self.ask(move |worker| worker.load(batch))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = batch;
            Err(CheckError::Unavailable)
        }
    }

    /// Replace one file's text, and forget what was derived from the old one.
    ///
    /// Returns `false`, and changes nothing, when `path` is not in the batch:
    /// a new file, or one the batch did not reach, needs a new batch
    /// [`Session::load`]ed. A `package.json` is in the batch for what it
    /// publishes, so editing one reloads the batch whole.
    ///
    /// # Errors
    ///
    /// As [`Session::load`], for the reload a manifest edit causes.
    pub fn edit(&self, path: &str, text: String) -> Result<bool, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.edit(&path, text))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, text);
            Err(CheckError::Unavailable)
        }
    }

    /// Whether `path` is in the batch.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn contains(&self, path: &str) -> Result<bool, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| Ok(worker.contains(&path)))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = path;
            Err(CheckError::Unavailable)
        }
    }

    /// The type of what is under `at` in `path`.
    ///
    /// [`None`] when there is nothing typed there — whitespace, a keyword, a
    /// comment — when `path` is not in the batch, and when the file does not
    /// parse or says `@noflow`, since there is no inference to ask then.
    ///
    /// # Errors
    ///
    /// When inference over `path` fails as a whole, as [`crate::check_sources`]
    /// would for the same file.
    pub fn type_at(&self, path: &str, at: Position) -> Result<Option<TypeAt>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.type_at(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// Where the name under `at` is defined: across files in the batch, and
    /// into library definitions.
    ///
    /// An imported name leads to its declaration in the module that exports
    /// it, not to the import. Empty when nothing under `at` has a definition.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`].
    pub fn definition(&self, path: &str, at: Position) -> Result<Vec<Definition>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.definition(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// Where the named types in the type under `at` are declared.
    ///
    /// For `const user: User`, the declaration of `User`; for a value of type
    /// `Array<User>`, both `Array` (a [`Origin::Builtin`]) and `User`. Empty
    /// when the type names nothing — a literal, an object type written inline.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`].
    pub fn type_definition(&self, path: &str, at: Position) -> Result<Vec<Definition>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.type_definition(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// What may be written at `at`: after `value.`, the members of `value`'s
    /// type with their types; elsewhere, the names in scope.
    ///
    /// A name exported by another file of the project is offered with the
    /// relative import that brings it in. A package, an `export *`, and a
    /// destructured binding are not in that index.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`].
    pub fn completion(&self, path: &str, at: Position) -> Result<Option<Completions>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.completion(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// The signature of the call or JSX component the cursor is inside, and
    /// which argument it is on.
    ///
    /// [`None`] when `at` is not inside a call or a component, the file is
    /// not in the batch, or it does not parse. A component's props are the
    /// parameters: the cursor after `<Menu ` is on that component's first
    /// unset prop.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`].
    pub fn signature_help(
        &self,
        path: &str,
        at: Position,
    ) -> Result<Option<SignatureHelp>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.signature_help(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// The rewrites Flow can make for type errors in `path` whose locations
    /// meet `span`.
    ///
    /// Each one is a diff of this file against the program
    /// `ast_transforms_of_error` returns. A rewrite that needs another file's
    /// typed signature, Flow's heap, or a package index is not offered: those
    /// lookups have nothing to read here. A suppressed error is not offered
    /// either. Import order is [`Session::organize_imports`], not one of these
    /// rewrites. [`QuickFix::safe`] is Flow's own judgement; nothing here
    /// applies a fix on save.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`].
    pub fn quick_fixes(&self, path: &str, span: Span) -> Result<Vec<QuickFix>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.quick_fixes(&path, span))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, span);
            Err(CheckError::Unavailable)
        }
    }

    /// What checking `path` reports — the diagnostics [`crate::check_sources`]
    /// gives the same file in the same batch, suppressions applied — over the
    /// text the session holds for it.
    ///
    /// Kept until the file or something it imports is [`Session::edit`]ed, so
    /// asking again about a file nothing changed under costs nothing.
    ///
    /// [`None`] when there is no inference to report on: `path` is not in the
    /// batch, does not parse, or says `@noflow`. Syntax errors are not
    /// repeated here; they are the parser's, and `uf_lint` reports them.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`].
    pub fn diagnostics(&self, path: &str) -> Result<Option<Vec<TypeDiagnostic>>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| {
                Ok(worker
                    .diagnostics(&path)?
                    .map(|found| found.iter().cloned().collect()))
            })
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = path;
            Err(CheckError::Unavailable)
        }
    }

    /// Every place the name under `at` is written: in `path`, and in every
    /// file of the batch that reaches the file declaring it through an import,
    /// transitively.
    ///
    /// Packages under `node_modules` are not searched — a use of a project
    /// name there is not the project's to find — though a declaration there
    /// is reported. [`None`] when nothing under `at` has a definition.
    ///
    /// A property is found through the types the checker gave each access,
    /// with one gap: an object literal checked against an annotated type is
    /// not linked back to that type's property, since that needs Flow's
    /// server-side inference hook.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`], for any file searched.
    pub fn references(&self, path: &str, at: Position) -> Result<Option<References>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.references(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// Every place in `path` itself that the name under `at` is written: what
    /// an editor highlights while the cursor rests on it.
    ///
    /// # Errors
    ///
    /// As [`Session::type_at`].
    pub fn highlights(&self, path: &str, at: Position) -> Result<Vec<Span>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.highlights(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// The identifier under `at` that a rename would replace, before asking
    /// for the new name. [`None`] when the cursor is not on one.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn rename_range(&self, path: &str, at: Position) -> Result<Option<Span>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| worker.rename_range(&path, at))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// Rename the name under `at` to `new_name` everywhere
    /// [`Session::references`] finds it.
    ///
    /// Nothing is written: the answer is the edits. Whether `new_name` is an
    /// identifier is the caller's to check.
    ///
    /// # Errors
    ///
    /// As [`Session::references`].
    pub fn rename(&self, path: &str, at: Position, new_name: &str) -> Result<Rename, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            let new_name = new_name.to_owned();
            self.ask(move |worker| worker.rename(&path, at, &new_name))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at, new_name);
            Err(CheckError::Unavailable)
        }
    }

    /// The outline of `path`: what it declares, nested as it is written.
    ///
    /// Read from the parse alone, so a file that says `@noflow` has one too.
    /// [`None`] when `path` is not in the batch.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn symbols(&self, path: &str) -> Result<Option<Vec<Symbol>>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| Ok(worker.symbols(&path)))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = path;
            Err(CheckError::Unavailable)
        }
    }

    /// Every symbol in the project's own files whose name contains `query`.
    ///
    /// The match is a case-insensitive ASCII substring, and an empty query is
    /// every symbol. Packages under `node_modules` are left out, the same
    /// files [`Session::references`] does not search. The outline is read from
    /// the parse, so a file that says `@noflow` is included.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn workspace_symbols(&self, query: &str) -> Result<Vec<WorkspaceSymbol>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let query = query.to_owned();
            self.ask(move |worker| Ok(worker.workspace_symbols(&query)))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = query;
            Err(CheckError::Unavailable)
        }
    }

    /// The edit that puts `path`'s imports in Flow's order, when they are not
    /// already.
    ///
    /// Type imports, then relative value imports, then value imports from a
    /// module whose name starts with a lowercase letter, then `require`.
    /// Specifiers from one module are combined, and the named ones are sorted.
    /// [`None`] when `path` is not in the batch, has no imports, or the text
    /// would not change.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn organize_imports(&self, path: &str) -> Result<Option<Vec<TextEdit>>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| Ok(worker.organize_imports(&path)))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = path;
            Err(CheckError::Unavailable)
        }
    }

    /// The ranges the selection at `at` can grow through, innermost first.
    ///
    /// Read from the parse. [`None`] when `path` is not in the batch, or `at`
    /// names no place in it.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn selection_range(
        &self,
        path: &str,
        at: Position,
    ) -> Result<Option<Selection>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| Ok(worker.selection_range(&path, at)))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// The JSX names that rename together with the name under `at`.
    ///
    /// A cursor in the opening name or the closing name links the two, and a
    /// fragment's brackets do the same. [`None`] when `at` is not on one, or
    /// `path` is not in the batch.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn linked_editing(
        &self,
        path: &str,
        at: Position,
    ) -> Result<Option<Vec<Span>>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| Ok(worker.linked_editing(&path, at)))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (path, at);
            Err(CheckError::Unavailable)
        }
    }

    /// The ranges in `path` an editor can fold: every multi-line declaration
    /// in its outline, nested ones included.
    ///
    /// A declaration that stays on one line is not a fold. [`None`] when
    /// `path` is not in the batch. An empty list is a file whose outline has
    /// nothing spanning two lines.
    ///
    /// # Errors
    ///
    /// [`CheckError::Worker`] when the worker has gone.
    pub fn folds(&self, path: &str) -> Result<Option<Vec<Span>>, CheckError> {
        #[cfg(feature = "upstream-typecheck")]
        {
            let path = path.to_owned();
            self.ask(move |worker| Ok(worker.folds(&path)))
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = path;
            Err(CheckError::Unavailable)
        }
    }

    /// Run `question` on the worker and wait for its answer.
    #[cfg(feature = "upstream-typecheck")]
    fn ask<T, F>(&self, question: F) -> Result<T, CheckError>
    where
        T: Send + 'static,
        F: FnOnce(&mut crate::upstream::session::Worker) -> Result<T, CheckError> + Send + 'static,
    {
        let gone = || CheckError::Worker {
            path: compact_str::CompactString::const_new("<session>"),
            detail: compact_str::CompactString::const_new("the checker panicked"),
        };
        let (reply, answer) = mpsc::channel();
        let job: Job = Box::new(move |worker| {
            // A caller that stopped waiting is not an error of the worker's.
            let _ = reply.send(question(worker));
        });
        self.jobs
            .as_ref()
            .ok_or_else(gone)?
            .send(job)
            .map_err(|_| gone())?;
        // The reply is dropped unsent when the question panicked.
        answer.recv().map_err(|_| gone())?
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Closing the queue is what ends the worker's loop.
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("running", &self.worker.is_some())
            .finish()
    }
}
