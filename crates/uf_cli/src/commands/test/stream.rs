//! Reporting a run while it is still running.
//!
//! `uf test` used to draw nothing but a spinner until the last file finished,
//! and then everything at once. The results were never the problem: the
//! runner hands each [`FileReport`] to an observer the moment that file's
//! worker answers. Nothing drew them. This module does.
//!
//! # What goes where
//!
//! * **stdout** gets one block per file, in the order the files finish, drawn
//!   by [`render_file`] — the same function a merge of shards draws its files
//!   with, so a streamed run and a merged one read alike. A failure's frame is
//!   in that block, so it is on screen while the rest of the suite runs.
//! * **stderr** gets a single progress line under the blocks — files done out
//!   of the total, how many failed, how many are running, and how long the run
//!   has taken — redrawn in place and taken off the screen before each block
//!   and before the summary, so the scrollback holds results and never a
//!   stale progress line.
//!
//! The progress line is drawn only where redrawing is safe to do: stderr is a
//! terminal, the command is not rendering JSON, and neither `CI` nor
//! `NO_COLOR` is set. Everywhere else — a pipe, a CI log — the blocks still
//! stream, one plain block per file, and nothing is ever redrawn. Under
//! `--json` nothing streams at all: [`Ui::render`] drops it, and stdout stays
//! one document.
//!
//! # Threads
//!
//! Files finish on the runner's worker threads, and the elapsed time has to
//! keep moving while a slow file holds every worker, so a third party — a
//! ticker thread — redraws the line between results. All three reach the
//! terminal through one lock around the [`Ui`] and the progress line, which
//! is what keeps a block and a redraw from interleaving mid-line.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use uf_term::{DEFAULT_TICK, GlyphSet, Progress};
use uf_test::{FileReport, RunObserver};

use super::render::{path_column, render_banner, render_file};
use crate::ui::Ui;

/// The observer that draws each file as it finishes.
///
/// Generic over where the progress line goes only so a test can read the
/// bytes; `uf test` draws it on stderr.
pub(super) struct Stream<'a, W: Write + Send = io::Stderr> {
    state: Mutex<State<'a, W>>,
}

struct State<'a, W: Write> {
    ui: &'a mut Ui,
    /// `None` when the line may not be drawn; see the module documentation.
    progress: Option<Progress<W>>,
    /// Each file's source by its path, for the code frame under a failure.
    sources: BTreeMap<&'a str, &'a str>,
    path_width: usize,
    /// Between the counts on the progress line.
    separator: &'static str,
    started: Instant,
    total: usize,
    finished: usize,
    running: usize,
    failed_files: usize,
    line: String,
}

impl<'a> Stream<'a> {
    /// A stream over `files` — each one's path and source, the files the run
    /// was handed — drawn on `ui`.
    ///
    /// Draws the banner straight away, so a run that takes a while to start
    /// its first worker has already said what it is.
    pub(super) fn start(
        ui: &'a mut Ui,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
        label: &str,
    ) -> Self {
        let progress = live_line_allowed(ui).then(|| ui.progress());
        let glyphs = ui.stderr_capabilities().glyphs();
        Stream::with_progress(ui, files, label, progress, glyphs)
    }
}

impl<'a, W: Write + Send> Stream<'a, W> {
    /// [`Stream::start`], with the progress line — or `None` for no line —
    /// and the glyphs it is drawn in handed over rather than found.
    pub(super) fn with_progress(
        ui: &'a mut Ui,
        files: impl IntoIterator<Item = (&'a str, &'a str)>,
        label: &str,
        progress: Option<Progress<W>>,
        glyphs: GlyphSet,
    ) -> Self {
        ui.render(|renderer, out| render_banner(renderer, out, label));
        let separator = match glyphs {
            GlyphSet::Unicode => " · ",
            GlyphSet::Ascii => ", ",
        };
        let sources: BTreeMap<&str, &str> = files.into_iter().collect();
        let path_width = path_column(sources.keys().copied());
        Self {
            state: Mutex::new(State {
                ui,
                progress,
                total: sources.len(),
                sources,
                path_width,
                separator,
                started: Instant::now(),
                finished: 0,
                running: 0,
                failed_files: 0,
                line: String::new(),
            }),
        }
    }

    /// Whether a progress line is being drawn, which is whether a ticker is
    /// worth starting.
    pub(super) fn is_live(&self) -> bool {
        self.with(|state| state.progress.is_some())
    }

    /// Run `body` — the run itself — keeping the progress line's clock moving
    /// until it returns, and take the line off the screen afterwards.
    pub(super) fn drive<T>(&self, body: impl FnOnce() -> T) -> T {
        if !self.is_live() {
            return body();
        }
        let (stop, stopped) = mpsc::channel::<()>();
        let result = std::thread::scope(|scope| {
            scope.spawn(move || {
                // A result redraws the line itself; this only has to keep the
                // clock honest while every worker is busy with a slow file.
                while let Err(mpsc::RecvTimeoutError::Timeout) =
                    stopped.recv_timeout(TICKER_INTERVAL)
                {
                    self.with(State::redraw);
                }
            });
            let result = body();
            drop(stop);
            result
        });
        self.finish();
        result
    }

    /// Take the progress line off the screen for good.
    pub(super) fn finish(&self) {
        self.with(|state| {
            if let Some(progress) = state.progress.as_mut() {
                progress.finish();
            }
            state.progress = None;
        });
    }

    fn with<T>(&self, body: impl FnOnce(&mut State<'a, W>) -> T) -> T {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        body(&mut state)
    }
}

/// How often the ticker redraws while no file finishes.
///
/// The progress line's own rate limit, so a redraw the ticker asks for is one
/// the line would draw.
const TICKER_INTERVAL: Duration = DEFAULT_TICK;

impl<W: Write + Send> RunObserver for Stream<'_, W> {
    fn run_started(&self, files: usize, _workers: usize) {
        self.with(|state| {
            state.total = files;
            state.redraw();
        });
    }

    fn file_started(&self, _file: &str) {
        self.with(|state| {
            state.running += 1;
            state.redraw();
        });
    }

    fn file_finished(&self, completed: usize, total: usize, report: &FileReport) {
        self.with(|state| {
            state.finished = completed;
            state.total = total;
            state.running = state.running.saturating_sub(1);
            if super::render::file_status(report) == uf_term::Status::Error {
                state.failed_files += 1;
            }
            // Off the screen before the block, so the block lands where the
            // line was rather than after it; back on straight after.
            if let Some(progress) = state.progress.as_mut() {
                progress.finish();
            }
            let source = state.sources.get(report.file.as_str()).copied();
            let path_width = state.path_width;
            state
                .ui
                .render(|renderer, out| render_file(renderer, out, source, report, path_width));
            state.draw();
        });
    }
}

impl<W: Write> State<'_, W> {
    /// Redraw the line, if the rate limit allows.
    fn redraw(&mut self) {
        if self.progress.is_none() {
            return;
        }
        self.compose();
        if let Some(progress) = self.progress.as_mut() {
            progress.tick(&self.line);
        }
    }

    /// Redraw the line now, whatever the rate limit says: it has just been
    /// taken off the screen, and a gap until the next tick reads as a stall.
    fn draw(&mut self) {
        if self.progress.is_none() {
            return;
        }
        self.compose();
        if let Some(progress) = self.progress.as_mut() {
            progress.draw(&self.line);
        }
    }

    /// `12/40 files · 1 failed · 4 running · 3.2s`
    fn compose(&mut self) {
        use std::fmt::Write as _;
        self.line.clear();
        let _ = write!(self.line, "{}/{} files", self.finished, self.total);
        let separator = self.separator;
        if self.failed_files > 0 {
            let _ = write!(self.line, "{separator}{} failed", self.failed_files);
        }
        if self.running > 0 {
            let _ = write!(self.line, "{separator}{} running", self.running);
        }
        self.line.push_str(separator);
        // Seconds to one decimal, always: a clock whose unit changes from
        // `ms` to `s` mid-run, or that shows microseconds changing on every
        // frame, is noise rather than information.
        let _ = write!(self.line, "{:.1}s", self.started.elapsed().as_secs_f64());
    }
}

/// Whether the progress line may be drawn at all.
///
/// [`Ui::progress`] already refuses a stream that is not a terminal and a
/// command rendering JSON. Two more signals say "do not redraw" even on a
/// terminal: `CI`, which hosted runners set and some of them set on a
/// pseudo-terminal whose log keeps every frame, and `NO_COLOR`, which asks for
/// plain output and gets the plain form of this — the blocks, streamed, and no
/// escape sequences between them.
fn live_line_allowed(ui: &Ui) -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    ui.progress().is_enabled() && !set("CI") && !set("NO_COLOR")
}

#[cfg(test)]
mod tests {
    use uf_term::{Capabilities, ColorLevel, Tty};
    use uf_test::{AssertionFailure, FileStatus, TestRecord, TestStatus};

    use super::*;
    use crate::ui::OutputMode;

    const SOURCE: &str = "it(\"adds\", () => {\n  expect(1).toBe(2);\n});\n";

    fn record(file: &str, name: &str, status: TestStatus) -> TestRecord {
        TestRecord {
            file: file.to_owned(),
            name: name.to_owned(),
            line: 1,
            column: 1,
            status,
            attempts: 1,
            duration_micros: 10,
            output: Vec::new(),
            bench: None,
        }
    }

    fn passing(file: &str) -> FileReport {
        FileReport {
            file: file.to_owned(),
            status: FileStatus::Completed,
            duration_micros: 3_000,
            records: vec![record(file, "passes", TestStatus::Passed)],
            output: Vec::new(),
        }
    }

    fn failing(file: &str) -> FileReport {
        let failure = AssertionFailure {
            message: String::from("expected 1 to be 2"),
            line: 2,
            column: 13,
            span: 4,
            expected: Some(String::from("2")),
            received: Some(String::from("1")),
            stack: None,
        };
        FileReport {
            file: file.to_owned(),
            status: FileStatus::Completed,
            duration_micros: 12_000,
            records: vec![
                record(file, "passes", TestStatus::Passed),
                record(
                    file,
                    "adds",
                    TestStatus::Failed {
                        failures: vec![failure],
                    },
                ),
            ],
            output: Vec::new(),
        }
    }

    fn interactive() -> Capabilities {
        Capabilities::new(ColorLevel::Never, GlyphSet::Unicode, Tty::Interactive)
    }

    /// Each file is on stdout as soon as it finishes — before the run has
    /// ended, which here is before the second file has even started — and a
    /// failure's frame comes with it.
    #[test]
    fn a_file_is_drawn_the_moment_it_finishes() {
        let mut ui = Ui::capturing(OutputMode::Human);
        let files = [("src/a.test.js", SOURCE), ("src/b.test.js", SOURCE)];
        {
            let stream =
                Stream::<io::Sink>::with_progress(&mut ui, files, "demo", None, GlyphSet::Unicode);
            stream.run_started(2, 2);
            stream.file_started("src/a.test.js");
            stream.file_finished(1, 2, &failing("src/a.test.js"));
        }
        let drawn = ui.take_captured();
        // A captured stdout is plain, so the marks are the ASCII ones.
        assert!(drawn.starts_with("uf test  demo\n"), "{drawn}");
        assert!(drawn.contains("x src/a.test.js"), "{drawn}");
        assert!(drawn.contains("1 passed, 1 failed"), "{drawn}");
        assert!(drawn.contains("x adds"), "{drawn}");
        assert!(drawn.contains("expected 1 to be 2"), "{drawn}");
        assert!(drawn.contains("expect(1).toBe(2);"), "the frame: {drawn}");
        assert!(
            !drawn.contains("passes"),
            "a passing test is counted, not listed: {drawn}"
        );

        {
            let mut ui = Ui::capturing(OutputMode::Human);
            let stream =
                Stream::<io::Sink>::with_progress(&mut ui, files, "demo", None, GlyphSet::Unicode);
            stream.file_finished(1, 2, &passing("src/b.test.js"));
            drop(stream);
            let drawn = ui.take_captured();
            // Padded to the widest path the run will report, so the
            // durations of every line share one column.
            assert!(
                drawn.contains("+ src/b.test.js      3ms  1 passed\n"),
                "{drawn:?}"
            );
        }
    }

    /// The progress line is taken off the screen before each block and put
    /// back straight after, says how far the run has got, and is gone for
    /// good when the run is.
    #[test]
    fn the_progress_line_is_lifted_for_each_file_and_removed_at_the_end() {
        let mut ui = Ui::capturing(OutputMode::Human);
        let mut line: Vec<u8> = Vec::new();
        {
            let progress = Progress::new(interactive(), &mut line);
            let stream = Stream::with_progress(
                &mut ui,
                [("src/a.test.js", SOURCE), ("src/b.test.js", SOURCE)],
                "demo",
                Some(progress),
                GlyphSet::Unicode,
            );
            assert!(stream.is_live());
            let report = stream.drive(|| {
                stream.run_started(2, 2);
                stream.file_started("src/a.test.js");
                stream.file_started("src/b.test.js");
                stream.file_finished(1, 2, &failing("src/a.test.js"));
                stream.file_finished(2, 2, &passing("src/b.test.js"));
                "done"
            });
            assert_eq!(report, "done");
            assert!(!stream.is_live(), "the line is gone once the run is");
        }
        let line = String::from_utf8(line).expect("utf-8");
        assert!(
            line.contains("1/2 files · 1 failed · 1 running"),
            "{line:?}"
        );
        assert!(line.contains("2/2 files · 1 failed"), "{line:?}");
        // Erased once per finished file and once at the end, and the cursor is
        // never hidden.
        assert!(line.matches("\r\x1b[K").count() >= 3, "{line:?}");
        assert!(line.ends_with("\r\x1b[K\x1b[?25h"), "{line:?}");
        assert!(!line.contains("\x1b[?25l"), "{line:?}");

        let drawn = ui.take_captured();
        assert!(!drawn.contains('\r'), "stdout is never redrawn: {drawn:?}");
        assert!(
            drawn.find("src/a.test.js") < drawn.find("src/b.test.js"),
            "in the order they finished: {drawn}"
        );
    }

    /// Without a progress line nothing is drawn on stderr at all, and the
    /// blocks are the same plain text a pipe gets.
    #[test]
    fn a_stream_without_a_line_draws_only_blocks() {
        let mut ui = Ui::capturing(OutputMode::Human);
        {
            let stream = Stream::<io::Sink>::with_progress(
                &mut ui,
                [("src/a.test.js", SOURCE)],
                "demo",
                None,
                GlyphSet::Ascii,
            );
            assert!(!stream.is_live());
            stream.drive(|| stream.file_finished(1, 1, &passing("src/a.test.js")));
        }
        let drawn = ui.take_captured();
        assert!(drawn.contains("+ src/a.test.js"), "{drawn}");
        assert!(!drawn.contains('\x1b'), "{drawn:?}");
    }

    /// `--json` owns stdout: the stream draws nothing there, not even the
    /// banner.
    #[test]
    fn a_json_run_streams_nothing() {
        let mut ui = Ui::capturing(OutputMode::Json);
        {
            let stream = Stream::start(&mut ui, [("src/a.test.js", SOURCE)], "demo");
            assert!(!stream.is_live());
            stream.file_finished(1, 1, &failing("src/a.test.js"));
        }
        assert_eq!(ui.take_captured(), "");
    }
}
