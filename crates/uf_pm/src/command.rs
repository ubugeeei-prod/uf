//! Command mapping from a detected [`PackageManager`] to a concrete invocation.
//!
//! `uf` drives whichever manager a project already uses, so every supported
//! operation is table-driven per manager and per edition. The table is the only
//! source of program names: [`Invocation::program`] is always a `&'static str`
//! from [`PROGRAMS`], never a value read from a manifest, so a hostile
//! `package.json` cannot inject a program name or an argument.
//!
//! Invocations are meant for `std::process::Command`, which spawns the program
//! directly. Nothing here is shell syntax, and nothing here may be handed to a
//! shell: [`Invocation`]'s `Display` is a diagnostic rendering only.
//!
//! # Table
//!
//! | Operation | uf | npm | pnpm | yarn classic | yarn berry | bun |
//! | --------- | -- | --- | ---- | ------------ | ---------- | --- |
//! | `Install` | `uf install` | `npm install` | `pnpm install` | `yarn install` | `yarn install` | `bun install` |
//! | `InstallFrozen` | `uf install --frozen-lockfile` | `npm ci` | `pnpm install --frozen-lockfile` | `yarn install --frozen-lockfile` | `yarn install --immutable` | `bun install --frozen-lockfile` |
//! | `Add { kind: Prod }` | `uf add` | `npm install` | `pnpm add` | `yarn add` | `yarn add` | `bun add` |
//! | `Add { kind: Dev }` | `uf add --dev` | `npm install --save-dev` | `pnpm add --save-dev` | `yarn add --dev` | `yarn add --dev` | `bun add --dev` |
//! | `Add { kind: Optional }` | `uf add --optional` | `npm install --save-optional` | `pnpm add --save-optional` | `yarn add --optional` | `yarn add --optional` | `bun add --optional` |
//! | `Add { kind: Peer }` | `uf add --peer` | `npm install --save-peer` | `pnpm add --save-peer` | `yarn add --peer` | `yarn add --peer` | `bun add --peer` |
//! | `Remove` | `uf remove` | `npm uninstall` | `pnpm remove` | `yarn remove` | `yarn remove` | `bun remove` |
//! | `Run { task }` | `uf run <task>` | `npm run <task>` | `pnpm run <task>` | `yarn run <task>` | `yarn run <task>` | `bun run <task>` |
//! | `Exec` | `uf exec` | `npm exec --` | `pnpm exec` | `yarn run` | `yarn exec` | `bun run` |
//! | `DlxExec` | `uf exec` | `npx --yes` | `pnpm dlx` | `npx --yes` | `yarn dlx` | `bunx` |
//! | `Update` | `uf update` | `npm update` | `pnpm update` | `yarn upgrade` | `yarn up` | `bun update` |
//! | `Why` | `uf why` | `npm explain` | `pnpm why` | `yarn why` | `yarn why` | `bun why` |
//!
//! Callers append their own operands (package names for `Add`/`Remove`/`Why`, the
//! binary and its arguments for `Exec`/`DlxExec`); only `Run` carries its operand
//! in the [`Operation`] because the task name is the whole command.

use std::borrow::Cow;
use std::fmt;

use serde::Serialize;
use smallvec::SmallVec;

use crate::detect::{PackageManager, YarnEdition};

/// Inline argument list; no mapped invocation needs a heap allocation.
pub type InvocationArgs = SmallVec<[Cow<'static, str>; 8]>;

/// Every program `uf` will spawn on behalf of a detected package manager.
///
/// The list is closed on purpose: it is the allowlist that keeps untrusted
/// manifest content out of `argv[0]`.
pub const PROGRAMS: [&str; 7] = ["uf", "npm", "npx", "pnpm", "yarn", "bun", "bunx"];

/// Which of a manifest's dependency maps an added package is recorded in.
///
/// Every manager in the table spells all four, so `uf add --peer` is a real
/// answer everywhere rather than one that works on npm and is dropped on the
/// floor elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyKind {
    /// `dependencies`: needed wherever the package runs.
    Prod,
    /// `devDependencies`: needed to build, test or lint it, not to run it.
    Dev,
    /// `optionalDependencies`: installed when it can be, skipped when it
    /// cannot, and the install still succeeds.
    Optional,
    /// `peerDependencies`: required of whoever depends on this package,
    /// rather than installed underneath it.
    Peer,
}

impl DependencyKind {
    /// Every kind, for exhaustive testing and for an error that lists them.
    pub const ALL: [Self; 4] = [Self::Prod, Self::Dev, Self::Optional, Self::Peer];

    /// The manifest field the manager will write the package into.
    ///
    /// The field rather than the flag, because the flag is the manager's
    /// vocabulary and the field is the one a reader can go and look at.
    #[must_use]
    pub const fn manifest_field(self) -> &'static str {
        match self {
            Self::Prod => "dependencies",
            Self::Dev => "devDependencies",
            Self::Optional => "optionalDependencies",
            Self::Peer => "peerDependencies",
        }
    }
}

/// Package manager operation requested by `uf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation<'a> {
    /// Install every dependency, refreshing the lockfile when it is stale.
    Install,
    /// Install exactly what the lockfile pins and fail when it is stale (CI).
    InstallFrozen,
    /// Add dependencies; the caller appends the package specifiers.
    Add {
        /// The manifest field the packages are recorded in.
        kind: DependencyKind,
    },
    /// Remove dependencies; the caller appends the package names.
    Remove,
    /// Run a project task.
    Run {
        /// Task name, appended as the final argument.
        task: &'a str,
    },
    /// Execute a binary already installed in the project.
    Exec,
    /// Fetch and execute a package that is not installed.
    DlxExec,
    /// Update dependencies within their declared ranges.
    Update,
    /// Explain why a package is present in the dependency tree.
    Why,
    /// List the installed dependency tree.
    List,
    /// Audit the installed tree against the registry's advisories.
    Audit,
    /// Search the registry; the caller appends the terms.
    ///
    /// The first operation the table cannot answer for every manager: yarn and
    /// bun have no search. That is why [`command_for`] returns an [`Option`].
    Search,
    /// Open a dependency for editing; the caller appends the package name.
    ///
    /// pnpm and yarn berry only. npm and yarn classic have nothing like it, and
    /// bun has nothing like it either — see [`crate::run`] for what uf says
    /// instead of substituting somebody else's manager.
    Patch,
    /// Write the patch from the directory [`Self::Patch`] opened, and install.
    PatchCommit,
}

impl Operation<'_> {
    /// Every operation, with a representative payload, for exhaustive testing.
    pub const ALL: [Self; 17] = [
        Self::Install,
        Self::InstallFrozen,
        Self::Add {
            kind: DependencyKind::Prod,
        },
        Self::Add {
            kind: DependencyKind::Dev,
        },
        Self::Add {
            kind: DependencyKind::Optional,
        },
        Self::Add {
            kind: DependencyKind::Peer,
        },
        Self::Remove,
        Self::Run { task: "build" },
        Self::Exec,
        Self::DlxExec,
        Self::Update,
        Self::Why,
        Self::List,
        Self::Audit,
        Self::Search,
        Self::Patch,
        Self::PatchCommit,
    ];

    /// Whether this operation can cause a dependency's install scripts to run.
    ///
    /// Everything that changes what is in `node_modules` can; `Why` reads the
    /// tree and reports on it. The distinction decides which invocations carry
    /// `--ignore-scripts`, and passing that flag to a manager's read-only
    /// query would only be a way to have it rejected as an unknown option.
    #[must_use]
    pub const fn installs_packages(self) -> bool {
        match self {
            Self::Install
            | Self::InstallFrozen
            | Self::Add { .. }
            | Self::Remove
            | Self::Update
            // `patch-commit` writes the patch, records it in the manifest, and
            // reinstalls the package it patched — which is an install, and one
            // whose scripts run against code the project has just edited.
            | Self::PatchCommit => true,
            Self::Run { .. }
            | Self::Exec
            | Self::DlxExec
            | Self::Why
            | Self::List
            | Self::Audit
            | Self::Search
            // `pnpm patch` extracts a copy into a temporary directory and
            // prints the path. Nothing enters `node_modules` until the commit.
            | Self::Patch => false,
        }
    }

    /// The operation's name, for a message about a manager that has no command
    /// for it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Install | Self::InstallFrozen => "install",
            Self::Add { .. } => "add",
            Self::Remove => "remove",
            Self::Run { .. } => "run",
            Self::Exec | Self::DlxExec => "exec",
            Self::Update => "update",
            Self::Why => "why",
            Self::List => "ls",
            Self::Audit => "audit",
            Self::Search => "search",
            Self::Patch => "patch",
            Self::PatchCommit => "patch-commit",
        }
    }
}

/// A concrete process invocation for a package manager operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invocation {
    /// Program to spawn; always an entry of [`PROGRAMS`].
    pub program: &'static str,
    /// Arguments passed to `program`, in order.
    pub args: InvocationArgs,
}

impl fmt::Display for Invocation {
    /// Render the invocation for diagnostics.
    ///
    /// Not shell-quoted, and never safe to hand to a shell.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.program)?;
        for arg in &self.args {
            write!(formatter, " {arg}")?;
        }
        Ok(())
    }
}

/// Map a package manager operation onto the command that performs it, or
/// [`None`] when the manager has no command for it.
///
/// [`None`] is a real answer rather than a gap to fill in later: yarn and bun
/// have no registry search, and the only honest thing uf can do is say so.
/// Falling back to another manager would run a program the project did not
/// choose, which is the one thing a package-manager-agnostic tool must not do.
#[must_use]
pub fn command_for(manager: PackageManager, operation: Operation<'_>) -> Option<Invocation> {
    let spec = command_spec(manager, operation)?;
    let mut args = InvocationArgs::with_capacity(spec.args.len() + 1);
    args.extend(spec.args.iter().copied().map(Cow::Borrowed));

    if let Operation::Run { task } = operation {
        args.push(Cow::Owned(task.to_owned()));
    }

    Some(Invocation {
        program: spec.program,
        args,
    })
}

#[derive(Debug, Clone, Copy)]
struct CommandSpec {
    program: &'static str,
    args: &'static [&'static str],
}

const fn spec(program: &'static str, args: &'static [&'static str]) -> Option<CommandSpec> {
    Some(CommandSpec { program, args })
}

/// The manager has no command for this operation.
///
/// Not a fallback and never a substitution: a package manager that quietly ran
/// a different one is how a lockfile comes to be written by something the
/// project did not choose. The caller is told which manager and which
/// operation, and says so.
const fn unsupported() -> Option<CommandSpec> {
    None
}

fn command_spec(manager: PackageManager, operation: Operation<'_>) -> Option<CommandSpec> {
    match manager {
        PackageManager::Uf => uf_spec(operation),
        PackageManager::Npm => npm_spec(operation),
        PackageManager::Pnpm => pnpm_spec(operation),
        PackageManager::Yarn(YarnEdition::Classic) => yarn_classic_spec(operation),
        PackageManager::Yarn(YarnEdition::Berry) => yarn_berry_spec(operation),
        PackageManager::Bun => bun_spec(operation),
    }
}

/// uf's own contract. `uf install` is lockfile-deterministic either way, and
/// `uf exec` always resolves through the content-addressed store, so `Exec` and
/// `DlxExec` coincide.
const fn uf_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("uf", &["install"]),
        Operation::InstallFrozen => spec("uf", &["install", "--frozen-lockfile"]),
        Operation::Add {
            kind: DependencyKind::Prod,
        } => spec("uf", &["add"]),
        Operation::Add {
            kind: DependencyKind::Dev,
        } => spec("uf", &["add", "--dev"]),
        Operation::Add {
            kind: DependencyKind::Optional,
        } => spec("uf", &["add", "--optional"]),
        Operation::Add {
            kind: DependencyKind::Peer,
        } => spec("uf", &["add", "--peer"]),
        Operation::Remove => spec("uf", &["remove"]),
        Operation::Run { .. } => spec("uf", &["run"]),
        Operation::Exec | Operation::DlxExec => spec("uf", &["exec"]),
        Operation::Update => spec("uf", &["update"]),
        Operation::Why => spec("uf", &["why"]),
        Operation::List => spec("uf", &["ls"]),
        Operation::Audit => spec("uf", &["audit"]),
        Operation::Search => spec("uf", &["search"]),
        Operation::Patch => spec("uf", &["patch"]),
        Operation::PatchCommit => spec("uf", &["patch", "--commit"]),
    }
}

const fn npm_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install
        | Operation::Add {
            kind: DependencyKind::Prod,
        } => spec("npm", &["install"]),
        // `npm ci` is the only npm install that refuses a stale lockfile.
        Operation::InstallFrozen => spec("npm", &["ci"]),
        Operation::Add {
            kind: DependencyKind::Dev,
        } => spec("npm", &["install", "--save-dev"]),
        Operation::Add {
            kind: DependencyKind::Optional,
        } => spec("npm", &["install", "--save-optional"]),
        Operation::Add {
            kind: DependencyKind::Peer,
        } => spec("npm", &["install", "--save-peer"]),
        Operation::Remove => spec("npm", &["uninstall"]),
        Operation::Run { .. } => spec("npm", &["run"]),
        Operation::Exec => spec("npm", &["exec", "--"]),
        // `--yes` keeps npx from opening an interactive install prompt.
        Operation::DlxExec => spec("npx", &["--yes"]),
        Operation::Update => spec("npm", &["update"]),
        Operation::Why => spec("npm", &["explain"]),
        Operation::List => spec("npm", &["ls"]),
        Operation::Audit => spec("npm", &["audit"]),
        Operation::Search => spec("npm", &["search"]),
        // npm has no patch command in any version. `patch-package` is the
        // ecosystem's answer and it is not npm's, so uf refuses rather than
        // reaching for a package the project has not installed.
        Operation::Patch | Operation::PatchCommit => unsupported(),
    }
}

const fn pnpm_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("pnpm", &["install"]),
        Operation::InstallFrozen => spec("pnpm", &["install", "--frozen-lockfile"]),
        Operation::Add {
            kind: DependencyKind::Prod,
        } => spec("pnpm", &["add"]),
        Operation::Add {
            kind: DependencyKind::Dev,
        } => spec("pnpm", &["add", "--save-dev"]),
        Operation::Add {
            kind: DependencyKind::Optional,
        } => spec("pnpm", &["add", "--save-optional"]),
        Operation::Add {
            kind: DependencyKind::Peer,
        } => spec("pnpm", &["add", "--save-peer"]),
        Operation::Remove => spec("pnpm", &["remove"]),
        Operation::Run { .. } => spec("pnpm", &["run"]),
        Operation::Exec => spec("pnpm", &["exec"]),
        Operation::DlxExec => spec("pnpm", &["dlx"]),
        Operation::Update => spec("pnpm", &["update"]),
        Operation::Why => spec("pnpm", &["why"]),
        Operation::List => spec("pnpm", &["list"]),
        Operation::Audit => spec("pnpm", &["audit"]),
        // Added in pnpm 11. An older pnpm answers with its own "unknown
        // command", which names the manager and the version, and is a better
        // message than one uf could write about a version it did not check.
        Operation::Search => spec("pnpm", &["search"]),
        Operation::Patch => spec("pnpm", &["patch"]),
        Operation::PatchCommit => spec("pnpm", &["patch-commit"]),
    }
}

/// Yarn 1.x has neither `exec` nor `dlx`: `yarn run <bin>` runs a project binary
/// and `npx` is the only fetch-and-run available.
const fn yarn_classic_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("yarn", &["install"]),
        Operation::InstallFrozen => spec("yarn", &["install", "--frozen-lockfile"]),
        Operation::Add { kind } => yarn_add(kind),
        Operation::Remove => spec("yarn", &["remove"]),
        Operation::Run { .. } | Operation::Exec => spec("yarn", &["run"]),
        Operation::DlxExec => spec("npx", &["--yes"]),
        Operation::Update => spec("yarn", &["upgrade"]),
        Operation::Why => spec("yarn", &["why"]),
        Operation::List => spec("yarn", &["list"]),
        Operation::Audit => spec("yarn", &["audit"]),
        // Yarn 1 has no registry search, and neither does Yarn 2+.
        Operation::Search => unsupported(),
        // `yarn patch` is Berry's; Yarn 1 never had one.
        Operation::Patch | Operation::PatchCommit => unsupported(),
    }
}

/// Both Yarn editions spell the dependency maps the same way, which is why
/// `Add` is the one row the editions share a function for.
const fn yarn_add(kind: DependencyKind) -> Option<CommandSpec> {
    match kind {
        DependencyKind::Prod => spec("yarn", &["add"]),
        DependencyKind::Dev => spec("yarn", &["add", "--dev"]),
        DependencyKind::Optional => spec("yarn", &["add", "--optional"]),
        DependencyKind::Peer => spec("yarn", &["add", "--peer"]),
    }
}

/// Yarn 2+ renamed the frozen install to `--immutable` and the update to `yarn up`.
const fn yarn_berry_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("yarn", &["install"]),
        Operation::InstallFrozen => spec("yarn", &["install", "--immutable"]),
        Operation::Add { kind } => yarn_add(kind),
        Operation::Remove => spec("yarn", &["remove"]),
        Operation::Run { .. } => spec("yarn", &["run"]),
        Operation::Exec => spec("yarn", &["exec"]),
        Operation::DlxExec => spec("yarn", &["dlx"]),
        Operation::Update => spec("yarn", &["up"]),
        Operation::Why => spec("yarn", &["why"]),
        // `yarn list` is Yarn 1's; Berry replaced it with `yarn info`, whose
        // `--all` is the whole project rather than the current workspace.
        Operation::List => spec("yarn", &["info", "--all"]),
        // Berry keeps the registry commands under `yarn npm`.
        Operation::Audit => spec("yarn", &["npm", "audit"]),
        Operation::Search => unsupported(),
        Operation::Patch => spec("yarn", &["patch"]),
        Operation::PatchCommit => spec("yarn", &["patch-commit"]),
    }
}

const fn bun_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("bun", &["install"]),
        Operation::InstallFrozen => spec("bun", &["install", "--frozen-lockfile"]),
        Operation::Add {
            kind: DependencyKind::Prod,
        } => spec("bun", &["add"]),
        Operation::Add {
            kind: DependencyKind::Dev,
        } => spec("bun", &["add", "--dev"]),
        Operation::Add {
            kind: DependencyKind::Optional,
        } => spec("bun", &["add", "--optional"]),
        Operation::Add {
            kind: DependencyKind::Peer,
        } => spec("bun", &["add", "--peer"]),
        Operation::Remove => spec("bun", &["remove"]),
        Operation::Run { .. } | Operation::Exec => spec("bun", &["run"]),
        Operation::DlxExec => spec("bunx", &[]),
        Operation::Update => spec("bun", &["update"]),
        Operation::Why => spec("bun", &["why"]),
        Operation::List => spec("bun", &["pm", "ls"]),
        Operation::Audit => spec("bun", &["audit"]),
        // Bun has no registry search.
        Operation::Search => unsupported(),
        // And no patch. `bun patch` exists in recent versions but writes a
        // `patches/` entry bun alone reapplies, which is a different contract
        // from pnpm's and yarn's — uf will not present three incompatible
        // things under one name.
        Operation::Patch | Operation::PatchCommit => unsupported(),
    }
}

#[cfg(test)]
mod tests;
