//! The two things `uf` draws while a command is still running.
//!
//! [`Progress`] is one line with a spinner on it. [`Live`] is a region of
//! several lines redrawn in place, for a command whose interesting state does
//! not fit on one — `uf install` narrating a package manager's phases is the
//! reason it exists. They share this module rather than sitting in two,
//! because they share every rule below and a second spinner with its own
//! frames, its own tick and its own opinion about the cursor is exactly the
//! divergence this crate is here to prevent.
//!
//! Three rules shape both types:
//!
//! * **Nothing is written unless the stream is an interactive terminal.** A CI
//!   log must never fill with carriage returns and erase sequences.
//! * **Frames are rate limited.** Redrawing on every event turns a fast build
//!   into a syscall benchmark, so a redraw only happens once per tick.
//!   `uf install` calls `tick` once per package the manager reports; on a
//!   four-thousand-package tree that is four thousand calls and eighty frames.
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
use crate::text::{push_truncated, push_usize};

/// Erase from the cursor to the end of the line.
const ERASE_LINE: &str = "\x1b[K";
/// Make the cursor visible; never paired with a hide.
const SHOW_CURSOR: &str = "\x1b[?25h";

const UNICODE_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ASCII_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];

/// The default redraw interval.
pub const DEFAULT_TICK: Duration = Duration::from_millis(80);

/// The widest a [`Live`] region draws before its rows are cut.
///
/// The same 72 columns `Renderer::banner` clamps a rule to. A row wider than
/// the terminal wraps onto a second physical line, which makes every "cursor
/// up" that follows land one line short and turns the region into a smear —
/// so the bound is not cosmetic here the way it is for a banner. 72 is the
/// figure the rest of uf already assumes a terminal has.
pub const LIVE_WIDTH: usize = 72;

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

/// A region of several lines, redrawn in place.
///
/// [`Progress`] is one line and cannot say what a slow install is slow at.
/// This draws a small block — a phase per row, each with its own mark, count
/// and elapsed time — and rewrites it where it stands, so the reader watches
/// one picture change instead of a log scrolling past.
///
/// # The cursor arithmetic, and why the rows are cut
///
/// Redrawing means moving the cursor back up over what was drawn last time,
/// and that only works while every row is exactly one physical line. A row
/// wider than the terminal wraps, the terminal counts two lines where this
/// counts one, and every subsequent frame is drawn one line too low until the
/// screen is unreadable. So rows are cut to [`LIVE_WIDTH`] columns by
/// [`push_truncated`], which measures what a reader sees rather than what a
/// styled string contains.
///
/// # Interleaving somebody else's output
///
/// A command that narrates a child process still has to let that process
/// speak. [`Live::clear`] takes the region off the screen and leaves the
/// cursor where it started, so the caller can write a line that stays in the
/// scrollback and then draw the region again underneath it.
#[derive(Debug)]
pub struct Live<W: Write> {
    sink: W,
    enabled: bool,
    glyphs: GlyphSet,
    color: ColorLevel,
    width: usize,
    interval: Duration,
    next_frame_at: Option<Instant>,
    frame: usize,
    rows: usize,
    dirty: bool,
    out: String,
}

impl<W: Write> Live<W> {
    /// Create a region writing to `sink`.
    ///
    /// When `capabilities` says the stream is not interactive, every method
    /// becomes a no-op and not one byte is written.
    pub fn new(capabilities: Capabilities, sink: W) -> Self {
        Self {
            sink,
            enabled: capabilities.is_interactive(),
            glyphs: capabilities.glyphs(),
            color: capabilities.color(),
            width: LIVE_WIDTH,
            interval: DEFAULT_TICK,
            next_frame_at: None,
            frame: 0,
            rows: 0,
            dirty: false,
            out: String::new(),
        }
    }

    /// Override the redraw interval.
    #[must_use]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Override the column at which rows are cut.
    #[must_use]
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Whether this region will draw anything at all.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The column at which rows are cut.
    pub fn width(&self) -> usize {
        self.width
    }

    /// How much colour the rows may carry.
    pub fn color(&self) -> ColorLevel {
        self.color
    }

    /// The glyph vocabulary the rows must be drawn in.
    ///
    /// Handed out so that a caller composing a row draws its marks in the same
    /// vocabulary the spinner is already using: a row with a Unicode tick and
    /// an ASCII spinner on it is one line resolved twice.
    pub fn glyphs(&self) -> GlyphSet {
        self.glyphs
    }

    /// Whether a [`Live::tick`] now would actually draw.
    ///
    /// Building a frame costs more than drawing one — a caller formats counts,
    /// durations and a package name into every row — and a caller that ticks
    /// once per package on a four-thousand-package tree would build fifty
    /// frames for every one that reaches the terminal. Asking first is how
    /// that stays free.
    pub fn is_due(&self) -> bool {
        self.enabled
            && match self.next_frame_at {
                Some(deadline) => Instant::now() >= deadline,
                None => true,
            }
    }

    /// The spinner frame the next draw will use.
    ///
    /// Handed out rather than drawn here, because a region has no single place
    /// a spinner belongs: `uf install` puts it on whichever row is the phase
    /// currently running. It advances on a draw and not on a read, so a frame
    /// that the tick interval throws away does not skip one.
    pub fn spinner(&self) -> &'static str {
        let frames: &[&str] = match self.glyphs {
            GlyphSet::Unicode => &UNICODE_FRAMES,
            GlyphSet::Ascii => &ASCII_FRAMES,
        };
        frames[self.frame % frames.len()]
    }

    /// Redraw the region as `rows`, if the tick interval has elapsed.
    pub fn tick(&mut self, rows: &[&str]) {
        if !self.is_due() {
            return;
        }
        self.draw(rows);
    }

    /// Redraw the region as `rows` regardless of the tick interval.
    pub fn draw(&mut self, rows: &[&str]) {
        if !self.enabled {
            return;
        }
        self.next_frame_at = Some(Instant::now() + self.interval);
        self.out.clear();
        if self.rows > 0 {
            push_cursor_up(&mut self.out, self.rows);
        }
        self.out.push('\r');
        for row in rows {
            push_truncated(&mut self.out, row, self.width);
            self.out.push_str(ERASE_LINE);
            self.out.push('\n');
        }
        // A frame with fewer rows than the last one leaves the difference on
        // screen, and a stale row of a progress display is worse than no
        // display: it reads as current.
        let surplus = self.rows.saturating_sub(rows.len());
        for _ in 0..surplus {
            self.out.push_str(ERASE_LINE);
            self.out.push('\n');
        }
        if surplus > 0 {
            push_cursor_up(&mut self.out, surplus);
        }
        self.rows = rows.len();
        self.frame = self.frame.wrapping_add(1);
        self.dirty = true;
        let _ = self.sink.write_all(self.out.as_bytes());
        let _ = self.sink.flush();
    }

    /// Take the region off the screen, leaving the cursor where it began.
    ///
    /// What a caller does before writing a line that belongs in the
    /// scrollback. The region can be drawn again afterwards.
    pub fn clear(&mut self) {
        if !self.enabled || self.rows == 0 {
            return;
        }
        self.out.clear();
        push_cursor_up(&mut self.out, self.rows);
        self.out.push('\r');
        for _ in 0..self.rows {
            self.out.push_str(ERASE_LINE);
            self.out.push('\n');
        }
        push_cursor_up(&mut self.out, self.rows);
        self.rows = 0;
        let _ = self.sink.write_all(self.out.as_bytes());
        let _ = self.sink.flush();
    }

    /// Erase the region and leave the cursor visible.
    ///
    /// Idempotent, and called automatically on drop.
    pub fn finish(&mut self) {
        if !self.enabled || !self.dirty {
            return;
        }
        self.clear();
        self.dirty = false;
        let _ = self.sink.write_all(SHOW_CURSOR.as_bytes());
        let _ = self.sink.flush();
    }
}

impl<W: Write> Drop for Live<W> {
    fn drop(&mut self) {
        self.finish();
    }
}

impl Live<io::Stderr> {
    /// A region on stderr, so that progress never pollutes piped stdout.
    pub fn stderr(capabilities: Capabilities) -> Self {
        Self::new(capabilities, io::stderr())
    }
}

/// Append "move the cursor up `lines` lines".
///
/// `ESC[0A` moves up one line in most terminals rather than none, so a zero is
/// written as nothing at all.
fn push_cursor_up(out: &mut String, lines: usize) {
    if lines == 0 {
        return;
    }
    out.push_str("\x1b[");
    push_usize(out, lines);
    out.push('A');
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
    #[test]
    fn a_live_region_on_a_piped_stream_writes_nothing_at_all() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut live = Live::new(caps(Tty::Piped), &mut sink);
            assert!(!live.is_enabled());
            for _ in 0..100 {
                live.draw(&["one", "two"]);
                live.tick(&["one", "two"]);
                live.clear();
            }
            live.finish();
        }
        assert!(
            sink.is_empty(),
            "a live region must be silent when not a TTY"
        );
    }

    #[test]
    fn a_live_region_erases_exactly_the_rows_it_drew() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut live = Live::new(caps(Tty::Interactive), &mut sink);
            live.draw(&["one", "two", "three"]);
            live.draw(&["one", "two", "three"]);
            live.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        // The first frame moves up nothing, the second moves up over its
        // three rows, and the erase moves up twice: once to reach the top of
        // the region and once to come back after wiping it.
        assert_eq!(output.matches("\x1b[3A").count(), 3);
        assert!(!output.contains("\x1b[4A"));
    }

    #[test]
    fn a_shorter_frame_wipes_the_rows_it_no_longer_uses() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut live = Live::new(caps(Tty::Interactive), &mut sink);
            live.draw(&["one", "two", "three"]);
            live.draw(&["only"]);
            live.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        let second = &output[output.find("only").unwrap()..];
        // Two rows are surplus: they are erased and then stepped back over, so
        // the region is one row tall from here on.
        assert!(second.contains("\x1b[2A"), "{second:?}");
        assert!(second.contains("\x1b[1A"), "{second:?}");
    }

    #[test]
    fn clearing_leaves_the_region_redrawable() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut live = Live::new(caps(Tty::Interactive), &mut sink);
            live.draw(&["one", "two"]);
            live.clear();
            live.draw(&["one", "two"]);
            live.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        // Cleared, so the redraw starts from nothing: exactly two moves up
        // over two rows, one for the clear and one for the finish.
        assert_eq!(output.matches("\x1b[2A").count(), 4);
        assert!(output.ends_with(SHOW_CURSOR));
    }

    #[test]
    fn a_live_region_never_hides_the_cursor() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut live = Live::new(caps(Tty::Interactive), &mut sink);
            for _ in 0..8 {
                live.draw(&["row"]);
            }
        }
        let output = String::from_utf8(sink).unwrap();
        assert!(!output.contains("\x1b[?25l"));
        assert!(output.contains(SHOW_CURSOR));
    }

    #[test]
    fn live_rows_are_cut_to_the_width() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut live = Live::new(caps(Tty::Interactive), &mut sink).with_width(4);
            live.draw(&["abcdefghij"]);
            live.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        assert!(output.contains("abcd"));
        assert!(!output.contains("abcde"));
    }

    #[test]
    fn live_ticks_are_rate_limited_and_can_be_asked_first() {
        let mut sink: Vec<u8> = Vec::new();
        {
            let mut live = Live::new(caps(Tty::Interactive), &mut sink)
                .with_interval(Duration::from_secs(3_600));
            assert!(live.is_due(), "nothing drawn yet, so a tick would draw");
            for _ in 0..100 {
                live.tick(&["step"]);
            }
            assert!(!live.is_due(), "the interval has not elapsed");
            live.finish();
        }
        let output = String::from_utf8(sink).unwrap();
        assert_eq!(output.matches("step").count(), 1);
    }

    #[test]
    fn the_spinner_advances_once_per_frame_and_not_once_per_read() {
        let mut sink: Vec<u8> = Vec::new();
        let mut live =
            Live::new(caps(Tty::Interactive), &mut sink).with_interval(Duration::from_secs(3_600));
        assert_eq!(live.spinner(), UNICODE_FRAMES[0]);
        assert_eq!(live.spinner(), UNICODE_FRAMES[0]);
        live.draw(&["a"]);
        assert_eq!(live.spinner(), UNICODE_FRAMES[1]);
        // Thrown away by the interval, so the frame it would have used is not
        // spent: an animation that skips frames looks broken rather than slow.
        live.tick(&["a"]);
        assert_eq!(live.spinner(), UNICODE_FRAMES[1]);
    }

    #[test]
    fn an_ascii_live_region_spins_in_ascii() {
        let sink: Vec<u8> = Vec::new();
        let live = Live::new(
            Capabilities::new(ColorLevel::Never, GlyphSet::Ascii, Tty::Interactive),
            sink,
        );
        assert_eq!(live.spinner(), ASCII_FRAMES[0]);
    }

    #[test]
    fn dropping_an_undrawn_live_region_writes_nothing() {
        let mut sink: Vec<u8> = Vec::new();
        drop(Live::new(caps(Tty::Interactive), &mut sink));
        assert!(sink.is_empty());
    }
}
