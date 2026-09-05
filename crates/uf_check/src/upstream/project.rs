//! Checking across modules: giving an import of another project file a type.
//!
//! # What this is a port of
//!
//! Flow's own checker never types a dependency by checking it. It builds the
//! dependency's *signature* — a compact, annotation-only description of what
//! the module exports, produced by `flow_type_sig` — and merges that signature
//! into the importing file's context. `flow_services_inference`'s
//! `check_service.rs` is where that happens: `dep_module_t` decides what a
//! resolved specifier is, `sig_module_t` merges a checked dependency's exports
//! into the importer with `copy_into`, and `dep_file` turns a packed signature
//! into the lazy `type_sig_merge::File` the merge reads from.
//!
//! This module is that path with the file system taken out. Flow reads
//! signatures from a shared heap that a separate parse phase filled and a merge
//! phase ordered into dependency components; `uf check` is handed a batch of
//! sources in memory and no heap, so the signature is built on demand from the
//! source text and cached for the rest of the batch. Everything downstream of
//! that — `dep_file`'s lazy tables, `merge_exports`, `copy_into` — is
//! upstream's, called rather than reimplemented.
//!
//! # What resolves to what
//!
//! | specifier | resolves to |
//! | --- | --- |
//! | relative, names a source in the batch | that module's signature, as a typed module |
//! | relative, names a source that cannot contribute a signature | an unchecked module, recorded |
//! | relative, names nothing in the batch | an unchecked module, recorded |
//! | bare, declared by Flow's libdefs | that `declare module` block |
//! | bare, anything else | an unchecked module, recorded |
//!
//! A bare specifier is never resolved against the batch, even when a file in it
//! happens to be called `react.js`: a package name resolves through
//! `node_modules`, a workspace, or a `declare module`, and guessing at it from
//! the batch's paths would type an import against a file that is not what the
//! runtime would load.
//!
//! # What is not resolved
//!
//! * **Workspace package specifiers.** `@uniflowed/react` names a package whose
//!   entry point lives behind a `package.json`'s `exports` map, and a batch of
//!   `.js` sources does not contain that `package.json`. These stay unchecked
//!   and stay in [`crate::CheckReport::untyped_modules`].
//! * **A directory's `package.json` `main`.** For the same reason: `./internal`
//!   finds `./internal/index.js` and nothing else.
//!
//! Both are stated here and named in the report rather than being approximated.
//!
//! # Cycles
//!
//! A module cycle terminates on laziness, which is why upstream's merge is
//! built out of thunks. Forcing `a`'s exports builds a *shallow* module type:
//! every member is a signature tvar from `mk_sig_tvar`, so it completes without
//! forcing `b`. Forcing one of those tvars reaches into `b`, whose exports are
//! shallow in the same way, so the descent is one module deep per force rather
//! than one per edge of the cycle. Nothing recurses without bound, and the two
//! re-entrancy guards behind it — `Lazy`'s and `ModuleTypeForcingState`'s —
//! cover what laziness does not.
//!
//! So a cycle where each module only *names* the other's types — the ordinary
//! shape of a package's internals — checks, with both modules fully typed.
//!
//! A cycle that is ill-founded, where a type is defined *as* itself through
//! another file (`export type A = B` against `export type B = A`), resolves to
//! `any`: the two signature tvars unify with each other, nothing concrete ever
//! flows in, and no error is reported. Within one file Flow catches the same
//! shape and reports `recursive-definition`, so this is a hole rather than a
//! decision — a wrong program that is accepted instead of rejected. It is
//! bounded: it needs a type alias that resolves to itself through an import,
//! and it costs a silent `any` rather than a wrong type. Tracked in
//! `a_type_defined_as_itself_across_files_resolves_to_any_instead_of_erroring`.

use std::cell::{LazyCell, OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use compact_str::{CompactString, ToCompactString};
use dupe::Dupe;
use flow_aloc::{ALoc, ALocTable, LazyALocTable, aloc_representation_do_not_use};
use flow_common::flow_import_specifier::{FlowImportSpecifier, Userland};
use flow_common::options::Options;
use flow_common::reason::{self, VirtualReasonDesc::RExports};
use flow_data_structure_wrapper::ord_map::FlowOrdMap;
use flow_data_structure_wrapper::smol_str::FlowSmolStr;
use flow_parser::file_key::{FileKey, FileKeyInner};
use flow_parser::loc::{LOC_NONE, Loc};
use flow_type_sig::compact_table::Index;
use flow_type_sig::packed_type_sig::Module;
use flow_type_sig::type_sig_options::TypeSigOptions;
use flow_type_sig::type_sig_pack as Pack;
use flow_type_sig::type_sig_utils;
use flow_typing::merge;
use flow_typing_builtins::builtins::Builtins;
use flow_typing_context::{Context, Metadata, ResolvedRequire, make_ccx};
use flow_typing_type::type_::constraint::forcing_state::ModuleTypeForcingState;
use flow_typing_type::type_::{ModuleType, Type};
use flow_typing_utils::annotation_inference;
use flow_typing_utils::type_sig_merge::{self, Exports};
use flow_utils_concurrency::check_budget::CheckBudget;

use super::parse;
use super::resolve::{self, ModuleIndex};
use crate::{CheckLimits, Source};

/// The resolver a file's context looks imports up through.
///
/// `Context<'static>` is not a choice: `ResolveRequire<'cx>` is `+ 'cx`, so a
/// resolver for a `'static` context may not borrow. That is why
/// [`ProjectModules`] owns its copy of the batch instead of pointing into the
/// caller's [`Source`]s.
pub(super) type Resolver =
    Rc<dyn Fn(&Context<'static>, &FlowImportSpecifier) -> ResolvedRequire<'static>>;

/// How Flow makes the builtin environment for one context.
pub(super) type MkBuiltins = Rc<dyn Fn(&Context<'static>) -> Builtins<'static, Context<'static>>>;

/// A module's exports, as a thunk the importing context copies from.
type ModuleThunk =
    Rc<dyn Fn(&Context<'static>, &Context<'static>) -> Result<ModuleType, Type> + 'static>;

/// One file's signature: everything the merge needs, and nothing else.
///
/// The AST is deliberately not kept. A dependency is merged from its signature,
/// so once the signature is packed the tree it came from is dead weight — and a
/// batch is a whole project, where holding every AST at once is the difference
/// between a checker that scales and one that does not.
struct Signature {
    file_key: FileKey,
    metadata: Metadata,
    type_sig: Arc<Module<Loc>>,
    aloc_table: LazyALocTable,
}

/// The batch, indexed for resolution, with the signatures it has been asked for.
pub(super) struct ProjectModules {
    /// Paths and text, owned. See [`Resolver`] for why this is a copy.
    sources: Vec<(CompactString, Box<str>)>,
    index: ModuleIndex,
    options: Options,
    /// One builtin environment for the whole batch.
    ///
    /// Shared rather than made per file, and not only to save the merge: a type
    /// crossing from a dependency into the importing file is compared against
    /// the importer's builtins, so two files whose `Array` came from two
    /// separate merges would not agree on it.
    mk_builtins: MkBuiltins,
    /// The wall-clock budget a dependency's merge is charged against when it is
    /// forced outside any importer's own context.
    file_timeout: Option<std::time::Duration>,
    /// [`None`] for a source that has no signature to take: one that did not
    /// parse, or one that said `@noflow`. Both are Flow's own answer — that is
    /// exactly when `check_service` reaches for `unchecked_module_t`.
    signatures: RefCell<HashMap<usize, Option<Rc<Signature>>>>,
    merged: RefCell<HashMap<usize, (type_sig_merge::File<'static>, Context<'static>)>>,
    /// Specifiers that resolved to nothing typed, shared across the batch.
    ///
    /// A `BTreeSet` rather than a counter: the same import in twenty files is
    /// one hole, not twenty, and the sorted order keeps the report
    /// deterministic.
    untyped: RefCell<BTreeSet<CompactString>>,
    /// Every aloc table the batch has built, the checked file's included.
    ///
    /// An error raised while merging a dependency is reported against the file
    /// being checked but can point *into* the dependency, and such a location
    /// is a key into that file's table rather than a line and column. Rendering
    /// it needs the table, and `Context::aloc_tables` only holds the ones from
    /// its own component — so the batch keeps its own map, which is what
    /// `flow_cli`'s own `make_loc_of_aloc` does with the heap.
    aloc_tables: RefCell<HashMap<FileKey, LazyALocTable>>,
}

impl ProjectModules {
    /// Index a batch and prepare the environment its files are checked in.
    pub(super) fn new(
        sources: &[Source<'_>],
        options: Options,
        mk_builtins: MkBuiltins,
        limits: &CheckLimits,
    ) -> Self {
        Self {
            index: ModuleIndex::new(sources.iter().map(|source| source.path)),
            sources: sources
                .iter()
                .map(|source| (source.path.to_compact_string(), Box::from(source.source)))
                .collect(),
            options,
            mk_builtins,
            file_timeout: limits.file_timeout,
            signatures: RefCell::new(HashMap::new()),
            merged: RefCell::new(HashMap::new()),
            untyped: RefCell::new(BTreeSet::new()),
            aloc_tables: RefCell::new(HashMap::new()),
        }
    }

    /// A resolver for the file at `importer`, for [`Context::make`].
    pub(super) fn resolver(self: &Rc<Self>, importer: &str) -> Resolver {
        let modules = Rc::clone(self);
        let importer = importer.to_compact_string();
        Rc::new(
            move |cx: &Context<'static>, specifier: &FlowImportSpecifier| {
                let FlowImportSpecifier::Userland(userland) = specifier;
                modules.resolve(cx, &importer, userland)
            },
        )
    }

    /// The abstract-location table the batch's `index`th source is packed
    /// with, taking `parsed` rather than parsing it again.
    ///
    /// This is the table inference must run the file with, and it is not an
    /// optimisation. A class's nominal identity is an `ALocId`, and
    /// `Context::make_aloc_id` derives one by looking the definition's location
    /// up in *the context's own table*: found, it becomes that table's key;
    /// not found, it stays a line and a column. So a file checked with an empty
    /// table names its classes one way and the same file merged from its
    /// signature names them another — and `QueryCache` stops being assignable
    /// to `QueryCache`. Flow keeps one table per file, made alongside the
    /// signature, and hands it to both; so does this.
    pub(super) fn aloc_table_for(&self, index: usize, parsed: &parse::Parsed) -> LazyALocTable {
        if let Some(Some(signature)) = self.signatures.borrow().get(&index) {
            return signature.aloc_table.dupe();
        }
        let signature = self.pack(parsed);
        let table = signature.aloc_table.dupe();
        self.signatures.borrow_mut().insert(index, Some(signature));
        table
    }

    /// The aloc tables the batch has built so far.
    pub(super) fn aloc_tables(&self) -> HashMap<FileKey, LazyALocTable> {
        self.aloc_tables.borrow().clone()
    }

    /// The specifiers that resolved to nothing typed, sorted and de-duped.
    pub(super) fn untyped_modules(&self) -> Vec<CompactString> {
        self.untyped.borrow().iter().cloned().collect()
    }

    /// Drop everything the merged dependencies hold.
    ///
    /// A dependency's context owns a resolver that owns this batch, so the two
    /// keep each other alive: without breaking the closures the whole graph
    /// outlives the check. Upstream breaks the same cycle when a file falls out
    /// of its LRU cache; this is the end-of-batch version of that.
    pub(super) fn release(&self) {
        for (file, cx) in self.merged.borrow_mut().drain().map(|(_, entry)| entry) {
            cx.post_inference_cleanup();
            file.drop_closures();
        }
        self.signatures.borrow_mut().clear();
    }

    /// What `specifier`, imported from `importer`, resolves to.
    fn resolve(
        self: &Rc<Self>,
        cx: &Context<'static>,
        importer: &str,
        specifier: &Userland,
    ) -> ResolvedRequire<'static> {
        let name = specifier.as_str();
        if resolve::is_relative(name) {
            if let Some(index) = self.index.resolve(importer, name)
                && let Some(signature) = self.signature(index)
            {
                return ResolvedRequire::TypedModule(self.module_thunk(index, &signature));
            }
            return self.unchecked(cx, name);
        }
        match typed_builtin_module(cx, specifier) {
            Some(module) => ResolvedRequire::TypedModule(module),
            None => self.unchecked(cx, name),
        }
    }

    /// Type the import as `any`, and say so.
    ///
    /// Exactly `flow_services_inference`'s `unchecked_module_t`: the module
    /// exists, this check simply has no signature for it, so the import becomes
    /// `any` and the file still checks. Reporting it as *missing* instead would
    /// be a lie, and would bury every real type error under one
    /// `cannot-resolve-module` per import. The name goes into
    /// [`crate::CheckReport::untyped_modules`] so the hole is stated.
    fn unchecked(&self, cx: &Context<'static>, name: &str) -> ResolvedRequire<'static> {
        self.untyped.borrow_mut().insert(name.to_compact_string());
        ResolvedRequire::UncheckedModule(ALoc::of_loc(Loc {
            source: Some(cx.file().dupe()),
            ..LOC_NONE
        }))
    }

    /// The exports of the batch's `index`th source, as a thunk.
    ///
    /// One thunk per call site; the work behind it — packing the signature,
    /// building the merge tables — happens once per module and is cached.
    fn module_thunk(self: &Rc<Self>, index: usize, signature: &Rc<Signature>) -> ModuleThunk {
        let modules = Rc::clone(self);
        let file_key = signature.file_key.dupe();
        Rc::new(move |cx: &Context<'static>, _dst_cx: &Context<'static>| {
            let (file, dep_cx) = modules.merged_file(index);
            cx.add_reachable_dep(file_key.dupe());
            merge::copy_into(&dep_cx, cx, file.exports.as_ref())
        })
    }

    /// The merged dependency for the batch's `index`th source, building it once.
    fn merged_file(
        self: &Rc<Self>,
        index: usize,
    ) -> (type_sig_merge::File<'static>, Context<'static>) {
        if let Some((file, cx)) = self.merged.borrow().get(&index) {
            return (file.dupe(), cx.dupe());
        }
        // Built outside the borrow. `dep_file` only *creates* thunks — it
        // forces nothing — so it cannot re-enter this map, but holding a
        // `RefCell` borrow across a call into the port is a trap worth not
        // setting.
        let signature = self
            .signature(index)
            .expect("a module thunk is only made for a source with a signature");
        let (file, cx) = self.dep_file(&signature);
        self.merged
            .borrow_mut()
            .insert(index, (file.dupe(), cx.dupe()));
        (file, cx)
    }

    /// The signature of the batch's `index`th source, building it once.
    fn signature(&self, index: usize) -> Option<Rc<Signature>> {
        if let Some(cached) = self.signatures.borrow().get(&index) {
            return cached.as_ref().map(Rc::clone);
        }
        // Built outside the borrow: packing a signature resolves nothing and
        // so cannot re-enter, but holding a `RefCell` borrow across a call into
        // the port is a trap worth not setting.
        let built = self.build_signature(index);
        let signature = built.as_ref().map(Rc::clone);
        self.signatures.borrow_mut().insert(index, built);
        signature
    }

    /// Parse one source and pack its signature.
    ///
    /// The AST is dropped when this returns; see [`Signature`].
    fn build_signature(&self, index: usize) -> Option<Rc<Signature>> {
        let (path, source) = &self.sources[index];
        let file_key = FileKey::new(FileKeyInner::SourceFile(path.to_string()));
        let parsed = parse::parse_file(file_key, source, &self.options, false);
        // A file that does not parse has no signature, and one that opted out
        // of Flow has no types in it. Both are reported where they belong —
        // against the file itself, when the batch reaches it — and both are
        // exactly when `check_service` reaches for `unchecked_module_t`.
        if !parsed.is_parseable() || !parsed.is_checked() {
            return None;
        }
        Some(self.pack(&parsed))
    }

    /// Pack one parsed file's signature and register its location table.
    fn pack(&self, parsed: &parse::Parsed) -> Rc<Signature> {
        let file_key = parsed.file_key.dupe();
        let sig_options = TypeSigOptions::of_options(
            &self.options,
            parsed.docblock.prevent_munge(),
            Vec::new(),
            &file_key,
            false,
        );
        let arena = bumpalo::Bump::new();
        let (_signature_errors, locs, type_sig) = type_sig_utils::parse_and_pack_module(
            &sig_options,
            &arena,
            parsed.docblock.is_strict(),
            flow_common::platform_set::available_platforms(
                &self.options.file_options,
                &self.options.projects_options,
                file_key.as_str(),
                parsed.docblock.supports_platform.as_deref(),
            ),
            Some(file_key.dupe()),
            parsed.ast.as_ref(),
        );
        // The signature's own errors are the "this export cannot be given a
        // signature" diagnostics Flow raises on the *defining* file. That file
        // is checked in its own right, where inference reports the same problem
        // against the code that caused it, so reporting them here as well would
        // attach a second copy to whoever imported it.

        let table = Rc::new(aloc_representation_do_not_use::make_table(
            file_key.dupe(),
            locs.into_vec(),
        ));
        let aloc_table: LazyALocTable = Rc::new(LazyCell::new(
            Box::new(move || table) as Box<dyn FnOnce() -> Rc<ALocTable>>
        ));
        self.aloc_tables
            .borrow_mut()
            .insert(file_key.dupe(), aloc_table.dupe());

        Rc::new(Signature {
            file_key,
            metadata: parsed.metadata.clone(),
            type_sig: Arc::new(type_sig),
            aloc_table,
        })
    }

    /// Turn one packed signature into the lazy tables the merge reads.
    ///
    /// A near-verbatim port of `check_service.rs`'s `dep_file`, with the heap
    /// reads replaced by [`Signature`] and the resolved-requires table built
    /// from the signature's own module references rather than from a merge
    /// phase's output. Nothing is forced here: every field is a thunk, which is
    /// what lets a cycle be a cycle.
    fn dep_file(
        self: &Rc<Self>,
        signature: &Rc<Signature>,
    ) -> (type_sig_merge::File<'static>, Context<'static>) {
        let file_key = signature.file_key.dupe();
        let source = Some(file_key.dupe());
        let type_sig = signature.type_sig.dupe();

        let aloc: MkALoc = {
            let source = source.dupe();
            Rc::new(move |i: &Index<Loc>| -> ALoc {
                aloc_representation_do_not_use::make_keyed(source.dupe(), i.as_usize() as u32)
            })
        };

        let resolved_requires: Rc<RefCell<ResolvedRequires>> =
            Rc::new(RefCell::new(BTreeMap::new()));

        let cx: Context<'static> = {
            let resolved_requires = resolved_requires.dupe();
            let modules = Rc::clone(self);
            let importer = signature.file_key.as_str().to_compact_string();
            let resolve_require: flow_typing_context::ResolveRequire<'static> =
                Rc::new(move |cx: &Context<'static>, mref: &FlowImportSpecifier| {
                    // A specifier the signature did not record — Flow panics
                    // here, on the grounds that the merge phase filled the map.
                    // uf fills it from the same signature, so a miss is not
                    // reachable; resolving it directly costs one hash lookup
                    // and turns a would-be abort into the right answer.
                    let lazy = resolved_requires.borrow().get(mref).map(Rc::clone);
                    match lazy {
                        Some(lazy) => lazy.get_forced(cx).dupe(),
                        None => {
                            let FlowImportSpecifier::Userland(userland) = mref;
                            modules.resolve(cx, &importer, userland)
                        }
                    }
                });
            Context::make(
                Rc::new(make_ccx()),
                signature.metadata.clone(),
                file_key.dupe(),
                Arc::default(),
                signature.aloc_table.dupe(),
                resolve_require,
                self.mk_builtins.dupe(),
                CheckBudget::new(self.file_timeout),
            )
        };

        {
            let mut requires = resolved_requires.borrow_mut();
            let importer = signature.file_key.as_str().to_compact_string();
            for mref in type_sig.module_refs.iter() {
                let modules = Rc::clone(self);
                let importer = importer.clone();
                let userland = mref.dupe();
                requires.insert(
                    FlowImportSpecifier::Userland(mref.dupe()),
                    Rc::new(flow_lazy::Lazy::new(Box::new(
                        move |cx: &Context<'static>| modules.resolve(cx, &importer, &userland),
                    ))),
                );
            }
        }

        let dependencies = {
            let resolved_requires = resolved_requires.dupe();
            type_sig.module_refs.map(|mref| {
                let lazy = resolved_requires
                    .borrow()
                    .get(&FlowImportSpecifier::Userland(mref.dupe()))
                    .expect("every module reference was just inserted")
                    .dupe();
                (mref.dupe(), lazy)
            })
        };

        // `Weak`, not `File`: the lazy closures below are stored *in* the file,
        // so a strong reference back to it would be a cycle that never drops.
        let file_cell: Rc<OnceCell<Weak<type_sig_merge::FileInner<'static>>>> =
            Rc::new(OnceCell::new());

        let file_loc = ALoc::of_loc(Loc {
            source: source.dupe(),
            ..LOC_NONE
        });
        let reason = reason::mk_reason(RExports, file_loc);
        let exports = exports_thunk(&cx, &type_sig, &aloc, &file_cell, reason);

        let local_defs = type_sig
            .local_defs
            .map(|def| local_def(&aloc, file_cell.dupe(), def));
        let remote_refs = type_sig
            .remote_refs
            .map(|rref| remote_ref(&aloc, file_cell.dupe(), rref));
        let pattern_defs = type_sig
            .pattern_defs
            .map(|def| pattern_def(&aloc, file_cell.dupe(), def));
        let patterns = type_sig
            .patterns
            .map(|pattern| merged_pattern(&aloc, file_cell.dupe(), pattern));

        let file = type_sig_merge::File::new(
            dependencies,
            exports,
            local_defs,
            remote_refs,
            pattern_defs,
            patterns,
        );
        file_cell
            .set(file.downgrade())
            .unwrap_or_else(|_| unreachable!("the file cell is set exactly once, here"));
        (file, cx)
    }
}

/// The lazily resolved specifiers of one dependency, keyed as the merge asks
/// for them.
type ResolvedRequires = BTreeMap<
    FlowImportSpecifier,
    Rc<
        flow_lazy::Lazy<
            Context<'static>,
            ResolvedRequire<'static>,
            Box<dyn FnOnce(&Context<'static>) -> ResolvedRequire<'static>>,
        >,
    >,
>;

/// Maps a signature-table index onto the abstract location it stands for.
type MkALoc = Rc<dyn Fn(&Index<Loc>) -> ALoc>;

/// A cell holding the file the lazy thunks below belong to, weakly.
type FileCell = Rc<OnceCell<Weak<type_sig_merge::FileInner<'static>>>>;

/// A dependency's module type, forced at most once.
type LazyModuleType = Rc<
    flow_lazy::Lazy<Context<'static>, ModuleType, Box<dyn FnOnce(&Context<'static>) -> ModuleType>>,
>;

/// A lazily merged type.
type LazyType =
    Rc<flow_lazy::Lazy<Context<'static>, Type, Box<dyn FnOnce(&Context<'static>) -> Type>>>;

/// Resolve `name` against Flow's builtin `declare module` blocks.
///
/// This is `check_service.rs`'s `typed_builtin_module_opt`. It is what gives
/// the checker `react`, `react-dom`, and everything else the library
/// definitions declare.
pub(super) fn typed_builtin_module(cx: &Context<'static>, name: &Userland) -> Option<ModuleThunk> {
    cx.builtin_module_opt(name).map(|(reason, lazy_module)| {
        let forcing = ModuleTypeForcingState::of_lazy_module(reason, lazy_module);
        annotation_inference::force_module_type_thunk(cx.dupe(), forcing)
    })
}

/// The module type a dependency's exports merge to, forced at most once.
fn exports_thunk(
    cx: &Context<'static>,
    type_sig: &Arc<Module<Loc>>,
    aloc: &MkALoc,
    file_cell: &FileCell,
    reason: reason::Reason,
) -> ModuleThunk {
    let module_kind = type_sig.module_kind.clone();
    let aloc = aloc.dupe();
    let file_cell = file_cell.dupe();
    let reason_for_forcing = reason.dupe();
    let lazy_module: LazyModuleType = Rc::new(flow_lazy::Lazy::new(Box::new(move |src_cx| {
        let exports = exports_of_module_kind(&module_kind, &aloc, &file_cell, &reason);
        let file = file_of(&file_cell);
        let module = type_sig_merge::merge_exports(src_cx, &file, reason, exports);
        module.get_forced(src_cx).dupe()
    })));
    annotation_inference::force_module_type_thunk(
        cx.dupe(),
        ModuleTypeForcingState::of_lazy_module(reason_for_forcing, lazy_module),
    )
}

/// Split a packed module kind into the export tables `merge_exports` takes.
fn exports_of_module_kind(
    module_kind: &Pack::ModuleKind<Index<Loc>>,
    aloc: &MkALoc,
    file_cell: &FileCell,
    reason: &reason::Reason,
) -> Exports<'static> {
    match module_kind {
        Pack::ModuleKind::CJSModule {
            type_exports,
            exports,
            info,
        } => {
            let Pack::CJSModuleInfo {
                type_export_keys,
                type_stars,
                strict,
                platform_availability_set,
            } = &info.map(&|i| aloc(i));
            Exports::CJSExports {
                type_exports: type_export_map(
                    type_export_keys,
                    type_exports,
                    aloc,
                    file_cell,
                    reason,
                ),
                exports: exports
                    .as_ref()
                    .map(|packed| cjs_export(aloc, file_cell.dupe(), packed)),
                type_stars: type_stars.clone(),
                strict: *strict,
                platform_availability_set: *platform_availability_set,
            }
        }
        Pack::ModuleKind::ESModule {
            type_exports,
            exports,
            ts_pending,
            info,
        } => {
            let Pack::ESModuleInfo {
                type_export_keys,
                type_stars,
                export_keys,
                stars,
                ts_pending_keys,
                strict,
                platform_availability_set,
            } = &info.map(&|i| aloc(i));
            assert_eq!(
                export_keys.len(),
                exports.len(),
                "the signature's export keys and exports must line up"
            );
            assert_eq!(
                ts_pending_keys.len(),
                ts_pending.len(),
                "the signature's pending TypeScript export keys and exports must line up"
            );
            Exports::ESExports {
                type_exports: type_export_map(
                    type_export_keys,
                    type_exports,
                    aloc,
                    file_cell,
                    reason,
                ),
                exports: export_keys
                    .iter()
                    .zip(exports.iter())
                    .map(|(name, export)| (name.dupe(), es_export(aloc, file_cell.dupe(), export)))
                    .collect(),
                ts_pending: ts_pending_keys
                    .iter()
                    .zip(ts_pending.iter())
                    .map(|(name, pending)| {
                        (
                            name.dupe(),
                            ts_pending_export(aloc, file_cell.dupe(), pending),
                        )
                    })
                    .collect(),
                type_stars: type_stars.clone(),
                stars: stars.clone(),
                strict: *strict,
                platform_availability_set: *platform_availability_set,
            }
        }
    }
}

fn type_export_map(
    keys: &[FlowSmolStr],
    type_exports: &[Pack::TypeExport<Index<Loc>>],
    aloc: &MkALoc,
    file_cell: &FileCell,
    reason: &reason::Reason,
) -> BTreeMap<FlowSmolStr, type_sig_merge::LazyExport<'static>> {
    assert_eq!(
        keys.len(),
        type_exports.len(),
        "the signature's type export keys and type exports must line up"
    );
    keys.iter()
        .zip(type_exports.iter())
        .map(|(name, export)| {
            (
                name.dupe(),
                type_export(aloc, file_cell.dupe(), reason.dupe(), export),
            )
        })
        .collect()
}

fn type_export(
    aloc: &MkALoc,
    file_cell: FileCell,
    reason: reason::Reason,
    export: &Pack::TypeExport<Index<Loc>>,
) -> type_sig_merge::LazyExport<'static> {
    let export = export.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let export = export.map(&|i| aloc(i));
            let file = file_of(&file_cell);
            type_sig_merge::merge_type_export(cx, &file, reason, &export)
        },
    )))
}

fn es_export(
    aloc: &MkALoc,
    file_cell: FileCell,
    export: &Pack::Export<Index<Loc>>,
) -> type_sig_merge::LazyExport<'static> {
    let export = export.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let export = export.map(&|i| aloc(i));
            let file = file_of(&file_cell);
            type_sig_merge::merge_export(cx, &file, &export)
        },
    )))
}

fn cjs_export(
    aloc: &MkALoc,
    file_cell: FileCell,
    packed: &Pack::Packed<Index<Loc>>,
) -> type_sig_merge::LazyCjsExport<'static> {
    let packed = packed.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let packed = packed.map(&|i| aloc(i));
            let file = file_of(&file_cell);
            type_sig_merge::merge_cjs_export_t(cx, &file, &packed)
        },
    )))
}

fn ts_pending_export(
    aloc: &MkALoc,
    file_cell: FileCell,
    pending: &Pack::TsPendingExport<Index<Loc>>,
) -> type_sig_merge::LazyTsPendingClassified<'static> {
    let pending = pending.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let pending = pending.map(&|i| aloc(i));
            let file = file_of(&file_cell);
            type_sig_merge::classify_ts_pending_export(cx, &file, &pending)
        },
    )))
}

/// One local definition, as the two lazy types the merge asks it for: the
/// binding's type, and the same type read as a `const` declaration.
#[expect(
    clippy::type_complexity,
    reason = "the tuple is `type_sig_merge::File`'s own local-definition row"
)]
fn local_def(
    aloc: &MkALoc,
    file_cell: FileCell,
    def: &Pack::PackedDef<Index<Loc>>,
) -> Rc<
    flow_lazy::Lazy<
        Context<'static>,
        (
            ALoc,
            FlowSmolStr,
            type_sig_merge::LocalDefBindingKind,
            LazyType,
            LazyType,
        ),
        Box<
            dyn FnOnce(
                &Context<'static>,
            ) -> (
                ALoc,
                FlowSmolStr,
                type_sig_merge::LocalDefBindingKind,
                LazyType,
                LazyType,
            ),
        >,
    >,
> {
    let def = def.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |_cx: &Context<'static>| {
            let def = Rc::new(def.map(
                &mut (),
                |_, loc: &Index<Loc>| aloc(loc),
                |_, packed: &Pack::Packed<Index<Loc>>| packed.map(&|i| aloc(i)),
            ));
            let loc = def.id_loc();
            let name = def.name().dupe();
            let binding_kind = type_sig_merge::def_binding_kind(&def);
            let reason = type_sig_merge::def_reason(&def);
            (
                loc,
                name,
                binding_kind,
                merged_def(file_cell.dupe(), reason.dupe(), def.dupe(), false),
                merged_def(file_cell, reason, def, true),
            )
        },
    )))
}

/// A local definition's type, behind the signature tvar that lets it be
/// referred to before it is merged.
fn merged_def(
    file_cell: FileCell,
    reason: reason::Reason,
    def: Rc<Pack::PackedDef<ALoc>>,
    const_decl: bool,
) -> LazyType {
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let resolved: LazyType = {
                let reason = reason.dupe();
                Rc::new(flow_lazy::Lazy::new(Box::new(
                    move |cx: &Context<'static>| {
                        let file = file_of(&file_cell);
                        type_sig_merge::merge_def(cx, &file, reason, &def, const_decl)
                    },
                )))
            };
            annotation_inference::mk_sig_tvar(cx, reason, resolved)
        },
    )))
}

/// One imported binding, as the merge's remote-reference row.
#[expect(
    clippy::type_complexity,
    reason = "the tuple is `type_sig_merge::File`'s own remote-reference row"
)]
fn remote_ref(
    aloc: &MkALoc,
    file_cell: FileCell,
    rref: &Pack::RemoteRef<Index<Loc>>,
) -> Rc<
    flow_lazy::Lazy<
        Context<'static>,
        (ALoc, FlowSmolStr, Type, LazyType),
        Box<dyn FnOnce(&Context<'static>) -> (ALoc, FlowSmolStr, Type, LazyType)>,
    >,
> {
    let rref = rref.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let remote_ref = rref.map(&|i| aloc(i));
            let loc = remote_ref.loc().dupe();
            let name = remote_ref.name().dupe();
            let reason = type_sig_merge::remote_ref_reason(&remote_ref);

            let resolved: LazyType = {
                let file_cell = file_cell.dupe();
                let reason = reason.dupe();
                let remote_ref = remote_ref.clone();
                Rc::new(flow_lazy::Lazy::new(Box::new(
                    move |cx: &Context<'static>| {
                        let file = file_of(&file_cell);
                        type_sig_merge::merge_remote_ref(cx, &file, reason, &remote_ref)
                    },
                )))
            };
            let t = annotation_inference::mk_sig_tvar(cx, reason.dupe(), resolved);

            let for_extends: LazyType = {
                let reason = reason.dupe();
                Rc::new(flow_lazy::Lazy::new(Box::new(
                    move |cx: &Context<'static>| {
                        let resolved: LazyType = {
                            let reason = reason.dupe();
                            Rc::new(flow_lazy::Lazy::new(Box::new(
                                move |cx: &Context<'static>| {
                                    let file = file_of(&file_cell);
                                    type_sig_merge::merge_remote_ref_for_extends(
                                        cx,
                                        &file,
                                        reason,
                                        &remote_ref,
                                    )
                                },
                            )))
                        };
                        annotation_inference::mk_sig_tvar(cx, reason, resolved)
                    },
                )))
            };
            (loc, name, t, for_extends)
        },
    )))
}

fn pattern_def(aloc: &MkALoc, file_cell: FileCell, def: &Pack::Packed<Index<Loc>>) -> LazyType {
    let def = def.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let def = def.map(&|i| aloc(i));
            let file = file_of(&file_cell);
            type_sig_merge::merge(FlowOrdMap::new(), cx, &file, &def)
        },
    )))
}

fn merged_pattern(
    aloc: &MkALoc,
    file_cell: FileCell,
    pattern: &Pack::Pattern<Index<Loc>>,
) -> LazyType {
    let pattern = pattern.clone();
    let aloc = aloc.dupe();
    Rc::new(flow_lazy::Lazy::new(Box::new(
        move |cx: &Context<'static>| {
            let pattern = pattern.map(&|i| aloc(i));
            let file = file_of(&file_cell);
            type_sig_merge::merge_pattern(cx, &file, &pattern)
        },
    )))
}

/// The file a thunk belongs to.
///
/// The upgrade cannot fail: these closures live inside the file, so the file is
/// alive whenever one of them runs.
fn file_of(file_cell: &FileCell) -> type_sig_merge::File<'static> {
    type_sig_merge::File::from_weak(
        file_cell
            .get()
            .expect("the file cell is set before any thunk can run"),
    )
}
