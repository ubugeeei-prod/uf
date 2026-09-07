//! What stops the second build doing the first build's work again.
//!
//! # The problem
//!
//! Every emitted file in this crate is already named by a digest of its source
//! and its parameters, so a rebuild writes the same bytes to the same path and
//! nothing downstream re-downloads. That makes the *output* incremental and
//! leaves the *work* untouched: the second build still decodes the image,
//! still runs Lanczos over it at seven widths, still encodes fourteen files,
//! still cuts the font into eight buckets — and then writes exactly what was
//! already there.
//!
//! On the fixture applications in this repository that is the whole cost of
//! the asset stage. So the manifest is cached beside the files, under the same
//! digest the files are named by, and a warm build reads one small JSON
//! document per asset instead.
//!
//! # Invalidation
//!
//! There is no invalidation step, because there is nothing to invalidate: the
//! key *is* the digest of every input. Change the image and the key changes.
//! Change a width, the quality, the family, the subset mode, the base URL, or
//! the version of this crate's cache format, and the key changes. A stale
//! entry cannot be read because nothing asks for it. Old entries are left
//! where they are — the cache directory is `.uf/cache`, a build artefact a
//! project already deletes wholesale.
//!
//! What the key cannot cover is the *files*: a manifest names files that a
//! `rm -rf dist` may have taken with it while the cache directory survived. So
//! a hit is only a hit when every file it names is still on disk, and a
//! manifest whose files are gone is treated as a miss and the work is redone.
//! That check is a `stat` per file, which is what makes it safe to trust the
//! rest of the entry without re-reading it.

use camino::Utf8Path;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest as _, Sha256};

#[cfg(test)]
mod tests;

/// Where manifests live under the caller's output directory.
///
/// A dotted subdirectory: the Vite plugin serves the output directory by file
/// name and emits the files it was told about, so a manifest must not look
/// like an asset. Nothing ever asks for one over HTTP.
const MANIFESTS: &str = ".manifests";

/// Bumped when the shape of a cached manifest changes.
///
/// Part of every key, so an upgrade of this crate reads none of the previous
/// version's entries rather than deserialising one into a struct that has
/// since grown a field.
const CACHE_VERSION: &str = "uf-assets/cache/v1";

/// A value, and whether it came from the cache.
///
/// The flag is not decoration: `uf explain build` reports it, and a benchmark
/// that cannot tell a warm run from a cold one is measuring nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cached<T> {
    /// The manifest.
    pub value: T,
    /// Whether it was read rather than produced.
    pub hit: bool,
}

/// The cache key for one asset.
///
/// `parts` must contain every input that could change the output — the source
/// bytes and each parameter — because the key is the whole of the
/// invalidation. Length-framed for the reason [`crate::name`] gives: a path
/// and a family are untrusted text that can contain any byte.
#[must_use]
pub fn cache_key(domain: &str, parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    frame(&mut hasher, CACHE_VERSION.as_bytes());
    frame(&mut hasher, domain.as_bytes());
    for part in parts {
        frame(&mut hasher, part);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(32);
    for byte in &digest[..16] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn frame(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u32).to_le_bytes());
    hasher.update(bytes);
}

/// Produce a manifest, or read the one a previous build left.
///
/// `files` names the emitted files a cached manifest depends on, relative to
/// `out_dir`; a hit is only returned when all of them are still there.
///
/// # Errors
///
/// Whatever `produce` returns. A cache that cannot be read or written is never
/// an error — it is a miss, and then a build that is slower rather than one
/// that failed.
pub fn memoise<T, E>(
    out_dir: &Utf8Path,
    key: &str,
    files: impl Fn(&T) -> Vec<String>,
    produce: impl FnOnce() -> Result<T, E>,
) -> Result<Cached<T>, E>
where
    T: Serialize + DeserializeOwned,
{
    let entry = out_dir.join(MANIFESTS).join(format!("{key}.json"));
    if let Ok(raw) = std::fs::read(&entry)
        && let Ok(value) = serde_json::from_slice::<T>(&raw)
        && files(&value).iter().all(|file| out_dir.join(file).exists())
    {
        return Ok(Cached { value, hit: true });
    }

    let value = produce()?;
    if let Ok(raw) = serde_json::to_vec(&value) {
        // Best effort in both directions. A build that could not write its
        // cache has still produced the right files, and turning that into a
        // failure would make a read-only output directory — a container image
        // layer, a sandbox — fail a build that works.
        let _ = std::fs::create_dir_all(out_dir.join(MANIFESTS));
        let _ = std::fs::write(&entry, raw);
    }
    Ok(Cached { value, hit: false })
}
