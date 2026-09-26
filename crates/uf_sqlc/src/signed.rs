//! Meta's SignedSource: `@generated SignedSource<<md5>>`.
//!
//! A generated file carries the token below in its header; signing replaces
//! it with the MD5 of the whole file as it stood with the token in place.
//! Relay's compiler signs its artifacts the same way, and `uf fmt` leaves a
//! signed file alone (see `crates/uf_fmt`), so a project's own formatting
//! settings never fight the generator. MD5 is what the scheme specifies; it
//! detects a hand edit, it is not a security boundary.

/// The placeholder a file carries until it is signed.
pub const TOKEN: &str = "<<SignedSource::*O*zOeWoEQle#+L!plEphiEmie@IsG>>";

/// Replace [`TOKEN`] in `contents` with its signature.
#[must_use]
pub fn sign(contents: &str) -> String {
    let digest = md5(contents.as_bytes());
    let hex: String = digest
        .iter()
        .map(|byte| uf_infra::cstr!("{byte:02x}").into_string())
        .collect();
    contents.replacen(TOKEN, uf_infra::cstr!("SignedSource<<{hex}>>").as_str(), 1)
}

const SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// RFC 1321.
#[must_use]
pub fn md5(input: &[u8]) -> [u8; 16] {
    let constants: [u32; 64] = std::array::from_fn(|i| {
        // floor(abs(sin(i + 1)) * 2^32), which is exact in an f64.
        let sine = ((i + 1) as f64).sin().abs();
        (sine * 4_294_967_296.0) as u32
    });
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut message = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_le_bytes());
    for block in message.as_chunks::<64>().0 {
        let words: [u32; 16] = std::array::from_fn(|i| {
            u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ])
        });
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a
                .wrapping_add(f)
                .wrapping_add(constants[i])
                .wrapping_add(words[g])
                .rotate_left(SHIFTS[i]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotated);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 16]) -> String {
        bytes
            .iter()
            .map(|byte| uf_infra::cstr!("{byte:02x}").into_string())
            .collect()
    }

    #[test]
    fn md5_matches_rfc_1321_vectors() {
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(md5(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            )),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn signs_in_place() {
        let signed = sign(uf_infra::cstr!("/** @generated {TOKEN} */\n").as_str());
        assert!(signed.starts_with("/** @generated SignedSource<<"));
        assert!(!signed.contains(TOKEN));
    }
}
