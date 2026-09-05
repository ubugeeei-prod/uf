//! `uf doc`: API documentation extracted from exported Flow source.

use anyhow::{Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::load_config;
use uf_doc::{DocDiagnostic, DocReport, generate, write_markdown};
use uf_term::{KeyValue, Status, Tone};

use crate::support::{plural, project_label, relative_to};
use crate::ui::Ui;

pub(crate) fn doc(cwd: &Utf8Path, ui: &mut Ui, out_dir: &Utf8Path, json: bool) -> Result<()> {
    let mut progress = ui.progress();
    progress.draw("loading configuration");
    let resolved = load_config(cwd)?;

    progress.tick("parsing Flow source");
    let report = generate(&resolved.root, &resolved.config)?;
    progress.finish();
    drop(progress);

    if json {
        ui.json(&json!({
            "command": "uf doc",
            "root": resolved.root.as_str(),
            "report": report,
        }))?;
        if !report.unreadable.is_empty() {
            bail!(
                "{} could not be read",
                plural(report.unreadable.len(), "file")
            );
        }
        if report.has_diagnostics() {
            bail!(
                "uf doc failed with {}",
                plural(report.diagnostics.len(), "parse error")
            );
        }
        return Ok(());
    }

    // Reported before the parse errors, and separately: a file that is not
    // UTF-8 has no syntax to be wrong about, and one of them used to stop the
    // walk before any other file was read.
    if !report.unreadable.is_empty() {
        render_unreadable(ui, &report);
    }
    if report.has_diagnostics() {
        render_diagnostics(ui, &report);
    }
    if !report.unreadable.is_empty() {
        bail!(
            "{} could not be read",
            plural(report.unreadable.len(), "file")
        );
    }
    if report.has_diagnostics() {
        bail!(
            "uf doc failed with {}",
            plural(report.diagnostics.len(), "parse error")
        );
    }

    let output_dir = resolved.root.join(out_dir);
    let output = write_markdown(&report, &output_dir)?;
    render_success(ui, &resolved.root, out_dir, &output, &report);
    Ok(())
}

/// The files discovery skipped.
fn render_unreadable(ui: &mut Ui, report: &DocReport) {
    let lines = report
        .unreadable
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf doc", None);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Warn,
            &format!("{} could not be read", plural(lines.len(), "file")),
        );
        renderer.bullet_list(out, 2, &lines);
        renderer.blank(out);
    });
}

fn render_success(
    ui: &mut Ui,
    root: &Utf8Path,
    out_dir: &Utf8Path,
    output: &Utf8PathBuf,
    report: &DocReport,
) {
    let project = project_label(root).to_string();
    let files = report.files_scanned.to_string();
    let modules = report.modules.len().to_string();
    let entries = report.entry_count().to_string();
    let output_label = relative_to(root, output);
    let out_dir_label = out_dir.to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf doc", Some(&project));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::toned("files", &files, Tone::Number),
                KeyValue::toned("modules", &modules, Tone::Number),
                KeyValue::toned("entries", &entries, Tone::Number),
                KeyValue::toned("out", &out_dir_label, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, &format!("wrote {}", output_label));
    });
}

fn render_diagnostics(ui: &mut Ui, report: &DocReport) {
    let errors = report.diagnostics.len().to_string();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf doc", None);
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[KeyValue::toned("parse errors", &errors, Tone::Number)],
        );
        renderer.blank(out);
        for diagnostic in &report.diagnostics {
            renderer.status(out, Status::Error, &diagnostic_label(diagnostic));
        }
    });
}

fn diagnostic_label(diagnostic: &DocDiagnostic) -> String {
    let location = match (diagnostic.line, diagnostic.column) {
        (Some(line), Some(column)) => format!("{}:{line}:{column}", diagnostic.path),
        (Some(line), None) => format!("{}:{line}", diagnostic.path),
        _ => diagnostic.path.clone(),
    };
    format!("{location}: {}", diagnostic.message)
}
