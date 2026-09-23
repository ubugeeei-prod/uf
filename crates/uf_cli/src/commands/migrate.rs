//! Reviewable adoption and versioned source migrations, without executing configs.
mod adopt;
mod codemod;
mod source;
#[cfg(test)]
mod tests;
mod ui_copies;
mod ui_namespaces;

use crate::ui::Ui;
use anyhow::{Context, Result, ensure};
use camino::Utf8Path;
use serde::Serialize;
use std::fs;

#[derive(Debug, Serialize)]
struct Change {
    path: String,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct Plan {
    command: String,
    from: Option<String>,
    to: Option<String>,
    migrations: Vec<String>,
    changes: Vec<Change>,
    unmapped: Vec<String>,
}

impl Plan {
    fn new(command: &str) -> Self {
        Self {
            command: command.into(),
            from: None,
            to: None,
            migrations: Vec::new(),
            changes: Vec::new(),
            unmapped: Vec::new(),
        }
    }
    fn write(&mut self, path: &str, before: String, after: String) {
        if before != after {
            self.changes.push(Change {
                path: path.into(),
                before: Some(before),
                after: Some(after),
            });
        }
    }
    fn create(&mut self, path: &str, after: String) {
        self.changes.push(Change {
            path: path.into(),
            before: None,
            after: Some(after),
        });
    }
    fn remove(&mut self, path: &str, before: String) {
        self.changes.push(Change {
            path: path.into(),
            before: Some(before),
            after: None,
        });
    }

    fn apply(&self, root: &Utf8Path) -> Result<()> {
        // Check the entire plan before writing a byte, including symlinks and
        // edits made between planning and application.
        for change in &self.changes {
            let path = root.join(&change.path);
            let metadata = fs::symlink_metadata(&path);
            ensure!(
                !metadata.as_ref().is_ok_and(|m| m.file_type().is_symlink()),
                "{} is a symlink; migrate the real project instead",
                change.path
            );
            ensure!(
                fs::read_to_string(&path).ok() == change.before,
                "{} changed while planning; rerun --dry-run",
                change.path
            );
        }
        ensure!(
            !fs::symlink_metadata(root.join(".uf")).is_ok_and(|m| m.file_type().is_symlink()),
            ".uf is a symlink; report must stay in this project"
        );
        let report = root.join(".uf/migration-report.json");
        ensure!(
            !fs::symlink_metadata(&report).is_ok_and(|m| m.file_type().is_symlink()),
            "migration report is a symlink"
        );
        fs::create_dir_all(report.parent().unwrap())?;
        fs::write(&report, serde_json::to_string_pretty(self)?)?;
        for (index, change) in self.changes.iter().enumerate() {
            let path = root.join(&change.path);
            let result = match &change.after {
                Some(text) => fs::write(&path, text),
                None => fs::remove_file(&path),
            };
            if let Err(error) = result {
                for previous in self.changes[..=index].iter().rev() {
                    let path = root.join(&previous.path);
                    match &previous.before {
                        Some(text) => {
                            fs::write(path, text).context("migration failed and rollback could not restore a file; originals are in .uf/migration-report.json")?;
                        }
                        None => {
                            if path.exists() {
                                fs::remove_file(path)?;
                            }
                        }
                    }
                }
                return Err(error.into());
            }
        }
        Ok(())
    }
}

pub(crate) fn migrate(root: &Utf8Path, ui: &mut Ui, dry_run: bool) -> Result<()> {
    finish(root, ui, adopt::plan(root)?, dry_run)
}

pub(crate) fn codemod(
    root: &Utf8Path,
    ui: &mut Ui,
    from: Option<&str>,
    to: &str,
    dry_run: bool,
) -> Result<()> {
    finish(root, ui, codemod::plan(root, from, to)?, dry_run)
}

fn finish(root: &Utf8Path, ui: &mut Ui, plan: Plan, dry_run: bool) -> Result<()> {
    if !dry_run {
        plan.apply(root)?;
    }
    if ui.is_json() {
        ui.json(&serde_json::json!({ "dryRun": dry_run, "plan": plan }))?;
    } else {
        ui.plain(&format!(
            "uf {}{}\n",
            plan.command,
            if dry_run { " --dry-run" } else { "" }
        ));
        for change in &plan.changes {
            ui.plain(&format!(
                "\n{} {}\n",
                if change.after.is_some() {
                    "write"
                } else {
                    "remove"
                },
                change.path
            ));
            if let Some(text) = &change.after {
                ui.plain(text);
            }
        }
        for item in &plan.unmapped {
            ui.plain(&format!("\nmanual: {item}\n"));
        }
        if !dry_run {
            ui.plain("\nReport and original contents: .uf/migration-report.json\n");
        }
    }
    Ok(())
}
