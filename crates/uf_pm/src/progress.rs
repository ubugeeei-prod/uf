//! Reading a package manager's output while it is still running.
//!
//! [`run_install`](crate::run_install) hands the terminal to somebody else's
//! process and reports what happened once it is over. That is the right shape
//! for a manager whose own output is good, and the wrong one for npm, which
//! prints nothing at all between "starting" and "done" when its stdout is a
//! pipe. This module is what lets `uf install` say what is happening while it
//! happens.
//!
//! # Only lines uf asked for are ever swallowed
//!
//! Narration needs evidence, and npm's evidence is behind `--loglevel=http`:
//! one line per registry request, which npm does not print otherwise.
//! [`Reader::classify`] therefore divides the manager's output in exactly one
//! place — the lines uf's own flag caused (`npm http …`) become
//! [`ManagerEvent`]s and are not reprinted; **every other line is passed
//! through untouched**, on the stream the manager wrote it to. A reader who
//! runs `uf install` sees precisely what npm would have said, plus a picture
//! of what it is doing.
//!
//! # Untrusted text
//!
//! A package name comes from a registry, and it arrives here inside a line
//! that `uf install` is about to draw into a region it navigates with cursor
//! movements. A name carrying its own escape sequence would move that cursor.
//! [`safe_label`] is where that stops: it is applied to every name taken out
//! of a manager's line, and nothing else in this module produces display text.
//!
//! # Phases
//!
//! npm reports two of the three phases anyone cares about. It says which
//! metadata it fetched (resolving) and which archives it fetched (fetching);
//! it says nothing at all about unpacking them into `node_modules`. That
//! third phase is inferred in [`InstallWatch::idle`] from the only evidence
//! there is — the registry going quiet while the process keeps running — and
//! the inference is provisional: one more archive and the ladder steps back to
//! fetching.

use std::time::{Duration, Instant};

use crate::detect::PackageManager;

/// How long the registry must stay quiet, once archives have started
/// arriving, before uf calls what the manager is doing "linking".
///
/// Long enough that a slow response does not flicker the ladder, short enough
/// that the last phase of a real install is not spent claiming to fetch. npm
/// fetches with a concurrency of well over one, so a gap this long inside the
/// fetch phase means the fetches are finished.
pub const LINKING_IDLE: Duration = Duration::from_millis(750);

/// The longest display label uf will build out of a manager's line.
///
/// A scoped name plus a version is comfortably inside this; a registry that
/// answers with something enormous does not get to allocate for it.
pub const MAX_LABEL: usize = 80;

/// One thing a package manager said it was doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerEvent {
    /// A package's metadata was read: the manager is still deciding what to
    /// install.
    Resolved {
        /// The package, as a label safe to draw.
        package: String,
    },
    /// A package's archive arrived.
    Fetched {
        /// The package and version, as a label safe to draw.
        package: String,
        /// Whether it came from the manager's cache rather than the network.
        cached: bool,
    },
    /// The manager asked the registry about known vulnerabilities.
    Audited,
    /// A line the manager would have printed with or without uf watching.
    ///
    /// Never consumed: the caller prints it.
    Passthrough,
}

/// Which managers `uf install` can narrate, and how to make them talk.
///
/// Deliberately not a guess. pnpm, Yarn and Bun all draw a good install
/// themselves, and taking their terminal away to replace it with uf's would be
/// a downgrade dressed as a feature; they keep their own output and uf keeps
/// the summary afterwards. npm is the one that says nothing, and it is also
/// the manager uf substitutes when a project evidences none, so it is the one
/// worth reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    /// npm's `--loglevel=http` lines.
    Npm,
}

impl Reader {
    /// The reader for `manager`, or `None` when uf should stand aside.
    #[must_use]
    pub const fn for_manager(manager: PackageManager) -> Option<Self> {
        match manager {
            PackageManager::Npm => Some(Self::Npm),
            _ => None,
        }
    }

    /// The argument that makes the manager report what it is doing.
    ///
    /// Static, like every other argument uf passes a manager: nothing read
    /// from a manifest reaches a command line.
    ///
    /// A manager that no longer understands it does not fail: npm answers an
    /// unrecognised log level with `invalid config` on stderr and installs
    /// anyway. The ladder would then sit at "resolving" for the whole run,
    /// which is a duller screen and not a broken install.
    #[must_use]
    pub const fn verbosity_argument(self) -> &'static str {
        match self {
            Self::Npm => "--loglevel=http",
        }
    }

    /// Classify one line of the manager's output.
    ///
    /// [`ManagerEvent::Passthrough`] means the line is the manager's own and
    /// must be printed; every other answer means uf asked for the line and
    /// owns it.
    #[must_use]
    pub fn classify(self, line: &str) -> ManagerEvent {
        match self {
            Self::Npm => classify_npm(line),
        }
    }
}

/// npm at `--loglevel=http` prefixes every request it makes with this.
const NPM_HTTP: &str = "npm http ";
/// The path npm posts a dependency tree to for its advisory report.
const ADVISORY_PATH: &str = "/npm/v1/security/advisories";
/// The separator npm's registry puts between a package's name and its tarball.
const TARBALL_MARKER: &str = "/-/";

fn classify_npm(line: &str) -> ManagerEvent {
    let Some(rest) = line.strip_prefix(NPM_HTTP) else {
        return ManagerEvent::Passthrough;
    };
    let Some(url) = first_url(rest) else {
        // `npm http` with no URL in it is not a request line uf recognises,
        // and a line uf cannot read is a line uf must not eat.
        return ManagerEvent::Passthrough;
    };
    if url.contains(ADVISORY_PATH) {
        return ManagerEvent::Audited;
    }
    let path = registry_path(url);
    match path.split_once(TARBALL_MARKER) {
        Some((name, archive)) => ManagerEvent::Fetched {
            package: safe_label(&tarball_label(name, archive)),
            cached: rest.starts_with("cache ") || line.ends_with("(cache hit)"),
        },
        None => ManagerEvent::Resolved {
            package: safe_label(&decode_slashes(path)),
        },
    }
}

/// The first `http(s)` URL in `text`, up to the next space.
fn first_url(text: &str) -> Option<&str> {
    let start = text.find("https://").or_else(|| text.find("http://"))?;
    let rest = &text[start..];
    Some(match rest.find(' ') {
        Some(end) => &rest[..end],
        None => rest,
    })
}

/// The path a registry URL addresses, without its scheme, host, or query.
fn registry_path(url: &str) -> &str {
    let after_scheme = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .trim_start_matches('/');
    let path = after_scheme
        .split_once('/')
        .map_or("", |(_, path)| path)
        .trim_start_matches('/');
    path.split(['?', '#']).next().unwrap_or(path)
}

/// `("react", "react-18.3.1.tgz")` reads as `react@18.3.1`.
///
/// The version is recovered from the archive's own name rather than from the
/// line, because npm writes the version in exactly one place a caller can rely
/// on. An archive that does not follow the convention keeps its package name
/// and loses the version, which is the right way round: a name with no version
/// is still the answer to "what is it downloading".
fn tarball_label(name: &str, archive: &str) -> String {
    let name = decode_slashes(name);
    let last = name.rsplit('/').next().unwrap_or(&name).to_owned();
    let file = archive.strip_suffix(".tgz").unwrap_or(archive);
    match file.strip_prefix(&format!("{last}-")) {
        Some(version) if !version.is_empty() => format!("{name}@{version}"),
        _ => name,
    }
}

/// Undo the one escape npm applies to a scoped package's path.
fn decode_slashes(path: &str) -> String {
    if !path.contains("%2f") && !path.contains("%2F") {
        return path.to_owned();
    }
    path.replace("%2f", "/").replace("%2F", "/")
}

/// A package name reduced to something that can be drawn.
///
/// Registry content is untrusted input that uf is about to write into a
/// terminal region it steers with cursor movements, so a name is stripped of
/// every control character — an escape sequence in a package name would
/// otherwise move that cursor — and cut to [`MAX_LABEL`] columns.
#[must_use]
pub fn safe_label(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(MAX_LABEL));
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        if out.chars().count() == MAX_LABEL {
            break;
        }
        out.push(ch);
    }
    out
}

/// The phases `uf install` draws, in the order a manager reaches them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InstallPhase {
    /// Reading package metadata to decide what the tree contains.
    Resolve,
    /// Downloading the archives that tree names.
    Fetch,
    /// Writing them into `node_modules`.
    Link,
    /// Asking the registry about known vulnerabilities.
    Audit,
}

impl InstallPhase {
    /// Every phase, in ladder order.
    pub const ALL: [Self; 4] = [Self::Resolve, Self::Fetch, Self::Link, Self::Audit];

    /// The label uf draws for this phase, and the label its timing carries.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Fetch => "fetch",
            Self::Link => "link",
            Self::Audit => "audit",
        }
    }

    /// The noun the phase's counter counts.
    #[must_use]
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Resolve => "manifest",
            Self::Fetch => "package",
            Self::Link => "package",
            Self::Audit => "report",
        }
    }

    /// What the phase is working on, when the manager never names it.
    ///
    /// npm reports which manifest it read and which archive it fetched, so
    /// those two phases always have something more specific to say. It reports
    /// nothing at all about unpacking or about the advisory request, and a row
    /// that is running with a dash where its subject goes reads as a row that
    /// is stuck.
    #[must_use]
    pub const fn subject(self) -> Option<&'static str> {
        match self {
            Self::Resolve | Self::Fetch => None,
            Self::Link => Some("node_modules"),
            Self::Audit => Some("registry advisories"),
        }
    }
}

/// Where one phase has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseState {
    /// Not reached yet.
    Waiting,
    /// The phase the manager is in.
    Running,
    /// Reached, and left behind.
    Done,
}

/// One phase's progress: how far it got, and how long it has had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseProgress {
    /// Which phase.
    pub phase: InstallPhase,
    /// Whether it is waiting, running, or finished.
    pub state: PhaseState,
    /// How many of [`InstallPhase::unit`] it has seen.
    pub count: usize,
    /// Wall time spent with this phase running.
    pub elapsed: Duration,
    /// The last thing the manager named in this phase.
    pub detail: String,
}

impl PhaseProgress {
    fn new(phase: InstallPhase) -> Self {
        Self {
            phase,
            state: PhaseState::Waiting,
            count: 0,
            elapsed: Duration::ZERO,
            detail: String::new(),
        }
    }
}

/// The phase ladder, driven by a manager's output.
///
/// Time is passed in rather than read, so the whole state machine is a pure
/// function of its events and can be tested without sleeping.
#[derive(Debug, Clone)]
pub struct InstallWatch {
    phases: Vec<PhaseProgress>,
    current: usize,
    since: Instant,
    last_event: Instant,
    cached: usize,
}

impl InstallWatch {
    /// A ladder that starts resolving at `now`.
    #[must_use]
    pub fn start(now: Instant) -> Self {
        let mut phases: Vec<PhaseProgress> = InstallPhase::ALL
            .iter()
            .copied()
            .map(PhaseProgress::new)
            .collect();
        phases[0].state = PhaseState::Running;
        Self {
            phases,
            current: 0,
            since: now,
            last_event: now,
            cached: 0,
        }
    }

    /// Record one event at `now`.
    ///
    /// [`ManagerEvent::Passthrough`] is not this method's business and is
    /// ignored, so a caller may hand it every line it reads.
    pub fn observe(&mut self, event: &ManagerEvent, now: Instant) {
        let phase = match event {
            ManagerEvent::Resolved { package } => {
                self.record(InstallPhase::Resolve, package);
                // npm keeps reading metadata long after the first archive
                // arrives, so a late manifest must not drag the ladder
                // backwards; it only holds the ladder where it already is.
                if self.current > index_of(InstallPhase::Resolve) {
                    self.last_event = now;
                    return;
                }
                InstallPhase::Resolve
            }
            ManagerEvent::Fetched { package, cached } => {
                if *cached {
                    self.cached += 1;
                }
                self.record(InstallPhase::Fetch, package);
                InstallPhase::Fetch
            }
            ManagerEvent::Audited => {
                self.record(InstallPhase::Audit, "");
                // The audit runs alongside the install rather than after it,
                // so it takes the ladder only once the fetching is over.
                if self.current < index_of(InstallPhase::Link) {
                    self.last_event = now;
                    return;
                }
                InstallPhase::Audit
            }
            ManagerEvent::Passthrough => return,
        };
        self.enter(index_of(phase), now);
        self.last_event = now;
    }

    /// Let the clock move without an event.
    ///
    /// Two jobs: keep the running phase's elapsed time honest between events,
    /// and make the call about linking. uf cannot see npm unpack an archive —
    /// npm does not say — but it can see the registry stop answering while the
    /// process is still alive, and what npm does then is write `node_modules`.
    pub fn idle(&mut self, now: Instant) {
        if self.current == index_of(InstallPhase::Fetch)
            && self.phases[self.current].count > 0
            && now.saturating_duration_since(self.last_event) >= LINKING_IDLE
        {
            self.enter(index_of(InstallPhase::Link), now);
        }
        self.accrue(now);
    }

    /// Close the ladder out at `now`, marking every reached phase done.
    pub fn finish(&mut self, now: Instant) {
        self.accrue(now);
        for phase in &mut self.phases {
            if matches!(phase.state, PhaseState::Running) {
                phase.state = PhaseState::Done;
            }
        }
    }

    /// The phase the manager is in.
    #[must_use]
    pub fn current(&self) -> InstallPhase {
        self.phases[self.current].phase
    }

    /// Every phase, in ladder order.
    #[must_use]
    pub fn phases(&self) -> &[PhaseProgress] {
        &self.phases
    }

    /// How many archives came from the manager's cache rather than the
    /// network.
    #[must_use]
    pub fn cached(&self) -> usize {
        self.cached
    }

    /// How many archives the manager fetched.
    #[must_use]
    pub fn fetched(&self) -> usize {
        self.phases[index_of(InstallPhase::Fetch)].count
    }

    fn record(&mut self, phase: InstallPhase, detail: &str) {
        let entry = &mut self.phases[index_of(phase)];
        entry.count += 1;
        if !detail.is_empty() {
            entry.detail.clear();
            entry.detail.push_str(detail);
        }
    }

    /// Make `next` the running phase, banking what the previous one spent.
    fn enter(&mut self, next: usize, now: Instant) {
        self.accrue(now);
        if next == self.current {
            return;
        }
        // Exactly one phase runs at a time. A ladder that steps backwards —
        // one late archive after uf has called it linking — must put the phase
        // it left down before it picks the earlier one up.
        for phase in &mut self.phases {
            if matches!(phase.state, PhaseState::Running) {
                phase.state = PhaseState::Done;
            }
        }
        // Everything before the new phase is finished too, including a phase
        // the manager skipped past without ever evidencing.
        for index in 0..next {
            self.phases[index].state = PhaseState::Done;
        }
        self.phases[next].state = PhaseState::Running;
        self.current = next;
    }

    /// Charge the time since the last accounting to the running phase.
    fn accrue(&mut self, now: Instant) {
        let spent = now.saturating_duration_since(self.since);
        self.phases[self.current].elapsed += spent;
        self.since = now;
    }
}

const fn index_of(phase: InstallPhase) -> usize {
    match phase {
        InstallPhase::Resolve => 0,
        InstallPhase::Fetch => 1,
        InstallPhase::Link => 2,
        InstallPhase::Audit => 3,
    }
}

#[cfg(test)]
mod tests;
