//! The checker itself, driven from Meta's official Flow Rust port.
//!
//! This is `flow_dot_js_wasm`'s check path with four things changed: the
//! builtin environment is merged once and shared, an import of another file in
//! the batch resolves to that file's signature the way
//! `flow_services_inference` resolves one to the heap's (see [`project`]), the
//! work runs on a thread with enough stack for user-controlled recursion, and
//! the result comes back as [`TypeDiagnostic`]s instead of JSON.
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

mod builtins;
mod convert;
mod environments;
mod options;
mod parse;
mod project;
mod resolve;

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
use flow_typing::{merge, type_inference};
use flow_typing_context::Context;
use flow_typing_errors::error_message::ErrorMessage;
use flow_typing_errors::error_suppressions::ErrorSuppressions;
use flow_typing_errors::flow_error::ErrorSet;
use flow_typing_errors::{flow_error, intermediate_error};
use flow_utils_concurrency::check_budget::CheckBudget;
use flow_utils_concurrency::job_error::JobError;

use crate::diagnostic::TypeDiagnostic;
use crate::limits::CHECK_STACK_BYTES;
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
pub(crate) fn prepare_builtins() -> Result<BuiltinsTiming, CheckError> {
    on_check_thread("<builtins>", builtins::prepare)?
}

/// Check every source in one batch, sharing one builtin environment.
pub(crate) fn check_sources(
    sources: &[Source<'_>],
    limits: &CheckLimits,
) -> Result<CheckReport, CheckError> {
    let path = sources.first().map_or("<empty>", |source| source.path);
    on_check_thread(path, || check_batch(sources, limits))?
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

fn check_batch(sources: &[Source<'_>], limits: &CheckLimits) -> Result<CheckReport, CheckError> {
    let builtins = builtins::prepare()?;
    let master_cx = builtins::master_context()?;
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
    let mut diagnostics = Vec::new();
    let mut skipped = 0usize;
    let mut result = Ok(());
    for (index, source) in sources.iter().enumerate() {
        match check_one(index, &options, &mk_builtins, limits, source, &modules) {
            Ok(outcome) => {
                if outcome.skipped {
                    skipped += 1;
                }
                diagnostics.extend(outcome.diagnostics);
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
        untyped_modules: modules.untyped_modules(),
        builtins,
        elapsed: started.elapsed(),
    })
}

/// What checking one file produced, and whether it was checked at all.
struct FileOutcome {
    /// Diagnostics for the file. A skipped file can still have parse errors.
    diagnostics: Vec<TypeDiagnostic>,
    /// Whether the file opted out of inference with `@noflow`.
    skipped: bool,
}

fn check_one(
    index: usize,
    options: &Options,
    mk_builtins: &MkBuiltins,
    limits: &CheckLimits,
    source: &Source<'_>,
    modules: &Rc<ProjectModules>,
) -> Result<FileOutcome, CheckError> {
    // Checked before the parser sees the text: the AST alone is several times
    // the size of the source, so an unbounded file is an unbounded allocation.
    if source.source.len() > limits.max_source_bytes {
        return Err(CheckError::SourceTooLarge {
            path: source.path.to_compact_string(),
            size: source.source.len(),
            limit: limits.max_source_bytes,
        });
    }

    let file_key = FileKey::new(FileKeyInner::SourceFile(source.path.to_owned()));
    let parsed = parse::parse_file(file_key.dupe(), source.source, options, false);
    if !parsed.is_parseable() {
        let errors = printable(&parsed, parse_error_set(&parsed));
        return Ok(FileOutcome {
            diagnostics: convert::diagnostics(
                &errors,
                &ConcreteLocPrintableErrorSet::empty(),
                source.path,
            ),
            // A file that does not parse is broken whatever its docblock says,
            // so it is reported — but it was not checked either.
            skipped: !parsed.is_checked(),
        });
    }

    // `@noflow`. The file parsed, and that is all uf asked of it.
    if !parsed.is_checked() {
        return Ok(FileOutcome {
            diagnostics: Vec::new(),
            skipped: true,
        });
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
    Ok(FileOutcome {
        diagnostics: convert::diagnostics(&errors, &warnings, source.path),
        skipped: false,
    })
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

fn parse_error_set(parsed: &parse::Parsed) -> ErrorSet {
    let mut errors = ErrorSet::empty();
    for (loc, error) in parsed.parse_errors.iter().cloned() {
        errors.add(flow_error::error_of_msg(
            parsed.file_key.dupe(),
            ErrorMessage::EParseError(Box::new((ALoc::of_loc(loc), error))),
        ));
    }
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
