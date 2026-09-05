//! One keystroke, read from a terminal in raw mode.
//!
//! A key is not a byte. An arrow is three bytes, a character outside ASCII is
//! two to four, and `Escape` is the first byte of most of the sequences it is
//! not. This module is the whole of that decoding, so the menu above it can be
//! written against `Key` and tested without a terminal.

use std::io::{self, Read};

use super::raw::RawMode;

/// What the reader pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Move the selection towards the top of the list.
    Up,
    /// Move the selection towards the bottom.
    Down,
    /// Take the highlighted item.
    Enter,
    /// Leave, taking nothing.
    Escape,
    /// Delete the character before the cursor.
    Backspace,
    /// Clear the whole filter, which is `Ctrl-U` as it is in a shell.
    ClearLine,
    /// A character to add to the filter.
    Char(char),
    /// A key with no meaning here.
    Other,
}

/// `Ctrl-C`. Not a `Key`: it means "stop", not "leave the menu".
pub const INTERRUPT: u8 = 0x03;

/// Read one keystroke.
///
/// `Ok(None)` means the input ended — a closed terminal, or `Ctrl-D` — which
/// is the same answer as `Escape` to every caller here but is worth keeping
/// distinct from "the reader chose to leave".
pub fn read_key(raw: &RawMode) -> io::Result<Option<Key>> {
    let Some(first) = read_byte_blocking(raw)? else {
        return Ok(None);
    };

    match first {
        INTERRUPT | 0x04 => Ok(None),
        b'\r' | b'\n' => Ok(Some(Key::Enter)),
        0x7f | 0x08 => Ok(Some(Key::Backspace)),
        0x15 => Ok(Some(Key::ClearLine)),
        0x1b => escape_sequence(raw),
        // `Ctrl-N` and `Ctrl-P`, which move in a shell's history and so move
        // here too. `j` and `k` are deliberately *not* bound: they are letters,
        // and every letter belongs to the filter.
        0x0e => Ok(Some(Key::Down)),
        0x10 => Ok(Some(Key::Up)),
        byte if byte < 0x20 => Ok(Some(Key::Other)),
        byte => decode_char(raw, byte),
    }
}

/// What followed an `ESC`.
///
/// Nothing at all means the reader pressed `Escape` itself, which is why the
/// terminal is put into its brief mode first — see [`RawMode::brief`].
fn escape_sequence(raw: &RawMode) -> io::Result<Option<Key>> {
    raw.brief()?;
    let next = read_byte(&mut [0u8; 1])?;
    let key = match next {
        None => Key::Escape,
        // `ESC [ A` and `ESC O A`: the second form is what a terminal in
        // application-cursor mode sends, which several send by default.
        Some(b'[' | b'O') => match read_byte(&mut [0u8; 1])? {
            Some(b'A') => Key::Up,
            Some(b'B') => Key::Down,
            // An arrow's siblings — Home, End, Page Up — arrive as `ESC [ 1 ~`
            // and friends. Their trailing bytes are drained so they do not
            // arrive later as stray characters in the filter.
            Some(byte) if byte.is_ascii_digit() => {
                drain_until_tilde()?;
                Key::Other
            }
            _ => Key::Other,
        },
        Some(_) => Key::Other,
    };
    raw.blocking()?;
    Ok(Some(key))
}

/// Swallow the rest of a `ESC [ <digits> ~` sequence.
fn drain_until_tilde() -> io::Result<()> {
    /// A sequence longer than this is not one a terminal sent.
    const BUDGET: usize = 8;

    for _ in 0..BUDGET {
        match read_byte(&mut [0u8; 1])? {
            None | Some(b'~') => return Ok(()),
            Some(_) => {}
        }
    }
    Ok(())
}

/// Finish a UTF-8 character whose first byte has already been read.
///
/// The filter is text, and a reader typing `テ` should see `テ` rather than
/// three replacement characters. The continuation bytes arrive together, so
/// this never has to wait.
fn decode_char(raw: &RawMode, first: u8) -> io::Result<Option<Key>> {
    let extra = match first {
        0x00..=0x7f => 0,
        0xc0..=0xdf => 1,
        0xe0..=0xef => 2,
        0xf0..=0xf7 => 3,
        // A continuation byte with no lead, which is not text.
        _ => return Ok(Some(Key::Other)),
    };

    let mut bytes = [first, 0, 0, 0];
    for slot in bytes.iter_mut().take(extra + 1).skip(1) {
        let Some(byte) = read_byte_blocking(raw)? else {
            return Ok(Some(Key::Other));
        };
        *slot = byte;
    }

    match std::str::from_utf8(&bytes[..=extra]) {
        Ok(text) => Ok(text.chars().next().map(Key::Char)),
        Err(_) => Ok(Some(Key::Other)),
    }
}

/// Read one byte, waiting for it.
fn read_byte_blocking(raw: &RawMode) -> io::Result<Option<u8>> {
    raw.blocking()?;
    read_byte(&mut [0u8; 1])
}

/// Read one byte in whatever mode the terminal is currently in.
fn read_byte(buffer: &mut [u8; 1]) -> io::Result<Option<u8>> {
    match io::stdin().read(buffer) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(buffer[0])),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(None),
        Err(error) => Err(error),
    }
}
