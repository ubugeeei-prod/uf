//! Actually changing what is installed.
//!
//! [`crate::install_workspace`] records what a workspace declares; it reaches
//! no registry and creates no `node_modules`. Everything a uf project imports —
//! React, Vite, the `@uniflowed/*` packages — has to come from somewhere, and
//! until uf's own resolver can fetch and link a dependency tree, that somewhere
//! is the package manager the project already uses.
//!
//! So `uf install`, `uf add`, `uf remove`, `uf update` and `uf why` each detect
//! the manager, map their [`Operation`] through the table in [`crate::command`],
//! and spawn it. The program name comes from that table and never from a
//! manifest.
//!
//! # Two ways to run one
//!
//! [`run_operation`] lets the child inherit uf's stdio, so its own progress and
//! errors reach the terminal unedited. That is right for pnpm, Yarn and Bun,
//! which all draw a good install, and it is the only way the four commands that
//! carry operands run at all.
//!
//! [`run_install_watched`] reads the child's output instead, so uf can draw
//! the phases as they happen — see [`crate::progress`] for what that costs and
//! what it refuses to swallow. It is used only for a manager
//! [`Reader::for_manager`] recognises, and even then every line the manager
//! would have printed on its own is handed straight back to the caller through
//! [`InstallObserver::line`]. A failed install still says why, in npm's words,
//! because those words arrive on that callback like any other.
//!
//! # Operands
//!
//! A package specifier is the one part of these invocations that comes from
//! outside the table. It is passed as a single `argv` entry and never through a
//! shell, so `uf add "react@^18 || ^19"` is one argument and stays one — but an
//! operand that *starts* with `-` would be read by the manager as a flag it was
//! never asked for, so [`operands_for`] refuses it before the spawn rather than
//! letting `uf remove --global` mean something.
//!
//! # Why a detected manager and not uf's own
//!
//! [`PackageManager::Uf`] is what detection reports when a project shows no
//! evidence of any manager. Spawning `uf install` for it would be a loop, and
//! uf's resolver cannot fetch yet, so that case falls back to npm — present
//! wherever Node.js is, which a uf project needs regardless. The report says
//! which manager ran and why, because "uf installed your dependencies" is not
//! true and should not be printed.

use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use camino::{Utf8Path, Utf8PathBuf};

use crate::command::{Invocation, LinkTarget, Operation, command_for};
use crate::detect::{
    Detection, DetectionSource, PackageManager, YarnEdition, declares_workspaces,
    detect_package_manager, is_pnpm_workspace_root,
};
use crate::progress::{InstallWatch, ManagerEvent, Reader};

/// How often a watched install reports progress when the manager is silent.
///
/// Not a frame rate — the caller decides that — but the granularity at which
/// "the registry has gone quiet" can be noticed at all.
const POLL: Duration = Duration::from_millis(50);

/// Longest line uf keeps whole from a manager's output.
///
/// A line arrives with registry-controlled content in it, and an answer that
/// is one enormous line must not become one enormous allocation. Past this the
/// remainder is read as the next line, which is bounded and readable rather
/// than unbounded and tidy.
const MAX_LINE: u64 = 8 * 1024;

/// Which of the manager's streams a line arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagerStream {
    /// The manager's stdout: its summary, and what it thinks the user asked
    /// for.
    Stdout,
    /// The manager's stderr: its warnings, its errors, and its progress.
    Stderr,
}

/// What a caller does with a manager's output while it is still running.
pub trait InstallObserver {
    /// The manager printed a line of its own; print it.
    ///
    /// Called for every line uf did not ask for, in the order it arrived on
    /// that stream. Nothing else in uf will print it.
    fn line(&mut self, stream: ManagerStream, text: &str);

    /// The ladder moved, or time passed.
    ///
    /// Called after every line and at least every [`POLL`], so an
    /// implementation that draws must decide for itself when a frame is due.
    fn progress(&mut self, watch: &InstallWatch);
}

/// What a delegated package-manager command did.
#[derive(Debug, Clone)]
pub struct ManagerRun {
    /// The manager that ran.
    pub manager: PackageManager,
    /// The command it ran, for display. Never shell syntax.
    pub invocation: Invocation,
    /// How the manager was chosen.
    pub source: DetectionSource,
    /// Whether uf substituted npm because detection found no real manager.
    pub substituted: bool,
    /// Directory the command ran in.
    pub root: Utf8PathBuf,
    /// The phase ladder, when uf read the manager's output as it ran.
    ///
    /// `None` from [`run_install`], which never reads it.
    pub watch: Option<InstallWatch>,
}

/// Running the manager failed, or uf refused to run it.
#[derive(Debug, thiserror::Error)]
pub enum ManagerRunError {
    /// The manager could not be started at all.
    #[error("could not run `{invocation}`: {source}\n{hint}")]
    Spawn {
        /// The command uf tried to run.
        invocation: String,
        /// Why the spawn failed.
        #[source]
        source: std::io::Error,
        /// What the user can do about it.
        hint: String,
    },
    /// The manager ran and reported failure. Its own output is already on the
    /// terminal, so this carries the status and nothing else.
    #[error("`{invocation}` exited with {status}")]
    Failed {
        /// The command that ran.
        invocation: String,
        /// How it described its failure.
        status: String,
    },
    /// An operand uf will not put on a manager's command line.
    #[error("{operand:?} is not a package name: {reason}")]
    Operand {
        /// The operand as it was given.
        operand: String,
        /// Why it was refused, and what to write instead.
        reason: String,
    },
    /// The project's manager has no command for what was asked.
    ///
    /// Reported rather than worked around. uf could run a different manager's
    /// equivalent, and that is exactly the substitution a project chose its
    /// manager to avoid — the replacement would resolve against its own
    /// registry configuration and could write its own lockfile.
    #[error("{manager} has no `{operation}`: {hint}")]
    Unsupported {
        /// The manager the project uses.
        manager: String,
        /// The uf operation it cannot perform.
        operation: &'static str,
        /// What to do instead.
        hint: String,
    },
}

/// Install `root`'s dependencies with the package manager that drives it.
///
/// [`run_operation`] with [`Operation::Install`] and no operands.
///
/// # Errors
///
/// The same as [`run_operation`].
pub fn run_install(root: &Utf8Path, allow_scripts: bool) -> Result<ManagerRun, ManagerRunError> {
    run_operation(root, Operation::Install, &[], allow_scripts)
}

/// Run one package-manager operation in `root`, with the manager that drives it.
///
/// `operands` are the package specifiers or names the operation takes — the
/// specifiers for [`Operation::Add`], the names for [`Operation::Remove`] and
/// [`Operation::Why`], the packages to hold to for [`Operation::Update`], and
/// nothing at all for the two installs.
///
/// `allow_scripts` is the project's `pm.allowLifecycleScripts`; when it is
/// false the manager is told not to run any, which is the only way to keep
/// that guarantee once the work is somebody else's process. It is passed only
/// for the operations that can install something — see
/// [`Operation::installs_packages`].
///
/// Blocks until the manager exits, with the child's stdio connected to uf's, so
/// the caller must have finished any progress rendering of its own first.
///
/// # Errors
///
/// [`ManagerRunError::Operand`] before anything is spawned when an operand
/// could be read as a flag, [`ManagerRunError::Spawn`] when the manager is not
/// installed, and [`ManagerRunError::Failed`] when it runs and fails.
pub fn run_operation(
    root: &Utf8Path,
    operation: Operation<'_>,
    operands: &[String],
    allow_scripts: bool,
) -> Result<ManagerRun, ManagerRunError> {
    let detection = detect_package_manager(root);
    run_operation_with_detection(root, &detection, operation, operands, allow_scripts, &[])
}

/// `PATH` with `prefix` in front of what this process inherited, or `None`
/// when there is nothing to put there and the child should inherit `PATH`
/// untouched.
///
/// Set on the manager's command rather than resolved into an absolute program
/// path, and that is deliberate: [`Invocation::program`] is a name from a fixed
/// table, which is what keeps a hostile manifest from choosing what uf starts.
/// A bare name is looked up in the child's `PATH` once one is set, so the
/// manager uf installed is the one found — and npm, pnpm and Yarn are Node
/// programs, so the runtime directory beside it in `prefix` is the Node they
/// start on.
pub(crate) fn prefixed_path(prefix: &[camino::Utf8PathBuf]) -> Option<std::ffi::OsString> {
    if prefix.is_empty() {
        return None;
    }
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(
        prefix
            .iter()
            .map(|directory| directory.as_std_path().to_path_buf())
            .chain(std::env::split_paths(&inherited)),
    )
    .ok()
}

/// Run one package-manager operation with a detection the caller already made.
///
/// Use this when the caller loaded `uf.config.js` and therefore knows about
/// `packageManager`; re-detecting from files alone would ignore that override
/// and could report or run a different manager from the one the command
/// preflighted.
///
/// `path` is the directories to put in front of `PATH` for the manager's
/// process — the release of it `packageManager` pins, and the runtime it runs
/// on, both from uf's store — or nothing, for the manager already on `PATH`.
pub fn run_operation_with_detection(
    root: &Utf8Path,
    detection: &Detection,
    operation: Operation<'_>,
    operands: &[String],
    allow_scripts: bool,
    path: &[camino::Utf8PathBuf],
) -> Result<ManagerRun, ManagerRunError> {
    let (manager, substituted) = installable(detection);
    let invocation = invocation_for(root, manager, operation, operands, allow_scripts)?;

    let path = prefixed_path(path);
    let mut command = Command::new(program_to_spawn(invocation.program, path.as_deref()));
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.envs(invocation.env.iter().copied());
    let status = command
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(root)
        .status()
        .map_err(|source| ManagerRunError::Spawn {
            invocation: invocation.to_string(),
            source,
            hint: missing_hint(manager),
        })?;

    if !status.success() {
        return Err(ManagerRunError::Failed {
            invocation: invocation.to_string(),
            status: status.to_string(),
        });
    }

    Ok(ManagerRun {
        manager,
        invocation,
        source: detection.source.clone(),
        substituted,
        root: root.to_path_buf(),
        watch: None,
    })
}

/// What a manager printed, and what it exited with.
///
/// For the commands whose answer is what the manager *said* rather than what it
/// did. See [`run_captured_with_detection`].
#[derive(Debug, Clone)]
pub struct CapturedRun {
    /// The manager that ran.
    pub manager: PackageManager,
    /// The command it ran, for display. Never shell syntax.
    pub invocation: Invocation,
    /// Whether it exited 0.
    pub succeeded: bool,
    /// Its standard output, as far as it is text.
    pub stdout: String,
    /// Its standard error, as far as it is text.
    pub stderr: String,
}

/// Run one operation with its output captured rather than shown, and give the
/// caller what it printed whether it succeeded or failed.
///
/// [`run_operation_with_detection`] lets the manager write straight to the
/// screen and turns a non-zero exit into [`ManagerRunError::Failed`], which is
/// what every command that *does* something wants. A check is the other shape:
/// `uf dedupe --check` asks pnpm and Yarn 2+ a question they answer with an
/// exit code, and npm one it answers in JSON on stdout while exiting 0 either
/// way. Neither answer is a failure, and neither is uf's report — so the
/// manager's own output is held back and the caller prints what it means.
///
/// # Errors
///
/// [`ManagerRunError::Operand`] and [`ManagerRunError::Unsupported`] as
/// [`run_operation_with_detection`] gives them, and [`ManagerRunError::Spawn`]
/// when the manager is not installed. Never [`ManagerRunError::Failed`]: a
/// non-zero exit is returned as `succeeded: false`.
pub fn run_captured_with_detection(
    root: &Utf8Path,
    detection: &Detection,
    operation: Operation<'_>,
    operands: &[String],
    allow_scripts: bool,
    path: &[camino::Utf8PathBuf],
) -> Result<CapturedRun, ManagerRunError> {
    let (manager, _) = installable(detection);
    let invocation = invocation_for(root, manager, operation, operands, allow_scripts)?;

    let path = prefixed_path(path);
    let mut command = Command::new(program_to_spawn(invocation.program, path.as_deref()));
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.envs(invocation.env.iter().copied());
    let output = command
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|source| ManagerRunError::Spawn {
            invocation: invocation.to_string(),
            source,
            hint: missing_hint(manager),
        })?;

    Ok(CapturedRun {
        manager,
        succeeded: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        invocation,
    })
}

/// What to do about an operation the project's manager does not have.
///
/// One sentence per operation, naming the managers that do have it — the
/// reader's next question is always "then what", and "your package manager
/// cannot" is only half an answer.
fn unsupported_hint(manager: PackageManager, operation: Operation<'_>) -> String {
    let classic = manager == PackageManager::Yarn(YarnEdition::Classic);
    match operation {
        Operation::Search => {
            "npm and pnpm can search the registry; `uf exec --yes npm search` runs npm's \
             without changing what this project installs with"
        }
        Operation::Patch | Operation::PatchCommit => {
            "pnpm and yarn 2+ can patch a dependency; on the others the ecosystem's answer is \
             `patch-package`, which uf does not install for you"
        }
        Operation::Dedupe { check: true } if classic => {
            "yarn 1 dedupes the tree on every install, so there is never anything for a check to \
             find: `uf install --frozen-lockfile` fails when the lockfile is out of date, which \
             is the check CI wants here"
        }
        Operation::Dedupe { .. } if classic => {
            "yarn 1 dedupes the tree on every install, so `uf install` is the dedupe"
        }
        Operation::Dedupe { check: true } => {
            "npm, pnpm and yarn 2+ can check; bun has no dedupe to check against"
        }
        Operation::Dedupe { .. } => {
            "npm, pnpm and yarn 2+ have one; `uf update` re-resolves every range to the newest \
             version it allows, which is what collapses the duplicates bun leaves"
        }
        Operation::Link {
            target: LinkTarget::Directory,
        } if classic => {
            "yarn 1 links by name: run `uf link` in that directory to register it, then \
             `uf link <its name>` here — or `uf add link:<dir>` to record the link in the manifest"
        }
        Operation::Link {
            target: LinkTarget::Directory,
        } => {
            "bun links by name: run `uf link` in that directory to register it, then \
             `uf link <its name>` here"
        }
        Operation::Link { .. } => {
            "yarn 2+ links by path and keeps no registry of linkable packages: run \
             `uf link <path to the package>` in the project that uses it"
        }
        Operation::Unlink {
            target: LinkTarget::Register,
        } => {
            "yarn 2+ keeps no registry of linkable packages, so nothing is registered to remove: \
             run `uf unlink <name or path>` in the project that links the package"
        }
        Operation::Unlink {
            target: LinkTarget::Directory,
        } => {
            "it unlinks by name, and `uf unlink <dir>` gives it the name that directory's \
             package.json gives"
        }
        Operation::Unlink { .. } => {
            "bun has no `unlink <name>` (it answers \"not implemented yet\"), so `uf unlink` \
             removes the link from node_modules itself"
        }
        Operation::InstallFrozenProd => {
            "yarn 2+ installs production dependencies with `yarn workspaces focus`, which never \
             writes the lockfile and so cannot refuse a stale one; run `uf install \
             --frozen-lockfile` to check the lockfile, then `uf install --prod`"
        }
        _ => "no package manager uf knows spells this one differently",
    }
    .to_owned()
}

/// Whether `manager` can run `operation` for chosen members of a workspace.
///
/// `uf add --filter`, `uf remove --filter` and `uf update --filter` run the
/// manager once in each member's directory, which every manager reads as "this
/// member" — except in one place. Yarn 2+'s `yarn up` moves a package in every
/// workspace that declares it, wherever it is run from, so a `--filter` would
/// be a scope the command silently ignored.
///
/// # Errors
///
/// [`ManagerRunError::Unsupported`], naming what to run instead.
pub fn check_member_operation(
    manager: PackageManager,
    operation: Operation<'_>,
) -> Result<(), ManagerRunError> {
    if manager == PackageManager::Yarn(YarnEdition::Berry) && operation == Operation::Update {
        return Err(ManagerRunError::Unsupported {
            manager: manager.to_string(),
            operation: "update --filter",
            hint: "`yarn up` moves a package in every workspace that declares it, wherever it \
                   runs; run `uf update` without --filter, or `uf add <package>@<range> \
                   --filter <member>` to move one member"
                .to_owned(),
        });
    }
    Ok(())
}

/// The sentence a manager's own failure usually needs, where uf knows one.
///
/// Yarn 2 and 3 have `yarn workspaces focus` only once the workspace-tools
/// plugin is imported; Yarn 4 includes it. A project's Yarn release and its
/// plugins are not something the table can see, and refusing every Yarn 2+
/// project would refuse the ones that have the plugin, so uf runs the command
/// and, when it fails, says what is most likely missing.
#[must_use]
pub fn failure_hint(manager: PackageManager, operation: Operation<'_>) -> Option<&'static str> {
    match (manager, operation) {
        (PackageManager::Yarn(YarnEdition::Berry), Operation::InstallProd) => Some(
            "on Yarn 2 and 3, `yarn workspaces focus` needs the workspace-tools plugin: run \
             `yarn plugin import workspace-tools`, then `uf install --prod` again — Yarn 4 \
             includes it",
        ),
        // pnpm 10 registers a package and links one by name; pnpm 12 answers
        // both with `ERR_PNPM_LINK_BAD_PARAMS`. Which pnpm a project runs is
        // not something the table can see either, so uf runs the command and,
        // when it fails, says what the refusal means and what to run instead.
        (
            PackageManager::Pnpm,
            Operation::Link {
                target: LinkTarget::Register | LinkTarget::Package,
            },
        ) => Some(
            "pnpm 12 links by path only and keeps no registry of linkable packages: run \
             `uf link <path to the package>` in the project that uses it",
        ),
        _ => None,
    }
}

/// Tell `manager` not to run dependency scripts during `operation`.
///
/// `--ignore-scripts` is the spelling almost everywhere, and the two places it
/// is not are both refusals rather than warnings, which is why they are here:
///
/// * **Yarn 2+** has no such flag on any command — `yarn install
///   --ignore-scripts` is "Unsupported option name" and exit 1. Its setting is
///   `enableScripts`, and Yarn reads every setting from `YARN_*` as well as
///   from `.yarnrc.yml`, so the variable is the whole-command answer.
/// * **`pnpm link`** does not declare `--ignore-scripts`, and pnpm refuses an
///   option a command does not declare. `--config.<key>` is pnpm's own way to
///   set any setting on any command.
fn refuse_scripts(invocation: &mut Invocation, manager: PackageManager, operation: Operation<'_>) {
    match (manager, operation) {
        (PackageManager::Yarn(YarnEdition::Berry), _) => {
            invocation.env.push(("YARN_ENABLE_SCRIPTS", "false"));
        }
        // `pnpm remove --ignore-scripts` is the same "Unknown option", and
        // `pnpm patch-commit` does not declare the flag either.
        (
            PackageManager::Pnpm,
            Operation::Link { .. }
            | Operation::Unlink { .. }
            | Operation::Remove
            | Operation::PatchCommit,
        ) => invocation
            .args
            .push(std::borrow::Cow::Borrowed("--config.ignore-scripts=true")),
        _ => invocation
            .args
            .push(std::borrow::Cow::Borrowed("--ignore-scripts")),
    }
}

/// The flag that says "yes, the workspace root is what I meant".
///
/// pnpm refuses `pnpm add` in a workspace root unless the root is named
/// explicitly: `ERR_PNPM_ADDING_TO_ROOT`, whose advice is to run the same
/// command again with `-w`. That advice cannot be followed through uf, because
/// `-w` is a flag uf never passed — so `uf add --dev @uniflowed/test` in a
/// pnpm monorepo failed, and told you to run the thing that had just failed
/// (ubugeeei-prod/uf#484).
///
/// uf passes it, and only where pnpm's own check is:
///
/// * **pnpm only.** npm, Yarn and Bun add to the root of a workspace without
///   asking, and `--workspace-root` is not a flag any of them has.
/// * **`add` only.** The check lives in pnpm's `add` handler and nowhere else;
///   `pnpm remove` and `pnpm update` at a root are not refused. A flag pushed
///   onto a command that does not need it is a flag that can only be wrong
///   later.
/// * **Only when `root` is the workspace root itself.** That is the whole
///   reason this is safe to do without asking. pnpm's check exists because a
///   shell can be in the root by accident; uf's `root` is the nearest ancestor
///   of the caller's `--cwd` holding a `package.json`, a uf config or a `.git`
///   (`uf_config::discover_root`), so `uf add` inside a member resolves to the
///   member, [`is_pnpm_workspace_root`] is false there, and the flag is not
///   passed. When it *is* passed, the directory the user pointed uf at and the
///   directory the dependency lands in are the same one.
///
/// It also makes uf's own report true: `delegate` diffs `root/package.json`
/// either way, so the run that pnpm refused was the only one where the
/// manifest uf reads and the manifest the manager writes could disagree.
///
/// Yarn 1 has the same check under a different name, and on `remove` as well
/// as `add` — both answer "Running this command will add the dependency to the
/// workspace root rather than the workspace itself" and ask for `-W`. Its
/// `upgrade` does not. So Yarn 1 is told `--ignore-workspace-root-check` on
/// exactly those two, and only where its own marker for a root is: a
/// `package.json` that declares `workspaces`.
fn workspace_root_argument(
    root: &Utf8Path,
    manager: PackageManager,
    operation: Operation<'_>,
) -> Option<&'static str> {
    match (manager, operation) {
        (PackageManager::Pnpm, Operation::Add { .. }) => {
            is_pnpm_workspace_root(root).then_some("--workspace-root")
        }
        (PackageManager::Yarn(YarnEdition::Classic), Operation::Add { .. } | Operation::Remove) => {
            declares_workspaces(root).then_some("--ignore-workspace-root-check")
        }
        _ => None,
    }
}

/// The exact command `run_operation` would spawn in `root`.
///
/// Separate from the spawn so that what uf is about to run can be asserted on,
/// and printed — `uf why` and `uf patch` name it — without running anything.
///
/// `root` is a parameter rather than something the caller adds afterwards
/// because one of uf's additions depends on it: see
/// [`workspace_root_argument`]. A command rendered without the root would be a
/// command that differs from the one that runs, on exactly the projects where
/// the difference decides whether it runs at all.
///
/// # Errors
///
/// [`ManagerRunError::Operand`] for an operand uf will not pass on.
pub fn invocation_for(
    root: &Utf8Path,
    manager: PackageManager,
    operation: Operation<'_>,
    operands: &[String],
    allow_scripts: bool,
) -> Result<Invocation, ManagerRunError> {
    let mut invocation =
        command_for(manager, operation).ok_or_else(|| ManagerRunError::Unsupported {
            manager: manager.to_string(),
            operation: operation.name(),
            hint: unsupported_hint(manager, operation),
        })?;

    // Which project, before how to install it: a reader checking the `command`
    // row wants the scope of the operation first.
    if let Some(flag) = workspace_root_argument(root, manager, operation) {
        invocation.args.push(std::borrow::Cow::Borrowed(flag));
    }

    // A dependency's `postinstall` is the supply-chain hole uf's own resolver
    // was going to close by never running one. Delegating to a manager that
    // runs them by default would have quietly reopened it, so the project's
    // `pm.allowLifecycleScripts` is passed through to the manager, in the
    // spelling that manager accepts for this command — see `refuse_scripts`.
    //
    // Before the operands rather than after: a flag that follows a package
    // name is still a flag to all four managers, but a reader checking the
    // `command` row against what they typed should see uf's own additions
    // together and their own specifiers last.
    if !allow_scripts && operation.installs_packages() {
        refuse_scripts(&mut invocation, manager, operation);
    }
    check_operands(operands)?;
    // `uf info <package> <field>`: the field is a second positional to every
    // manager but Yarn 2+, whose `yarn npm info` takes it as `--fields`. The
    // operand still comes last, so what was typed is still the end of the line.
    let fields_flag = manager == PackageManager::Yarn(YarnEdition::Berry)
        && operation == Operation::Info
        && operands.len() > 1;
    for (index, operand) in operands.iter().enumerate() {
        if fields_flag && index == 1 {
            invocation.args.push(std::borrow::Cow::Borrowed("--fields"));
        }
        invocation
            .args
            .push(std::borrow::Cow::Owned(operand.clone()));
    }
    Ok(invocation)
}

/// Refuse an operand uf will not put on a manager's command line.
///
/// Nothing here is quoted or split: a specifier is one `argv` entry and stays
/// one, which is what makes `uf add "react@>=18 <20"` mean what it says. What
/// is refused is the operand that would stop being an operand — an empty
/// string, which every manager reads as a package with no name, and anything
/// starting with `-`, which it would read as a flag.
///
/// Public, and separate from [`invocation_for`], so a caller can refuse before
/// it has done anything at all: `uf add -- --global` used to rewrite `uf.lock`
/// on its way to failing, which is a command that both refused and wrote.
///
/// # Errors
///
/// [`ManagerRunError::Operand`], naming the operand and what to write instead.
pub fn check_operands(operands: &[String]) -> Result<(), ManagerRunError> {
    for operand in operands {
        if operand.is_empty() {
            return Err(ManagerRunError::Operand {
                operand: operand.clone(),
                reason: "it is empty; name the package you meant".to_owned(),
            });
        }
        if operand.starts_with('-') {
            return Err(ManagerRunError::Operand {
                operand: operand.clone(),
                reason: format!(
                    "it starts with `-`, which the package manager would read as a flag; \
                     write the package name, or `./{operand}` for a path"
                ),
            });
        }
    }
    Ok(())
}

/// Install `root`'s dependencies, reading the manager's output as it goes.
///
/// [`run_watched`] with [`Operation::Install`].
///
/// # Errors
///
/// The same as [`run_watched`].
pub fn run_install_watched(
    root: &Utf8Path,
    allow_scripts: bool,
    observer: &mut dyn InstallObserver,
) -> Result<ManagerRun, ManagerRunError> {
    run_watched(root, Operation::Install, allow_scripts, observer)
}

/// Run an install-shaped `operation` in `root`, reading the manager's output as
/// it goes.
///
/// Same contract as [`run_operation`] — same manager, same detection, same
/// `--ignore-scripts` — with the child's streams piped instead of inherited so
/// that `observer` sees them. It takes no operands, because the two operations
/// worth narrating a ladder for are the two installs and neither has any.
///
/// When the detected manager is not one [`Reader::for_manager`] knows how to
/// read, this falls back to [`run_operation`] rather than piping a manager uf
/// cannot narrate: taking a good install screen away and replacing it with a
/// spinner is not an improvement.
///
/// # Errors
///
/// The same as [`run_operation`], and for the same reasons.
pub fn run_watched(
    root: &Utf8Path,
    operation: Operation<'_>,
    allow_scripts: bool,
    observer: &mut dyn InstallObserver,
) -> Result<ManagerRun, ManagerRunError> {
    let detection = detect_package_manager(root);
    run_watched_with_detection(root, &detection, operation, allow_scripts, observer, &[])
}

/// Run an install-shaped operation with a detection the caller already made.
pub fn run_watched_with_detection(
    root: &Utf8Path,
    detection: &Detection,
    operation: Operation<'_>,
    allow_scripts: bool,
    observer: &mut dyn InstallObserver,
    path: &[camino::Utf8PathBuf],
) -> Result<ManagerRun, ManagerRunError> {
    let (manager, substituted) = installable(detection);
    let Some(reader) = Reader::for_manager(manager) else {
        return run_operation_with_detection(root, detection, operation, &[], allow_scripts, path);
    };
    let mut invocation = invocation_for(root, manager, operation, &[], allow_scripts)?;
    // Asked for so that there is something to narrate: npm prints nothing at
    // all between "starting" and "done" when its output is a pipe. Every line
    // this flag causes is consumed by `Reader::classify` and no other, so the
    // flag changes what uf can see and not what the reader is shown.
    invocation
        .args
        .push(std::borrow::Cow::Borrowed(reader.verbosity_argument()));

    let path = prefixed_path(path);
    let mut command = Command::new(program_to_spawn(invocation.program, path.as_deref()));
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.envs(invocation.env.iter().copied());
    let mut child = command
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| ManagerRunError::Spawn {
            invocation: invocation.to_string(),
            source,
            hint: missing_hint(manager),
        })?;

    // Two pipes and one blocking read each: a single-threaded reader that
    // drained stdout first would deadlock the moment npm filled its stderr
    // pipe, which for a `--loglevel=http` install is immediately.
    let (sender, receiver) = mpsc::channel();
    let pumps = [
        child
            .stdout
            .take()
            .map(|stream| pump(stream, ManagerStream::Stdout, sender.clone())),
        child
            .stderr
            .take()
            .map(|stream| pump(stream, ManagerStream::Stderr, sender.clone())),
    ];
    // The loop below ends when every sender is gone, so uf's own must be.
    drop(sender);

    let mut watch = InstallWatch::start(Instant::now());
    loop {
        match receiver.recv_timeout(POLL) {
            Ok((stream, line)) => match reader.classify(&line) {
                ManagerEvent::Passthrough => observer.line(stream, &line),
                event => watch.observe(&event, Instant::now()),
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        watch.idle(Instant::now());
        observer.progress(&watch);
    }
    for pump in pumps.into_iter().flatten() {
        let _ = pump.join();
    }
    watch.finish(Instant::now());

    let status = child.wait().map_err(|source| ManagerRunError::Spawn {
        invocation: invocation.to_string(),
        source,
        hint: missing_hint(manager),
    })?;
    if !status.success() {
        return Err(ManagerRunError::Failed {
            invocation: invocation.to_string(),
            status: status.to_string(),
        });
    }

    Ok(ManagerRun {
        manager,
        invocation,
        source: detection.source.clone(),
        substituted,
        root: root.to_path_buf(),
        watch: Some(watch),
    })
}

/// Read `stream` line by line onto `sink` until it closes.
fn pump<R: Read + Send + 'static>(
    stream: R,
    which: ManagerStream,
    sink: Sender<(ManagerStream, String)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut raw = Vec::new();
        loop {
            raw.clear();
            match reader.by_ref().take(MAX_LINE).read_until(b'\n', &mut raw) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            while matches!(raw.last(), Some(b'\n' | b'\r')) {
                raw.pop();
            }
            // Lossy on purpose: a manager that writes bytes which are not
            // UTF-8 has still said something, and refusing to show it is worse
            // than showing it with a replacement character in it.
            if sink
                .send((which, String::from_utf8_lossy(&raw).into_owned()))
                .is_err()
            {
                return;
            }
        }
    })
}

fn program_to_spawn(program: &str, path: Option<&OsStr>) -> OsString {
    #[cfg(windows)]
    {
        windows_program_to_spawn(program, path).unwrap_or_else(|| OsString::from(program))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        OsString::from(program)
    }
}

#[cfg(windows)]
fn windows_program_to_spawn(program: &str, path: Option<&OsStr>) -> Option<OsString> {
    if program.contains('/') || program.contains('\\') {
        return None;
    }
    let search = path
        .map(OsString::from)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let extensions =
        std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
    windows_program_in_path(program, &search, &extensions)
}

#[cfg(any(windows, test))]
fn windows_program_in_path(program: &str, path: &OsStr, pathext: &OsStr) -> Option<OsString> {
    let has_extension = std::path::PathBuf::from(program).extension().is_some();
    for directory in std::env::split_paths(path) {
        let candidate = directory.join(program);
        if candidate.is_file() {
            return Some(candidate.into_os_string());
        }
        if has_extension {
            continue;
        }
        for extension in pathext.to_string_lossy().split(';') {
            if extension.is_empty() {
                continue;
            }
            let candidate = directory.join(format!("{program}{extension}"));
            if candidate.is_file() {
                return Some(candidate.into_os_string());
            }
        }
    }
    None
}

/// The manager to actually spawn, and whether it was substituted.
///
/// Detection reports [`PackageManager::Uf`] both when a project pins uf and
/// when it shows no evidence at all. Neither can install anything today, so
/// both become npm.
///
/// Public because a caller has to know which lockfile the install is about to
/// rewrite *before* it runs: reading it afterwards and calling that the
/// "before" state would report an empty delta for every install.
#[must_use]
pub fn installable(detection: &Detection) -> (PackageManager, bool) {
    match detection.package_manager {
        PackageManager::Uf => (PackageManager::Npm, true),
        other => (other, false),
    }
}

fn missing_hint(manager: PackageManager) -> String {
    match manager {
        PackageManager::Npm => "npm comes with Node.js; install Node.js and try again".to_owned(),
        other => format!(
            "this project is pinned to {other}; install it, or change the lockfile and `packageManager` field to a manager you have"
        ),
    }
}

#[cfg(test)]
mod tests;
