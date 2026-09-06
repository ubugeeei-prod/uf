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
//! table and never from a manifest.
//!
//! # Two ways to run it
//!
//! [`run_install`] lets the child inherit uf's stdio, so its own progress and
//! errors reach the terminal unedited. That is right for pnpm, Yarn and Bun,
//! which all draw a good install.
//!
//! [`run_install_watched`] reads the child's output instead, so uf can draw
//! the phases as they happen — see [`crate::progress`] for what that costs and
//! what it refuses to swallow. It is used only for a manager
//! [`Reader::for_manager`] recognises, and even then every line the manager
//! would have printed on its own is handed straight back to the caller through
//! [`InstallObserver::line`]. A failed install still says why, in npm's words,
//! because those words arrive on that callback like any other.
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
    /// The phase ladder, when uf read the manager's output as it ran.
    ///
    /// `None` from [`run_install`], which never reads it.
    pub watch: Option<InstallWatch>,
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
        watch: None,
    })
}

/// Install `root`'s dependencies, reading the manager's output as it goes.
///
/// Same contract as [`run_install`] — same manager, same detection, same
/// `--ignore-scripts` — with the child's streams piped instead of inherited so
/// that `observer` sees them. When the detected manager is not one
/// [`Reader::for_manager`] knows how to read, this falls back to
/// [`run_install`] rather than piping a manager uf cannot narrate: taking a
/// good install screen away and replacing it with a spinner is not an
/// improvement.
///
/// # Errors
///
/// The same two as [`run_install`], and for the same reasons.
pub fn run_install_watched(
    root: &Utf8Path,
    allow_scripts: bool,
    observer: &mut dyn InstallObserver,
) -> Result<InstallRun, InstallRunError> {
    let detection = detect_package_manager(root);
    let (manager, substituted) = installable(&detection);
    let Some(reader) = Reader::for_manager(manager) else {
        return run_install(root, allow_scripts);
    };
    let mut invocation = command_for(manager, Operation::Install);
    if !allow_scripts {
        invocation
            .args
            .push(std::borrow::Cow::Borrowed("--ignore-scripts"));
    }
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
        .map_err(|source| InstallRunError::Spawn {
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

    let status = child.wait().map_err(|source| InstallRunError::Spawn {
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
