//! Turning text into a module path, and refusing the text that is not one.
//!
//! Two directions, one rule. [`module_path`] normalizes a path the watcher
//! observed; [`relative_candidate`] folds a specifier a module wrote. Both
//! refuse rather than repair: an absolute path, a drive letter, or a `..` with
//! nothing to pop is an error, never a value trimmed into something that looks
//! project-relative. That is the same decision [`crate::resolve`] makes about a
//! request path, and for the same reason — a repaired path is a path nobody
//! checked.

use camino::{Utf8Path, Utf8PathBuf};
use smallvec::SmallVec;
use uf_rsc::ImportSpecifier;

use super::{GraphError, MAX_MODULE_DEPTH};

/// The module paths a specifier candidate may resolve to, in precedence order.
pub(super) fn candidate_paths(candidate: &Utf8Path) -> SmallVec<[Utf8PathBuf; 3]> {
    let mut paths: SmallVec<[Utf8PathBuf; 3]> = SmallVec::new();
    paths.push(candidate.to_owned());
    let text = candidate.as_str();
    if !text.ends_with(".js") {
        let mut with_extension = String::with_capacity(text.len() + 3);
        with_extension.push_str(text);
        with_extension.push_str(".js");
        paths.push(Utf8PathBuf::from(with_extension));
    }
    paths.push(candidate.join("index.js"));
    paths
}

/// Fold a relative specifier against its importer, or `None` for anything that
/// is not a relative specifier or that climbs out of the project.
///
/// A bare specifier (`react`, `@uniflowed/core`) is not a graph edge: it is
/// resolved by the package layer, not by the dev graph.
pub(super) fn relative_candidate(
    importer: &Utf8Path,
    specifier: &ImportSpecifier,
) -> Option<Utf8PathBuf> {
    let text = specifier.specifier.as_str();
    let normalized;
    let text = if text.contains('\\') {
        normalized = text.replace('\\', "/");
        normalized.as_str()
    } else {
        text
    };
    if !(text.starts_with("./") || text.starts_with("../") || text == "." || text == "..") {
        return None;
    }

    let base = importer.parent().unwrap_or(Utf8Path::new(""));
    let mut segments: SmallVec<[&str; 16]> = base
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    for segment in text.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => {
                if segments.len() >= MAX_MODULE_DEPTH {
                    return None;
                }
                segments.push(other);
            }
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(Utf8PathBuf::from(segments.join("/")))
}

/// Normalize a module path into the project-relative, forward-slash form.
///
/// Public because it is the gate every caller that turns a watched path into a
/// filesystem read has to go through first — see
/// [`HmrSession`](crate::hmr::HmrSession), which normalizes before it joins
/// anything onto the project root.
///
/// # Errors
///
/// Returns [`GraphError::NotProjectRelative`] for a path that is absolute,
/// climbs out of the project, is empty, or carries a NUL byte, and
/// [`GraphError::TooDeep`] for one with more than [`MAX_MODULE_DEPTH`] segments.
pub fn module_path(raw: &str) -> Result<Utf8PathBuf, GraphError> {
    let rejected = || GraphError::NotProjectRelative {
        path: Utf8PathBuf::from(raw),
    };
    if raw.is_empty()
        || raw.starts_with('/')
        || raw.starts_with('\\')
        || raw.contains(':')
        || raw.bytes().any(|byte| byte == 0)
    {
        return Err(rejected());
    }

    let mut segments: SmallVec<[&str; 16]> = SmallVec::new();
    for segment in raw.split(['/', '\\']) {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop().ok_or_else(rejected)?;
            }
            other => {
                if segments.len() == MAX_MODULE_DEPTH {
                    return Err(GraphError::TooDeep {
                        path: Utf8PathBuf::from(raw),
                        depth: segments.len() + 1,
                    });
                }
                segments.push(other);
            }
        }
    }
    if segments.is_empty() {
        return Err(rejected());
    }
    Ok(Utf8PathBuf::from(segments.join("/")))
}
