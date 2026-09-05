//! Which repositories are using which store entries.
//!
//! # Why a root is a file outside the repository
//!
//! Collection has to know what is in use, and it cannot find that out by
//! looking at the store: an entry does not record who linked it. So each
//! repository writes a root — a small file naming itself and the entries it
//! uses — into uf's state directory, and collection reads those.
//!
//! The root lives outside the repository on purpose. A root inside a checkout
//! disappears when the checkout is deleted, which is exactly when its entries
//! stop being needed; but it also disappears when the checkout is *moved*,
//! and then the entries are still in use and nothing says so. Keeping the
//! root outside means a moved repository re-registers itself on the next
//! command and the stale root is collected because the path it names is gone.
//!
//! # Why the repository path is hashed
//!
//! A file name has to be one path component, and a repository path is not.
//! The hash names the file; the path is recorded inside it, because a
//! directory of hashes nobody can read is a directory nobody can audit.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::EnvError;
use crate::store::state_home;

/// What one repository is using.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    /// The repository this root speaks for.
    pub repository: Utf8PathBuf,
    /// The store entries it uses, by directory name.
    pub entries: Vec<String>,
}

impl Root {
    /// Whether the repository this root names is still there.
    ///
    /// A root whose repository has been deleted is holding entries for
    /// nobody, and is what collection prunes first.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.repository.is_dir()
    }
}

/// The directory of roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roots {
    root: Utf8PathBuf,
}

impl Roots {
    /// The roots directory at `root`.
    #[must_use]
    pub fn new(root: impl Into<Utf8PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The roots this machine uses: `$UF_ROOTS`, or
    /// `$XDG_STATE_HOME/uf/roots`.
    ///
    /// # Errors
    ///
    /// When neither `$XDG_STATE_HOME` nor `$HOME` is set.
    pub fn discover() -> Result<Self, EnvError> {
        if let Ok(explicit) = std::env::var("UF_ROOTS")
            && !explicit.trim().is_empty()
        {
            return Ok(Self::new(explicit));
        }
        Ok(Self::new(state_home()?.join("uf").join("roots")))
    }

    /// Where the roots are.
    #[must_use]
    pub fn path(&self) -> &Utf8Path {
        &self.root
    }

    /// Record that `repository` uses `entries`, replacing what it used before.
    ///
    /// Replacing rather than adding: a project that drops a tool from
    /// `uf.config.js` should stop holding it, and the next install is the
    /// moment that becomes true.
    ///
    /// # Errors
    ///
    /// When the root cannot be written.
    pub fn register(&self, repository: &Utf8Path, entries: &[String]) -> Result<(), EnvError> {
        fs::create_dir_all(&self.root).map_err(|source| EnvError::Write {
            path: self.root.clone(),
            source,
        })?;
        let mut entries = entries.to_vec();
        entries.sort();
        entries.dedup();
        let root = Root {
            repository: repository.to_path_buf(),
            entries,
        };
        let file = self.file_for(repository);
        let body = serde_json::to_string_pretty(&root).map_err(EnvError::Encode)?;
        fs::write(&file, format!("{body}\n")).map_err(|source| EnvError::Write {
            path: file,
            source,
        })
    }

    /// Every root, with the file it came from.
    ///
    /// A file that does not parse is reported rather than skipped: it is
    /// holding entries and nobody can tell which, and silently ignoring it
    /// would make collection delete things that are in use.
    ///
    /// # Errors
    ///
    /// When the directory cannot be read, or a root in it cannot be parsed.
    pub fn all(&self) -> Result<Vec<(Utf8PathBuf, Root)>, EnvError> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Ok(Vec::new());
        };
        let mut found = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| EnvError::Read {
                path: self.root.clone(),
                source,
            })?;
            let path = Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|path| EnvError::NotUtf8 { path })?;
            if path.extension() != Some("json") {
                continue;
            }
            let body = fs::read_to_string(&path).map_err(|source| EnvError::Read {
                path: path.clone(),
                source,
            })?;
            let root = serde_json::from_str(&body).map_err(|source| EnvError::Decode {
                path: path.clone(),
                source,
            })?;
            found.push((path, root));
        }
        found.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(found)
    }

    /// Forget one root.
    ///
    /// # Errors
    ///
    /// When the file exists and cannot be removed.
    pub fn forget(&self, file: &Utf8Path) -> Result<(), EnvError> {
        match fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(EnvError::Write {
                path: file.to_path_buf(),
                source,
            }),
        }
    }

    /// The file a repository's root is written to.
    fn file_for(&self, repository: &Utf8Path) -> Utf8PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(repository.as_str().as_bytes());
        let digest = hasher.finalize();
        // Sixteen hex characters is 64 bits: enough that two repositories on
        // one machine will not collide, short enough to read.
        let mut name = String::with_capacity(16);
        for byte in digest.iter().take(8) {
            name.push_str(&format!("{byte:02x}"));
        }
        self.root.join(format!("{name}.json"))
    }
}
