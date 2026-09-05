//! What one repository declares, and where its tools are linked.
//!
//! # Why the links are in the repository and the tools are not
//!
//! `.uniflowed/env/bin` holds a symlink per executable, pointing into the
//! store. It is small, it is disposable, and it is per-project — which is
//! what makes two checkouts on different Node versions able to sit beside
//! each other without either of them being "active".
//!
//! Nothing is added to `PATH` by installing. `uf env exec` puts this
//! directory in front for one command, and prints it for a reader who wants
//! it in a shell. That is the whole of the activation model: no shim on
//! `PATH`, no shell hook, no global "current version" to be surprised by.
//!
//! # Why the links are rebuilt rather than patched
//!
//! Working out which links are stale is a diff between two sets, and a diff
//! that is subtly wrong leaves a link to a store entry that has been
//! collected — a `node` that is on `PATH` and does not exist. Removing the
//! directory and making it again is one operation with no such state.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use uf_config::UniflowedConfig;

use crate::EnvError;
use crate::store::Store;
use crate::tool::{Pin, Platform, Tool};

/// The directory inside a repository that holds its links.
pub const ENV_DIR: &str = ".uniflowed/env";

/// What a repository's `uf.config.js` asks for.
///
/// # Errors
///
/// When a name is not a tool uf installs, or a version is not exact. Both
/// are refusals rather than warnings: a typo that silently installs nothing
/// is a project that thinks it pinned its runtime and did not.
pub fn declared(config: &UniflowedConfig, platform: Platform) -> Result<Vec<Pin>, EnvError> {
    let mut pins = Vec::new();
    for (name, version) in &config.env.toolchain {
        let tool = Tool::parse(name).ok_or_else(|| EnvError::UnknownTool {
            name: name.to_string(),
        })?;
        let version = version.trim();
        if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(EnvError::NotAnExactVersion {
                tool: tool.name(),
                version: version.to_owned(),
            });
        }
        pins.push(Pin {
            tool,
            version: version.to_owned(),
            platform,
        });
    }
    pins.sort();
    Ok(pins)
}

/// Where a repository's links live.
#[must_use]
pub fn bin_dir(root: &Utf8Path) -> Utf8PathBuf {
    root.join(ENV_DIR).join("bin")
}

/// Rebuild `root`'s links so they point at exactly `pins`.
///
/// Returns the executables that were linked, in the order they were made —
/// which is the order `pins` is in, and so the order a later tool shadows an
/// earlier one. See [`Tool::executables`] for why `npx` is such a case.
///
/// # Errors
///
/// When the directory cannot be rebuilt, or a pin is not installed.
pub fn link(root: &Utf8Path, store: &Store, pins: &[Pin]) -> Result<Vec<String>, EnvError> {
    let bin = bin_dir(root);
    let _ = fs::remove_dir_all(&bin);
    fs::create_dir_all(&bin).map_err(|source| EnvError::Write {
        path: bin.clone(),
        source,
    })?;

    let mut linked = Vec::new();
    for pin in pins {
        let entry = store.path(pin);
        if !entry.is_dir() {
            return Err(EnvError::NotInstalled { pin: pin.clone() });
        }
        for executable in pin.tool.executables() {
            let Some(target) = locate(&entry, executable) else {
                // A tool that does not ship one of its optional executables
                // — Node without `corepack` on an old release — is not an
                // error. A tool that ships none of them is caught below.
                continue;
            };
            let link = bin.join(executable);
            let _ = fs::remove_file(&link);
            symlink(&target, &link)?;
            linked.push(executable.to_string());
        }
        if !linked.iter().any(|name| name == pin.tool.name()) {
            return Err(EnvError::NoExecutable {
                pin: pin.clone(),
                entry,
            });
        }
    }
    Ok(linked)
}

/// Where an executable is inside a store entry.
///
/// An npm package answers this itself, in `package.json`'s `bin` map —
/// pnpm's `pnpm` is `bin/pnpm.cjs`, and no amount of guessing at extensions
/// finds that. So the publisher's own answer is read first, and the guesses
/// are only for release tarballs, which have no manifest: Node keeps its
/// programs in `bin/`, and Bun's and Deno's archives are one binary at the
/// root.
///
/// The file is left where it is and symlinked, shebang and mode intact —
/// `bin/pnpm.cjs` is `#!/usr/bin/env node` with the executable bit set, so
/// a link to it runs, and it finds the `node` that is beside it on the
/// project's path.
fn locate(entry: &Utf8Path, executable: &str) -> Option<Utf8PathBuf> {
    if let Some(declared) = declared_bin(entry, executable)
        && declared.is_file()
    {
        return Some(declared);
    }
    [entry.join("bin").join(executable), entry.join(executable)]
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// What `package.json`'s `bin` says, when there is one.
///
/// `bin` is either a map of name to path, or a bare string for a package
/// whose one executable is named after it.
fn declared_bin(entry: &Utf8Path, executable: &str) -> Option<Utf8PathBuf> {
    let manifest = std::fs::read_to_string(entry.join("package.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let bin = manifest.get("bin")?;
    let relative = match bin {
        serde_json::Value::String(path) => {
            (manifest.get("name")?.as_str()? == executable).then_some(path.as_str())?
        }
        serde_json::Value::Object(map) => map.get(executable)?.as_str()?,
        _ => return None,
    };
    Some(entry.join(relative))
}

#[cfg(unix)]
fn symlink(target: &Utf8Path, link: &Utf8Path) -> Result<(), EnvError> {
    std::os::unix::fs::symlink(target, link).map_err(|source| EnvError::Write {
        path: link.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn symlink(target: &Utf8Path, link: &Utf8Path) -> Result<(), EnvError> {
    fs::copy(target, link)
        .map(|_| ())
        .map_err(|source| EnvError::Write {
            path: link.to_path_buf(),
            source,
        })
}
