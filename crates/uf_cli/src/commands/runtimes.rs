//! Which runtime a command starts, and which release of a package manager it
//! drives.
//!
//! # What this answers
//!
//! `uf.config.js` can name the tool each command runs on —
//! `build: { runtime: "node@26" }`, `test: { runtime: "bun@1.4" }`,
//! `packageManager: "pnpm@10"` — and `uf_config::tools` reads that, with the key
//! that said it. This turns the answer into a process: a program to start, and
//! the directories to put in front of `PATH` for everything that program
//! starts in turn. See ubugeeei-prod/uf#940.
//!
//! | Command | Role | Keys, in order |
//! | --- | --- | --- |
//! | `uf dev`, `uf build`, `uf preview` | [`Role::Build`] | `build.runtime`, `runtime` |
//! | `uf test` | [`Role::Test`] | `test.runtime`, the runner's own, `runtime` |
//! | `uf start`, `uf run`, `uf exec` | [`Role::Runtime`] | `runtime` |
//! | `uf install`, `uf add`, … | [`manager_path`] | `packageManager`, and `runtime` for the Node it runs on |
//!
//! # Three answers
//!
//! * **Nothing declared**: the host [`resolve_host`] has always found —
//!   `app.runtime.capabilityJsHost`, on `PATH` — and the manager on `PATH`. A
//!   project that writes none of the new keys runs exactly as it did.
//! * **A name with no version** — `runtime: "node"` — the one on `PATH`, and a
//!   refusal naming the key when there is none, rather than a fall back to
//!   another host: the project said which one it means.
//! * **A version**: the release `uf.lock` locks it to — resolved and locked the
//!   first time — installed into the store the first time, and started from
//!   there. Each of those firsts says so, once, and a run after them says
//!   nothing at all.
//!
//! # Why the directory goes in front of `PATH`
//!
//! Starting the pinned `node` by absolute path is half of running on it. Vite
//! starts workers, a plugin shells out to `node`, a package's bin script is
//! `#!/usr/bin/env node`, and every one of those finds its runtime on `PATH`.
//! [`Runtime::environment`] is how the other half is reached: every process uf
//! starts for the project's code takes that environment. A package manager is
//! the same case twice over — pnpm is itself a `#!/usr/bin/env node` program —
//! which is why [`manager_path`] carries the runtime as well as the manager.

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;
use uf_config::env_files::ProjectEnv;
use uf_config::{DeclaredTool, ResolvedConfig, RuntimeSpec, ToolVersion, UniflowedConfig, Written};
use uf_env::toolchain::{Publishers, Releases, Resolution};

use crate::commands::vite::{Host, find_program, resolve_host};

/// Which declared runtime a command reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// `uf dev`, `uf build` and `uf preview`: `build.runtime`, then `runtime`.
    Build,
    /// `uf test`: `test.runtime`, then the runtime the runner brings, then
    /// `runtime`.
    Test,
    /// `uf start`, `uf run` and `uf exec`: `runtime`.
    Runtime,
}

impl Role {
    /// What the project declares for this role, when it declares anything.
    pub(crate) fn declared(self, config: &UniflowedConfig) -> Option<DeclaredTool<RuntimeSpec>> {
        match self {
            Self::Build => config.build_runtime_tool(),
            Self::Test => config.test_runtime_tool(),
            Self::Runtime => config.runtime_tool(),
        }
    }

    /// What the runtime does for the commands in this role, as a clause.
    const fn purpose(self) -> &'static str {
        match self {
            Self::Build => "runs the builder and any JavaScript plugin",
            Self::Test => "runs the test workers",
            Self::Runtime => "runs the server a build emitted, and every task",
        }
    }
}

/// The runtime a command starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Runtime {
    /// The host to start.
    pub(crate) host: Host,
    /// The directory to put in front of `PATH` for everything the command
    /// starts, when the runtime is one uf installed.
    pub(crate) path: Option<Utf8PathBuf>,
}

impl Runtime {
    /// `env`, with this runtime in front of `PATH` when it came from the store.
    ///
    /// A runtime found on `PATH` changes nothing: it is already the first
    /// `node` a child process finds, which is how it was found.
    pub(crate) fn environment(&self, env: ProjectEnv) -> ProjectEnv {
        match &self.path {
            Some(directory) => env.with_path_prefix(directory.clone()),
            None => env,
        }
    }
}

/// The runtime `role` names for this project, installed if it has to be.
///
/// `notify` is told each thing that is about to take a while or change a file
/// — a prefix locked in `uf.lock`, a release being downloaded — one sentence
/// each, so a command can finish its progress line before saying it.
///
/// # Errors
///
/// When nothing is declared and no host is on `PATH`; when a name with no
/// version is not on `PATH`; when a prefix cannot be resolved or a release
/// cannot be installed — offline, or with no build for this platform — each
/// naming the key and the spec.
pub(crate) fn resolve(
    resolved: &ResolvedConfig,
    role: Role,
    notify: &mut dyn FnMut(&str),
) -> Result<Runtime> {
    resolve_with(resolved, role, notify, &Publishers)
}

/// [`resolve`], with the release lists coming from `releases`.
pub(crate) fn resolve_with(
    resolved: &ResolvedConfig,
    role: Role,
    notify: &mut dyn FnMut(&str),
    releases: &dyn Releases,
) -> Result<Runtime> {
    let config = &resolved.config;
    let Some(declared) = role.declared(config) else {
        return Ok(Runtime {
            host: resolve_host(config)?,
            path: None,
        });
    };
    let key = declared.source.key().unwrap_or("runtime");
    let spec = &declared.spec;
    let kind = spec.name;
    let name = kind.as_str();

    if spec.version == ToolVersion::OnPath {
        let program = find_program(name).ok_or_else(|| {
            anyhow!(
                "{key} is `{spec}`, which is whatever `{name}` is on PATH, and there is no \
                 `{name}` on PATH. Install it, or write a version — `{name}@<major>` — and uf \
                 installs that release the first time a command needs it"
            )
        })?;
        return Ok(Runtime {
            host: Host { kind, program },
            path: None,
        });
    }

    let tool = uf_env::toolchain::tool_for_runtime(kind);
    let wanted = Wanted {
        key,
        spec: &spec.to_string(),
        tool,
        version: &spec.version,
    };
    let bin = store_tool(resolved, &wanted, false, notify, releases)?;
    Ok(Runtime {
        host: Host {
            kind,
            program: bin.join(name),
        },
        path: Some(bin),
    })
}

/// The directories to put in front of `PATH` for the package manager a command
/// drives, installing what has to be.
///
/// Two, at most, in this order: the release `packageManager` pins, when it pins
/// one for `manager`; and the release `runtime` pins, because npm, pnpm and
/// Yarn are Node programs and the Node they start on is the project's
/// runtime like any other. Empty for a project that pins neither, which is the
/// manager and the Node on `PATH`, exactly as before.
///
/// `frozen` is `uf install --frozen-lockfile`, which installs what the
/// lockfiles pin and changes none of them. A prefix nothing has locked yet
/// would have to be written into `uf.lock`, so it is refused rather than
/// resolved.
///
/// # Errors
///
/// As [`resolve`], and when `frozen` meets a prefix that is not locked.
pub(crate) fn manager_path(
    resolved: &ResolvedConfig,
    manager: uf_pm::PackageManager,
    frozen: bool,
    notify: &mut dyn FnMut(&str),
) -> Result<Vec<Utf8PathBuf>> {
    manager_path_with(resolved, manager, frozen, notify, &Publishers)
}

/// [`manager_path`], with the release lists coming from `releases`.
pub(crate) fn manager_path_with(
    resolved: &ResolvedConfig,
    manager: uf_pm::PackageManager,
    frozen: bool,
    notify: &mut dyn FnMut(&str),
    releases: &dyn Releases,
) -> Result<Vec<Utf8PathBuf>> {
    let config = &resolved.config;
    let mut path = Vec::new();
    if let Some(spec) = config.package_manager.as_ref().and_then(Written::spec)
        && spec.version != ToolVersion::OnPath
        && uf_pm::PackageManager::from_spec(spec) == manager
    {
        let wanted = Wanted {
            key: "packageManager",
            spec: &spec.to_string(),
            tool: uf_env::toolchain::tool_for_manager(spec.name),
            version: &spec.version,
        };
        path.push(store_tool(resolved, &wanted, frozen, notify, releases)?);
    }
    if let Some(declared) = Role::Runtime.declared(config)
        && declared.spec.version != ToolVersion::OnPath
    {
        let wanted = Wanted {
            key: declared.source.key().unwrap_or("runtime"),
            spec: &declared.spec.to_string(),
            tool: uf_env::toolchain::tool_for_runtime(declared.spec.name),
            version: &declared.spec.version,
        };
        path.push(store_tool(resolved, &wanted, frozen, notify, releases)?);
    }
    Ok(path)
}

/// One versioned declaration, as the sentences about it name it.
struct Wanted<'a> {
    /// The key that declared it: `build.runtime`, `packageManager`.
    key: &'a str,
    /// The spec as written: `node@26`.
    spec: &'a str,
    /// Which tool.
    tool: uf_env::Tool,
    /// What follows the `@`.
    version: &'a ToolVersion,
}

/// The release `wanted` names — locked, installed and linked — as the
/// directory its links are in.
///
/// The one path every first use takes, for a runtime and a manager alike, so
/// the lock, the install and the two sentences that announce them are the same
/// for both.
fn store_tool(
    resolved: &ResolvedConfig,
    wanted: &Wanted<'_>,
    frozen: bool,
    notify: &mut dyn FnMut(&str),
    releases: &dyn Releases,
) -> Result<Utf8PathBuf> {
    let config = &resolved.config;
    let Wanted {
        key,
        spec,
        tool,
        version,
    } = *wanted;
    let name = tool.name();
    let lockfile = config.pm.lockfile.as_str();
    let platform = uf_env::Platform::current().ok_or_else(|| {
        anyhow!(
            "{key} is `{spec}`, and uf does not install tools for {} on {}; write `{name}` to \
             use the one on PATH",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    if frozen && let ToolVersion::Prefix(prefix) = version {
        let lock = uf_env::lock::read(&resolved.root.join(lockfile))?;
        if lock.get(tool, prefix).is_none() {
            bail!(
                "{key} is `{spec}`, which is not locked in {lockfile} yet, and a frozen install \
                 changes no lockfile. Run `uf env install` — or `uf install` without \
                 `--frozen-lockfile` — and commit {lockfile}"
            );
        }
    }

    let resolution = uf_env::toolchain::release(&resolved.root, config, tool, version, releases)
        .with_context(|| format!("{key} is `{spec}`"))?;
    let Some(release) = resolution.version() else {
        // `release` answers a versioned spec with a release or an error.
        return Err(anyhow!(
            "{key} is `{spec}`, and no release could be settled for it"
        ));
    };
    if let Resolution::Resolved { version, .. } = &resolution {
        notify(&format!(
            "{key} is `{spec}`: locked at {version} in {lockfile}"
        ));
    }
    let pin = uf_env::Pin {
        tool,
        version: release.to_owned(),
        platform,
    };

    let store = uf_env::Store::discover()?;
    if !store.has(&pin) {
        notify(&format!(
            "installing {pin} for {key} — once, into the store every project on this machine \
             shares"
        ));
        uf_env::archive::ensure(&store, &pin)
            .with_context(|| format!("{key} is `{spec}`, and {pin} could not be installed"))?;
    }
    let envs = uf_env::project::Envs::discover()?;
    let bin = uf_env::project::link_pin(&envs, &store, &pin)
        .with_context(|| format!("{key} is `{spec}`, and {pin} could not be linked"))?;
    // Held for this project, so `uf env gc` does not collect a tool a command
    // is using just because `uf env install` was never run here.
    uf_env::Roots::discover()?.add(&resolved.root, &[pin.slug()])?;
    Ok(bin)
}

/// What [`resolve`] would start for a role, read rather than done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Described {
    /// The tool, as a provider: `node 26.8.2`, or the spec while no release is
    /// settled.
    pub(crate) provider: String,
    /// The key, the lock and the store, in one sentence.
    pub(crate) detail: String,
}

/// What `role` runs on for this project, without resolving, fetching or
/// installing anything — or `None` when nothing declares a runtime for it, and
/// the host `resolve_host` finds is still the answer.
///
/// For `uf explain` and `uf inspect`, which describe a plan and must neither
/// fail nor download because of the machine they are run on. So a prefix
/// nothing has locked is said to be resolved on first use rather than resolved
/// here, and a release the store lacks is said to be installed on first use.
pub(crate) fn describe(resolved: &ResolvedConfig, role: Role) -> Option<Described> {
    let declared = role.declared(&resolved.config)?;
    let wanted = Wanted {
        key: declared.source.key().unwrap_or("runtime"),
        spec: &declared.spec.to_string(),
        tool: uf_env::toolchain::tool_for_runtime(declared.spec.name),
        version: &declared.spec.version,
    };
    Some(describe_wanted(resolved, &wanted, role.purpose()))
}

/// What `packageManager` pins, read the way [`describe`] reads a runtime, or
/// `None` when the key is absent or names a manager other than `manager`, the
/// one the command runs — the same test [`manager_path`] makes.
pub(crate) fn describe_manager(
    resolved: &ResolvedConfig,
    manager: uf_pm::PackageManager,
) -> Option<Described> {
    let spec = resolved
        .config
        .package_manager
        .as_ref()
        .and_then(Written::spec)
        .filter(|spec| uf_pm::PackageManager::from_spec(spec) == manager)?;
    let wanted = Wanted {
        key: "packageManager",
        spec: &spec.to_string(),
        tool: uf_env::toolchain::tool_for_manager(spec.name),
        version: &spec.version,
    };
    Some(describe_wanted(
        resolved,
        &wanted,
        "installs, adds and removes dependencies",
    ))
}

fn describe_wanted(resolved: &ResolvedConfig, wanted: &Wanted<'_>, purpose: &str) -> Described {
    let Wanted {
        key,
        spec,
        tool,
        version,
    } = *wanted;
    let name = tool.name();
    let lockfile = resolved.config.pm.lockfile.as_str();

    let (release, named) = match version {
        ToolVersion::OnPath => {
            return Described {
                provider: format!("{name} (on PATH)"),
                detail: format!(
                    "{purpose}; `{key}` names no version, so whichever `{name}` is on PATH runs, \
                     and a machine without one is told so rather than handed another"
                ),
            };
        }
        ToolVersion::Exact(version) => (
            Some(version.to_string()),
            format!("`{key}` names exactly {version}"),
        ),
        ToolVersion::Prefix(prefix) => {
            let locked = uf_env::lock::read(&resolved.root.join(lockfile))
                .ok()
                .and_then(|lock| lock.get(tool, prefix).map(str::to_owned));
            match locked {
                Some(version) => (
                    Some(version.clone()),
                    format!("`{key}` names {spec}, locked at {version} in {lockfile}"),
                ),
                None => (
                    None,
                    format!(
                        "`{key}` names {spec}, not locked yet — the newest release is resolved \
                         and locked in {lockfile} the first time a command needs it"
                    ),
                ),
            }
        }
    };
    let stored = match (
        &release,
        uf_env::Platform::current(),
        uf_env::Store::discover().ok(),
    ) {
        (Some(version), Some(platform), Some(store)) => store.has(&uf_env::Pin {
            tool,
            version: version.clone(),
            platform,
        }),
        _ => false,
    };
    let store = if stored {
        "in the store"
    } else {
        "installed into the store on first use"
    };
    Described {
        provider: match &release {
            Some(version) => format!("{name} {version}"),
            None => spec.to_owned(),
        },
        detail: format!(
            "{purpose}; {named}; {store}, and first on PATH for every process it starts"
        ),
    }
}

/// The release `uf.lock` locks a role's prefix to, when the role declares a
/// prefix and something has locked it.
///
/// Read without touching the network, for `uf inspect` and `uf explain`. A
/// test runner that brings its own runtime locks under that runtime's spec,
/// which is the same text: `bun@1.4` is one entry whichever key wrote it.
pub(crate) fn locked(resolved: &ResolvedConfig, role: uf_config::ToolRole) -> Option<String> {
    use uf_config::ToolRole;

    let config = &resolved.config;
    let runtime = |declared: Option<DeclaredTool<RuntimeSpec>>| {
        declared.and_then(|declared| match declared.spec.version {
            ToolVersion::Prefix(prefix) => Some((
                uf_env::toolchain::tool_for_runtime(declared.spec.name),
                prefix,
            )),
            ToolVersion::OnPath | ToolVersion::Exact(_) => None,
        })
    };
    let (tool, prefix) = match role {
        ToolRole::Runtime => runtime(config.runtime_tool()),
        ToolRole::BuildRuntime => runtime(config.build_runtime_tool()),
        ToolRole::TestRuntime => runtime(config.test_runtime_tool()),
        ToolRole::TestRunner => runtime(config.test_runner_tool().spec.implied_runtime().map(
            |spec| DeclaredTool {
                spec,
                source: uf_config::ToolSource::ImpliedBy("test.runner"),
            },
        )),
        ToolRole::PackageManager => {
            config
                .package_manager_tool()
                .and_then(|declared| match declared.spec.version {
                    ToolVersion::Prefix(prefix) => Some((
                        uf_env::toolchain::tool_for_manager(declared.spec.name),
                        prefix,
                    )),
                    ToolVersion::OnPath | ToolVersion::Exact(_) => None,
                })
        }
        ToolRole::Builder => None,
    }?;
    let lock = uf_env::lock::read(&resolved.root.join(config.pm.lockfile.as_str())).ok()?;
    lock.get(tool, &prefix).map(str::to_owned)
}
