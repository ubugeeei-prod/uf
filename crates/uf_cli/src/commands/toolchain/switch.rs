//! Switching which runtime `uf`, `ufr` and `ufx` run.
//!
//! # What a killed process may leave behind
//!
//! A switch writes four things: `runtime.json` beside the runtime, the record
//! of the version it switches away from, the three links, and
//! `active-runtime.json`. Each is written beside its destination under a name
//! of its own and renamed over it. A rename within one directory is atomic, so
//! a reader — or a shell looking `uf` up on `PATH` — sees the old file or the
//! new one, never a truncated file and never a missing one, and a process
//! killed between two renames leaves every name pointing at a complete
//! runtime.
//!
//! What it can leave is a switch half made: `ufr` moved and `uf` not. That is
//! why `uf` moves last. It is the name a reader types and the one
//! [`active_version`] reads, so until it moves the switch has not happened as
//! far as anything can tell, and running the same command again finishes it.
//!
//! The version being replaced is recorded *before* any name moves. Recorded
//! after, a kill between the last link and the record would leave
//! `--rollback` pointing at the version before that one, and switching a
//! reader to a version they did not ask for is worse than refusing. Recorded
//! first, the worst a kill leaves is a record naming the version that is still
//! active, which `--rollback` refuses by name.
//!
//! # Why the three names are not one link to a directory
//!
//! Linking them through `<root>/current/bin/*` and swapping `current` would
//! move all three in a single rename. The installer has to make the same
//! switch in POSIX `sh`, though, and there it cannot: `mv` onto a link to a
//! directory moves *into* that directory unless it is given `-T` (GNU) or `-h`
//! (BSD), and neither spelling works on the other. A machine laid out one way
//! or the other depending on which of the two installed last is the
//! two-stores problem of ubugeeei-prod/uf#534 again.

use std::fs;
use std::io::Write as _;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;

use super::{BINARIES, Origin, RUNTIME_NAME, Store, is_version, recorded_origin};

/// The order the three names are switched in: `uf` last, for the reason the
/// module documentation gives.
pub(super) const SWITCH_ORDER: [&str; 3] = ["ufr", "ufx", "uf"];

/// A point in a switch at which a killed process leaves the machine as it is.
///
/// Only the tests stop at one. They stop at each in turn and check that what
/// is left still runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// `runtime.json` says what the runtime is.
    Manifest,
    /// The version being replaced is recorded.
    Previous,
    /// One of the three names points at the new runtime.
    Link(&'static str),
    /// `active-runtime.json` says which runtime is active.
    State,
}

impl Step {
    /// Every step, in the order a switch takes them.
    #[cfg(test)]
    pub(super) const ALL: [Self; 6] = [
        Self::Manifest,
        Self::Previous,
        Self::Link(SWITCH_ORDER[0]),
        Self::Link(SWITCH_ORDER[1]),
        Self::Link(SWITCH_ORDER[2]),
        Self::State,
    ];
}

/// What a switch wrote, and what it replaced.
#[derive(Debug)]
pub(super) struct Switched {
    pub(super) active_runtime: Utf8PathBuf,
    pub(super) runtime_manifest: Utf8PathBuf,
    pub(super) runtime_binary: Utf8PathBuf,
    pub(super) shim: Utf8PathBuf,
    pub(super) origin: Origin,
    /// The version `uf` ran before, when that was another runtime in this
    /// store — which is the one `--rollback` now returns to.
    pub(super) replaced: Option<String>,
}

/// Point `uf`, `ufr` and `ufx` at `version`, which must already be in the
/// store.
///
/// `checkpoint` runs after each [`Step`]. An error from it stops the switch
/// there with nothing undone, which is what a kill does; everything but the
/// tests passes one that never fails.
pub(super) fn switch_to(
    store: &Store,
    version: &str,
    origin: Origin,
    checkpoint: &mut dyn FnMut(Step) -> Result<()>,
) -> Result<Switched> {
    let runtime_dir = store.version_dir(version);
    if !store.has_complete(version) {
        bail!("uf@{version} is not installed: {runtime_dir} does not hold all of uf, ufr and ufx");
    }
    let replaced = active_version(store).filter(|active| active != version);
    let runtime_binary = store.binary(version);
    let runtime_manifest = runtime_dir.join("runtime.json");

    // A version the installer unpacked has no manifest, and inventing an
    // origin for it would be a guess written down as a fact. An existing one
    // is left alone: where a binary came from is settled when it arrives, and
    // activating it a second time does not change the answer.
    let origin = match (origin, recorded_origin(&runtime_manifest)) {
        (Origin::AlreadyInstalled, Some(recorded)) => recorded,
        (origin, _) => origin,
    };
    write_json(
        &runtime_manifest,
        &json!({
            "name": RUNTIME_NAME,
            "version": version,
            "binary": runtime_binary.as_str(),
            "source": origin.as_str(),
        }),
    )?;
    checkpoint(Step::Manifest)?;

    if let Some(replaced) = &replaced {
        write_atomically(&store.previous_record(), format!("{replaced}\n").as_bytes())?;
    }
    checkpoint(Step::Previous)?;

    fs::create_dir_all(&store.bin_dir)
        .with_context(|| format!("failed to create {}", store.bin_dir))?;
    for name in SWITCH_ORDER {
        link_atomically(
            &runtime_dir.join("bin").join(name),
            &store.bin_dir.join(name),
        )?;
        checkpoint(Step::Link(name))?;
    }

    let active_runtime = store.state_dir.join("active-runtime.json");
    write_json(
        &active_runtime,
        &json!({
            "name": RUNTIME_NAME,
            "version": version,
            "manifest": runtime_manifest.as_str(),
            "binary": runtime_binary.as_str(),
        }),
    )?;
    checkpoint(Step::State)?;

    Ok(Switched {
        active_runtime,
        runtime_manifest,
        runtime_binary,
        shim: store.bin_dir.join("uf"),
        origin,
        replaced,
    })
}

/// The version `uf` in the link directory runs, when it is a runtime in this
/// store.
///
/// Read from the link rather than from `active-runtime.json`: the link is the
/// switch's commit point, and the installer switches versions too without ever
/// writing that file.
pub(super) fn active_version(store: &Store) -> Option<String> {
    let target = fs::read_link(store.bin_dir.join("uf")).ok()?;
    let target = Utf8PathBuf::from_path_buf(target).ok()?;
    // `<runtimes>/uf@<version>/bin/uf`.
    let bin = target.parent()?;
    let runtime = bin.parent()?;
    let version = runtime.file_name()?.strip_prefix("uf@")?;
    let in_this_store =
        bin.file_name() == Some("bin") && runtime.parent() == Some(store.runtimes.as_path());
    (in_this_store && is_version(version)).then(|| version.to_owned())
}

/// Whether all three names already point at `version`.
///
/// `uf` alone is not enough to call a switch finished: a process killed after
/// moving `ufr` and before moving `uf` leaves exactly that, and it is the
/// state the next run is there to repair.
#[cfg(unix)]
pub(super) fn links_agree(store: &Store, version: &str) -> bool {
    let bin = store.version_dir(version).join("bin");
    BINARIES.iter().all(|name| {
        fs::read_link(store.bin_dir.join(name))
            .is_ok_and(|target| target.as_path() == bin.join(name).as_std_path())
    })
}

#[cfg(not(unix))]
pub(super) fn links_agree(_store: &Store, _version: &str) -> bool {
    false
}

/// The version the last switch replaced, as it was recorded.
///
/// A record that does not hold a version is not one: `--rollback` must not
/// read half a line, or somebody else's file, as the name of a runtime.
pub(super) fn recorded_previous(store: &Store) -> Option<String> {
    let contents = fs::read_to_string(store.previous_record()).ok()?;
    let version = contents.trim();
    is_version(version).then(|| version.to_owned())
}

/// Put a complete, staged runtime where its version lives.
///
/// One rename when nothing is there. When a runtime of that version already is
/// — possibly the one running this process — its files are replaced one rename
/// at a time instead, `bin/uf` last, so the directory the links point into is
/// never absent. Renaming it aside and the staged one in, which the installer
/// used to do, left every link dangling between the two renames, and for good
/// when the process died between them.
pub(super) fn place(staged: &Utf8Path, runtime_dir: &Utf8Path) -> Result<()> {
    if !runtime_dir.is_dir() {
        return fs::rename(staged, runtime_dir)
            .with_context(|| format!("failed to move {staged} to {runtime_dir}"));
    }

    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(staged).min_depth(1) {
        let entry = entry.with_context(|| format!("failed to read {staged}"))?;
        if entry.file_type().is_dir() {
            continue;
        }
        let path = Utf8Path::from_path(entry.path())
            .ok_or_else(|| anyhow!("{} is not a UTF-8 path", entry.path().display()))?;
        files.push(path.strip_prefix(staged)?.to_owned());
    }
    // `false` sorts first, and the sort is stable.
    files.sort_by_key(|file| file.as_str() == "bin/uf");

    for file in &files {
        let destination = runtime_dir.join(file);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).with_context(|| format!("failed to create {parent}"))?;
        }
        fs::rename(staged.join(file), &destination)
            .with_context(|| format!("failed to replace {destination}"))?;
    }
    fs::remove_dir_all(staged).with_context(|| format!("failed to remove {staged}"))
}

/// Install the running binary as its own version.
///
/// Only ever called when the requested version *is* the running one, which is
/// what makes it a copy rather than a rename. `ufr` and `ufx` are the same
/// binary with a different `argv[0]`, so all three are written — into a staged
/// directory first, because a copy killed half way through is exactly the
/// half-written binary a switch must never point at.
pub(super) fn install_running_binary(store: &Store, version: &str) -> Result<()> {
    let staged = store
        .runtimes
        .join(format!(".uf@{version}.incoming.{}", std::process::id()));
    match fs::remove_dir_all(&staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("failed to remove {staged}")),
    }
    let bin = staged.join("bin");
    fs::create_dir_all(&bin).with_context(|| format!("failed to create {bin}"))?;

    let current_exe = std::env::current_exe().with_context(|| "failed to locate current uf")?;
    for name in BINARIES {
        let destination = bin.join(binary_file(name));
        fs::copy(&current_exe, &destination).with_context(|| {
            format!(
                "failed to install {destination} from {}",
                current_exe.display()
            )
        })?;
        mark_executable(&destination)?;
    }
    place(&staged, &store.version_dir(version))
}

/// A binary's file name on this platform.
pub(super) fn binary_file(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Whether `path` is a file this user could run.
#[cfg(unix)]
pub(super) fn is_executable_file(path: &Utf8Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
pub(super) fn is_executable_file(path: &Utf8Path) -> bool {
    path.is_file()
}

/// Write `contents` to `path` so that a reader sees the old file or the new
/// one, never part of either.
pub(super) fn write_atomically(path: &Utf8Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {parent}"))?;
    }
    let incoming = incoming(path);
    let written = fs::File::create(&incoming)
        .and_then(|mut file| {
            file.write_all(contents)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&incoming, path));
    if let Err(error) = written {
        let _ = fs::remove_file(&incoming);
        return Err(error).with_context(|| format!("failed to write {path}"));
    }
    Ok(())
}

fn write_json(path: &Utf8Path, value: &serde_json::Value) -> Result<()> {
    let mut contents = serde_json::to_string_pretty(value)?;
    contents.push('\n');
    write_atomically(path, contents.as_bytes())
}

/// Link `path` to `target` in one rename, replacing whatever was there.
///
/// The link is made beside `path` and renamed over it, so there is no moment
/// at which `path` is missing — which there was when this removed the old link
/// and then created the new one, and which there is in `ln -sfn` wherever `ln`
/// unlinks first, as BSD's does.
#[cfg(unix)]
pub(super) fn link_atomically(target: &Utf8Path, path: &Utf8Path) -> Result<()> {
    if !is_executable_file(target) {
        bail!("the runtime is missing {target}");
    }
    let incoming = incoming(path);
    // A leftover under this name was made by a process with this id that is
    // gone, because no two running processes share one.
    match fs::remove_file(&incoming) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("failed to remove {incoming}")),
    }
    std::os::unix::fs::symlink(target, &incoming)
        .with_context(|| format!("failed to link {incoming} to {target}"))?;
    if let Err(error) = fs::rename(&incoming, path) {
        let _ = fs::remove_file(&incoming);
        return Err(error).with_context(|| format!("failed to point {path} at {target}"));
    }
    Ok(())
}

/// A batch file that runs the versioned binary, for a platform with no
/// symlink an unprivileged user may create.
#[cfg(not(unix))]
pub(super) fn link_atomically(target: &Utf8Path, path: &Utf8Path) -> Result<()> {
    let target = target.with_extension("exe");
    if !target.exists() {
        bail!("the runtime is missing {target}");
    }
    write_atomically(
        &path.with_extension("cmd"),
        format!("@echo off\r\n\"{target}\" %*\r\n").as_bytes(),
    )
}

/// Where a file is made before it is renamed over `path`.
///
/// Beside it, because a rename is only atomic within one filesystem, and named
/// the way the installer names its own — `.<name>.incoming.<pid>` — so that
/// each recognises what the other left behind.
fn incoming(path: &Utf8Path) -> Utf8PathBuf {
    let name = path.file_name().unwrap_or("uf");
    path.with_file_name(format!(".{name}.incoming.{}", std::process::id()))
}

#[cfg(unix)]
fn mark_executable(path: &Utf8Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to read permissions for {path}"))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to update permissions for {path}"))
}

#[cfg(not(unix))]
fn mark_executable(_path: &Utf8Path) -> Result<()> {
    Ok(())
}
