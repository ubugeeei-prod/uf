//! The one hash this crate uses, and the one way it is built.
//!
//! Lifted in shape from `uf_check::cache`, which settled the argument first:
//! every field is followed by a NUL so that two different splits of the same
//! bytes cannot produce one digest, and a builder is tagged with what the
//! digest is *for* so that two digests over the same fields in two different
//! roles cannot collide either.

use std::fs;
use std::io::Read as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

/// A SHA-256 digest.
pub(crate) type Digest = [u8; 32];

/// A digest as lowercase hex.
pub(crate) fn hex(digest: &Digest) -> String {
    const DIGITS: [u8; 16] = *b"0123456789abcdef";
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    out
}

/// A digest builder for a named role.
pub(crate) struct Fields(Sha256);

impl Fields {
    /// A builder tagged with what the digest is for.
    pub(crate) fn new(domain: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(domain.as_bytes());
        hasher.update([0]);
        Self(hasher)
    }

    /// Add one field.
    pub(crate) fn push(&mut self, field: &str) -> &mut Self {
        self.0.update(field.as_bytes());
        self.0.update([0]);
        self
    }

    /// Add one already-digested field.
    pub(crate) fn push_digest(&mut self, digest: &Digest) -> &mut Self {
        self.0.update(digest);
        self.0.update([0]);
        self
    }

    /// Finish.
    pub(crate) fn finish(&self) -> Digest {
        self.0.clone().finalize().into()
    }
}

/// Largest file this crate will hash as an input, in bytes.
///
/// A task that names a two-gigabyte artefact as an input has said something
/// uf cannot check cheaply, and hashing it would cost more than most tasks
/// cost to run. Past this the file is not summarised in any weaker way — it is
/// reported as unhashable, and a task with an unhashable input is not cached.
/// A cache that gave up on the expensive half of a key and kept the rest would
/// be answering from a key that no longer describes the inputs.
pub(crate) const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;

/// The digest of a file's contents, or [`None`] when it cannot be read or is
/// past [`MAX_INPUT_BYTES`].
///
/// Streamed rather than read whole: `target/release/uf` is a hundred megabytes
/// and is a perfectly reasonable thing for a task to name.
pub(crate) fn file_digest(path: &Path) -> Option<Digest> {
    let mut file = fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES {
        return None;
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(_) => return None,
        }
    }
    Some(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_between_two_fields_changes_the_digest() {
        let mut one = Fields::new("test");
        one.push("ab").push("");
        let mut two = Fields::new("test");
        two.push("a").push("b");
        assert_ne!(one.finish(), two.finish());
    }

    #[test]
    fn the_domain_is_part_of_the_digest() {
        let mut one = Fields::new("task");
        one.push("x");
        let mut two = Fields::new("input");
        two.push("x");
        assert_ne!(one.finish(), two.finish());
    }

    #[test]
    fn a_missing_file_has_no_digest() {
        assert!(file_digest(Path::new("/definitely/not/here")).is_none());
    }
}
