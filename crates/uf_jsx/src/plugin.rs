//! The `uf:jsx` stage, as a plugin the container can run.
//!
//! `uf_plugin` owns the descriptor and knows nothing about JSX; this module
//! owns the transform and knows nothing about ordering. [`FnPlugin`] is the
//! seam, which is why the container never grows a dependency on a transform
//! crate and why the stage `uf inspect --json` prints is the stage that runs.
//!
//! # Where it sits
//!
//! Last. `uf:jsx` is in the `Post` band, after `uf:react-compiler`, and that
//! order is the same one Babel uses: the React compiler reads components as
//! their author wrote them, JSX included, and lowering to `_jsx` calls
//! afterwards changes no data flow it reasoned about. Running JSX first would
//! take away the syntax the compiler analyses.

use uf_plugin::{BuiltinPlugin, FnPlugin, HookFailure, HookOutcome, ModuleCode};

use crate::{JsxError, JsxOptions, MAX_SOURCE_BYTES, transform};

/// Rule id reported when a project's React version needs the classic runtime.
pub const CLASSIC_RUNTIME_RULE: &str = "jsx/classic-runtime";

/// Rule id reported when lowering would move a module's lines.
pub const LINE_COUNT_RULE: &str = "jsx/line-count";

/// Build the `uf:jsx` plugin for a project's options.
#[must_use]
pub fn plugin(options: JsxOptions) -> FnPlugin {
    FnPlugin::new(BuiltinPlugin::Jsx.descriptor()).on_transform(move |input| {
        match transform(input.code, &options) {
            Ok(transformed) if transformed.is_unchanged() => Ok(HookOutcome::Passthrough),
            Ok(transformed) => Ok(HookOutcome::Handled(ModuleCode::new(transformed.code))),
            Err(error) => Err(failure(&error)),
        }
    })
}

/// Turn a lowering failure into the container's typed refusal.
fn failure(error: &JsxError) -> HookFailure {
    match error {
        JsxError::SourceTooLarge { bytes, limit } => HookFailure::InputTooLarge {
            bytes: *bytes,
            limit: *limit,
        },
        JsxError::TooManyElements { .. } => HookFailure::InputTooLarge {
            bytes: crate::MAX_ELEMENTS,
            limit: MAX_SOURCE_BYTES,
        },
        JsxError::ClassicRuntimeUnsupported { .. } => HookFailure::Rejected {
            rule: CLASSIC_RUNTIME_RULE,
        },
        JsxError::LineCountChanged { .. } => HookFailure::Rejected {
            rule: LINE_COUNT_RULE,
        },
    }
}
