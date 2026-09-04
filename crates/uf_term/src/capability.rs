//! What the attached terminal can actually render.
//!
//! Capability is resolved **once**, at start-up, from three inputs: the
//! `--color` flag, the environment, and whether the stream is a terminal. The
//! result is a plain `Copy` value that is threaded through every renderer, so
//! no write path ever re-probes the environment or asks the operating system
//! whether a file descriptor is a TTY.

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

    /// Set the effective locale.
    pub fn with_locale(mut self, value: &str) -> Self {
        self.locale = Some(value.to_owned());
        self
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
