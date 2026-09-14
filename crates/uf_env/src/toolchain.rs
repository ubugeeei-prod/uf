//! The tools a project declares, where each one is used, and which release each
//! one is.
//!
//! # From a key to a pin
//!
//! `uf_config::tools` reads what a project wrote — `runtime: "node@26"` — and
//! which key it came from. This module turns that into what the store holds: a
//! [`Pin`], one exact release for this platform. A spec is one of three things,
//! and each has one answer:
//!
//! * no version — `node` — is whatever is on `PATH`, and pins nothing;
//! * an exact version — `pnpm@12.0.0` — is its own pin;
//! * a prefix — `node@26` — is the release `uf.lock` locks it to, or, when
//!   nothing has, the newest release in the publisher's list, which is then
//!   locked. [`crate::lock`] says why it is locked and [`crate::index`] where
//!   the lists come from.
//!
//! # One tool, several uses
//!
//! `runtime: "node@26"` is also what `uf dev` runs when `build.runtime` is
//! absent, so one pin can serve several roles. A tool is listed once with every
//! use it has, rather than once per role, because the question `uf env list`
//! answers is "what does this project install, and what for".
//!
//! # The spellings that came before
//!
//! `env.toolchain` and exact `package.json#engines` entries keep declaring pins
//! that `uf env install` installs and `uf env exec` links, as they always did,
//! and they are listed with the key that declared them. An `engines` entry is a
//! compatibility statement as often as a pin, so it pins only a tool nothing in
//! `uf.config.js` names — the precedence `env.toolchain` already had over it.

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use uf_config::{CapabilityJsHost, PackageManagerName, ToolVersion, UniflowedConfig};

use crate::EnvError;
use crate::index::{self, Index};
use crate::lock::{self, ToolchainLock};
use crate::project;
use crate::tool::{Pin, Platform, Tool};

/// How far resolution may go to answer a prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// Read `uf.lock` and nothing else, and write nothing.
    ///
    /// A prefix nothing has locked is reported as [`Resolution::Unlocked`].
    /// What `uf env list` does, because a listing that fetched would be a
    /// listing that moves the lock.
    LockOnly,
    /// Answer from `uf.lock` where it has an answer, and resolve and lock the
    /// rest.
    ///
    /// What `uf env install` does, and what a command does the first time it
    /// needs a tool. The publisher's list is fetched; when it cannot be, the
    /// last list fetched is used, because locking the newest release uf knows
    /// of is better than refusing to run on a train.
    Missing,
    /// Resolve every prefix again from the publisher's current list, and move
    /// the lock to match.
    ///
    /// What `uf env update` does. A cached list is not good enough here: "the
    /// newest release" is a question about now.
    Latest,
}

/// Where a declared tool is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Use {
    /// What it is used as — `runtime`, `build runtime`, `test runtime`,
    /// `package manager` — or, for the spellings that never said,
    /// `env.toolchain` and `package.json engines`.
    pub role: &'static str,
    /// The key that declared it: `build.runtime`, `test.runner`,
    /// `env.toolchain.node`, `package.json#engines.node`.
    pub key: String,
}

/// Which release a declared tool is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// No version: the tool on `PATH`, which uf does not install.
    OnPath,
    /// An exact version, which is its own answer.
    Exact(String),
    /// A prefix, and the release `uf.lock` locks it to.
    Locked {
        /// The prefix as written: `26`.
        prefix: String,
        /// The release: `26.8.2`.
        version: String,
    },
    /// A prefix this run resolved from the publisher's list and locked.
    Resolved {
        /// The prefix as written.
        prefix: String,
        /// The release it resolved to.
        version: String,
        /// What `uf.lock` locked it to before, when that was something else.
        was: Option<String>,
    },
    /// A prefix nothing has locked yet, in a run that may not resolve one.
    Unlocked {
        /// The prefix as written.
        prefix: String,
    },
}

impl Resolution {
    /// The exact release, when there is one.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Exact(version)
            | Self::Locked { version, .. }
            | Self::Resolved { version, .. } => Some(version),
            Self::OnPath | Self::Unlocked { .. } => None,
        }
    }

    /// The version as the project wrote it: the prefix, or the exact version.
    #[must_use]
    pub fn written(&self) -> Option<&str> {
        match self {
            Self::OnPath => None,
            Self::Exact(version) => Some(version),
            Self::Locked { prefix, .. }
            | Self::Resolved { prefix, .. }
            | Self::Unlocked { prefix } => Some(prefix),
        }
    }
}

/// One tool a project declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// Which tool.
    pub tool: Tool,
    /// Which release of it.
    pub resolution: Resolution,
    /// Every place it is used, in the order a report lists them.
    pub uses: Vec<Use>,
}

impl Declared {
    /// The spec as a project writes it: `node@26`, `pnpm@12.0.0`, `bun`.
    #[must_use]
    pub fn spec(&self) -> String {
        match self.resolution.written() {
            Some(version) => format!("{}@{version}", self.tool.name()),
            None => self.tool.name().to_owned(),
        }
    }

    /// The store entry it is on `platform`, when it has an exact release.
    #[must_use]
    pub fn pin(&self, platform: Platform) -> Option<Pin> {
        self.resolution.version().map(|version| Pin {
            tool: self.tool,
            version: version.to_owned(),
            platform,
        })
    }

    /// Its roles, as a report lists them: `runtime, build runtime`.
    #[must_use]
    pub fn roles(&self) -> String {
        let mut roles: Vec<&str> = Vec::with_capacity(self.uses.len());
        for used in &self.uses {
            if !roles.contains(&used.role) {
                roles.push(used.role);
            }
        }
        roles.join(", ")
    }
}

/// Everything a project declares, resolved as far as a [`Lookup`] allowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolchain {
    /// Every declared tool, runtimes first, once per release.
    pub tools: Vec<Declared>,
    /// The project's `uf.lock`.
    pub lock_path: Utf8PathBuf,
    /// Whether this run changed what `uf.lock` locks.
    pub lock_changed: bool,
}

impl Toolchain {
    /// Every store entry the project uses on `platform`, once each, runtimes
    /// first.
    #[must_use]
    pub fn pins(&self, platform: Platform) -> Vec<Pin> {
        let mut pins: Vec<Pin> = self
            .tools
            .iter()
            .filter_map(|declared| declared.pin(platform))
            .collect();
        pins.sort();
        pins.dedup();
        pins
    }

    /// The entries `uf env exec` puts on `PATH`: one per tool.
    ///
    /// A `PATH` has room for one `node`, and a project may use two —
    /// `build.runtime: "node@24"` beside `runtime: "node@26"`. The one linked is
    /// the one declared first, which puts `runtime`, the default every command
    /// falls back to, ahead of the roles only one command reads, and every key
    /// in `uf.config.js` ahead of the spellings that came before. The others
    /// are in the store all the same, for the commands that run them.
    #[must_use]
    pub fn linked(&self, platform: Platform) -> Vec<Pin> {
        let mut linked: Vec<Pin> = Vec::new();
        for declared in &self.tools {
            let Some(pin) = declared.pin(platform) else {
                continue;
            };
            if linked.iter().all(|existing| existing.tool != pin.tool) {
                linked.push(pin);
            }
        }
        linked
    }
}

/// Where release lists come from.
///
/// A trait so resolution can be exercised without a network; [`Publishers`] is
/// the real one.
pub trait Releases {
    /// The publisher's current list for `tool`.
    ///
    /// # Errors
    ///
    /// When it cannot be fetched or read.
    fn fetch(&self, tool: Tool) -> Result<Index, EnvError>;

    /// The last list fetched for `tool`, read without the network.
    fn cached(&self, tool: Tool) -> Option<Index>;
}

/// The real publishers, through [`crate::index`] and its cache.
#[derive(Debug, Clone, Copy, Default)]
pub struct Publishers;

impl Releases for Publishers {
    fn fetch(&self, tool: Tool) -> Result<Index, EnvError> {
        index::refresh(tool)
    }

    fn cached(&self, tool: Tool) -> Option<Index> {
        index::cached(tool)
    }
}

/// Everything `config` declares for the project at `root`, resolved as far as
/// `lookup` allows, with `uf.lock` rewritten when resolution changed it.
///
/// Rewriting also drops a lock entry for a prefix nothing declares any more,
/// so the record stays a function of the config. [`Lookup::LockOnly`] writes
/// nothing, ever.
///
/// # Errors
///
/// When an `env.toolchain` pin is not exact or names a tool uf does not
/// install; when `uf.lock` or `package.json` cannot be read; when a prefix has
/// to be resolved and no list can be had; when a list has no release for a
/// prefix; or when `uf.lock` cannot be written.
pub fn resolve(
    root: &Utf8Path,
    config: &UniflowedConfig,
    lookup: Lookup,
    releases: &dyn Releases,
) -> Result<Toolchain, EnvError> {
    let lock_path = root.join(config.pm.lockfile.as_str());
    // Held until this returns whenever anything may be written, so a command
    // resolving beside this one reads what this one locked rather than
    // writing over it; see [`lock::Guard`]. A listing writes nothing and waits
    // for nobody.
    let _guard = match lookup {
        Lookup::LockOnly => None,
        Lookup::Missing | Lookup::Latest => Some(lock::guard(&lock_path)?),
    };
    let before = lock::read(&lock_path)?;
    let mut lock = before.clone();
    let mut lists = BTreeMap::new();
    let mut tools = Vec::new();
    for (tool, version, uses) in declarations(root, config)? {
        let resolution = match version {
            ToolVersion::OnPath => Resolution::OnPath,
            ToolVersion::Exact(version) => Resolution::Exact(version.to_string()),
            ToolVersion::Prefix(prefix) => {
                let mut session = Session {
                    lookup,
                    lock_path: &lock_path,
                    lock: &mut lock,
                    lists: &mut lists,
                    releases,
                };
                session.resolve(tool, &prefix)?
            }
        };
        tools.push(Declared {
            tool,
            resolution,
            uses,
        });
    }
    // Stable, so two releases of one tool keep the order they were declared
    // in, which is the order [`Toolchain::linked`] chooses by.
    tools.sort_by_key(|declared| declared.tool);

    let mut lock_changed = false;
    if lookup != Lookup::LockOnly {
        let locked: Vec<String> = tools
            .iter()
            .filter(|declared| {
                matches!(
                    declared.resolution,
                    Resolution::Locked { .. } | Resolution::Resolved { .. }
                )
            })
            .map(Declared::spec)
            .collect();
        lock.retain(|spec| locked.iter().any(|declared| declared == spec));
        if lock != before {
            lock::write(&lock_path, &lock)?;
            lock_changed = true;
        }
    }
    Ok(Toolchain {
        tools,
        lock_path,
        lock_changed,
    })
}

/// The release one declared spec runs, locking a prefix nothing has locked
/// yet.
///
/// What a command calls for the one tool it is about to run — `uf build` for
/// `build.runtime` — where [`resolve`] is what `uf env` calls for all of them.
/// The difference is what it writes: this adds the one prefix it had to
/// resolve and leaves every other entry in `uf.lock` alone, because a command
/// that knows one of the project's tools is in no position to decide that the
/// rest are stale.
///
/// # Errors
///
/// As [`resolve`] with [`Lookup::Missing`], for the one spec.
pub fn release(
    root: &Utf8Path,
    config: &UniflowedConfig,
    tool: Tool,
    version: &ToolVersion,
    releases: &dyn Releases,
) -> Result<Resolution, EnvError> {
    let prefix = match version {
        ToolVersion::OnPath => return Ok(Resolution::OnPath),
        ToolVersion::Exact(version) => return Ok(Resolution::Exact(version.to_string())),
        ToolVersion::Prefix(prefix) => prefix,
    };
    let lock_path = root.join(config.pm.lockfile.as_str());
    // Nearly every run finds the prefix locked and has nothing to write, so it
    // answers without the guard and never waits on an install beside it. Only
    // a prefix nothing has locked takes the guard — and reads the lock again
    // under it, because the command that held it may have locked this very
    // prefix in the meantime. See [`lock::Guard`].
    if let Some(version) = lock::read(&lock_path)?.get(tool, prefix) {
        return Ok(Resolution::Locked {
            prefix: prefix.to_string(),
            version: version.to_owned(),
        });
    }
    let _guard = lock::guard(&lock_path)?;
    let mut lock = lock::read(&lock_path)?;
    let mut lists = BTreeMap::new();
    let resolution = Session {
        lookup: Lookup::Missing,
        lock_path: &lock_path,
        lock: &mut lock,
        lists: &mut lists,
        releases,
    }
    .resolve(tool, prefix)?;
    if matches!(resolution, Resolution::Resolved { .. }) {
        lock::write(&lock_path, &lock)?;
    }
    Ok(resolution)
}

/// The tool a runtime spec names: `node`, `bun` or `deno`.
#[must_use]
pub const fn tool_for_runtime(host: CapabilityJsHost) -> Tool {
    runtime(host)
}

/// The tool a package manager spec names: `npm`, `pnpm`, `yarn` or `bun`.
#[must_use]
pub const fn tool_for_manager(name: PackageManagerName) -> Tool {
    manager(name)
}

/// One resolution run's state: the lock being written and the lists fetched so
/// far, so a project naming `node@26` and `node@24` fetches Node's list once.
struct Session<'a> {
    lookup: Lookup,
    lock_path: &'a Utf8Path,
    lock: &'a mut ToolchainLock,
    lists: &'a mut BTreeMap<Tool, Index>,
    releases: &'a dyn Releases,
}

impl Session<'_> {
    fn resolve(&mut self, tool: Tool, prefix: &str) -> Result<Resolution, EnvError> {
        let locked = self.lock.get(tool, prefix).map(str::to_owned);
        match (self.lookup, locked) {
            (Lookup::LockOnly | Lookup::Missing, Some(version)) => {
                return Ok(Resolution::Locked {
                    prefix: prefix.to_owned(),
                    version,
                });
            }
            (Lookup::LockOnly, None) => {
                return Ok(Resolution::Unlocked {
                    prefix: prefix.to_owned(),
                });
            }
            (Lookup::Missing | Lookup::Latest, _) => {}
        }

        let spec = format!("{}@{prefix}", tool.name());
        // Copied out first: the list is borrowed from `self` for as long as it
        // is read, and the refusal needs these two while it is.
        let (lookup, lock_path) = (self.lookup, self.lock_path);
        let list = self.list(tool).map_err(|error| match lookup {
            Lookup::Missing => EnvError::Unresolvable {
                spec: spec.clone(),
                lock: lock_path.to_path_buf(),
                detail: error.to_string(),
            },
            Lookup::LockOnly | Lookup::Latest => error,
        })?;
        let Some(release) = list.resolve(prefix) else {
            return Err(EnvError::NoSuchRelease {
                spec,
                list: list.sources.join(" and "),
                hint: deno_before_npm(tool, prefix),
            });
        };
        let version = release.version.clone();
        Ok(match self.lock.insert(tool, prefix, &version) {
            Some(was) if was == version => Resolution::Locked {
                prefix: prefix.to_owned(),
                version,
            },
            was => Resolution::Resolved {
                prefix: prefix.to_owned(),
                version,
                was,
            },
        })
    }

    /// `tool`'s list for this run, fetched once.
    fn list(&mut self, tool: Tool) -> Result<&Index, EnvError> {
        if !self.lists.contains_key(&tool) {
            let list = match self.releases.fetch(tool) {
                Ok(list) => list,
                Err(error) if self.lookup == Lookup::Missing => {
                    self.releases.cached(tool).ok_or(error)?
                }
                Err(error) => return Err(error),
            };
            self.lists.insert(tool, list);
        }
        Ok(&self.lists[&tool])
    }
}

/// The sentence that explains an empty answer for an old Deno.
///
/// Deno's list is npm's, and npm has Deno from 1.46 on, so "no release" for
/// `deno@1.40` is true of the list and false of Deno. The reader is told which,
/// and that an exact version needs no list at all.
fn deno_before_npm(tool: Tool, prefix: &str) -> &'static str {
    let mut parts = prefix.split('.').map(|part| part.parse::<u64>().ok());
    let major = parts.next().flatten();
    let minor = parts.next().flatten();
    let before = match (major, minor) {
        (Some(0), _) => true,
        (Some(1), Some(minor)) => minor < 46,
        _ => false,
    };
    if tool == Tool::Deno && before {
        " — Deno's releases are read from npm, which has them from 1.46 on; an exact version \
         such as `deno@1.40.0` needs no list"
    } else {
        ""
    }
}

/// What `config` and the project's manifest declare, each tool once per
/// release with every use it has.
///
/// In the order a report lists them — the keys in `uf.config.js` by role, then
/// `env.toolchain`, then exact `engines` — which is also the order
/// [`Toolchain::linked`] prefers.
fn declarations(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<(Tool, ToolVersion, Vec<Use>)>, EnvError> {
    let mut found = Vec::new();
    for (role, declared) in [
        ("runtime", config.runtime_tool()),
        ("build runtime", config.build_runtime_tool()),
        ("test runtime", config.test_runtime_tool()),
    ] {
        if let Some(declared) = declared {
            let key = declared.source.key().unwrap_or(role).to_owned();
            add(
                &mut found,
                runtime(declared.spec.name),
                declared.spec.version,
                role,
                key,
            );
        }
    }
    if let Some(declared) = config.package_manager_tool() {
        let key = declared.source.key().unwrap_or("packageManager").to_owned();
        add(
            &mut found,
            manager(declared.spec.name),
            declared.spec.version,
            "package manager",
            key,
        );
    }

    let named: Vec<Tool> = found.iter().map(|(tool, _, _)| *tool).collect();
    for (name, version) in &config.env.toolchain {
        let tool = Tool::parse(name).ok_or_else(|| EnvError::UnknownTool {
            name: name.to_string(),
        })?;
        let version = version.trim();
        if !project::is_exact_version(version) {
            return Err(EnvError::NotAnExactVersion {
                tool: tool.name(),
                version: version.to_owned(),
            });
        }
        add(
            &mut found,
            tool,
            ToolVersion::Exact(version.into()),
            "env.toolchain",
            format!("env.toolchain.{name}"),
        );
    }
    for (name, version) in project::engines(&root.join("package.json"))? {
        let Some(tool) = Tool::parse(&name) else {
            continue;
        };
        if named.contains(&tool) || config.env.toolchain.contains_key(name.as_str()) {
            continue;
        }
        add(
            &mut found,
            tool,
            ToolVersion::Exact(version),
            "package.json engines",
            format!("package.json#engines.{name}"),
        );
    }
    Ok(found)
}

/// Record one use, on the entry for that tool and release when there is one.
fn add(
    found: &mut Vec<(Tool, ToolVersion, Vec<Use>)>,
    tool: Tool,
    version: ToolVersion,
    role: &'static str,
    key: String,
) {
    let used = Use { role, key };
    match found
        .iter_mut()
        .find(|(existing, written, _)| *existing == tool && *written == version)
    {
        Some((_, _, uses)) => uses.push(used),
        None => found.push((tool, version, vec![used])),
    }
}

/// The tool a runtime spec names.
const fn runtime(host: CapabilityJsHost) -> Tool {
    match host {
        CapabilityJsHost::Node => Tool::Node,
        CapabilityJsHost::Bun => Tool::Bun,
        CapabilityJsHost::Deno => Tool::Deno,
    }
}

/// The tool a package manager spec names.
const fn manager(name: PackageManagerName) -> Tool {
    match name {
        PackageManagerName::Npm => Tool::Npm,
        PackageManagerName::Pnpm => Tool::Pnpm,
        PackageManagerName::Yarn => Tool::Yarn,
        PackageManagerName::Bun => Tool::Bun,
    }
}

#[cfg(test)]
mod tests;
