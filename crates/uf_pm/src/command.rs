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
//! | `Add { dev: false }` | `uf add` | `npm install` | `pnpm add` | `yarn add` | `yarn add` | `bun add` |
//! | `Add { dev: true }` | `uf add --dev` | `npm install --save-dev` | `pnpm add --save-dev` | `yarn add --dev` | `yarn add --dev` | `bun add --dev` |
//! | `Remove` | `uf remove` | `npm uninstall` | `pnpm remove` | `yarn remove` | `yarn remove` | `bun remove` |
//! | `Run { task }` | `uf run <task>` | `npm run <task>` | `pnpm run <task>` | `yarn run <task>` | `yarn run <task>` | `bun run <task>` |
//! | `Exec` | `uf exec` | `npm exec --` | `pnpm exec` | `yarn run` | `yarn exec` | `bun run` |
//! | `DlxExec` | `uf exec` | `npx --yes` | `pnpm dlx` | `npx --yes` | `yarn dlx` | `bunx` |
//! | `Update` | `uf upgrade` | `npm update` | `pnpm update` | `yarn upgrade` | `yarn up` | `bun update` |
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

/// Package manager operation requested by `uf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation<'a> {
    /// Install every dependency, refreshing the lockfile when it is stale.
    Install,
    /// Install exactly what the lockfile pins and fail when it is stale (CI).
    InstallFrozen,
    /// Add dependencies; the caller appends the package specifiers.
    Add {
        /// Record the packages as development dependencies.
        dev: bool,
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
}

impl Operation<'_> {
    /// Every operation, with a representative payload, for exhaustive testing.
    pub const ALL: [Self; 10] = [
        Self::Install,
        Self::InstallFrozen,
        Self::Add { dev: false },
        Self::Add { dev: true },
        Self::Remove,
        Self::Run { task: "build" },
        Self::Exec,
        Self::DlxExec,
        Self::Update,
        Self::Why,
    ];
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

/// Map a package manager operation onto the command that performs it.
#[must_use]
pub fn command_for(manager: PackageManager, operation: Operation<'_>) -> Invocation {
    let spec = command_spec(manager, operation);
    let mut args = InvocationArgs::with_capacity(spec.args.len() + 1);
    args.extend(spec.args.iter().copied().map(Cow::Borrowed));

    if let Operation::Run { task } = operation {
        args.push(Cow::Owned(task.to_owned()));
    }

    Invocation {
        program: spec.program,
        args,
    }
}

#[derive(Debug, Clone, Copy)]
struct CommandSpec {
    program: &'static str,
    args: &'static [&'static str],
}

const fn spec(program: &'static str, args: &'static [&'static str]) -> CommandSpec {
    CommandSpec { program, args }
}

fn command_spec(manager: PackageManager, operation: Operation<'_>) -> CommandSpec {
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
const fn uf_spec(operation: Operation<'_>) -> CommandSpec {
    match operation {
        Operation::Install => spec("uf", &["install"]),
        Operation::InstallFrozen => spec("uf", &["install", "--frozen-lockfile"]),
        Operation::Add { dev: false } => spec("uf", &["add"]),
        Operation::Add { dev: true } => spec("uf", &["add", "--dev"]),
        Operation::Remove => spec("uf", &["remove"]),
        Operation::Run { .. } => spec("uf", &["run"]),
        Operation::Exec | Operation::DlxExec => spec("uf", &["exec"]),
        Operation::Update => spec("uf", &["upgrade"]),
        Operation::Why => spec("uf", &["why"]),
    }
}

const fn npm_spec(operation: Operation<'_>) -> CommandSpec {
    match operation {
        Operation::Install | Operation::Add { dev: false } => spec("npm", &["install"]),
        // `npm ci` is the only npm install that refuses a stale lockfile.
        Operation::InstallFrozen => spec("npm", &["ci"]),
        Operation::Add { dev: true } => spec("npm", &["install", "--save-dev"]),
        Operation::Remove => spec("npm", &["uninstall"]),
        Operation::Run { .. } => spec("npm", &["run"]),
        Operation::Exec => spec("npm", &["exec", "--"]),
        // `--yes` keeps npx from opening an interactive install prompt.
        Operation::DlxExec => spec("npx", &["--yes"]),
        Operation::Update => spec("npm", &["update"]),
        Operation::Why => spec("npm", &["explain"]),
    }
}

const fn pnpm_spec(operation: Operation<'_>) -> CommandSpec {
    match operation {
        Operation::Install => spec("pnpm", &["install"]),
        Operation::InstallFrozen => spec("pnpm", &["install", "--frozen-lockfile"]),
        Operation::Add { dev: false } => spec("pnpm", &["add"]),
        Operation::Add { dev: true } => spec("pnpm", &["add", "--save-dev"]),
        Operation::Remove => spec("pnpm", &["remove"]),
        Operation::Run { .. } => spec("pnpm", &["run"]),
        Operation::Exec => spec("pnpm", &["exec"]),
        Operation::DlxExec => spec("pnpm", &["dlx"]),
        Operation::Update => spec("pnpm", &["update"]),
        Operation::Why => spec("pnpm", &["why"]),
    }
}

/// Yarn 1.x has neither `exec` nor `dlx`: `yarn run <bin>` runs a project binary
/// and `npx` is the only fetch-and-run available.
const fn yarn_classic_spec(operation: Operation<'_>) -> CommandSpec {
    match operation {
        Operation::Install => spec("yarn", &["install"]),
        Operation::InstallFrozen => spec("yarn", &["install", "--frozen-lockfile"]),
        Operation::Add { dev: false } => spec("yarn", &["add"]),
        Operation::Add { dev: true } => spec("yarn", &["add", "--dev"]),
        Operation::Remove => spec("yarn", &["remove"]),
        Operation::Run { .. } | Operation::Exec => spec("yarn", &["run"]),
        Operation::DlxExec => spec("npx", &["--yes"]),
        Operation::Update => spec("yarn", &["upgrade"]),
        Operation::Why => spec("yarn", &["why"]),
    }
}

/// Yarn 2+ renamed the frozen install to `--immutable` and the update to `yarn up`.
const fn yarn_berry_spec(operation: Operation<'_>) -> CommandSpec {
    match operation {
        Operation::Install => spec("yarn", &["install"]),
        Operation::InstallFrozen => spec("yarn", &["install", "--immutable"]),
        Operation::Add { dev: false } => spec("yarn", &["add"]),
        Operation::Add { dev: true } => spec("yarn", &["add", "--dev"]),
        Operation::Remove => spec("yarn", &["remove"]),
        Operation::Run { .. } => spec("yarn", &["run"]),
        Operation::Exec => spec("yarn", &["exec"]),
        Operation::DlxExec => spec("yarn", &["dlx"]),
        Operation::Update => spec("yarn", &["up"]),
        Operation::Why => spec("yarn", &["why"]),
    }
}

const fn bun_spec(operation: Operation<'_>) -> CommandSpec {
    match operation {
        Operation::Install => spec("bun", &["install"]),
        Operation::InstallFrozen => spec("bun", &["install", "--frozen-lockfile"]),
        Operation::Add { dev: false } => spec("bun", &["add"]),
        Operation::Add { dev: true } => spec("bun", &["add", "--dev"]),
        Operation::Remove => spec("bun", &["remove"]),
        Operation::Run { .. } | Operation::Exec => spec("bun", &["run"]),
        Operation::DlxExec => spec("bunx", &[]),
        Operation::Update => spec("bun", &["update"]),
        Operation::Why => spec("bun", &["why"]),
    }
}

#[cfg(test)]
mod tests;
