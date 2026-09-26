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
mod input;
mod key;
mod menu;
mod multi;
mod raw;

#[cfg(test)]
mod tests;

use std::io::{self, IsTerminal, Write};

use crate::capability::{Capabilities, ColorChoice, TerminalEnv};
use crate::theme::Theme;

pub use input::{Answer, Question, input};
pub use menu::{Choice, VISIBLE};
pub use multi::{ManyOutcome, select_many};

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
    let mut stderr = io::stderr();
    let outcome = run(
        &mut menu,
        &mut || read_key(&raw),
        &mut |menu, buffer| draw::frame(menu, &frame, buffer),
        &mut stderr,
    );
    drop(raw);
    outcome
}

/// Where keystrokes come from: the terminal, or a test's list of them.
///
/// `Ok(None)` is the input ending, which every prompt reads as "cancelled".
type Keys<'k> = dyn FnMut() -> io::Result<Option<Key>> + 'k;

/// What one key does to a menu, and whether it ends the prompt.
fn press<'a>(menu: &mut Menu<'a>, key: Key) -> Option<Outcome<'a>> {
    match key {
        Key::Escape => return Some(Outcome::Cancelled),
        // Nothing selected means nothing matches what was typed, and the
        // frame already says so.
        Key::Enter => return menu.selected().map(Outcome::Chose),
        Key::Up => menu.up(),
        Key::Down => menu.down(),
        Key::Backspace => menu.backspace(),
        Key::ClearLine => menu.clear(),
        Key::Char(character) => menu.push(character),
        Key::Other => {}
    }
    None
}

/// Read keys and redraw until something ends it, then erase the block.
///
/// Generic over where keys come from and where frames go, so the whole loop —
/// not only the state it drives — runs in a test with no terminal.
fn run<'a, W: Write + ?Sized>(
    menu: &mut Menu<'a>,
    keys: &mut Keys<'_>,
    draw: &mut dyn FnMut(&Menu<'a>, &mut String),
    out: &mut W,
) -> Outcome<'a> {
    let outcome = redraw_until(menu, keys, draw, out, |menu, key| press(menu, key))
        .unwrap_or(Outcome::Cancelled);
    // Erase the menu. A chosen command is about to print its own output, and
    // leaving the picker above it turns the answer into part of the question.
    erase(menu, draw, out);
    outcome
}

/// The loop every prompt shares: draw a frame, read a key, apply it.
///
/// `None` when the input ended or could not be written to — cancelled, as far
/// as any caller is concerned.
fn redraw_until<S, T, W: Write + ?Sized>(
    state: &mut S,
    keys: &mut Keys<'_>,
    draw: &mut dyn FnMut(&S, &mut String),
    out: &mut W,
    mut apply: impl FnMut(&mut S, Key) -> Option<T>,
) -> Option<T> {
    let mut drawn = 0usize;
    let mut buffer = String::with_capacity(1024);

    loop {
        buffer.clear();
        if drawn > 0 {
            // Back to the top of the block, then clear everything below: a
            // frame that lost rows must not leave the old ones on screen.
            uf_infra::append!(buffer, "\x1b[{drawn}A");
            buffer.push_str("\x1b[J");
        }
        buffer.push_str("\x1b[?25l");
        let start = buffer.len();
        draw(state, &mut buffer);
        drawn = buffer[start..].matches('\n').count();

        if write(out, &buffer).is_err() {
            return None;
        }

        match keys() {
            Ok(Some(key)) => {
                if let Some(done) = apply(state, key) {
                    return Some(done);
                }
            }
            Ok(None) | Err(_) => return None,
        }
    }
}

/// Erase the block a prompt was drawing in.
fn erase<S, W: Write + ?Sized>(state: &S, draw: &mut dyn FnMut(&S, &mut String), out: &mut W) {
    let mut buffer = String::new();
    draw(state, &mut buffer);
    let lines = buffer.matches('\n').count();
    // The cursor is put back by `RawMode`'s drop, which runs whichever way
    // this returned, so showing it here as well would only be a second copy.
    let _ = write(out, uf_infra::cstr!("\x1b[{lines}A\x1b[J").as_str());
}

/// Write and flush, so a frame appears before the next key is read.
fn write<W: Write + ?Sized>(out: &mut W, text: &str) -> io::Result<()> {
    out.write_all(text.as_bytes())?;
    out.flush()
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
