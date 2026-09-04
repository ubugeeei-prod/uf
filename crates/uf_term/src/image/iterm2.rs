//! iTerm2's inline images.
//!
//! One image is one operating system command:
//!
//! ```text
//! ESC ] 1337 ; File = <key>=<value>;... : <base64 payload> BEL
//! ```
//!
//! No chunking: the sequence carries the whole payload, and terminals that
//! implement this read to the terminator. `size` is the *decoded* byte count,
//! which iTerm2 uses to show progress while a large image arrives.

use super::Placement;
use super::base64::encode_into;
use super::push_number;

/// Write `png` as an iTerm2 inline image escape.
///
/// `width` and `height` are given in cells, so the image occupies the same
/// space in the layout as the block mark it replaces. `preserveAspectRatio=1`
/// makes that box a bound rather than a stretch, and `inline=1` is what
/// separates displaying the image from downloading it as a file.
pub(super) fn encode(png: &[u8], placement: Placement, out: &mut String) {
    out.push_str("\x1b]1337;File=inline=1;preserveAspectRatio=1;size=");
    push_number(out, png.len() as u64);
    out.push_str(";width=");
    push_number(out, placement.columns);
    out.push_str(";height=");
    push_number(out, placement.rows);
    out.push(':');
    encode_into(png, out);
    out.push('\x07');
}
