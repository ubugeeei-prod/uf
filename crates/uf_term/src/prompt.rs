//! Asking the reader to pick one thing.
//!
//! The rest of this crate writes and never reads. This module is the one place
//! that takes keystrokes, and it exists because a list of twenty commands is
//! something to choose from rather than something to read: printing the list
//! and leaving the reader to retype one of its entries is asking them to do
//! the part a terminal is good at.
//!
//! # What it will not do
//!
//! It refuses unless a person is watching. A prompt in a pipeline is a hang
//! nobody can see, so [`select`] returns [`Outcome::NotInteractive`] when
//! either stream is redirected, when `TERM` is `dumb`, and when `CI` is set —
//! and the caller prints whatever it would have printed before. Nothing that
//! reads uf's output has to know a menu exists.
//!
//! # The frame
//!
//! Drawing is a redraw of the same block in place: the cursor goes back up as
//! many lines as were written, and each line clears to the right edge as it is
//! rewritten. Nothing scrolls, nothing flickers, and the scrollback afterwards
//! holds one frame rather than one per keystroke.
//!
//! ```no_run
//! use uf_term::prompt::{Choice, Outcome, Request, select};
//!
//! let choices = [
//!     Choice::new("build", "Build the project for production"),
//!     Choice::new("test", "Run the test suite"),
//! ];
//! match select(&Request::new("What would you like to run?", &choices)) {
//!     Outcome::Chose(choice) => println!("running {}", choice.name),
//!     Outcome::Cancelled => println!("nothing chosen"),
//!     Outcome::NotInteractive => println!("no terminal; print the help instead"),
//! }
//! ```

mod draw;
mod key;
mod menu;
mod raw;

#[cfg(test)]
mod tests;

use std::io::{self, IsTerminal, Write};

use crate::capability::{Capabilities, ColorChoice, TerminalEnv};
use crate::theme::Theme;

pub use menu::{Choice, VISIBLE};

use key::{Key, read_key};
use menu::Menu;
use raw::RawMode;

/// What to ask, and what the answers are.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The question, shown above the filter.
    pub title: &'a str,
    /// The hint shown in the filter line while it is empty.
    pub placeholder: &'a str,
    /// Everything on offer.
    pub choices: &'a [Choice<'a>],
    /// How much colour to use.
    pub color: ColorChoice,
}

impl<'a> Request<'a> {
    /// A request with the usual placeholder and automatic colour.
    pub fn new(title: &'a str, choices: &'a [Choice<'a>]) -> Self {
        Self {
            title,
            placeholder: "type to filter",
            choices,
            color: ColorChoice::Auto,
        }
    }
}

/// How the prompt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// The reader picked this.
    Chose(&'a Choice<'a>),
    /// The reader left without picking: `Escape`, `Ctrl-C`, or a closed input.
    Cancelled,
    /// There was nobody to ask.
    NotInteractive,
}

/// Ask, and return what was chosen.
///
/// Draws on stderr rather than stdout, so `uf` can be asked a question and
/// still have its answer piped somewhere: the menu is the conversation, and
/// the command's output is the result.
pub fn select<'a>(request: &Request<'a>) -> Outcome<'a> {
    if !is_interactive() {
        return Outcome::NotInteractive;
    }
    let Ok(raw) = RawMode::enter() else {
        return Outcome::NotInteractive;
    };

    let theme = Theme::default();
    let capabilities = Capabilities::for_stderr(request.color, &TerminalEnv::from_process());
    let frame = draw::Frame {
        title: request.title,
        placeholder: request.placeholder,
        capabilities,
        theme: &theme,
    };

    let mut menu = Menu::new(request.choices);
    let outcome = run(&mut menu, &frame, &raw);

    // Erase the menu. A chosen command is about to print its own output, and
    // leaving the picker above it turns the answer into part of the question.
    clear(&frame, &menu);
    drop(raw);
    outcome
}

/// Read keys and redraw until something ends it.
fn run<'a>(menu: &mut Menu<'a>, frame: &draw::Frame<'_>, raw: &RawMode) -> Outcome<'a> {
    let mut drawn = 0usize;
    let mut buffer = String::with_capacity(1024);

    loop {
        buffer.clear();
        if drawn > 0 {
            // Back to the top of the block, then clear everything below: a
            // frame that lost rows must not leave the old ones on screen.
            buffer.push_str(&format!("\x1b[{drawn}A"));
            buffer.push_str("\x1b[J");
        }
        buffer.push_str("\x1b[?25l");
        draw::frame(menu, frame, &mut buffer);
        drawn = buffer.matches('\n').count();

        if write(&buffer).is_err() {
            return Outcome::Cancelled;
        }

        match read_key(raw) {
            Ok(None) | Err(_) => return Outcome::Cancelled,
            Ok(Some(Key::Escape)) => return Outcome::Cancelled,
            Ok(Some(Key::Enter)) => {
                // Nothing selected means nothing matches what was typed, and
                // the frame already says so.
                if let Some(choice) = menu.selected() {
                    return Outcome::Chose(choice);
                }
            }
            Ok(Some(Key::Up)) => menu.up(),
            Ok(Some(Key::Down)) => menu.down(),
            Ok(Some(Key::Backspace)) => menu.backspace(),
            Ok(Some(Key::ClearLine)) => menu.clear(),
            Ok(Some(Key::Char(character))) => menu.push(character),
            Ok(Some(Key::Other)) => {}
        }
    }
}

/// Erase the block the menu was drawing in.
fn clear(frame: &draw::Frame<'_>, menu: &Menu<'_>) {
    let mut buffer = String::new();
    draw::frame(menu, frame, &mut buffer);
    let lines = buffer.matches('\n').count();
    // The cursor is put back by `RawMode`'s drop, which runs whichever way
    // this returned, so showing it here as well would only be a second copy.
    let _ = write(&format!("\x1b[{lines}A\x1b[J"));
}

/// Write to stderr and flush, so a frame appears before the next key is read.
fn write(text: &str) -> io::Result<()> {
    let mut stderr = io::stderr();
    stderr.write_all(text.as_bytes())?;
    stderr.flush()
}

/// Whether there is a person at a terminal to ask.
///
/// Both streams, not just one. Stdin has to be a terminal or there are no
/// keystrokes to read; stderr has to be one or the frame is being written into
/// a file that will end up full of cursor movements.
fn is_interactive() -> bool {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return false;
    }
    // `CI` is set by every hosted runner, and a runner that allocates a tty —
    // several do — would otherwise sit at a prompt until the job times out.
    if std::env::var_os("CI").is_some() {
        return false;
    }
    !matches!(std::env::var("TERM").as_deref(), Ok("dumb"))
}
