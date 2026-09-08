//! `uf i18n`: the catalogue a translator is handed, and the file they send back.
//!
//! Two commands and one file format. `uf i18n extract` walks the project for
//! every `message(…)` `@uniflowed/i18n` declares and writes them as JSON;
//! `uf i18n merge` reads that file back with a vendor's translations in it and
//! writes the locale module `defineLocales` loads. `uf_i18n`'s own header says
//! why the format is JSON carrying MessageFormat 2 rather than XLIFF, and why
//! the walk is Rust rather than the package.
//!
//! # Why either command can fail
//!
//! Both refuse rather than guess. Extraction fails when a `message(…)` was
//! found and could not be read, because the alternative is a message quietly
//! missing from the file a vendor is sent — invisible in the extraction,
//! invisible in review, and visible to a user reading English on a Japanese
//! page. Merging fails when a returned translation's message has changed since
//! it went out, because merging it would put a fluent, confident, wrong
//! sentence on a page.
//!
//! # `--json` writes nothing
//!
//! The same rule `uf doc --json` follows: the JSON form is the report, for a
//! script that wants to see what would happen. The file is written by the run
//! that renders, so `uf i18n extract --json` is the dry run and needs no second
//! flag to spell it.

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::load_config;
use uf_i18n::extract::ExtractReport;
use uf_i18n::merge::{MergeReport, default_module_path, write_module};
use uf_i18n::{ExtractOptions, extract, merge};
use uf_term::{KeyValue, Status, Tone};

use crate::cli::I18nCommand;
use crate::support::{plural, project_label, relative_to};
use crate::ui::Ui;

/// Where a catalogue goes when `--out` did not say.
///
/// A directory of its own, named for the locale it holds, because the merged
/// module lands beside it as `<locale>.js` — `i18n/en-US.json` out,
/// `i18n/ja-JP.json` back, `i18n/ja-JP.js` merged — and that is what the
/// `() => import("./ja-JP.js")` in `defineLocales` already points at.
fn default_catalogue_path(locale: &str) -> Utf8PathBuf {
    Utf8PathBuf::from("i18n").join(format!("{locale}.json"))
}

pub(crate) fn i18n(cwd: &Utf8Path, ui: &mut Ui, command: I18nCommand) -> Result<()> {
    match command {
        I18nCommand::Extract { locale, out, json } => {
            extract_command(cwd, ui, locale, out.as_deref(), json)
        }
        I18nCommand::Merge { file, out, json } => {
            merge_command(cwd, ui, &file, out.as_deref(), json)
        }
    }
}

fn extract_command(
    cwd: &Utf8Path,
    ui: &mut Ui,
    locale: Option<String>,
    out: Option<&Utf8Path>,
    json: bool,
) -> Result<()> {
    let mut progress = ui.progress();
    progress.draw("loading configuration");
    let resolved = load_config(cwd)?;

    progress.tick("reading messages");
    let report = extract(&resolved.root, &resolved.config, &ExtractOptions { locale })?;
    progress.finish();
    drop(progress);

    let destination = out.map_or_else(
        || default_catalogue_path(&report.catalogue.source_locale),
        Utf8Path::to_path_buf,
    );
    let path = resolved.root.join(&destination);

    if json {
        ui.json(&json!({
            "command": "uf i18n extract",
            "root": resolved.root.as_str(),
            "out": destination.as_str(),
            "report": report,
        }))?;
        return refuse_extraction(&report);
    }

    if report.has_problems() {
        render_extract_problems(ui, &report);
        return refuse_extraction(&report);
    }

    report.catalogue.write_to(&path)?;
    render_extracted(ui, &resolved.root, &path, &report);
    Ok(())
}

/// The sentence that fails the command, or `Ok` when nothing was wrong.
///
/// Shared by both output forms so that `--json` and the rendered run agree
/// about what a failure is: a script switching to `--json` must not find that
/// the exit code changed with it.
fn refuse_extraction(report: &ExtractReport) -> Result<()> {
    if !report.unreadable.is_empty() {
        bail!(
            "{} could not be read",
            plural(report.unreadable.len(), "file")
        );
    }
    if !report.problems.is_empty() {
        bail!(
            "{} could not be extracted; nothing was written, because a catalogue \
             missing a message is worse than no catalogue",
            plural(report.problems.len(), "message")
        );
    }
    Ok(())
}

fn merge_command(
    cwd: &Utf8Path,
    ui: &mut Ui,
    file: &Utf8Path,
    out: Option<&Utf8Path>,
    json: bool,
) -> Result<()> {
    let mut progress = ui.progress();
    progress.draw("loading configuration");
    let resolved = load_config(cwd)?;

    // Relative to where the command was typed, the way every other path
    // argument here is: a translation file arrives in a download directory as
    // often as it arrives in the repository.
    let file = if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    };

    progress.tick("reading messages");
    let report = merge(&resolved.root, &resolved.config, &file)?;
    progress.finish();
    drop(progress);

    let destination = out.map_or_else(
        || default_module_path(&file, &report.locale),
        Utf8Path::to_path_buf,
    );
    let path = if destination.is_absolute() {
        destination.clone()
    } else {
        cwd.join(&destination)
    };

    if json {
        ui.json(&json!({
            "command": "uf i18n merge",
            "root": resolved.root.as_str(),
            "file": file.as_str(),
            "out": path.as_str(),
            "report": report,
        }))?;
        return refuse_merge(&report);
    }

    if report.has_problems() {
        render_merge_problems(ui, &report);
        return refuse_merge(&report);
    }

    write_module(&path, &report.module)?;
    render_merged(ui, &resolved.root, &path, &report);
    Ok(())
}

fn refuse_merge(report: &MergeReport) -> Result<()> {
    refuse_extraction(&report.extraction)?;
    if !report.stale.is_empty() {
        bail!(
            "{} changed since this file was extracted; nothing was written. \
             Extract again and send the changed messages back out.",
            plural(report.stale.len(), "message")
        );
    }
    Ok(())
}

fn render_extracted(ui: &mut Ui, root: &Utf8Path, path: &Utf8Path, report: &ExtractReport) {
    let project = project_label(root).to_string();
    let files = report.files_parsed.to_string();
    let modules = report.modules.to_string();
    let messages = report.catalogue.messages.len().to_string();
    let locale = report.catalogue.source_locale.clone();
    let output = relative_to(root, path);

    ui.render(|renderer, out| {
        renderer.banner(out, "uf i18n extract", Some(&project));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("locale", &locale, Tone::Number),
                KeyValue::toned("modules", &modules, Tone::Number),
                KeyValue::toned("files parsed", &files, Tone::Number),
                KeyValue::toned("messages", &messages, Tone::Number),
                KeyValue::toned("out", &output, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, &format!("wrote {output}"));
    });
}

fn render_extract_problems(ui: &mut Ui, report: &ExtractReport) {
    let unreadable = report
        .unreadable
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let problems = report
        .problems
        .iter()
        .map(|problem| {
            format!(
                "{}: {} — {}",
                problem.at,
                problem.kind.label(),
                problem.detail
            )
        })
        .collect::<Vec<_>>();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf i18n extract", None);
        renderer.blank(out);
        if !unreadable.is_empty() {
            renderer.status(
                out,
                Status::Warn,
                &format!("{} could not be read", plural(unreadable.len(), "file")),
            );
            renderer.bullet_list(out, 2, &unreadable);
            renderer.blank(out);
        }
        for problem in &problems {
            renderer.status(out, Status::Error, problem);
        }
    });
}

fn render_merged(ui: &mut Ui, root: &Utf8Path, path: &Utf8Path, report: &MergeReport) {
    let locale = report.locale.clone();
    let translated = report.translated.to_string();
    let untranslated = report.untranslated.len().to_string();
    let missing = report.missing.len().to_string();
    let unknown = report.unknown.len().to_string();
    let output = relative_to(root, path);

    // The three counts below are the state of a translation in progress rather
    // than mistakes, so they are printed and do not fail the command — but
    // printed, because "83 of 214" is the only number anybody wants from this.
    ui.render(|renderer, out| {
        renderer.banner(out, "uf i18n merge", Some(&locale));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("translated", &translated, Tone::Number),
                KeyValue::toned("still in the source locale", &untranslated, Tone::Number),
                KeyValue::toned("not in the file", &missing, Tone::Number),
                KeyValue::toned("no longer declared", &unknown, Tone::Number),
                KeyValue::toned("out", &output, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, &format!("wrote {output}"));
    });
}

fn render_merge_problems(ui: &mut Ui, report: &MergeReport) {
    let stale = report
        .stale
        .iter()
        .map(|message| {
            format!(
                "{}: {} ({})",
                message.key, message.what, message.declared_at
            )
        })
        .collect::<Vec<_>>();

    if report.extraction.has_problems() {
        render_extract_problems(ui, &report.extraction);
    }
    if stale.is_empty() {
        return;
    }
    ui.render(|renderer, out| {
        renderer.banner(out, "uf i18n merge", Some(&report.locale));
        renderer.blank(out);
        renderer.status(
            out,
            Status::Error,
            &format!(
                "{} changed since this file was extracted",
                plural(stale.len(), "message")
            ),
        );
        for message in &stale {
            renderer.status(out, Status::Error, message);
        }
    });
}
