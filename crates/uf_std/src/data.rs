//! Hashing, comparison, buffers, digests and the small value codecs.
//!
//! The fast hash here is explicitly non-cryptographic and exists for hot maps;
//! anything with a security meaning goes through [`digest_bytes`] or
//! [`constant_time_equal`], which does not exit early on the first differing
//! byte.

use std::hash::Hasher;

use compact_str::CompactString;
use rustc_hash::FxHasher;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use smallvec::SmallVec;

/// Small byte buffer optimized for common protocol and hashing paths.
pub type InlineBytes = SmallVec<[u8; 64]>;

/// Hash bytes with the native fast hash used by hot uf maps.
pub fn fast_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write(bytes);
    hasher.finish()
}

/// Hash UTF-8 text with the native fast hash.
pub fn fast_hash_str(value: &str) -> u64 {
    fast_hash_bytes(value.as_bytes())
}

/// Compare bytes without early return on content mismatch.
pub fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut diff = 0u8;
    for index in 0..left.len() {
        diff |= left[index] ^ right[index];
    }
    diff == 0
}

/// Inline byte buffer for `@uniflowed/std/buffer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ByteBuffer {
    /// Buffer bytes.
    pub bytes: InlineBytes,
}

impl ByteBuffer {
    /// Create a buffer from a byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut buffer = InlineBytes::new();
        buffer.extend_from_slice(bytes);
        Self { bytes: buffer }
    }

    /// Create a UTF-8 buffer.
    pub fn from_utf8(value: &str) -> Self {
        Self::from_bytes(value.as_bytes())
    }

    /// Return the underlying byte slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Return the buffer length.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Return whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Encode the buffer as lowercase hexadecimal text.
    pub fn to_hex(&self) -> CompactString {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = CompactString::new("");
        for byte in &self.bytes {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }
}

/// Split a slice into fixed-size borrowed chunks.
pub fn chunk<T>(items: &[T], size: usize) -> SmallVec<[&[T]; 8]> {
    let mut chunks = SmallVec::new();
    if size == 0 {
        return chunks;
    }
    for chunk in items.chunks(size) {
        chunks.push(chunk);
    }
    chunks
}

/// Clamp an integer between inclusive bounds.
pub fn clamp(value: i64, min: i64, max: i64) -> i64 {
    value.max(min).min(max)
}

/// Linearly interpolate two floating point values.
pub fn lerp(start: f64, end: f64, amount: f64) -> f64 {
    start + (end - start) * amount
}

/// Supported crypto digest algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DigestAlgorithm {
    /// Fast non-cryptographic hash for caches and maps.
    FastHash,
    /// SHA-256 contract for WebCrypto-compatible bindings.
    Sha256,
}

/// Digest bytes with the requested algorithm.
pub fn digest_bytes(algorithm: DigestAlgorithm, bytes: &[u8]) -> CompactString {
    match algorithm {
        DigestAlgorithm::FastHash => hex_u64(fast_hash_bytes(bytes)),
        DigestAlgorithm::Sha256 => hex_bytes(Sha256::digest(bytes).as_slice()),
    }
}

fn hex_bytes(bytes: &[u8]) -> CompactString {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = CompactString::new("");
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_u64(value: u64) -> CompactString {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = CompactString::new("");
    for shift in (0..64).step_by(4).rev() {
        let index = ((value >> shift) & 0x0f) as usize;
        output.push(HEX[index] as char);
    }
    output
}

/// Decode one hexadecimal digit, or `None` when the byte is not one.
pub(crate) fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// UUID version supported by the native generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UuidVersion {
    /// Random UUID v4.
    V4,
    /// Time-ordered UUID v7.
    V7,
}

/// Parsed UUID descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedUuid {
    /// Lowercase canonical UUID text.
    pub value: CompactString,
    /// UUID version nibble when present.
    pub version: Option<u8>,
}

/// Parse canonical UUID text.
pub fn parse_uuid(value: &str) -> Option<ParsedUuid> {
    let bytes = value.as_bytes();
    let dash_positions = [8, 13, 18, 23];
    if bytes.len() != 36 {
        return None;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if dash_positions.contains(&index) {
            if *byte != b'-' {
                return None;
            }
        } else if !byte.is_ascii_hexdigit() {
            return None;
        }
    }
    let version = hex_value(bytes[14]);
    Some(ParsedUuid {
        value: value.to_ascii_lowercase().into(),
        version,
    })
}

/// ZIP compression mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ZipCompression {
    /// Store files without compression.
    Store,
    /// Deflate compression.
    Deflate,
}

/// ZIP archive entry descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipEntry {
    /// Entry path inside the archive.
    pub path: CompactString,
    /// Compression mode.
    pub compression: ZipCompression,
    /// Uncompressed size in bytes.
    pub size: u64,
}
