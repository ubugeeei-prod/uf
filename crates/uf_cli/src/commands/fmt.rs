//! `uf fmt`: which files changed, which could not be read, and whether the
//! check passed.

use std::fs;

use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use uf_config::load_config;
use uf_fmt::format_source;
use uf_project::scan_selected_source_files;
use uf_term::Status;

use crate::support::{
    ignore_deprecation, plural, quoted_list, render_ignore_deprecation, selects, unreadable_lines,
};
use crate::ui::Ui;

pub(crate) fn fmt(cwd: &Utf8Path, ui: &mut Ui, check: bool, paths: &[String]) -> Result<()> {
    let resolved = load_config(cwd)?;
    let deprecation = ignore_deprecation(&resolved.config);
    // Discovery returns `package.json` too, because the linter reads it. The
    // formatter must not touch it: it is a Flow formatter, and running it
    // over JSON inserts a statement terminator and leaves the file unparseable.
    let mut scan = scan_selected_source_files(&resolved.root, &resolved.config, paths)?;
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
    let formatter_name = resolved.config.fmt.non_flow.formatter.as_str();
    let mut non_flow_unformatted = false;
    // Kept apart from `skipped`, which is "the parser refused this file". A
    // formatter that is not installed is a different problem with a different
    // fix, and folding the two together reported a missing binary as an
    // unparseable file.
    let mut non_flow_failure = None;
    // And kept apart from both: the formatter uf chose is not installed, which
    // is neither uf's failure nor the project's. It is reported, named file by
    // file, and it does not decide the exit code. See ubugeeei-prod/uf#441.
    let mut non_flow_skipped = None;
    match uf_fmt::non_flow::run(&resolved.root, &non_flow, check, &resolved.config.fmt) {
        // Which files it rewrote is deliberately not read here: `uf fmt` was
        // asked to write, so a rewrite is the command doing its job rather
        // than something to report against. `uf prepare --fix` is the caller
        // that has to know, because there the rewrite lands outside the index.
        Ok(uf_fmt::NonFlowOutcome::Formatted { .. }) => {}
        Ok(uf_fmt::NonFlowOutcome::Unformatted) => non_flow_unformatted = true,
        Ok(uf_fmt::NonFlowOutcome::Skipped { formatter, paths }) => {
            non_flow_skipped = Some((
                uf_fmt::non_flow::skipped_message(&formatter, paths.len()),
                paths,
            ));
        }
        Err(error) => non_flow_failure = Some(error.to_string()),
    }

    let paths = changed.iter().map(String::as_str).collect::<Vec<_>>();
    let skipped_paths = skipped.iter().map(String::as_str).collect::<Vec<_>>();
    let left_alone = non_flow_skipped
        .as_ref()
        .map(|(_, paths)| paths.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    // A formatter uf chose and the project does not have is deliberately not
    // here: the exit code answers for the files uf could format, which is the
    // question `uf fmt --check` is asked in CI.
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
            && non_flow_skipped.is_none()
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
            // Once, and with the files named: a reader told that "1 non-Flow
            // file was skipped" and not which one has been given a puzzle
            // rather than a report.
            if let Some((message, _)) = non_flow_skipped.as_ref() {
                renderer.status(out, Status::Warn, message);
                renderer.bullet_list(out, 2, &left_alone);
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
    // After the summary, and outside the closure that draws it: the answer is
    // what a reader came for, and the note about a key they should move is the
    // sentence after it. `uf fmt` is the command the old name misleads about,
    // so it is the one that most has to say this.
    render_ignore_deprecation(ui, deprecation);

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
