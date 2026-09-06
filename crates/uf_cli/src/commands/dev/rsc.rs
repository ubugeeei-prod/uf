//! The React Server Components contract, while the code is being written.
//!
//! `uf build` runs [`analyze_project`] and puts the number of violations in
//! its summary. `uf dev` did not run it at all, so the loop was: write a
//! Server Component that calls `useEffect`, watch it work — it does work,
//! because nothing is split yet and every module runs in both places — and
//! find out from CI. Development is where a contract is cheapest to keep and
//! was the one place uf never mentioned it.
//!
//! # Recomputed, not remembered
//!
//! The analysis is a whole-graph property: whether a module is client-only
//! depends on which entries reach it, so there is no per-module answer that is
//! also a complete one. Running it once at start-up would therefore give a
//! *complete* answer that is *wrong* the moment a directive is added, a file
//! appears, or an import is deleted — and `uf dev`'s entire promise is that it
//! serves what `uf build` writes.
//!
//! So it is recomputed, and the change that triggers it comes from Vite's
//! watcher rather than from a watcher of uf's own (see `watchSources` in
//! `@uniflowed/vite`'s driver). The whole project is rescanned rather than the
//! one module patched, because patching a graph is a cache and this is cheap:
//! the documentation site is twelve modules and single-digit milliseconds. The
//! incremental version is worth writing when a project is large enough for the
//! difference to be measured rather than assumed.
//!
//! # It reports and does not fail
//!
//! Nothing here changes an exit code. Whether a contract violation should fail
//! `uf build` is a real decision with a real cost — the diagnostics uf's own
//! documentation site has today are accurate against a split that has not
//! happened, so failing on them would fail the build of a site that works —
//! and it belongs to ubugeeei-prod/uf#281, which owns the reporter and the
//! exit code together. A dev server has no exit code to argue about: it either
//! says something or it does not.

use camino::{Utf8Path, Utf8PathBuf};
use uf_rsc::{BuildId, ProjectScanOptions, RscDiagnostic, RscSeverity, analyze_project};
use uf_term::{CodeFrame, DiagnosticLevel, Status};

use crate::support::plural;
use crate::ui::Ui;

/// What one rescan decided to say.
///
/// Separated from the rendering so that "when does this speak" is a value a
/// test can assert on. Whether it stays quiet on an unchanged project is the
/// difference between a report and a log, and it is not something a terminal
/// makes easy to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RscUpdate {
    /// The answer is the one already on the screen.
    Unchanged,
    /// There is nothing left to report, and there was something before.
    Cleared,
    /// These, in the order the graph reports them.
    Reported(Vec<RscDiagnostic>),
    /// The project could not be scanned, and this was not the last reason.
    Failed(String),
}

/// The RSC analysis, as `uf dev` keeps it.
pub(crate) struct RscReport {
    root: Utf8PathBuf,
    build_id: BuildId,
    options: ProjectScanOptions,
    /// The diagnostics last rendered. `None` before the first scan and after a
    /// failed one, so the next success is always rendered in full.
    last: Option<Vec<RscDiagnostic>>,
    /// The last scan failure, so an unreadable file is reported once rather
    /// than on every save until it is fixed.
    last_error: Option<String>,
}

impl RscReport {
    /// A report for the project at `root`.
    pub(crate) fn new(root: &Utf8Path) -> Self {
        Self {
            root: root.to_path_buf(),
            build_id: BuildId::from_env_or_generate(),
            options: ProjectScanOptions::default(),
            last: None,
            last_error: None,
        }
    }

    /// Rescan, and say what changed.
    pub(crate) fn report(&mut self, ui: &mut Ui) {
        let update = self.refresh();
        render(ui, &update);
    }

    /// Rescan the project and decide whether there is anything new to say.
    fn refresh(&mut self) -> RscUpdate {
        let analysis = match analyze_project(&self.root, &self.build_id, &self.options) {
            Ok(analysis) => analysis,
            // A project that cannot be scanned is not a dev server that should
            // stop: the file being edited is very often the unreadable one.
            Err(error) => {
                let message = format!("the server-component analysis could not run: {error}");
                self.last = None;
                if self.last_error.as_ref() == Some(&message) {
                    return RscUpdate::Unchanged;
                }
                self.last_error = Some(message.clone());
                return RscUpdate::Failed(message);
            }
        };
        self.last_error = None;

        let diagnostics = analysis.graph.diagnostics().to_vec();
        if self.last.as_ref() == Some(&diagnostics) {
            return RscUpdate::Unchanged;
        }
        let was_reporting = self.last.as_ref().is_some_and(|last| !last.is_empty());
        self.last = Some(diagnostics.clone());

        if diagnostics.is_empty() {
            // Only worth a line when it replaces one. "Still fine" on every
            // keystroke is the noise that makes a terminal stop being read.
            return if was_reporting {
                RscUpdate::Cleared
            } else {
                RscUpdate::Unchanged
            };
        }
        RscUpdate::Reported(diagnostics)
    }
}

/// Draw an update, if it has anything to draw.
///
/// On stderr, which is where every other problem `uf dev` reports goes: the
/// server's own log is the command's output, and this is a report about the
/// project rather than about a request.
fn render(ui: &mut Ui, update: &RscUpdate) {
    match update {
        RscUpdate::Unchanged => {}
        RscUpdate::Failed(message) => ui.render_err(|renderer, out| {
            renderer.status(out, Status::Warn, message);
        }),
        RscUpdate::Cleared => ui.render_err(|renderer, out| {
            renderer.status(out, Status::Success, "the server-component contract holds");
        }),
        RscUpdate::Reported(diagnostics) => {
            let frames = diagnostics.iter().map(Frame::of).collect::<Vec<_>>();
            let errors = diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity() == RscSeverity::Error)
                .count();
            let summary = format!(
                "{} against the server/client contract, {} of them fatal to it once the \
                 client/server split lands (ubugeeei-prod/uf#252)",
                plural(diagnostics.len(), "diagnostic"),
                errors,
            );
            ui.render_err(|renderer, out| {
                renderer.blank(out);
                renderer.heading(out, 2, "server components");
                for frame in &frames {
                    // `RscDiagnostic::line` returns 0 to mean "there is no
                    // line" — a module reached through the client graph is a
                    // fact about the graph rather than about a position — and
                    // a code frame cannot say that. It would print `:0:1`,
                    // which reads as a line number and is not one.
                    if frame.line == 0 {
                        let line = format!("{}: {}", frame.rule, frame.message);
                        renderer.status(out, frame.status(), &line);
                    } else {
                        renderer.code_frame_at(out, &frame.frame(), 4);
                    }
                }
                renderer.status(out, Status::Warn, &summary);
            });
        }
    }
}

/// One diagnostic, with the strings a [`CodeFrame`] borrows.
struct Frame {
    rule: &'static str,
    message: String,
    path: String,
    line: usize,
    column: usize,
    level: DiagnosticLevel,
}

impl Frame {
    fn of(diagnostic: &RscDiagnostic) -> Self {
        Self {
            rule: diagnostic.rule(),
            message: diagnostic.to_string(),
            // Already relative to the project root, which is what makes it the
            // path a reader would type.
            path: diagnostic.module().to_string(),
            line: diagnostic.line() as usize,
            column: column_of(diagnostic),
            level: match diagnostic.severity() {
                RscSeverity::Error => DiagnosticLevel::Error,
                RscSeverity::Warn => DiagnosticLevel::Warning,
            },
        }
    }

    /// How a diagnostic with no position is marked instead.
    fn status(&self) -> Status {
        match self.level {
            DiagnosticLevel::Error => Status::Error,
            _ => Status::Warn,
        }
    }

    fn frame(&self) -> CodeFrame<'_> {
        CodeFrame {
            level: self.level,
            rule: Some(self.rule),
            message: &self.message,
            path: &self.path,
            line: self.line,
            column: self.column,
            span: 1,
            // Deliberately no source line. `uf dev` renders this on every save
            // of a file that is being edited *now*, and the line read back
            // from disk is a line the reader has already changed — a caret
            // under the wrong text is worse than a caret under none.
            source_line: None,
            label: None,
        }
    }
}

/// The column a diagnostic points at, or the start of the line.
///
/// Only the client-only checks know one; an import or an export is reported at
/// its line. Absorbed here rather than in every reader.
fn column_of(diagnostic: &RscDiagnostic) -> usize {
    match diagnostic {
        RscDiagnostic::ClientOnlyApiInServerModule { column, .. }
        | RscDiagnostic::UnclassifiedHookInServerModule { column, .. } => *column as usize,
        _ => 1,
    }
}

#[cfg(test)]
mod tests;
