//! The module transform uf contributes to a build, independent of who drives it.
//!
//! Vite runs uf's plugin in JavaScript and Rolldown runs it in Rust, and both
//! have to apply exactly the same three steps in exactly the same order or a
//! `uf dev` session and a `uf build` disagree about what the code is. This is
//! that pipeline, in one place, so neither driver can drift from the other.

use thiserror::Error;
use uf_jsx::JsxOptions;

use crate::pipeline::blank_directive_prologue;

/// Why a module could not be transformed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransformError {
    /// Flow type erasure failed.
    #[error("{id}: {source}")]
    Flow {
        /// Module the failure belongs to.
        id: String,
        /// What the eraser reported.
        source: uf_flow::StripError,
    },
    /// JSX lowering failed.
    #[error("{id}: {source}")]
    Jsx {
        /// Module the failure belongs to.
        id: String,
        /// What the lowering reported.
        source: uf_jsx::JsxError,
    },
}

/// Apply uf's build stages to one module.
///
/// Order is `uf:flow`, then `uf:rsc`, then `uf:jsx`, the order the pipeline
/// descriptors declare. Returns `None` when no stage changed anything, so a
/// caller can hand the original source on untouched rather than re-emitting a
/// copy of it.
pub fn transform_module(
    id: &str,
    code: &str,
    jsx: &JsxOptions,
) -> Result<Option<String>, TransformError> {
    let stripped = uf_flow::strip_types(code).map_err(|source| TransformError::Flow {
        id: id.to_owned(),
        source,
    })?;
    let mut current = if stripped.is_unchanged() {
        None
    } else {
        Some(stripped.code)
    };

    if let Some(blanked) = blank_directive_prologue(current.as_deref().unwrap_or(code)) {
        current = Some(blanked);
    }

    let lowered = uf_jsx::transform(current.as_deref().unwrap_or(code), jsx).map_err(|source| {
        TransformError::Jsx {
            id: id.to_owned(),
            source,
        }
    })?;
    if lowered.code != current.as_deref().unwrap_or(code) {
        current = Some(lowered.code);
    }

    Ok(current)
}

/// Whether a module is one uf is responsible for transforming.
///
/// Two things are excluded and one is deliberately not. A build driver
/// synthesises runtime modules of its own — Rolldown's `require` shim, Vite's
/// client — and handing one of those to a Flow type eraser turns its helpers
/// into a syntax error in a file nobody wrote. A third-party dependency ships
/// JavaScript that is already JavaScript.
///
/// `@uniflowed/*` under `node_modules` is *not* excluded: those packages ship
/// Flow source, because that is what uf tells everyone to write. Skipping them
/// leaves `// @flow` in front of a bundler that reports `Flow is not
/// supported`, which is what happens the first time a project installs them.
#[must_use]
pub fn is_project_module(id: &str) -> bool {
    if !id.ends_with(".js") || id.starts_with('\0') {
        return false;
    }
    match id.rfind("/node_modules/") {
        Some(at) => id[at..].starts_with("/node_modules/@uniflowed/"),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> JsxOptions {
        JsxOptions::default()
    }

    #[test]
    fn the_three_stages_run_in_order() {
        let source = "\"use client\";\n// @flow\nexport function App(): mixed {\n  return <main>hi</main>;\n}\n";

        let output = transform_module("app.js", source, &options())
            .expect("transforms")
            .expect("something changed");

        assert!(
            !output.contains("\"use client\""),
            "prologue survived:\n{output}"
        );
        assert!(
            !output.contains(": mixed"),
            "annotation survived:\n{output}"
        );
        assert!(!output.contains("<main"), "JSX survived:\n{output}");

        let parsed = uf_flow::validate_source(&output).expect("parser ran");
        assert!(
            parsed.is_ok(),
            "output does not parse: {:?}\n{output}",
            parsed.diagnostics
        );
    }

    #[test]
    fn a_module_needing_no_stage_reports_no_change() {
        let source = "export const a = 1;\n";

        assert_eq!(transform_module("plain.js", source, &options()), Ok(None));
    }

    #[test]
    fn a_driver_s_own_modules_are_not_project_modules() {
        assert!(is_project_module("/app/main.js"));
        assert!(!is_project_module("\0vite/client"));
        assert!(!is_project_module("/app/node_modules/react/index.js"));
        // uf's own packages ship Flow, so they must still be transformed.
        assert!(is_project_module(
            "/app/node_modules/@uniflowed/react/index.js"
        ));
        assert!(is_project_module(
            "/app/node_modules/@uniflowed/core/internal/native-runtime.js"
        ));
        assert!(!is_project_module("/app/styles.css"));
    }
}
