//! Actually changing what is installed.
//!
//! [`crate::install_workspace`] records what a workspace declares; it reaches
//! no registry and creates no `node_modules`. Everything a uf project imports —
//! React, Vite, the `@uniflowed/*` packages — has to come from somewhere, and
//! until uf's own resolver can fetch and link a dependency tree, that somewhere
//! is the package manager the project already uses.
//!
//! So `uf install`, `uf add`, `uf remove`, `uf update` and `uf why` each detect
//! the manager, map their [`Operation`] through the table in [`crate::command`],
//! and spawn it. The program name comes from that table and never from a
//! manifest.
//!
//! # Two ways to run one
//!
//! [`run_operation`] lets the child inherit uf's stdio, so its own progress and
//! errors reach the terminal unedited. That is right for pnpm, Yarn and Bun,
//! which all draw a good install, and it is the only way the four commands that
//! carry operands run at all.
//!
//! [`run_install_watched`] reads the child's output instead, so uf can draw
//! the phases as they happen — see [`crate::progress`] for what that costs and
//! what it refuses to swallow. It is used only for a manager
//! [`Reader::for_manager`] recognises, and even then every line the manager
//! would have printed on its own is handed straight back to the caller through
//! [`InstallObserver::line`]. A failed install still says why, in npm's words,
//! because those words arrive on that callback like any other.
//!
//! # Operands
//!
//! A package specifier is the one part of these invocations that comes from
//! outside the table. It is passed as a single `argv` entry and never through a
//! shell, so `uf add "react@^18 || ^19"` is one argument and stays one — but an
//! operand that *starts* with `-` would be read by the manager as a flag it was
//! never asked for, so [`operands_for`] refuses it before the spawn rather than
//! letting `uf remove --global` mean something.
//!
//! # Why a detected manager and not uf's own
//!
//! [`PackageManager::Uf`] is what detection reports when a project shows no
//! evidence of any manager. Spawning `uf install` for it would be a loop, and
//! uf's resolver cannot fetch yet, so that case falls back to npm — present
//! wherever Node.js is, which a uf project needs regardless. The report says
//! which manager ran and why, because "uf installed your dependencies" is not
//! true and should not be printed.

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};

use crate::command::{Invocation, Operation, command_for};
use crate::detect::{Detection, DetectionSource, PackageManager, detect_package_manager};
use crate::progress::{InstallWatch, ManagerEvent, Reader};

/// How often a watched install reports progress when the manager is silent.
///
/// Not a frame rate — the caller decides that — but the granularity at which
/// "the registry has gone quiet" can be noticed at all.
const POLL: Duration = Duration::from_millis(50);

/// Longest line uf keeps whole from a manager's output.
///
/// A line arrives with registry-controlled content in it, and an answer that
/// is one enormous line must not become one enormous allocation. Past this the
/// remainder is read as the next line, which is bounded and readable rather
/// than unbounded and tidy.
const MAX_LINE: u64 = 8 * 1024;

/// Which of the manager's streams a line arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerStream {
    /// The manager's stdout: its summary, and what it thinks the user asked
    /// for.
    Stdout,
    /// The manager's stderr: its warnings, its errors, and its progress.
    Stderr,
}

/// What a caller does with a manager's output while it is still running.
pub trait InstallObserver {
    /// The manager printed a line of its own; print it.
    ///
    /// Called for every line uf did not ask for, in the order it arrived on
    /// that stream. Nothing else in uf will print it.
    fn line(&mut self, stream: ManagerStream, text: &str);

    /// The ladder moved, or time passed.
    ///
    /// Called after every line and at least every [`POLL`], so an
    /// implementation that draws must decide for itself when a frame is due.
    fn progress(&mut self, watch: &InstallWatch);
}

/// What a delegated package-manager command did.
#[derive(Debug, Clone)]
pub struct ManagerRun {
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
    /// The phase ladder, when uf read the manager's output as it ran.
    ///
    /// `None` from [`run_install`], which never reads it.
    pub watch: Option<InstallWatch>,
}

/// Running the manager failed, or uf refused to run it.
#[derive(Debug, thiserror::Error)]
pub enum ManagerRunError {
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
    /// An operand uf will not put on a manager's command line.
    #[error("{operand:?} is not a package name: {reason}")]
    Operand {
        /// The operand as it was given.
        operand: String,
        /// Why it was refused, and what to write instead.
        reason: String,
    },
    /// The project's manager has no command for what was asked.
    ///
    /// Reported rather than worked around. uf could run a different manager's
    /// equivalent, and that is exactly the substitution a project chose its
    /// manager to avoid — the replacement would resolve against its own
    /// registry configuration and could write its own lockfile.
    #[error("{manager} has no `{operation}`: {hint}")]
    Unsupported {
        /// The manager the project uses.
        manager: String,
        /// The uf operation it cannot perform.
        operation: &'static str,
        /// What to do instead.
        hint: String,
    },
}

/// Install `root`'s dependencies with the package manager that drives it.
///
/// [`run_operation`] with [`Operation::Install`] and no operands.
///
/// # Errors
///
/// The same as [`run_operation`].
pub fn run_install(root: &Utf8Path, allow_scripts: bool) -> Result<ManagerRun, ManagerRunError> {
    run_operation(root, Operation::Install, &[], allow_scripts)
}

/// Run one package-manager operation in `root`, with the manager that drives it.
///
/// `operands` are the package specifiers or names the operation takes — the
/// specifiers for [`Operation::Add`], the names for [`Operation::Remove`] and
/// [`Operation::Why`], the packages to hold to for [`Operation::Update`], and
/// nothing at all for the two installs.
///
/// `allow_scripts` is the project's `pm.allowLifecycleScripts`; when it is
/// false the manager is told not to run any, which is the only way to keep
/// that guarantee once the work is somebody else's process. It is passed only
/// for the operations that can install something — see
/// [`Operation::installs_packages`].
///
/// Blocks until the manager exits, with the child's stdio connected to uf's, so
/// the caller must have finished any progress rendering of its own first.
///
/// # Errors
///
/// [`ManagerRunError::Operand`] before anything is spawned when an operand
/// could be read as a flag, [`ManagerRunError::Spawn`] when the manager is not
/// installed, and [`ManagerRunError::Failed`] when it runs and fails.
pub fn run_operation(
    root: &Utf8Path,
    operation: Operation<'_>,
    operands: &[String],
    allow_scripts: bool,
) -> Result<ManagerRun, ManagerRunError> {
    let detection = detect_package_manager(root);
    let (manager, substituted) = installable(&detection);
    let invocation = invocation_for(manager, operation, operands, allow_scripts)?;

    let status = Command::new(invocation.program)
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(root)
        .status()
        .map_err(|source| ManagerRunError::Spawn {
            invocation: invocation.to_string(),
            source,
            hint: missing_hint(manager),
        })?;

    if !status.success() {
        return Err(ManagerRunError::Failed {
            invocation: invocation.to_string(),
            status: status.to_string(),
        });
    }

    Ok(ManagerRun {
        manager,
        invocation,
        source: detection.source,
        substituted,
        root: root.to_path_buf(),
        watch: None,
    })
}

/// What to do about an operation the project's manager does not have.
///
/// One sentence per operation, naming the managers that do have it — the
/// reader's next question is always "then what", and "your package manager
/// cannot" is only half an answer.
fn unsupported_hint(operation: Operation<'_>) -> String {
    match operation {
        Operation::Search => {
            "npm and pnpm can search the registry; `uf exec --yes npm search` runs npm's \
             without changing what this project installs with"
        }
        Operation::Patch | Operation::PatchCommit => {
            "pnpm and yarn 2+ can patch a dependency; on the others the ecosystem's answer is \
             `patch-package`, which uf does not install for you"
        }
        _ => "no package manager uf knows spells this one differently",
    }
    .to_owned()
}

/// The exact command `run_operation` would spawn.
///
/// Separate from the spawn so that what uf is about to run can be asserted on,
/// and printed — `uf explain add` names it — without running anything.
///
/// # Errors
///
/// [`ManagerRunError::Operand`] for an operand uf will not pass on.
pub fn invocation_for(
    manager: PackageManager,
    operation: Operation<'_>,
    operands: &[String],
    allow_scripts: bool,
) -> Result<Invocation, ManagerRunError> {
    let mut invocation =
        command_for(manager, operation).ok_or_else(|| ManagerRunError::Unsupported {
            manager: manager.to_string(),
            operation: operation.name(),
            hint: unsupported_hint(operation),
        })?;

    // A dependency's `postinstall` is the supply-chain hole uf's own resolver
    // was going to close by never running one. Delegating to a manager that
    // runs them by default would have quietly reopened it, so the project's
    // `pm.allowLifecycleScripts` is passed through to the manager. Every
    // manager in the table spells the flag the same way.
    //
    // Before the operands rather than after: a flag that follows a package
    // name is still a flag to all four managers, but a reader checking the
    // `command` row against what they typed should see uf's own additions
    // together and their own specifiers last.
    if !allow_scripts && operation.installs_packages() {
        invocation
            .args
            .push(std::borrow::Cow::Borrowed("--ignore-scripts"));
    }
    check_operands(operands)?;
    for operand in operands {
        invocation
            .args
            .push(std::borrow::Cow::Owned(operand.clone()));
    }
    Ok(invocation)
}

/// Refuse an operand uf will not put on a manager's command line.
///
/// Nothing here is quoted or split: a specifier is one `argv` entry and stays
/// one, which is what makes `uf add "react@>=18 <20"` mean what it says. What
/// is refused is the operand that would stop being an operand — an empty
/// string, which every manager reads as a package with no name, and anything
/// starting with `-`, which it would read as a flag.
///
/// Public, and separate from [`invocation_for`], so a caller can refuse before
/// it has done anything at all: `uf add -- --global` used to rewrite `uf.lock`
/// on its way to failing, which is a command that both refused and wrote.
///
/// # Errors
///
/// [`ManagerRunError::Operand`], naming the operand and what to write instead.
pub fn check_operands(operands: &[String]) -> Result<(), ManagerRunError> {
    for operand in operands {
        if operand.is_empty() {
            return Err(ManagerRunError::Operand {
                operand: operand.clone(),
                reason: "it is empty; name the package you meant".to_owned(),
            });
        }
        if operand.starts_with('-') {
            return Err(ManagerRunError::Operand {
                operand: operand.clone(),
                reason: format!(
                    "it starts with `-`, which the package manager would read as a flag; \
                     write the package name, or `./{operand}` for a path"
                ),
            });
        }
    }
    Ok(())
}

/// Install `root`'s dependencies, reading the manager's output as it goes.
///
/// [`run_watched`] with [`Operation::Install`].
///
/// # Errors
///
/// The same as [`run_watched`].
pub fn run_install_watched(
    root: &Utf8Path,
    allow_scripts: bool,
    observer: &mut dyn InstallObserver,
) -> Result<ManagerRun, ManagerRunError> {
    run_watched(root, Operation::Install, allow_scripts, observer)
}

/// Run an install-shaped `operation` in `root`, reading the manager's output as
/// it goes.
///
/// Same contract as [`run_operation`] — same manager, same detection, same
/// `--ignore-scripts` — with the child's streams piped instead of inherited so
/// that `observer` sees them. It takes no operands, because the two operations
/// worth narrating a ladder for are the two installs and neither has any.
///
/// When the detected manager is not one [`Reader::for_manager`] knows how to
/// read, this falls back to [`run_operation`] rather than piping a manager uf
/// cannot narrate: taking a good install screen away and replacing it with a
/// spinner is not an improvement.
///
/// # Errors
///
/// The same as [`run_operation`], and for the same reasons.
pub fn run_watched(
    root: &Utf8Path,
    operation: Operation<'_>,
    allow_scripts: bool,
    observer: &mut dyn InstallObserver,
) -> Result<ManagerRun, ManagerRunError> {
    let detection = detect_package_manager(root);
    let (manager, substituted) = installable(&detection);
    let Some(reader) = Reader::for_manager(manager) else {
        return run_operation(root, operation, &[], allow_scripts);
    };
    let mut invocation = invocation_for(manager, operation, &[], allow_scripts)?;
    // Asked for so that there is something to narrate: npm prints nothing at
    // all between "starting" and "done" when its output is a pipe. Every line
    // this flag causes is consumed by `Reader::classify` and no other, so the
    // flag changes what uf can see and not what the reader is shown.
    invocation
        .args
        .push(std::borrow::Cow::Borrowed(reader.verbosity_argument()));

    let mut child = Command::new(invocation.program)
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ManagerRunError::Spawn {
            invocation: invocation.to_string(),
            source,
            hint: missing_hint(manager),
        })?;

    // Two pipes and one blocking read each: a single-threaded reader that
    // drained stdout first would deadlock the moment npm filled its stderr
    // pipe, which for a `--loglevel=http` install is immediately.
    let (sender, receiver) = mpsc::channel();
    let pumps = [
        child
            .stdout
            .take()
            .map(|stream| pump(stream, ManagerStream::Stdout, sender.clone())),
        child
            .stderr
            .take()
            .map(|stream| pump(stream, ManagerStream::Stderr, sender.clone())),
    ];
    // The loop below ends when every sender is gone, so uf's own must be.
    drop(sender);

    let mut watch = InstallWatch::start(Instant::now());
    loop {
        match receiver.recv_timeout(POLL) {
            Ok((stream, line)) => match reader.classify(&line) {
                ManagerEvent::Passthrough => observer.line(stream, &line),
                event => watch.observe(&event, Instant::now()),
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        watch.idle(Instant::now());
        observer.progress(&watch);
    }
    for pump in pumps.into_iter().flatten() {
        let _ = pump.join();
    }
    watch.finish(Instant::now());

    let status = child.wait().map_err(|source| ManagerRunError::Spawn {
        invocation: invocation.to_string(),
        source,
        hint: missing_hint(manager),
    })?;
    if !status.success() {
        return Err(ManagerRunError::Failed {
            invocation: invocation.to_string(),
            status: status.to_string(),
        });
    }

    Ok(ManagerRun {
        manager,
        invocation,
        source: detection.source,
        substituted,
        root: root.to_path_buf(),
        watch: Some(watch),
    })
}

/// Read `stream` line by line onto `sink` until it closes.
fn pump<R: Read + Send + 'static>(
    stream: R,
    which: ManagerStream,
    sink: Sender<(ManagerStream, String)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut raw = Vec::new();
        loop {
            raw.clear();
            match reader.by_ref().take(MAX_LINE).read_until(b'\n', &mut raw) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            while matches!(raw.last(), Some(b'\n' | b'\r')) {
                raw.pop();
            }
            // Lossy on purpose: a manager that writes bytes which are not
            // UTF-8 has still said something, and refusing to show it is worse
            // than showing it with a replacement character in it.
            if sink
                .send((which, String::from_utf8_lossy(&raw).into_owned()))
                .is_err()
            {
                return;
            }
        }
    })
}

/// The manager to actually spawn, and whether it was substituted.
///
/// Detection reports [`PackageManager::Uf`] both when a project pins uf and
/// when it shows no evidence at all. Neither can install anything today, so
/// both become npm.
///
/// Public because a caller has to know which lockfile the install is about to
/// rewrite *before* it runs: reading it afterwards and calling that the
/// "before" state would report an empty delta for every install.
#[must_use]
pub fn installable(detection: &Detection) -> (PackageManager, bool) {
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

#[cfg(test)]
mod tests;
