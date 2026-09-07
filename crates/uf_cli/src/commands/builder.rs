//! Which builder uf drives, resolved from `uf.config.js`.
//!
//! `uf dev`, `uf build`, `uf preview` and `uf start` all do the same thing:
//! start a **driver** on the project's Capability JS Host and read the JSON
//! events it writes. Until ubugeeei-prod/uf#549 the driver was
//! `@uniflowed/vite/driver.js` by name, in four places, which made Vite not a
//! provider uf orchestrates but a dependency uf has. `docs/red-lines.md`
//! line 3 says every built-in provider must be replaceable, and this was the
//! largest one in the toolchain with no seam at all.
//!
//! # The contract
//!
//! A builder is a directory with a `package.json` and a driver module. It is
//! written down in full in `docs/architecture.md`; what this module needs from
//! it is three facts:
//!
//! * **where the driver is** — `uf.builder.driver` in the package manifest,
//!   defaulting to `./driver.js`;
//! * **what to preload on Bun** — `uf.builder.preload.bun`, because Bun has no
//!   `module.register` and a builder that transforms Flow needs its hooks
//!   installed some other way. Optional, and absent for a builder that needs
//!   none;
//! * **which version it is** — `version` from the same manifest, reported by
//!   `uf explain build`, because "which builder ran" and "which build of it"
//!   are two questions and #549 asks both.
//!
//! Everything else — the subcommands, the arguments, the event vocabulary — is
//! in [`super::vite`], which is where the protocol lives and is deliberately
//! not named after any implementation of it.
//!
//! # Why the manifest and not a convention
//!
//! `installed_package` already used `driver.js` as a marker file, so uf could
//! have kept calling that the contract. A declaration is better for one
//! reason: a package that has not opted in is not a builder. `@uniflowed/vite`
//! declares itself one; a package that happens to have a `driver.js` in it
//! does not, and a config that names it gets a sentence rather than a spawn.
//!
//! The fallback exists anyway, and it is deliberate rather than lazy: uf's own
//! packages are version-pinned to each other, but a project may reasonably be
//! holding an older `@uniflowed/vite` than the `uf` binary driving it, and
//! refusing to start over a manifest key added after it was published would
//! break a working project to enforce a declaration uf can infer.

use std::fs;

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::UniflowedConfig;

/// The driver module a builder is expected to expose when it declares none.
///
/// The name `@uniflowed/vite` has always used, and the marker file the old
/// `package_dir` looked for.
const DEFAULT_DRIVER: &str = "driver.js";

/// Bun's counterpart to Node's loader hooks, when a builder declares none.
const DEFAULT_BUN_PRELOAD: &str = "bun-preload.js";

/// A resolved builder: what uf is about to run, and what to call it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Builder {
    /// The specifier from `uf.config.js`, as written.
    ///
    /// Reported rather than the resolved path, because that is the string a
    /// reader can compare against their config file.
    pub(crate) module: String,
    /// The version from the builder's `package.json`, when it has one.
    ///
    /// `None` for a builder resolved from a path inside the project, which is
    /// how somebody tries one out before publishing it.
    pub(crate) version: Option<String>,
    /// The directory the driver was found in.
    pub(crate) directory: Utf8PathBuf,
    /// The driver module, absolute.
    pub(crate) driver: Utf8PathBuf,
    /// What to `--preload` when the host is Bun.
    ///
    /// A path inside the builder when it declared one, and otherwise the
    /// specifier it declared, handed to Bun untouched — which is what
    /// `@uniflowed/vite` needs, because the hooks it preloads live in
    /// `@uniflowed/host` rather than in the builder itself.
    pub(crate) bun_preload: Option<String>,
}

impl Builder {
    /// The name and version, as `uf explain build` prints them.
    pub(crate) fn label(&self) -> String {
        match &self.version {
            Some(version) => format!("{} {version}", self.module),
            None => self.module.clone(),
        }
    }
}

/// The builder `config` names, found from `root`.
///
/// Two shapes of specifier, and they are resolved differently on purpose:
///
/// * a **package name** — `@uniflowed/vite`, `some-builder` — is found by
///   walking up `node_modules` the way module resolution does, so a workspace
///   and a plain project both find the copy their package manager installed;
/// * a **path** — anything starting with `.` or `/` — is resolved from the
///   project root and refused if it leaves it. That is how a builder is tried
///   before it is published, and the refusal is the same rule `uf_plugin`
///   applies for the same reason: `uf.config.js` is untrusted input, and "run
///   this file as the toolchain" is the most dangerous sentence in it.
pub(crate) fn resolve(root: &Utf8Path, config: &UniflowedConfig) -> Result<Builder> {
    let module = config.builder.module.as_str();
    if module.is_empty() {
        bail!("`builder.module` is empty in uf.config.js; name a builder or remove the key");
    }
    let directory = if module.starts_with('.') || module.starts_with('/') {
        project_directory(root, module)?
    } else {
        installed_package(root, module)?
    };
    describe(module, directory)
}

/// Read a resolved directory's manifest into a [`Builder`].
fn describe(module: &str, directory: Utf8PathBuf) -> Result<Builder> {
    let manifest = read_manifest(&directory);
    let declared = manifest
        .as_ref()
        .and_then(|value| value.get("uf"))
        .and_then(|value| value.get("builder"));
    let driver = declared
        .and_then(|value| value.get("driver"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_DRIVER);
    let driver = directory.join(driver.trim_start_matches("./"));
    if !driver.is_file() {
        bail!(
            "`{module}` is not a builder: uf looked for its driver at {driver} and found no \
             file. A builder declares one as `uf.builder.driver` in its package.json; see \
             docs/architecture.md for the contract."
        );
    }

    // What to preload on Bun, and there are three answers rather than two.
    //
    // A declaration that starts with `.` is a file inside the builder and is
    // checked, because a builder that names a file it does not ship is broken
    // and finding out at the first Bun run is finding out in the wrong place.
    // Anything else is a *module specifier* and is handed to Bun untouched:
    // `@uniflowed/vite` preloads `@uniflowed/host/bun-preload`, because the
    // Flow hooks belong to the host package rather than to the bundler, and a
    // resolution that could only name files inside the builder could not
    // express that. (It could not before, either: uf passed
    // `<builder>/bun-preload.js`, which `@uniflowed/vite` has never shipped,
    // so every Bun run of the driver preloaded a file that was not there.)
    //
    // And `None` when nothing is declared and the legacy filename is absent,
    // which is what a builder that needs no hooks at all looks like.
    let bun_preload = match declared
        .and_then(|value| value.get("preload"))
        .and_then(|value| value.get("bun"))
        .and_then(serde_json::Value::as_str)
    {
        Some(declared) if declared.starts_with('.') => {
            let path = directory.join(declared.trim_start_matches("./"));
            if !path.is_file() {
                bail!(
                    "`{module}` declares `uf.builder.preload.bun` at {path}, and there is no \
                     file there"
                );
            }
            Some(path.into_string())
        }
        Some(declared) => Some(declared.to_owned()),
        None => {
            let path = directory.join(DEFAULT_BUN_PRELOAD);
            path.is_file().then(|| path.into_string())
        }
    };

    Ok(Builder {
        module: module.to_owned(),
        version: manifest
            .as_ref()
            .and_then(|value| value.get("version"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        directory,
        driver,
        bun_preload,
    })
}

/// A builder inside the project, named by a relative or absolute path.
///
/// Lexical containment, checked after `join` rather than with `canonicalize`:
/// what is being refused is a *config file* that points the toolchain
/// somewhere else, and a symlink inside a project the user already trusts is
/// not that. The check that matters is that the string cannot climb out.
fn project_directory(root: &Utf8Path, module: &str) -> Result<Utf8PathBuf> {
    let candidate = root.join(module);
    let normalised = normalise(&candidate);
    if !normalised.starts_with(root) {
        bail!(
            "`builder.module` is {module}, which resolves outside {root}. A builder given as a \
             path has to be inside the project; publish it and name it as a package otherwise."
        );
    }
    if !normalised.is_dir() {
        bail!("`builder.module` is {module}, and there is no directory at {normalised}");
    }
    Ok(normalised)
}

/// Resolve `.` and `..` in a path without touching the filesystem.
fn normalise(path: &Utf8Path) -> Utf8PathBuf {
    let mut out = Utf8PathBuf::new();
    for component in path.components() {
        match component.as_str() {
            "." => {}
            ".." => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// The directory of an installed package, found by walking up `node_modules`.
///
/// `marker`-free, unlike the `installed_package` this replaced: whether the
/// directory is a builder is [`describe`]'s question, and answering it here
/// would turn "this package is not a builder" into "this package is not
/// installed", which sends a reader to the wrong fix.
fn installed_package(root: &Utf8Path, name: &str) -> Result<Utf8PathBuf> {
    let mut directory = Some(root);
    while let Some(current) = directory {
        let candidate = current.join("node_modules").join(name);
        if candidate.is_dir() {
            return Ok(candidate);
        }
        directory = current.parent();
    }
    bail!(
        "`{name}` is not installed for {root}; add it to the project's dependencies and run the \
         package manager (`uf install`)"
    )
}

fn read_manifest(directory: &Utf8Path) -> Option<serde_json::Value> {
    let text = fs::read_to_string(directory.join("package.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// The installed `@uniflowed/<name>` a command other than a build needs.
///
/// Kept beside the builder resolution because it is the same walk, and
/// deliberately separate from it: these are uf's own packages, reached by name
/// because uf wrote them, and none of them is a provider a project selects.
pub(crate) fn uniflowed_package(root: &Utf8Path, name: &str, marker: &str) -> Result<Utf8PathBuf> {
    let mut directory = Some(root);
    while let Some(current) = directory {
        let candidate = current.join("node_modules/@uniflowed").join(name);
        if candidate.join(marker).is_file() {
            return Ok(candidate);
        }
        directory = current.parent();
    }
    bail!(
        "`@uniflowed/{name}` is not installed for {root}; add it to the project's dependencies \
         and run the package manager (`uf install`)"
    )
}

#[cfg(test)]
mod tests;
