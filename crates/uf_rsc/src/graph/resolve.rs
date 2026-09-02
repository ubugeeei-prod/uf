//! Import-specifier resolution and the path-traversal guard around it.
//!
//! Specifiers are resolved entirely in memory: `..` segments are folded
//! textually and anything climbing above the project root is refused rather
//! than resolved, so no analysis downstream is ever handed a path that leaves
//! the project. The `server-only` package list and the `.server.js` suffix
//! convention are also decided here.

use std::borrow::Cow;

use camino::{Utf8Path, Utf8PathBuf};
use uf_infra::FxHashMap;

use super::{ModuleId, SERVER_ONLY_PACKAGES, SERVER_ONLY_SUFFIX};

/// Whether an import specifier names server-only code.
pub fn is_server_only_specifier(specifier: &str) -> bool {
    if SERVER_ONLY_PACKAGES.binary_search(&specifier).is_ok() {
        return true;
    }
    if SERVER_ONLY_PACKAGES.iter().any(|package| {
        specifier
            .strip_prefix(package)
            .is_some_and(|rest| rest.starts_with('/'))
    }) {
        return true;
    }
    is_server_only_path(specifier)
}

pub(crate) fn is_server_only_path(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|name| {
        name.len() > SERVER_ONLY_SUFFIX.len() && name.ends_with(SERVER_ONLY_SUFFIX)
    })
}

/// What an import specifier turned out to name.
///
/// This is the shared path guard: every consumer of the graph — the RSC
/// analysis and the bundler alike — asks this question in one place, so there
/// is exactly one implementation of "does this specifier leave the project?"
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecifierResolution {
    /// A project-relative path, normalized and proven to stay inside the root.
    Relative(Utf8PathBuf),
    /// A bare specifier: a package name, not a path.
    Bare,
    /// A relative specifier that climbs above the project root.
    Escapes,
}

/// Resolve an import specifier written in `importer` against the project root.
///
/// Purely lexical: nothing is read from disk, `..` is folded textually, and a
/// specifier that climbs out of the project is [`SpecifierResolution::Escapes`]
/// rather than a path anyone could open.
pub fn resolve_specifier(importer: &Utf8Path, specifier: &str) -> SpecifierResolution {
    let specifier = if specifier.contains('\\') {
        Cow::Owned(specifier.replace('\\', "/"))
    } else {
        Cow::Borrowed(specifier)
    };
    if !(specifier.starts_with("./") || specifier.starts_with("../") || specifier == ".") {
        return SpecifierResolution::Bare;
    }
    let base = importer.parent().unwrap_or(Utf8Path::new(""));
    match normalize_relative(base, &specifier) {
        Some(path) => SpecifierResolution::Relative(path),
        None => SpecifierResolution::Escapes,
    }
}

/// Join and normalize without touching the file system.
///
/// `..` segments are resolved textually and a specifier that climbs above the
/// project root returns `None` rather than a path outside it. This is the
/// path-traversal guard for the graph: no analysis, and no bundler consuming the
/// manifest, is ever handed a path that leaves the project.
fn normalize_relative(base: &Utf8Path, specifier: &str) -> Option<Utf8PathBuf> {
    let mut segments: Vec<&str> = base
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();

    for segment in specifier.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }

    let mut path = String::with_capacity(specifier.len() + base.as_str().len());
    for (position, segment) in segments.iter().enumerate() {
        if position > 0 {
            path.push('/');
        }
        path.push_str(segment);
    }
    Some(Utf8PathBuf::from(path))
}

/// Try the resolved path, then the `.js` and `/index.js` forms.
pub(crate) fn resolve_candidates(
    candidate: &Utf8Path,
    index: &FxHashMap<Utf8PathBuf, ModuleId>,
) -> Option<ModuleId> {
    if let Some(id) = index.get(candidate) {
        return Some(*id);
    }
    let with_extension = Utf8PathBuf::from(format!("{candidate}.js"));
    if let Some(id) = index.get(&with_extension) {
        return Some(*id);
    }
    let with_index = candidate.join("index.js");
    index.get(&with_index).copied()
}

/// Normalize a module path to the project-relative, forward-slash form.
///
/// A `..` that cannot be resolved is kept so [`is_inside_project`] can reject the
/// path instead of silently rewriting an escape into a plausible-looking module.
pub fn normalize_module_path(path: &Utf8Path) -> Utf8PathBuf {
    let text = path.as_str().replace('\\', "/");
    let mut segments: Vec<&str> = Vec::new();

    for segment in text.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.last().is_some_and(|last| *last != "..") {
                    segments.pop();
                } else {
                    segments.push("..");
                }
            }
            other => segments.push(other),
        }
    }

    let mut normalized = String::with_capacity(text.len());
    if text.starts_with('/') {
        normalized.push('/');
    }
    for (position, segment) in segments.iter().enumerate() {
        if position > 0 {
            normalized.push('/');
        }
        normalized.push_str(segment);
    }
    Utf8PathBuf::from(normalized)
}

/// Whether a normalized module path stays inside the project root.
///
/// Rejects absolute paths, anything that climbs with `..`, and anything
/// carrying a `:` — a Windows drive letter is an absolute path wearing a URL
/// scheme's clothes, and both are refused.
pub fn is_inside_project(path: &Utf8Path) -> bool {
    let text = path.as_str();
    !text.is_empty()
        && !text.starts_with('/')
        && !text.starts_with("../")
        && text != ".."
        && !text.contains(':')
}
