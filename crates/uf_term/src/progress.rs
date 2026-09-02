//! A spinner that is silent everywhere it would be noise.
//!
//! Three rules shape this type:
//!
//! * **Nothing is written unless the stream is an interactive terminal.** A CI
//!   log must never fill with carriage returns and erase sequences.
//! * **Frames are rate limited.** Redrawing on every event turns a fast build
//!   into a syscall benchmark, so a redraw only happens once per tick.
//! * **The cursor is never hidden.** Hiding it and restoring it on `Drop` is the
//!   usual trick, but `Drop` does not run when a process is killed, and this
//!   workspace builds release binaries with `panic = "abort"`. A terminal left
//!   without a cursor is a genuinely broken shell, so this spinner simply never
//!   hides it, and writes an explicit "show cursor" when it finishes in case
//!   something else did.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::capability::{Capabilities, ColorLevel, GlyphSet};
use crate::style::Style;

/// Erase from the cursor to the end of the line.
const ERASE_LINE: &str = "\x1b[K";
/// Make the cursor visible; never paired with a hide.
const SHOW_CURSOR: &str = "\x1b[?25h";

const UNICODE_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ASCII_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];

/// The default redraw interval.
pub const DEFAULT_TICK: Duration = Duration::from_millis(80);

/// A single-line progress reporter.
///
/// Generic over its sink so that tests can assert on the exact byte stream.
#[derive(Debug)]
pub struct Progress<W: Write> {
    sink: W,
    enabled: bool,
    glyphs: GlyphSet,
    style: Style,
    color: ColorLevel,
    interval: Duration,
    next_frame_at: Option<Instant>,
    frame: usize,
    dirty: bool,
    line: String,
}

impl<W: Write> Progress<W> {
    /// Create a reporter writing to `sink`.
    ///
    /// When `capabilities` says the stream is not interactive, every method
    /// becomes a no-op and not one byte is written.
    pub fn new(capabilities: Capabilities, sink: W) -> Self {
        Self {
            sink,
            enabled: capabilities.is_interactive(),
            glyphs: capabilities.glyphs(),
            style: Style::new().dim(),
            color: capabilities.color(),
            interval: DEFAULT_TICK,
            next_frame_at: None,
            frame: 0,
            dirty: false,
            line: String::new(),
        }
    }

    /// Override the redraw interval.
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Whether this reporter will write anything at all.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Redraw with `message`, if the tick interval has elapsed.
    pub fn tick(&mut self, message: &str) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        match self.next_frame_at {
            Some(deadline) if now < deadline => return,
            _ => {}
        }
        self.next_frame_at = Some(now + self.interval);
        self.draw(message);
    }

    /// Redraw with `message` regardless of the tick interval.
    pub fn draw(&mut self, message: &str) {
        if !self.enabled {
            return;
        }
        let frames: &[&str] = match self.glyphs {
            GlyphSet::Unicode => &UNICODE_FRAMES,
            GlyphSet::Ascii => &ASCII_FRAMES,
        };
        self.line.clear();
        self.line.push('\r');
        self.style.open(self.color, &mut self.line);
        self.line.push_str(frames[self.frame % frames.len()]);
        self.line.push(' ');
        self.line.push_str(message);
        self.style.close(self.color, &mut self.line);
        self.line.push_str(ERASE_LINE);
        self.frame = self.frame.wrapping_add(1);
        self.dirty = true;
        let _ = self.sink.write_all(self.line.as_bytes());
        let _ = self.sink.flush();
    }

    /// Erase the progress line and leave the cursor visible.
    ///
    /// Idempotent, and called automatically on drop.
    pub fn finish(&mut self) {
        if !self.enabled || !self.dirty {
            return;
        }
        self.dirty = false;
        let _ = self.sink.write_all(b"\r");
        let _ = self.sink.write_all(ERASE_LINE.as_bytes());
        let _ = self.sink.write_all(SHOW_CURSOR.as_bytes());
        let _ = self.sink.flush();
    }
}

impl<W: Write> Drop for Progress<W> {
    fn drop(&mut self) {
        self.finish();
    }
}

impl Progress<io::Stderr> {
    /// A reporter on stderr, so that progress never pollutes piped stdout.
    pub fn stderr(capabilities: Capabilities) -> Self {
        Self::new(capabilities, io::stderr())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Tty;

    fn caps(tty: Tty) -> Capabilities {
        Capabilities::new(ColorLevel::Ansi16, GlyphSet::Unicode, tty)
    }

    #[test]
    fn a_piped_stream_writes_nothing_at_all() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(caps(Tty::Piped), &mut sink);
            assert!(!progress.is_enabled());
            for _ in 0..1_000 {
                progress.draw("running");
                progress.tick("running");
            }
            progress.finish();
        }
        assert!(sink.is_empty(), "progress must be silent when not a TTY");
    }

    #[test]
    fn an_interactive_stream_draws_a_frame() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(caps(Tty::Interactive), &mut sink);
            progress.draw("compiling routes");
            progress.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        assert!(output.contains("compiling routes"));
        assert!(output.starts_with('\r'));
        assert!(output.ends_with(SHOW_CURSOR));
    }

    #[test]
    fn the_cursor_is_never_hidden() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(caps(Tty::Interactive), &mut sink);
            for _ in 0..8 {
                progress.draw("step");
            }
        }
        let output = String::from_utf8(sink).unwrap();
        assert!(
            !output.contains("\x1b[?25l"),
            "the spinner must never hide the cursor"
        );
        assert!(output.contains(SHOW_CURSOR));
    }

    #[test]
    fn ticks_are_rate_limited() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(caps(Tty::Interactive), &mut sink)
                .with_interval(Duration::from_secs(3_600));
            for _ in 0..100 {
                progress.tick("step");
            }
            progress.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        assert_eq!(output.matches("step").count(), 1);
    }

    #[test]
    fn frames_advance_between_draws() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(caps(Tty::Interactive), &mut sink);
            progress.draw("a");
            progress.draw("a");
        }
        let output = String::from_utf8(sink).unwrap();
        assert!(output.contains(UNICODE_FRAMES[0]));
        assert!(output.contains(UNICODE_FRAMES[1]));
    }

    #[test]
    fn ascii_capabilities_use_ascii_frames() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(
                Capabilities::new(ColorLevel::Never, GlyphSet::Ascii, Tty::Interactive),
                &mut sink,
            );
            progress.draw("step");
        }
        let output = String::from_utf8(sink).unwrap();
        assert!(output.contains("- step"));
        assert!(!output.contains(UNICODE_FRAMES[0]));
    }

    #[test]
    fn dropping_an_undrawn_reporter_writes_nothing() {
        let mut sink: Vec<u8> = Vec::new();
        drop(Progress::new(caps(Tty::Interactive), &mut sink));
        assert!(sink.is_empty());
    }

    #[test]
    fn finishing_twice_erases_once() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(caps(Tty::Interactive), &mut sink);
            progress.draw("step");
            progress.finish();
            progress.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        assert_eq!(output.matches(SHOW_CURSOR).count(), 1);
    }

    #[test]
    fn color_off_keeps_the_progress_line_escape_free_except_for_erasure() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut progress = Progress::new(
                Capabilities::new(ColorLevel::Never, GlyphSet::Ascii, Tty::Interactive),
                &mut sink,
            );
            progress.draw("step");
        }
        let output = String::from_utf8(sink).unwrap();
        let without_control = output.replace(ERASE_LINE, "").replace(SHOW_CURSOR, "");
        assert!(!without_control.contains('\x1b'));
    }
}
