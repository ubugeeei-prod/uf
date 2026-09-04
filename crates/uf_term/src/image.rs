//! Drawing a real image in a terminal, where the terminal can draw one.
//!
//! uf's mark is a picture, and for years the only way to put a picture in a
//! terminal was to approximate it out of block characters. That is no longer
//! true: kitty, ghostty, WezTerm, iTerm2, Konsole, VS Code's terminal and Hyper
//! all accept an image inline, and between them they cover most of the
//! terminals anyone installs uf from.
//!
//! Two protocols cover all of them — [`kitty`] and [`iterm2`] — and this module
//! is the whole of uf's use of either: encode one PNG, place it in a box
//! measured in cells, hand back a string. It never queries the terminal, never
//! writes a temporary file, and never blocks.
//!
//! **Nothing here decides whether to draw an image.** [`ImageEnv::protocol`]
//! says what the terminal understands; whether uf *should* — colour is on, the
//! stream is a terminal, the user has not asked for quiet — belongs to the
//! caller, which already knows all three. The fallback is not this module's
//! business either: uf's block mark renders on everything and is what a caller
//! prints when this returns [`None`].

mod base64;
mod iterm2;
mod kitty;
mod protocol;

#[cfg(test)]
mod tests;

pub use protocol::{ImageEnv, ImageProtocol};

/// Base64 characters one kitty escape sequence may carry.
///
/// The protocol's own limit. Exceeding it is not a soft failure: the terminal
/// discards the sequence and prints nothing.
const MAX_PAYLOAD_CHARS: usize = 4096;

/// Where an inline image goes, measured in terminal cells.
///
/// Cells rather than pixels, because the layout around the image is measured in
/// cells and a terminal's cell size is not knowable from here. Both protocols
/// scale the image into this box and preserve its aspect ratio, so the box is a
/// bound and the picture is never stretched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Width in cells.
    pub columns: u16,
    /// Height in cells.
    pub rows: u16,
}

impl Placement {
    /// A placement `columns` wide and `rows` tall.
    #[must_use]
    pub const fn new(columns: u16, rows: u16) -> Self {
        Self { columns, rows }
    }
}

/// Encode `png` for `protocol`, placed in `placement`.
///
/// Returns [`None`] for an empty payload rather than an escape sequence with
/// nothing in it: a terminal handed one either prints nothing or prints the
/// sequence, and the caller's block mark is better than both.
#[must_use]
pub fn inline_image(png: &[u8], protocol: ImageProtocol, placement: Placement) -> Option<String> {
    if png.is_empty() {
        return None;
    }

    let mut out = String::new();
    match protocol {
        ImageProtocol::Kitty => kitty::encode(png, placement, &mut out),
        ImageProtocol::ITerm2 => iterm2::encode(png, placement, &mut out),
    }
    Some(out)
}

/// Append `value`'s decimal spelling to `out`.
///
/// `push_str(&value.to_string())` would allocate a `String` per number, and
/// this module assembles escape sequences into one buffer on purpose.
fn push_number(out: &mut String, value: impl Into<u64>) {
    let mut value = value.into();
    if value == 0 {
        out.push('0');
        return;
    }
    let mut digits = [0u8; 20];
    let mut length = 0;
    while value > 0 {
        digits[length] = b'0' + (value % 10) as u8;
        value /= 10;
        length += 1;
    }
    for index in (0..length).rev() {
        out.push(digits[index] as char);
    }
}
