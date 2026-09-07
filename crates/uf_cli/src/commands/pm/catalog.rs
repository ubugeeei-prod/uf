//! `uf catalog`: one version, named once, for a workspace that declares it
//! everywhere.
//!
//! # The decision this command is (ubugeeei-prod/uf#496)
//!
//! pnpm has *catalogs*: `pnpm-workspace.yaml` names a version once, and every
//! package writes `"react": "catalog:"`. Nothing else has the concept — npm,
//! yarn and bun read `catalog:` as a specifier they cannot resolve and refuse
//! the install.
//!
//! That left two ways to have a `uf catalog`, and both were wrong:
//!
//! **A passthrough to pnpm** is a command that does nothing in four projects
//! out of five, which is not a command uf should ship.
//!
//! **uf's own `catalog:`, resolved before the manager sees the manifests**,
//! means uf rewriting everybody's `package.json` on the way into an install and
//! putting it back afterwards. That is uf standing between the manifests and
//! the installer — the thing `docs/red-lines.md` exists to prevent — and it is
//! unsafe besides: a `^C` half way through leaves a workspace full of manifests
//! nobody wrote.
//!
//! So uf does not invent a fifth catalogue, and it does not resolve anything
//! behind a manager's back. It offers the thing catalogs are actually *for*,
//! in a form all five managers already understand: **one place to change a
//! version, and a check that every package agrees.**
//!
//! - The catalogue is the workspace itself. A package declared by more than one
//!   manifest is a catalogue entry, and the range they agree on is its value.
//! - `uf catalog` prints it, and prints every package whose manifests disagree
//!   — which is the bug a catalogue exists to prevent, and the one nothing
//!   currently reports.
//! - `uf catalog set NAME RANGE` writes that range into every manifest that
//!   declares `NAME`, and installs. One place to change a version, on npm,
//!   pnpm, yarn and bun alike.
//!
//! What this gives up against pnpm's version is the indirection: a manifest
//! here says `^19.0.0` rather than `catalog:`. What it buys is that the file
//! says what will be installed, on every manager, with nothing in between —
//! and that `uf catalog set` is one command rather than an edit plus a hope.
//!
//! A project that already uses pnpm's catalogs keeps them. uf reads `catalog:`
//! as a range it does not rewrite, reports how many there are, and leaves
//! pnpm to resolve them, because pnpm can.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use uf_config::load_config;
use uf_infra::FxHashMap;
use uf_pm::Operation;
use uf_pm::manifests::{Changes, Declaration};
use uf_term::{Cell, Column, Status, Table, Tone};

use crate::support::{plural, project_label};
use crate::ui::Ui;

/// How many rows either table prints before it stops listing them.
const ROWS_SHOWN: usize = 40;

/// `uf catalog`.
///
/// # Errors
///
/// When the project or one of its manifests cannot be read.
pub(crate) fn list(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let resolved = load_config(cwd)?;
    let project = project_label(&resolved.root).to_string();
    let entries = entries(
        &resolved.root,
        &uf_pm::manifests::declarations(&resolved.root)?,
    );

    ui.render(|renderer, out| {
        renderer.banner(out, "uf catalog", Some(&project));
        renderer.blank(out);
        render(renderer, out, &entries);
    });
    Ok(())
}

/// `uf catalog set NAME RANGE [--dry-run]`.
///
/// # Errors
///
/// When the range is not one that can go in a manifest, when no manifest
/// declares the package, or when a manifest cannot be rewritten.
pub(crate) fn set(
    cwd: &Utf8Path,
    ui: &mut Ui,
    name: &str,
    range: &str,
    dry_run: bool,
) -> Result<()> {
    // The name reaches a package manager's argv and the range reaches a JSON
    // string in a file the project cannot install without; both are checked
    // before either is written.
    uf_pm::check_operands(std::slice::from_ref(&name.to_owned()))?;
    if range.trim().is_empty() || range.chars().any(char::is_control) {
        bail!("`{range}` is not a range that can go in a manifest");
    }

    let resolved = load_config(cwd)?;
    let project = project_label(&resolved.root).to_string();
    let declared = uf_pm::manifests::declarations(&resolved.root)?;
    let affected: Vec<&Declaration> = declared
        .iter()
        .filter(|declaration| declaration.name == name)
        .collect();
    if affected.is_empty() {
        bail!("no manifest in this workspace declares `{name}`");
    }

    let mut moving: Vec<(String, CompactString)> = Vec::new();
    let mut changes: BTreeMap<Utf8PathBuf, Changes> = BTreeMap::new();
    for declaration in &affected {
        if declaration.range == range {
            continue;
        }
        moving.push((
            relative(&resolved.root, &declaration.manifest),
            declaration.range.clone(),
        ));
        changes
            .entry(declaration.manifest.clone())
            .or_default()
            .insert(
                (declaration.field, declaration.name.clone()),
                range.to_compact_string(),
            );
    }

    let rows: Vec<Vec<Cell<'_>>> = moving
        .iter()
        .take(ROWS_SHOWN)
        .map(|(manifest, was)| {
            vec![
                Cell::toned(manifest.as_str(), Tone::Path),
                Cell::toned(was.as_str(), Tone::Muted),
                Cell::toned(range, Tone::Number),
            ]
        })
        .collect();
    let nothing = moving.is_empty();
    let name_owned = name.to_owned();
    let summary = format!(
        "{} in {}",
        plural(moving.len(), "declaration"),
        plural(changes.len(), "manifest")
    );
    ui.render(|renderer, out| {
        renderer.banner(out, "uf catalog set", Some(&project));
        renderer.blank(out);
        if nothing {
            renderer.status(
                out,
                Status::Success,
                &format!("every manifest already declares {name_owned} at {range}"),
            );
            return;
        }
        let mut table = Table::new(vec![
            Column::left("manifest"),
            Column::left("was"),
            Column::left("becomes"),
        ]);
        for row in &rows {
            table.push(row.clone());
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        if dry_run {
            renderer.status(
                out,
                Status::Info,
                &format!("{summary} would change; run without --dry-run"),
            );
        }
    });

    if nothing || dry_run {
        return Ok(());
    }

    // All of them or none. A failure part way through would leave the very thing
    // this command exists to prevent — some manifests on the new range and some
    // on the old — and then install it. See `uf_pm::manifests::apply_all`.
    uf_pm::manifests::apply_all(&changes)
        .with_context(|| format!("could not rewrite {}", project_label(&resolved.root)))?;
    ui.render(|renderer, out| {
        renderer.status(out, Status::Success, &format!("wrote {summary}"));
        renderer.blank(out);
    });

    // The manifests say something new, so the lockfile has to be made to agree.
    super::deps::delegate(
        cwd,
        ui,
        &super::deps::Request {
            heading: "uf catalog set",
            operation: Operation::Install,
            operands: &[],
            retry: "uf install".to_owned(),
            announced: true,
        },
    )
}

/// One package more than one manifest declares.
struct Entry {
    name: CompactString,
    /// Every manifest that declares it, and at what.
    declared: Vec<(String, CompactString)>,
}

impl Entry {
    /// The one range they all say, when they all say one.
    fn agreed(&self) -> Option<&CompactString> {
        let first = &self.declared.first()?.1;
        self.declared
            .iter()
            .all(|(_, range)| range == first)
            .then_some(first)
    }
}

/// What the whole command has to say.
struct Catalogue {
    entries: Vec<Entry>,
    manifests: usize,
    /// Declarations that are pnpm's `catalog:`, which pnpm resolves itself.
    pnpm_catalog: usize,
}

/// Every package that more than one manifest declares.
///
/// One manifest declaring a package is not a catalogue entry — there is nothing
/// for it to agree with, and a workspace's whole dependency list is not a
/// catalogue. Two is where the question "do these say the same thing" starts
/// having an answer.
fn entries(root: &Utf8Path, declared: &[Declaration]) -> Catalogue {
    let mut by_name: FxHashMap<CompactString, Vec<(String, CompactString)>> = FxHashMap::default();
    let mut manifests: Vec<&Utf8PathBuf> = Vec::new();
    let mut pnpm_catalog = 0;
    for declaration in declared {
        if !manifests.contains(&&declaration.manifest) {
            manifests.push(&declaration.manifest);
        }
        if declaration.range.starts_with("catalog:") {
            pnpm_catalog += 1;
        }
        by_name.entry(declaration.name.clone()).or_default().push((
            relative(root, &declaration.manifest),
            declaration.range.clone(),
        ));
    }

    let mut entries: Vec<Entry> = by_name
        .into_iter()
        .filter(|(_, declared)| declared.len() > 1)
        .map(|(name, mut declared)| {
            declared.sort();
            Entry { name, declared }
        })
        .collect();
    // Disagreements first, then by name: the rows that need a decision are the
    // rows worth putting at the top.
    entries.sort_by(|left, right| {
        left.agreed()
            .is_some()
            .cmp(&right.agreed().is_some())
            .then_with(|| left.name.cmp(&right.name))
    });
    Catalogue {
        entries,
        manifests: manifests.len(),
        pnpm_catalog,
    }
}

/// Draw it.
fn render(renderer: &uf_term::Renderer, out: &mut String, catalogue: &Catalogue) {
    if catalogue.manifests < 2 {
        renderer.status(
            out,
            Status::Info,
            "one manifest, so there is nothing for a catalogue to keep in step",
        );
        render_pnpm(renderer, out, catalogue);
        return;
    }
    if catalogue.entries.is_empty() {
        renderer.status(
            out,
            Status::Success,
            &format!(
                "no package is declared by more than one of this workspace's {}",
                plural(catalogue.manifests, "manifest")
            ),
        );
        render_pnpm(renderer, out, catalogue);
        return;
    }

    let disagreeing: Vec<&Entry> = catalogue
        .entries
        .iter()
        .filter(|entry| entry.agreed().is_none())
        .collect();

    renderer.heading(out, 2, "shared");
    renderer.blank(out);
    let counts: Vec<String> = catalogue
        .entries
        .iter()
        .map(|entry| plural(entry.declared.len(), "package"))
        .collect();
    let mut table = Table::new(vec![
        Column::left("package"),
        Column::left("range"),
        Column::left("declared in"),
    ]);
    for (entry, count) in catalogue.entries.iter().take(ROWS_SHOWN).zip(&counts) {
        let (range, tone) = match entry.agreed() {
            Some(range) => (range.as_str(), Tone::Muted),
            // Not a range, because there isn't one — that is the finding, and
            // the disagreements table below says what they are.
            None => ("differs", Tone::Warn),
        };
        table.push(vec![
            Cell::new(entry.name.as_str()),
            Cell::toned(range, tone),
            Cell::toned(count, Tone::Number),
        ]);
    }
    renderer.table(out, 2, &table);
    if catalogue.entries.len() > ROWS_SHOWN {
        renderer.blank(out);
        renderer.status(
            out,
            Status::Info,
            &format!(
                "and {} more; --json prints all of them",
                catalogue.entries.len() - ROWS_SHOWN
            ),
        );
    }
    renderer.blank(out);

    if !disagreeing.is_empty() {
        renderer.heading(out, 2, "disagreements");
        renderer.blank(out);
        let mut table = Table::new(vec![
            Column::left("package"),
            Column::left("manifest"),
            Column::left("range"),
        ]);
        for entry in disagreeing.iter().take(ROWS_SHOWN) {
            for (manifest, range) in &entry.declared {
                table.push(vec![
                    Cell::new(entry.name.as_str()),
                    Cell::toned(manifest.as_str(), Tone::Path),
                    Cell::toned(range.as_str(), Tone::Warn),
                ]);
            }
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Warn,
            &format!(
                "{} declared at more than one range",
                plural(disagreeing.len(), "package")
            ),
        );
        renderer.status(
            out,
            Status::Info,
            &format!(
                "uf catalog set {} <range> makes every manifest agree",
                disagreeing[0].name
            ),
        );
    } else {
        renderer.status(
            out,
            Status::Success,
            &format!(
                "{} shared, and every manifest agrees on all of them",
                plural(catalogue.entries.len(), "package")
            ),
        );
    }
    render_pnpm(renderer, out, catalogue);
}

/// pnpm's own catalogue, when the project has one.
fn render_pnpm(renderer: &uf_term::Renderer, out: &mut String, catalogue: &Catalogue) {
    if catalogue.pnpm_catalog == 0 {
        return;
    }
    renderer.status(
        out,
        Status::Info,
        &format!(
            "{} written as pnpm's `catalog:`, which pnpm resolves itself",
            plural(catalogue.pnpm_catalog, "declaration")
        ),
    );
}

/// A manifest as the reader knows it: `.` for the root, else its directory.
fn relative(root: &Utf8Path, manifest: &Utf8Path) -> String {
    let directory = manifest.parent().unwrap_or(manifest);
    match directory.strip_prefix(root) {
        Ok(path) if path.as_str().is_empty() => ".".to_owned(),
        Ok(path) => path.to_string(),
        Err(_) => directory.to_string(),
    }
}

#[cfg(test)]
mod tests;
