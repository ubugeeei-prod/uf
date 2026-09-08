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

use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};
use uf_rsc::{
    BuildId, ClientBundleReason, ProjectScanOptions, RSC_MANIFEST_BUILD_DIR,
    RSC_MANIFEST_FILE_NAME, RscDiagnostic, RscGraph, RscSeverity, analyze_project, write_manifest,
};
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

/// The most moves one report spells out.
///
/// Adding `"use client"` to a module deep in a tree moves that module and
/// every module above it in one edit, and a project can have a great many of
/// those. The number of modules that moved is always exact; the chains are
/// what is bounded, because the point of a chain is that somebody reads it.
const MAX_MOVES: usize = 12;

/// A module that entered or left the client bundle between two scans.
///
/// The reason is carried as the line a reader gets rather than as the
/// [`uf_rsc::ModuleId`]s it was computed from. Those are positions in a table
/// that is rebuilt from scratch on the next save, so a chain kept past the
/// graph that produced it would be a set of indices into a table that no
/// longer exists — and this value outlives its graph by design, because the
/// whole of what it is for is to be compared against the *next* one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BundleMove {
    /// The browser now has to be able to evaluate this module.
    Entered {
        /// Path relative to the project root.
        module: Utf8PathBuf,
        /// Why: the directive on the module, or the imports that reach one.
        reason: String,
    },
    /// It no longer does, so its code is out of the bundle a visitor downloads.
    Left {
        /// Path relative to the project root.
        module: Utf8PathBuf,
    },
}

impl BundleMove {
    /// The module the move is about.
    fn module(&self) -> &Utf8Path {
        match self {
            Self::Entered { module, .. } | Self::Left { module } => module,
        }
    }

    /// The move as the sentence the terminal prints.
    fn line(&self) -> String {
        match self {
            Self::Entered { module, reason } => {
                format!("{module} is now in the client bundle — {reason}")
            }
            Self::Left { module } => {
                format!("{module} is out of the client bundle")
            }
        }
    }
}

/// What moved in or out of the client bundle at one rescan.
///
/// Separate from [`RscUpdate`] because the two answer different questions and
/// a save can move both: `RscUpdate` is whether the *contract* holds, and this
/// is what the browser is being sent. Reporting them through one value would
/// mean choosing which of the two a reader is told about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BundleUpdate {
    /// The moves worth spelling out, at most [`MAX_MOVES`] of them.
    pub(crate) moves: Vec<BundleMove>,
    /// How many moved in total, which is what the summary line says when it is
    /// more than [`Self::moves`] holds.
    pub(crate) total: usize,
}

impl BundleUpdate {
    /// Whether there is anything to draw.
    fn is_empty(&self) -> bool {
        self.total == 0
    }
}

/// The RSC analysis, as `uf dev` keeps it.
///
/// # Two readers
///
/// The diagnostics are for the person, and the manifest is for the bundler.
/// `@uniflowed/vite` leaves a route's page out of the client route table when
/// no client boundary is reachable from it, and it reads that decision out of
/// the manifest this writes — so `uf dev` has to keep the file current for the
/// same reason it keeps the diagnostics current. A dev server that hydrated a
/// route `uf build` ships no page for would be a dev server disagreeing with
/// the build about what the browser gets, which is the one thing it may never
/// do.
pub(crate) struct RscReport {
    root: Utf8PathBuf,
    /// Where the bundler reads the analysis from.
    manifest: Utf8PathBuf,
    build_id: BuildId,
    options: ProjectScanOptions,
    /// The diagnostics the last successful scan found. `None` before the first
    /// scan and after a failed one, so the next success is always rendered in
    /// full.
    last: Option<Vec<RscDiagnostic>>,
    /// Whether a violation is on the reader's screen.
    ///
    /// Not derivable from `last`, which is why it is kept. `last` is what the
    /// last *scan* found and is cleared by a failure so the next success
    /// redraws; this is what the last *render* put on the terminal, and a
    /// failure does not take it off. Without the two apart, the sequence
    /// report → failed scan → clean project answered [`RscUpdate::Unchanged`]
    /// — leaving the violation on screen with nothing to say it was fixed, at
    /// exactly the moment the reader fixed it.
    displayed: bool,
    /// The last scan failure, so an unreadable file is reported once rather
    /// than on every save until it is fixed.
    last_error: Option<String>,
    /// The modules the browser had to evaluate at the last *successful* scan.
    ///
    /// `None` before the first one, which is what keeps the start-up quiet:
    /// there is no move to report when nothing was there to move from, and a
    /// list of every client module on start-up is the banner nobody reads.
    ///
    /// Deliberately kept across a failed scan, unlike [`Self::last`]. A
    /// failure recomputes nothing, so it says nothing about what the browser
    /// gets; forgetting it would make the scan after a half-written file
    /// report the whole client bundle as newly arrived.
    last_bundle: Option<BTreeSet<Utf8PathBuf>>,
    /// What the last rescan found had moved, for [`Self::report`] to draw.
    moved: BundleUpdate,
}

impl RscReport {
    /// A report for the project at `root`.
    pub(crate) fn new(root: &Utf8Path) -> Self {
        Self {
            root: root.to_path_buf(),
            manifest: root
                .join(RSC_MANIFEST_BUILD_DIR)
                .join(RSC_MANIFEST_FILE_NAME),
            build_id: BuildId::from_env_or_generate(),
            options: ProjectScanOptions::default(),
            last: None,
            displayed: false,
            last_error: None,
            last_bundle: None,
            moved: BundleUpdate::default(),
        }
    }

    /// Where the bundler is told to read the analysis from.
    ///
    /// A path rather than a file: it is handed to the driver before the first
    /// scan has necessarily succeeded, and `@uniflowed/vite` treats a manifest
    /// it cannot read as "split nothing" — which is what the build did before
    /// any of this existed. So the path is always true even when the file is
    /// not yet there.
    pub(crate) fn manifest_path(&self) -> &Utf8Path {
        &self.manifest
    }

    /// Scan once and write the manifest, before the bundler starts.
    ///
    /// Failure is deliberately silent. A project that cannot be scanned is not
    /// a dev server that should refuse to start — the file being edited is very
    /// often the unreadable one — and the consequence of no manifest is that
    /// nothing is split, which is a whole route table rather than a broken one.
    /// The first [`Self::report`] scans again and writes it if it can.
    pub(crate) fn prime(&mut self) {
        if let Ok(analysis) = analyze_project(&self.root, &self.build_id, &self.options) {
            self.persist(&analysis);
        }
    }

    /// Rescan, and say what changed.
    ///
    /// The bundle moves come first because they are the consequence and the
    /// contract diagnostics are the rule: a reader who has just made a module
    /// the browser's business wants to be told that, and then told what it
    /// costs them.
    pub(crate) fn report(&mut self, ui: &mut Ui) {
        let update = self.refresh();
        render_bundle(ui, &self.moved);
        render(ui, &update);
    }

    /// Write the manifest, unless the bytes it would hold are already there.
    ///
    /// Only when they moved: `@uniflowed/vite` watches this file and rebuilds
    /// the client route table when it changes, so rewriting identical bytes on
    /// every keystroke would be a full page reload on every keystroke.
    ///
    /// A write that fails is not reported. The diagnostics above are what this
    /// module exists to say; a manifest that could not be written costs the
    /// reader a split, and a second warning channel about `.uf/` on every save
    /// would cost them the diagnostics.
    fn persist(&self, analysis: &uf_rsc::RscAnalysis) {
        let manifest = analysis.manifest();
        let Ok(json) = manifest.to_json() else {
            return;
        };
        if std::fs::read_to_string(&self.manifest).is_ok_and(|current| current == json) {
            return;
        }
        let _ = write_manifest(&self.root.join(RSC_MANIFEST_BUILD_DIR), &manifest);
    }

    /// Rescan the project and decide whether there is anything new to say.
    fn refresh(&mut self) -> RscUpdate {
        self.moved = BundleUpdate::default();
        let analysis = match analyze_project(&self.root, &self.build_id, &self.options) {
            Ok(analysis) => analysis,
            // A project that cannot be scanned is not a dev server that should
            // stop: the file being edited is very often the unreadable one.
            Err(error) => {
                let message = format!("the server-component analysis could not run: {error}");
                // The scan is forgotten and the screen is not. A failure says
                // nothing about whether the violations it cannot recompute are
                // still true, so the next success redraws them in full; but it
                // also does not erase the ones already printed, and the reader
                // is still owed the line that says they are gone.
                self.last = None;
                if self.last_error.as_ref() == Some(&message) {
                    return RscUpdate::Unchanged;
                }
                self.last_error = Some(message.clone());
                return RscUpdate::Failed(message);
            }
        };
        self.last_error = None;
        // Before the early returns below: the manifest has to follow the graph
        // even when the *diagnostics* are unchanged, because adding a
        // `"use client"` import moves what the browser gets without moving what
        // the contract says.
        self.persist(&analysis);
        // And, for the same reason, so does the report about it. This is the
        // edit that changes what a visitor downloads while changing nothing
        // the contract has an opinion about, and it was the one edit `uf dev`
        // said nothing at all about.
        self.moved = self.bundle_moves(&analysis.graph);

        let diagnostics = analysis.graph.diagnostics().to_vec();
        if self.last.as_ref() == Some(&diagnostics) {
            return RscUpdate::Unchanged;
        }
        self.last = Some(diagnostics.clone());

        if diagnostics.is_empty() {
            // Only worth a line when it replaces one that is still on screen.
            // "Still fine" on every keystroke is the noise that makes a
            // terminal stop being read.
            return if std::mem::take(&mut self.displayed) {
                RscUpdate::Cleared
            } else {
                RscUpdate::Unchanged
            };
        }
        self.displayed = true;
        RscUpdate::Reported(diagnostics)
    }

    /// Diff the client bundle against the last successful scan's.
    ///
    /// The set is the modules [`uf_rsc::RscModule::requires_client_bundle`]
    /// answers `true` for — the same predicate `@uniflowed/vite` splits the
    /// route table with, read from the same graph in the same rescan, so what
    /// this reports and what the browser is served cannot drift apart.
    ///
    /// The reason for each arrival is asked of the graph rather than derived
    /// here: `client_bundle_reason` is one walk of the analysis that already
    /// exists, and a second walk written in this file would be a second
    /// analysis that agrees with the first until one of them is edited.
    fn bundle_moves(&mut self, graph: &RscGraph) -> BundleUpdate {
        let current: BTreeSet<Utf8PathBuf> = graph
            .modules()
            .iter()
            .filter(|module| module.requires_client_bundle())
            .map(|module| module.path.clone())
            .collect();

        let Some(previous) = self.last_bundle.replace(current.clone()) else {
            // The first scan has nothing to compare against, and a project's
            // whole client bundle listed at start-up is not a report about a
            // decision anybody just made.
            return BundleUpdate::default();
        };

        let mut moves: Vec<BundleMove> = current
            .difference(&previous)
            .map(|module| BundleMove::Entered {
                module: module.clone(),
                reason: reason_line(graph, module),
            })
            .chain(
                previous
                    .difference(&current)
                    .map(|module| BundleMove::Left {
                        module: module.clone(),
                    }),
            )
            .collect();
        let total = moves.len();
        // By path, so that a page and the components under it read as one
        // change rather than as arrivals interleaved with departures.
        moves.sort_by(|left, right| left.module().cmp(right.module()));
        moves.truncate(MAX_MOVES);
        BundleUpdate { moves, total }
    }
}

/// Why `module` is in the client bundle, as one line.
///
/// [`ClientBundleReason::Isolated`] cannot be reached from
/// [`RscReport::bundle_moves`] — every module it asks about answered
/// `requires_client_bundle`, and `uf_rsc::graph` holds those two against each
/// other — so the arm below is a sentence rather than a panic. A report is not
/// worth stopping a dev server for, and the honest thing to print when the two
/// disagree is that they did.
fn reason_line(graph: &RscGraph, module: &Utf8Path) -> String {
    let Some(id) = graph.module_id(module) else {
        return "the graph no longer has it".to_owned();
    };
    match graph.client_bundle_reason(id) {
        ClientBundleReason::Declared => "it declares `\"use client\"`".to_owned(),
        ClientBundleReason::Imports(chain) => {
            let paths = chain
                .iter()
                .filter_map(|id| graph.module_by_id(*id))
                .map(|module| module.path.as_str())
                .collect::<Vec<_>>();
            match paths.split_last() {
                // The chain reads as the imports somebody would follow, and
                // the last hop is named for what it is rather than left to be
                // inferred from the arrow before it.
                Some((boundary, above)) => format!(
                    "{} imports {boundary}, which declares `\"use client\"`",
                    above.join(" imports ")
                ),
                None => "it reaches a client boundary".to_owned(),
            }
        }
        ClientBundleReason::Isolated => {
            "the split and the explanation disagree about it, which is a bug in uf".to_owned()
        }
    }
}

/// Draw what moved in or out of the client bundle, if anything did.
///
/// On stderr beside the contract report, and for the same reason: this is a
/// report about the project rather than about a request.
///
/// `Info` rather than `Warn`. A module entering the client bundle is what
/// `"use client"` is *for* — it is the cost of a decision the reader has just
/// made deliberately, not a mistake — and a warning colour on it would train
/// people to add the directive without reading the line. What makes it worth
/// printing is that the cost is otherwise invisible until somebody measures a
/// bundle.
fn render_bundle(ui: &mut Ui, update: &BundleUpdate) {
    if update.is_empty() {
        return;
    }
    let lines = update
        .moves
        .iter()
        .map(BundleMove::line)
        .collect::<Vec<_>>();
    let items = lines.iter().map(String::as_str).collect::<Vec<_>>();
    let elided = update.total.saturating_sub(update.moves.len());
    let summary = if elided == 0 {
        format!(
            "{} moved across the client bundle",
            plural(update.total, "module")
        )
    } else {
        format!(
            "{} moved across the client bundle, {elided} not listed",
            plural(update.total, "module"),
        )
    };
    ui.render_err(|renderer, out| {
        renderer.blank(out);
        renderer.heading(out, 2, "client bundle");
        renderer.bullet_list(out, 4, &items);
        renderer.status(out, Status::Info, &summary);
    });
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
            // The error names the module it could not analyse. #659.
            renderer.status(out, Status::Warn, &uf_term::safe_message(message));
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
                "{} against the server/client contract, {} of them fatal to `uf \
                 build` (ubugeeei-prod/uf#281)",
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
                        // This is the one path that bypasses `code_frame_at`,
                        // and so the one that has to prepare the message
                        // itself. The rule id is uf's own; the message is the
                        // checkout's. #659.
                        let line =
                            format!("{}: {}", frame.rule, uf_term::safe_message(&frame.message));
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
