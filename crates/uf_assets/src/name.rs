//! The names uf gives the files it emits.
//!
//! An emitted asset is served with a far-future cache header, so its name has
//! to change exactly when its bytes do — no sooner, or every deploy costs every
//! reader a re-download, and no later, or a reader keeps a stale file forever.
//! The name therefore carries a digest of the bytes and of every parameter that
//! produced them.
//!
//! # Construction
//!
//! ```text
//! frame(s) = u32::to_le_bytes(s.len()) ‖ s
//! digest   = SHA-256( frame(domain) ‖ frame(content) ‖ frame(part)… )
//! name     = stem ‖ "." ‖ hex(digest[0..4]) ‖ suffix ‖ "." ‖ extension
//! ```
//!
//! This follows `uf_stylex::class`, and for the same reasons written out there:
//! SHA-256 rather than a `Hasher` whose output is not stable across Rust
//! releases, a versioned domain string so a change to what is hashed is a
//! greppable bump rather than a silent renaming of every file in every project,
//! and length-framed components because an image path or a font family is
//! untrusted text that can contain any byte including whatever separator would
//! otherwise have been used.
//!
//! Hexadecimal rather than the base-36 `uf_stylex` uses, and eight characters
//! rather than thirteen: this name goes in a URL and in a directory listing
//! where a person reads it, `[hash]` in every other bundler is hex, and 32 bits
//! is the width Vite and Rollup have both settled on. The collision that
//! matters is between two *different* assets in one build, and a build with a
//! thousand of them has about a one-in-ten-thousand chance of any collision —
//! at which point the second write wins and both references point at one of the
//! two files, which is why the digest covers the parameters and not just the
//! stem.

use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

/// Domain string for an emitted asset's name.
const ASSET_DOMAIN: &str = "uf-assets/name/v1";

/// How many hexadecimal characters an emitted name carries.
pub const NAME_DIGITS: usize = 8;

/// The name for one emitted file.
///
/// `parts` are everything besides the source bytes that decided the content:
/// the width it was resized to, the quality it was encoded at, the family a
/// font was declared under. Two assets that differ in any of them get different
/// names, which is the property that makes the name safe to cache forever.
///
/// `suffix` is appended after the digest and before the extension, unhashed, so
/// a directory listing is readable — `hero.1a2b3c4d.640w.webp` rather than four
/// files distinguishable only by their digests.
#[must_use]
pub fn hashed_name_with_suffix(
    stem: &str,
    content: &[u8],
    parts: &[&[u8]],
    suffix: &str,
    extension: &str,
) -> String {
    let mut hasher = Sha256::new();
    frame(&mut hasher, ASSET_DOMAIN.as_bytes());
    frame(&mut hasher, content);
    for part in parts {
        frame(&mut hasher, part);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(NAME_DIGITS);
    for byte in &digest[..NAME_DIGITS / 2] {
        hex.push_str(&format!("{byte:02x}"));
    }
    format!("{}.{hex}{suffix}.{extension}", sanitise(stem))
}

/// The name for one emitted file, with nothing between the digest and the
/// extension.
#[must_use]
pub fn hashed_name(stem: &str, content: &[u8], parts: &[&[u8]], extension: &str) -> String {
    hashed_name_with_suffix(stem, content, parts, "", extension)
}

/// A file stem that is safe as a path segment and as a URL segment.
///
/// The stem comes from a source path, which a dependency chooses, so it is
/// reduced to a conservative set rather than escaped: anything outside it
/// becomes `-`. A stem that reduces to nothing becomes `asset`, because a name
/// beginning with `.` is a hidden file that most static servers refuse to
/// serve and that no reader would think to look for.
fn sanitise(stem: &str) -> String {
    let mut out: String = stem
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '-',
        })
        .collect();
    // Long stems are the emitting project's own doing, but the name has to fit
    // a filesystem's limit with the digest, the suffix and the extension still
    // on it. 64 leaves room on every platform uf targets.
    out.truncate(64);
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        String::from("asset")
    } else {
        trimmed.to_owned()
    }
}

/// Feed one length-framed component into the digest.
fn frame(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u32).to_le_bytes());
    hasher.update(bytes);
}
