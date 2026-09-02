//! `uf fmt`: which files changed, and whether the check passed.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use uf_config::load_config;
use uf_fmt::format_source;
use uf_project::collect_source_files;
use uf_term::Status;

use crate::support::plural;
use crate::ui::Ui;

pub(crate) fn fmt(cwd: &Utf8Path, ui: &mut Ui, check: bool) -> Result<()> {
    let resolved = load_config(cwd)?;
    // Discovery returns `package.json` too, because the linter reads it. The
    // formatter must not touch it: it is a JavaScript formatter, and running it
    // over JSON inserts a statement terminator and leaves the file unparseable.
    let files = collect_source_files(&resolved.root, &resolved.config)?
        .into_iter()
        .filter(|file| file.kind.is_formattable())
        .collect::<Vec<_>>();
    let scanned = files.len();
    let mut changed = Vec::new();

    for file in files {
        let result = format_source(&file.source, &resolved.config.fmt)?;
        if result.changed {
            if !check {
                fs::write(&file.absolute_path, result.output)
                    .with_context(|| format!("failed to write {}", file.absolute_path))?;
            }
            changed.push(file.relative_path);
        }
    }

    let paths = changed.iter().map(String::as_str).collect::<Vec<_>>();
    let failing = check && !changed.is_empty();
    let summary = if check {
        format!(
            "{} of {} {} formatting",
            plural(changed.len(), "file"),
            scanned,
            if changed.len() == 1 { "needs" } else { "need" }
        )
    } else {
        format!("formatted {} of {}", plural(changed.len(), "file"), scanned)
    };

    ui.render(|renderer, out| {
        renderer.banner(out, "uf fmt", None);
        renderer.blank(out);
        if paths.is_empty() {
            renderer.status(out, Status::Success, "every file is already formatted");
        } else {
            renderer.bullet_list(out, 2, &paths);
            renderer.blank(out);
            renderer.status(
                out,
                if failing {
                    Status::Warn
                } else {
                    Status::Success
                },
                &summary,
            );
        }
    });

    if failing {
        bail!("{} need formatting", plural(changed.len(), "file"));
    }
    Ok(())
}
