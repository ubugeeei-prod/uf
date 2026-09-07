//! `uf update`: the two different things people mean by it.
//!
//! # The two
//!
//! Every package manager's `update` moves a dependency to the newest version
//! **its declared range already allows**. A project on `^18.2.0` with `19.2.0`
//! published gets `18.3.1`, and is told nothing about the 19.
//!
//! That is a defensible thing for a package manager to do, and it is not what
//! most people mean when they say "update my dependencies". What they mean is
//! the other one: rewrite the range in `package.json`, then install.
//!
//! So `uf update` does the first and *reports* the second:
//!
//! ```text
//!   outside the range
//!
//!   package   declared   newest   step
//!   react     ^18.2.0    19.2.0   major
//! ```
//!
//! and `--latest`, `--minor` and `--patch` do the second — rewriting the
//! manifests to the newest version at or below that level, then installing.
//!
//! # Why this one is uf's own rather than a passthrough
//!
//! `uf add` delegates, `uf remove` delegates, `uf install` delegates; the red
//! line is that uf orchestrates and does not reimplement. This command still
//! delegates the part that installs. What it does not delegate is the part no
//! manager has: npm, pnpm, yarn and bun each report *outdated* differently and
//! none of them rewrites a range. A `uf update --latest` that shelled out to
//! `npm outdated --json` would mean something different in a pnpm project, and
//! nothing at all in a bun one.
//!
//! Reading a packument is not installing — see [`uf_pm::registry`] for why that
//! distinction is the one that matters.
//!
//! # What it will not rewrite
//!
//! `workspace:*`, `catalog:`, `npm:`, `file:`, a git URL, a compound range: uf
//! reports the count and leaves them, because each of them says something a
//! single comparator cannot restate. [`uf_pm::ranges`] has the argument.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use uf_config::load_config;
use uf_pm::detect::Version;
use uf_pm::manifests::{Changes, Declaration};
use uf_pm::ranges::{Level, Range};
use uf_pm::{Operation, registry};
use uf_term::{Cell, Column, Status, Table, Tone};

use crate::support::{plural, project_label};
use crate::ui::Ui;

/// How many rows the report prints before it stops listing them.
///
/// A project with two hundred outdated dependencies has a spreadsheet problem,
/// not a reading problem, and `--json` is the answer for it.
const ROWS_SHOWN: usize = 40;

/// `uf update [PACKAGE...] [--latest|--minor|--patch] [--dry-run]`.
///
/// # Errors
///
/// When the project cannot be read, when a manifest cannot be rewritten, or
/// when the manager's own command fails. A registry that cannot be reached is
/// *not* an error: the report says so and the command still succeeds, because
/// by then the manager's update has already run and failing after a successful
/// mutation would be the worst of both.
pub(crate) fn update(
    cwd: &Utf8Path,
    ui: &mut Ui,
    packages: &[String],
    level: Option<Level>,
    dry_run: bool,
) -> Result<()> {
    uf_pm::check_operands(packages)?;
    let resolved = load_config(cwd)?;
    let root = resolved.root.clone();
    let project = project_label(&root).to_string();

    // Read before anything runs. After the manager's update the manifests are
    // unchanged — it does not touch them — but the reading has to happen on
    // this side of the rewrite for the level path, and doing it once keeps the
    // two paths reporting the same numbers.
    let declared = uf_pm::manifests::declarations(&root)?;
    let wanted = requested(&declared, packages);

    // The plain form runs the manager first, so its output is on the screen
    // before uf's own report — the report is the new thing, and the new thing
    // goes last.
    if level.is_none() && !dry_run {
        super::deps::update(cwd, ui, packages)?;
    }

    // The same registry `uf.lock` records, through the same inference — so the
    // versions this report is about are the versions `uf install` would resolve
    // against. uf has one registry setting and it lives under `publish`, which
    // is the wrong place for a value that is also read from; see
    // ubugeeei-prod/uf#540.
    let registry_url = uf_pm::PackageManagerPlan::infer_from_config(&resolved.config).registry;
    let names: Vec<CompactString> = wanted
        .iter()
        .filter(|declaration| Range::parse(&declaration.range).is_some())
        .map(|declaration| declaration.name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let published = registry::packuments(&registry_url, &names);

    let mut rows = Vec::new();
    let mut changes: BTreeMap<Utf8PathBuf, Changes> = BTreeMap::new();
    let mut undecidable = 0;
    let mut unreachable = Vec::new();
    for declaration in &wanted {
        let Some(range) = Range::parse(&declaration.range) else {
            undecidable += 1;
            continue;
        };
        match published.get(&declaration.name) {
            None | Some(Err(_)) => {
                if let Some(Err(error)) = published.get(&declaration.name) {
                    unreachable.push(format!("{}: {error}", declaration.name));
                }
                continue;
            }
            Some(Ok(packument)) => {
                let Some(newest) = uf_pm::ranges::best(
                    &packument.versions,
                    &range.base,
                    level.unwrap_or(Level::Major),
                ) else {
                    continue;
                };
                // Inside the range is the manager's job, and it has either
                // already done it or is about to. This report is the other one.
                if level.is_none() && range.allows(newest) {
                    continue;
                }
                let step = uf_pm::ranges::step(&range.base, newest).unwrap_or(Level::Patch);
                let rewritten = range.rewritten_to(newest);
                if level.is_some() {
                    changes
                        .entry(declaration.manifest.clone())
                        .or_default()
                        .insert(
                            (declaration.field, declaration.name.clone()),
                            rewritten.clone(),
                        );
                }
                rows.push(Row {
                    manifest: relative(&root, &declaration.manifest),
                    name: declaration.name.clone(),
                    declared: declaration.range.clone(),
                    rewritten,
                    newest: newest.clone(),
                    step,
                });
            }
        }
    }
    rows.sort_by(|left, right| {
        right
            .step
            .cmp(&left.step)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.manifest.cmp(&right.manifest))
    });

    let report = Report {
        level,
        dry_run,
        rows,
        undecidable,
        unreachable,
        asked_about: names.len(),
        manifests: declared
            .iter()
            .map(|declaration| &declaration.manifest)
            .collect::<BTreeSet<_>>()
            .len(),
    };
    let banner = level.is_some() || dry_run;
    ui.render(|renderer, out| {
        if banner {
            renderer.banner(out, "uf update", Some(&project));
        }
        renderer.blank(out);
        render(renderer, out, &report);
    });

    if level.is_none() || dry_run || changes.is_empty() {
        return Ok(());
    }

    let mut written = 0;
    for (manifest, changes) in &changes {
        written += uf_pm::manifests::apply(manifest, changes)
            .with_context(|| format!("could not rewrite {}", relative(&root, manifest)))?;
    }
    let summary = format!(
        "{} in {}",
        plural(written, "range"),
        plural(changes.len(), "manifest")
    );
    ui.render(|renderer, out| {
        renderer.status(out, Status::Success, &format!("rewrote {summary}"));
        renderer.blank(out);
    });

    // The manifests say something new, so the lockfile and the tree have to be
    // made to agree with them. A `--latest` that left the install behind would
    // have changed a file and nothing else.
    super::deps::delegate(
        cwd,
        ui,
        &super::deps::Request {
            heading: "uf update",
            operation: Operation::Install,
            operands: &[],
            retry: "uf install".to_owned(),
            announced: true,
        },
    )
}

/// One dependency that could move.
struct Row {
    manifest: String,
    name: CompactString,
    declared: CompactString,
    rewritten: CompactString,
    newest: Version,
    step: Level,
}

/// Everything the command has to say.
struct Report {
    level: Option<Level>,
    dry_run: bool,
    rows: Vec<Row>,
    /// Ranges uf reads but does not rewrite.
    undecidable: usize,
    /// One line per package the registry did not answer for.
    unreachable: Vec<String>,
    asked_about: usize,
    manifests: usize,
}

/// Draw it.
///
/// Split out from the command so a test can render it against
/// [`uf_term::Capabilities::plain`] and read the layout, rather than asserting
/// on whatever npm published this morning.
fn render(renderer: &uf_term::Renderer, out: &mut String, report: &Report) {
    if report.rows.is_empty() {
        let line = match report.level {
            _ if report.asked_about == 0 => {
                "no dependency declares a range uf can compare against a registry".to_owned()
            }
            None => "every dependency's newest version is inside its declared range".to_owned(),
            Some(level) => format!("no range would move at the {level} level"),
        };
        renderer.status(out, Status::Success, &line);
        render_notes(renderer, out, report);
        return;
    }

    renderer.heading(
        out,
        2,
        if report.level.is_some() {
            "ranges to rewrite"
        } else {
            "outside the range"
        },
    );
    renderer.blank(out);

    // The manifest column earns its width only in a workspace that has more
    // than one; in a single-package project every row would say the same thing.
    let many = report.manifests > 1;
    let mut columns = vec![Column::left("package"), Column::left("declared")];
    if report.level.is_some() {
        columns.push(Column::left("becomes"));
    } else {
        columns.push(Column::left("newest"));
    }
    columns.push(Column::left("step"));
    if many {
        columns.push(Column::left("manifest"));
    }
    let mut table = Table::new(columns);

    let newest: Vec<String> = report
        .rows
        .iter()
        .map(|row| row.newest.to_string())
        .collect();
    for (row, newest) in report.rows.iter().take(ROWS_SHOWN).zip(&newest) {
        let mut cells = vec![
            Cell::new(row.name.as_str()),
            Cell::toned(row.declared.as_str(), Tone::Muted),
        ];
        cells.push(if report.level.is_some() {
            Cell::toned(row.rewritten.as_str(), Tone::Number)
        } else {
            Cell::toned(newest, Tone::Number)
        });
        cells.push(Cell::toned(
            row.step.name(),
            // A major is the one that needs a changelog read, so it is the one
            // that is not muted.
            if row.step == Level::Major {
                Tone::Warn
            } else {
                Tone::Muted
            },
        ));
        if many {
            cells.push(Cell::toned(row.manifest.as_str(), Tone::Path));
        }
        table.push(cells);
    }
    renderer.table(out, 2, &table);
    if report.rows.len() > ROWS_SHOWN {
        renderer.blank(out);
        renderer.status(
            out,
            Status::Info,
            &format!(
                "and {} more; --json prints all of them",
                report.rows.len() - ROWS_SHOWN
            ),
        );
    }
    renderer.blank(out);

    let majors = report
        .rows
        .iter()
        .filter(|row| row.step == Level::Major)
        .count();
    match report.level {
        None => {
            renderer.status(
                out,
                Status::Info,
                &format!(
                    "{} {} newer than {} range allows",
                    plural(report.rows.len(), "dependency"),
                    if report.rows.len() == 1 { "is" } else { "are" },
                    if report.rows.len() == 1 {
                        "its"
                    } else {
                        "their"
                    }
                ),
            );
            renderer.status(
                out,
                Status::Info,
                "uf update --latest rewrites the ranges; --minor and --patch cap the step",
            );
        }
        Some(_) if report.dry_run => {
            renderer.status(
                out,
                Status::Info,
                &format!(
                    "{} would be rewritten; run without --dry-run",
                    plural(report.rows.len(), "range")
                ),
            );
        }
        Some(_) => {
            if majors > 0 {
                renderer.status(
                    out,
                    Status::Warn,
                    &format!(
                        "{} of these is a major; read what changed before you ship it",
                        majors
                    ),
                );
            }
        }
    }
    render_notes(renderer, out, report);
}

/// The two things that are true whether or not anything moved.
fn render_notes(renderer: &uf_term::Renderer, out: &mut String, report: &Report) {
    if report.undecidable > 0 {
        renderer.status(
            out,
            Status::Info,
            &format!(
                "{} left alone: workspace:, catalog:, npm:, a URL, or a range with more than one comparator",
                plural(report.undecidable, "range")
            ),
        );
    }
    if report.unreachable.is_empty() {
        return;
    }
    // Named, not counted: a registry that answered for forty packages and not
    // for one is a different situation from one that answered for none, and the
    // reader can only tell from the name.
    renderer.status(
        out,
        Status::Warn,
        &format!(
            "{} could not be read from the registry",
            plural(report.unreachable.len(), "package")
        ),
    );
    for line in report.unreachable.iter().take(3) {
        renderer.status(out, Status::Info, line);
    }
}

/// The declarations this run is about.
///
/// Naming packages narrows it, the way `uf update react` narrows the manager's
/// own update. A name that nothing declares is not an error — it is a name that
/// contributes no rows, and the empty report says so.
fn requested<'a>(declared: &'a [Declaration], packages: &[String]) -> Vec<&'a Declaration> {
    if packages.is_empty() {
        return declared.iter().collect();
    }
    let named: BTreeSet<CompactString> = packages
        .iter()
        .map(|package| package.as_str().to_compact_string())
        .collect();
    declared
        .iter()
        .filter(|declaration| named.contains(&declaration.name))
        .collect()
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
