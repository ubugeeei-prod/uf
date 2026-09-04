//! Standard base64, because both inline-image protocols carry their payload
//! that way.
//!
//! Written here rather than depended on: `uf_term` has no dependencies, and an
//! encoder with no alphabet options, no streaming, and no decoder is forty
//! lines. A crate would be more code to audit than this is to read.

/// The standard alphabet, RFC 4648 §4.
const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode `bytes` as standard base64 with padding, appending to `out`.
///
/// Appends rather than returns, so a caller assembling an escape sequence
/// writes the payload straight into the buffer it is already building.
pub(super) fn encode_into(bytes: &[u8], out: &mut String) {
    out.reserve(encoded_len(bytes.len()));

    let chunks = bytes.as_chunks::<3>();
    for chunk in chunks.0 {
        let triple = u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8 | u32::from(chunk[2]);
        for shift in [18, 12, 6, 0] {
            out.push(ALPHABET[(triple >> shift & 0x3f) as usize] as char);
        }
    }

    match chunks.1 {
        [] => {}
        [one] => {
            let triple = u32::from(*one) << 16;
            out.push(ALPHABET[(triple >> 18 & 0x3f) as usize] as char);
            out.push(ALPHABET[(triple >> 12 & 0x3f) as usize] as char);
            out.push_str("==");
        }
        [one, two] => {
            let triple = u32::from(*one) << 16 | u32::from(*two) << 8;
            for shift in [18, 12, 6] {
                out.push(ALPHABET[(triple >> shift & 0x3f) as usize] as char);
            }
            out.push('=');
        }
        // `as_chunks::<3>` cannot leave three or more bytes over.
        _ => unreachable!("the remainder of a 3-byte chunking is shorter than 3"),
    }
}

/// How many characters `encode_into` will append for `len` bytes.
pub(super) const fn encoded_len(len: usize) -> usize {
    len.div_ceil(3) * 4
}

/// Encode `bytes` into a fresh `String`.
#[cfg(test)]
pub(super) fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    encode_into(bytes, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vectors from RFC 4648 §10, which is what "standard base64" means.
    #[test]
    fn matches_the_rfc_4648_test_vectors() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn encodes_every_byte_value() {
        let all = (0u8..=255).collect::<Vec<_>>();
        let encoded = encode(&all);

        assert_eq!(encoded.len(), encoded_len(all.len()));
        assert!(
            encoded
                .bytes()
                .all(|byte| ALPHABET.contains(&byte) || byte == b'='),
            "every character must be in the alphabet or be padding"
        );
    }

    /// The high bit must survive: a PNG is mostly bytes above 0x7f, and an
    /// encoder that sign-extended them would corrupt every image it sent.
    #[test]
    fn high_bytes_are_not_sign_extended() {
        assert_eq!(encode(&[0xff, 0xff, 0xff]), "////");
        assert_eq!(encode(&[0x80, 0x00, 0x00]), "gAAA");
    }

    #[test]
    fn the_predicted_length_is_the_real_length() {
        for len in 0..64 {
            let bytes = vec![0xa5; len];
            assert_eq!(encode(&bytes).len(), encoded_len(len), "at length {len}");
        }
    }

    #[test]
    fn encoding_appends_rather_than_replaces() {
        let mut out = String::from("data:image/png;base64,");
        encode_into(b"foobar", &mut out);

        assert_eq!(out, "data:image/png;base64,Zm9vYmFy");
    }
}
