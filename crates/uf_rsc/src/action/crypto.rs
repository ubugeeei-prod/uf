//! The primitives the action-id construction rests on.
//!
//! Kept together because each one is here for a security reason rather than for
//! convenience: the keyed digest that makes an id underivable without the build
//! id, the branch-free digest comparison that keeps a lookup from leaking how
//! much of a guess matched, the hexadecimal codec for the wire form, and the
//! operating-system entropy a fresh build id is drawn from.

use compact_str::CompactString;
use sha2::{Digest, Sha256};

/// HMAC-SHA256, RFC 2104.
pub(crate) fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_LEN: usize = 64;

    let mut block = [0u8; BLOCK_LEN];
    if key.len() > BLOCK_LEN {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0x36u8; BLOCK_LEN];
    let mut outer_pad = [0x5cu8; BLOCK_LEN];
    for ((inner, outer), key_byte) in inner_pad
        .iter_mut()
        .zip(outer_pad.iter_mut())
        .zip(block.iter())
    {
        *inner ^= key_byte;
        *outer ^= key_byte;
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    outer.finalize().into()
}

/// Returns 1 when the two digests are equal, without an early exit.
pub(crate) fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> u8 {
    let mut difference = 0u8;
    for (left_byte, right_byte) in left.iter().zip(right.iter()) {
        difference |= left_byte ^ right_byte;
    }
    (((difference as u32).wrapping_sub(1)) >> 31) as u8
}

pub(crate) fn hex(bytes: &[u8]) -> CompactString {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    CompactString::from(text)
}

pub(crate) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

pub(crate) fn os_entropy(buffer: &mut [u8]) -> bool {
    use std::io::Read;

    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(buffer))
        .is_ok()
}
