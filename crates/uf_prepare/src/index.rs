//! Checking what is staged, rather than what is on disk.
//!
//! A file can be half staged: `git add -p` puts some of its changes in the
//! index and leaves the rest in the working tree. The commit holds the staged
//! half, so that is the half a pre-commit check has to read — and `uf prepare`
//! read the file on disk, which checked a version nobody was committing.
//!
//! So for the length of a run, [`StagedView`] puts the staged content of every
//! half-staged file into the working tree, having first copied what was there
//! to `.uf/prepare/unstaged/`, and puts it back when the run is over. Every
//! step in between — the generators, the staged tasks, `uf lint`, `uf fmt` —
//! reads what the commit will hold through the same file reads it always made.
//! A fully staged file needs nothing: its working-tree copy already is what is
//! staged.
//!
//! `git checkout-index` writes the staged content rather than uf reading the
//! blob and writing it out, because the index holds the *clean* form of a
//! file and a checkout applies the repository's filters and line endings to
//! it. What a step reads is then what `git checkout` would have produced.
//!
//! # Rewrites
//!
//! A step that rewrites a fully staged file has rewritten what is being
//! committed, and the caller may stage the rewrite. A step that rewrites a
//! half-staged file has rewritten a mixture that exists only for this run, and
//! putting the unstaged half back discards it; [`StagedView::close`] names
//! those files, so a run can say it dropped a fix instead of dropping it in
//! silence.
//!
//! # A run that is killed
//!
//! The copies and a manifest naming them are on disk before anything is
//! overwritten, so a run that dies half way leaves what is needed to undo it,
//! and the next run puts the working tree back before it does anything else:
//! [`recover_interrupted`].

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};

use crate::git_command;

/// Where the working-tree copies are kept while a run holds them.
const HELD: &str = ".uf/prepare/unstaged";

/// Why the staged content could not be put in place, or taken back out.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// Git refused, in its own words.
    #[error("git could not {action}: {message}")]
    Git {
        /// What was being asked.
        action: &'static str,
        /// Git's first line.
        message: String,
    },
    /// A file could not be copied, written or removed.
    #[error("could not {action} {path}: {source}")]
    Io {
        /// What was being done.
        action: &'static str,
        /// To which file.
        path: Utf8PathBuf,
        /// Why not.
        source: std::io::Error,
    },
}

/// One file a run is holding the working-tree copy of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Held {
    path: CompactString,
    /// Whether there was a working-tree copy to hold at all: a file staged
    /// and then deleted has none, and putting it back means deleting it again.
    existed: bool,
}

/// The working tree showing what is staged, for as long as this value lives.
#[derive(Debug)]
pub struct StagedView {
    root: Utf8PathBuf,
    held: Vec<Held>,
    restored: bool,
}

impl StagedView {
    /// Put the staged content of every half-staged file among `staged` — paths
    /// relative to `root` — into the working tree.
    ///
    /// # Errors
    ///
    /// When git cannot say which files are half staged, or a copy cannot be
    /// made. Nothing has been overwritten when either happens.
    pub fn open(root: &Utf8Path, staged: &[CompactString]) -> Result<Self, IndexError> {
        let half = changed_since_staged(root, staged)?;
        let mut view = Self {
            root: root.to_path_buf(),
            held: Vec::with_capacity(half.len()),
            restored: half.is_empty(),
        };
        if half.is_empty() {
            return Ok(view);
        }

        let held = root.join(HELD);
        // Made here rather than beside the first copy: a staged file deleted
        // from the working tree has no copy to make, and its manifest entry is
        // the only record that it has to be deleted again.
        fs::create_dir_all(&held).map_err(|source| io("create", &held, source))?;
        for path in &half {
            let from = root.join(path.as_str());
            let existed = from.is_file();
            if existed {
                let to = held.join("files").join(path.as_str());
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent).map_err(|source| io("create", parent, source))?;
                }
                fs::copy(&from, &to).map_err(|source| io("keep a copy of", &from, source))?;
            }
            view.held.push(Held {
                path: path.clone(),
                existed,
            });
        }
        let manifest = serde_json::to_vec(&view.held).unwrap_or_default();
        fs::write(held.join("manifest.json"), manifest)
            .map_err(|source| io("write", &held.join("manifest.json"), source))?;

        let mut checkout = git_command(root);
        checkout.args(["checkout-index", "--force", "--"]);
        checkout.args(half.iter().map(CompactString::as_str));
        run(checkout, "write the staged content")?;
        Ok(view)
    }

    /// The half-staged files, relative to the project root.
    pub fn half_staged(&self) -> impl Iterator<Item = &str> {
        self.held.iter().map(|held| held.path.as_str())
    }

    /// Put every working-tree copy back, and name the half-staged files a step
    /// rewrote in the meantime — the rewrites this discards.
    ///
    /// # Errors
    ///
    /// When a copy cannot be put back. It is still on disk under
    /// `.uf/prepare/unstaged/`, and the next run tries again.
    pub fn close(mut self) -> Result<Vec<CompactString>, IndexError> {
        let paths: Vec<CompactString> = self.held.iter().map(|held| held.path.clone()).collect();
        let rewritten = changed_since_staged(&self.root, &paths)?;
        self.restore()?;
        Ok(rewritten)
    }

    fn restore(&mut self) -> Result<(), IndexError> {
        if self.restored {
            return Ok(());
        }
        put_back(&self.root, &self.held)?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for StagedView {
    /// A run that returned early — an error, a panic unwinding — still puts
    /// the working tree back. A failure here leaves the copies for
    /// [`recover_interrupted`].
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Put back what a run that did not finish was holding, and name the files.
///
/// # Errors
///
/// When a copy cannot be put back; it stays where it is.
pub fn recover_interrupted(root: &Utf8Path) -> Result<Vec<CompactString>, IndexError> {
    let manifest = root.join(HELD).join("manifest.json");
    let Ok(bytes) = fs::read(&manifest) else {
        return Ok(Vec::new());
    };
    let held: Vec<Held> = serde_json::from_slice(&bytes).unwrap_or_default();
    put_back(root, &held)?;
    Ok(held.into_iter().map(|held| held.path).collect())
}

/// Which of `paths` — staged files, relative to `root` — are not in the
/// working tree as they are in the index.
///
/// Before a run opens a [`StagedView`], these are the half-staged files.
/// While one is open, they are the files a step has rewritten.
///
/// # Errors
///
/// When git cannot be asked.
pub fn changed_since_staged(
    root: &Utf8Path,
    paths: &[CompactString],
) -> Result<Vec<CompactString>, IndexError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut diff = git_command(root);
    diff.args(["diff", "--name-only", "-z", "--relative", "--"]);
    diff.args(paths.iter().map(CompactString::as_str));
    let stdout = run(diff, "compare the working tree with the index")?;
    let mut changed: Vec<CompactString> = stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .map(ToCompactString::to_compact_string)
        .filter(|path| paths.contains(path))
        .collect();
    changed.sort_unstable();
    changed.dedup();
    Ok(changed)
}

/// Stage `paths`, relative to `root`, as they are in the working tree.
///
/// # Errors
///
/// When git refuses.
pub fn stage(root: &Utf8Path, paths: &[CompactString]) -> Result<(), IndexError> {
    if paths.is_empty() {
        return Ok(());
    }
    let mut add = git_command(root);
    add.args(["add", "--"]);
    add.args(paths.iter().map(CompactString::as_str));
    run(add, "stage the rewritten files").map(|_| ())
}

fn put_back(root: &Utf8Path, held: &[Held]) -> Result<(), IndexError> {
    let directory = root.join(HELD);
    for file in held {
        let to = root.join(file.path.as_str());
        if file.existed {
            let from = directory.join("files").join(file.path.as_str());
            fs::copy(&from, &to).map_err(|source| io("put back", &to, source))?;
        } else if to.exists() {
            fs::remove_file(&to).map_err(|source| io("remove", &to, source))?;
        }
    }
    fs::remove_dir_all(&directory).map_err(|source| io("remove", &directory, source))
}

fn run(mut command: std::process::Command, action: &'static str) -> Result<Vec<u8>, IndexError> {
    let output = command.output().map_err(|error| IndexError::Git {
        action,
        message: error.to_string(),
    })?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(IndexError::Git {
        action,
        message: stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("git exited with a failure")
            .to_owned(),
    })
}

fn io(action: &'static str, path: &Utf8Path, source: std::io::Error) -> IndexError {
    IndexError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        git(&root, &["init", "--quiet"]);
        (dir, root)
    }

    fn git(root: &Utf8Path, args: &[&str]) {
        let status = git_command(root).args(args).status().unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn names(paths: &[&str]) -> Vec<CompactString> {
        paths.iter().map(|path| CompactString::new(path)).collect()
    }

    /// `a.txt` staged as `staged` and then edited to `unstaged`; `b.txt` fully
    /// staged.
    fn half_staged() -> (tempfile::TempDir, Utf8PathBuf) {
        let (dir, root) = repository();
        fs::write(root.join("a.txt"), "staged\n").unwrap();
        fs::write(root.join("b.txt"), "whole\n").unwrap();
        git(&root, &["add", "a.txt", "b.txt"]);
        fs::write(root.join("a.txt"), "unstaged\n").unwrap();
        (dir, root)
    }

    #[test]
    fn a_half_staged_file_reads_as_staged_while_the_view_is_open() {
        let (_dir, root) = half_staged();
        let staged = names(&["a.txt", "b.txt"]);

        let view = StagedView::open(&root, &staged).unwrap();
        assert_eq!(view.half_staged().collect::<Vec<_>>(), vec!["a.txt"]);
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "staged\n");
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "whole\n");

        assert!(view.close().unwrap().is_empty(), "nothing rewrote anything");
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "unstaged\n"
        );
        assert!(!root.join(HELD).exists(), "the copies are cleaned up");
    }

    #[test]
    fn a_rewrite_of_a_half_staged_file_is_named_and_not_kept() {
        let (_dir, root) = half_staged();
        let view = StagedView::open(&root, &names(&["a.txt", "b.txt"])).unwrap();
        fs::write(root.join("a.txt"), "rewritten\n").unwrap();

        assert_eq!(view.close().unwrap(), names(&["a.txt"]));
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "unstaged\n"
        );
    }

    #[test]
    fn a_view_dropped_without_closing_still_puts_the_working_tree_back() {
        let (_dir, root) = half_staged();
        drop(StagedView::open(&root, &names(&["a.txt"])).unwrap());
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "unstaged\n"
        );
    }

    /// A run killed while it held the copies — here, a view that is never
    /// dropped — is undone by the next one.
    #[test]
    fn an_interrupted_run_is_put_back_by_the_next() {
        let (_dir, root) = half_staged();
        std::mem::forget(StagedView::open(&root, &names(&["a.txt"])).unwrap());
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "staged\n");

        assert_eq!(recover_interrupted(&root).unwrap(), names(&["a.txt"]));
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "unstaged\n"
        );
        assert!(recover_interrupted(&root).unwrap().is_empty());
    }

    #[test]
    fn a_staged_file_deleted_from_the_working_tree_is_deleted_again() {
        let (_dir, root) = half_staged();
        fs::remove_file(root.join("b.txt")).unwrap();

        let view = StagedView::open(&root, &names(&["b.txt"])).unwrap();
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "whole\n");
        view.close().unwrap();
        assert!(!root.join("b.txt").exists());
    }

    #[test]
    fn staging_a_rewrite_puts_it_in_the_index() {
        let (_dir, root) = half_staged();
        fs::write(root.join("b.txt"), "fixed\n").unwrap();
        assert_eq!(
            changed_since_staged(&root, &names(&["b.txt"])).unwrap(),
            names(&["b.txt"])
        );
        stage(&root, &names(&["b.txt"])).unwrap();
        assert!(
            changed_since_staged(&root, &names(&["b.txt"]))
                .unwrap()
                .is_empty()
        );
    }
}
