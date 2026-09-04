//! The kitty graphics protocol.
//!
//! One image becomes one or more Application Programming Command sequences:
//!
//! ```text
//! ESC _G <key>=<value>,... ; <base64 payload> ESC \
//! ```
//!
//! The payload is chunked because the protocol caps one escape sequence's
//! payload at 4096 base64 characters. Every chunk but the last carries `m=1`,
//! "more is coming"; the last carries `m=0`. Only the first chunk carries the
//! rest of the keys — repeating them on a continuation is an error, not a
//! redundancy.

use super::base64::encode_into;
use super::{MAX_PAYLOAD_CHARS, Placement, push_number};

/// Write `png` as kitty graphics escapes.
///
/// - `a=T` transmit and display in one step, so nothing has to be freed later.
/// - `f=100` the payload is a PNG, which the terminal decodes itself.
/// - `t=d` the payload is the data, not a path — uf never writes a temporary
///   file for this, which also means it works over ssh.
/// - `c` and `r` place the image in a box that many cells wide and tall, so the
///   surrounding layout is the same whatever the terminal's cell size is.
pub(super) fn encode(png: &[u8], placement: Placement, out: &mut String) {
    let mut payload = String::new();
    encode_into(png, &mut payload);

    let mut rest = payload.as_str();
    let mut first = true;
    loop {
        let take = rest.len().min(MAX_PAYLOAD_CHARS);
        let (chunk, remainder) = rest.split_at(take);
        rest = remainder;

        out.push_str("\x1b_G");
        if first {
            out.push_str("a=T,f=100,t=d,c=");
            push_number(out, placement.columns);
            out.push_str(",r=");
            push_number(out, placement.rows);
            out.push(',');
            first = false;
        }
        out.push_str(if rest.is_empty() { "m=0;" } else { "m=1;" });
        out.push_str(chunk);
        out.push_str("\x1b\\");

        if rest.is_empty() {
            break;
        }
    }
}
