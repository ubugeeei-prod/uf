//! Which inline-image protocol the attached terminal speaks, if any.
//!
//! There is no portable way to *ask*. Both protocols have a query form, but the
//! answer arrives on stdin — and uf's two callers cannot read it: `curl … | sh`
//! has the script on stdin, and a CLI that blocked on a reply would hang on
//! every terminal that does not implement the query. So this is detection from
//! the environment, which is what every other tool that draws images in a
//! terminal does, and it is deliberately conservative: an unrecognised terminal
//! gets the block mark, which always renders.

/// An inline-image protocol a terminal understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    /// The kitty graphics protocol, `ESC _G … ESC \`.
    ///
    /// kitty's own, also implemented by ghostty and by WezTerm.
    Kitty,
    /// iTerm2's inline images, `ESC ] 1337 ; File= … BEL`.
    ///
    /// iTerm2's own, also implemented by WezTerm, Konsole, VS Code's terminal,
    /// and Hyper.
    ITerm2,
}

/// The environment variables that identify a terminal emulator.
///
/// Captured into an owned value for the same reason [`crate::TerminalEnv`] is:
/// detection stays a pure function of its inputs and is unit-testable without
/// mutating the process environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageEnv {
    /// `TERM`, which identifies ghostty and kitty by their terminfo names.
    pub term: Option<String>,
    /// `TERM_PROGRAM`, which is how iTerm2, WezTerm and VS Code identify
    /// themselves.
    pub term_program: Option<String>,
    /// `KITTY_WINDOW_ID`, set by kitty in every window it owns.
    pub kitty_window_id: Option<String>,
    /// `KONSOLE_VERSION`, set by Konsole, which speaks the iTerm2 protocol.
    pub konsole_version: Option<String>,
    /// `TMUX`, set inside a tmux session.
    pub tmux: Option<String>,
    /// `STY`, set inside a GNU screen session.
    pub sty: Option<String>,
    /// `UF_INLINE_IMAGES`; `0`/`never` refuses, `kitty`/`iterm2` force one.
    pub uf_inline_images: Option<String>,
}

impl ImageEnv {
    /// Read the relevant variables from the process environment.
    pub fn from_process() -> Self {
        Self {
            term: var("TERM"),
            term_program: var("TERM_PROGRAM"),
            kitty_window_id: var("KITTY_WINDOW_ID"),
            konsole_version: var("KONSOLE_VERSION"),
            tmux: var("TMUX"),
            sty: var("STY"),
            uf_inline_images: var("UF_INLINE_IMAGES"),
        }
    }

    /// Set `TERM`.
    #[must_use]
    pub fn with_term(mut self, value: &str) -> Self {
        self.term = Some(value.to_owned());
        self
    }

    /// Set `TERM_PROGRAM`.
    #[must_use]
    pub fn with_term_program(mut self, value: &str) -> Self {
        self.term_program = Some(value.to_owned());
        self
    }

    /// Set `KITTY_WINDOW_ID`.
    #[must_use]
    pub fn with_kitty_window_id(mut self, value: &str) -> Self {
        self.kitty_window_id = Some(value.to_owned());
        self
    }

    /// Set `KONSOLE_VERSION`.
    #[must_use]
    pub fn with_konsole_version(mut self, value: &str) -> Self {
        self.konsole_version = Some(value.to_owned());
        self
    }

    /// Set `TMUX`.
    #[must_use]
    pub fn with_tmux(mut self, value: &str) -> Self {
        self.tmux = Some(value.to_owned());
        self
    }

    /// Set `STY`.
    #[must_use]
    pub fn with_sty(mut self, value: &str) -> Self {
        self.sty = Some(value.to_owned());
        self
    }

    /// Set `UF_INLINE_IMAGES`.
    #[must_use]
    pub fn with_uf_inline_images(mut self, value: &str) -> Self {
        self.uf_inline_images = Some(value.to_owned());
        self
    }

    /// Which protocol to use, or [`None`] to draw the block mark instead.
    ///
    /// `UF_INLINE_IMAGES` wins over everything, in both directions: a terminal
    /// this does not recognise can be told to try, and one it recognises
    /// wrongly can be told to stop. That is the whole escape hatch, and it is
    /// an environment variable rather than a config key because the answer
    /// belongs to the terminal a person is sitting at, not to the project they
    /// happen to be in.
    #[must_use]
    pub fn protocol(&self) -> Option<ImageProtocol> {
        match non_empty(self.uf_inline_images.as_deref()) {
            Some("0" | "never" | "no" | "off" | "false") => return None,
            Some("kitty") => return Some(ImageProtocol::Kitty),
            Some("iterm2" | "iterm") => return Some(ImageProtocol::ITerm2),
            // `1`/`yes` asks for the best available, which is what detection
            // already answers, so it falls through rather than forcing one.
            _ => {}
        }

        // A multiplexer rewrites the stream it forwards, and neither protocol
        // survives that intact without passthrough that is off by default. The
        // failure mode is not a missing image, it is escape-sequence garbage
        // across the pane, so this refuses rather than guesses.
        if non_empty(self.tmux.as_deref()).is_some() || non_empty(self.sty.as_deref()).is_some() {
            return None;
        }

        if non_empty(self.kitty_window_id.as_deref()).is_some() {
            return Some(ImageProtocol::Kitty);
        }

        if let Some(term) = non_empty(self.term.as_deref()) {
            // Both are terminfo names, matched on a substring because a
            // terminal is commonly reached through `xterm-kitty`, `xterm-ghostty`
            // or a `-direct` variant of either.
            if term.contains("kitty") || term.contains("ghostty") {
                return Some(ImageProtocol::Kitty);
            }
        }

        match non_empty(self.term_program.as_deref()) {
            // WezTerm implements both; kitty's is the one it implements more
            // completely, and the one that places an image in cells rather than
            // at the cursor.
            Some("WezTerm" | "ghostty") => Some(ImageProtocol::Kitty),
            Some("iTerm.app" | "vscode" | "Hyper" | "rio") => Some(ImageProtocol::ITerm2),
            _ if non_empty(self.konsole_version.as_deref()).is_some() => {
                Some(ImageProtocol::ITerm2)
            }
            _ => None,
        }
    }
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
