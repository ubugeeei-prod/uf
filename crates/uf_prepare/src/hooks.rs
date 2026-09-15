//! The git hook that runs `uf prepare` before a commit.
//!
//! A check nobody wires into git does not run. The usual way to wire one in
//! is a `postinstall` script, and uf refuses lifecycle scripts, so the wiring
//! is one explicit command instead: `uf prepare --install-hooks`.
//!
//! It writes a dispatcher to `.githooks/pre-commit`, which is meant to be
//! committed, and points this clone's `core.hooksPath` at `.githooks`. The
//! file travels with the repository; the setting cannot, because git keeps it
//! in `.git/config`, which is not versioned — so each clone runs the command
//! once, and nothing runs anything on a clone's behalf before it has.
//!
//! The dispatcher decides nothing. It finds `uf` and runs `uf prepare` in the
//! project, so what a commit is checked against is read from `uf.config.js`
//! by the same binary the rest of the project uses, and changing it never
//! means editing a hook.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};

use crate::git_command;

/// Where the dispatcher lives, relative to the repository root.
pub const HOOKS_DIRECTORY: &str = ".githooks";

/// The line that marks a `pre-commit` as one uf wrote. A hook without it is
/// somebody else's, and is never overwritten.
pub const DISPATCHER_MARK: &str = "# uf: written by `uf prepare --install-hooks`";

/// What installing the hook did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookInstall {
    /// The dispatcher, relative to the repository root.
    pub hook: Utf8PathBuf,
    /// Whether the file was written — `false` when it already said this.
    pub wrote: bool,
    /// Whether `core.hooksPath` was set — `false` when it already named
    /// [`HOOKS_DIRECTORY`].
    pub configured: bool,
}

/// Why the hook was not installed.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// There is no repository to hook into.
    #[error("this directory is not in a git working tree, so there is no commit to run before")]
    NotAWorkingTree,
    /// Git refused, in its own words.
    #[error("git could not {action}: {message}")]
    Git {
        /// What was being asked.
        action: &'static str,
        /// Git's first line.
        message: String,
    },
    /// A `pre-commit` uf did not write is already there.
    #[error(
        "{0} is a pre-commit hook uf did not write, and uf will not overwrite it\n\n  \
         move what it runs into `staged` in uf.config.js, or remove it, and run this again"
    )]
    ForeignHook(Utf8PathBuf),
    /// `core.hooksPath` already names another directory.
    #[error(
        "git's core.hooksPath is already {0:?}, and uf will not repoint it\n\n  \
         unset it with `git config --unset core.hooksPath`, or run `uf prepare` from a \
         pre-commit hook there"
    )]
    HooksPathTaken(String),
    /// The dispatcher could not be written.
    #[error("could not write {path}: {source}")]
    Write {
        /// Where.
        path: Utf8PathBuf,
        /// Why.
        source: std::io::Error,
    },
}

/// Where the hook stands in the repository around `root`, for `uf explain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookState {
    /// Git runs the dispatcher before each commit.
    Installed,
    /// The dispatcher is there and this clone does not run it — a fresh clone
    /// that has not run `uf prepare --install-hooks` yet.
    NotConfigured,
    /// There is no dispatcher.
    Absent,
    /// There is no repository.
    NotARepository,
}

/// Write the dispatcher for the project at `root`, and point this clone at it.
///
/// Refuses before writing anything: a run that stopped at `core.hooksPath`
/// after writing the file would leave a hook no clone runs.
///
/// # Errors
///
/// See [`HookError`].
pub fn install_hooks(root: &Utf8Path) -> Result<HookInstall, HookError> {
    let top = repository_root(root).ok_or(HookError::NotAWorkingTree)?;
    let hook = Utf8PathBuf::from(HOOKS_DIRECTORY).join("pre-commit");
    let path = top.join(&hook);
    let contents = dispatcher(&project_path(root, &top));

    let existing = fs::read_to_string(&path).ok();
    if existing
        .as_deref()
        .is_some_and(|text| !text.contains(DISPATCHER_MARK))
    {
        return Err(HookError::ForeignHook(hook));
    }
    let hooks_path = configured_hooks_path(&top);
    if let Some(current) = hooks_path.as_deref()
        && current != HOOKS_DIRECTORY
    {
        return Err(HookError::HooksPathTaken(current.to_owned()));
    }

    let wrote = existing.as_deref() != Some(contents.as_str());
    if wrote {
        write_executable(&path, &contents)?;
    }
    let configured = hooks_path.is_none();
    if configured {
        git(
            &top,
            "set core.hooksPath",
            &["config", "core.hooksPath", HOOKS_DIRECTORY],
        )?;
    }
    Ok(HookInstall {
        hook,
        wrote,
        configured,
    })
}

/// Whether git will run uf's dispatcher before a commit in `root`.
#[must_use]
pub fn hook_state(root: &Utf8Path) -> HookState {
    let Some(top) = repository_root(root) else {
        return HookState::NotARepository;
    };
    let written = fs::read_to_string(top.join(HOOKS_DIRECTORY).join("pre-commit"))
        .is_ok_and(|text| text.contains(DISPATCHER_MARK));
    match (written, configured_hooks_path(&top).as_deref()) {
        (false, _) => HookState::Absent,
        (true, Some(HOOKS_DIRECTORY)) => HookState::Installed,
        (true, _) => HookState::NotConfigured,
    }
}

/// The dispatcher for a project at `project`, relative to the repository
/// root — empty for the root itself.
///
/// `--cwd` rather than `cd`, because git starts a hook at the repository root
/// and a project may be a directory below it; and a missing `uf` fails the
/// commit rather than skipping the check, saying how to skip it once on
/// purpose. A hook that let a commit through unchecked whenever `uf` was not
/// on `PATH` would be a check that stops running without anyone noticing.
#[must_use]
pub fn dispatcher(project: &str) -> String {
    let run = if project.is_empty() {
        String::from("exec uf prepare")
    } else {
        format!("exec uf --cwd {} prepare", shell_quote(project))
    };
    format!(
        "#!/bin/sh\n\
         {DISPATCHER_MARK}\n\
         #\n\
         # Commit this file. A clone runs `uf prepare --install-hooks` once to point git\n\
         # here, and git then runs it before every commit. What a commit is checked\n\
         # against is `staged` in uf.config.js, not this file, and running that command\n\
         # again rewrites it.\n\
         if ! command -v uf >/dev/null 2>&1; then\n\
         \x20 echo \"pre-commit: uf is not on PATH, so uf prepare cannot check this commit.\" >&2\n\
         \x20 echo \"Install uf, or skip the check once with: git commit --no-verify\" >&2\n\
         \x20 exit 1\n\
         fi\n\
         {run}\n"
    )
}

/// `text` as one word to a POSIX shell.
fn shell_quote(text: &str) -> String {
    let plain = text
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "_./@%+=:,-".contains(character));
    if plain && !text.is_empty() {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

fn repository_root(root: &Utf8Path) -> Option<Utf8PathBuf> {
    let output = git_command(root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    let top = String::from_utf8(output.stdout).ok()?;
    (output.status.success() && !top.trim().is_empty()).then(|| Utf8PathBuf::from(top.trim()))
}

/// `root` relative to `top`, through symbolic links: git answers with the
/// resolved path, and `/tmp` against `/private/tmp` would otherwise share no
/// prefix at all.
fn project_path(root: &Utf8Path, top: &Utf8Path) -> String {
    let resolve = |path: &Utf8Path| {
        path.canonicalize_utf8()
            .unwrap_or_else(|_| path.to_path_buf())
    };
    resolve(root)
        .strip_prefix(resolve(top))
        .map(|relative| relative.as_str().to_owned())
        .unwrap_or_default()
}

fn configured_hooks_path(top: &Utf8Path) -> Option<String> {
    let output = git_command(top)
        .args(["config", "--get", "core.hooksPath"])
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !value.is_empty()).then_some(value)
}

fn git(top: &Utf8Path, action: &'static str, args: &[&str]) -> Result<(), HookError> {
    let output = git_command(top)
        .args(args)
        .output()
        .map_err(|error| HookError::Git {
            action,
            message: error.to_string(),
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(HookError::Git {
        action,
        message: stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("git exited with a failure")
            .to_owned(),
    })
}

fn write_executable(path: &Utf8Path, contents: &str) -> Result<(), HookError> {
    let failed = |source| HookError::Write {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    fs::write(path, contents).map_err(failed)?;
    // Git runs a hook only when it can execute it, and says nothing about one
    // it cannot. The bit is also what `git add` records, so the clones that
    // check the file out get a hook that runs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(failed)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let init = git_command(&root)
            .args(["init", "--quiet"])
            .status()
            .unwrap();
        assert!(init.success());
        (dir, root)
    }

    #[test]
    fn the_dispatcher_runs_uf_prepare_in_the_project() {
        assert!(dispatcher("").ends_with("exec uf prepare\n"));
        assert!(dispatcher("packages/app").ends_with("exec uf --cwd packages/app prepare\n"));
        assert!(dispatcher("my app").ends_with("exec uf --cwd 'my app' prepare\n"));
        assert!(dispatcher("").starts_with("#!/bin/sh\n"));
    }

    #[test]
    fn installing_writes_the_dispatcher_and_points_git_at_it_once() {
        let (_dir, root) = repository();
        assert_eq!(hook_state(&root), HookState::Absent);

        let first = install_hooks(&root).unwrap();
        assert!(first.wrote && first.configured, "{first:?}");
        assert_eq!(hook_state(&root), HookState::Installed);
        let hook = root.join(".githooks/pre-commit");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&hook).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "git runs only an executable hook");
        }

        let second = install_hooks(&root).unwrap();
        assert!(!second.wrote && !second.configured, "{second:?}");
    }

    #[test]
    fn a_hook_uf_did_not_write_is_left_alone() {
        let (_dir, root) = repository();
        fs::create_dir_all(root.join(".githooks")).unwrap();
        fs::write(root.join(".githooks/pre-commit"), "#!/bin/sh\nmake lint\n").unwrap();

        assert!(matches!(
            install_hooks(&root),
            Err(HookError::ForeignHook(_))
        ));
        assert_eq!(
            fs::read_to_string(root.join(".githooks/pre-commit")).unwrap(),
            "#!/bin/sh\nmake lint\n"
        );
    }

    #[test]
    fn a_hooks_path_somebody_else_set_is_not_repointed() {
        let (_dir, root) = repository();
        let set = git_command(&root)
            .args(["config", "core.hooksPath", ".husky"])
            .status()
            .unwrap();
        assert!(set.success());

        assert!(matches!(
            install_hooks(&root),
            Err(HookError::HooksPathTaken(path)) if path == ".husky"
        ));
        assert!(
            !root.join(".githooks/pre-commit").exists(),
            "a refused install wrote a hook no clone runs"
        );
    }

    #[test]
    fn outside_a_repository_there_is_nothing_to_install() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        // A temporary directory can sit inside somebody's repository; only
        // assert when it does not.
        if repository_root(&root).is_none() {
            assert!(matches!(
                install_hooks(&root),
                Err(HookError::NotAWorkingTree)
            ));
        }
    }
}
