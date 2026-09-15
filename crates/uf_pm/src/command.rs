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
//! | `InstallProd` | `uf install --prod` | `npm install --omit=dev` | `pnpm install --prod` | `yarn install --production` | `yarn workspaces focus --all --production` | `bun install --omit=dev` |
//! | `InstallFrozenProd` | `uf install --frozen-lockfile --prod` | `npm ci --omit=dev` | `pnpm install --frozen-lockfile --prod` | `yarn install --frozen-lockfile --production` | — | `bun install --frozen-lockfile --production` |
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
//! | `Dedupe` | `uf dedupe` | `npm dedupe` | `pnpm dedupe` | — | `yarn dedupe` | — |
//! | `Link { target: Register }` | `uf link` | `npm link` | `pnpm link` | `yarn link` | — | `bun link` |
//! | `Link { target: Package }` | `uf link <name>` | `npm link <name>` | `pnpm link <name>` | `yarn link <name>` | — | `bun link <name>` |
//! | `Link { target: Directory }` | `uf link <dir>` | `npm link <dir>` | `pnpm link <dir>` | — | `yarn link <dir>` | — |
//! | `Info` | `uf info` | `npm view` | `pnpm view` | `yarn info` | `yarn npm info` | `bun info` |
//!
//! A dash is a manager with no such command, which [`command_for`] answers with
//! [`None`] rather than with somebody else's command.
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

/// Environment variables uf sets on the manager's process, beyond what it
/// inherits.
///
/// Empty for almost every invocation. The exception is a setting a manager has
/// no command-line spelling for: Yarn 2+ answers `--ignore-scripts` with
/// "Unsupported option name" on every command, and reads the same decision from
/// `YARN_ENABLE_SCRIPTS`. Both halves are `&'static str` for the reason
/// [`Invocation::program`] is — nothing here is read from a manifest.
pub type InvocationEnv = SmallVec<[(&'static str, &'static str); 1]>;

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

/// What `uf link` was pointed at.
///
/// Three different requests that npm, pnpm and bun happen to spell with one
/// word, and that Yarn's two editions split between them: Yarn 1 links by name
/// and cannot take a path, Yarn 2+ links by path and keeps no registry to name
/// anything in. Keeping the three apart is what lets the table refuse the one a
/// manager does not have instead of handing it a word it will misread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkTarget {
    /// Nothing named: make the package in this directory linkable from other
    /// projects on this machine.
    Register,
    /// A name: link a package that some other directory registered.
    Package,
    /// A path: link that directory into this project.
    Directory,
}

impl LinkTarget {
    /// Every target, for exhaustive testing.
    pub const ALL: [Self; 3] = [Self::Register, Self::Package, Self::Directory];

    /// Which of the three `operand` is.
    ///
    /// A path is written as one — `.`, `..`, or starting with `./`, `../` or
    /// `/` — and everything else is a package name, a scoped `@acme/ui`
    /// included. That is the line npm draws between `npm link ../ui` and
    /// `npm link ui`, drawn once here, so a name that happens to match a
    /// directory beside the project still means the package.
    #[must_use]
    pub fn of(operand: Option<&str>) -> Self {
        match operand {
            None => Self::Register,
            Some(operand) if is_written_as_a_path(operand) => Self::Directory,
            Some(_) => Self::Package,
        }
    }
}

fn is_written_as_a_path(operand: &str) -> bool {
    // A backslash is in no npm package name, so `.\ui` and `..\ui` are paths on
    // every platform; an absolute path is whatever this platform calls one —
    // `C:\work\ui` and `\\server\share\ui` on Windows.
    if operand.starts_with(".\\")
        || operand.starts_with("..\\")
        || std::path::Path::new(operand).is_absolute()
    {
        return true;
    }
    operand == "."
        || operand == ".."
        || ["./", "../", "/"]
            .iter()
            .any(|prefix| operand.starts_with(prefix))
}

/// Package manager operation requested by `uf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation<'a> {
    /// Install every dependency, refreshing the lockfile when it is stale.
    Install,
    /// Install exactly what the lockfile pins and fail when it is stale (CI).
    InstallFrozen,
    /// Install every dependency except the development ones.
    ///
    /// What a server or a production image runs: `devDependencies` stay out of
    /// `node_modules`, and the lockfile still describes all of them.
    InstallProd,
    /// [`Self::InstallProd`], refusing a stale lockfile the way
    /// [`Self::InstallFrozen`] does.
    ///
    /// Yarn 2+ has no such install. Its production install is `yarn workspaces
    /// focus`, which never writes the lockfile and so has nothing to refuse.
    InstallFrozenProd,
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
    /// Collapse the versions the declared ranges allow to be one, and install.
    ///
    /// npm, pnpm and Yarn 2+. Yarn 1 answers that `yarn install` already
    /// dedupes, and bun has no command for it.
    Dedupe,
    /// Link a package that is being developed somewhere else into a project,
    /// or make one linkable; the caller appends the name or the path.
    Link {
        /// What was named, which is what the managers disagree about.
        target: LinkTarget,
    },
    /// Print a package's metadata from the registry; the caller appends the
    /// package and, optionally, one field of it.
    Info,
}

impl Operation<'_> {
    /// Every operation, with a representative payload, for exhaustive testing.
    pub const ALL: [Self; 24] = [
        Self::Install,
        Self::InstallFrozen,
        Self::InstallProd,
        Self::InstallFrozenProd,
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
        Self::Dedupe,
        Self::Link {
            target: LinkTarget::Register,
        },
        Self::Link {
            target: LinkTarget::Package,
        },
        Self::Link {
            target: LinkTarget::Directory,
        },
        Self::Info,
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
            | Self::InstallProd
            | Self::InstallFrozenProd
            | Self::Add { .. }
            | Self::Remove
            | Self::Update
            // `patch-commit` writes the patch, records it in the manifest, and
            // reinstalls the package it patched — which is an install, and one
            // whose scripts run against code the project has just edited.
            | Self::PatchCommit
            // A dedupe is an install of a smaller tree.
            | Self::Dedupe
            // Every manager links by installing: npm reifies the project
            // around the link, and registering a package installs it into the
            // global directory, dependencies and their scripts included.
            | Self::Link { .. } => true,
            Self::Run { .. }
            | Self::Exec
            | Self::DlxExec
            | Self::Why
            | Self::List
            | Self::Audit
            | Self::Search
            // `pnpm patch` extracts a copy into a temporary directory and
            // prints the path. Nothing enters `node_modules` until the commit.
            | Self::Patch
            | Self::Info => false,
        }
    }

    /// The operation's name, for a message about a manager that has no command
    /// for it.
    ///
    /// As the reader would have typed it, flags and all, where the flag is the
    /// difference: "yarn has no `install --frozen-lockfile --prod`" is a
    /// sentence someone can act on, and "yarn has no `install`" is false.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Install | Self::InstallFrozen => "install",
            Self::InstallProd => "install --prod",
            Self::InstallFrozenProd => "install --frozen-lockfile --prod",
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
            Self::Dedupe => "dedupe",
            Self::Link {
                target: LinkTarget::Register,
            } => "link",
            Self::Link {
                target: LinkTarget::Package,
            } => "link <name>",
            Self::Link {
                target: LinkTarget::Directory,
            } => "link <dir>",
            Self::Info => "info",
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
    /// Variables set on the process, in order. See [`InvocationEnv`].
    #[serde(skip_serializing_if = "SmallVec::is_empty")]
    pub env: InvocationEnv,
}

impl fmt::Display for Invocation {
    /// Render the invocation for diagnostics.
    ///
    /// Not shell-quoted, and never safe to hand to a shell. A variable is
    /// written in front of the program the way a reader would set it by hand,
    /// because a `command` row that left it out would describe a command that
    /// fails.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, value) in &self.env {
            write!(formatter, "{name}={value} ")?;
        }
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
        env: InvocationEnv::new(),
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
        Operation::InstallProd => spec("uf", &["install", "--prod"]),
        Operation::InstallFrozenProd => spec("uf", &["install", "--frozen-lockfile", "--prod"]),
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
        Operation::Dedupe => spec("uf", &["dedupe"]),
        Operation::Link { .. } => spec("uf", &["link"]),
        Operation::Info => spec("uf", &["info"]),
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
        // `--omit=dev` rather than `--production`, which npm 9 deprecated and
        // warns about on every run.
        Operation::InstallProd => spec("npm", &["install", "--omit=dev"]),
        Operation::InstallFrozenProd => spec("npm", &["ci", "--omit=dev"]),
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
        Operation::Dedupe => spec("npm", &["dedupe"]),
        // One word for all three: nothing registers the package, a name links
        // a registered one, and a path links the directory.
        Operation::Link { .. } => spec("npm", &["link"]),
        // `npm info` is an alias; `view` is the command's name.
        Operation::Info => spec("npm", &["view"]),
    }
}

const fn pnpm_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("pnpm", &["install"]),
        Operation::InstallFrozen => spec("pnpm", &["install", "--frozen-lockfile"]),
        Operation::InstallProd => spec("pnpm", &["install", "--prod"]),
        Operation::InstallFrozenProd => spec("pnpm", &["install", "--frozen-lockfile", "--prod"]),
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
        Operation::Dedupe => spec("pnpm", &["dedupe"]),
        // pnpm 10 spells the three the way npm does: `pnpm link` registers the
        // package globally, `pnpm link <name>` links a registered one, and
        // `pnpm link <dir>` writes `link:<dir>` into the manifest.
        Operation::Link { .. } => spec("pnpm", &["link"]),
        // pnpm hands `view` to the npm that Node.js ships beside it.
        Operation::Info => spec("pnpm", &["view"]),
    }
}

/// Yarn 1.x has neither `exec` nor `dlx`: `yarn run <bin>` runs a project binary
/// and `npx` is the only fetch-and-run available.
const fn yarn_classic_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("yarn", &["install"]),
        Operation::InstallFrozen => spec("yarn", &["install", "--frozen-lockfile"]),
        Operation::InstallProd => spec("yarn", &["install", "--production"]),
        Operation::InstallFrozenProd => {
            spec("yarn", &["install", "--frozen-lockfile", "--production"])
        }
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
        // `yarn dedupe` exists only to say "The dedupe command isn't necessary.
        // `yarn install` will already dedupe." — and to exit 1 saying it.
        Operation::Dedupe => unsupported(),
        Operation::Link {
            target: LinkTarget::Register | LinkTarget::Package,
        } => spec("yarn", &["link"]),
        // `yarn link` takes a registered name, never a path.
        Operation::Link {
            target: LinkTarget::Directory,
        } => unsupported(),
        Operation::Info => spec("yarn", &["info"]),
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
        // Built into Yarn 4, and the only install Berry has that leaves
        // `devDependencies` out. It installs without persisting the project,
        // which is also why there is no frozen form of it below.
        Operation::InstallProd => spec("yarn", &["workspaces", "focus", "--all", "--production"]),
        Operation::InstallFrozenProd => unsupported(),
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
        Operation::Dedupe => spec("yarn", &["dedupe"]),
        // Berry links by path, into `resolutions`, and keeps no registry of
        // linkable packages for a name to be looked up in.
        Operation::Link {
            target: LinkTarget::Directory,
        } => spec("yarn", &["link"]),
        Operation::Link {
            target: LinkTarget::Register | LinkTarget::Package,
        } => unsupported(),
        Operation::Info => spec("yarn", &["npm", "info"]),
    }
}

const fn bun_spec(operation: Operation<'_>) -> Option<CommandSpec> {
    match operation {
        Operation::Install => spec("bun", &["install"]),
        Operation::InstallFrozen => spec("bun", &["install", "--frozen-lockfile"]),
        // Not `--production`, which freezes the lockfile as well — "lockfile
        // had changes, but lockfile is frozen" is the frozen form's failure, and
        // this one may still bring a stale lockfile up to date.
        Operation::InstallProd => spec("bun", &["install", "--omit=dev"]),
        Operation::InstallFrozenProd => {
            spec("bun", &["install", "--frozen-lockfile", "--production"])
        }
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
        // Nor a dedupe, in any version.
        Operation::Dedupe => unsupported(),
        Operation::Link {
            target: LinkTarget::Register | LinkTarget::Package,
        } => spec("bun", &["link"]),
        // "error: unrecognised dependency format: ../lib" — `bun link` takes a
        // registered name.
        Operation::Link {
            target: LinkTarget::Directory,
        } => unsupported(),
        Operation::Info => spec("bun", &["info"]),
    }
}

#[cfg(test)]
mod tests;
