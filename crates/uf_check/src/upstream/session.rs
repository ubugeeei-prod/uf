//! The worker behind [`crate::Session`]: the state it keeps warm, and the
//! port's own services asked over it.
//!
//! Everything here runs on the one thread [`spawn`] starts. See
//! [`crate::session`] for what is kept and why.

use std::cell::LazyCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
use flow_parser_utils::flow_ast_differ;
use flow_parser_utils_output::{js_layout_generator, replacement_printer};
use flow_services_autocomplete::autocomplete_service_js::{
    self, AcOptions, AutocompleteServiceResultGeneric, ac_completion,
};
use flow_services_autocomplete::module_system_info::LspModuleSystemInfo;
use flow_services_autocomplete::{autocomplete_js, autocomplete_sigil};
use flow_services_get_def::find_refs_utils::AstInfo;
use flow_services_get_def::get_def_js::{self, GetDefResult};
use flow_services_get_def::get_def_types::{DefInfo, PropertyDefInfo, Purpose};
use flow_services_get_def::get_def_utils;
use flow_services_references::find_refs_js;
use flow_services_references::find_refs_types::{FindRefsOk, Kind, RefKind, Request, SingleRef};
use flow_services_references::{prepare_rename_searcher, rename_mapper};
use flow_typing::{query_types, type_inference};
use flow_typing_context::Context;
use flow_typing_utils::typed_ast_utils::AvailableAst;
use flow_utils_concurrency::check_budget::CheckBudget;

use super::project::{MkBuiltins, ProjectModules};
use super::{
    BatchEnvironment, Inferred, builtins, convert, infer, job_error, loc_of_aloc, options, parse,
    suppressed,
};
use crate::limits::CHECK_STACK_BYTES;
use crate::session::{
    Completion, CompletionEdit, Completions, Definition, Origin, OwnedSource, References, Rename,
    Symbol, TextEdit, TypeAt,
};
use crate::{CheckError, CheckLimits, Position, Source, Span, TypeDiagnostic};

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
    /// What checking each file said, by batch index, for as long as nothing
    /// it depends on changes.
    ///
    /// Kept apart from [`Self::checked`] and not bounded by it: an editor
    /// re-asks every open file after an edit, because any of them may import
    /// the edited one, and a file whose answer cannot have changed must not
    /// be inferred again just because hovering elsewhere pushed its typed AST
    /// out. A list of diagnostics is small next to the AST it came from.
    diagnosed: HashMap<usize, Rc<[TypeDiagnostic]>>,
    /// What each searched file imports, by batch index: the graph find
    /// references walks backwards from a definition.
    ///
    /// Read from each file's own imports rather than from what inference has
    /// resolved so far, because the files a search has to reach are exactly
    /// the ones nobody has asked about yet. Parsed once per file and dropped
    /// when that file is edited.
    imports: HashMap<usize, Rc<[usize]>>,
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
            diagnosed: HashMap::new(),
            imports: HashMap::new(),
        }
    }

    /// Drop the batch and everything derived from it.
    fn release(&mut self) {
        self.checked.clear();
        self.diagnosed.clear();
        self.imports.clear();
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
        self.diagnosed.retain(|index, _| !stale.contains(index));
        // Only the edited file's own imports can have changed.
        self.imports.remove(&index);
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

    /// What checking the batch's file at `path` reports: the diagnostics
    /// `uf check` gives the same file, suppressions applied.
    ///
    /// [`None`] when there is no inference to report on — the file is not in
    /// the batch, does not parse, or says `@noflow`. A syntax error is the
    /// parser's to report, and the editor already has it from `uf lint`'s
    /// `flow/syntax`; saying it twice would put two markers on one token.
    pub(crate) fn diagnostics(
        &mut self,
        path: &str,
    ) -> Result<Option<Rc<[TypeDiagnostic]>>, CheckError> {
        let Some(modules) = self.modules.clone() else {
            return Ok(None);
        };
        let Some(index) = modules.index_of(path) else {
            return Ok(None);
        };
        if let Some(found) = self.diagnosed.get(&index) {
            return Ok(Some(Rc::clone(found)));
        }
        let Some(checked) = self.checked(path)? else {
            return Ok(None);
        };
        // The same two steps as `check_one`, over the same context, so an
        // editor and `uf check` cannot disagree about a file. They take the
        // suppressions out of the context, so they may run once per
        // inference, and do: an entry leaves `diagnosed` only where `edit` or
        // `release` drops the inference it came from as well.
        let (errors, warnings) = suppressed(
            &checked.inferred.cx,
            &checked.parsed,
            checked
                .inferred
                .cx
                .errors()
                .union(&modules.signature_errors(index)),
            &modules,
        );
        let found: Rc<[TypeDiagnostic]> = convert::diagnostics(&errors, &warnings, path).into();
        self.diagnosed.insert(index, Rc::clone(&found));
        Ok(Some(found))
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

    /// Find references from `at` in `path`: the definition the name leads to,
    /// and every reference to it in `path` and in the files that can see it.
    ///
    /// Flow's own server does this in two steps as well — the file asked
    /// about, then a check of every dependent of the definition's file with
    /// the same request — and the second step is the same call here, over the
    /// batch instead of Flow's heap.
    fn find_references(
        &mut self,
        path: &str,
        at: Position,
        kind: Kind,
        global: bool,
    ) -> Result<Option<(DefInfo, Vec<SingleRef>)>, CheckError> {
        let Some(checked) = self.checked(path)?.filter(|found| within(&found.text, at)) else {
            return Ok(None);
        };
        let loc_of_aloc = self.loc_of_aloc();
        // Only a property through an object literal needs this map, and
        // filling it takes a hook inside inference; see `Session::references`.
        let object_literals = BTreeMap::new();
        let found = find_refs_js::find_local_refs(
            &loc_of_aloc,
            &checked.parsed.file_key,
            &ast_info(&checked.parsed),
            &checked.inferred.cx,
            &checked.inferred.typed_ast,
            &object_literals,
            kind,
            at.line,
            at.column.saturating_sub(1),
        )
        .map_err(|error| job_error(path, error))?;
        let (def_info, local) = match found {
            // A cursor the service cannot place, or a definition it cannot
            // follow: there is nothing to report, as there is for hover.
            Err(_) | Ok((DefInfo::NoDefinition(_), _)) => return Ok(None),
            Ok((def_info, FindRefsOk::FoundReferences(refs))) => (def_info, refs),
            Ok((def_info, FindRefsOk::NoDefinition(_))) => (def_info, Vec::new()),
        };
        let mut refs = local;
        // A private name cannot be written outside its class, so the file
        // asked about has all of them; Flow's server stops here too.
        let private = matches!(
            def_info,
            DefInfo::PropertyDefinition(PropertyDefInfo::PrivateNameProperty { .. })
        );
        if global && !private {
            let request = Request {
                def_info: def_info.clone(),
                kind,
            };
            for other in self.dependents_of(&def_info, checked.index) {
                refs.extend(self.references_in(other, &request)?);
            }
        }
        Ok(Some((def_info, find_refs_js::sort_and_dedup(refs))))
    }

    /// `request` asked of the batch's `index`th file, which is not the one
    /// the cursor is in.
    fn references_in(
        &mut self,
        index: usize,
        request: &Request,
    ) -> Result<Vec<SingleRef>, CheckError> {
        let Some(modules) = self.modules.clone() else {
            return Ok(Vec::new());
        };
        let (path, _) = modules.source(index);
        let Some(checked) = self.checked(&path)? else {
            return Ok(Vec::new());
        };
        let loc_of_aloc = self.loc_of_aloc();
        let found = find_refs_js::local_refs_of_find_ref_request(
            &loc_of_aloc,
            &ast_info(&checked.parsed),
            &checked.inferred.cx,
            &checked.inferred.typed_ast,
            &BTreeMap::new(),
            &checked.parsed.file_key,
            request,
        )
        .map_err(|error| job_error(&path, error))?;
        Ok(match found {
            Ok(FindRefsOk::FoundReferences(refs)) => refs,
            Ok(FindRefsOk::NoDefinition(_)) | Err(_) => Vec::new(),
        })
    }

    /// The files a definition can be referenced from, other than `asked`:
    /// the searched files it is declared in, and every searched file that
    /// reaches one of those through its imports, transitively.
    fn dependents_of(&mut self, def_info: &DefInfo, asked: usize) -> BTreeSet<usize> {
        let Some(modules) = self.modules.clone() else {
            return BTreeSet::new();
        };
        let declared: BTreeSet<usize> = get_def_utils::all_locs_of_def_info(def_info)
            .iter()
            .filter_map(|loc| modules.index_of(loc.source.as_ref()?.as_str()))
            .collect();
        let mut importers: HashMap<usize, Vec<usize>> = HashMap::new();
        for index in 0..modules.len() {
            if !searched(&modules.source(index).0) {
                continue;
            }
            for &target in self.imports_of(&modules, index).iter() {
                importers.entry(target).or_default().push(index);
            }
        }
        let mut found: BTreeSet<usize> = declared.iter().copied().collect();
        let mut frontier: Vec<usize> = declared.into_iter().collect();
        while let Some(target) = frontier.pop() {
            for &importer in importers.get(&target).into_iter().flatten() {
                if found.insert(importer) {
                    frontier.push(importer);
                }
            }
        }
        found.remove(&asked);
        found.retain(|&index| searched(&modules.source(index).0));
        found
    }

    /// The batch files the `index`th one imports, from its own file
    /// signature.
    fn imports_of(&mut self, modules: &Rc<ProjectModules>, index: usize) -> Rc<[usize]> {
        if let Some(found) = self.imports.get(&index) {
            return Rc::clone(found);
        }
        let (path, text) = modules.source(index);
        let file_key = FileKey::new(FileKeyInner::SourceFile(path.to_string()));
        let parsed = parse::parse_file(file_key, &text, &self.options, false);
        let found: Rc<[usize]> = parsed
            .file_sig
            .require_loc_map()
            .keys()
            .filter_map(|specifier| {
                let flow_common::flow_import_specifier::FlowImportSpecifier::Userland(userland) =
                    specifier;
                modules.locate(&path, userland.as_str())
            })
            .collect();
        self.imports.insert(index, Rc::clone(&found));
        found
    }

    pub(crate) fn references(
        &mut self,
        path: &str,
        at: Position,
    ) -> Result<Option<References>, CheckError> {
        let Some((def_info, refs)) = self.find_references(path, at, Kind::FindReferences, true)?
        else {
            return Ok(None);
        };
        let declarations = get_def_utils::all_locs_of_def_info(&def_info);
        Ok(Some(References {
            spans: refs
                .iter()
                .filter_map(|(_, loc)| span_of(loc, path))
                .collect(),
            declarations: declarations
                .iter()
                .filter(|loc| refs.iter().any(|(_, found)| found == *loc))
                .filter_map(|loc| span_of(loc, path))
                .collect(),
        }))
    }

    pub(crate) fn highlights(&mut self, path: &str, at: Position) -> Result<Vec<Span>, CheckError> {
        let Some((_, refs)) = self.find_references(path, at, Kind::FindReferences, false)? else {
            return Ok(Vec::new());
        };
        Ok(refs
            .iter()
            .filter(|(_, loc)| loc.source.as_ref().is_some_and(|key| key.as_str() == path))
            .filter_map(|(_, loc)| span_of(loc, path))
            .collect())
    }

    /// The batch's `index`th file, parsed as it stands now.
    fn parsed(&self, path: &str) -> Option<parse::Parsed> {
        let modules = self.modules.as_ref()?;
        let index = modules.index_of(path)?;
        let (source_path, text) = modules.source(index);
        let file_key = FileKey::new(FileKeyInner::SourceFile(source_path.to_string()));
        Some(parse::parse_file(file_key, &text, &self.options, false))
    }

    pub(crate) fn rename_range(
        &mut self,
        path: &str,
        at: Position,
    ) -> Result<Option<Span>, CheckError> {
        let Some(parsed) = self.parsed(path) else {
            return Ok(None);
        };
        Ok(
            prepare_rename_searcher::search_rename_loc(&parsed.ast, &cursor(&parsed.file_key, at))
                .and_then(|loc| span_of(&loc, path)),
        )
    }

    pub(crate) fn rename(
        &mut self,
        path: &str,
        at: Position,
        new_name: &str,
    ) -> Result<Rename, CheckError> {
        let Some((def_info, refs)) = self.find_references(path, at, Kind::Rename, true)? else {
            return Ok(Rename::Nothing);
        };
        let Some(modules) = self.modules.clone() else {
            return Ok(Rename::Nothing);
        };
        // Every declaration and every use has to be somewhere the rename may
        // edit, or the edits would leave the project half-renamed.
        let outside = get_def_utils::all_locs_of_def_info(&def_info)
            .into_iter()
            .chain(refs.iter().map(|(_, loc)| loc.dupe()))
            .find(|loc| {
                !loc.source.as_ref().is_some_and(|key| {
                    matches!(key.inner(), FileKeyInner::SourceFile(_))
                        && modules.index_of(key.as_str()).is_some()
                        && searched(key.as_str())
                })
            });
        if let Some(loc) = outside {
            return Ok(match span_of(&loc, path) {
                Some(span) => Rename::Outside(span),
                None => Rename::Nothing,
            });
        }
        let mut by_file: BTreeMap<String, BTreeMap<Loc, RefKind>> = BTreeMap::new();
        for (kind, loc) in &refs {
            if let Some(key) = &loc.source {
                by_file
                    .entry(key.as_str().to_owned())
                    .or_default()
                    .insert(loc.dupe(), *kind);
            }
        }
        let mut changes = Vec::new();
        for (file, targets) in &by_file {
            let Some(parsed) = self.parsed(file) else {
                continue;
            };
            let renamed = rename_mapper::rename(true, targets, new_name, &parsed.ast);
            changes.extend(flow_ast_differ::program(&parsed.ast, &renamed));
        }
        let patches = replacement_printer::mk_loc_patch_ast_differ(
            &js_layout_generator::default_opts(),
            &changes,
        );
        Ok(Rename::Edits(
            patches
                .into_iter()
                .filter_map(|(loc, new_text)| {
                    Some(TextEdit {
                        span: span_of(&loc, path)?,
                        new_text,
                    })
                })
                .collect(),
        ))
    }

    pub(crate) fn symbols(&mut self, path: &str) -> Option<Vec<Symbol>> {
        let parsed = self.parsed(path)?;
        Some(
            flow_lsp_server::document_symbol_provider::provide_document_symbols(&parsed.ast)
                .iter()
                .filter_map(|found| symbol(found, path))
                .collect(),
        )
    }
}

/// The parse artifacts the references service reads.
fn ast_info(parsed: &parse::Parsed) -> AstInfo {
    (
        parsed.ast.dupe(),
        parsed.file_sig.dupe(),
        Arc::new(parsed.docblock.clone()),
    )
}

/// Whether find references searches the batch file at `path`, and a rename
/// may edit it: a module of the project's own, not a package under
/// `node_modules` or a manifest.
fn searched(path: &str) -> bool {
    const FLOW_EXTENSIONS: [&str; 5] = [".js", ".jsx", ".mjs", ".cjs", ".flow"];
    FLOW_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
        && !path.split('/').any(|segment| segment == "node_modules")
}

/// One of the provider's outline entries, in uf's words.
///
/// The provider answers in the protocol's ranges, which it made from Flow
/// locations by taking one off the line and keeping the column, so the way
/// back is one onto each.
fn symbol(found: &lsp_types::DocumentSymbol, path: &str) -> Option<Symbol> {
    let span = |range: &lsp_types::Range| -> Option<Span> {
        let position = |at: &lsp_types::Position| -> Option<Position> {
            Some(Position {
                line: at.line.checked_add(1)?,
                column: at.character.checked_add(1)?,
            })
        };
        Some(Span {
            path: CompactString::new(path),
            start: position(&range.start)?,
            end: position(&range.end)?,
        })
    };
    Some(Symbol {
        name: found.name.clone(),
        detail: found.detail.clone(),
        kind: serde_json::to_value(found.kind)
            .ok()?
            .as_u64()
            .and_then(|kind| u32::try_from(kind).ok())?,
        span: span(&found.range)?,
        selection: span(&found.selection_range)?,
        children: found
            .children
            .iter()
            .flatten()
            .filter_map(|child| symbol(child, path))
            .collect(),
    })
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

    /// A worker over `model.js`, which declares `n`, and `app.js`, which
    /// imports it and uses it twice — once in a shorthand property.
    fn project() -> Worker {
        let mut worker = Worker::new(Vec::new(), CheckLimits::default());
        worker
            .load(vec![
                OwnedSource::new(
                    "app.js",
                    "// @flow\nimport { n } from './model.js';\nconst m = n + 1;\nconst o = { n };\n",
                ),
                OwnedSource::new(
                    "model.js",
                    "// @flow\nexport const n: number = 1;\nexport const twice: number = n * 2;\n",
                ),
                OwnedSource::new("other.js", "// @flow\nconst n = 'unrelated';\n"),
                OwnedSource::new(
                    "node_modules/pkg/package.json",
                    r#"{ "name": "pkg", "main": "index.js" }"#,
                ),
                OwnedSource::new(
                    "node_modules/pkg/index.js",
                    "// @flow\nexport const p: number = 1;\n",
                ),
                OwnedSource::new(
                    "uses_pkg.js",
                    "// @flow\nimport { p } from 'pkg';\nconst q = p;\n",
                ),
            ])
            .expect("the batch loads");
        worker
    }

    fn at(line: u32, column: u32) -> Position {
        Position { line, column }
    }

    fn places(spans: &[Span]) -> Vec<(&str, u32, u32, u32)> {
        spans
            .iter()
            .map(|span| {
                (
                    span.path.as_str(),
                    span.start.line,
                    span.start.column,
                    span.end.column,
                )
            })
            .collect()
    }

    #[test]
    fn references_cross_into_the_file_that_declares_the_name_and_back() {
        super::super::on_check_thread("app.js", cross_file_references).expect("runs");
    }

    fn cross_file_references() {
        let mut worker = project();
        // From the use in `app.js`, and from the declaration in `model.js`:
        // the same set either way, and nothing from `other.js`'s own `n`.
        let expected = vec![
            ("app.js", 2, 10, 11),
            ("app.js", 3, 11, 12),
            ("app.js", 4, 13, 14),
            ("model.js", 2, 14, 15),
            ("model.js", 3, 30, 31),
        ];
        for (path, cursor) in [("app.js", at(3, 11)), ("model.js", at(2, 14))] {
            let found = worker
                .references(path, cursor)
                .expect("runs")
                .expect("a definition");
            assert_eq!(places(&found.spans), expected, "from {path}");
            assert_eq!(places(&found.declarations), [("model.js", 2, 14, 15)]);
        }
        worker.release();
    }

    #[test]
    fn references_to_a_property_follow_the_type_across_files() {
        super::super::on_check_thread("app.js", property_references).expect("runs");
    }

    fn property_references() {
        let mut worker = Worker::new(Vec::new(), CheckLimits::default());
        worker
            .load(vec![
                OwnedSource::new(
                    "user.js",
                    "// @flow\nexport type User = { name: string };\nexport const nameOf = (user: User): string => user.name;\n",
                ),
                OwnedSource::new(
                    "app.js",
                    "// @flow\nimport type { User } from './user.js';\nconst shout = (user: User): string => user.name.toUpperCase();\n",
                ),
            ])
            .expect("the batch loads");
        // `name`, in `user.name` in `app.js`.
        let found = worker
            .references("app.js", at(3, 44))
            .expect("runs")
            .expect("a definition");
        assert_eq!(
            places(&found.spans),
            [
                ("app.js", 3, 44, 48),
                ("user.js", 2, 22, 26),
                ("user.js", 3, 52, 56),
            ]
        );
        worker.release();
    }

    #[test]
    fn highlights_stay_in_the_file_asked_about() {
        super::super::on_check_thread("app.js", local_highlights).expect("runs");
    }

    fn local_highlights() {
        let mut worker = project();
        let found = worker.highlights("app.js", at(3, 11)).expect("runs");
        assert_eq!(
            places(&found),
            [
                ("app.js", 2, 10, 11),
                ("app.js", 3, 11, 12),
                ("app.js", 4, 13, 14)
            ]
        );
        assert!(
            worker
                .highlights("app.js", at(1, 3))
                .expect("runs")
                .is_empty()
        );
        worker.release();
    }

    #[test]
    fn a_rename_edits_every_file_and_keeps_a_shorthand_property_named() {
        super::super::on_check_thread("app.js", rename_across_files).expect("runs");
    }

    fn rename_across_files() {
        let mut worker = project();
        let range = worker
            .rename_range("app.js", at(3, 11))
            .expect("runs")
            .expect("on an identifier");
        assert_eq!(places(&[range]), [("app.js", 3, 11, 12)]);
        let Rename::Edits(edits) = worker.rename("app.js", at(3, 11), "count").expect("runs")
        else {
            panic!("a rename");
        };
        let mut written: Vec<(&str, u32, u32, &str)> = edits
            .iter()
            .map(|edit| {
                (
                    edit.span.path.as_str(),
                    edit.span.start.line,
                    edit.span.start.column,
                    edit.new_text.as_str(),
                )
            })
            .collect();
        written.sort_unstable();
        assert_eq!(
            written,
            [
                ("app.js", 2, 10, "count"),
                ("app.js", 3, 11, "count"),
                ("app.js", 4, 13, "n: count"),
                ("model.js", 2, 14, "count"),
                ("model.js", 3, 30, "count"),
            ]
        );
        worker.release();
    }

    #[test]
    fn a_name_declared_in_a_package_is_not_renamed() {
        super::super::on_check_thread("uses_pkg.js", rename_into_a_package).expect("runs");
    }

    fn rename_into_a_package() {
        let mut worker = project();
        let refused = worker.rename("uses_pkg.js", at(3, 11), "r").expect("runs");
        assert!(
            matches!(&refused, Rename::Outside(span) if span.path == "node_modules/pkg/index.js"),
            "{refused:?}"
        );
        worker.release();
    }

    #[test]
    fn symbols_outline_the_file_as_it_is_written() {
        super::super::on_check_thread("shapes.js", outline).expect("runs");
    }

    fn outline() {
        let mut worker = Worker::new(Vec::new(), CheckLimits::default());
        worker
            .load(vec![OwnedSource::new(
                "shapes.js",
                "// @noflow\nexport class Shape {\n  area() { return 0; }\n}\nfunction make() {}\n",
            )])
            .expect("the batch loads");
        let found = worker.symbols("shapes.js").expect("in the batch");
        let names: Vec<(&str, u32, usize)> = found
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.kind, symbol.children.len()))
            .collect();
        // `SymbolKind`: 5 is a class, 12 a function.
        assert_eq!(names, [("Shape", 5, 1), ("make", 12, 0)]);
        let area = &found[0].children[0];
        assert_eq!(area.name, "area");
        assert_eq!(
            places(std::slice::from_ref(&area.selection)),
            [("shapes.js", 3, 3, 7)]
        );
        assert!(worker.symbols("missing.js").is_none());
        worker.release();
    }
}
