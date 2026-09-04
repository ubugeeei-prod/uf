//! Raw mode, and putting the terminal back the way it was found.
//!
//! A menu needs every keystroke as it happens: no line buffering, and no echo
//! of the arrow keys as `^[[A`. That is a terminal *mode*, not something a
//! program can ask for per read.
//!
//! It is set here by running `stty`, and the reason is a constraint this crate
//! chose: `uf_term` has no third-party dependencies, and `tcsetattr` needs
//! either `libc` or a hand-written `struct termios` whose layout differs
//! between macOS and Linux — a thing that goes wrong silently and only on the
//! platform nobody tested. `stty` is POSIX, is on every machine that has a
//! shell, and takes about a millisecond. A menu opens once.
//!
//! The saved settings come from `stty -g`, which prints them in a form `stty`
//! itself accepts. Restoring is therefore exact rather than a guess at what
//! the terminal probably had: a program that leaves a terminal in raw mode
//! leaves the user typing into a shell that does not echo, which they can only
//! fix by closing the window.

use std::io::{self, Write};
use std::process::{Command, Stdio};

/// Raw mode for as long as this value lives.
///
/// Restoring on drop rather than at the end of the menu, because the ways out
/// of a menu are many — a chosen item, an escape, a `?` that fails to write —
/// and every one of them has to put the terminal back. A `Drop` covers the
/// early return that has not been written yet, which is the one that will
/// otherwise be forgotten.
#[derive(Debug)]
pub struct RawMode {
    /// What `stty -g` reported before anything was changed.
    saved: String,
}

impl RawMode {
    /// Put the terminal into raw mode, remembering how to undo it.
    ///
    /// Fails when there is no terminal to change, which is the same condition
    /// that means no menu should have been opened.
    pub fn enter() -> io::Result<Self> {
        let saved = stty(&["-g"])?;
        let saved = saved.trim().to_owned();
        if saved.is_empty() {
            return Err(io::Error::other("stty -g reported nothing"));
        }
        // `-echo` as well as `raw`: `raw` alone still echoes, and an arrow key
        // would print `^[[A` into the middle of the menu it is scrolling.
        stty(&["raw", "-echo"])?;
        Ok(Self { saved })
    }

    /// Wait for a byte rather than returning immediately when none is ready.
    ///
    /// The mode a key read wants: one byte, however long that takes.
    pub fn blocking(&self) -> io::Result<()> {
        stty(&["min", "1", "time", "0"]).map(drop)
    }

    /// Return after a tenth of a second whether or not a byte arrived.
    ///
    /// This exists for exactly one ambiguity. `Escape` is one byte, and so is
    /// the start of every arrow key — the terminal sends `ESC [ A` for "up".
    /// Reading the byte after an `ESC` while blocking means a reader who
    /// pressed `Escape` and nothing else waits forever, and the menu looks
    /// frozen. A tenth of a second is far longer than the gap inside a real
    /// escape sequence and far shorter than a person notices.
    pub fn brief(&self) -> io::Result<()> {
        stty(&["min", "0", "time", "1"]).map(drop)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = stty(&[self.saved.as_str()]);
        // The cursor is hidden while a menu is drawing; leaving it hidden is
        // the other way to hand back a terminal that looks broken.
        let mut stderr = io::stderr();
        let _ = stderr.write_all(b"\x1b[?25h");
        let _ = stderr.flush();
    }
}

/// Run `stty` against the controlling terminal and return what it printed.
///
/// `stdin` is inherited deliberately: `stty` acts on the terminal attached to
/// its own standard input, and that is the terminal the reader is typing at.
fn stty(args: &[&str]) -> io::Result<String> {
    let output = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!("stty {} failed", args.join(" "))));
    }
    String::from_utf8(output.stdout).map_err(io::Error::other)
}
