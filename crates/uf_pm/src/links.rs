//! What `uf link` left in a project, read off the filesystem.
//!
//! The managers record a link in different places. pnpm writes an override and
//! Yarn 2+ a resolution, but npm, Yarn 1 and bun write nothing except
//! `node_modules` itself. A report built from the manifest and the lockfile alone
//! therefore said "already up to date" right after npm, Yarn 1 or bun had linked
//! a package, because the one thing they changed was the one place it did not
//! look (ubugeeei-prod/uf#976). `node_modules/<name>` is where every manager's
//! link ends up, whatever else it writes, so [`link_state`] reads that: whether
//! it is a link, and where the link leads.
//!
//! # Untrusted input
//!
//! A package name reaches this from the command line or from a manifest, and is
//! joined onto a path. [`is_package_name`] refuses anything that could name
//! something outside `node_modules` — a separator, `.` or `..`, a drive — before
//! anything is looked at. A manifest is read only when it is a regular file no
//! larger than [`MAX_MANIFEST_BYTES`], and nothing read here reaches a command
//! line.

use std::fs;
use std::io::Read;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;

use crate::detect::MAX_MANIFEST_BYTES;
use crate::is_polluting_json_key;

/// What `node_modules/<name>` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    /// A link to a directory outside the project's own `node_modules` — what
    /// `npm link`, `pnpm link`, `yarn link` and `bun link` all leave — and the
    /// directory it leads to, every link along the way followed.
    Linked(Utf8PathBuf),
    /// A link that leads nowhere, and what it says, exactly as written.
    Broken(Utf8PathBuf),
    /// Something a manager installed: a directory, or a link into the
    /// project's own `node_modules`, which is how pnpm installs every package
    /// out of `.pnpm`.
    Installed,
    /// Nothing there.
    Absent,
}

/// What `node_modules/<name>` is in the project at `root`.
///
/// `None` when `name` is not a package name, so that nothing outside
/// `node_modules` is ever looked at on its behalf.
#[must_use]
pub fn link_state(root: &Utf8Path, name: &str) -> Option<LinkState> {
    if !is_package_name(name) {
        return None;
    }
    let modules = root.join("node_modules");
    let entry = modules.join(name);
    let Ok(metadata) = fs::symlink_metadata(&entry) else {
        return Some(LinkState::Absent);
    };
    if !metadata.file_type().is_symlink() {
        return Some(LinkState::Installed);
    }
    let Ok(target) = entry.canonicalize_utf8() else {
        let written = fs::read_link(&entry)
            .ok()
            .and_then(|written| Utf8PathBuf::from_path_buf(written).ok())
            .unwrap_or_default();
        return Some(LinkState::Broken(written));
    };
    let inside = modules
        .canonicalize_utf8()
        .is_ok_and(|modules| target.starts_with(modules));
    Some(if inside {
        LinkState::Installed
    } else {
        LinkState::Linked(target)
    })
}

/// Whether `name`, joined onto `node_modules`, names something inside it:
/// `name` or `@scope/name`, each part neither empty, `.` nor `..`, and free of
/// separators, drive colons and NUL.
///
/// Not npm's validation of a name, which is the manager's to apply; only what
/// keeps a path inside the directory it was joined onto.
#[must_use]
pub fn is_package_name(name: &str) -> bool {
    let unscoped = match name.strip_prefix('@') {
        Some(scoped) => match scoped.split_once('/') {
            Some((scope, unscoped)) if is_segment(scope) => unscoped,
            _ => return false,
        },
        None => name,
    };
    is_segment(unscoped)
}

fn is_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains(['/', '\\', ':', '\0'])
}

/// The `name` the manifest in `dir` gives its package.
///
/// What `uf link DIR` is about to link, read before the manager runs so the
/// report can find it in `node_modules` afterwards. A manifest that is missing,
/// is not a regular file, is too large, is not JSON, or names no package name
/// gives `None`, and the manager says why it cannot link it, in its own words.
#[must_use]
pub fn package_name(dir: &Utf8Path) -> Option<String> {
    let manifest = read_manifest(&dir.join("package.json"))?;
    let name = manifest.get("name")?.as_str()?;
    is_package_name(name).then(|| name.to_owned())
}

/// The `resolutions` entry in the manifest at `root` that points `name` at a
/// directory, written `portal:` or `link:` — which is how Yarn 2+ records
/// `yarn link`.
#[must_use]
pub fn linked_resolution(root: &Utf8Path, name: &str) -> Option<String> {
    if is_polluting_json_key(name) {
        return None;
    }
    let manifest = read_manifest(&root.join("package.json"))?;
    let resolution = manifest.get("resolutions")?.get(name)?.as_str()?;
    (resolution.starts_with("portal:") || resolution.starts_with("link:"))
        .then(|| resolution.to_owned())
}

/// A manifest, when it is a regular file of a readable size holding JSON.
fn read_manifest(path: &Utf8Path) -> Option<Value> {
    // `symlink_metadata`, so a manifest that is a link to somewhere else is
    // not followed there.
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let mut source = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_MANIFEST_BYTES)
        .read_to_string(&mut source)
        .ok()?;
    serde_json::from_str(&source).ok()
}

#[cfg(test)]
mod tests;
