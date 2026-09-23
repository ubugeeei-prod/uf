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
/// at start-up whether it draws a spinner or not. Size costs an `open` and a
/// system call — see [`TerminalSize::detect`] — and only the commands that
/// lay text out to the window need it: a region redrawn in place, and the
/// help. Folding it into `Capabilities` would put both in front of
/// `uf --version`.
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
    /// created — some CI shells report `0 0` — and is replaced
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
    /// `detectSize` walks, and `packages/tui/tui.test.js` compares the two
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
    /// One `ioctl(TIOCGWINSZ)` on `/dev/tty`: an `open` and a system call,
    /// a few microseconds. It used to be a `stty size` spawn, about a
    /// millisecond on an idle machine and several on a busy one, which was a
    /// fair price for the one command that redraws a region and not for every
    /// help page, which is laid out to the terminal too. See [`probe_size`].
    ///
    /// It is also the reason this is not re-asked per frame, and therefore the
    /// reason a terminal resized *during* an install is not noticed: seeing
    /// that needs `SIGWINCH`, which needs a signal handler this workspace does
    /// not have. A region laid out for the terminal as it was is a large
    /// improvement on one laid out for a terminal nobody measured.
    pub fn detect(tty: Tty, env: &TerminalEnv) -> Self {
        let answered = env.declared_columns().is_some() && env.declared_rows().is_some();
        let reported = match tty {
            // Nobody is watching, so there is nothing to measure.
            Tty::Piped => None,
            // The environment already said both, so the terminal's answer
            // would lose anyway.
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
/// `packages/tui/tui.test.js` compares the two rather than believing this
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
/// `/dev/tty` rather than one of this process's streams, because the stream a
/// region is drawn on is not the stream a caller piped something into: `echo y
/// | uf install` still draws on the terminal the reader is looking at. With no
/// `/dev/tty` — no controlling terminal, or a platform without one — this
/// answers `None` and the fallback applies.
///
/// `TIOCGWINSZ` is read with a hand-written declaration rather than through
/// `libc`, because `uf_term` has no third-party dependencies. What that takes
/// is small and fixed: `struct winsize` is four `unsigned short`s, rows first,
/// on macOS and on Linux alike, and only the request number differs between
/// them. A target this does not name answers `None`, the same as a terminal
/// that could not be asked.
#[cfg(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
))]
fn probe_size() -> Option<(usize, usize)> {
    use std::os::fd::AsRawFd;
    use std::os::raw::{c_int, c_ulong};

    /// `struct winsize`, from `<sys/ttycom.h>` on macOS and
    /// `<asm-generic/termios.h>` on Linux.
    #[repr(C)]
    #[derive(Default)]
    struct WinSize {
        rows: u16,
        columns: u16,
        x_pixels: u16,
        y_pixels: u16,
    }

    /// `_IOR('t', 104, struct winsize)` on macOS.
    #[cfg(target_os = "macos")]
    const TIOCGWINSZ: c_ulong = 0x4008_7468;
    /// The generic `TIOCGWINSZ`, which x86-64 and AArch64 Linux both use.
    #[cfg(target_os = "linux")]
    const TIOCGWINSZ: c_ulong = 0x5413;

    unsafe extern "C" {
        fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    }

    let terminal = std::fs::File::open("/dev/tty").ok()?;
    let mut size = WinSize::default();
    // SAFETY: the descriptor is open for as long as `terminal` lives, and
    // `TIOCGWINSZ` writes exactly one `struct winsize` through the pointer,
    // which points at one.
    let status = unsafe { ioctl(terminal.as_raw_fd(), TIOCGWINSZ, &raw mut size) };
    let WinSize {
        rows,
        columns,
        x_pixels: _,
        y_pixels: _,
    } = size;
    (status == 0).then_some((usize::from(columns), usize::from(rows)))
}

/// Ask the controlling terminal how big it is: on this target, nothing can be
/// asked, and the fallback applies.
#[cfg(not(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )
)))]
fn probe_size() -> Option<(usize, usize)> {
    None
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

/// Which glyph set a terminal can draw — which is not what it may colour.
///
/// **`NO_COLOR` is not consulted here**, and that is the decision
/// ubugeeei-prod/uf#393 asked for rather than an omission. The convention at
/// no-color.org is about ANSI colour and says nothing about characters, and uf
/// already has the right signal for "cannot render Unicode": the locale. A
/// UTF-8 terminal whose owner asked for no colour can still draw `├─`, and
/// giving it `+- ` instead is worse output for no reason.
///
/// `packages/tui/capability.js` decides it the same way, and
/// `tests/library/tui.test.js` compares the two rules so they cannot drift
/// apart again — which is what let them disagree in the first place.
fn detect_glyphs(env: &TerminalEnv) -> GlyphSet {
    if env.is_dumb() || !env.utf8_locale() {
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
