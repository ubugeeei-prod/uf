//! The worker behind [`crate::Session`]: the state it keeps warm, and the
//! port's own services asked over it.
//!
//! Everything here runs on the one thread [`spawn`] starts. See
//! [`crate::session`] for what is kept and why.

use std::cell::LazyCell;
use std::collections::BTreeSet;
use std::panic::AssertUnwindSafe;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;

use compact_str::{CompactString, ToCompactString};
use dupe::Dupe;
use flow_aloc::{ALoc, ALocTable, LazyALocTable};
use flow_common::options::Options;
use flow_common_ty::ty_printer::{self, PrinterOptions, TypeAtPosPrint};
use flow_parser::file_key::{FileKey, FileKeyInner};
use flow_parser::loc::Loc;
use flow_services_autocomplete::autocomplete_service_js::{
    self, AcOptions, AutocompleteServiceResultGeneric, ac_completion,
};
use flow_services_autocomplete::module_system_info::LspModuleSystemInfo;
use flow_services_autocomplete::{autocomplete_js, autocomplete_sigil};
use flow_services_get_def::get_def_js::{self, GetDefResult};
use flow_services_get_def::get_def_types::Purpose;
use flow_typing::{query_types, type_inference};
use flow_typing_context::Context;
use flow_typing_utils::typed_ast_utils::AvailableAst;
use flow_utils_concurrency::check_budget::CheckBudget;

use super::project::{MkBuiltins, ProjectModules};
use super::{BatchEnvironment, Inferred, builtins, infer, job_error, loc_of_aloc, options, parse};
use crate::limits::CHECK_STACK_BYTES;
use crate::session::{
    Completion, CompletionEdit, Completions, Definition, Origin, OwnedSource, TypeAt,
};
use crate::{CheckError, CheckLimits, Position, Source, Span};

/// One question, as the worker runs it.
pub(crate) type Job = Box<dyn FnOnce(&mut Worker) + Send>;

/// How many files' inference the session keeps.
///
/// An editor asks about the file being edited and, now and then, one it
/// jumped to. A handful covers that; the bound is what stops a session that
/// has been walked across a whole project from holding every typed AST in it.
const INFERRED_FILES: usize = 8;

/// How deep a type is normalized before it is printed: what
/// `flow_dot_js_wasm` and `flow type-at-pos` pass.
const MAX_DEPTH: u32 = 40;

/// How much of a type a hover prints before it elides the rest: Flow's own
/// language server's `MAX_TYPE_AT_POS_PRINT_SIZE`.
const MAX_PRINT_SIZE: usize = 100;

/// Start the worker, and the queue its questions arrive on.
pub(crate) fn spawn(
    libs: Vec<OwnedSource>,
    limits: CheckLimits,
) -> Result<(mpsc::Sender<Job>, JoinHandle<()>), CheckError> {
    let (jobs, queue) = mpsc::channel::<Job>();
    let worker = std::thread::Builder::new()
        .name("uf-typecheck-session".to_owned())
        .stack_size(CHECK_STACK_BYTES)
        .spawn(move || {
            let mut worker = Worker::new(libs, limits);
            while let Ok(job) = queue.recv() {
                let answered = std::panic::catch_unwind(AssertUnwindSafe(|| job(&mut worker)));
                if answered.is_err() {
                    // Whatever the port was in the middle of, the batch may
                    // now hold half of it. Dropped rather than trusted.
                    worker.release();
                }
            }
            worker.release();
        })
        .map_err(|error| CheckError::Worker {
            path: CompactString::const_new("<session>"),
            detail: error.to_compact_string(),
        })?;
    Ok((jobs, worker))
}

/// A file's inference, and the text it was inferred from.
struct Checked {
    index: usize,
    text: Rc<str>,
    parsed: Rc<parse::Parsed>,
    inferred: Inferred,
}

/// Everything the session keeps between questions.
pub(crate) struct Worker {
    limits: CheckLimits,
    options: Options,
    libs: Vec<OwnedSource>,
    /// Built with the first batch, and then kept for the session: the libdefs
    /// do not change under it.
    environment: Option<BatchEnvironment>,
    mk_builtins: Option<MkBuiltins>,
    modules: Option<Rc<ProjectModules>>,
    /// The files most recently inferred, most recent first.
    checked: Vec<Rc<Checked>>,
}

impl Worker {
    fn new(libs: Vec<OwnedSource>, limits: CheckLimits) -> Self {
        Self {
            options: options::options(&limits),
            limits,
            libs,
            environment: None,
            mk_builtins: None,
            modules: None,
            checked: Vec::new(),
        }
    }

    /// Drop the batch and everything derived from it.
    fn release(&mut self) {
        self.checked.clear();
        if let Some(modules) = self.modules.take() {
            modules.release();
        }
    }

    pub(crate) fn load(&mut self, batch: Vec<OwnedSource>) -> Result<(), CheckError> {
        self.release();
        for source in &batch {
            if source.source.len() > self.limits.max_source_bytes {
                return Err(CheckError::SourceTooLarge {
                    path: source.path.to_compact_string(),
                    size: source.source.len(),
                    limit: self.limits.max_source_bytes,
                });
            }
        }
        let mk_builtins = self.mk_builtins()?;
        let sources: Vec<Source<'_>> = batch
            .iter()
            .map(|source| Source::new(&source.path, &source.source))
            .collect();
        self.modules = Some(Rc::new(ProjectModules::new(
            &sources,
            self.options.clone(),
            Some(mk_builtins),
            &self.limits,
        )));
        Ok(())
    }

    fn mk_builtins(&mut self) -> Result<MkBuiltins, CheckError> {
        if let Some(mk_builtins) = &self.mk_builtins {
            return Ok(mk_builtins.dupe());
        }
        let libs: Vec<Source<'_>> = self
            .libs
            .iter()
            .map(|lib| Source::new(&lib.path, &lib.source))
            .collect();
        let environment = match &mut self.environment {
            Some(environment) => environment,
            None => self
                .environment
                .insert(BatchEnvironment::new(builtins::prepare(&libs)?)),
        };
        let mk_builtins = environment.mk_builtins(&libs, &self.options)?;
        self.mk_builtins = Some(mk_builtins.dupe());
        Ok(mk_builtins)
    }

    pub(crate) fn contains(&self, path: &str) -> bool {
        self.modules
            .as_ref()
            .is_some_and(|modules| modules.index_of(path).is_some())
    }

    pub(crate) fn edit(&mut self, path: &str, text: String) -> Result<bool, CheckError> {
        let Some(modules) = self.modules.clone() else {
            return Ok(false);
        };
        let Some(index) = modules.index_of(path) else {
            return Ok(false);
        };
        if text.len() > self.limits.max_source_bytes {
            return Err(CheckError::SourceTooLarge {
                path: path.to_compact_string(),
                size: text.len(),
                limit: self.limits.max_source_bytes,
            });
        }
        // A manifest is read once, into the batch's package table, so there is
        // nothing smaller than the batch to invalidate.
        if path == "package.json" || path.ends_with("/package.json") {
            let batch = (0..modules.len())
                .map(|at| {
                    let (source_path, source) = modules.source(at);
                    match at == index {
                        true => OwnedSource::new(source_path.as_str(), text.as_str()),
                        false => OwnedSource::new(source_path.as_str(), &*source),
                    }
                })
                .collect();
            self.load(batch)?;
            return Ok(true);
        }
        let stale = modules.replace(index, &text);
        self.checked
            .retain(|checked| !stale.contains(&checked.index));
        Ok(true)
    }

    /// The inference of the batch's file at `path`, from the session when it
    /// has it.
    ///
    /// [`None`] when the file is not in the batch, does not parse, or opted
    /// out of Flow: there is no inference to ask then.
    fn checked(&mut self, path: &str) -> Result<Option<Rc<Checked>>, CheckError> {
        let Some(modules) = self.modules.clone() else {
            return Ok(None);
        };
        let Some(index) = modules.index_of(path) else {
            return Ok(None);
        };
        if let Some(at) = self.checked.iter().position(|found| found.index == index) {
            let found = self.checked.remove(at);
            self.checked.insert(0, Rc::clone(&found));
            return Ok(Some(found));
        }
        let mk_builtins = self.mk_builtins()?;
        let (source_path, text) = modules.source(index);
        let file_key = FileKey::new(FileKeyInner::SourceFile(source_path.to_string()));
        let parsed = Rc::new(parse::parse_file(file_key, &text, &self.options, false));
        if !parsed.is_parseable() || !parsed.is_checked() {
            return Ok(None);
        }
        let inferred = infer(
            index,
            &self.options,
            &mk_builtins,
            &self.limits,
            &source_path,
            &modules,
            &parsed,
        )?;
        let checked = Rc::new(Checked {
            index,
            text,
            parsed,
            inferred,
        });
        self.checked.insert(0, Rc::clone(&checked));
        self.checked.truncate(INFERRED_FILES);
        Ok(Some(checked))
    }

    /// Resolve an abstract location with every table the batch has built.
    fn loc_of_aloc(&self) -> impl Fn(&ALoc) -> Loc + 'static {
        let tables = self
            .modules
            .as_ref()
            .map(|modules| modules.aloc_tables())
            .unwrap_or_default();
        move |aloc: &ALoc| loc_of_aloc(&tables, aloc)
    }

    pub(crate) fn type_at(
        &mut self,
        path: &str,
        at: Position,
    ) -> Result<Option<TypeAt>, CheckError> {
        let Some(checked) = self.checked(path)?.filter(|found| within(&found.text, at)) else {
            return Ok(None);
        };
        let loc_of_aloc = self.loc_of_aloc();
        let Some((loc, found)) = type_at_pos(&checked, path, at, None)? else {
            return Ok(None);
        };
        let (printed, _) = ty_printer::string_of_type_at_pos_result(
            TypeAtPosPrint::<Loc> {
                ty: &found.ty,
                refs: None,
                binder: found.binder.as_ref(),
            },
            &loc_of_aloc,
            &PrinterOptions {
                size: MAX_PRINT_SIZE,
                ..PrinterOptions::default()
            },
        );
        Ok(span_of(&loc, path).map(|span| TypeAt { span, printed }))
    }

    pub(crate) fn type_definition(
        &mut self,
        path: &str,
        at: Position,
    ) -> Result<Vec<Definition>, CheckError> {
        let Some(checked) = self.checked(path)?.filter(|found| within(&found.text, at)) else {
            return Ok(Vec::new());
        };
        let loc_of_aloc = self.loc_of_aloc();
        let Some((_, found)) = type_at_pos(&checked, path, at, Some(&loc_of_aloc))? else {
            return Ok(Vec::new());
        };
        let locations: BTreeSet<Loc> = found
            .refs
            .into_iter()
            .flatten()
            .map(|symbol| symbol.sym_def_loc)
            .collect();
        Ok(self.definitions(locations))
    }

    pub(crate) fn definition(
        &mut self,
        path: &str,
        at: Position,
    ) -> Result<Vec<Definition>, CheckError> {
        let Some(checked) = self.checked(path)?.filter(|found| within(&found.text, at)) else {
            return Ok(Vec::new());
        };
        let loc_of_aloc = self.loc_of_aloc();
        let cursor = cursor(&checked.parsed.file_key, at);
        let found = get_def_js::get_def(
            &loc_of_aloc,
            &checked.inferred.cx,
            &checked.parsed.file_sig,
            Some(&checked.text),
            checked.parsed.ast.as_ref(),
            AvailableAst::TypedAst(&checked.inferred.typed_ast),
            &Purpose::GoToDefinition,
            &cursor,
        )
        .map_err(|error| job_error(path, error))?;
        Ok(match found {
            GetDefResult::Def(locations, _) | GetDefResult::Partial(locations, _, _) => {
                self.definitions(locations)
            }
            GetDefResult::BadLoc(_) | GetDefResult::DefError(_) => Vec::new(),
        })
    }

    /// Locations as definitions, each told apart by the kind of file it is in.
    fn definitions(&self, locations: BTreeSet<Loc>) -> Vec<Definition> {
        locations
            .into_iter()
            .filter_map(|loc| {
                let origin = match loc.source.as_ref()?.inner() {
                    FileKeyInner::LibFile(name) => {
                        match self.libs.iter().any(|lib| lib.path == *name) {
                            true => Origin::Library,
                            false => Origin::Builtin,
                        }
                    }
                    _ => Origin::Source,
                };
                Some(Definition {
                    span: span_of(&loc, "")?,
                    origin,
                })
            })
            .collect()
    }

    pub(crate) fn completion(
        &mut self,
        path: &str,
        at: Position,
    ) -> Result<Option<Completions>, CheckError> {
        let Some(modules) = self.modules.clone() else {
            return Ok(None);
        };
        let Some(index) = modules.index_of(path) else {
            return Ok(None);
        };
        let mk_builtins = self.mk_builtins()?;
        let loc_of_aloc = self.loc_of_aloc();
        let (source_path, text) = modules.source(index);
        if !within(&text, at) {
            return Ok(None);
        }
        let file_key = FileKey::new(FileKeyInner::SourceFile(source_path.to_string()));
        let cursor = cursor(&file_key, at);
        // The service's own trick: write a placeholder identifier at the
        // cursor, so that `value.` parses as a member expression whose
        // property is being typed.
        let (with_sigil, _, canonical) = autocomplete_sigil::add(
            Some(&file_key),
            &text,
            cursor.start.line,
            usize::try_from(cursor.start.column).unwrap_or(0),
        );
        let canonical_cursor = canonical
            .as_ref()
            .map_or_else(|| cursor.dupe(), |token| token.cursor.dupe());
        autocomplete_js::autocomplete_set_hooks(&canonical_cursor);
        let answer = (|| {
            let parsed = Rc::new(parse::parse_file(
                file_key.dupe(),
                &with_sigil,
                &self.options,
                false,
            ));
            if !parsed.is_parseable() || !parsed.is_checked() {
                return Ok(None);
            }
            // An empty table, as `flow_dot_js_wasm` uses: this text is the
            // file with a placeholder in it, and its locations must not be
            // registered as the file's own.
            let table_key = file_key.dupe();
            let aloc_table: LazyALocTable =
                Rc::new(LazyCell::new(
                    Box::new(move || Rc::new(ALocTable::empty(table_key)))
                        as Box<dyn FnOnce() -> Rc<ALocTable>>,
                ));
            let cx = Context::make(
                Rc::new(flow_typing_context::make_ccx()),
                parsed.metadata.clone(),
                file_key.dupe(),
                Arc::default(),
                aloc_table,
                modules.resolver(&source_path),
                mk_builtins.dupe(),
                CheckBudget::new(self.limits.file_timeout),
            );
            cx.set_merge_dst_cx(&cx);
            let aloc_ast = flow_aloc::loc_to_aloc_ast(parsed.ast.as_ref());
            type_inference::initialize_env(&cx, None, aloc_ast)
                .map_err(|error| job_error(path, error))?;
            let module_system_info = LspModuleSystemInfo {
                file_options: Arc::default(),
                haste_module_system: false,
                get_haste_module_info: Arc::new(|_| None),
                get_package_info: Box::new(|_| None),
                is_package_file: Box::new(|_, _| false),
                node_resolver_root_relative_dirnames: Vec::new(),
                resolves_to_real_path: Box::new(|_, _| false),
            };
            // No export index: auto-imports are not offered.
            let nothing = flow_services_export::export_search_types::SearchResults {
                results: Vec::new(),
                is_incomplete: false,
            };
            let search = |_: &AcOptions, _: &str| nothing.clone();
            let layout = flow_parser_utils_output::js_layout_generator::default_opts();
            let typing = autocomplete_service_js::mk_typing_artifacts(
                &layout,
                Box::new(loc_of_aloc),
                Box::new(|_| None),
                &module_system_info,
                &search,
                &search,
                &cx,
                parsed.file_sig.dupe(),
                parsed.ast.dupe(),
                canonical.as_ref(),
            );
            let (_, _, _, found) = autocomplete_service_js::autocomplete_get_results(
                &typing,
                &AcOptions {
                    imports: false,
                    imports_min_characters: 0,
                    imports_ranked_usage: true,
                    imports_ranked_usage_boost_exact_match_min_length: 5,
                    show_ranking_info: false,
                },
                None,
                &cursor,
            )
            .map_err(|error| job_error(path, error))?;
            Ok(match found {
                AutocompleteServiceResultGeneric::AcResult(found) => Some(Completions {
                    items: found
                        .result
                        .items
                        .iter()
                        .map(|item| completion(item, path))
                        .collect(),
                    incomplete: found.result.is_incomplete,
                }),
                AutocompleteServiceResultGeneric::AcEmpty(_) => Some(Completions::default()),
                AutocompleteServiceResultGeneric::AcFatalError(_) => None,
            })
        })();
        autocomplete_js::autocomplete_unset_hooks();
        answer
    }
}

/// `type_at_pos_type` at `at`, when it found something it could normalize.
fn type_at_pos(
    checked: &Checked,
    path: &str,
    at: Position,
    include_refs: Option<&dyn Fn(&ALoc) -> Loc>,
) -> Result<Option<(Loc, flow_common_ty::ty::TypeAtPosResult)>, CheckError> {
    let found = query_types::type_at_pos_type(
        &checked.inferred.cx,
        checked.parsed.file_sig.dupe(),
        false,
        false,
        MAX_DEPTH,
        &checked.inferred.typed_ast,
        false,
        include_refs,
        cursor(&checked.parsed.file_key, at),
    )
    .map_err(|error| job_error(path, error))?;
    Ok(match found {
        query_types::QueryResult::Success(loc, found) => Some((loc, found)),
        // Something is there, and the normalizer could not say what: the
        // honest answer is none, not the port's internal constructor name.
        query_types::QueryResult::FailureUnparseable(..)
        | query_types::QueryResult::FailureNoMatch => None,
    })
}

/// Whether `at` names a place in `text`: a line it has, and a column no
/// further than just past that line's last byte.
///
/// Asked before the port is, because the port does not ask: a cursor past the
/// end of a line panics inside the typed-AST search, and an editor sends one
/// whenever the pointer rests in the empty space after a short line.
fn within(text: &str, at: Position) -> bool {
    let Some(line) = (at.line as usize)
        .checked_sub(1)
        .and_then(|index| text.split('\n').nth(index))
    else {
        return false;
    };
    at.column >= 1 && (at.column as usize) <= line.trim_end_matches('\r').len() + 1
}

/// A uf position — one-based line, one-based byte column — as the cursor Flow
/// looks up, whose column is zero-based.
fn cursor(file_key: &FileKey, at: Position) -> Loc {
    Loc::cursor(
        Some(file_key.dupe()),
        i32::try_from(at.line).unwrap_or(i32::MAX),
        i32::try_from(at.column.saturating_sub(1)).unwrap_or(i32::MAX),
    )
}

/// A Flow location as a uf span, in `fallback` when it names no file.
///
/// Zero-based columns become one-based, and the end stays exclusive, which is
/// the same conversion diagnostics make.
fn span_of(loc: &Loc, fallback: &str) -> Option<Span> {
    let path = match &loc.source {
        Some(source) => source.as_str(),
        None if !fallback.is_empty() => fallback,
        None => return None,
    };
    Some(Span {
        path: CompactString::new(path),
        start: Position {
            line: u32::try_from(loc.start.line).ok()?,
            column: u32::try_from(loc.start.column).ok()?.saturating_add(1),
        },
        end: Position {
            line: u32::try_from(loc.end.line).ok()?,
            column: u32::try_from(loc.end.column).ok()?.saturating_add(1),
        },
    })
}

/// One of the service's items, in uf's words.
fn completion(item: &ac_completion::CompletionItem, path: &str) -> Completion {
    Completion {
        label: item.name.clone(),
        detail: item.itemDetail.clone(),
        kind: item
            .kind
            .and_then(|kind| serde_json::to_value(kind).ok())
            .and_then(|kind| kind.as_u64())
            .and_then(|kind| u32::try_from(kind).ok()),
        edit: item.text_edit.as_ref().and_then(|edit| {
            Some(CompletionEdit {
                new_text: edit.newText.clone(),
                insert: span_of(&edit.insert, path)?,
                replace: span_of(&edit.replace, path)?,
            })
        }),
        sort_text: item.sort_text.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A worker on this thread, over `app.js` importing `model.js`, and an
    /// `other.js` that imports nothing.
    ///
    /// Driven directly rather than through [`crate::Session`], so the test can
    /// look at what the worker kept; on the check thread, for its stack.
    fn worker() -> Worker {
        let mut worker = Worker::new(Vec::new(), CheckLimits::default());
        worker
            .load(vec![
                OwnedSource::new(
                    "app.js",
                    "// @flow\nimport { n } from './model.js';\nconst m = n;\n",
                ),
                OwnedSource::new("model.js", "// @flow\nexport const n: number = 1;\n"),
                OwnedSource::new("other.js", "// @flow\n\nconst o = 'o';\n"),
            ])
            .expect("the batch loads");
        worker
    }

    fn inferred(worker: &Worker) -> Vec<usize> {
        let mut indices: Vec<usize> = worker.checked.iter().map(|found| found.index).collect();
        indices.sort_unstable();
        indices
    }

    #[test]
    fn an_edit_forgets_the_file_and_what_imports_it_and_nothing_else() {
        super::super::on_check_thread("app.js", edit_forgets).expect("runs");
    }

    fn edit_forgets() {
        let mut worker = worker();
        let at = Position { line: 3, column: 7 };
        assert!(worker.type_at("app.js", at).expect("runs").is_some());
        assert!(worker.type_at("other.js", at).expect("runs").is_some());
        assert_eq!(inferred(&worker), [0, 2]);

        assert!(
            worker
                .edit(
                    "model.js",
                    "// @flow\nexport const n: string = '1';\n".to_owned()
                )
                .expect("the edit applies")
        );

        // `app.js` reached `model.js` and is forgotten; `other.js` did not
        // and is still inferred.
        assert_eq!(inferred(&worker), [2]);
        let again = worker.type_at("app.js", at).expect("runs").expect("typed");
        assert_eq!(again.printed, "const m: string");
        worker.release();
    }

    #[test]
    fn a_question_about_an_inferred_file_does_not_infer_it_again() {
        super::super::on_check_thread("app.js", asked_twice).expect("runs");
    }

    fn asked_twice() {
        let mut worker = worker();
        let at = Position { line: 3, column: 7 };
        worker.type_at("app.js", at).expect("runs");
        let first = Rc::clone(&worker.checked[0]);

        worker.definition("app.js", at).expect("runs");
        worker.type_at("app.js", at).expect("runs");

        assert!(Rc::ptr_eq(&first, &worker.checked[0]));
        worker.release();
    }
}
