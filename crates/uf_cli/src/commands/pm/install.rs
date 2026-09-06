//! `uf install`: the phases while they happen, and what changed once they have.
//!
//! The install itself is a package manager's — see [`uf_pm::run`] for why, and
//! why npm stands in when a project evidences no manager at all. What this
//! module owns is the two things uf can do that the manager cannot.
//!
//! # While it runs
//!
//! [`Screen`] draws a five-row block on stderr and rewrites it where it
//! stands: the ladder of phases, which one is running, what it is working on,
//! how many it has done and how long each has taken. Off a terminal it draws
//! nothing whatsoever — [`uf_term::Live`] refuses, and it is the only thing
//! asked — so a CI log is the manager's output and uf's summary, exactly as it
//! was before any of this existed.
//!
//! The manager's own lines go straight through [`Screen::line`], on the stream
//! it wrote them to, with the region taken off the screen first so they land
//! in the scrollback rather than inside a block that is about to be redrawn. A
//! failed install therefore still says why, in npm's words: those words are
//! npm's own output and nothing here filters them.
//!
//! # Once it has
//!
//! The manager reports a count. `uf install` reports the lockfile, before and
//! after: which packages arrived, which left, which changed version, and which
//! only moved. That is [`uf_pm::delta`], and it is the difference between "16
//! packages" and an answer to why a build started behaving differently.
//!
//! A run that changed nothing says so in one line and stops. Most installs
//! change nothing, and a person who runs this ten times a day should not have
//! to read a report about it.

use std::time::{Duration, Instant};

use anyhow::Result;
use camino::Utf8Path;
use uf_bundle::ByteSize;
use uf_config::load_config;
use uf_pm::delta::{ChangeKind, LockfileDelta};
use uf_pm::{
    DetectionSource, InstallObserver, InstallWatch, LockfileSnapshot, ManagerStream,
    PackageManagerPlan, PhaseProgress, PhaseState, detect_package_manager, install_workspace,
    installable, run_install_watched,
};
use uf_term::{
    Align, Capabilities, Cell, Column, GlyphSet, KeyValue, Live, Phase, PhaseTimer, Renderer,
    Status, Style, Table, Tone, Tty, display_width, format_duration, push_spaces,
    truncate_to_width,
};

use crate::brand;
use crate::commands::vite::resolve_host;
use crate::support::{plural, project_label};
use crate::ui::Ui;

/// How many changed packages the summary names before it stops listing them.
///
/// The same reasoning as `uf build --size-report`'s cap: a list of four
/// thousand is not a summary, and the count above the table is the fact a
/// reader is actually after.
const CHANGES_SHOWN: usize = 15;

/// Columns of the live region, left to right.
const INDENT: usize = 2;
const LABEL_WIDTH: usize = 8;
const DETAIL_WIDTH: usize = 38;
const COUNT_WIDTH: usize = 6;
const TIME_WIDTH: usize = 7;
/// The whole region's width, which must stay inside [`uf_term::LIVE_WIDTH`].
const ROW_WIDTH: usize =
    INDENT + 2 + LABEL_WIDTH + 1 + DETAIL_WIDTH + 2 + COUNT_WIDTH + 2 + TIME_WIDTH;

/// `uf install`.
pub(crate) fn install(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let mut timer = PhaseTimer::start();
    let resolved = timer.measure("config", || load_config(cwd))?;
    let plan = PackageManagerPlan::infer_from_config(&resolved.config);

    // A manifest that declares scripts is refused before anything is fetched,
    // the way it was when uf planned the install itself. `--ignore-scripts`
    // inside `run_install_watched` covers the dependencies; this covers the
    // project.
    install_workspace(&resolved.root, &resolved.config)?;

    // Which manager is about to run has to be settled here rather than left to
    // the runner, because the lockfile it is about to rewrite must be read
    // first: a "before" state read afterwards is the "after" state.
    let (manager, _) = installable(&detect_package_manager(&resolved.root));
    let before = uf_pm::delta::snapshot(&resolved.root, manager);
    timer.lap("workspace");
    let prelude = timer.phases().to_vec();

    let project = project_label(&resolved.root).to_string();
    ui.render(|renderer, out| {
        brand::render_product_card(renderer, out, "uf install");
        renderer.blank(out);
        renderer.banner(out, "uf install", Some(&project));
        renderer.blank(out);
    });

    let manager_label = manager.to_string();
    let (outcome, echoed) = {
        let mut screen = Screen::new(ui, &project, &manager_label);
        let run = run_install_watched(&resolved.root, !plan.forbids_npm_scripts(), &mut screen);
        screen.close();
        (run, screen.echoed)
    };
    let outcome = outcome?;

    let read_back = Instant::now();
    let after = uf_pm::delta::snapshot(&resolved.root, manager);
    let delta = uf_pm::delta::diff(&before, &after);
    let lockfile = read_back.elapsed();
    let total = timer.total();

    let report = InstallReport {
        manager: manager_label,
        chosen_by: chosen_by(&outcome.source, outcome.substituted),
        command: outcome.invocation.to_string(),
        runtime: runtime_label(&resolved.config),
        lockfile: lockfile_label(&resolved.root, &after),
        phases: phases(&prelude, outcome.watch.as_ref(), lockfile),
        total,
        delta,
    };

    // The manager's own lines end wherever they end. A summary that starts on
    // the next one reads as the last thing the manager said. A manager uf did
    // not narrate wrote straight to the terminal and uf never saw a line of
    // it, so the gap is assumed rather than counted.
    let spoken = echoed || outcome.watch.is_none();
    ui.render(|renderer, out| {
        if spoken {
            renderer.blank(out);
        }
        render_summary(renderer, out, &report);
    });
    Ok(())
}

/// Everything `uf install` has to say once the manager has exited.
struct InstallReport {
    manager: String,
    chosen_by: String,
    command: String,
    runtime: Option<String>,
    lockfile: String,
    phases: Vec<Phase>,
    total: Duration,
    delta: LockfileDelta,
}

/// Draw the summary.
///
/// Split out from the command so that a test can render it with
/// [`uf_term::Capabilities::plain`] and read the layout, rather than asserting
/// on whatever a package manager happened to install today.
fn render_summary(renderer: &Renderer, out: &mut String, report: &InstallReport) {
    let mut rows = vec![
        KeyValue::new("manager", &report.manager),
        KeyValue::toned("chosen by", &report.chosen_by, Tone::Muted),
        KeyValue::toned("command", &report.command, Tone::Path),
    ];
    if let Some(runtime) = &report.runtime {
        rows.push(KeyValue::toned("runtime", runtime, Tone::Path));
    }
    rows.push(KeyValue::toned("lockfile", &report.lockfile, Tone::Path));
    renderer.key_values(out, 2, &rows);

    let elapsed = format_duration(report.total);
    if report.delta.is_unchanged() {
        // The common case, and the one that has to stay cheap to read. There
        // is no delta to show, so the phases and the next steps would be a
        // report about nothing happening.
        renderer.blank(out);
        renderer.status(
            out,
            Status::Success,
            &format!("already up to date in {elapsed}"),
        );
        return;
    }

    renderer.blank(out);
    renderer.timings(out, 2, &report.phases, Some(report.total));

    // A lockfile uf does not parse gets no section at all rather than an empty
    // one: the install changed something — the file is a different size — and
    // uf has nothing more to say about it than the row above already did.
    if report.delta.detailed {
        renderer.blank(out);
        // Not "dependencies": these are the lockfile's rows, and a lockfile
        // records the whole resolved tree including the optional builds for
        // the platforms this machine is not. npm's own `added 16 packages`
        // counts what it unpacked. Both numbers are true and they are not the
        // same number, so they do not get the same word.
        renderer.heading(out, 2, "dependency tree");
        render_change_counts(renderer, out, &report.delta);
        render_change_table(renderer, out, &report.delta);
    }

    renderer.blank(out);
    renderer.heading(out, 2, "next steps");
    renderer.ordered_list(out, 4, &["uf dev", "uf check"]);

    renderer.blank(out);
    renderer.status(
        out,
        Status::Success,
        &format!("dependencies installed in {elapsed}"),
    );
}

/// The four counts, in one aligned block, omitting the kinds that did not
/// happen.
fn render_change_counts(renderer: &Renderer, out: &mut String, delta: &LockfileDelta) {
    let counts: Vec<(ChangeKind, String)> = ChangeKind::ALL
        .into_iter()
        .map(|kind| (kind, delta.count(kind)))
        .filter(|(_, count)| *count > 0)
        .map(|(kind, count)| (kind, count.to_string()))
        .collect();
    let rows: Vec<KeyValue<'_>> = counts
        .iter()
        .map(|(kind, count)| KeyValue::toned(kind.as_str(), count, Tone::Number))
        .collect();
    renderer.key_values(out, 4, &rows);
}

/// The changed packages themselves, capped.
fn render_change_table(renderer: &Renderer, out: &mut String, delta: &LockfileDelta) {
    if delta.changes.is_empty() {
        return;
    }
    // `4.17.20 -> 4.17.21` reads as one fact and `4.17.20 4.17.21` reads as
    // two, so the arrow is drawn in whichever vocabulary the terminal takes.
    let arrow = match renderer.glyph_set() {
        GlyphSet::Unicode => " → ",
        GlyphSet::Ascii => " -> ",
    };
    let versions: Vec<String> = delta
        .changes
        .iter()
        .take(CHANGES_SHOWN)
        .map(|change| match change.kind {
            ChangeKind::Added => change.after.to_string(),
            ChangeKind::Removed => change.before.to_string(),
            ChangeKind::Updated => format!("{}{arrow}{}", change.before, change.after),
            ChangeKind::Moved => change.after.to_string(),
        })
        .collect();

    renderer.blank(out);
    let mut table = Table::new(vec![
        Column::left(""),
        Column::left("package"),
        Column::left("version"),
    ]);
    for (change, version) in delta.changes.iter().zip(&versions) {
        table.push(vec![
            Cell::toned(change.kind.mark(), tone_of(change.kind)),
            Cell::new(&change.name),
            Cell::toned(version, Tone::Number),
        ]);
    }
    renderer.table(out, 4, &table);

    let hidden = delta.changes.len().saturating_sub(versions.len());
    if hidden > 0 {
        push_spaces(out, 4);
        renderer.line(out, renderer.theme().muted, &format!("and {hidden} more"));
    }
}

const fn tone_of(kind: ChangeKind) -> Tone {
    match kind {
        ChangeKind::Added => Tone::Good,
        ChangeKind::Removed => Tone::Bad,
        ChangeKind::Updated => Tone::Accent,
        ChangeKind::Moved => Tone::Muted,
    }
}

/// uf's own phases, then the manager's, then reading the lockfile back.
///
/// In the order they ran, which is not the order they were measured in: the
/// manager's phases are recovered from its output after it has exited, and a
/// timing block whose rows are out of sequence is a timing block nobody can
/// add up.
///
/// A phase that took no time is left out even when it happened. npm's audit
/// runs alongside the install rather than after it, so it holds the ladder for
/// no time at all on a fast run, and a row reading `audit ··· 0ns` claims a
/// measurement uf did not make.
fn phases(prelude: &[Phase], watch: Option<&InstallWatch>, lockfile: Duration) -> Vec<Phase> {
    let mut phases: Vec<Phase> = prelude.to_vec();
    if let Some(watch) = watch {
        phases.extend(
            watch
                .phases()
                .iter()
                .filter(|phase| phase.elapsed > Duration::ZERO)
                .map(|phase| Phase {
                    label: phase.phase.label(),
                    duration: phase.elapsed,
                }),
        );
    }
    phases.push(Phase {
        label: "lockfile",
        duration: lockfile,
    });
    phases
}

/// Why this manager, in a sentence rather than in a derived `Debug`.
fn chosen_by(source: &DetectionSource, substituted: bool) -> String {
    if substituted {
        // Something named uf's own resolver, which records what a workspace
        // declares and reaches no registry, so npm did the fetching. Saying
        // which thing named it matters: `uf install` writes `uf.lock` itself,
        // a step before this one, and a reader who is told only "npm" will go
        // looking for the lockfile they did not put there.
        let evidence = match source {
            DetectionSource::Default => {
                return "no lockfile or packageManager field".to_owned();
            }
            DetectionSource::ConfigOverride => "pm.packageManager",
            DetectionSource::PackageManagerField { .. } => "the packageManager field",
            DetectionSource::Lockfile { lockfile, .. } => lockfile.file_name(),
            DetectionSource::WorkspaceRoot { .. } => "the workspace root",
        };
        return format!("{evidence} names uf, whose resolver cannot fetch yet");
    }
    match source {
        DetectionSource::ConfigOverride => "pm.packageManager in uf.config.js".to_owned(),
        DetectionSource::PackageManagerField { spec, .. } => {
            format!("packageManager field: {}@{}", spec.manager, spec.version)
        }
        DetectionSource::Lockfile { lockfile, .. } => lockfile.file_name().to_owned(),
        DetectionSource::WorkspaceRoot { root, .. } => format!("workspace root {root}"),
        DetectionSource::Default => "uf's default".to_owned(),
    }
}

/// The JavaScript runtime uf resolved, and where it found it.
///
/// Looked up on `PATH` rather than executed: `node --version` is a process
/// spawn, and an install that is already up to date finishes in less time than
/// several of those take.
fn runtime_label(config: &uf_config::UniflowedConfig) -> Option<String> {
    let host = resolve_host(config).ok()?;
    Some(format!("{} · {}", host.name(), host.program))
}

/// The lockfile, its size, and how many packages it pins.
fn lockfile_label(root: &Utf8Path, snapshot: &LockfileSnapshot) -> String {
    let name = snapshot
        .path
        .strip_prefix(root)
        .unwrap_or(&snapshot.path)
        .to_string();
    if !snapshot.present {
        return format!("{name} · not written");
    }
    let size = ByteSize::from_bytes(snapshot.bytes);
    match snapshot.package_count() {
        Some(count) => format!("{name} · {} · {size}", plural(count, "package")),
        None => format!("{name} · {size}"),
    }
}

/// The live region, and the manager's own output going past it.
struct Screen<'a> {
    ui: &'a mut Ui,
    live: Live<std::io::Stderr>,
    renderer: Renderer,
    started: Instant,
    header: String,
    manager: String,
    rows: Vec<String>,
    /// Whether the manager said anything of its own.
    echoed: bool,
}

impl<'a> Screen<'a> {
    fn new(ui: &'a mut Ui, project: &str, manager: &str) -> Self {
        let live = ui.live();
        // The region's own capabilities, not stderr's: `Live` has already
        // decided how much colour and which glyphs it may use, and resolving
        // that a second time is how two halves of one row end up disagreeing.
        let renderer = Renderer::new(Capabilities::new(
            live.color(),
            live.glyphs(),
            Tty::Interactive,
        ));
        Self {
            ui,
            live,
            renderer,
            started: Instant::now(),
            header: format!("uf install · {project}"),
            manager: manager.to_owned(),
            rows: Vec::new(),
            echoed: false,
        }
    }

    /// Take the region down for good.
    fn close(&mut self) {
        self.live.finish();
    }

    /// Rebuild the frame from the ladder.
    fn compose(&mut self, watch: &InstallWatch) {
        frame(
            &self.renderer,
            &mut self.rows,
            &FrameHeader {
                title: &self.header,
                manager: &self.manager,
                elapsed: self.started.elapsed(),
            },
            self.live.spinner(),
            watch,
        );
    }
}

/// The line above the ladder: what is running, and how long it has been.
struct FrameHeader<'a> {
    title: &'a str,
    manager: &'a str,
    elapsed: Duration,
}

/// The whole live frame, one `String` per row.
///
/// A free function rather than a method, so a test can render the frame that
/// would have gone to a terminal. There is no other way to see it: a pseudo-
/// terminal is the only thing that makes `Live` draw at all, and a test that
/// needed one would be a test that does not run everywhere.
///
/// The row buffers are reused between frames — twelve frames a second for the
/// length of an install is not the place to allocate five strings each time.
fn frame(
    renderer: &Renderer,
    rows: &mut Vec<String>,
    header: &FrameHeader<'_>,
    spinner: &str,
    watch: &InstallWatch,
) {
    rows.resize(1 + watch.phases().len(), String::new());
    for row in rows.iter_mut() {
        row.clear();
    }
    let (header_row, ladder) = rows.split_first_mut().expect("a frame has a header");

    let right = format!("{} · {}", header.manager, format_duration(header.elapsed));
    push_spaces(header_row, INDENT);
    renderer
        .theme()
        .title
        .paint(renderer.color(), header.title, header_row);
    push_spaces(
        header_row,
        ROW_WIDTH.saturating_sub(INDENT + display_width(header.title) + display_width(&right)),
    );
    renderer
        .theme()
        .muted
        .paint(renderer.color(), &right, header_row);

    for (row, phase) in ladder.iter_mut().zip(watch.phases()) {
        phase_row(row, renderer, spinner, phase);
    }
}

impl InstallObserver for Screen<'_> {
    fn line(&mut self, stream: ManagerStream, text: &str) {
        // Off the screen first: a line printed into a region that is about to
        // be redrawn is a line the redraw erases.
        self.live.clear();
        self.echoed = true;
        let text = format!("{text}\n");
        match stream {
            ManagerStream::Stdout => self.ui.plain(&text),
            ManagerStream::Stderr => self.ui.plain_err(&text),
        }
    }

    fn progress(&mut self, watch: &InstallWatch) {
        if !self.live.is_due() {
            return;
        }
        self.compose(watch);
        let Self { live, rows, .. } = self;
        let frame: Vec<&str> = rows.iter().map(String::as_str).collect();
        live.draw(&frame);
    }
}

/// One phase's row: mark, label, what it is working on, how many, how long.
fn phase_row(row: &mut String, renderer: &Renderer, spinner: &str, phase: &PhaseProgress) {
    let glyphs = renderer.glyph_set();
    let dash = match glyphs {
        GlyphSet::Unicode => "—",
        GlyphSet::Ascii => "-",
    };
    let (mark, mark_style) = match phase.state {
        PhaseState::Done => (
            Status::Success.glyph(glyphs),
            renderer.status_style(Status::Success),
        ),
        PhaseState::Running => (spinner, renderer.theme().accent),
        PhaseState::Waiting => (
            Status::Skip.glyph(glyphs),
            renderer.status_style(Status::Skip),
        ),
    };
    let label_style = match phase.state {
        PhaseState::Running => renderer.theme().title,
        PhaseState::Done => renderer.theme().value,
        PhaseState::Waiting => renderer.theme().muted,
    };
    // The manager's own word for what it is on, or the phase's own when the
    // manager never says — but not before the phase has started, because a
    // subject under a waiting mark reads as work already happening.
    let detail = match (phase.detail.as_str(), phase.state) {
        ("", PhaseState::Waiting) => dash,
        ("", _) => phase.phase.subject().unwrap_or(dash),
        (detail, _) => detail,
    };
    let count = if phase.count == 0 {
        dash.to_owned()
    } else {
        phase.count.to_string()
    };
    let time = if phase.elapsed == Duration::ZERO {
        dash.to_owned()
    } else {
        format_duration(phase.elapsed)
    };

    push_spaces(row, INDENT);
    mark_style.paint(renderer.color(), mark, row);
    row.push(' ');
    cell(
        row,
        renderer,
        label_style,
        phase.phase.label(),
        LABEL_WIDTH,
        Align::Left,
    );
    row.push(' ');
    let detail_style = if detail == dash {
        renderer.theme().muted
    } else {
        renderer.theme().path
    };
    cell(
        row,
        renderer,
        detail_style,
        detail,
        DETAIL_WIDTH,
        Align::Left,
    );
    push_spaces(row, 2);
    cell(
        row,
        renderer,
        renderer.theme().number,
        &count,
        COUNT_WIDTH,
        Align::Right,
    );
    push_spaces(row, 2);
    cell(
        row,
        renderer,
        renderer.theme().number,
        &time,
        TIME_WIDTH,
        Align::Right,
    );
}

/// Append one padded, styled cell.
///
/// The padding is measured from the plain text and the styling wrapped around
/// it afterwards, because an escape sequence is not a column: padding a
/// pre-styled string to a width counts the colour codes into it and the
/// columns stop lining up.
fn cell(
    out: &mut String,
    renderer: &Renderer,
    style: Style,
    text: &str,
    width: usize,
    align: Align,
) {
    let text = truncate_to_width(text, width);
    let padding = width.saturating_sub(display_width(text));
    match align {
        Align::Right => {
            push_spaces(out, padding);
            style.paint(renderer.color(), text, out);
        }
        _ => {
            style.paint(renderer.color(), text, out);
            push_spaces(out, padding);
        }
    }
}

#[cfg(test)]
mod tests;
