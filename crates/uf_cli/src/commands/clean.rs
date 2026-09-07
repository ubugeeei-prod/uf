//! `uf clean`: remove what a rebuild would write again.
//!
//! # The boundary, which is the whole of the design
//!
//! Four kinds of thing accumulate in a uf project, and they cost different
//! amounts to lose:
//!
//! | | costs | removed by |
//! | --- | --- | --- |
//! | `dist/` — the build's output | a rebuild | `uf clean` |
//! | `.uf/cache/` — transform and check answers | a rebuild, slower | `uf clean` |
//! | `.uf/` — the rest of uf's per-project state | a re-resolve | `uf clean` |
//! | `node_modules/` and the lockfile's store | a network round trip | `--deps` |
//!
//! The line is drawn at the network. Everything `uf clean` removes by default,
//! this checkout can produce again on its own; `node_modules/` cannot, and a
//! command that deleted it because somebody wanted their `dist/` gone would be
//! a command that turned a two-second mistake into a two-minute one on a good
//! connection and an impossible one on a train.
//!
//! `uf.lock` and `package-lock.json` are never removed at all, by any flag.
//! They are inputs — checked in, reviewed, and the reason an install is
//! reproducible — and no amount of cleaning should make the next install
//! resolve something different from the last one.
//!
//! # What it prints
//!
//! What it removed and how much, which is the one thing a command that deletes
//! owes its reader. `--dry-run` prints the same table and removes nothing, so
//! the answer to "what would this take" is available before it is taken.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use uf_bundle::size::ByteSize;
use uf_config::load_config;
use uf_term::{Cell, Column, Status, Table, Tone};

use crate::support::{plural, project_label};
use crate::ui::Ui;

/// One directory `uf clean` considered.
struct Target {
    /// Project-relative, as the reader knows it.
    label: String,
    path: Utf8PathBuf,
    /// What losing it costs, in the reader's terms rather than uf's.
    cost: &'static str,
    bytes: u64,
    files: u64,
}

/// `uf clean [--deps] [--dry-run]`.
pub(crate) fn clean(cwd: &Utf8Path, ui: &mut Ui, deps: bool, dry_run: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    let root = &resolved.root;

    let mut targets = Vec::new();
    // The build's output first, because it is what most readers mean.
    push(
        &mut targets,
        root,
        &resolved.config.build.out_dir,
        "a rebuild",
    );
    // The documentation site writes under `dist/` by default, and under its
    // own directory when a project moved it. Pushed separately and skipped
    // when it is already inside one that was pushed, so a nested directory is
    // not counted twice.
    push(
        &mut targets,
        root,
        &resolved.config.docs.out_dir,
        "a docs rebuild",
    );
    push(&mut targets, root, ".uf", "re-resolving and re-checking");
    if deps {
        push(&mut targets, root, "node_modules", "an install");
    }

    let removed = targets
        .iter()
        .filter(|target| target.bytes > 0 || target.files > 0)
        .collect::<Vec<_>>();

    let project = project_label(root).to_string();
    if removed.is_empty() {
        ui.render(|renderer, out| {
            renderer.banner(out, "uf clean", Some(&project));
            renderer.blank(out);
            renderer.status(out, Status::Success, "nothing to remove");
        });
        return Ok(());
    }

    let total_bytes: u64 = removed.iter().map(|target| target.bytes).sum();
    let total_files: u64 = removed.iter().map(|target| target.files).sum();
    // Owned first, borrowed in the render: `Cell` borrows its text, and the
    // renderer runs inside a closure the locals must outlive.
    let cells = removed
        .iter()
        .map(|target| {
            (
                target.label.clone(),
                plural(target.files as usize, "file"),
                ByteSize::from_bytes(target.bytes).to_string(),
            )
        })
        .collect::<Vec<_>>();
    let rows = cells
        .iter()
        .zip(&removed)
        .map(|((label, files, size), target)| {
            vec![
                Cell::toned(label, Tone::Path),
                Cell::toned(files, Tone::Number),
                Cell::toned(size, Tone::Number),
                Cell::toned(target.cost, Tone::Muted),
            ]
        })
        .collect::<Vec<_>>();

    let summary = format!(
        "{} in {}",
        ByteSize::from_bytes(total_bytes),
        plural(total_files as usize, "file")
    );
    let dry = dry_run;
    ui.render(|renderer, out| {
        renderer.banner(out, "uf clean", Some(&project));
        renderer.blank(out);
        let mut table = Table::new(vec![
            Column::left("directory"),
            Column::right("files"),
            Column::right("size"),
            Column::left("costs"),
        ]);
        for row in &rows {
            table.push(row.clone());
        }
        renderer.table(out, 2, &table);
        renderer.blank(out);
        if dry {
            renderer.status(
                out,
                Status::Info,
                &format!("{summary} would be removed; run without --dry-run"),
            );
        }
    });

    if dry_run {
        return Ok(());
    }

    for target in &removed {
        std::fs::remove_dir_all(&target.path)
            .with_context(|| format!("could not remove {}", target.label))?;
    }

    ui.render(|renderer, out| {
        renderer.status(out, Status::Success, &format!("removed {summary}"));
    });
    Ok(())
}

/// Add `relative` to the list, measured, unless it is not there or is already
/// inside something else on the list.
///
/// The nesting check is what keeps `dist/docs` from being counted and then
/// removed a second time under `dist/` — where the second removal is not an
/// error but the size in the table would be wrong, which is the number the
/// reader is deciding on.
fn push(targets: &mut Vec<Target>, root: &Utf8Path, relative: &str, cost: &'static str) {
    let path = root.join(relative);
    if !path.is_dir() {
        return;
    }
    if targets
        .iter()
        .any(|target| path.starts_with(&target.path) || target.path.starts_with(&path))
    {
        return;
    }
    let (bytes, files) = measure(&path);
    targets.push(Target {
        label: relative.to_owned(),
        path,
        cost,
        bytes,
        files,
    });
}

/// The size and file count under `path`.
///
/// Unreadable entries are skipped rather than reported: this number exists to
/// tell a reader roughly what they are about to lose, and a directory uf cannot
/// stat is one `remove_dir_all` will complain about in a moment with a better
/// message than a walk could.
fn measure(path: &Utf8Path) -> (u64, u64) {
    let mut bytes = 0;
    let mut files = 0;
    for entry in walkdir::WalkDir::new(path).into_iter().flatten() {
        if let Ok(metadata) = entry.metadata()
            && metadata.is_file()
        {
            bytes += metadata.len();
            files += 1;
        }
    }
    (bytes, files)
}

#[cfg(test)]
mod tests;
