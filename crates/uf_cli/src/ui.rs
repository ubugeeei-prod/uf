//! The output surface every `uf` command renders through.
//!
//! Two rules are enforced here rather than left to each command:
//!
//! * **`--json` is sacred.** In [`OutputMode::Json`] every human render is
//!   dropped on the floor and progress is disabled, so `uf inspect --json > f`
//!   contains nothing but JSON.
//! * **Streams have jobs.** Human output goes to stdout; errors and progress go
//!   to stderr, so redirecting stdout never loses an error and never captures a
//!   spinner.

use std::io::{self, Write};

use uf_term::{
    Capabilities, ColorChoice, Progress, Renderer, TerminalEnv, display_width, push_spaces,
};

/// Whether a command is rendering for a person or emitting machine JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputMode {
    /// Rendered, styled, and laid out for a reader.
    Human,
    /// Pure JSON on stdout: no banner, no styling, no progress.
    Json,
}

/// The rendering surface: one renderer per stream, and one reusable buffer.
pub(crate) struct Ui {
    stdout: Renderer,
    stderr: Renderer,
    mode: OutputMode,
    buffer: String,
}

impl Ui {
    /// Resolve capabilities once for both streams.
    pub(crate) fn new(choice: ColorChoice, mode: OutputMode) -> Self {
        let env = TerminalEnv::from_process();
        Self {
            stdout: Renderer::new(Capabilities::for_stdout(choice, &env)),
            stderr: Renderer::new(Capabilities::for_stderr(choice, &env)),
            mode,
            buffer: String::with_capacity(8 * 1024),
        }
    }

    /// Whether human output is suppressed in favour of JSON.
    pub(crate) fn is_json(&self) -> bool {
        matches!(self.mode, OutputMode::Json)
    }

    /// Render a block to stdout. A no-op in JSON mode.
    pub(crate) fn render(&mut self, body: impl FnOnce(&Renderer, &mut String)) {
        if self.is_json() {
            return;
        }
        self.buffer.clear();
        body(&self.stdout, &mut self.buffer);
        write_all(&mut io::stdout().lock(), &self.buffer);
    }

    /// Render a block to stderr. Always rendered, including in JSON mode,
    /// because stderr never carries machine output.
    pub(crate) fn render_err(&mut self, body: impl FnOnce(&Renderer, &mut String)) {
        self.buffer.clear();
        body(&self.stderr, &mut self.buffer);
        write_all(&mut io::stderr().lock(), &self.buffer);
    }

    /// Write text to stdout exactly as given, with no styling and no framing.
    ///
    /// The third kind of output, and genuinely distinct from the other two: a
    /// rendered block is for a person, JSON is for a program, and this is for
    /// another *language*. `uf completion bash` is piped into `eval` and
    /// `uf __complete` into a completion list, so a banner, a colour, or a
    /// trailing status line is a syntax error in somebody's shell — but so is
    /// the silence [`Ui::render`] would give it, since both commands are in
    /// JSON mode precisely because they own stdout.
    pub(crate) fn plain(&mut self, text: &str) {
        write_all(&mut io::stdout().lock(), text);
    }

    /// Emit machine-readable JSON on stdout with no styling of any kind.
    pub(crate) fn json(&mut self, value: &serde_json::Value) -> serde_json::Result<()> {
        let mut rendered = serde_json::to_string_pretty(value)?;
        rendered.push('\n');
        write_all(&mut io::stdout().lock(), &rendered);
        Ok(())
    }

    /// A progress reporter on stderr, silent unless stderr is a terminal and
    /// the command is rendering for a person.
    pub(crate) fn progress(&self) -> Progress<io::Stderr> {
        let capabilities = if self.is_json() {
            Capabilities::plain()
        } else {
            self.stderr.capabilities()
        };
        Progress::stderr(capabilities)
    }

    /// Render a failure as a distinct block on stderr.
    ///
    /// The headline is prefixed `error:` rather than marked with the error
    /// glyph, so a failing command's last stdout line and its stderr block do
    /// not read as the same sentence printed twice.
    ///
    /// The chain of causes is printed underneath, so a wrapped IO error still
    /// says which file it was.
    pub(crate) fn error(&mut self, error: &anyhow::Error) {
        let headline = error.to_string();
        let causes: Vec<String> = error
            .chain()
            .skip(1)
            .map(|cause| cause.to_string())
            .collect();
        self.render_err(|renderer, out| {
            renderer
                .theme()
                .error
                .bold()
                .paint(renderer.color(), "error", out);
            out.push_str(": ");
            out.push_str(&headline);
            out.push('\n');
            for cause in &causes {
                push_spaces(out, 2);
                renderer
                    .theme()
                    .muted
                    .paint(renderer.color(), "caused by: ", out);
                out.push_str(cause);
                out.push('\n');
            }
        });
    }
}

/// Write a rendered block, ignoring a closed pipe.
///
/// A CLI that panics because `head` closed its stdout is a broken CLI.
fn write_all(sink: &mut impl Write, text: &str) {
    let _ = sink.write_all(text.as_bytes());
    let _ = sink.flush();
}

/// The widest display width in `values`, for laying out a column.
pub(crate) fn widest<'a>(values: impl IntoIterator<Item = &'a str>) -> usize {
    values.into_iter().map(display_width).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_mode_suppresses_human_rendering() {
        let mut ui = Ui::new(ColorChoice::Never, OutputMode::Json);
        let mut called = false;
        ui.render(|_, _| called = true);

        assert!(!called, "human rendering must not run in JSON mode");
        assert!(ui.is_json());
    }

    #[test]
    fn human_mode_runs_the_render_body() {
        let mut ui = Ui::new(ColorChoice::Never, OutputMode::Human);
        let mut called = false;
        ui.render(|_, out| {
            called = true;
            out.push_str("");
        });

        assert!(called);
        assert!(!ui.is_json());
    }

    #[test]
    fn json_mode_disables_progress() {
        let ui = Ui::new(ColorChoice::Always, OutputMode::Json);
        assert!(!ui.progress().is_enabled());
    }

    #[test]
    fn the_widest_value_drives_column_layout() {
        assert_eq!(widest(["a", "bbb", "cc"]), 3);
        assert_eq!(widest(["日本", "abc"]), 4);
        assert_eq!(widest([]), 0);
    }
}
