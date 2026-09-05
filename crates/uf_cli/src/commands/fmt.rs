//! `uf fmt`: which files changed, which could not be read, and whether the
//! check passed.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use uf_config::load_config;
use uf_fmt::format_source;
use uf_project::scan_source_files;
use uf_term::Status;

use crate::support::{plural, quoted_list, selects, unreadable_lines};
use crate::ui::Ui;

pub(crate) fn fmt(cwd: &Utf8Path, ui: &mut Ui, check: bool, paths: &[String]) -> Result<()> {
    let resolved = load_config(cwd)?;
    // Discovery returns `package.json` too, because the linter reads it. The
    // formatter must not touch it: it is a Flow formatter, and running it
    // over JSON inserts a statement terminator and leaves the file unparseable.
    let mut scan = scan_source_files(&resolved.root, &resolved.config)?;
    // Narrowed before anything is reported: a file outside what was asked
    // about must not fail the run for being unreadable either.
    scan.unreadable
        .retain(|failure| selects(paths, &failure.relative_path));
    let unreadable = unreadable_lines(&scan.unreadable);
    let discovered = scan
        .files
        .into_iter()
        .filter(|file| selects(paths, &file.relative_path))
        .collect::<Vec<_>>();
    if discovered.is_empty() && !paths.is_empty() && unreadable.is_empty() {
        bail!("no file matched {}", quoted_list(paths));
    }
    // Two piles, because they go to two different formatters. uf prints Flow
    // from the official parser's syntax tree; JSON, CSS and TypeScript go to a
    // formatter that understands them, which uf runs rather than writes.
    let non_flow = discovered
        .iter()
        .filter(|file| file.kind.is_non_flow_formattable())
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    let files = discovered
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

    // The other formatter, over the other pile. A failure here is reported
    // beside uf's own rather than raised: a project whose Biome is missing
    // should still learn what uf's formatter found.
    let formatter = resolved.config.fmt.non_flow.formatter;
    let formatter_name = formatter.as_str();
    let mut non_flow_unformatted = false;
    // Kept apart from `skipped`, which is "the parser refused this file". A
    // formatter that is not installed is a different problem with a different
    // fix, and folding the two together reported a missing binary as an
    // unparseable file.
    let mut non_flow_failure = None;
    match uf_fmt::non_flow::run(
        formatter,
        &resolved.root,
        &non_flow,
        check,
        &resolved.config.fmt,
    ) {
        Ok(formatted) => non_flow_unformatted = !formatted,
        Err(error) => non_flow_failure = Some(error.to_string()),
    }

    let paths = changed.iter().map(String::as_str).collect::<Vec<_>>();
    let skipped_paths = skipped.iter().map(String::as_str).collect::<Vec<_>>();
    let failing = (check && (!changed.is_empty() || non_flow_unformatted))
        || !skipped.is_empty()
        || !unreadable.is_empty()
        || non_flow_failure.is_some();
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
        if paths.is_empty()
            && skipped_paths.is_empty()
            && unreadable.is_empty()
            && non_flow_failure.is_none()
            && !non_flow_unformatted
        {
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
            if !unreadable.is_empty() {
                renderer.status(
                    out,
                    Status::Warn,
                    &format!("{} could not be read", plural(unreadable.len(), "file")),
                );
                renderer.bullet_list(
                    out,
                    2,
                    &unreadable.iter().map(String::as_str).collect::<Vec<_>>(),
                );
                renderer.blank(out);
            }
            if let Some(failure) = non_flow_failure.as_deref() {
                renderer.status(out, Status::Warn, failure);
                renderer.blank(out);
            }
            if non_flow_unformatted {
                renderer.status(
                    out,
                    Status::Warn,
                    &format!(
                        "{formatter_name} reports that some non-Flow files need formatting; \
                         run `uf fmt` to fix them"
                    ),
                );
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

    if !unreadable.is_empty() {
        bail!("{} could not be read", plural(unreadable.len(), "file"));
    }
    if !skipped.is_empty() {
        bail!("{} could not be parsed", plural(skipped.len(), "file"));
    }
    if let Some(failure) = non_flow_failure {
        bail!("{failure}");
    }
    if failing {
        // The verb agrees with the count, the way the line above it does:
        // "1 file need formatting" sits directly under "1 file of 26 needs
        // formatting" and reads as a typo in the tool.
        bail!(
            "{} {} formatting",
            plural(changed.len(), "file"),
            if changed.len() == 1 { "needs" } else { "need" }
        );
    }
    Ok(())
}
