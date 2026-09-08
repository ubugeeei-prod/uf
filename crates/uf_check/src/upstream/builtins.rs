//! The shared builtin environment.
//!
//! Merging Flow's library definitions — the global type environment, plus the
//! `declare module` blocks for `react` and friends — costs tens of
//! milliseconds and produces a value that never changes. Doing it per file
//! would dominate the cost of checking a project, so it happens once per
//! process and every check borrows the result.
//!
//! # Why "once" is once *per set of library definitions*
//!
//! Flow's own definitions are fixed, but a project's are not: `.flowconfig`'s
//! `[libs]` is source the project wrote, it is merged into the same
//! environment, and two projects do not have the same one. So the memo is a
//! map keyed by a digest of the libdefs rather than a single slot — a batch
//! asks for the environment its libdefs make, and pays the merge only the
//! first time that combination is asked for.
//!
//! It is bounded at [`MAX_ENVIRONMENTS`], because a `MasterContext` is not
//! small and an unbounded map would grow with every `.flowconfig` an editor
//! session opened. The bound is on memory and never on correctness: an
//! environment that falls out is merged again, and no batch is ever handed one
//! built from libdefs it did not ask for.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::Once;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use compact_str::ToCompactString;
use flow_common::type_strictness::TypeStrictnessKind;
use flow_parser::file_key::{FileKey, FileKeyInner};
use flow_typing::merge;
use flow_typing_context::MasterContext;

use super::VIRTUAL_ROOT;
use super::options::{builtin_sig_options, options};
use super::parse::parse_file;
use crate::cache::{Digest, Fields};
use crate::{BuiltinsTiming, CheckError, CheckLimits, Source};

/// How many merged environments one process keeps.
///
/// Two: the one this run is checking under, and one more so that an embedder
/// alternating between a project and a bare batch — an editor's scratch
/// buffer, a test — does not re-merge on every switch. A CLI run only ever
/// asks for one.
const MAX_ENVIRONMENTS: usize = 2;

/// Who wrote a library definition, which decides how a failure to parse one is
/// reported: a vendored one means the submodule and this crate disagree, and a
/// project's means a file its author can fix.
#[derive(Clone, Copy)]
enum Origin {
    /// From `upstream/flow`, or from `crates/uf_check/libdefs`.
    Vendored,
    /// From the project's `.flowconfig` `[libs]`.
    Project,
}

/// The merged builtins and what they cost to build.
#[derive(Clone)]
struct Builtins {
    master_cx: Arc<MasterContext>,
    cold_elapsed: Duration,
}

/// Sharing the merged builtins across threads is the entire point of caching
/// them, so make the requirement a compile error here rather than a confusing
/// one at the `OnceLock`.
const _: () = {
    const fn assert_shareable<T: Send + Sync>() {}
    assert_shareable::<MasterContext>();
};

/// Merged environments, most recently asked for first.
///
/// A `Vec` rather than a map because [`MAX_ENVIRONMENTS`] is two: the scan is
/// shorter than a hash, and the order is what the bound evicts by.
type Environments = Vec<(Digest, Result<Builtins, CheckError>)>;

static BUILTINS: OnceLock<Mutex<Environments>> = OnceLock::new();
static ROOTS: Once = Once::new();

/// Install the two process-global roots the port reads through statics.
///
/// `FileKey::to_absolute` panics on first use when these are unset rather than
/// defaulting, which is defensible for a long-lived server and a trap for an
/// embedder. Both are set to [`VIRTUAL_ROOT`], which the error renderer strips
/// back off; see that constant for why it is not the real project root.
pub(super) fn ensure_roots() {
    ROOTS.call_once(|| {
        flow_parser::file_key::set_project_root(VIRTUAL_ROOT);
        flow_parser::file_key::set_flowlib_root(VIRTUAL_ROOT);
    });
}

/// What a set of project library definitions is filed under.
///
/// Over the path *and* the text of every one, in order, because all three
/// change what the environment declares: editing a libdef, renaming one, and
/// reordering two are three different environments, and the last is not a
/// no-op — a later definition shadows an earlier one.
pub(super) fn digest(libs: &[Source<'_>]) -> Digest {
    let mut fields = Fields::new("uf-check-libdefs-v1");
    for lib in libs {
        fields.push(lib.path);
        fields.push(lib.source);
    }
    fields.finish()
}

/// Build the environment `libs` makes, or return the shared one.
pub(crate) fn prepare(libs: &[Source<'_>]) -> Result<BuiltinsTiming, CheckError> {
    let started = Instant::now();
    let (builtins, cold) = environment(libs)?;
    Ok(BuiltinsTiming {
        elapsed: started.elapsed(),
        cold_elapsed: builtins.cold_elapsed,
        cold,
    })
}

/// The shared master context for `libs`, building it on first use.
pub(super) fn master_context(libs: &[Source<'_>]) -> Result<Arc<MasterContext>, CheckError> {
    Ok(environment(libs)?.0.master_cx)
}

/// The memoised environment for `libs`, and whether this call built it.
fn environment(libs: &[Source<'_>]) -> Result<(Builtins, bool), CheckError> {
    let key = digest(libs);
    let mut environments = BUILTINS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        // A panic while merging poisons the lock, and the entry it was writing
        // is simply not there. Recovering the guard costs one extra merge and
        // keeps a single bad libdef from disabling the environment for the
        // rest of the process.
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if let Some(position) = environments
        .iter()
        .position(|(existing, _)| *existing == key)
    {
        // Moved to the front so the bound evicts the environment nobody is
        // asking for rather than the one being answered for free.
        let entry = environments.remove(position);
        let found = entry.1.clone();
        environments.insert(0, entry);
        return found.map(|builtins| (builtins, false));
    }

    let built = build(libs);
    environments.insert(0, (key, built.clone()));
    environments.truncate(MAX_ENVIRONMENTS);
    built.map(|builtins| (builtins, true))
}

/// Parse and merge every library definition baked into the port, then the
/// project's own.
fn build(libs: &[Source<'_>]) -> Result<Builtins, CheckError> {
    ensure_roots();
    let started = Instant::now();
    // Library definitions are merged in reverse declaration order: later
    // definitions shadow earlier ones, and `merge_lib_files` folds from the
    // front. This mirrors `flow_dot_js_wasm`.
    // Flow's own `lib/`, then the platform environments. `core.js` and
    // `react.js` are all `lib/` holds; `document` and `Response` come from
    // `evals/flow-typed/environment`, which Flow loads through a
    // `.flowconfig`'s `[libs]` and uf merges unconditionally. See
    // [`super::environments`].
    let mut contents: Vec<(&str, &str, Origin)> = flow_flowlib::contents_list(false)
        .into_iter()
        .chain(super::environments::ENVIRONMENTS.iter().copied())
        .map(|(name, content)| (name, content, Origin::Vendored))
        .collect();
    // Last, so a project's own `[libs]` shadow both — which is the order
    // `flow check` merges in, and the reason a project can redeclare a global
    // Flow already describes.
    contents.extend(
        libs.iter()
            .map(|lib| (lib.path, lib.source, Origin::Project)),
    );
    let options = options(&CheckLimits::default());
    let mut asts = Vec::with_capacity(contents.len());
    for (name, content, origin) in contents.into_iter().rev() {
        let file_key = FileKey::new(FileKeyInner::LibFile(name.to_string()));
        let parsed = parse_file(file_key, content, &options, true);
        if let Some((loc, error)) = parsed.parse_errors.first() {
            let detail =
                format!("{}:{}: {error}", loc.start.line, loc.start.column + 1).to_compact_string();
            return Err(match origin {
                Origin::Project => CheckError::LibDef {
                    path: name.to_compact_string(),
                    detail,
                },
                Origin::Vendored => CheckError::Builtins {
                    detail: format!("{name}:{detail}").to_compact_string(),
                },
            });
        }
        asts.push((TypeStrictnessKind::from_is_typescript(false), parsed.ast));
    }

    // The merge's own error set describes problems inside Flow's library
    // definitions, which are vendored and fixed. `flow_dot_js_wasm` discards it
    // for the same reason: there is nothing a user of this crate could do.
    let (_lib_errors, master_cx) =
        merge::merge_lib_files(&builtin_sig_options(), Arc::default(), &asts);

    Ok(Builtins {
        master_cx: Arc::new(master_cx),
        cold_elapsed: started.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sets_of_library_definitions_are_two_environments() {
        let one = [Source::new("flow-typed/a.js", "declare type A = string;\n")];
        let other = [Source::new("flow-typed/a.js", "declare type A = number;\n")];

        assert_ne!(digest(&one), digest(&other));
    }

    #[test]
    fn reordering_library_definitions_is_a_different_environment() {
        let first = Source::new("flow-typed/a.js", "declare type A = string;\n");
        let second = Source::new("flow-typed/b.js", "declare type B = number;\n");

        // A later definition shadows an earlier one, so the order is part of
        // what the environment means and not a presentation detail.
        assert_ne!(digest(&[first, second]), digest(&[second, first]));
    }

    #[test]
    fn no_library_definitions_is_a_key_of_its_own() {
        assert_ne!(
            digest(&[]),
            digest(&[Source::new("flow-typed/a.js", "declare type A = string;\n")])
        );
    }
}
