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
use std::io::{self, Read};
use std::process::{Command, Stdio};

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;

use crate::detect::{MAX_MANIFEST_BYTES, PackageManager, YarnEdition};
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

/// The directories each manager keeps registered packages in, found the way
/// that manager finds them.
///
/// pnpm, Yarn 1 and bun are read from the environment and each manager's own
/// defaults. npm is asked (`npm root --global`), because its global directory
/// is a setting with more sources — `.npmrc`, `npm_config_prefix`, where Node
/// is installed — than uf should try to reproduce.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalDirs {
    /// npm's global `node_modules`.
    pub npm: Option<Utf8PathBuf>,
    /// pnpm's home; `global/<layout>/node_modules` in it holds what
    /// `pnpm link` registered.
    pub pnpm: Option<Utf8PathBuf>,
    /// Yarn 1's link folder.
    pub yarn: Option<Utf8PathBuf>,
    /// bun's global directory; its `node_modules` holds what `bun link`
    /// registered.
    pub bun: Option<Utf8PathBuf>,
}

impl GlobalDirs {
    /// The directories `manager` uses, for the project at `root`.
    ///
    /// `path` goes in front of `PATH` for npm, as it does for every manager uf
    /// runs: the release `packageManager` pins, when it pins one.
    #[must_use]
    pub fn for_manager(manager: PackageManager, root: &Utf8Path, path: &[Utf8PathBuf]) -> Self {
        match manager {
            PackageManager::Npm | PackageManager::Uf => Self {
                npm: npm_global_root(root, path),
                ..Self::default()
            },
            _ => Self::from_env(&|name| std::env::var(name).ok().filter(|value| !value.is_empty())),
        }
    }

    /// pnpm's, Yarn 1's and bun's directories, from the variables `env` reads.
    #[must_use]
    pub fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Self {
        let home = env("HOME")
            .or_else(|| env("USERPROFILE"))
            .map(Utf8PathBuf::from);
        let local = env("LOCALAPPDATA").map(Utf8PathBuf::from);
        let data = env("XDG_DATA_HOME").map(Utf8PathBuf::from);

        let pnpm = env("PNPM_HOME")
            .map(Utf8PathBuf::from)
            .or_else(|| data.as_ref().map(|data| data.join("pnpm")))
            .or_else(|| {
                if cfg!(windows) {
                    local.as_ref().map(|local| local.join("pnpm"))
                } else if cfg!(target_os = "macos") {
                    home.as_ref().map(|home| home.join("Library/pnpm"))
                } else {
                    home.as_ref().map(|home| home.join(".local/share/pnpm"))
                }
            });
        let yarn = if cfg!(windows) {
            local.as_ref().map(|local| local.join("Yarn/Data/link"))
        } else {
            data.as_ref()
                .map(|data| data.join("yarn/link"))
                .or_else(|| home.as_ref().map(|home| home.join(".config/yarn/link")))
        };
        let bun = env("BUN_INSTALL_GLOBAL_DIR")
            .map(Utf8PathBuf::from)
            .or_else(|| {
                env("BUN_INSTALL").map(|install| Utf8PathBuf::from(install).join("install/global"))
            })
            .or_else(|| home.as_ref().map(|home| home.join(".bun/install/global")));

        Self {
            npm: None,
            pnpm,
            yarn,
            bun,
        }
    }
}

/// `npm root --global`, asked in the project, or `None` when npm cannot say.
///
/// What npm prints is masked before uf reads it, so the answer is put back
/// together by [`unmasked`] before it is used as a path.
fn npm_global_root(root: &Utf8Path, path: &[Utf8PathBuf]) -> Option<Utf8PathBuf> {
    let mut command = Command::new("npm");
    if let Some(path) = crate::run::prefixed_path(path) {
        command.env("PATH", path);
    }
    let output = command
        .args(["root", "--global"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let printed = String::from_utf8(output.stdout).ok()?;
    let printed = printed.trim();
    if printed.is_empty() {
        return None;
    }
    unmasked(Utf8Path::new(printed))
}

/// What npm prints in place of anything in its output it takes for a secret.
const MASKED: &str = "***";

/// `printed` with every segment npm masked put back from the filesystem.
///
/// npm redacts its own output, and a UUID counts: a global directory under
/// `/var/folders/…/a8f1c2e4-…/lib/node_modules` is printed with `***` where
/// that segment was, by `npm root --global`, by `npm prefix --global`, by
/// `--json` and by `--parseable` alike. npm 11 answers `npm config get prefix`
/// with "the prefix option is protected", so there is no channel to ask again
/// on; the path uf was given is simply not the path on disk, and a directory
/// nobody can read is read as nothing registered (ubugeeei-prod/uf#976).
///
/// A masked segment is put back as the one entry of its parent directory that
/// makes the rest of the path exist. Nothing that fits, or more than one, is
/// no answer: uf removes things from this directory, so a guess is worse than
/// saying it cannot find it. An answer with no mask in it is returned as
/// printed, existing or not — a global directory npm has never had to create
/// holds no registration either way.
fn unmasked(printed: &Utf8Path) -> Option<Utf8PathBuf> {
    let segments = printed
        .components()
        .map(|segment| segment.as_str())
        .collect::<Vec<_>>();
    if !segments.contains(&MASKED) {
        return Some(printed.to_owned());
    }
    put_back(Utf8PathBuf::new(), &segments)
}

/// `base` joined with `segments`, looking up each masked one, or `None` when
/// the path that makes is not one directory.
fn put_back(base: Utf8PathBuf, segments: &[&str]) -> Option<Utf8PathBuf> {
    let Some((segment, rest)) = segments.split_first() else {
        return base.is_dir().then_some(base);
    };
    if *segment != MASKED {
        return put_back(base.join(segment), rest);
    }
    let mut found = None;
    for entry in base.read_dir_utf8().ok()?.filter_map(Result::ok) {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(fits) = put_back(entry.into_path(), rest) else {
            continue;
        };
        if found.is_some() {
            return None;
        }
        found = Some(fits);
    }
    found
}

/// Where `manager` keeps a package registered as `name`: one place for npm,
/// Yarn 1 and bun, one per store layout pnpm has used, and none for Yarn 2+,
/// which keeps no registry.
#[must_use]
pub fn registry_entries(
    manager: PackageManager,
    name: &str,
    dirs: &GlobalDirs,
) -> Vec<Utf8PathBuf> {
    if !is_package_name(name) {
        return Vec::new();
    }
    match manager {
        PackageManager::Npm | PackageManager::Uf => {
            dirs.npm.iter().map(|root| root.join(name)).collect()
        }
        PackageManager::Pnpm => dirs
            .pnpm
            .iter()
            .flat_map(|home| layouts(&home.join("global")))
            .map(|layout| layout.join("node_modules").join(name))
            .collect(),
        PackageManager::Yarn(YarnEdition::Classic) => {
            dirs.yarn.iter().map(|links| links.join(name)).collect()
        }
        PackageManager::Yarn(YarnEdition::Berry) => Vec::new(),
        PackageManager::Bun => dirs
            .bun
            .iter()
            .map(|global| global.join("node_modules").join(name))
            .collect(),
    }
}

/// The store layouts in pnpm's global directory — `5` is pnpm 10's — in a
/// stable order. A package an older pnpm registered is still registered.
fn layouts(global: &Utf8Path) -> Vec<Utf8PathBuf> {
    let Ok(entries) = global.read_dir_utf8() else {
        return Vec::new();
    };
    let mut layouts = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(camino::Utf8DirEntry::into_path)
        .collect::<Vec<_>>();
    layouts.sort();
    layouts
}

/// What a registry holds under a package's name, as far as one package's
/// directory is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Registration {
    /// A link at this entry to the package's directory: what `uf link` left.
    Linked(Utf8PathBuf),
    /// A link to a different directory, or to nothing.
    Elsewhere {
        /// The registry entry.
        entry: Utf8PathBuf,
        /// Where it leads, or what it says when it leads nowhere.
        to: Utf8PathBuf,
    },
    /// A package installed at this entry, which is not a link at all.
    Installed(Utf8PathBuf),
    /// Nothing under the name.
    Absent,
}

/// What `entries` hold for the package in `dir`.
///
/// A link to `dir` anywhere among them is the registration; otherwise the
/// first thing found under the name says why there is nothing to remove.
#[must_use]
pub fn registration(entries: &[Utf8PathBuf], dir: &Utf8Path) -> Registration {
    let dir = dir.canonicalize_utf8().ok();
    let mut found = Registration::Absent;
    for entry in entries {
        let Ok(metadata) = fs::symlink_metadata(entry) else {
            continue;
        };
        let other = if metadata.file_type().is_symlink() {
            match entry.canonicalize_utf8() {
                Ok(to) if dir.as_ref() == Some(&to) => return Registration::Linked(entry.clone()),
                Ok(to) => Registration::Elsewhere {
                    entry: entry.clone(),
                    to,
                },
                Err(_) => Registration::Elsewhere {
                    entry: entry.clone(),
                    to: fs::read_link(entry)
                        .ok()
                        .and_then(|to| Utf8PathBuf::from_path_buf(to).ok())
                        .unwrap_or_default(),
                },
            }
        } else {
            Registration::Installed(entry.clone())
        };
        if found == Registration::Absent {
            found = other;
        }
    }
    found
}

/// Remove the link at `node_modules/<name>` under `root`, and never anything it
/// leads to.
///
/// For bun, whose `bun unlink <name>` is "not implemented yet". Anything that
/// is not a link at the moment it is removed is refused: a directory there is
/// a package a manager installed, and taking one of those out is `uf remove`'s
/// job.
///
/// # Errors
///
/// When `name` is not a package name, when the entry is not a link, or when the
/// filesystem refuses.
pub fn remove_link(root: &Utf8Path, name: &str) -> io::Result<()> {
    if !is_package_name(name) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            uf_infra::into_string(uf_infra::cstr!("{name:?} is not a package name")),
        ));
    }
    let entry = root.join("node_modules").join(name);
    if !fs::symlink_metadata(&entry)?.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            uf_infra::into_string(uf_infra::cstr!("{entry} is not a link")),
        ));
    }
    // A link to a directory is a file to unlink on Unix and a directory to
    // remove on Windows, and neither call follows it.
    fs::remove_file(&entry).or_else(|error| {
        if cfg!(windows) {
            fs::remove_dir(&entry)
        } else {
            Err(error)
        }
    })
}

/// The override `pnpm link` wrote for `name` in the project's
/// `pnpm-workspace.yaml`, read the way [`remove_link_override`] would remove
/// it, without removing anything.
///
/// What tells a link `pnpm link` made from a `link:` dependency somebody
/// declared: pnpm 12 writes both for one link, and only the override is its
/// own.
#[must_use]
pub fn link_override(root: &Utf8Path, name: &str) -> Option<String> {
    if !is_package_name(name) {
        return None;
    }
    let path = root.join("pnpm-workspace.yaml");
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    let source = fs::read_to_string(&path).ok()?;
    without_link_override(&source, name).map(|(_, value)| value)
}

/// Take the override `pnpm link` wrote for `name` out of the project's
/// `pnpm-workspace.yaml`, and return the value it had.
///
/// pnpm 10's `pnpm unlink` answers "Nothing to unlink" and leaves that
/// override where `pnpm link` wrote it, so the link comes back on every
/// install. uf removes that one line and nothing else, and only when the file
/// is laid out the way pnpm writes it: a top-level `overrides:` block with one
/// `name: link:<path>` entry per line. A flow mapping, a comment on the line,
/// a value that is not a link, or the name listed twice leaves the file alone
/// and gives `None`, so a file someone wrote by hand is never rewritten on a
/// guess. A file the removal leaves empty is removed, because `pnpm link`
/// created it.
///
/// # Errors
///
/// When the file exists and cannot be read or written.
pub fn remove_link_override(root: &Utf8Path, name: &str) -> io::Result<Option<String>> {
    if !is_package_name(name) {
        return Ok(None);
    }
    let path = root.join("pnpm-workspace.yaml");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)?;
    let Some((rewritten, value)) = without_link_override(&source, name) else {
        return Ok(None);
    };
    if rewritten.trim().is_empty() {
        fs::remove_file(&path)?;
    } else {
        fs::write(&path, rewritten)?;
    }
    Ok(Some(value))
}

/// `source` without its `overrides` line for `name`, and that line's value, or
/// `None` when the file is not laid out the way pnpm writes one.
fn without_link_override(source: &str, name: &str) -> Option<(String, String)> {
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.trim_end() == "overrides:")?;
    let end = lines[start + 1..]
        .iter()
        .position(|line| !line.starts_with(' ') && !line.trim().is_empty())
        .map_or(lines.len(), |offset| start + 1 + offset);

    let mut found = None;
    for (index, line) in lines.iter().enumerate().take(end).skip(start + 1) {
        let line = line.trim_end_matches(['\n', '\r']);
        if line.trim().is_empty() {
            continue;
        }
        let entry = line.strip_prefix("  ")?;
        if entry.starts_with(' ') {
            return None;
        }
        let (key, value) = entry.split_once(": ")?;
        if unquote(key) != Some(name) {
            continue;
        }
        let value = unquote(value)?;
        if !value.starts_with("link:") || found.is_some() {
            return None;
        }
        found = Some((index, value.to_owned()));
    }
    let (index, value) = found?;

    let others = (start + 1..end).any(|other| other != index && !lines[other].trim().is_empty());
    let kept = lines
        .iter()
        .enumerate()
        .filter(|(line, _)| *line != index && (others || *line != start))
        .map(|(_, text)| *text)
        .collect::<String>();
    Some((kept, value))
}

/// A YAML scalar the way pnpm writes a key or a value: plain, or in single or
/// double quotes with nothing in them to unescape. `None` for anything else.
fn unquote(scalar: &str) -> Option<&str> {
    let scalar = scalar.trim();
    for quote in ['\'', '"'] {
        if let Some(inner) = scalar.strip_prefix(quote) {
            let inner = inner.strip_suffix(quote)?;
            return (!inner.contains(quote) && !inner.contains('\\')).then_some(inner);
        }
    }
    (!scalar.is_empty() && !scalar.contains(['#', '\'', '"', '{', '}', '[', ']', ',']))
        .then_some(scalar)
}

#[cfg(test)]
mod tests;
