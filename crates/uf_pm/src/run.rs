//! Actually installing dependencies.
//!
//! [`crate::install_workspace`] records what a workspace declares; it reaches
//! no registry and creates no `node_modules`. Everything a uf project imports —
//! React, Vite, the `@uniflowed/*` packages — has to come from somewhere, and
//! until uf's own resolver can fetch and link a dependency tree, that somewhere
//! is the package manager the project already uses.
//!
//! So `uf install` detects the manager, maps [`Operation::Install`] through the
//! table in [`crate::command`], and spawns it. The program name comes from that
//! table and never from a manifest, and the child inherits stdio so its own
//! progress and errors reach the terminal unedited rather than being summarised
//! by uf.
//!
//! # Why a detected manager and not uf's own
//!
//! [`PackageManager::Uf`] is what detection reports when a project shows no
//! evidence of any manager. Spawning `uf install` for it would be a loop, and
//! uf's resolver cannot fetch yet, so that case falls back to npm — present
//! wherever Node.js is, which a uf project needs regardless. The report says
//! which manager ran and why, because "uf installed your dependencies" is not
//! true and should not be printed.

use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};

use crate::command::{Invocation, Operation, command_for};
use crate::detect::{Detection, DetectionSource, PackageManager, detect_package_manager};

/// What `uf install` did.
#[derive(Debug, Clone)]
pub struct InstallRun {
    /// The manager that ran.
    pub manager: PackageManager,
    /// The command it ran, for display. Never shell syntax.
    pub invocation: Invocation,
    /// How the manager was chosen.
    pub source: DetectionSource,
    /// Whether uf substituted npm because detection found no real manager.
    pub substituted: bool,
    /// Directory the command ran in.
    pub root: Utf8PathBuf,
}

/// Installing failed.
#[derive(Debug, thiserror::Error)]
pub enum InstallRunError {
    /// The manager could not be started at all.
    #[error("could not run `{invocation}`: {source}\n{hint}")]
    Spawn {
        /// The command uf tried to run.
        invocation: String,
        /// Why the spawn failed.
        #[source]
        source: std::io::Error,
        /// What the user can do about it.
        hint: String,
    },
    /// The manager ran and reported failure. Its own output is already on the
    /// terminal, so this carries the status and nothing else.
    #[error("`{invocation}` exited with {status}")]
    Failed {
        /// The command that ran.
        invocation: String,
        /// How it described its failure.
        status: String,
    },
}

/// Install `root`'s dependencies with the package manager that drives it.
///
/// `allow_scripts` is the project's `pm.allowLifecycleScripts`; when it is
/// false the manager is told not to run any, which is the only way to keep
/// that guarantee once the install is somebody else's process.
///
/// Blocks until the manager exits, with the child's stdio connected to uf's, so
/// the caller must have finished any progress rendering of its own first.
///
/// # Errors
///
/// [`InstallRunError::Spawn`] when the manager is not installed, and
/// [`InstallRunError::Failed`] when it runs and fails.
pub fn run_install(root: &Utf8Path, allow_scripts: bool) -> Result<InstallRun, InstallRunError> {
    let detection = detect_package_manager(root);
    let (manager, substituted) = installable(&detection);
    let mut invocation = command_for(manager, Operation::Install);

    // A dependency's `postinstall` is the supply-chain hole uf's own resolver
    // was going to close by never running one. Delegating to a manager that
    // runs them by default would have quietly reopened it, so the project's
    // `pm.allowLifecycleScripts` is passed through to the manager. Every
    // manager in the table spells the flag the same way.
    if !allow_scripts {
        invocation
            .args
            .push(std::borrow::Cow::Borrowed("--ignore-scripts"));
    }

    let status = Command::new(invocation.program)
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(root)
        .status()
        .map_err(|source| InstallRunError::Spawn {
            invocation: invocation.to_string(),
            source,
            hint: missing_hint(manager),
        })?;

    if !status.success() {
        return Err(InstallRunError::Failed {
            invocation: invocation.to_string(),
            status: status.to_string(),
        });
    }

    Ok(InstallRun {
        manager,
        invocation,
        source: detection.source,
        substituted,
        root: root.to_path_buf(),
    })
}

/// The manager to actually spawn, and whether it was substituted.
///
/// Detection reports [`PackageManager::Uf`] both when a project pins uf and
/// when it shows no evidence at all. Neither can install anything today, so
/// both become npm.
fn installable(detection: &Detection) -> (PackageManager, bool) {
    match detection.package_manager {
        PackageManager::Uf => (PackageManager::Npm, true),
        other => (other, false),
    }
}

fn missing_hint(manager: PackageManager) -> String {
    match manager {
        PackageManager::Npm => "npm comes with Node.js; install Node.js and try again".to_owned(),
        other => format!(
            "this project is pinned to {other}; install it, or change the lockfile and `packageManager` field to a manager you have"
        ),
    }
}
