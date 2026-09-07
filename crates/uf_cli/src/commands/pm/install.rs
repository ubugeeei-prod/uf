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

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use uf_bundle::ByteSize;
use uf_config::load_config;
use uf_pm::delta::{ChangeKind, LockfileDelta};
use uf_pm::{
    DetectionSource, InstallObserver, InstallWatch, LockfileSnapshot, ManagerStream, Operation,
    PackageManagerPlan, PhaseProgress, PhaseState, detect_package_manager, install_workspace,
    installable, run_watched,
};
use uf_term::{
    Align, Capabilities, Cell, Column, GlyphSet, KeyValue, Live, Phase, PhaseTimer, Renderer,
    Status, Style, Table, Tone, Tty, display_width, format_duration, push_spaces,
    truncate_to_width,
};

use super::scripts_allowed;
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

/// Columns of the live region, left to right, at their natural widths.
const INDENT: usize = 2;
const LABEL_WIDTH: usize = 8;
const DETAIL_WIDTH: usize = 38;
const COUNT_WIDTH: usize = 6;
const TIME_WIDTH: usize = 7;
/// Everything before the detail column: the indent, the mark, and the label.
const LEFT_WIDTH: usize = INDENT + 2 + LABEL_WIDTH;
/// The whole region's width, which must stay inside [`uf_term::LIVE_WIDTH`].
const ROW_WIDTH: usize = LEFT_WIDTH + 1 + DETAIL_WIDTH + 2 + COUNT_WIDTH + 2 + TIME_WIDTH;
/// The narrowest a detail column is worth drawing.
///
/// Six columns of `react-dom@18.3.1` is `react-`, which is still one package
/// rather than another. Below that it is a cut word that could be almost
/// anything, taking room from the count and the clock — and those two are
/// exactly as readable at six columns as at sixty.
const MIN_DETAIL_WIDTH: usize = 6;
/// The narrowest a phase label is cut to before the row gives up on fitting.
const MIN_LABEL_WIDTH: usize = 4;

/// Which of the ladder's columns fit, and how wide each one is.
///
/// A live region is redrawn by moving the cursor back up over the rows it drew
/// last time, and that arithmetic is only correct while every row is one
/// physical line. A row wider than the window wraps, the terminal counts two
/// lines where this counts one, and the region walks down the screen. So the
/// ladder is not merely clipped to the terminal — it is *laid out* for it, and
/// the columns leave in the order a reader would miss them least.
///
/// A `0` means the column is not drawn at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ladder {
    /// Leading spaces, and the mark that follows them.
    indent: usize,
    /// Whether there is room for the phase mark at all.
    mark: bool,
    label: usize,
    detail: usize,
    count: usize,
    time: usize,
}

impl Ladder {
    /// The widest ladder that fits in `width` columns.
    ///
    /// The order things go in: the package name shrinks first, because it is
    /// the only elastic column and a shortened name still identifies work;
    /// then it goes entirely; then the count, which the phase's own mark
    /// already half-implies; then the clock, which is the last thing worth
    /// losing because it is what tells a reader whether an install is stuck.
    fn for_width(width: usize) -> Self {
        let full = Self {
            indent: INDENT,
            mark: true,
            label: LABEL_WIDTH,
            detail: DETAIL_WIDTH,
            count: COUNT_WIDTH,
            time: TIME_WIDTH,
        };
        if width >= ROW_WIDTH {
            return full;
        }
        let numbers = 2 + COUNT_WIDTH + 2 + TIME_WIDTH;
        let detail = width.saturating_sub(LEFT_WIDTH + 1 + numbers);
        if detail >= MIN_DETAIL_WIDTH {
            return Self { detail, ..full };
        }
        if width >= LEFT_WIDTH + numbers {
            return Self { detail: 0, ..full };
        }
        if width >= LEFT_WIDTH + 2 + TIME_WIDTH {
            return Self {
                detail: 0,
                count: 0,
                ..full
            };
        }
        let bare = Self {
            detail: 0,
            count: 0,
            time: 0,
            ..full
        };
        if width >= LEFT_WIDTH {
            return bare;
        }
        // The indent and the mark are two columns each and are the first thing
        // a phone-sized pane cannot afford. Below the width at which a cut
        // label would still read, they go and the label takes the whole row —
        // "reso" says more than two spaces and a dot do.
        let room = width.saturating_sub(INDENT + 2);
        if room >= MIN_LABEL_WIDTH {
            return Self {
                label: room,
                ..bare
            };
        }
        Self {
            indent: 0,
            mark: false,
            label: width,
            ..bare
        }
    }
}

/// `uf install`, and `uf install --frozen-lockfile`.
pub(crate) fn install(cwd: &Utf8Path, ui: &mut Ui, frozen: bool) -> Result<()> {
    let mut timer = PhaseTimer::start();
    let resolved = timer.measure("config", || load_config(cwd))?;
    let plan = PackageManagerPlan::infer_from_config(&resolved.config);

    // A manifest that declares scripts is refused before anything is fetched,
    // the way it was when uf planned the install itself. `--ignore-scripts`
    // inside `run_watched` covers the dependencies; this covers the project.
    //
    // `uf.lock` is a pure function of those manifests, so writing it here is
    // deterministic — but a frozen install promises to change nothing, and a
    // `uf.lock` that comes out different is the workspace having drifted from
    // it. `guard_uf_lock` puts the old one back and says so.
    let guard = UfLockGuard::read(&resolved.root, &resolved.config, frozen);
    install_workspace(&resolved.root, &resolved.config)?;
    guard.check()?;

    // Which manager is about to run has to be settled here rather than left to
    // the runner, because the lockfile it is about to rewrite must be read
    // first: a "before" state read afterwards is the "after" state.
    let (manager, _) = installable(&detect_package_manager(&resolved.root));
    let before = uf_pm::delta::snapshot(&resolved.root, manager);

    // Dependency confusion, checked before the manager is allowed to install
    // what this lockfile pins. A lockfile that resolves a bound scope from
    // somewhere else is the attack having already succeeded once; installing
    // from it is letting it succeed again, on this machine, now.
    let routing = uf_pm::RegistryRouting::from_config(&resolved.config);
    refuse_confusion(&routing, &before)?;
    timer.lap("workspace");
    let prelude = timer.phases().to_vec();

    let operation = if frozen {
        Operation::InstallFrozen
    } else {
        Operation::Install
    };
    let heading = if frozen {
        "uf install --frozen-lockfile"
    } else {
        "uf install"
    };
    let project = project_label(&resolved.root).to_string();
    ui.render(|renderer, out| {
        brand::render_product_card(renderer, out, "uf install");
        renderer.blank(out);
        renderer.banner(out, heading, Some(&project));
        renderer.blank(out);
    });

    let manager_label = manager.to_string();
    let (outcome, echoed) = {
        let mut screen = Screen::new(ui, &project, &manager_label);
        let run = run_watched(
            &resolved.root,
            operation,
            scripts_allowed(&resolved.root, manager, &plan)?,
            &mut screen,
        );
        screen.close();
        (run, screen.echoed)
    };
    let outcome = outcome.map_err(|error| frozen_hint(error, frozen))?;

    let read_back = Instant::now();
    let after = uf_pm::delta::snapshot(&resolved.root, manager);
    let delta = uf_pm::delta::diff(&before, &after);
    let lockfile = read_back.elapsed();

    // And again on what the manager actually wrote. The first check covers a
    // lockfile that arrived with the repository; this one covers the resolution
    // the manager has just done — a project with no lockfile at all had nothing
    // for the first check to read.
    refuse_confusion(&routing, &after)?;
    let provenance = check_provenance(&resolved.config, &routing, &delta, &after)?;
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
        provenance,
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

/// `uf.lock` as it stood before `install_workspace` rewrote it.
///
/// Only a frozen install has anything to guard: everywhere else rewriting
/// `uf.lock` is the point. Reading it costs one `read` of a file that is about
/// to be read again, so it is not read at all unless `--frozen-lockfile` was
/// asked for.
struct UfLockGuard {
    path: Utf8PathBuf,
    before: Option<Option<Vec<u8>>>,
}

impl UfLockGuard {
    /// Read `uf.lock`, when there is a reason to.
    fn read(root: &Utf8Path, config: &uf_config::UniflowedConfig, frozen: bool) -> Self {
        let path = root.join(config.pm.lockfile.as_str());
        let before = frozen.then(|| std::fs::read(&path).ok());
        Self { path, before }
    }

    /// Fail when the rewrite changed it, having first put the old one back.
    ///
    /// Putting it back is what makes this a check rather than a side effect: a
    /// CI job that fails here must leave the tree it was handed, so the next
    /// step can print a diff of the real file.
    fn check(&self) -> Result<()> {
        let Some(before) = &self.before else {
            return Ok(());
        };
        let after = std::fs::read(&self.path).ok();
        if &after == before {
            return Ok(());
        }
        match before {
            Some(bytes) => std::fs::write(&self.path, bytes)
                .with_context(|| format!("failed to restore {}", self.path))?,
            None => std::fs::remove_file(&self.path)
                .with_context(|| format!("failed to remove {}", self.path))?,
        }
        let name = self.path.file_name().unwrap_or("uf.lock");
        Err(anyhow!(
            "{name} does not match this workspace's package manifests\n\n  \
             --frozen-lockfile installs what the lockfiles pin and changes nothing\n  \
             run `uf install` and commit the {name} it writes"
        ))
    }
}

/// Say what a frozen install's failure usually means.
///
/// The manager has already printed its own diagnosis — `npm ci`'s is three
/// lines naming the packages that are missing from the lockfile — and this
/// adds the sentence it does not: what to run to fix it.
fn frozen_hint(error: uf_pm::ManagerRunError, frozen: bool) -> anyhow::Error {
    if !frozen || !matches!(error, uf_pm::ManagerRunError::Failed { .. }) {
        return anyhow!(error);
    }
    anyhow!(
        "{error}\n\n  \
         a frozen install refuses a lockfile the manifests have moved away from\n  \
         the manager named the packages above; run `uf install` and commit the lockfile it writes"
    )
}

/// How many attestations one install will read.
///
/// A first install of a large project changes everything in the tree, and one
/// request per package would be thousands. This is the ceiling past which uf
/// stops asking and says how many it looked at — a bounded check that reports
/// its own bound is worth more than an unbounded one people turn off.
const MAX_PROVENANCE_READS: usize = 250;

/// Refuse a lockfile that resolves a bound scope from somewhere else.
///
/// A hard error rather than a warning, and before the install rather than
/// after: a warning on this one is a line in a CI log above a successful
/// install of the attacker's package. See [`uf_pm::confusion`].
fn refuse_confusion(
    routing: &uf_pm::RegistryRouting,
    snapshot: &uf_pm::LockfileSnapshot,
) -> Result<()> {
    let found = uf_pm::confusion::check(routing, snapshot);
    if found.is_empty() {
        return Ok(());
    }
    // Every one of them, not the first: a reader who fixes one and runs again
    // to find the next has been told the same thing three times.
    let listed = found
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n  ");
    // The verb has to agree with the count. `plural` gives the noun phrase and
    // nothing after it, and "2 packages resolves" is the kind of sentence a
    // reader stops trusting the rest of.
    let headline = if found.len() == 1 {
        format!(
            "{} resolves from a registry its scope is not bound to",
            plural(1, "package")
        )
    } else {
        format!(
            "{} resolve from registries their scopes are not bound to",
            plural(found.len(), "package")
        )
    };
    Err(anyhow!(
        "{headline}\n\n  {listed}\n\n  {}",
        uf_pm::confusion::REMEDY
    ))
}

/// What the provenance reads found.
///
/// Counts, plus the names of the packages that arrived without an attestation:
/// the count is the fact, and the names are what makes a change from "attested"
/// to "not attested" visible in a diff of two install logs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct ProvenanceSummary {
    /// Packages whose attestation was read and is about the tarball installed.
    attested: usize,
    /// Packages the registry publishes no provenance for, by name.
    unattested: Vec<String>,
    /// Packages uf could not ask about, because the registry did not answer.
    unavailable: usize,
    /// Packages that were eligible and were never asked about, because
    /// [`MAX_PROVENANCE_READS`] was reached.
    ///
    /// Counted and printed rather than dropped: a bound nobody is told about is
    /// a report that overstates how much of the tree was looked at, which is the
    /// one thing a security summary must not do.
    past_the_cap: usize,
    /// One repository an attestation named, so the reader can see the shape of
    /// the answer without `--json`.
    origin: Option<String>,
}

impl ProvenanceSummary {
    /// How many packages were looked at at all.
    fn checked(&self) -> usize {
        self.attested + self.unattested.len() + self.unavailable
    }
}

/// Read the provenance of every package this install brought in or moved.
///
/// Not of every package in the tree: an install that changed nothing has
/// nothing new to check, and the moment a new artefact arrives is the moment
/// the question is worth a request. `pm.provenance: "off"` skips it entirely,
/// for a machine with no route to a registry.
///
/// # Errors
///
/// When a package publishes an attestation that is not about the tarball that
/// was installed. That is the whole point and it is not a warning.
fn check_provenance(
    config: &uf_config::UniflowedConfig,
    routing: &uf_pm::RegistryRouting,
    delta: &LockfileDelta,
    after: &uf_pm::LockfileSnapshot,
) -> Result<ProvenanceSummary> {
    let mut summary = ProvenanceSummary::default();
    if !config.pm.provenance.reads_attestations() || !delta.detailed {
        return Ok(summary);
    }
    let arrived: std::collections::BTreeSet<&str> = delta
        .changes
        .iter()
        .filter(|change| matches!(change.kind, ChangeKind::Added | ChangeKind::Updated))
        .map(|change| change.name.as_str())
        .collect();
    if arrived.is_empty() {
        return Ok(summary);
    }
    // The subjects first, then the requests: a tree that nests the same version
    // twice is one artefact and one request, and the whole set has to be known
    // before it can be asked about a few at a time.
    let mut seen: std::collections::BTreeSet<(&str, &str)> = std::collections::BTreeSet::new();
    let mut asked: Vec<uf_pm::Subject> = Vec::new();
    for entry in after.entries.values() {
        // A workspace link came from this repository; a package with no
        // integrity hash, or one the manager did not fetch over TLS, has no
        // artefact an attestation could be about.
        if entry.link || !arrived.contains(entry.name.as_str()) {
            continue;
        }
        let Some(integrity) = entry.integrity.clone() else {
            continue;
        };
        if !entry
            .resolved
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"))
        {
            continue;
        }
        if !seen.insert((entry.name.as_str(), entry.version.as_str())) {
            continue;
        }
        // Past the ceiling the scan keeps going rather than breaking, because
        // the number it stopped at is the number the summary has to print, and
        // a loop that broke would have nothing to print but the ceiling.
        if asked.len() >= MAX_PROVENANCE_READS {
            summary.past_the_cap += 1;
            continue;
        }
        asked.push(uf_pm::Subject {
            name: entry.name.clone(),
            version: entry.version.clone(),
            // The lockfile's own hash: this binds the attestation to the bytes
            // being installed, which is the strong form of the question.
            integrity: Some(integrity),
        });
    }
    if asked.is_empty() {
        return Ok(summary);
    }
    // Each name goes to the registry the *project* trusts, and is asked about
    // the tarball that was actually installed. A mirror that served different
    // bytes fails the digest comparison, which is the answer that matters.
    for (subject, answer) in asked
        .iter()
        .zip(uf_pm::provenance::read_many(routing, &asked))
    {
        match answer? {
            uf_pm::Outcome::Attested(provenance) => {
                summary.attested += 1;
                if summary.origin.is_none() {
                    summary.origin = provenance.origin().map(|origin| origin.to_string());
                }
            }
            uf_pm::Outcome::Unattested => summary.unattested.push(subject.name.to_string()),
            uf_pm::Outcome::Unavailable(_) => summary.unavailable += 1,
        }
    }
    Ok(summary)
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
    provenance: ProvenanceSummary,
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

    render_provenance(renderer, out, &report.provenance);

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

/// What the provenance reads found, when any were made.
///
/// Nothing at all when nothing was checked: an install that changed no
/// registry package has no attestations to have read, and a `0 attested` row
/// on it would read as a finding rather than as an absence of work.
///
/// The unattested names are listed rather than only counted, because the
/// signal a taken-over publishing account produces is a package that *had*
/// provenance and stops having it — and a reader can only see that if the
/// package is named.
fn render_provenance(renderer: &Renderer, out: &mut String, summary: &ProvenanceSummary) {
    if summary.checked() == 0 {
        return;
    }
    renderer.blank(out);
    renderer.heading(out, 2, "provenance");
    let attested = summary.attested.to_string();
    let unattested = summary.unattested.len().to_string();
    let unavailable = summary.unavailable.to_string();
    let mut rows = vec![KeyValue::toned("attested", &attested, Tone::Number)];
    if !summary.unattested.is_empty() {
        rows.push(KeyValue::toned("unattested", &unattested, Tone::Number));
    }
    if summary.unavailable > 0 {
        rows.push(KeyValue::toned("unknown", &unavailable, Tone::Number));
    }
    if let Some(origin) = &summary.origin {
        rows.push(KeyValue::toned("built by", origin, Tone::Path));
    }
    renderer.key_values(out, 4, &rows);

    // The bound this check has, said out loud. `250 attested` printed under a
    // first install of nine hundred packages is a report of how far uf looked
    // that reads as a report of the tree, and a security summary that overstates
    // its own coverage is worse than one that admits a ceiling.
    if summary.past_the_cap > 0 {
        renderer.status(
            out,
            Status::Info,
            &format!(
                "{} were not read: uf reads at most {MAX_PROVENANCE_READS} attestations in one install",
                plural(summary.past_the_cap, "package")
            ),
        );
    }

    if summary.unattested.is_empty() {
        return;
    }
    let listed: Vec<&str> = summary
        .unattested
        .iter()
        .take(CHANGES_SHOWN)
        .map(String::as_str)
        .collect();
    renderer.blank(out);
    renderer.bullet_list(out, 4, &listed);
    if summary.unattested.len() > listed.len() {
        renderer.status(
            out,
            Status::Info,
            &format!(
                "and {} more with no attestation",
                summary.unattested.len() - listed.len()
            ),
        );
    }
    // Not a failure. Most of npm publishes no provenance, and a tool that
    // refused every package without one is a tool nobody runs.
    renderer.status(
        out,
        Status::Info,
        "no provenance is published for these; an attestation names the repository and \
         workflow that built a tarball, and most of npm has none",
    );
}

/// The four counts, in one aligned block, omitting the kinds that did not
/// happen.
///
/// Shared with `uf add`, `uf remove` and `uf update`: the same lockfile, read
/// the same way, deserves the same block.
pub(super) fn render_change_counts(renderer: &Renderer, out: &mut String, delta: &LockfileDelta) {
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
pub(super) fn render_change_table(renderer: &Renderer, out: &mut String, delta: &LockfileDelta) {
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
pub(super) fn chosen_by(source: &DetectionSource, substituted: bool) -> String {
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
pub(super) fn lockfile_label(root: &Utf8Path, snapshot: &LockfileSnapshot) -> String {
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
            // The width the region will actually draw at, which is the
            // terminal's when the terminal is narrower than the ceiling. Asking
            // `Live` rather than assuming `ROW_WIDTH` is the whole point: a row
            // this function builds too wide is a row the terminal wraps, and a
            // wrapped row breaks every redraw after it.
            self.live.width(),
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
    width: usize,
    spinner: &str,
    watch: &InstallWatch,
) {
    rows.resize(1 + watch.phases().len(), String::new());
    for row in rows.iter_mut() {
        row.clear();
    }
    let (header_row, ladder) = rows.split_first_mut().expect("a frame has a header");

    header_line(header_row, renderer, header, width);

    let columns = Ladder::for_width(width);
    for (row, phase) in ladder.iter_mut().zip(watch.phases()) {
        phase_row(row, renderer, columns, spinner, phase);
    }
}

/// The header: the title on the left, the manager and the clock on the right.
///
/// The right-hand half is dropped rather than squeezed when the two would meet,
/// because a title and a manager name run together read as one string. The
/// title itself is cut last, and only when it alone is wider than the window.
fn header_line(row: &mut String, renderer: &Renderer, header: &FrameHeader<'_>, width: usize) {
    let right = format!("{} · {}", header.manager, format_duration(header.elapsed));
    let indent = INDENT.min(width);
    let title = truncate_to_width(header.title, width - indent);
    let title_width = display_width(title);
    // One column of daylight between the two halves, so they stay two things.
    let room = width - indent - title_width;
    push_spaces(row, indent);
    renderer.theme().title.paint(renderer.color(), title, row);
    if room <= display_width(&right) {
        return;
    }
    push_spaces(row, room - display_width(&right));
    renderer.theme().muted.paint(renderer.color(), &right, row);
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
fn phase_row(
    row: &mut String,
    renderer: &Renderer,
    columns: Ladder,
    spinner: &str,
    phase: &PhaseProgress,
) {
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

    push_spaces(row, columns.indent);
    if columns.mark {
        mark_style.paint(renderer.color(), mark, row);
        row.push(' ');
    }
    cell(
        row,
        renderer,
        label_style,
        phase.phase.label(),
        columns.label,
        Align::Left,
    );
    if columns.detail > 0 {
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
            columns.detail,
            Align::Left,
        );
    }
    if columns.count > 0 {
        push_spaces(row, 2);
        cell(
            row,
            renderer,
            renderer.theme().number,
            &count,
            columns.count,
            Align::Right,
        );
    }
    if columns.time > 0 {
        push_spaces(row, 2);
        cell(
            row,
            renderer,
            renderer.theme().number,
            &time,
            columns.time,
            Align::Right,
        );
    }
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
