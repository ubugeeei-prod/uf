//! The checker itself, driven from Meta's official Flow Rust port.
//!
//! This is `flow_dot_js_wasm`'s check path with five things changed: the
//! builtin environment is merged once and shared, an import of another file in
//! the batch resolves to that file's signature the way
//! `flow_services_inference` resolves one to the heap's (see [`project`]), the
//! work runs on a thread with enough stack for user-controlled recursion, a
//! file whose answer is already on disk is not inferred again (see
//! [`crate::cache`] and [`graph`]), and the result comes back as
//! [`TypeDiagnostic`]s instead of JSON.
//!
//! Nothing below this module is allowed to leak upstream's types: everything
//! the crate exposes is `uf`'s own, so the shape of the port stays an
//! implementation detail of this one directory.

// The port's API is `Rc` and `Arc` all the way down — `Context` is built from
// `Rc` closures and `MasterContext` is shared through an `Arc` — so the
// workspace's "prefer references or arena allocation" policy has nothing to
// bite on here. The reference counting is upstream's, not ours; it stops at the
// module boundary, and no `Rc` or `Arc` appears in this crate's public API.
#![allow(
    clippy::disallowed_types,
    reason = "the upstream Flow port's own API is Rc- and Arc-based"
)]

mod assets;
mod builtins;
mod closure;
mod convert;
mod environments;
mod graph;
mod options;
mod packages;
mod parse;
mod project;
mod resolve;

pub(crate) use convert::error_code;

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use compact_str::{CompactString, ToCompactString};
use dupe::{Dupe, OptionDupedExt};
use flow_aloc::{ALoc, LazyALocTable, aloc_representation_do_not_use};
use flow_common::options::Options;
use flow_common_errors::error_utils::ConcreteLocPrintableErrorSet;
use flow_parser::ast;
use flow_parser::file_key::{FileKey, FileKeyInner};
use flow_parser::loc::{LOC_NONE, Loc};
use flow_parser::parse_error::ParseError;
use flow_typing::{merge, type_inference};
use flow_typing_context::Context;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::{flow_error, intermediate_error};
use flow_utils_concurrency::check_budget::CheckBudget;
use flow_utils_concurrency::job_error::JobError;

use crate::cache::{CachedAnswer, CheckCache, Digest, Fields, Record, hex};
use crate::diagnostic::TypeDiagnostic;
use crate::limits::CHECK_STACK_BYTES;
use crate::upstream::graph::{Graph, ModuleFacts};
use crate::upstream::project::{MkBuiltins, ProjectModules};
use crate::{BuiltinsTiming, CheckError, CheckLimits, CheckReport, Source};

/// The absolute root every Flow path is resolved against.
///
/// Flow resolves a file key to an absolute path before rendering it, and its
/// renderer *panics* on a relative one, so an embedder has to supply a root
/// even when it has no filesystem in play — `uf` hands the checker
/// project-relative paths and gets them back stripped.
///
/// It is deliberately a path that cannot exist: the renderer reads a location's
/// file from disk when it can, to build a codepoint offset table. `uf` does not
/// want that. It wants Flow's raw byte columns, which is what `uf_term`'s code
/// frames and `uf_lint`'s diagnostics both measure in, and it already holds
/// every source in memory. A root under a dot-directory at `/` cannot be
/// created without root privileges, so the read always misses and the columns
/// stay in bytes.
pub(super) const VIRTUAL_ROOT: &str = "/.uf-check-virtual-root";

/// Merge the builtins, on the check thread so the merge gets its stack too.
pub(crate) fn prepare_builtins(libs: &[Source<'_>]) -> Result<BuiltinsTiming, CheckError> {
    on_check_thread("<builtins>", || builtins::prepare(libs))?
}

/// Check every source in one batch, sharing one builtin environment.
pub(crate) fn check_sources(
    sources: &[Source<'_>],
    libs: &[Source<'_>],
    limits: &CheckLimits,
    cache: Option<&CheckCache>,
) -> Result<CheckReport, CheckError> {
    let path = sources.first().map_or("<empty>", |source| source.path);
    on_check_thread(path, || check_batch(sources, libs, limits, cache))?
}

/// The closure of `seeds` over `available`, by the batch's own rules.
pub(crate) fn module_closure<'a>(
    seeds: &[&str],
    available: &[Source<'a>],
    libs: &[Source<'_>],
    limits: &CheckLimits,
) -> Result<crate::ModuleClosure<'a>, CheckError> {
    let path = seeds.first().copied().unwrap_or("<empty>");
    on_check_thread(path, || {
        // The builtins are merged here so that a specifier the library
        // definitions already describe — Flow's own or the project's — is not
        // handed back as something the caller should go and find. Once per
        // process and shared, so the check that follows this walk pays nothing
        // for having asked.
        builtins::prepare(libs)?;
        let master_cx = builtins::master_context(libs)?;
        let options = options::options(limits);
        let base_metadata = flow_typing_context::mk_context_metadata(&options, Arc::default());
        let mk_builtins = merge::mk_builtins(&base_metadata, &master_cx);
        // A batch of no files: this exists only to ask what the builtins
        // declare, which is a property of the compiler and not of any source.
        let probe = ProjectModules::new(&[], options.clone(), mk_builtins, limits);
        let found = closure::closure(seeds, available, &options, &|specifier| {
            probe.declared_externally(specifier)
        });
        probe.release();
        Ok(crate::ModuleClosure {
            sources: found
                .reached
                .into_iter()
                .map(|index| available[index])
                .collect(),
            unresolved: found.unresolved,
        })
    })?
}

/// Run `work` on a thread with a stack large enough for recursive descent over
/// user-controlled nesting.
///
/// Both the parser and inference recurse once per level of nesting, so a file
/// full of nested generics is a stack-depth attack against a default 2 MiB
/// worker. A large stack turns that into Flow's own recursion limit firing,
/// which is a diagnostic rather than an abort.
fn on_check_thread<T, F>(path: &str, work: F) -> Result<T, CheckError>
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("uf-typecheck".to_owned())
            .stack_size(CHECK_STACK_BYTES)
            .spawn_scoped(scope, work)
            .map_err(|error| CheckError::Worker {
                path: path.to_compact_string(),
                detail: error.to_compact_string(),
            })?;
        worker.join().map_err(|_| CheckError::Worker {
            path: path.to_compact_string(),
            detail: CompactString::const_new("the checker panicked"),
        })
    })
}

/// One batch, in two passes.
///
/// The first pass says what every file *is* — its signature and its imports —
/// without running inference over any of them: from the cache where it has a
/// record, and by parsing and packing where it does not. Only then can the
/// second pass ask, per file, whether the answer on disk is still the right
/// one, because that question is about the whole batch and not about the file.
///
/// The order matters in one more way. Packing a signature in the first pass
/// leaves it where the second pass looks, so a file's signature is built at
/// most once whichever pass needed it; the cost the split really adds is one
/// extra parse of the files nothing imports, which the batch used to parse
/// once and now parses twice.
fn check_batch(
    sources: &[Source<'_>],
    libs: &[Source<'_>],
    limits: &CheckLimits,
    cache: Option<&CheckCache>,
) -> Result<CheckReport, CheckError> {
    // Before anything reads a source, including the pass that only wants its
    // signature: the AST alone is several times the size of the text, so an
    // unbounded file is an unbounded allocation and no phase of this function
    // may be the one that finds out. It used to be checked inside `check_one`,
    // which was the only thing that parsed; it is checked here now because
    // `ProjectModules::facts` parses too.
    for source in sources {
        if source.source.len() > limits.max_source_bytes {
            return Err(CheckError::SourceTooLarge {
                path: source.path.to_compact_string(),
                size: source.source.len(),
                limit: limits.max_source_bytes,
            });
        }
    }

    let builtins = builtins::prepare(libs)?;
    let master_cx = builtins::master_context(libs)?;
    let options = options::options(limits);
    // One builtin environment for the batch, made from the metadata a file has
    // before its own docblock is applied — `mk_check_file` keeps exactly this
    // one and hands it to every file it checks. Per file it would be both
    // wasted work and wrong: a type crossing a module boundary is compared
    // against the importer's builtins, and two independent merges of `core.js`
    // do not agree on `Array`.
    let base_metadata = flow_typing_context::mk_context_metadata(&options, Arc::default());
    let mk_builtins = merge::mk_builtins(&base_metadata, &master_cx);
    let modules = Rc::new(ProjectModules::new(
        sources,
        options.clone(),
        mk_builtins.dupe(),
        limits,
    ));

    let started = Instant::now();
    // What each file's record is filed under. Computed even for a file the
    // cache turns out to know nothing about, because it is also where the
    // recomputed answer is written back.
    let libdefs = builtins::digest(libs);
    let keys: Vec<Digest> = match cache {
        Some(cache) => sources
            .iter()
            .map(|source| file_key(cache, limits, &libdefs, source))
            .collect(),
        None => Vec::new(),
    };
    let mut records: Vec<Option<Record>> = match cache {
        Some(cache) => keys
            .iter()
            .zip(sources)
            .map(|(key, source)| cache.read(key, source.path))
            .collect(),
        None => vec![None; sources.len()],
    };

    // The batch described before anything is checked: from the cache where it
    // could answer, and by parsing and packing where it could not. Nothing here
    // runs inference, so a run whose every file is unchanged never reaches it.
    let mut facts: Vec<ModuleFacts> = Vec::with_capacity(sources.len());
    for (index, record) in records.iter_mut().enumerate() {
        match record.as_ref().and_then(facts_of) {
            Some(known) => facts.push(known),
            None => {
                // A record that cannot be read back as facts is a record about
                // a shape this build does not understand: dropped, not
                // repaired, so nothing downstream reads half of it.
                *record = None;
                facts.push(modules.facts(index));
            }
        }
    }
    let graph = Graph::new(
        sources.iter().map(|source| source.path).collect(),
        &facts,
        &modules,
    );

    let mut diagnostics = Vec::new();
    let mut untyped = BTreeSet::new();
    let mut skipped = 0usize;
    let mut from_cache = 0usize;
    let mut result = Ok(());
    for (index, source) in sources.iter().enumerate() {
        if facts[index].skipped {
            skipped += 1;
        }
        untyped.extend(graph.untyped(index));

        let dependencies = graph.dependency_digest(index);
        // The record is about this file; the digest says whether it is still
        // about this *batch*. Both have to hold, and they fail for different
        // reasons: the key stops matching when the file was edited, the digest
        // when something it reaches was — and a record carries an answer per
        // batch, so a project checked both whole and by path finds both here.
        let believed = records[index]
            .as_ref()
            .and_then(|record| record.answer(&dependencies))
            .map(<[TypeDiagnostic]>::to_vec);
        if let Some(found) = believed {
            diagnostics.extend(found);
            // Counted against `files_checked`, which is why a `@noflow` file is
            // not counted at all: it was not checked either way.
            if !facts[index].skipped {
                from_cache += 1;
            }
            // The answer this batch used goes to the front, so eviction drops
            // the batch nobody asks for rather than the one that keeps being
            // answered without a write. Written back only when the order really
            // moved, so a settled warm run still touches no file on disk.
            if let Some(cache) = cache
                && let Some(record) = records[index].as_mut()
                && record.touch(&dependencies)
            {
                cache.write(&keys[index], record);
            }
            continue;
        }

        match check_one(index, &options, &mk_builtins, limits, source, &modules) {
            Ok(found) => {
                if let Some(cache) = cache {
                    // Onto whatever the record already knew, not over it: the
                    // answer another batch computed for this same file is still
                    // true of that batch. ubugeeei-prod/uf#406.
                    let record =
                        records[index].get_or_insert_with(|| record_of(source.path, &facts[index]));
                    record.remember(CachedAnswer {
                        dependencies,
                        diagnostics: found.clone(),
                    });
                    cache.write(&keys[index], record);
                }
                diagnostics.extend(found);
            }
            Err(error) => {
                result = Err(error);
                break;
            }
        }
    }
    // Whatever happened, the merged dependencies hold this batch alive through
    // their own resolvers; letting them go is not optional.
    modules.release();
    result?;

    Ok(CheckReport {
        diagnostics,
        files_checked: sources.len() - skipped,
        files_skipped: skipped,
        files_from_cache: from_cache,
        untyped_modules: untyped.into_iter().collect(),
        builtins,
        elapsed: started.elapsed(),
    })
}

/// Where one file's record is filed.
///
/// The compiler's identity is first because it is the input a reader is most
/// likely to forget is one: see [`crate::cache`]. The limits and the library
/// definitions are here rather than in the dependency digest because neither is
/// a property of any file — raising the recursion limit, or adding a
/// `declare module` to `flow-typed`, changes what every file in the batch
/// reports, including the files that reach nothing at all.
fn file_key(
    cache: &CheckCache,
    limits: &CheckLimits,
    libdefs: &Digest,
    source: &Source<'_>,
) -> Digest {
    let mut fields = Fields::new("uf-check-file-v1");
    fields.push(cache.identity());
    fields.push(&limits_field(limits));
    fields.push_digest(libdefs);
    fields.push(source.path);
    fields.push(source.source);
    fields.finish()
}

/// Every limit that can change what a check reports, as one field.
///
/// [`CheckLimits::file_timeout`] is deliberately not here. It decides whether a
/// check *finishes*, never what it says: a run that completes under a budget
/// reports what a run with no budget would have reported, and a run that
/// exhausts one writes nothing for the file it gave up on. Keying on it would
/// only mean an editor that bounds its own latency and a `uf check` that does
/// not could never read each other's entries — the same "two batches, one
/// record" waste as ubugeeei-prod/uf#406, for a difference that is not a
/// difference.
fn limits_field(limits: &CheckLimits) -> String {
    format!(
        "max-source-bytes={};recursion-limit={};type-expansion-recursion-limit={}",
        limits.max_source_bytes, limits.recursion_limit, limits.type_expansion_recursion_limit,
    )
}

/// What a record says about the file, or [`None`] when it does not say it in a
/// shape this build can read.
fn facts_of(record: &Record) -> Option<ModuleFacts> {
    let signature = match &record.signature {
        Some(spelling) => Some(unhex(spelling)?),
        None => None,
    };
    Some(ModuleFacts {
        signature,
        requires: record.requires.clone(),
        skipped: record.skipped,
    })
}

/// A thirty-two byte digest written as hex, or [`None`] when it is not one.
fn unhex(spelling: &str) -> Option<Digest> {
    let bytes = spelling.as_bytes();
    if bytes.len() != std::mem::size_of::<Digest>() * 2 {
        return None;
    }
    let mut digest = [0u8; std::mem::size_of::<Digest>()];
    let (pairs, _) = bytes.as_chunks::<2>();
    for (byte, pair) in digest.iter_mut().zip(pairs) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        *byte = u8::try_from(high * 16 + low).ok()?;
    }
    Some(digest)
}

/// An empty record about the file `facts` describes.
///
/// What a file *is* — its signature, its imports, whether it opted out — is a
/// property of its own text and of the compiler, both of which the key already
/// covers. So this is the same for every batch, and only the answers hung off
/// it differ.
fn record_of(path: &str, facts: &ModuleFacts) -> Record {
    Record::new(
        path,
        facts.signature.as_ref().map(hex),
        facts.requires.clone(),
        facts.skipped,
    )
}

fn check_one(
    index: usize,
    options: &Options,
    mk_builtins: &MkBuiltins,
    limits: &CheckLimits,
    source: &Source<'_>,
    modules: &Rc<ProjectModules>,
) -> Result<Vec<TypeDiagnostic>, CheckError> {
    let file_key = FileKey::new(FileKeyInner::SourceFile(source.path.to_owned()));
    let parsed = parse::parse_file(file_key.dupe(), source.source, options, false);
    if !parsed.is_parseable() {
        // A file that does not parse is broken whatever its docblock says, so
        // it is reported. Whether it also *counts* as checked is
        // `ProjectModules::facts`' answer, not this one: a run that reads this
        // file's diagnostics from the cache never gets here and must still
        // count it the same way.
        return Ok(parse_diagnostics(&parsed, source));
    }

    // `@noflow`. The file parsed, and that is all uf asked of it.
    if !parsed.is_checked() {
        return Ok(Vec::new());
    }

    let metadata = parsed.metadata.clone();
    let lint_severities = merge::get_lint_severities(
        &metadata,
        &options.strict_mode,
        options.lint_severities.clone(),
    );
    // The table this file's own signature was packed with, not an empty one:
    // it is what makes a class defined here the same class an importing file
    // sees. See `ProjectModules::aloc_table_for`.
    let aloc_table = modules.aloc_table_for(index, &parsed);
    let cx = Context::make(
        Rc::new(flow_typing_context::make_ccx()),
        metadata.clone(),
        file_key.dupe(),
        Arc::default(),
        aloc_table,
        modules.resolver(source.path),
        mk_builtins.dupe(),
        CheckBudget::new(limits.file_timeout),
    );
    cx.set_merge_dst_cx(&cx);

    let ast::Program { all_comments, .. } = parsed.ast.as_ref();
    let aloc_ast = flow_aloc::loc_to_aloc_ast(parsed.ast.as_ref());
    type_inference::infer_ast(
        &lint_severities,
        &cx,
        &parsed.file_key,
        parsed.file_sig.dupe(),
        &metadata,
        all_comments,
        aloc_ast,
    )
    .map_err(|error| job_error(source.path, error))?;

    let (errors, warnings) = suppressed(&cx, &parsed, cx.errors(), modules);
    Ok(convert::diagnostics(&errors, &warnings, source.path))
}

fn job_error(path: &str, error: JobError) -> CheckError {
    match error {
        JobError::TimedOut(timeout) => CheckError::Budget {
            path: path.to_compact_string(),
            limit_ms: u64::try_from(timeout.elapsed.as_millis()).unwrap_or(u64::MAX),
        },
        JobError::Canceled(_) => CheckError::Cancelled {
            path: path.to_compact_string(),
        },
        JobError::DebugThrow { .. } => CheckError::Worker {
            path: path.to_compact_string(),
            detail: CompactString::const_new("$Flow$DebugThrow"),
        },
    }
}

/// This file's syntax errors, in uf's words rather than the port's.
///
/// `uf fmt`, `uf lint` and `uf transform` all pass a parse error through
/// [`uf_flow::explain`], which is where uf says what a construct the parser
/// does not implement actually is. The checker did not, so the same file was
/// described one way by `uf check` and another by `uf lint`, and which of the
/// two a reader saw depended on the command they happened to run first
/// (ubugeeei-prod/uf#431).
///
/// Every error is offered to that function and nearly all of them come back
/// unchanged, which is the decision the issue asks for: the fallback is the
/// parser's own sentence, so the four commands agree by construction rather
/// than because a list of interesting errors was kept in step in two places.
///
/// # One error at a time
///
/// The port renders an *ordered set*, which has no way back from an entry to
/// the error it was made from — and the explanation is a function of that
/// error's own location and message. So each is rendered on its own and paired
/// with its explanation by construction. It costs nothing:
/// `make_errors_printable` is a fold, so folding one error N times is the work
/// of folding N once. What it gives up is the set's deduplication, which the
/// parser cannot exercise: it reports errors as it advances, so no two of them
/// carry the same location *and* the same message.
///
/// It also gains the parser's order. `ConcreteLocPrintableErrorSet` is a
/// `BTreeSet`, so a file that ends mid-declaration reported its five
/// end-of-input errors alphabetically — `)`, `,`, `{`, `}` — where `uf lint`
/// reports them in the order the parser reached them. Same errors, one order
/// now.
fn parse_diagnostics(parsed: &parse::Parsed, source: &Source<'_>) -> Vec<TypeDiagnostic> {
    parsed
        .parse_errors
        .iter()
        .flat_map(|(loc, error)| {
            let mut diagnostics = convert::diagnostics(
                &printable(parsed, parse_error_set(parsed, loc, error)),
                &ConcreteLocPrintableErrorSet::empty(),
                source.path,
            );
            if let Some(explanation) =
                uf_flow::explain::explanation(source.source, loc, &error.to_string())
            {
                for diagnostic in &mut diagnostics {
                    convert::explain(diagnostic, &explanation);
                }
            }
            diagnostics
        })
        .collect()
}

/// One syntax error, as the set the port's renderer takes.
fn parse_error_set(parsed: &parse::Parsed, loc: &Loc, error: &ParseError) -> ErrorSet {
    let mut errors = ErrorSet::empty();
    errors.add(flow_error::error_of_msg(
        parsed.file_key.dupe(),
        ErrorMessage::EParseError(Box::new((ALoc::of_loc(loc.clone()), error.clone()))),
    ));
    errors
}

fn printable(parsed: &parse::Parsed, errors: ErrorSet) -> ConcreteLocPrintableErrorSet {
    let ast = parsed.ast.dupe();
    let file_key = parsed.file_key.dupe();
    intermediate_error::make_errors_printable(
        // Parse errors carry the parser's own concrete locations, so there is
        // no table to look anything up in.
        |aloc: &ALoc| concrete_loc(aloc),
        move |requested: &FileKey| (requested == &file_key).then(|| ast.dupe()),
        Some(Path::new(VIRTUAL_ROOT)),
        errors,
        FileKey::is_lib_file,
    )
}

/// Turn an abstract location into a concrete one, with every file's table.
///
/// Merging a dependency's signature produces *keyed* locations: an index into
/// the table that dependency was packed with, rather than a line and a column.
/// One of those reaches a diagnostic whenever an error about the importing file
/// points at where the imported thing was declared, and resolving it needs the
/// table that made it — which is why the batch keeps every table it built. This
/// is `flow_cli`'s `make_loc_of_aloc` with the batch standing in for the heap.
fn loc_of_aloc(tables: &HashMap<FileKey, LazyALocTable>, aloc: &ALoc) -> Loc {
    match aloc.source().and_then(|source| tables.get(source)) {
        Some(table) => aloc.to_loc(table),
        None => concrete_loc(aloc),
    }
}

/// An abstract location that is already concrete, or a bare reference to its
/// file when it is not.
///
/// `ALoc::to_loc_exn` *panics* on a keyed location, which would turn "uf built
/// no table for this file" into an abort during error rendering. Naming the
/// file without a position is worse than the real location and better than
/// losing the diagnostic.
fn concrete_loc(aloc: &ALoc) -> Loc {
    if aloc_representation_do_not_use::is_keyed(aloc) {
        Loc {
            source: aloc.source().duped(),
            ..LOC_NONE
        }
    } else {
        aloc.to_loc_exn().dupe()
    }
}

/// Split lints by severity and drop anything a suppression comment covers.
///
/// Mirrors `flow_dot_js_wasm`, which mirrors OCaml `check_content`: without
/// this, `$FlowFixMe` and `$FlowExpectedError` comments in a real project would
/// be ignored.
fn suppressed(
    cx: &Context<'_>,
    parsed: &parse::Parsed,
    errors: ErrorSet,
    modules: &ProjectModules,
) -> (ConcreteLocPrintableErrorSet, ConcreteLocPrintableErrorSet) {
    let mut suppressions = cx.take_error_suppressions();
    let severity_cover = cx.severity_cover();
    let include_suppressions = cx.include_suppressions();
    // The batch's tables, not `cx.aloc_tables()`: a dependency is merged in its
    // own component context, so its table is not in this file's.
    let aloc_tables = modules.aloc_tables();
    let (errors, warnings) =
        suppressions.filter_lints(errors, &aloc_tables, include_suppressions, &severity_cover);
    drop(severity_cover);

    let loc_of_aloc = |aloc: &ALoc| loc_of_aloc(&aloc_tables, aloc);
    let file_key = parsed.file_key.dupe();
    let ast = parsed.ast.dupe();
    let get_ast = move |requested: &FileKey| (requested == &file_key).then(|| ast.dupe());
    let root = Path::new(VIRTUAL_ROOT);
    let unsuppressable = BTreeSet::new();

    let mut unused = ErrorSuppressions::empty();
    let (errors, _) = suppressions.filter_suppressed_errors(
        root,
        None,
        false,
        &unsuppressable,
        loc_of_aloc,
        &get_ast,
        FileKey::is_lib_file,
        &errors,
        &mut unused,
    );

    let mut unused = ErrorSuppressions::empty();
    let (warnings, _) = suppressions.filter_suppressed_errors(
        root,
        None,
        false,
        &unsuppressable,
        loc_of_aloc,
        &get_ast,
        FileKey::is_lib_file,
        &warnings,
        &mut unused,
    );

    (errors, warnings)
}
