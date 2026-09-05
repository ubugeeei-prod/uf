//! The store: one directory per tool, version and platform, shared by every
//! repository on the machine.
//!
//! # Why entries are immutable
//!
//! An entry is written once, under a temporary name, and renamed into place.
//! Nothing ever writes into an entry that exists. Two `uf env install` runs
//! racing on the same tool therefore cannot produce a half-unpacked Node that
//! the loser links into a project — the rename is atomic, and the loser
//! discards its own copy.
//!
//! It is also what lets a repository hold a *path* rather than a version. A
//! path that exists is a tool that works; there is no state where the
//! directory is there and its contents are still arriving.
//!
//! # Why the store is not in the repository
//!
//! Two repositories on the same Node download it once. That is the whole
//! reason to have a store rather than a `.uniflowed/node` per project, and it
//! is why collection has to exist: a directory nobody can attribute to a
//! project is a directory nobody deletes.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::EnvError;
use crate::tool::Pin;

/// The place installed tools live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    root: Utf8PathBuf,
}

impl Store {
    /// The store under `root`, which is `<data>/uf/store`.
    #[must_use]
    pub fn new(root: impl Into<Utf8PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The store this machine uses.
    ///
    /// `$UF_STORE` first, so a test and a curious reader can put it
    /// somewhere else; then `$XDG_DATA_HOME/uf/store`; then
    /// `$HOME/.local/share/uf/store`, which is what XDG says to do when the
    /// variable is unset.
    ///
    /// # Errors
    ///
    /// When neither `$XDG_DATA_HOME` nor `$HOME` is set, because then there
    /// is nowhere this could mean.
    pub fn discover() -> Result<Self, EnvError> {
        if let Some(explicit) = env_path("UF_STORE") {
            return Ok(Self::new(explicit));
        }
        Ok(Self::new(data_home()?.join("uf").join("store")))
    }

    /// Where the store keeps its entries.
    #[must_use]
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// Where `pin` lives, whether or not it is installed.
    #[must_use]
    pub fn path(&self, pin: &Pin) -> Utf8PathBuf {
        self.root.join(pin.slug())
    }

    /// Whether `pin` is installed.
    #[must_use]
    pub fn has(&self, pin: &Pin) -> bool {
        self.path(pin).is_dir()
    }

    /// Every entry in the store, by directory name, sorted.
    ///
    /// Names rather than parsed pins: an entry written by a newer uf that
    /// this one cannot parse is still an entry, still taking space, and still
    /// something collection has to reason about. Dropping it from the listing
    /// would make it invisible *and* immortal.
    ///
    /// # Errors
    ///
    /// When the store exists and cannot be read. A store that does not exist
    /// yet is empty, not an error.
    pub fn entries(&self) -> Result<Vec<String>, EnvError> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Ok(Vec::new());
        };
        let mut found = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| EnvError::Read {
                path: self.root.clone(),
                source,
            })?;
            if !entry.path().is_dir() {
                continue;
            }
            found.push(entry.file_name().to_string_lossy().into_owned());
        }
        found.sort();
        Ok(found)
    }

    /// Move `staged` into the store as `pin`, atomically.
    ///
    /// `staged` must be on the same filesystem — [`Store::staging`] gives a
    /// directory that is, which is why it exists rather than callers using a
    /// system temporary directory.
    ///
    /// A pin that is already installed wins: the staged copy is discarded and
    /// the existing entry is returned untouched. Installing is idempotent
    /// because that is what makes it safe to run on every `uf install`.
    ///
    /// # Errors
    ///
    /// When the store cannot be created or the rename fails for a reason
    /// other than the destination already existing.
    pub fn adopt(&self, pin: &Pin, staged: &Utf8Path) -> Result<Utf8PathBuf, EnvError> {
        let destination = self.path(pin);
        if destination.is_dir() {
            let _ = fs::remove_dir_all(staged);
            return Ok(destination);
        }
        fs::create_dir_all(&self.root).map_err(|source| EnvError::Write {
            path: self.root.clone(),
            source,
        })?;
        match fs::rename(staged, &destination) {
            Ok(()) => Ok(destination),
            // Somebody else finished first. Their copy is as good as this
            // one — the entry is identified by what is in it — so this is a
            // success, not a collision.
            Err(_) if destination.is_dir() => {
                let _ = fs::remove_dir_all(staged);
                Ok(destination)
            }
            Err(source) => Err(EnvError::Write {
                path: destination,
                source,
            }),
        }
    }

    /// A directory beside the store to unpack into, on the same filesystem.
    ///
    /// # Errors
    ///
    /// When it cannot be created.
    pub fn staging(&self, pin: &Pin) -> Result<Utf8PathBuf, EnvError> {
        let staging = self.root.join(format!(".staging-{}", pin.slug()));
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(|source| EnvError::Write {
            path: staging.clone(),
            source,
        })?;
        Ok(staging)
    }

    /// Delete one entry by its directory name.
    ///
    /// # Errors
    ///
    /// When the directory exists and cannot be removed.
    pub fn remove(&self, slug: &str) -> Result<(), EnvError> {
        let path = self.root.join(slug);
        match fs::remove_dir_all(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(EnvError::Write { path, source }),
        }
    }
}

/// `$XDG_DATA_HOME`, or `$HOME/.local/share`.
fn data_home() -> Result<Utf8PathBuf, EnvError> {
    if let Some(xdg) = env_path("XDG_DATA_HOME") {
        return Ok(xdg);
    }
    let home = env_path("HOME").ok_or(EnvError::NoHome)?;
    Ok(home.join(".local").join("share"))
}

/// `$XDG_STATE_HOME`, or `$HOME/.local/state`.
pub(crate) fn state_home() -> Result<Utf8PathBuf, EnvError> {
    if let Some(xdg) = env_path("XDG_STATE_HOME") {
        return Ok(xdg);
    }
    let home = env_path("HOME").ok_or(EnvError::NoHome)?;
    Ok(home.join(".local").join("state"))
}

/// A non-empty environment variable, as a path.
fn env_path(name: &str) -> Option<Utf8PathBuf> {
    let value = std::env::var(name).ok()?;
    (!value.trim().is_empty()).then(|| Utf8PathBuf::from(value))
}
