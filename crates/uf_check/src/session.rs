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
//! | [`Session::completion`] | `flow_services_autocomplete::autocomplete_service_js` |
//! | [`Session::diagnostics`] | the context's own errors, filtered and printed as `uf check` does |
//! | [`Session::references`] | `flow_services_references::find_refs_js`, over the file asked about and every file that reaches the definition through an import |
//! | [`Session::highlights`] | `find_refs_js::find_local_refs`, over the one file |
//! | [`Session::rename_range`] | `flow_services_references::prepare_rename_searcher` |
//! | [`Session::rename`] | the references, then `rename_mapper` and Flow's AST differ and printer |
//! | [`Session::symbols`] | `flow_lsp_server::document_symbol_provider`, over the parse alone |
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

/// Every place a name is written: its declarations and each use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct References {
    /// Each place, declarations included, in file and then source order.
    pub spans: Vec<Span>,
    /// Which of [`Self::spans`] declare the name, for an editor asked to
    /// leave declarations out.
    pub declarations: Vec<Span>,
}

/// One replacement a rename makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// The text replaced.
    pub span: Span,
    /// What replaces it. Not always the new name alone: renaming the binding
    /// behind a shorthand property `{ user }` writes `{ user: account }`, so
    /// the property keeps its name.
    pub new_text: String,
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
        #[cfg(feature = "upstream-typecheck")]
        {
            let (jobs, worker) = crate::upstream::session::spawn(libs, limits)?;
            Ok(Self {
                jobs: Some(jobs),
                worker: Some(worker),
            })
        }
        #[cfg(not(feature = "upstream-typecheck"))]
        {
            let _ = (libs, limits);
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
    /// Auto-imports are not offered: the session has no index of what the
    /// project exports.
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
