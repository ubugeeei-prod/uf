//! What one repository declares, and where its tools are linked.
//!
//! # Why the links are not in the repository
//!
//! A directory of symlinks per project is *per project* and it is not the
//! project's: nobody wrote it, nobody reads it, it cannot be committed, and it
//! is one more thing every `.gitignore` has to know about. `.uniflowed/env/`
//! was that directory, it was never added to this repository's own
//! `.gitignore`, and a generated file from it was committed — which is
//! ubugeeei-prod/uf#427, where a leftover from an older `uf env use` stops
//! `uf env install` working.
//!
//! So the links live beside the store instead, under `<data>/uf/envs/`, one
//! directory per project. The property that made them per-project is kept
//! exactly — two checkouts on different Node versions still sit beside each
//! other with neither of them "active" — because the directory is *keyed* by
//! the project rather than *inside* it. See [`Envs::dir_for`].
//!
//! Nothing is added to `PATH` by installing. `uf env exec` puts the project's
//! directory in front for one command, and prints it for a reader who wants it
//! in a shell. That is the whole of the activation model: no shim on `PATH`,
//! no shell hook, no global "current version" to be surprised by.
//!
//! # Why the links are rebuilt rather than patched
//!
//! Working out which links are stale is a diff between two sets, and a diff
//! that is subtly wrong leaves a link to a store entry that has been
//! collected — a `node` that is on `PATH` and does not exist. Removing the
//! directory and making it again is one operation with no such state.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use sha2::{Digest, Sha256};
use uf_config::UniflowedConfig;

use crate::EnvError;
use crate::store::{Store, data_home, env_path};
use crate::tool::{Pin, Platform, Tool};

/// The directory a project's links used to live in.
///
/// Kept so that `uf env install` can remove one it finds: a reader who
/// upgrades has a `.uniflowed/` full of links into the store that nothing
/// will ever rebuild, and #427 is what happens when it is left there.
pub const LEGACY_ENV_DIR: &str = ".uniflowed";

/// Where every project's links live, which is not in any project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envs {
    root: Utf8PathBuf,
}

impl Envs {
    /// The directory under `root`.
    #[must_use]
    pub fn new(root: impl Into<Utf8PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Where this machine keeps them.
    ///
    /// `$UF_ENVS` first, so a test and a curious reader can put it somewhere
    /// else; then `<data>/uf/envs`, beside the store — the same resolution
    /// [`Store::discover`] makes, for the same reason.
    ///
    /// # Errors
    ///
    /// When neither `$XDG_DATA_HOME` nor `$HOME` is set.
    pub fn discover() -> Result<Self, EnvError> {
        if let Some(explicit) = env_path("UF_ENVS") {
            return Ok(Self::new(explicit));
        }
        Ok(Self::new(data_home()?.join("uf").join("envs")))
    }

    /// Where they all live.
    #[must_use]
    pub fn root(&self) -> &Utf8Path {
        &self.root
    }

    /// The directory belonging to the project at `project_root`.
    ///
    /// `<name>-<hash>`, where the name is the project's own directory and the
    /// hash is of its absolute path. Both halves earn their place: the hash is
    /// what makes two checkouts of the same repository two environments, and
    /// the name is what lets a person looking at `~/.local/share/uf/envs` see
    /// which is which. A hash alone would be correct and unreadable.
    ///
    /// Sixteen hex characters of SHA-256. This is a directory name, not a
    /// security boundary: the thing it has to do is not collide between the
    /// handful of checkouts on one machine.
    #[must_use]
    pub fn dir_for(&self, project_root: &Utf8Path) -> Utf8PathBuf {
        // Canonicalised, so `/repo` and `/repo/.` are one project. A path that
        // cannot be canonicalised — the directory was removed under us — is
        // hashed as written, which is still stable and still that project's.
        let absolute = project_root
            .canonicalize_utf8()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let digest = Sha256::digest(absolute.as_str().as_bytes());
        let hash: String = digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let name = absolute.file_name().unwrap_or("project");
        self.root.join(format!("{name}-{hash}"))
    }

    /// Where the project's links are.
    #[must_use]
    pub fn bin_dir(&self, project_root: &Utf8Path) -> Utf8PathBuf {
        self.dir_for(project_root).join("bin")
    }
}

/// What a repository's `uf.config.js` asks for.
///
/// # Errors
///
/// When a name is not a tool uf installs, or a version is not exact. Both
/// are refusals rather than warnings: a typo that silently installs nothing
/// is a project that thinks it pinned its runtime and did not.
pub fn declared(config: &UniflowedConfig, platform: Platform) -> Result<Vec<Pin>, EnvError> {
    declared_from(config.env.toolchain.iter(), platform)
}

/// What a repository asks for through uf's config and standard manifest fields.
///
/// `env.toolchain` is the explicit uf answer and wins when both places name the
/// same tool. A package manifest's `engines` object is the standard spelling,
/// so exact versions there are used as the fallback instead of forcing a
/// second uf-only declaration.
///
/// # Errors
///
/// When uf's config names a tool uf installs with a version that is not exact,
/// or when the manifest cannot be read as JSON. Non-exact manifest engines are
/// ignored because `engines` commonly carries compatibility ranges.
pub fn declared_for_project(
    root: &Utf8Path,
    config: &UniflowedConfig,
    platform: Platform,
) -> Result<Vec<Pin>, EnvError> {
    let manifest = root.join("package.json");
    let engines = engines(&manifest)?;
    let merged = engines
        .iter()
        .filter(|(name, _)| !config.env.toolchain.contains_key(name))
        .map(|(name, version)| (name, version))
        .chain(config.env.toolchain.iter());
    declared_from(merged, platform)
}

fn declared_from<'a>(
    entries: impl Iterator<Item = (&'a CompactString, &'a CompactString)>,
    platform: Platform,
) -> Result<Vec<Pin>, EnvError> {
    let mut pins = Vec::new();
    for (name, version) in entries {
        let tool = Tool::parse(name).ok_or_else(|| EnvError::UnknownTool {
            name: name.to_string(),
        })?;
        let version = version.trim();
        if !is_exact_version(version) {
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

fn engines(path: &Utf8Path) -> Result<Vec<(CompactString, CompactString)>, EnvError> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(EnvError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let value = serde_json::from_str::<serde_json::Value>(&source).map_err(|source| {
        EnvError::ManifestJson {
            path: path.to_path_buf(),
            source,
        }
    })?;

    let Some(engines) = value.get("engines").and_then(serde_json::Value::as_object) else {
        return Ok(Vec::new());
    };

    Ok(engines
        .iter()
        .filter_map(|(name, version)| {
            let version = version.as_str()?.trim();
            if !is_exact_version(version) {
                return None;
            }
            Tool::parse(name).map(|tool| (tool.name().into(), version.into()))
        })
        .collect())
}

fn is_exact_version(version: &str) -> bool {
    let Some(separator) = version.find(['-', '+']) else {
        return is_version_core(version);
    };
    is_version_core(&version[..separator]) && is_version_suffix(&version[separator..])
}

fn is_version_core(version: &str) -> bool {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return false;
    };
    let Some(minor) = parts.next() else {
        return false;
    };
    let Some(patch) = parts.next() else {
        return false;
    };
    parts.next().is_none() && [major, minor, patch].into_iter().all(is_digits)
}

fn is_version_suffix(suffix: &str) -> bool {
    if let Some(rest) = suffix.strip_prefix('-') {
        let Some((pre, build)) = rest.split_once('+') else {
            return is_version_identifiers(rest);
        };
        return is_version_identifiers(pre) && is_version_identifiers(build);
    }

    let Some(build) = suffix.strip_prefix('+') else {
        return false;
    };
    is_version_identifiers(build)
}

fn is_version_identifiers(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn is_digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

/// Move what an older uf left in `.uniflowed/`, and remove the rest.
///
/// Two things were in there and they are not the same kind of thing:
///
/// * **the profile**, which is a decision somebody made — `uf env use review`
///   — and is moved to `.uf/profile` rather than lost. Only when there is
///   nothing at the new path already: a project that has run `uf env use`
///   since upgrading has said something newer.
/// * **`env/`**, which is a directory of symlinks into the store that nothing
///   rebuilds now. Removed, because it is uf's own output in a location uf no
///   longer uses and there is nothing a reader could decide about it. As a
///   *file* rather than a directory it is ubugeeei-prod/uf#427, where a
///   leftover from an older `uf env use` stops `uf env install` outright.
///
/// Returns whether anything was there.
///
/// # Errors
///
/// When the profile cannot be copied to its new home. Nothing is removed in
/// that case — the point of copying rather than renaming is that a failure
/// leaves the original where the reader can still see it, and deleting
/// `.uniflowed/` after a copy that did not happen would lose the profile and
/// report that it had moved.
pub fn migrate_legacy_dir(root: &Utf8Path) -> Result<bool, EnvError> {
    let legacy = root.join(LEGACY_ENV_DIR);
    if !legacy.exists() {
        return Ok(false);
    }

    let legacy_profile = root.join(uf_config::env_files::LEGACY_PROFILE_FILE);
    let profile = root.join(uf_config::env_files::PROFILE_FILE);
    // Only when there is nothing at the new path already: a project that has
    // run `uf env use` since upgrading has said something newer.
    if legacy_profile.is_file() && !profile.exists() {
        if let Some(parent) = profile.parent() {
            fs::create_dir_all(parent).map_err(|source| EnvError::Write {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::copy(&legacy_profile, &profile).map_err(|source| EnvError::Write {
            path: profile.clone(),
            source,
        })?;
    }

    // Either shape: the directory it became, and the file it used to be.
    Ok(fs::remove_dir_all(&legacy).is_ok() || fs::remove_file(&legacy).is_ok())
}

/// Rebuild the links for the project at `root` so they point at exactly
/// `pins`.
///
/// Returns the executables that were linked, in the order they were made —
/// which is the order `pins` is in, and so the order a later tool shadows an
/// earlier one. See [`Tool::executables`] for why `npx` is such a case.
///
/// # Errors
///
/// When the directory cannot be rebuilt, or a pin is not installed.
pub fn link(
    root: &Utf8Path,
    envs: &Envs,
    store: &Store,
    pins: &[Pin],
) -> Result<Vec<String>, EnvError> {
    let bin = envs.bin_dir(root);
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
