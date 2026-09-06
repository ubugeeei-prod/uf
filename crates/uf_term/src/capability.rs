//! What the attached terminal can actually render, and how big it is.
//!
//! Capability is resolved **once**, at start-up, from three inputs: the
//! `--color` flag, the environment, and whether the stream is a terminal. The
//! result is a plain `Copy` value that is threaded through every renderer, so
//! no write path ever re-probes the environment or asks the operating system
//! whether a file descriptor is a TTY.
//!
//! [`TerminalSize`] is resolved the same way and separately, because it costs
//! more: see [`TerminalSize::detect`].

use std::io::IsTerminal;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::image::{ImageEnv, ImageProtocol};

/// Colour behaviour requested on the command line, i.e. `--color`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorChoice {
    /// Decide from the environment and whether the stream is a terminal.
    #[default]
    Auto,
    /// Always emit colour, even when the stream is redirected.
    Always,
    /// Never emit colour.
    Never,
}

impl ColorChoice {
    /// Parse a `--color` argument value, accepting the usual spellings.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" | "force" | "yes" => Some(Self::Always),
            "never" | "none" | "no" | "off" => Some(Self::Never),
            _ => None,
        }
    }

    /// Canonical spelling, as accepted by [`ColorChoice::parse`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

/// How much colour a stream can carry.
///
/// This is deliberately an enum rather than a boolean: a 24-bit accent is
/// downgraded to the 256-colour cube and then to the 16 base colours, so one
/// theme renders correctly everywhere instead of being written twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorLevel {
    /// No escape sequences at all.
    Never,
    /// The 8 base colours plus their bright variants.
    Ansi16,
    /// The 256-colour indexed palette.
    Ansi256,
    /// 24-bit direct colour.
    TrueColor,
}

impl ColorLevel {
    /// Whether any escape sequence may be written.
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Never)
    }
}

/// Which glyph vocabulary is safe to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphSet {
    /// Box-drawing characters, arrows, and check marks.
    Unicode,
    /// Pure ASCII, for terminals or locales that cannot be trusted with more.
    Ascii,
}

/// Whether a stream is attached to an interactive terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tty {
    /// A terminal a human is looking at; progress may animate.
    Interactive,
    /// A pipe, a file, or a CI log; progress must stay silent.
    Piped,
}

impl Tty {
    /// Classify a stream from `std::io::IsTerminal`.
    pub fn of(stream: &impl IsTerminal) -> Self {
        if stream.is_terminal() {
            Self::Interactive
        } else {
            Self::Piped
        }
    }
}

/// The environment variables that influence terminal rendering.
///
/// Captured into an owned value once so that detection is a pure function of
/// its inputs and can be unit-tested without mutating the process environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalEnv {
    /// `NO_COLOR`; any non-empty value disables colour.
    pub no_color: Option<String>,
    /// `FORCE_COLOR`; `0`/`false` disables, `1`/`2`/`3` pick a level.
    pub force_color: Option<String>,
    /// `CLICOLOR`; `0` disables colour on a terminal.
    pub clicolor: Option<String>,
    /// `CLICOLOR_FORCE`; any value other than `0` forces colour on.
    pub clicolor_force: Option<String>,
    /// `TERM`; `dumb` disables colour and Unicode glyphs.
    pub term: Option<String>,
    /// `COLORTERM`; `truecolor`/`24bit` advertise direct colour.
    pub colorterm: Option<String>,
    /// `COLUMNS`; how wide the terminal is, when something has said so.
    pub columns: Option<String>,
    /// `LINES`; how tall the terminal is, when something has said so.
    pub lines: Option<String>,
    /// The effective locale, from `LC_ALL`, `LC_CTYPE`, or `LANG`.
    pub locale: Option<String>,
}

impl TerminalEnv {
    /// Read the relevant variables from the process environment.
    pub fn from_process() -> Self {
        Self {
            no_color: var("NO_COLOR"),
            force_color: var("FORCE_COLOR"),
            clicolor: var("CLICOLOR"),
            clicolor_force: var("CLICOLOR_FORCE"),
            term: var("TERM"),
            colorterm: var("COLORTERM"),
            columns: var("COLUMNS"),
            lines: var("LINES"),
            locale: var("LC_ALL")
                .or_else(|| var("LC_CTYPE"))
                .or_else(|| var("LANG")),
        }
    }

    /// Set `NO_COLOR`.
    pub fn with_no_color(mut self, value: &str) -> Self {
        self.no_color = Some(value.to_owned());
        self
    }

    /// Set `FORCE_COLOR`.
    pub fn with_force_color(mut self, value: &str) -> Self {
        self.force_color = Some(value.to_owned());
        self
    }

    /// Set `CLICOLOR`.
    pub fn with_clicolor(mut self, value: &str) -> Self {
        self.clicolor = Some(value.to_owned());
        self
    }

    /// Set `CLICOLOR_FORCE`.
    pub fn with_clicolor_force(mut self, value: &str) -> Self {
        self.clicolor_force = Some(value.to_owned());
        self
    }

    /// Set `TERM`.
    pub fn with_term(mut self, value: &str) -> Self {
        self.term = Some(value.to_owned());
        self
    }

    /// Set `COLORTERM`.
    pub fn with_colorterm(mut self, value: &str) -> Self {
        self.colorterm = Some(value.to_owned());
        self
    }

    /// Set `COLUMNS`.
    pub fn with_columns(mut self, value: &str) -> Self {
        self.columns = Some(value.to_owned());
        self
    }

    /// Set `LINES`.
    pub fn with_lines(mut self, value: &str) -> Self {
        self.lines = Some(value.to_owned());
        self
    }

    /// Set the effective locale.
    pub fn with_locale(mut self, value: &str) -> Self {
        self.locale = Some(value.to_owned());
        self
    }

    /// `COLUMNS`, when it names a usable number of columns.
    fn declared_columns(&self) -> Option<usize> {
        positive(self.columns.as_deref())
    }

    /// `LINES`, when it names a usable number of rows.
    fn declared_rows(&self) -> Option<usize> {
        positive(self.lines.as_deref())
    }

    fn no_color_requested(&self) -> bool {
        non_empty(self.no_color.as_deref()).is_some()
    }

    fn is_dumb(&self) -> bool {
        matches!(self.term.as_deref(), Some("dumb"))
    }

    fn utf8_locale(&self) -> bool {
        match non_empty(self.locale.as_deref()) {
            // An unset locale is the common case on macOS and inside CI images
            // that still render UTF-8 correctly, so it is not treated as a
            // downgrade signal.
            None => true,
            Some(locale) => {
                contains_ignore_ascii_case(locale, "utf-8")
                    || contains_ignore_ascii_case(locale, "utf8")
            }
        }
    }

    /// The level advertised by `COLORTERM` and `TERM`, ignoring every switch.
    fn declared_level(&self) -> ColorLevel {
        if let Some(colorterm) = non_empty(self.colorterm.as_deref())
            && (contains_ignore_ascii_case(colorterm, "truecolor")
                || contains_ignore_ascii_case(colorterm, "24bit"))
        {
            return ColorLevel::TrueColor;
        }
        match non_empty(self.term.as_deref()) {
            Some(term) if contains_ignore_ascii_case(term, "direct") => ColorLevel::TrueColor,
            Some(term) if contains_ignore_ascii_case(term, "256") => ColorLevel::Ansi256,
            _ => ColorLevel::Ansi16,
        }
    }

    /// `FORCE_COLOR`, which both disables (`0`) and picks a level (`1`..`3`).
    fn force_color_level(&self) -> Option<ColorLevel> {
        let value = self.force_color.as_deref()?;
        match value.trim() {
            // `FORCE_COLOR=` with an empty value means "on" by convention.
            "" | "1" | "true" => Some(self.declared_level().max(ColorLevel::Ansi16)),
            "0" | "false" => Some(ColorLevel::Never),
            "2" => Some(ColorLevel::Ansi256),
            "3" => Some(ColorLevel::TrueColor),
            _ => Some(self.declared_level().max(ColorLevel::Ansi16)),
        }
    }

    fn clicolor_forced(&self) -> bool {
        matches!(non_empty(self.clicolor_force.as_deref()), Some(value) if value != "0")
    }

    fn clicolor_disabled(&self) -> bool {
        matches!(non_empty(self.clicolor.as_deref()), Some("0"))
    }
}

/// The resolved rendering capability of one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    color: ColorLevel,
    glyphs: GlyphSet,
    tty: Tty,
    image: Option<ImageProtocol>,
}

impl Capabilities {
    /// Resolve capability from a flag, a stream classification, and the
    /// environment.
    ///
    /// Precedence, highest first:
    ///
    /// 1. `--color never` / `--color always`
    /// 2. `NO_COLOR` (any non-empty value)
    /// 3. `FORCE_COLOR`
    /// 4. `CLICOLOR_FORCE`
    /// 5. `TERM=dumb`
    /// 6. `CLICOLOR=0`
    /// 7. whether the stream is a terminal
    /// 8. `COLORTERM` / `TERM`
    ///
    /// The inline-image protocol is resolved here too, and gated on the same
    /// two answers a caller would otherwise have to re-ask: an image is a large
    /// escape sequence, so a stream that may not carry colour may not carry one
    /// either, and a stream nobody is looking at gets bytes in a log rather
    /// than a picture.
    pub fn detect(choice: ColorChoice, tty: Tty, env: &TerminalEnv) -> Self {
        let color = detect_color(choice, tty, env);
        Self {
            color,
            glyphs: detect_glyphs(env),
            tty,
            image: detect_image(color, tty, &ImageEnv::from_process()),
        }
    }

    /// Capability for the process stdout.
    pub fn for_stdout(choice: ColorChoice, env: &TerminalEnv) -> Self {
        Self::detect(choice, Tty::of(&std::io::stdout()), env)
    }

    /// Capability for the process stderr.
    pub fn for_stderr(choice: ColorChoice, env: &TerminalEnv) -> Self {
        Self::detect(choice, Tty::of(&std::io::stderr()), env)
    }

    /// The most conservative capability: no colour, ASCII glyphs, not a TTY.
    ///
    /// This is what `--json` and redirected output use.
    pub fn plain() -> Self {
        Self {
            color: ColorLevel::Never,
            glyphs: GlyphSet::Ascii,
            tty: Tty::Piped,
            image: None,
        }
    }

    /// Build a capability directly, for tests and for callers that already know
    /// what they want.
    pub fn new(color: ColorLevel, glyphs: GlyphSet, tty: Tty) -> Self {
        Self {
            color,
            glyphs,
            tty,
            image: None,
        }
    }

    /// The same, with an inline-image protocol.
    #[must_use]
    pub fn with_image(mut self, image: Option<ImageProtocol>) -> Self {
        self.image = image;
        self
    }

    /// How much colour this stream can carry.
    pub fn color(self) -> ColorLevel {
        self.color
    }

    /// Which glyph vocabulary is safe on this stream.
    pub fn glyphs(self) -> GlyphSet {
        self.glyphs
    }

    /// Whether the stream is attached to an interactive terminal.
    pub fn is_interactive(self) -> bool {
        matches!(self.tty, Tty::Interactive)
    }

    /// Whether Unicode box drawing is safe.
    pub fn is_unicode(self) -> bool {
        matches!(self.glyphs, GlyphSet::Unicode)
    }

    /// The inline-image protocol this stream accepts, if any.
    pub fn image(self) -> Option<ImageProtocol> {
        self.image
    }
}

/// How wide a terminal is assumed to be when nothing will say.
///
/// The number every terminal has been at least as wide as since VT100s, and
/// the same one `@uniflowed/tui` falls back to.
pub const FALLBACK_COLUMNS: usize = 80;

/// How tall a terminal is assumed to be when nothing will say.
pub const FALLBACK_ROWS: usize = 24;

/// How big the terminal is, in cells.
///
/// # Why this is not part of [`Capabilities`]
///
/// Colour, glyphs and interactivity are answered by reading environment
/// variables and one `isatty`, which is free, so every command resolves them
/// at start-up whether it draws a spinner or not. Size is not free — see
/// [`TerminalSize::detect`] — and only the handful of commands that redraw a
/// region in place need it. Folding it into `Capabilities` would put a process
/// spawn in front of `uf --version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    columns: usize,
    rows: usize,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self::fallback()
    }
}

impl TerminalSize {
    /// A size stated directly.
    ///
    /// A zero in either dimension is a terminal that has not finished being
    /// created — `stty` reports `0 0` inside some CI shells — and is replaced
    /// by the fallback rather than propagated as a region nothing fits in.
    pub fn new(columns: usize, rows: usize) -> Self {
        Self {
            columns: if columns == 0 {
                FALLBACK_COLUMNS
            } else {
                columns
            },
            rows: if rows == 0 { FALLBACK_ROWS } else { rows },
        }
    }

    /// The size assumed when nothing will say: 80 by 24.
    pub const fn fallback() -> Self {
        Self {
            columns: FALLBACK_COLUMNS,
            rows: FALLBACK_ROWS,
        }
    }

    /// How many columns wide the terminal is.
    pub fn columns(self) -> usize {
        self.columns
    }

    /// How many rows tall the terminal is.
    pub fn rows(self) -> usize {
        self.rows
    }

    /// Resolve the terminal's size, once.
    ///
    /// Precedence, highest first — the same list `@uniflowed/tui`'s
    /// `detectSize` walks, and `tests/library/tui.test.js` compares the two
    /// orders rather than believing this sentence:
    ///
    /// 1. `COLUMNS` and `LINES`, each on its own, when they parse as a
    ///    positive number. POSIX makes them the override, and they are what a
    ///    `watch`, a `script` or a CI wrapper sets when the terminal itself
    ///    cannot be asked.
    /// 2. what the terminal reports, for a stream somebody is watching
    /// 3. 80 by 24
    ///
    /// # What "asking the terminal" costs
    ///
    /// One `stty size` on `/dev/tty`, which is a process spawn — about a
    /// millisecond. It is the same trade `prompt::RawMode` made for the same
    /// reason: `uf_term` has no third-party dependencies, and reading a
    /// `winsize` out of an `ioctl` needs either `libc` or a hand-written
    /// struct whose layout differs between macOS and Linux. A spawn is asked
    /// for once per process, by the one command that redraws a region.
    ///
    /// It is also the reason this is not re-asked per frame, and therefore the
    /// reason a terminal resized *during* an install is not noticed: seeing
    /// that needs `SIGWINCH`, which needs a signal handler this workspace does
    /// not have. A region laid out for the terminal as it was is a large
    /// improvement on one laid out for a terminal nobody measured.
    pub fn detect(tty: Tty, env: &TerminalEnv) -> Self {
        let answered = env.declared_columns().is_some() && env.declared_rows().is_some();
        let reported = match tty {
            // Nobody is watching, so there is nothing to measure — and paying
            // a process spawn to discover that would be a spawn in the code
            // path of every piped command.
            Tty::Piped => None,
            // The environment already said both, so the spawn would be for an
            // answer that loses anyway.
            Tty::Interactive if answered => None,
            Tty::Interactive => probe_size(),
        };
        detect_size(env, reported)
    }
}

/// The size, from what the environment declared and then from what the
/// terminal reported.
///
/// The two dimensions are resolved separately, because `COLUMNS` without
/// `LINES` is the common shape: a wrapper that cares about width sets one of
/// them. `reported` is a parameter rather than a probe so that every rule
/// above it is a pure function of its inputs.
///
/// `@uniflowed/tui`'s `detectSize` is this function, chain for chain, and
/// `tests/library/tui.test.js` compares the two rather than believing this
/// sentence.
fn detect_size(env: &TerminalEnv, reported: Option<(usize, usize)>) -> TerminalSize {
    let columns = env
        .declared_columns()
        .or_else(|| reported_columns(reported))
        .unwrap_or(FALLBACK_COLUMNS);
    let rows = env
        .declared_rows()
        .or_else(|| reported_rows(reported))
        .unwrap_or(FALLBACK_ROWS);
    TerminalSize::new(columns, rows)
}

/// The columns the terminal reported, when it reported a usable number.
fn reported_columns(reported: Option<(usize, usize)>) -> Option<usize> {
    reported.map(|(columns, _)| columns).filter(|it| *it > 0)
}

/// The rows the terminal reported, when it reported a usable number.
fn reported_rows(reported: Option<(usize, usize)>) -> Option<usize> {
    reported.map(|(_, rows)| rows).filter(|it| *it > 0)
}

/// Ask the controlling terminal how big it is.
///
/// `/dev/tty` rather than this process's standard input, because the stream a
/// region is drawn on is not the stream a caller piped something into: `echo y
/// | uf install` still draws on the terminal the reader is looking at, and
/// `stty` reads the terminal attached to *its* standard input. On a platform
/// with no `/dev/tty`, or none that has `stty` at one of [`STTY_PROGRAMS`],
/// this answers `None` and the fallback applies.
fn probe_size() -> Option<(usize, usize)> {
    let program = stty_program()?;
    let terminal = std::fs::File::open("/dev/tty").ok()?;
    let output = Command::new(program)
        .arg("size")
        .stdin(Stdio::from(terminal))
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let reported = String::from_utf8(output.stdout).ok()?;
    parse_stty_size(&reported)
}

/// The absolute paths this crate will run `stty` from, in the order tried.
///
/// Naming a program by its bare name resolves it through the `PATH` this
/// process inherited, and [`probe_size`] runs during ordinary terminal
/// probing — so a `stty` planted anywhere earlier on a user's `PATH` than the
/// system one would be executed, as that user, by a command that only wanted
/// to know how wide the window is. That is CWE-426, and the answer is to not
/// ask `PATH` at all.
///
/// These two are POSIX's own default utility path, `/bin:/usr/bin` — what
/// `confstr(_CS_PATH)` answers on macOS and on glibc — and between them they
/// cover the platforms uf supports: macOS ships `/bin/stty`, the GNU
/// distributions ship `/usr/bin/stty` with `/bin` a symlink to it or a copy,
/// and BusyBox ships `/bin/stty`. What makes them trustworthy is not that the
/// file is usually there but that the *directory* is root-owned on all of
/// them, which is the entire vulnerability: an attacker who can write to
/// `/bin` does not need this bug.
///
/// A system that keeps its utilities somewhere else entirely — NixOS, whose
/// are under `/run/current-system/sw/bin` — matches neither, and gets `None`:
/// the size falls back to `COLUMNS`/`LINES` and then to 80×24. That is a
/// deliberate trade. A terminal measured wrongly costs a worse layout; a
/// terminal measured by whatever `PATH` happened to point at costs more than
/// a layout, and `COLUMNS`/`LINES` are already the documented way to say how
/// big the window is when it cannot be asked.
///
/// The alternative that needs no subprocess at all is `TIOCGWINSZ`, and it is
/// tracked rather than done here: it needs either a `libc` dependency this
/// crate does not have or a hand-written `winsize` whose layout differs
/// between macOS and Linux, which is the same reason [`TerminalSize::detect`]
/// gives for spawning in the first place.
const STTY_PROGRAMS: [&str; 2] = ["/bin/stty", "/usr/bin/stty"];

/// The `stty` this process will run, or nothing when no trusted one is there.
fn stty_program() -> Option<&'static Path> {
    STTY_PROGRAMS
        .iter()
        .map(Path::new)
        .find(|program| program.is_file())
}

/// `stty size` prints rows first, then columns, separated by a space.
fn parse_stty_size(reported: &str) -> Option<(usize, usize)> {
    let mut parts = reported.split_ascii_whitespace();
    let rows: usize = parts.next()?.parse().ok()?;
    let columns: usize = parts.next()?.parse().ok()?;
    Some((columns, rows))
}

/// Which inline-image protocol may be used on a stream.
///
/// Separate from [`ImageEnv::protocol`] because that answers what the terminal
/// *understands* and this answers what uf may *send*: the two differ whenever
/// colour is off or the stream is not a terminal.
fn detect_image(color: ColorLevel, tty: Tty, env: &ImageEnv) -> Option<ImageProtocol> {
    if !color.is_enabled() || !matches!(tty, Tty::Interactive) {
        return None;
    }
    env.protocol()
}

fn detect_color(choice: ColorChoice, tty: Tty, env: &TerminalEnv) -> ColorLevel {
    match choice {
        ColorChoice::Never => return ColorLevel::Never,
        ColorChoice::Always => return env.declared_level().max(ColorLevel::Ansi16),
        ColorChoice::Auto => {}
    }
    if env.no_color_requested() {
        return ColorLevel::Never;
    }
    if let Some(level) = env.force_color_level() {
        return level;
    }
    if env.clicolor_forced() {
        return env.declared_level().max(ColorLevel::Ansi16);
    }
    if env.is_dumb() || env.clicolor_disabled() || matches!(tty, Tty::Piped) {
        return ColorLevel::Never;
    }
    env.declared_level().max(ColorLevel::Ansi16)
}

fn detect_glyphs(env: &TerminalEnv) -> GlyphSet {
    if env.is_dumb() || env.no_color_requested() || !env.utf8_locale() {
        GlyphSet::Ascii
    } else {
        GlyphSet::Unicode
    }
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

/// A variable that names a positive number of cells, or nothing.
///
/// `COLUMNS=0` and `COLUMNS=wide` are both a variable saying nothing useful,
/// and both have to fall through to the next rule rather than produce a
/// terminal zero columns across.
fn positive(value: Option<&str>) -> Option<usize> {
    let parsed: usize = non_empty(value)?.trim().parse().ok()?;
    (parsed > 0).then_some(parsed)
}

/// Case-insensitive ASCII substring test that never allocates.
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests;
