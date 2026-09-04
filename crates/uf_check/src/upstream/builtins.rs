//! The shared builtin environment.
//!
//! Merging Flow's library definitions — the global type environment, plus the
//! `declare module` blocks for `react` and friends — costs tens of
//! milliseconds and produces a value that never changes. Doing it per file
//! would dominate the cost of checking a project, so it happens once per
//! process and every check borrows the result.

use std::sync::Arc;
use std::sync::Once;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use compact_str::{CompactString, ToCompactString};
use dupe::Dupe;
use flow_common::type_strictness::TypeStrictnessKind;
use flow_parser::file_key::{FileKey, FileKeyInner};
use flow_typing::merge;
use flow_typing_context::MasterContext;

use super::VIRTUAL_ROOT;
use super::options::{builtin_sig_options, options};
use super::parse::parse_file;
use crate::{BuiltinsTiming, CheckError, CheckLimits};

/// The merged builtins and what they cost to build.
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

static BUILTINS: OnceLock<Result<Builtins, CompactString>> = OnceLock::new();
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

/// Build the builtins, or return the shared ones.
pub(crate) fn prepare() -> Result<BuiltinsTiming, CheckError> {
    let started = Instant::now();
    let mut cold = false;
    let cached = BUILTINS.get_or_init(|| {
        cold = true;
        build()
    });
    let elapsed = started.elapsed();

    match cached {
        Ok(builtins) => Ok(BuiltinsTiming {
            elapsed,
            cold_elapsed: builtins.cold_elapsed,
            cold,
        }),
        Err(detail) => Err(CheckError::Builtins {
            detail: detail.clone(),
        }),
    }
}

/// The shared master context, building it on first use.
pub(super) fn master_context() -> Result<Arc<MasterContext>, CheckError> {
    match BUILTINS.get_or_init(build) {
        Ok(builtins) => Ok(builtins.master_cx.dupe()),
        Err(detail) => Err(CheckError::Builtins {
            detail: detail.clone(),
        }),
    }
}

/// Parse and merge every library definition baked into the port.
fn build() -> Result<Builtins, CompactString> {
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
    let mut contents = flow_flowlib::contents_list(false);
    contents.extend(super::environments::ENVIRONMENTS.iter().copied());
    let options = options(&CheckLimits::default());
    let mut asts = Vec::with_capacity(contents.len());
    for (name, content) in contents.into_iter().rev() {
        let file_key = FileKey::new(FileKeyInner::LibFile(name.to_string()));
        let parsed = parse_file(file_key, content, &options, true);
        if let Some((loc, error)) = parsed.parse_errors.first() {
            return Err(format!(
                "{name}:{}:{}: {error}",
                loc.start.line,
                loc.start.column + 1
            )
            .to_compact_string());
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
