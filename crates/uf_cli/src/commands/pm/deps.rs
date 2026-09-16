//! `uf add`, `uf remove`, `uf update`, `uf dedupe`, `uf link`, `uf info` and
//! `uf why`: one dependency at a time.
//!
//! `uf install` is the whole tree; these are the things a person does to it
//! between installs. All of them are the project's own package manager, for
//! the reason [`uf_pm::run`] gives at length: uf's resolver reaches no
//! registry, so an `uf add` that claimed to have fetched `date-fns` would be
//! lying about the one moment where lying matters — installing a new package is
//! exactly when a `postinstall` script arrives.
//!
//! Delegating is a complete answer, and it is a *better* answer than dropping
//! out to npm by hand, because everything uf knows about the project stays in
//! force: `pm.allowLifecycleScripts` becomes `--ignore-scripts` on the child,
//! and a manifest that declares install-time lifecycle scripts of its own is
//! refused before anything is fetched. Native uf projects also rewrite `uf.lock` and the
//! content-addressed store; delegated npm, pnpm, Yarn and Bun projects keep the
//! lockfile they already use.
//!
//! # What each one reports
//!
//! Which manager ran, what named it, and the exact command — the same three
//! rows `uf install` prints, and for the same reason: a delegating command that
//! does not say who it delegated to is a black box (`docs/red-lines.md`, line
//! 7).
//!
//! Then the two files that changed. The **manifest** section is read back out
//! of `package.json`, so `uf add --peer react` is answered with the field the
//! package actually landed in rather than with the flag that was asked for. The
//! **dependency tree** section is [`uf_pm::delta`] over the manager's lockfile,
//! the same block `uf install` draws. A run that moved neither says so in one
//! line and stops.
//!
//! `uf why` writes nothing, so it reports the three rows *before* the manager
//! runs and nothing after: the manager's explanation is the answer, and the
//! last thing on the screen should be the answer.
//!
//! # Workspaces
//!
//! `--filter` and `-w` choose which of a workspace's projects `uf add`,
//! `uf remove` and `uf update` change, and the manager runs once in each chosen
//! project's directory. That is the one spelling every manager agrees on: npm,
//! pnpm, both Yarns and bun all read "run in a member's directory" as "change
//! that member", and all of them settle the one lockfile at the workspace root.
//! The flags they each have for it — npm's `--workspace`, pnpm's `--filter`,
//! `yarn workspace <name>` — take different selectors and do not exist on bun's
//! `add` at all, so reaching for them would make `--filter` mean five things.
//! The selector grammar is `uf run --filter`'s, resolved by uf, so it means one.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;
use uf_config::{ResolvedConfig, load_config};
use uf_pm::delta::LockfileDelta;
use uf_pm::{
    DependencyKind, DetectionOptions, LinkTarget, ManagerRunError, Operation, PackageManagerPlan,
    check_workspace_manifests, detect_package_manager_with, install_workspace, installable,
    is_polluting_json_key, run_operation_with_detection,
};
use uf_term::{Cell, Column, KeyValue, Renderer, Status, Table, Tone, format_duration};

use super::install::{
    chosen_by, lockfile_label, render_change_counts, render_change_table, tracks_uf_lock,
};
use super::scripts_allowed;
use crate::support::{plural, project_label};
use crate::ui::Ui;

/// How many manifest changes the summary names before it stops listing them.
///
/// The same reasoning as the tree table's cap: `uf add` with two hundred
/// specifiers is a script, and a script does not read a list of two hundred.
const MANIFEST_CHANGES_SHOWN: usize = 15;

/// Which of a workspace's projects `uf add`, `uf remove` and `uf update`
/// change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The project uf was pointed at, which is what every one of these meant
    /// before there was a choice.
    #[default]
    Project,
    /// The root of the workspace around it, from anywhere inside (`-w`).
    WorkspaceRoot,
    /// The members these selectors pick (`--filter`), in `uf run --filter`'s
    /// grammar.
    Members(Vec<String>),
}

impl Scope {
    /// From the two flags, which clap has already refused together.
    pub(crate) fn from_flags(filter: Vec<String>, workspace_root: bool) -> Self {
        if workspace_root {
            Self::WorkspaceRoot
        } else if filter.is_empty() {
            Self::Project
        } else {
            Self::Members(filter)
        }
    }
}

/// One project a scoped command runs the manager in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Target {
    /// The directory the manager runs in, whose `package.json` it changes.
    pub(super) dir: Utf8PathBuf,
    /// What the report calls it, when it is not simply the project.
    pub(super) label: Option<String>,
}

/// Where a scoped command settles the lockfile, and where it runs the manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Targets {
    /// The workspace root — or, unscoped, the project itself — whose lockfile
    /// every run below writes.
    pub(super) base: Utf8PathBuf,
    /// The projects, in the order the manager runs in them.
    pub(super) each: Vec<Target>,
}

/// Resolve `scope` against the workspace `resolved` belongs to.
///
/// # Errors
///
/// When a scope names a workspace and the project is not in one, and when a
/// selector picks no member — a misspelt `--filter` that changed nothing and
/// exited 0 would be a command that succeeded at not doing what it was asked.
pub(super) fn targets(resolved: &ResolvedConfig, scope: &Scope) -> Result<Targets> {
    let selectors = match scope {
        Scope::Project => {
            return Ok(Targets {
                base: resolved.root.clone(),
                each: vec![Target {
                    dir: resolved.root.clone(),
                    label: None,
                }],
            });
        }
        Scope::WorkspaceRoot => None,
        Scope::Members(selectors) => Some(selectors),
    };

    let Some((root, members)) = uf_project::enclosing_workspace(&resolved.root, &resolved.config)
    else {
        bail!(
            "{} chooses projects in a workspace, and {} is not in one\n\n  \
             a workspace is a package.json that lists `workspaces`, a pnpm-workspace.yaml, \
             or a directory whose members have a uf.config.js of their own",
            if selectors.is_some() {
                "--filter"
            } else {
                "--workspace-root"
            },
            project_label(&resolved.root)
        );
    };
    let Some(selectors) = selectors else {
        return Ok(Targets {
            base: root.clone(),
            each: vec![Target {
                dir: root,
                label: Some("workspace root".to_owned()),
            }],
        });
    };

    let dependencies = uf_project::workspace_dependencies(&root, &members);
    let selected =
        uf_project::select_workspaces(&members, &dependencies, selectors).map_err(|error| {
            let names = members
                .iter()
                .map(|member| member.name.as_str())
                .collect::<Vec<_>>();
            anyhow!("{error}\n\n  members: {}", names.join(", "))
        })?;
    let each = selected
        .into_iter()
        .map(|at| Target {
            dir: root.join(&members[at].path),
            label: Some(members[at].name.to_string()),
        })
        .collect();
    Ok(Targets { base: root, each })
}

/// `uf add [--dev|--optional|--peer] [--filter SELECTOR|-w] SPEC...`.
pub(crate) fn add(
    cwd: &Utf8Path,
    ui: &mut Ui,
    specs: &[String],
    kind: DependencyKind,
    scope: &Scope,
) -> Result<()> {
    delegate(
        cwd,
        ui,
        &Request {
            heading: "uf add",
            operation: Operation::Add { kind },
            operands: specs,
            retry: retry_line("uf add", specs),
            announced: false,
            scope,
        },
    )
}

/// `uf remove [--filter SELECTOR|-w] NAME...`.
pub(crate) fn remove(cwd: &Utf8Path, ui: &mut Ui, names: &[String], scope: &Scope) -> Result<()> {
    delegate(
        cwd,
        ui,
        &Request {
            heading: "uf remove",
            operation: Operation::Remove,
            operands: names,
            retry: retry_line("uf remove", names),
            announced: false,
            scope,
        },
    )
}

/// The manager's own update: everything moves inside the range it is declared
/// with, and nothing else moves at all.
///
/// [`super::update`] is the command; this is the half of it that delegates.
pub(super) fn update(
    cwd: &Utf8Path,
    ui: &mut Ui,
    packages: &[String],
    scope: &Scope,
) -> Result<()> {
    delegate(
        cwd,
        ui,
        &Request {
            heading: "uf update",
            operation: Operation::Update,
            operands: packages,
            retry: retry_line("uf update", packages),
            announced: false,
            scope,
        },
    )
}

/// `uf dedupe`: the manager's own, and the tree it leaves.
///
/// Refused by name on Yarn 1, whose `yarn dedupe` exists only to say `yarn
/// install` already does it, and on bun, which has none.
pub(crate) fn dedupe(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    delegate(
        cwd,
        ui,
        &Request {
            heading: "uf dedupe",
            operation: Operation::Dedupe,
            operands: &[],
            retry: "uf dedupe".to_owned(),
            announced: false,
            scope: &Scope::Project,
        },
    )
}

/// `uf link [NAME|DIR]`.
///
/// A directory or a name is linked into this project, and reported like an
/// add: the manifest and the tree. With nothing named it registers this
/// package with the manager's global directory, which changes nothing in this
/// project — so it is reported the way a query is, and the manager's own line
/// about where it put the link is the answer. [`LinkTarget::of`] decides which
/// of the three was asked for.
pub(crate) fn link(cwd: &Utf8Path, ui: &mut Ui, target: Option<&str>) -> Result<()> {
    let operation = Operation::Link {
        target: LinkTarget::of(target),
    };
    let Some(target) = target else {
        return query(cwd, ui, "uf link", operation, &[]);
    };
    let operands = [target.to_owned()];
    delegate(
        cwd,
        ui,
        &Request {
            heading: "uf link",
            operation,
            operands: &operands,
            retry: format!("uf link {target}"),
            announced: false,
            scope: &Scope::Project,
        },
    )
}

/// `uf info PACKAGE [FIELD]`: the registry's answer, through the project's own
/// manager, so it is the registry that manager would install from.
pub(crate) fn info(cwd: &Utf8Path, ui: &mut Ui, package: &str, field: Option<&str>) -> Result<()> {
    let operands = std::iter::once(package)
        .chain(field)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    query(cwd, ui, "uf info", Operation::Info, &operands)
}

/// `uf why NAME`.
///
/// The one command here that changes nothing, so it neither takes the
/// `install_workspace` guard — there is no install to guard — nor rewrites
/// `uf.lock`. Asking why a package is installed must not install anything.
/// `uf ls`, `uf audit`, `uf search` and `uf info`: read the project, change
/// nothing. And `uf patch`, which changes a temporary directory and not this
/// project, and `uf link` with nothing named, which changes the manager's
/// global directory.
///
/// The same shape as [`why`] and for the same reason — the manager's own
/// output is the answer, so uf says who it is about to ask and then gets out
/// of the way. What differs is only which operation, and whether there are
/// operands.
///
/// A manager with no such command is reported rather than substituted. uf
/// could run npm's `search` for a bun project and it would even work; it would
/// also be uf choosing a package manager the project did not, which is the one
/// thing a manager-agnostic tool must not do. See
/// [`uf_pm::ManagerRunError::Unsupported`].
pub(crate) fn query(
    cwd: &Utf8Path,
    ui: &mut Ui,
    title: &str,
    operation: Operation<'_>,
    operands: &[String],
) -> Result<()> {
    let resolved = load_config(cwd)?;
    crate::support::render_deprecations(ui, resolved.config.package_manager_deprecation());
    let detection = detect_package_manager_with(
        &resolved.root,
        &DetectionOptions::from_config(&resolved.config),
    );
    let (manager, substituted) = installable(&detection);
    // Only an operation that installs has scripts to refuse, and only reading
    // the approvals for one keeps `uf ls` from depending on a manifest it has
    // no reason to parse. Registering a package for `uf link` is the query
    // that installs.
    let allow_scripts = if operation.installs_packages() {
        let plan = PackageManagerPlan::infer_from_config(&resolved.config);
        scripts_allowed(&resolved.root, manager, &plan)?
    } else {
        true
    };
    // Before the manager is installed: a manager with no such command is
    // refused without first being downloaded to find that out.
    let invocation =
        uf_pm::invocation_for(&resolved.root, manager, operation, operands, allow_scripts)?;
    // The release `packageManager` pins, and the runtime it runs on, in front
    // of `PATH` for the manager's process — installed the first time.
    let path =
        crate::commands::runtimes::manager_path(&resolved, manager, false, &mut |message| {
            ui.render_err(|renderer, out| renderer.status(out, uf_term::Status::Info, message));
        })?;
    let project = project_label(&resolved.root).to_string();
    let manager_label = manager.to_string();
    let source = chosen_by(&detection.source, substituted);
    let command = invocation.to_string();
    let title = title.to_owned();
    ui.render(|renderer, out| {
        renderer.banner(out, &title, Some(&project));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("manager", &manager_label),
                KeyValue::toned("chosen by", &source, Tone::Muted),
                KeyValue::toned("command", &command, Tone::Path),
            ],
        );
        renderer.blank(out);
    });

    run_operation_with_detection(
        &resolved.root,
        &detection,
        operation,
        operands,
        allow_scripts,
        &path,
    )
    .map_err(|error| {
        let mut what_to_do = format!("{manager_label} reported a problem; its output is above");
        // What the refusal means, where uf knows: pnpm 12 answers `pnpm link`
        // with nothing named with a usage error, and does not say what to run.
        if let Some(hint) = uf_pm::run::failure_hint(manager, operation) {
            what_to_do.push_str("\n\n  ");
            what_to_do.push_str(hint);
        }
        failed_hint(error, &what_to_do)
    })?;
    Ok(())
}

/// `uf patch NAME` and `uf patch --commit DIR`.
///
/// Two halves of one escape hatch: the first opens a copy of a dependency
/// somewhere you can edit it and prints where, the second turns those edits
/// into a patch file the install reapplies from then on.
///
/// The open half goes through [`query`], whose shape is exactly right for it:
/// the manager's own output *is* the answer — a directory path you are about to
/// `cd` into — so uf says which manager it is asking and then gets out of the
/// way rather than printing a summary after it. Nothing about this project
/// changes, so there is no lockfile to report on.
///
/// The commit half installs — it records the patch in the manifest and
/// reinstalls the package it patched, whose scripts then run against code you
/// have just edited — so it goes through [`delegate`], which refuses scripts,
/// rewrites `uf.lock`, and reports what moved in the tree.
///
/// On npm, bun and Yarn 1 both halves are refused by name rather than attempted
/// (see [`uf_pm::ManagerRunError::Unsupported`]). uf could run `patch-package`
/// for them and it would even work; it would also be uf installing a dependency
/// the project did not choose, on the one command whose entire purpose is to
/// edit somebody else's code.
pub(crate) fn patch(cwd: &Utf8Path, ui: &mut Ui, target: &str, commit: bool) -> Result<()> {
    let operands = [target.to_owned()];
    if commit {
        return delegate(
            cwd,
            ui,
            &Request {
                heading: "uf patch --commit",
                operation: Operation::PatchCommit,
                operands: &operands,
                retry: format!("uf patch --commit {target}"),
                announced: false,
                scope: &Scope::Project,
            },
        );
    }
    query(cwd, ui, "uf patch", Operation::Patch, &operands)
}

pub(crate) fn why(cwd: &Utf8Path, ui: &mut Ui, package: &str) -> Result<()> {
    let resolved = load_config(cwd)?;
    crate::support::render_deprecations(ui, resolved.config.package_manager_deprecation());
    let detection = detect_package_manager_with(
        &resolved.root,
        &DetectionOptions::from_config(&resolved.config),
    );
    let (manager, substituted) = installable(&detection);
    // The release `packageManager` pins, and the runtime it runs on, in front
    // of `PATH` for the manager's process — installed the first time.
    let path =
        crate::commands::runtimes::manager_path(&resolved, manager, false, &mut |message| {
            ui.render_err(|renderer, out| renderer.status(out, uf_term::Status::Info, message));
        })?;
    let operands = [package.to_owned()];

    // Before the manager runs, because its answer is what the reader came for
    // and it should be the last thing on the screen.
    let project = project_label(&resolved.root).to_string();
    let invocation =
        uf_pm::invocation_for(&resolved.root, manager, Operation::Why, &operands, true)?;
    let manager_label = manager.to_string();
    let source = chosen_by(&detection.source, substituted);
    let command = invocation.to_string();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf why", Some(&project));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("manager", &manager_label),
                KeyValue::toned("chosen by", &source, Tone::Muted),
                KeyValue::toned("command", &command, Tone::Path),
            ],
        );
        renderer.blank(out);
    });

    run_operation_with_detection(
        &resolved.root,
        &detection,
        Operation::Why,
        &operands,
        true,
        &path,
    )
    .map_err(|error| {
        failed_hint(
            error,
            &format!(
                "{manager_label} could not explain {package:?}; it is not in this project's tree, \
                 or the name is spelled differently in the manifest"
            ),
        )
    })?;
    Ok(())
}

/// One delegated command that changes the project.
pub(super) struct Request<'a> {
    /// The banner and the heading, e.g. `uf add`.
    pub(super) heading: &'static str,
    /// What the manager is being asked to do.
    pub(super) operation: Operation<'a>,
    /// The package specifiers or names, exactly as they were typed.
    pub(super) operands: &'a [String],
    /// The line to tell someone to run again after they have fixed it.
    pub(super) retry: String,
    /// Whether the banner is already on the screen.
    ///
    /// `uf update --latest` draws its own, reports what it is about to rewrite,
    /// rewrites it, and only then delegates the install. Drawing a second
    /// banner in the middle of that would read as a second command.
    pub(super) announced: bool,
    /// Which of the workspace's projects the manager runs in.
    pub(super) scope: &'a Scope,
}

/// Detect, refuse scripts, run the manager, and report both files.
pub(super) fn delegate(cwd: &Utf8Path, ui: &mut Ui, request: &Request<'_>) -> Result<()> {
    let started = Instant::now();
    // Before anything is read or written: a command that refuses its own
    // argument must not have rewritten `uf.lock` on the way to refusing it.
    uf_pm::check_operands(request.operands)?;
    let resolved = load_config(cwd)?;
    crate::support::render_deprecations(ui, resolved.config.package_manager_deprecation());
    // Which projects, before which manager: a selector that picks nothing is
    // refused before anything else is looked at.
    let targets = targets(&resolved, request.scope)?;
    // The workspace root's own config decides the manager, the release of it
    // uf's store holds, and the script policy. A member with no `uf.config.js`
    // says nothing about any of them, and `-w` run from one must not mean "the
    // defaults, and whatever manager is on PATH".
    let resolved = if targets.base == resolved.root {
        resolved
    } else {
        load_config(&targets.base)?
    };
    let plan = PackageManagerPlan::infer_from_config(&resolved.config);
    let base = targets.base.as_path();

    let detection =
        detect_package_manager_with(base, &DetectionOptions::from_config(&resolved.config));
    let tracks_uf_lock = tracks_uf_lock(&detection);
    let (manager, _) = installable(&detection);
    // A manager without the command, or without a per-member form of it, is
    // refused here too, before the workspace guard below rewrites `uf.lock`.
    uf_pm::invocation_for(base, manager, request.operation, request.operands, true)?;
    if matches!(request.scope, Scope::Members(_)) {
        uf_pm::check_member_operation(manager, request.operation)?;
    }

    // The same refusal `uf install` makes, for the same reason and a stronger
    // one: adding a dependency is when a lifecycle script most often arrives,
    // and this is the guard that runs before anything is fetched. Delegated
    // projects get the guard without being forced to grow `uf.lock`.
    if tracks_uf_lock {
        install_workspace(base, &resolved.config)?;
    } else {
        check_workspace_manifests(base, &resolved.config)?;
    }

    // Both "before" states have to be read before the manager runs. A manifest
    // read afterwards is the manifest the manager wrote, and a lockfile read
    // afterwards is the lockfile it wrote: either one would report that nothing
    // changed, every time.
    let manifests_before = targets
        .each
        .iter()
        .map(|target| dependency_entries(&target.dir.join("package.json")))
        .collect::<Vec<_>>();
    let path =
        crate::commands::runtimes::manager_path(&resolved, manager, false, &mut |message| {
            ui.render_err(|renderer, out| renderer.status(out, uf_term::Status::Info, message));
        })?;
    let tree_before = uf_pm::delta::snapshot(base, manager);

    let project = project_label(base).to_string();
    let announced = request.announced;
    ui.render(|renderer, out| {
        if !announced {
            renderer.banner(out, request.heading, Some(&project));
        }
        renderer.blank(out);
    });

    let allow_scripts = scripts_allowed(base, manager, &plan)?;
    // What `uf link` is about to put in `node_modules`, named before the
    // manager runs: `uf link DIR` names the package by that directory's manifest.
    let expected_link = linked_name(base, request);
    let mut commands = Vec::with_capacity(targets.each.len());
    let mut outcome = None;
    for target in &targets.each {
        let run = run_operation_with_detection(
            &target.dir,
            &detection,
            request.operation,
            request.operands,
            allow_scripts,
            &path,
        )
        .map_err(|error| {
            let place = target
                .label
                .as_ref()
                .map(|label| format!(" in {label}"))
                .unwrap_or_default();
            // A hint says what to run instead, so the command that failed is
            // not offered again beside it: on pnpm 12, `uf link <name>` is
            // the refused form itself.
            let what_to_do =
                if let Some(hint) = uf_pm::run::failure_hint(manager, request.operation) {
                    format!("the manager printed why above{place}\n\n  {hint}")
                } else {
                    format!(
                        "the manager printed why above{place}; fix that and run `{}` again",
                        request.retry
                    )
                };
            failed_hint(error, &what_to_do)
        })?;
        commands.push(match &target.label {
            Some(label) => format!("{}  ({label})", run.invocation),
            None => run.invocation.to_string(),
        });
        outcome = Some(run);
    }
    let outcome = outcome.ok_or_else(|| anyhow!("no project was chosen to run the manager in"))?;

    // The manifests the manager just rewrote are checked again. A native uf
    // project also rewrites the lock and store it owns; a delegated project
    // leaves that to npm, pnpm, Yarn or Bun.
    if tracks_uf_lock {
        install_workspace(base, &resolved.config).with_context(|| {
            format!(
                "`{}` succeeded, but uf could not rewrite {} from the manifests it changed",
                outcome.invocation, resolved.config.pm.lockfile
            )
        })?;
    } else {
        check_workspace_manifests(base, &resolved.config).with_context(|| {
            format!(
                "`{}` succeeded, but uf could not re-check the manifests it changed",
                outcome.invocation
            )
        })?;
    }

    let mut manifest = Vec::new();
    for (target, before) in targets.each.iter().zip(&manifests_before) {
        let after = dependency_entries(&target.dir.join("package.json"));
        manifest.extend(manifest_changes(before, &after).into_iter().map(|change| {
            ManifestChange {
                member: target.label.clone(),
                ..change
            }
        }));
    }
    let tree_after = uf_pm::delta::snapshot(base, manager);
    let tree = uf_pm::delta::diff(&tree_before, &tree_after);

    // npm, Yarn 1 and bun link without writing either file, so whether
    // anything was linked is read where every manager leaves a link.
    let link = expected_link
        .map(|name| {
            link_report(
                base,
                manager,
                &name,
                uf_pm::links::link_state(base, &name),
                uf_pm::links::linked_resolution(base, &name),
            )
            .map_err(|why| anyhow!("`{}` succeeded, but {why}", outcome.invocation))
        })
        .transpose()?;

    let report = DepsReport {
        heading: request.heading,
        continued: request.announced,
        manager: outcome.manager.to_string(),
        chosen_by: chosen_by(&outcome.source, outcome.substituted),
        commands,
        lockfile: lockfile_label(base, &tree_after),
        manifest,
        tree,
        link,
        elapsed: started.elapsed(),
    };

    ui.render(|renderer, out| {
        renderer.blank(out);
        render_summary(renderer, out, &report);
    });
    Ok(())
}

/// Everything one of these commands has to say once the manager has exited.
struct DepsReport {
    heading: &'static str,
    /// Whether this is the second half of a command that already reported.
    continued: bool,
    manager: String,
    chosen_by: String,
    /// One command line per project the manager ran in, in that order.
    commands: Vec<String>,
    lockfile: String,
    manifest: Vec<ManifestChange>,
    tree: LockfileDelta,
    /// What `uf link NAME` or `uf link DIR` left in `node_modules`, and `None`
    /// for every other command.
    link: Option<LinkReport>,
    elapsed: Duration,
}

/// A link, as `node_modules` has it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkReport {
    /// The package.
    name: String,
    /// Where `node_modules/<name>` leads, written the way someone in the
    /// project would write it; for a `recorded` link, the resolution Yarn wrote.
    to: String,
    /// Yarn 2+ recorded the link in `resolutions`, and nothing in the project
    /// depends on the package yet, so `node_modules` has no link to it.
    recorded: bool,
}

/// Draw the summary.
///
/// Split out from the command the way `uf install`'s is, so a test can render
/// it with [`uf_term::Capabilities::plain`] and read the layout rather than
/// asserting on whatever npm happened to resolve today.
fn render_summary(renderer: &Renderer, out: &mut String, report: &DepsReport) {
    let mut rows = vec![
        KeyValue::new("manager", &report.manager),
        KeyValue::toned("chosen by", &report.chosen_by, Tone::Muted),
    ];
    rows.extend(
        report
            .commands
            .iter()
            .map(|command| KeyValue::toned("command", command.as_str(), Tone::Path)),
    );
    rows.push(KeyValue::toned("lockfile", &report.lockfile, Tone::Path));
    renderer.key_values(out, 2, &rows);

    let elapsed = format_duration(report.elapsed);
    if report.link.is_none() && report.manifest.is_empty() && report.tree.is_unchanged() {
        // The second time somebody adds the same thing, which is most of the
        // times anybody adds anything twice.
        renderer.blank(out);
        renderer.status(
            out,
            Status::Success,
            &format!("already up to date in {elapsed}"),
        );
        return;
    }

    if let Some(link) = &report.link {
        renderer.blank(out);
        renderer.heading(out, 2, if link.recorded { "recorded" } else { "linked" });
        renderer.blank(out);
        renderer.key_values(out, 4, &[KeyValue::toned(&link.name, &link.to, Tone::Path)]);
    }

    if !report.manifest.is_empty() {
        renderer.blank(out);
        renderer.heading(out, 2, "manifest");
        render_manifest_changes(renderer, out, &report.manifest);
    }

    if report.tree.detailed && !report.tree.is_unchanged() {
        renderer.blank(out);
        renderer.heading(out, 2, "dependency tree");
        render_change_counts(renderer, out, &report.tree);
        render_change_table(renderer, out, &report.tree);
    }

    renderer.blank(out);
    match &report.link {
        Some(link) if link.recorded => renderer.status(
            out,
            Status::Warn,
            &format!(
                "{} recorded in resolutions in {elapsed}; nothing here depends on it yet, so \
                 node_modules has no link to it",
                link.name
            ),
        ),
        Some(link) => renderer.status(
            out,
            Status::Success,
            &format!("linked {} in {elapsed}", link.name),
        ),
        None => renderer.status(
            out,
            Status::Success,
            &format!("{} in {elapsed}", headline(report)),
        ),
    }
}

/// The package `uf link NAME` or `uf link DIR` is about to link, or `None` for
/// every other command.
///
/// A directory is named by its own manifest. One with no manifest, or one that
/// names no package, gives `None`: the manager refuses to link it in its own
/// words, and uf has nothing of its own to check.
fn linked_name(base: &Utf8Path, request: &Request<'_>) -> Option<String> {
    let Operation::Link { target } = request.operation else {
        return None;
    };
    let operand = request.operands.first()?;
    match target {
        LinkTarget::Register => None,
        LinkTarget::Package => uf_pm::links::is_package_name(operand).then(|| operand.clone()),
        LinkTarget::Directory => uf_pm::links::package_name(&base.join(operand)),
    }
}

/// What a link command did, from `node_modules` rather than from the manager.
///
/// # Errors
///
/// The clause that follows "the manager succeeded, but", when nothing is
/// linked. A success message after a manager that linked nothing is the report
/// this exists to stop printing (ubugeeei-prod/uf#976).
fn link_report(
    base: &Utf8Path,
    manager: uf_pm::PackageManager,
    name: &str,
    state: Option<uf_pm::links::LinkState>,
    resolution: Option<String>,
) -> Result<LinkReport, String> {
    use uf_pm::links::LinkState;

    if let Some(LinkState::Linked(to)) = &state {
        return Ok(LinkReport {
            name: name.to_owned(),
            to: shown_path(base, to),
            recorded: false,
        });
    }
    // Yarn 2+ records a link in `resolutions`, and a resolution only resolves
    // a package something already depends on. So a recorded link explains an
    // empty `node_modules/<name>` and nothing else: an installed package or a
    // broken link beside a resolution is a link that did not take.
    if manager == uf_pm::PackageManager::Yarn(uf_pm::YarnEdition::Berry)
        && matches!(state, Some(LinkState::Absent))
        && let Some(resolution) = resolution
    {
        return Ok(LinkReport {
            name: name.to_owned(),
            to: resolution,
            recorded: true,
        });
    }
    Err(match state {
        Some(LinkState::Broken(to)) => format!(
            "node_modules/{name} is a link to {to}, which does not exist: nothing was linked"
        ),
        Some(LinkState::Installed) => {
            format!("node_modules/{name} is an installed package, not a link: nothing was linked")
        }
        _ => format!("there is no node_modules/{name}: nothing was linked"),
    })
}

/// A path the way someone standing in the project would write it: `vendor/ui`
/// inside it, `../ui` beside it, and the whole path anywhere else.
fn shown_path(base: &Utf8Path, path: &Utf8Path) -> String {
    let base = base
        .canonicalize_utf8()
        .unwrap_or_else(|_| base.to_path_buf());
    if let Ok(inside) = path.strip_prefix(&base) {
        return inside.to_string();
    }
    if let Some(parent) = base.parent()
        && let Ok(beside) = path.strip_prefix(parent)
    {
        return format!("../{beside}");
    }
    path.to_string()
}

/// What happened, in one clause, from the files rather than from the request.
///
/// The manifest is what `uf add`, `uf remove` and `uf update` are *for*, so for
/// them it speaks first. A run that changed no manifest entry but did move the
/// tree says that instead of claiming a package was added: `uf add react` in a
/// project that already depended on `react` at that range really did change
/// the tree and really did not change the manifest, and both halves are worth
/// saying. `uf dedupe` never meant to change a manifest, so for it the tree is
/// the whole sentence.
fn headline(report: &DepsReport) -> String {
    if report.manifest.is_empty() {
        let manifest_was_the_point = matches!(
            report.heading,
            "uf add" | "uf remove" | "uf update" | "uf patch --commit"
        );
        // `uf update --latest` rewrote the manifests itself and said so a few
        // lines above; from the manager's side there was then nothing left to
        // change. Saying "the manifest already said so" there would read as a
        // denial of the line the reader just saw.
        if report.continued || !manifest_was_the_point {
            return format!(
                "{} in the tree",
                plural(report.tree.changes.len(), "change")
            );
        }
        return format!(
            "the manifest already said so; {} in the tree",
            plural(report.tree.changes.len(), "change")
        );
    }
    let verb = match report.heading {
        "uf remove" => "taken out of",
        "uf update" => "re-ranged in",
        _ => "recorded in",
    };
    let mut fields: Vec<&str> = report
        .manifest
        .iter()
        .map(|change| change.field)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    fields.sort_unstable();
    format!(
        "{} {verb} {}",
        plural(report.manifest.len(), "package"),
        fields.join(", ")
    )
}

/// The manifest entries that moved, capped, with the field they moved in —
/// and the workspace member, when the command chose members.
fn render_manifest_changes(renderer: &Renderer, out: &mut String, changes: &[ManifestChange]) {
    renderer.blank(out);
    let members = changes.iter().any(|change| change.member.is_some());
    let mut columns = vec![Column::left("")];
    if members {
        columns.push(Column::left("workspace"));
    }
    columns.extend([
        Column::left("field"),
        Column::left("package"),
        Column::left("range"),
    ]);
    let mut table = Table::new(columns);
    for change in changes.iter().take(MANIFEST_CHANGES_SHOWN) {
        let mut row = vec![Cell::toned(change.mark(), change.tone())];
        if members {
            row.push(Cell::new(change.member.as_deref().unwrap_or_default()));
        }
        row.extend([
            Cell::new(change.field),
            Cell::new(&change.name),
            Cell::toned(&change.range, Tone::Number),
        ]);
        table.push(row);
    }
    renderer.table(out, 4, &table);

    let hidden = changes.len().saturating_sub(MANIFEST_CHANGES_SHOWN);
    if hidden > 0 {
        uf_term::push_spaces(out, 4);
        renderer.line(out, renderer.theme().muted, &format!("and {hidden} more"));
    }
}

/// One dependency map entry that is different from how it was.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestChange {
    /// The workspace member whose manifest it is, when the command chose
    /// members rather than the project.
    member: Option<String>,
    /// The `package.json` field it is in, or was in.
    field: &'static str,
    /// The package name.
    name: String,
    /// The range now, or the range it had when it was taken out.
    range: String,
    /// What happened to it.
    kind: ManifestChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestChangeKind {
    /// The field did not list it before.
    Added,
    /// The field does not list it any more.
    Removed,
    /// Same field, different range.
    Reranged,
}

impl ManifestChange {
    const fn mark(&self) -> &'static str {
        match self.kind {
            ManifestChangeKind::Added => "+",
            ManifestChangeKind::Removed => "-",
            ManifestChangeKind::Reranged => "~",
        }
    }

    const fn tone(&self) -> Tone {
        match self.kind {
            ManifestChangeKind::Added => Tone::Good,
            ManifestChangeKind::Removed => Tone::Bad,
            ManifestChangeKind::Reranged => Tone::Accent,
        }
    }
}

/// Every dependency `package.json` declares, keyed by field and name.
///
/// A manifest is repository content and this reads it without anything having
/// validated it: a file that is not JSON, or not an object, is no entries
/// rather than an error, because the manager is the thing that gets to refuse a
/// manifest it cannot read and it will say so far better than this could.
/// Prototype-pollution keys are dropped the way they are everywhere else uf
/// walks a JSON map.
fn dependency_entries(manifest: &Utf8Path) -> BTreeMap<(&'static str, String), String> {
    let mut entries = BTreeMap::new();
    let Ok(source) = std::fs::read_to_string(manifest) else {
        return entries;
    };
    let Ok(value) = serde_json::from_str::<Value>(&source) else {
        return entries;
    };
    for kind in DependencyKind::ALL {
        let field = kind.manifest_field();
        let Some(map) = value.get(field).and_then(Value::as_object) else {
            continue;
        };
        for (name, range) in map {
            if is_polluting_json_key(name) {
                continue;
            }
            if let Some(range) = range.as_str() {
                entries.insert((field, name.clone()), range.to_owned());
            }
        }
    }
    entries
}

/// What moved between two readings of the same manifest.
fn manifest_changes(
    before: &BTreeMap<(&'static str, String), String>,
    after: &BTreeMap<(&'static str, String), String>,
) -> Vec<ManifestChange> {
    let mut changes = Vec::new();
    for ((field, name), range) in after {
        let change = match before.get(&(field, name.clone())) {
            None => ManifestChangeKind::Added,
            Some(previous) if previous == range => continue,
            Some(_) => ManifestChangeKind::Reranged,
        };
        changes.push(ManifestChange {
            member: None,
            field,
            name: name.clone(),
            range: range.clone(),
            kind: change,
        });
    }
    for ((field, name), range) in before {
        if after.contains_key(&(field, name.clone())) {
            continue;
        }
        changes.push(ManifestChange {
            member: None,
            field,
            name: name.clone(),
            range: range.clone(),
            kind: ManifestChangeKind::Removed,
        });
    }
    changes
}

/// The command line to run again, with the operands the reader typed.
fn retry_line(command: &str, operands: &[String]) -> String {
    if operands.is_empty() {
        return command.to_owned();
    }
    format!("{command} {}", operands.join(" "))
}

/// A manager that ran and failed, with the sentence it did not print.
///
/// The manager's own diagnosis is already on the terminal — that is the whole
/// point of letting it inherit uf's stdio — so this adds what to do about it
/// and nothing else. Anything that is not a failed run already carries its own
/// hint: a missing manager names the manager, and a refused operand says what
/// to write instead.
fn failed_hint(error: ManagerRunError, what_to_do: &str) -> anyhow::Error {
    if !matches!(error, ManagerRunError::Failed { .. }) {
        return anyhow!(error);
    }
    anyhow!("{error}\n\n  {what_to_do}")
}

#[cfg(test)]
mod tests;
