//! `uf fmt`: which files changed, which could not be read, and whether the
//! check passed.

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
    // formatter must not touch it: it is a Flow formatter, and running it
    // over JSON inserts a statement terminator and leaves the file unparseable.
    let files = collect_source_files(&resolved.root, &resolved.config)?
        .into_iter()
        .filter(|file| file.kind.is_formattable())
        .collect::<Vec<_>>();
    let scanned = files.len();
    let mut changed = Vec::new();
    let mut skipped = Vec::new();

    for file in files {
        // A file the parser refuses is reported and left exactly as it is:
        // the formatter prints from a syntax tree, and there is no tree to
        // print when the source does not parse. Rewriting the parser's
        // guess at what was meant would lose code.
        let result = match format_source(&file.source, &resolved.config.fmt) {
            Ok(result) => result,
            Err(error) => {
                skipped.push(format!("{}: {error}", file.relative_path));
                continue;
            }
        };
        if result.changed {
            if !check {
                fs::write(&file.absolute_path, result.output)
                    .with_context(|| format!("failed to write {}", file.absolute_path))?;
            }
            changed.push(file.relative_path);
        }
    }

    let paths = changed.iter().map(String::as_str).collect::<Vec<_>>();
    let skipped_paths = skipped.iter().map(String::as_str).collect::<Vec<_>>();
    let failing = (check && !changed.is_empty()) || !skipped.is_empty();
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
        if paths.is_empty() && skipped_paths.is_empty() {
            renderer.status(out, Status::Success, "every file is already formatted");
        } else {
            if !paths.is_empty() {
                renderer.bullet_list(out, 2, &paths);
                renderer.blank(out);
            }
            if !skipped_paths.is_empty() {
                renderer.status(
                    out,
                    Status::Warn,
                    &format!(
                        "{} could not be parsed",
                        plural(skipped_paths.len(), "file")
                    ),
                );
                renderer.bullet_list(out, 2, &skipped_paths);
                renderer.blank(out);
            }
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

    if !skipped.is_empty() {
        bail!("{} could not be parsed", plural(skipped.len(), "file"));
    }
    if failing {
        bail!("{} need formatting", plural(changed.len(), "file"));
    }
    Ok(())
}
