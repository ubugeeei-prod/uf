//! What a project's code may reach, declared once and translated per host.
//!
//! Deno's contribution to this class of tool was never the runtime: it was that
//! a program declares what it may reach — the filesystem, the network, the
//! environment, another program — and gets nothing it did not ask for. Node has
//! `--permission` and Bun has nothing, and uf runs on all three. Because uf owns
//! the Capability JS Host abstraction, the permission set can be a property of
//! the *toolchain* rather than of whichever runtime a project happens to be on:
//! one declaration in `uf.config.js`, translated here.
//!
//! # The rule that shapes every decision below
//!
//! `docs/security.md`'s standard is that an unsupported configuration is
//! rejected clearly rather than accepted and quietly meaning something weaker.
//! A permission set a host cannot enforce is exactly that case, so
//! [`host_arguments`] returns an error naming the categories rather than
//! dropping them — and the error names a host that *can* enforce them. Silently
//! passing four of five `--allow-*` flags would produce a run that looks
//! sandboxed and is not, which is worse than no permission model at all because
//! somebody would rely on it.
//!
//! The same rule is why [`RuntimeHost::Bun`] is a refusal rather than a no-op.
//!
//! # What a permission set does and does not restrict
//!
//! A host has to read the project to run it: the module graph, `node_modules`,
//! the loader, the worker. Those reads are uf's own and are added to whatever
//! the project declared — see [`ToolchainAccess`] — so **a permission set does
//! not narrow a run's access to the project's own files. What it denies is the
//! rest of the machine.** `~/.ssh`, `~/.aws`, `/etc`, the network, the
//! environment: a test that wants one of those has to say so in
//! `uf.config.js`, which is the property the model exists to provide.
//!
//! Writing that down is not a caveat, it is the specification. A reader who
//! believed the declared `read` list was the *whole* list would think a test
//! could be denied one of its own fixtures, and would be surprised in the
//! direction that matters.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::kind::RuntimeHost;

/// One capability a permission set can name.
///
/// Five, and deliberately the five Deno has: the model is uf's, but a category
/// no host can express would be a category nothing enforces. Adding one means
/// finding a host that enforces it first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    /// Reading a path.
    Read,
    /// Writing a path.
    Write,
    /// Opening a network connection to a host.
    Net,
    /// Reading an environment variable.
    Env,
    /// Starting another program.
    Run,
}

impl Permission {
    /// Every permission, in the order they are documented.
    pub const ALL: &'static [Self] = &[Self::Read, Self::Write, Self::Net, Self::Env, Self::Run];

    /// The key a person writes in `uf.config.js`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Net => "net",
            Self::Env => "env",
            Self::Run => "run",
        }
    }

    /// What an entry in this list names, for an error message.
    #[must_use]
    pub const fn subject(self) -> &'static str {
        match self {
            Self::Read | Self::Write => "a path",
            Self::Net => "a host, optionally with a port",
            Self::Env => "a variable name",
            Self::Run => "a program",
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a project declares its own code may reach.
///
/// Absent from `uf.config.js` — `None` where this is held — means no permission
/// model: the host runs as it always did. Present means **deny by default**: a
/// category with no entries grants nothing, because that is what a host's
/// missing `--allow-*` flag already means and a second meaning for "empty"
/// would be a trap. There is deliberately no way to spell "everything"; a
/// project that wants everything does not declare a permission set.
///
/// `deny_unknown_fields` because this is the one config block where a typo is a
/// security bug: `permissions: { nett: [...] }` would otherwise deserialize to
/// a set granting no network at all, and the project would run believing it had
/// declared one. Everywhere else in `uf.config.js` an unknown key is ignored;
/// here it is an error naming the key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
#[non_exhaustive]
pub struct Permissions {
    /// Paths the project's code may read.
    pub read: Vec<String>,
    /// Paths the project's code may write.
    pub write: Vec<String>,
    /// Network hosts the project's code may reach.
    pub net: Vec<String>,
    /// Environment variables the project's code may read.
    pub env: Vec<String>,
    /// Programs the project's code may start.
    pub run: Vec<String>,
}

impl Permissions {
    /// The entries declared for one permission.
    #[must_use]
    pub fn entries(&self, permission: Permission) -> &[String] {
        match permission {
            Permission::Read => &self.read,
            Permission::Write => &self.write,
            Permission::Net => &self.net,
            Permission::Env => &self.env,
            Permission::Run => &self.run,
        }
    }

    /// The permissions this set actually grants something under.
    pub fn granted(&self) -> impl Iterator<Item = Permission> + '_ {
        Permission::ALL
            .iter()
            .copied()
            .filter(|permission| !self.entries(*permission).is_empty())
    }
}

/// What uf itself must reach on the host, whatever the project declared.
///
/// A worker cannot import a test file it may not read, and cannot transform
/// Flow without starting `uf transform`. These are not the project's
/// permissions and are not read from its configuration: they are what the
/// command being run needs in order to be that command at all, and a run that
/// omitted them would fail with a permission error inside uf's own loader
/// rather than in anything a person wrote.
///
/// They are added to the declared set rather than replacing it, and
/// [`explain`] is what makes them visible — a grant nobody can see is the same
/// silent widening this module exists to refuse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolchainAccess {
    /// Paths the host reads to load the project: its root, and `node_modules`.
    pub read: Vec<String>,
    /// Paths the run writes: `.uf`, and a coverage directory when there is one.
    pub write: Vec<String>,
    /// Programs uf starts on the project's behalf — the `uf` binary itself.
    pub run: Vec<String>,
    /// Whether uf's Flow loader runs on a thread of the host's.
    ///
    /// Node's `register()` installs module hooks on a loader thread rather than
    /// on the main one, and Node's permission model classifies that thread as
    /// `WorkerThreads`: without `--allow-worker` the very first import fails
    /// with `ERR_ACCESS_DENIED`, before any of the project's code runs. It is a
    /// field rather than a constant because it is a property of *how a
    /// particular command loads Flow*, not of the host — `uf transform`'s own
    /// process needs no such grant.
    ///
    /// Every other host ignores it. Deno's permission model has no thread
    /// dimension at all, and Bun refuses a permission set outright.
    pub loader_thread: bool,
}

/// Why a host cannot be handed a permission set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionError {
    /// The host has no way to enforce these permissions.
    Unenforceable {
        /// The host that was asked.
        host: RuntimeHost,
        /// The permissions it cannot enforce, in [`Permission::ALL`] order.
        permissions: Vec<Permission>,
    },
    /// An entry cannot survive the host's own argument syntax.
    ///
    /// Deno takes one `--allow-read` whose value is comma-separated, so an
    /// entry containing a comma would arrive as two grants and the second would
    /// be one nobody wrote. Refusing is the only answer that cannot widen the
    /// set: quoting is not available and dropping the entry would narrow it
    /// without saying so.
    Unexpressible {
        /// The host that was asked.
        host: RuntimeHost,
        /// The permission the entry was declared under.
        permission: Permission,
        /// The entry itself.
        entry: String,
    },
}

impl fmt::Display for PermissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unenforceable { host, permissions } => {
                let names = permissions
                    .iter()
                    .map(|permission| format!("`{permission}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                match host {
                    RuntimeHost::Bun => write!(
                        f,
                        "`uf.config.js` declares permissions ({names}) and Bun has no permission \
                         model to enforce them with. uf will not run a set it cannot enforce and \
                         call it enforced: remove the block, or run this on Deno, which enforces \
                         all five, or on Node.js, which enforces `read` and `write`"
                    ),
                    _ => write!(
                        f,
                        "{} cannot enforce {names}. Its permission system has `--allow-fs-read` \
                         and `--allow-fs-write` and nothing for the network, the environment or a \
                         named program, so passing the rest would produce a run that looks \
                         sandboxed and is not. Drop those keys, or run this on Deno, which \
                         enforces all five",
                        host.display_name(),
                    ),
                }
            }
            Self::Unexpressible {
                host,
                permission,
                entry,
            } => write!(
                f,
                "{} takes one `--allow-{permission}` whose entries are separated by commas, and \
                 `{entry}` — declared under `permissions.{permission}` — contains one. It would \
                 arrive as two grants, the second of which nobody wrote. Name {} without a comma \
                 in it",
                host.display_name(),
                permission.subject(),
            ),
        }
    }
}

impl std::error::Error for PermissionError {}

/// The arguments that put `permissions` in force on `host`, or why they cannot.
///
/// The result goes *before* the module the host runs, which is where every host
/// wants its own flags, and it carries no subcommand: `deno run` is part of
/// starting Deno at all and belongs with the rest of that host's invocation.
///
/// # Node
///
/// `--permission` and the two flags that exist under it. Node's model has no
/// network, environment or program dimension at all, so a set naming one of
/// those is refused rather than partly applied.
///
/// `--allow-child-process` is granted whenever uf has to start a program of its
/// own, and that is the one place this is weaker than it reads: Node cannot
/// scope it to a program, so a run whose Flow is transformed by `uf transform`
/// — which is every run — can start *any* child. That is why a project's own
/// `run` list is refused here rather than translated: uf can disclose that it
/// needed the hole, and it can decline to pretend the hole has a shape.
///
/// # Deno
///
/// All five, each as one flag whose entries are comma-separated. A permission
/// with no entries gets no flag, which is Deno's own "denied".
///
/// # Bun
///
/// Nothing to translate into. Refused.
pub fn host_arguments(
    host: RuntimeHost,
    permissions: &Permissions,
    toolchain: &ToolchainAccess,
) -> Result<Vec<String>, PermissionError> {
    match host {
        RuntimeHost::Node => node_arguments(permissions, toolchain),
        RuntimeHost::Deno => deno_arguments(host, permissions, toolchain),
        _ => Err(PermissionError::Unenforceable {
            host,
            permissions: permissions.granted().collect(),
        }),
    }
}

fn node_arguments(
    permissions: &Permissions,
    toolchain: &ToolchainAccess,
) -> Result<Vec<String>, PermissionError> {
    let unenforceable: Vec<Permission> = permissions
        .granted()
        .filter(|permission| !matches!(permission, Permission::Read | Permission::Write))
        .collect();
    if !unenforceable.is_empty() {
        return Err(PermissionError::Unenforceable {
            host: RuntimeHost::Node,
            permissions: unenforceable,
        });
    }

    let mut arguments = vec![String::from("--permission")];
    // Repeated rather than comma-joined: Node accepts either, and a path with a
    // comma in it is a path, so the spelling that cannot misread one is the
    // right spelling. Deno has no such choice, which is what
    // `PermissionError::Unexpressible` is about.
    for path in toolchain.read.iter().chain(&permissions.read) {
        arguments.push(format!("--allow-fs-read={path}"));
    }
    for path in toolchain.write.iter().chain(&permissions.write) {
        arguments.push(format!("--allow-fs-write={path}"));
    }
    // Both of these Node itself warns about at startup —
    // "must be used with extreme caution. It could invalidate the permission
    // model" — and both are here because uf's own Flow loader needs them: the
    // hooks run on a loader thread, and every module they transform goes
    // through a `uf transform` child. Neither is scoped, which is why a
    // project's own `run` list is refused above rather than translated into
    // one of them, and why `docs/security.md` says in as many words what a
    // test can still reach through them.
    if toolchain.loader_thread {
        arguments.push(String::from("--allow-worker"));
    }
    if !toolchain.run.is_empty() {
        arguments.push(String::from("--allow-child-process"));
    }
    Ok(arguments)
}

fn deno_arguments(
    host: RuntimeHost,
    permissions: &Permissions,
    toolchain: &ToolchainAccess,
) -> Result<Vec<String>, PermissionError> {
    const NONE: &[String] = &[];
    let mut arguments = Vec::new();
    for permission in Permission::ALL.iter().copied() {
        // uf's own grants exist for three of the five. It never needs the
        // network or the environment on the project's behalf, and a category it
        // does not need is one it must not quietly open.
        let toolchain_entries: &[String] = match permission {
            Permission::Read => &toolchain.read,
            Permission::Write => &toolchain.write,
            Permission::Run => &toolchain.run,
            Permission::Net | Permission::Env => NONE,
        };
        let entries: Vec<&String> = toolchain_entries
            .iter()
            .chain(permissions.entries(permission))
            .collect();
        if entries.is_empty() {
            continue;
        }
        for entry in &entries {
            if entry.contains(',') {
                return Err(PermissionError::Unexpressible {
                    host,
                    permission,
                    entry: (*entry).clone(),
                });
            }
        }
        let joined = entries
            .iter()
            .map(|entry| entry.as_str())
            .collect::<Vec<_>>()
            .join(",");
        arguments.push(format!("--allow-{permission}={joined}"));
    }
    Ok(arguments)
}

/// One line per permission, saying who enforces it and how.
///
/// This is what `uf explain` prints. A permission model whose translation
/// nobody can see is a permission model nobody can check, and the toolchain's
/// own grants — the ones a project did not declare — are exactly the ones a
/// reader has to be told about.
#[must_use]
pub fn explain(
    host: RuntimeHost,
    permissions: &Permissions,
    toolchain: &ToolchainAccess,
) -> Vec<String> {
    let mut lines = Vec::new();
    for permission in Permission::ALL.iter().copied() {
        let declared = permissions.entries(permission).len();
        let added = match (host, permission) {
            (RuntimeHost::Node, Permission::Read) | (RuntimeHost::Deno, Permission::Read) => {
                toolchain.read.len()
            }
            (RuntimeHost::Node, Permission::Write) | (RuntimeHost::Deno, Permission::Write) => {
                toolchain.write.len()
            }
            (RuntimeHost::Deno, Permission::Run) => toolchain.run.len(),
            _ => 0,
        };
        let enforced = match host {
            RuntimeHost::Node => matches!(permission, Permission::Read | Permission::Write),
            RuntimeHost::Deno => true,
            _ => false,
        };
        let how = match (enforced, host, permission) {
            (false, RuntimeHost::Bun, _) => "not enforced: Bun has no permission model".to_string(),
            (false, RuntimeHost::Node, Permission::Run) => {
                "not enforced: `--allow-child-process` is all programs or none, so uf refuses a \
                 list it could only widen"
                    .to_string()
            }
            (false, _, _) => format!("not enforced: {} has no such flag", host.display_name()),
            (true, RuntimeHost::Node, Permission::Read) => {
                "`node --permission --allow-fs-read`".to_string()
            }
            (true, RuntimeHost::Node, Permission::Write) => {
                "`node --permission --allow-fs-write`".to_string()
            }
            (true, _, _) => format!("`deno run --allow-{permission}`"),
        };
        lines.push(format!(
            "{permission}: {declared} declared, {added} added by uf, {how}"
        ));
    }
    lines
}
