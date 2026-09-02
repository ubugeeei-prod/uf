//! Content hashing for chunk file names.
//!
//! A chunk's name has to change when — and only when — its bytes change, or a
//! CDN either serves stale JavaScript or throws away a cache for nothing. The
//! hash is therefore taken over a length-prefixed encoding of everything that
//! decides the chunk's content, so two different module lists can never hash
//! the same way by concatenating to the same bytes.

use sha2::{Digest, Sha256};

/// Hex characters in a chunk-name hash.
///
/// Eight is what the ecosystem uses: long enough that a collision needs
/// billions of chunks, short enough to read in a directory listing.
pub const HASH_HEX_LEN: usize = 8;

/// Accumulates the parts of a chunk's identity into one content hash.
#[derive(Debug, Default, Clone)]
pub struct ContentHasher {
    digest: Sha256,
}

impl ContentHasher {
    /// A fresh hasher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Absorb one field.
    ///
    /// The length prefix is what makes the encoding unambiguous: without it,
    /// `["ab", "c"]` and `["a", "bc"]` would produce the same digest.
    pub fn field(&mut self, bytes: &[u8]) {
        self.digest.update((bytes.len() as u64).to_le_bytes());
        self.digest.update(bytes);
    }

    /// Absorb one field, returning the hasher for chaining.
    #[must_use]
    pub fn with(mut self, bytes: &[u8]) -> Self {
        self.field(bytes);
        self
    }

    /// The first [`HASH_HEX_LEN`] hex characters of the digest.
    #[must_use]
    pub fn finish(self) -> String {
        let digest = self.digest.finalize();
        let mut hex = String::with_capacity(HASH_HEX_LEN);
        for byte in digest.iter().take(HASH_HEX_LEN.div_ceil(2)) {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
        }
        hex.truncate(HASH_HEX_LEN);
        hex
    }
}

/// The content hash of a single byte string.
#[must_use]
pub fn hash_bytes(bytes: &[u8]) -> String {
    ContentHasher::new().with(bytes).finish()
}
