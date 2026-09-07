//! `uf add`, `uf remove`, `uf update` and `uf why`: one dependency at a time.
//!
//! `uf install` is the whole tree; these four are the four things a person does
//! to it between installs. All of them are the project's own package manager,
//! for the reason [`uf_pm::run`] gives at length: uf's resolver reaches no
//! registry, so an `uf add` that claimed to have fetched `date-fns` would be
//! lying about the one moment where lying matters — installing a new package is
//! exactly when a `postinstall` script arrives.
//!
//! Delegating is a complete answer, and it is a *better* answer than dropping
//! out to npm by hand, because everything uf knows about the project stays in
//! force: `pm.allowLifecycleScripts` becomes `--ignore-scripts` on the child, a
//! manifest that declares scripts of its own is refused before anything is
//! fetched, and `uf.lock` and the content-addressed store are rewritten from
//! the manifests the manager just changed. Reaching for `npm install` skips all
//! three.
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

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use camino::Utf8Path;
use serde_json::Value;
use uf_config::load_config;
use uf_pm::delta::LockfileDelta;
use uf_pm::{
    DependencyKind, ManagerRunError, Operation, PackageManagerPlan, detect_package_manager,
    install_workspace, installable, is_polluting_json_key, run_operation,
};
use uf_term::{Cell, Column, KeyValue, Renderer, Status, Table, Tone, format_duration};

use super::install::{chosen_by, lockfile_label, render_change_counts, render_change_table};
use crate::support::{plural, project_label};
use crate::ui::Ui;

/// How many manifest changes the summary names before it stops listing them.
///
/// The same reasoning as the tree table's cap: `uf add` with two hundred
/// specifiers is a script, and a script does not read a list of two hundred.
const MANIFEST_CHANGES_SHOWN: usize = 15;

/// `uf add [--dev|--optional|--peer] SPEC...`.
pub(crate) fn add(
    cwd: &Utf8Path,
    ui: &mut Ui,
    specs: &[String],
    kind: DependencyKind,
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
        },
    )
}

/// `uf remove NAME...`.
pub(crate) fn remove(cwd: &Utf8Path, ui: &mut Ui, names: &[String]) -> Result<()> {
    delegate(
        cwd,
        ui,
        &Request {
            heading: "uf remove",
            operation: Operation::Remove,
            operands: names,
            retry: retry_line("uf remove", names),
            announced: false,
        },
    )
}

/// The manager's own update: everything moves inside the range it is declared
/// with, and nothing else moves at all.
///
/// [`super::update`] is the command; this is the half of it that delegates.
pub(super) fn update(cwd: &Utf8Path, ui: &mut Ui, packages: &[String]) -> Result<()> {
    delegate(
        cwd,
        ui,
        &Request {
            heading: "uf update",
            operation: Operation::Update,
            operands: packages,
            retry: retry_line("uf update", packages),
            announced: false,
        },
    )
}

/// `uf why NAME`.
///
/// The one command here that changes nothing, so it neither takes the
/// `install_workspace` guard — there is no install to guard — nor rewrites
/// `uf.lock`. Asking why a package is installed must not install anything.
/// `uf ls`, `uf audit` and `uf search`: read the project, change nothing. And
/// `uf patch`, which changes a temporary directory and not this project.
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
    let detection = detect_package_manager(&resolved.root);
    let (manager, substituted) = installable(&detection);

    let invocation = uf_pm::invocation_for(manager, operation, operands, true)?;
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

    run_operation(&resolved.root, operation, operands, true).map_err(|error| {
        failed_hint(
            error,
            &format!("{manager_label} reported a problem; its output is above"),
        )
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
            },
        );
    }
    query(cwd, ui, "uf patch", Operation::Patch, &operands)
}

pub(crate) fn why(cwd: &Utf8Path, ui: &mut Ui, package: &str) -> Result<()> {
    let resolved = load_config(cwd)?;
    let detection = detect_package_manager(&resolved.root);
    let (manager, substituted) = installable(&detection);
    let operands = [package.to_owned()];

    // Before the manager runs, because its answer is what the reader came for
    // and it should be the last thing on the screen.
    let project = project_label(&resolved.root).to_string();
    let invocation = uf_pm::invocation_for(manager, Operation::Why, &operands, true)?;
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

    run_operation(&resolved.root, Operation::Why, &operands, true).map_err(|error| {
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
}

/// Detect, refuse scripts, run the manager, and report both files.
pub(super) fn delegate(cwd: &Utf8Path, ui: &mut Ui, request: &Request<'_>) -> Result<()> {
    let started = Instant::now();
    // Before anything is read or written: a command that refuses its own
    // argument must not have rewritten `uf.lock` on the way to refusing it.
    uf_pm::check_operands(request.operands)?;
    let resolved = load_config(cwd)?;
    let plan = PackageManagerPlan::infer_from_config(&resolved.config);

    // The same refusal `uf install` makes, for the same reason and a stronger
    // one: adding a dependency is when a lifecycle script most often arrives,
    // and this is the guard that runs before anything is fetched.
    install_workspace(&resolved.root, &resolved.config)?;

    // Both "before" states have to be read before the manager runs. A manifest
    // read afterwards is the manifest the manager wrote, and a lockfile read
    // afterwards is the lockfile it wrote: either one would report that nothing
    // changed, every time.
    let manifest_path = resolved.root.join("package.json");
    let manifest_before = dependency_entries(&manifest_path);
    let (manager, _) = installable(&detect_package_manager(&resolved.root));
    let tree_before = uf_pm::delta::snapshot(&resolved.root, manager);

    let project = project_label(&resolved.root).to_string();
    let announced = request.announced;
    ui.render(|renderer, out| {
        if !announced {
            renderer.banner(out, request.heading, Some(&project));
        }
        renderer.blank(out);
    });

    let outcome = run_operation(
        &resolved.root,
        request.operation,
        request.operands,
        !plan.forbids_npm_scripts(),
    )
    .map_err(|error| {
        failed_hint(
            error,
            &format!(
                "the manager printed why above; fix that and run `{}` again",
                request.retry
            ),
        )
    })?;

    // The manifests the manager just rewrote are what `uf.lock` and the store
    // describe, so they are rewritten from them. Without this the lockfile uf
    // owns would still be describing the project as it was before the command
    // that was just run.
    install_workspace(&resolved.root, &resolved.config).with_context(|| {
        format!(
            "`{}` succeeded, but uf could not rewrite {} from the manifests it changed",
            outcome.invocation, resolved.config.pm.lockfile
        )
    })?;

    let manifest = manifest_changes(&manifest_before, &dependency_entries(&manifest_path));
    let tree_after = uf_pm::delta::snapshot(&resolved.root, manager);
    let tree = uf_pm::delta::diff(&tree_before, &tree_after);

    let report = DepsReport {
        heading: request.heading,
        continued: request.announced,
        manager: outcome.manager.to_string(),
        chosen_by: chosen_by(&outcome.source, outcome.substituted),
        command: outcome.invocation.to_string(),
        lockfile: lockfile_label(&resolved.root, &tree_after),
        manifest,
        tree,
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
    command: String,
    lockfile: String,
    manifest: Vec<ManifestChange>,
    tree: LockfileDelta,
    elapsed: Duration,
}

/// Draw the summary.
///
/// Split out from the command the way `uf install`'s is, so a test can render
/// it with [`uf_term::Capabilities::plain`] and read the layout rather than
/// asserting on whatever npm happened to resolve today.
fn render_summary(renderer: &Renderer, out: &mut String, report: &DepsReport) {
    renderer.key_values(
        out,
        2,
        &[
            KeyValue::new("manager", &report.manager),
            KeyValue::toned("chosen by", &report.chosen_by, Tone::Muted),
            KeyValue::toned("command", &report.command, Tone::Path),
            KeyValue::toned("lockfile", &report.lockfile, Tone::Path),
        ],
    );

    let elapsed = format_duration(report.elapsed);
    if report.manifest.is_empty() && report.tree.is_unchanged() {
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
    renderer.status(
        out,
        Status::Success,
        &format!("{} in {elapsed}", headline(report)),
    );
}

/// What happened, in one clause, from the files rather than from the request.
///
/// The manifest is what these commands are *for*, so it speaks first. A run
/// that changed no manifest entry but did move the tree says that instead of
/// claiming a package was added: `uf add react` in a project that already
/// depended on `react` at that range really did change the tree and really did
/// not change the manifest, and both halves are worth saying.
fn headline(report: &DepsReport) -> String {
    if report.manifest.is_empty() {
        // `uf update --latest` rewrote the manifests itself and said so a few
        // lines above; from the manager's side there was then nothing left to
        // change. Saying "the manifest already said so" there would read as a
        // denial of the line the reader just saw.
        if report.continued {
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

/// The manifest entries that moved, capped, with the field they moved in.
fn render_manifest_changes(renderer: &Renderer, out: &mut String, changes: &[ManifestChange]) {
    renderer.blank(out);
    let mut table = Table::new(vec![
        Column::left(""),
        Column::left("field"),
        Column::left("package"),
        Column::left("range"),
    ]);
    for change in changes.iter().take(MANIFEST_CHANGES_SHOWN) {
        table.push(vec![
            Cell::toned(change.mark(), change.tone()),
            Cell::new(change.field),
            Cell::new(&change.name),
            Cell::toned(&change.range, Tone::Number),
        ]);
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
