//! The toolchain half of `uf.lock`: which release each version prefix resolved
//! to.
//!
//! # Why a prefix is locked
//!
//! `runtime: "node@26"` is the newest 26.x — today. Resolving it on every
//! command would make "which Node does this project run on" a question whose
//! answer changes the moment nodejs.org publishes, on one machine before
//! another. So the answer is written down the first time it is asked and read
//! from there by every machine until `uf env update` moves it: the bargain a
//! lockfile makes for a dependency range, made for the toolchain.
//!
//! # Why in `uf.lock`
//!
//! One file a project commits to say what everything resolved to, rather than a
//! second lockfile with a second set of rules about when to commit it. `uf.lock`
//! is also what uf's own resolver writes, and two things follow from sharing it,
//! each handled where it would bite:
//!
//! * a `uf.lock` holding nothing but this record is not evidence that a project
//!   uses uf's resolver, so `uf_pm`'s detection skips one — a pnpm project that
//!   locks a Node does not become a uf-resolver project; see
//!   [`is_toolchain_only`];
//! * `uf install` rewrites `uf.lock` from the manifests, and carries this record
//!   over rather than dropping it.
//!
//! # The shape
//!
//! ```json
//! {
//!   "toolchain": {
//!     "bun@1.4": "1.4.2",
//!     "node@26": "26.8.2"
//!   }
//! }
//! ```
//!
//! Keyed by the spec as written and sorted, so the record is a function of the
//! declarations and a diff of it reads as "this prefix moved". Only prefixes
//! are recorded: an exact version needs no lock, and a spec with no version is
//! whatever is on `PATH`.

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Map, Value};

use crate::EnvError;
use crate::tool::Tool;

/// Held while one process reads, changes and replaces `uf.lock`.
///
/// # Why a guard, and not only an atomic rename
///
/// A rename means nobody reads half a file. It does not mean nobody's change is
/// lost: two commands started together in one checkout — a dev server and a
/// test watch, each on a prefix nothing had locked, or `uf env update` beside
/// `uf install` — both read `uf.lock`, each writes back what it read plus its
/// own change, and the second rename drops the first command's. The next run
/// resolves that prefix again, perhaps to a release published in between,
/// which is the drift the lock exists to stop.
///
/// So every writer of `uf.lock` holds this for the whole read, change and
/// rename — [`crate::toolchain::resolve`] here, and `uf_pm`'s resolver when it
/// rewrites its half — and a second writer waits for the first. It is advisory,
/// and taken on a file of its own under `.uf/` rather than on `uf.lock`, whose
/// rename replaces the very file a lock would have been taken on.
///
/// Taken once per operation: a process that asked for it twice would wait on
/// itself. Dropping it lets it go.
#[derive(Debug)]
pub struct Guard {
    _file: fs::File,
}

/// The file the guard for the lockfile at `lockfile` is taken on:
/// `.uf/uf.lock.guard` beside it.
#[must_use]
pub fn guard_path(lockfile: &Utf8Path) -> Utf8PathBuf {
    let name = lockfile.file_name().unwrap_or("uf.lock");
    lockfile
        .parent()
        .unwrap_or_else(|| Utf8Path::new(""))
        .join(".uf")
        .join(uf_infra::into_string(uf_infra::cstr!("{name}.guard")))
}

/// Wait for, and take, the guard on the lockfile at `lockfile`.
///
/// # Errors
///
/// When the guard file cannot be created or locked.
pub fn guard(lockfile: &Utf8Path) -> Result<Guard, EnvError> {
    let path = guard_path(lockfile);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| EnvError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| EnvError::Write {
            path: path.clone(),
            source,
        })?;
    file.lock()
        .map_err(|source| EnvError::Write { path, source })?;
    Ok(Guard { _file: file })
}

/// The key the record lives under.
pub const KEY: &str = "toolchain";

/// Which release each locked prefix resolved to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolchainLock {
    entries: BTreeMap<String, String>,
}

impl ToolchainLock {
    /// The release `tool@prefix` is locked to, when it is.
    #[must_use]
    pub fn get(&self, tool: Tool, prefix: &str) -> Option<&str> {
        self.entries.get(&spec(tool, prefix)).map(String::as_str)
    }

    /// Lock `tool@prefix` to `version`, returning what it was locked to before.
    pub fn insert(&mut self, tool: Tool, prefix: &str, version: &str) -> Option<String> {
        self.entries.insert(spec(tool, prefix), version.to_owned())
    }

    /// Keep only the entries whose spec `keep` answers `true` for.
    ///
    /// What makes the record a function of the declarations: a prefix a project
    /// stopped writing is a lock nothing reads, and a lock nothing reads is a
    /// line that goes stale in every diff until somebody wonders what it is.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.entries.retain(|spec, _| keep(spec));
    }

    /// Every entry, as `(spec, version)`, sorted by spec.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(spec, version)| (spec.as_str(), version.as_str()))
    }

    /// Whether nothing is locked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `node@26`.
fn spec(tool: Tool, prefix: &str) -> String {
    uf_infra::into_string(uf_infra::cstr!("{}@{prefix}", tool.name()))
}

/// Read the record out of the `uf.lock` at `path`.
///
/// A file that is not there locks nothing, which is every project until its
/// first prefix is resolved.
///
/// # Errors
///
/// When the file exists and is not a JSON object, or its record is not an
/// object of strings. Refused rather than read as empty, because an unreadable
/// lock read as empty is a lock silently re-resolved to something newer.
///
/// And when an entry is not a tool uf installs, a version prefix, and one exact
/// release. That is a security boundary as well as a format: `uf.lock` arrives
/// with a cloned repository, and a locked version becomes a directory name in
/// the store and part of a download URL, so `x/../../outside` must be refused
/// here rather than trusted as far as `Store::path`.
pub fn read(path: &Utf8Path) -> Result<ToolchainLock, EnvError> {
    let Some(document) = document(path)? else {
        return Ok(ToolchainLock::default());
    };
    let Some(record) = document.get(KEY) else {
        return Ok(ToolchainLock::default());
    };
    let Value::Object(record) = record else {
        return Err(unreadable(path, "`toolchain` is not an object"));
    };
    let mut entries = BTreeMap::new();
    for (spec, version) in record {
        let Value::String(version) = version else {
            return Err(unreadable(
                path,
                uf_infra::cstr!("`toolchain.{spec}` is not a version string").as_str(),
            ));
        };
        if !is_locked_spec(spec) {
            return Err(unreadable(
                path,
                &uf_infra::into_string(uf_infra::cstr!(
                    "`toolchain.{spec}` is not a tool and a version prefix, such as `node@26`"
                )),
            ));
        }
        if !crate::project::is_exact_version(version) {
            return Err(unreadable(
                path,
                &uf_infra::into_string(uf_infra::cstr!(
                    "`toolchain.{spec}` is `{version}`, which is not a release"
                )),
            ));
        }
        entries.insert(spec.clone(), version.clone());
    }
    Ok(ToolchainLock { entries })
}

/// Whether `spec` is what the record is keyed by: a tool uf installs and a
/// numeric version prefix — `node@26`, `bun@1.4`.
fn is_locked_spec(spec: &str) -> bool {
    let Some((name, prefix)) = spec.split_once('@') else {
        return false;
    };
    let parts: Vec<&str> = prefix.split('.').collect();
    Tool::parse(name).is_some()
        && (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Write the record into the `uf.lock` at `path`, leaving everything else in the
/// file exactly where it was.
///
/// The rest of the file is uf's resolver's, and is carried through key for key
/// in its own order. An empty record removes the key, and a file left holding
/// nothing at all is removed: `{}` in `uf.lock` is what an empty resolver lock
/// looks like, and leaving one behind would turn a project that only ever
/// locked a Node into one that reads as using uf's resolver.
///
/// Written the way `uf_pm` writes the file — pretty-printed, two spaces, a
/// trailing newline — under a temporary name and renamed into place.
///
/// # Errors
///
/// When the existing file is not a JSON object, or the file cannot be written
/// or removed.
pub fn write(path: &Utf8Path, lock: &ToolchainLock) -> Result<(), EnvError> {
    let existed = path.is_file();
    let mut document = document(path)?.unwrap_or_default();
    if lock.is_empty() {
        document.shift_remove(KEY);
    } else {
        let record: Map<String, Value> = lock
            .entries()
            .map(|(spec, version)| (spec.to_owned(), Value::String(version.to_owned())))
            .collect();
        document.insert(KEY.to_owned(), Value::Object(record));
    }

    if document.is_empty() {
        if existed {
            fs::remove_file(path).map_err(|source| EnvError::Write {
                path: path.to_path_buf(),
                source,
            })?;
        }
        return Ok(());
    }

    let mut text =
        serde_json::to_string_pretty(&Value::Object(document)).map_err(EnvError::Encode)?;
    text.push('\n');
    let staging = path.with_file_name(uf_infra::into_string(uf_infra::cstr!(
        ".{}.{}",
        path.file_name().unwrap_or("uf.lock"),
        std::process::id()
    )));
    fs::write(&staging, text).map_err(|source| EnvError::Write {
        path: staging.clone(),
        source,
    })?;
    fs::rename(&staging, path).map_err(|source| {
        let _ = fs::remove_file(&staging);
        EnvError::Write {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Whether `text` is a `uf.lock` that holds this record and nothing else.
///
/// What `uf_pm`'s lockfile detection asks before counting a `uf.lock` as a
/// vote for uf's resolver. Anything else — the resolver's own fields, an empty
/// object, text that is not JSON — answers `false` and votes as it always did,
/// so the only file this changes the answer for is one this module wrote.
#[must_use]
pub fn is_toolchain_only(text: &str) -> bool {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(document)) => document.len() == 1 && document.contains_key(KEY),
        _ => false,
    }
}

/// The file as a JSON object, or `None` when it is not there.
fn document(path: &Utf8Path) -> Result<Option<Map<String, Value>>, EnvError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(EnvError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(document)) => Ok(Some(document)),
        Ok(_) => Err(unreadable(path, "it is not a JSON object")),
        Err(error) => Err(unreadable(path, &error.to_string())),
    }
}

fn unreadable(path: &Utf8Path, detail: &str) -> EnvError {
    EnvError::LockUnreadable {
        path: path.to_path_buf(),
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests;
