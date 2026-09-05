//! Fetching an archive, checking it, and unpacking it.
//!
//! # Why this shells out
//!
//! `curl`, `tar` and `unzip` are already the installer's requirements —
//! `infra/cloudflare/setup-assets/install.sh` refuses to run without the
//! first two — so a machine that can install uf can already do this. Linking
//! an HTTP client and a tar implementation into every `uf` binary to repeat
//! what the operating system ships would grow the download for nothing.
//!
//! The one thing that is *not* delegated is the digest. `shasum` differs
//! between platforms, its absence is silent, and "the checksum tool was
//! missing so we skipped the check" is exactly the failure this exists to
//! prevent. So the hash is computed here, in Rust, over the bytes on disk.
//!
//! # Why the digest is checked before the unpack
//!
//! An archive that fails its check is never handed to `tar`. Path traversal,
//! symlink escapes and zip bombs are all things a bad archive does *while*
//! being unpacked, so refusing to start is the only place the refusal helps.

use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest as _, Sha256, Sha512};

use crate::EnvError;
use crate::source::{Checksum, Digest, Format, Source};

/// Fetch `source`, check it, and unpack it into `into`.
///
/// `into` must exist and should be a staging directory from
/// [`crate::Store::staging`], because what lands there is only a store entry
/// once it has been adopted.
///
/// # Errors
///
/// When a required program is missing, a download fails, the digest does not
/// match what the publisher says, or the archive will not unpack.
pub fn install(source: &Source, into: &Utf8Path) -> Result<(), EnvError> {
    let unpacker = match source.format {
        Format::TarGz => "tar",
        Format::Zip => "unzip",
    };
    for program in ["curl", unpacker] {
        require(program)?;
    }

    let archive = into.join(match source.format {
        Format::TarGz => ".archive.tar.gz",
        Format::Zip => ".archive.zip",
    });
    download(&source.archive, &archive)?;

    let expected = expected_digest(source, into)?;
    let actual = digest_of(&archive, &expected)?;
    if actual != expected {
        // The archive is removed: leaving it behind invites a second run
        // that finds a file already there and trusts it.
        let _ = std::fs::remove_file(&archive);
        return Err(EnvError::ChecksumMismatch {
            url: source.archive.clone(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }

    unpack(source, &archive, into)?;
    let _ = std::fs::remove_file(&archive);
    Ok(())
}

/// The digest the publisher says the archive has.
fn expected_digest(source: &Source, scratch: &Utf8Path) -> Result<Digest, EnvError> {
    match &source.checksum {
        Checksum::Sha256File { url, file } => {
            let listing = scratch.join(".checksums");
            download(url, &listing)?;
            let body = std::fs::read_to_string(&listing).map_err(|error| EnvError::Read {
                path: listing.clone(),
                source: error,
            })?;
            let _ = std::fs::remove_file(&listing);
            digest_for(&body, file)
                .map(Digest::Sha256Hex)
                .ok_or_else(|| EnvError::ChecksumMissing {
                    url: url.clone(),
                    file: file.clone(),
                })
        }
        Checksum::NpmIntegrity { url } => {
            let manifest = scratch.join(".manifest.json");
            download(url, &manifest)?;
            let body = std::fs::read_to_string(&manifest).map_err(|error| EnvError::Read {
                path: manifest.clone(),
                source: error,
            })?;
            let _ = std::fs::remove_file(&manifest);
            npm_integrity(&body)
                .map(Digest::Sha512Base64)
                .ok_or_else(|| EnvError::ChecksumMissing {
                    url: url.clone(),
                    file: "dist.integrity".to_owned(),
                })
        }
    }
}

/// The digest for `file` in a `SHASUMS256.txt`-style listing.
///
/// Lines are `<64 hex>  <name>`, two spaces by convention and sometimes one.
/// Matching on the name rather than the position is what keeps this working
/// when a release adds a file.
fn digest_for(listing: &str, file: &str) -> Option<String> {
    listing.lines().find_map(|line| {
        let (digest, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| digest.trim().to_owned())
    })
}

/// npm's `dist.integrity`, without its `sha512-` prefix.
///
/// The registry also publishes `dist.shasum`, which is SHA-1 of the tarball.
/// It is not used: SHA-1 collisions are constructible, and a check that can
/// be forged is a check in name only. A version old enough to have no
/// `integrity` is refused rather than installed on the strength of a SHA-1.
fn npm_integrity(manifest: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(manifest).ok()?;
    let integrity = value.get("dist")?.get("integrity")?.as_str()?;
    integrity.strip_prefix("sha512-").map(str::to_owned)
}

/// Fetch one URL to one file.
fn download(url: &str, to: &Utf8Path) -> Result<(), EnvError> {
    let output = Command::new("curl")
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(to.as_str())
        .arg(url)
        .output()
        .map_err(|source| EnvError::Program {
            program: "curl",
            source,
        })?;
    if output.status.success() {
        return Ok(());
    }
    Err(EnvError::Download {
        url: url.to_owned(),
        detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

/// The digest of a file, in whatever form `like` is in.
///
/// Taking the shape from the expected value rather than hashing twice: there
/// is exactly one algorithm worth computing, and it is the one the publisher
/// used.
fn digest_of(path: &Utf8Path, like: &Digest) -> Result<Digest, EnvError> {
    let bytes = std::fs::read(path).map_err(|source| EnvError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(match like {
        Digest::Sha256Hex(_) => Digest::Sha256Hex(hex(&Sha256::digest(&bytes))),
        Digest::Sha512Base64(_) => Digest::Sha512Base64(base64(&Sha512::digest(&bytes))),
    })
}

/// Lower-case hex.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Standard base64 with padding, which is what npm's `integrity` uses.
///
/// Written out rather than taken as a dependency: it is twenty lines, it has
/// one caller, and the alternative is a crate in the tree for an alphabet.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let bits = u32::from(chunk[0]) << 16
            | u32::from(chunk.get(1).copied().unwrap_or(0)) << 8
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for index in 0..4 {
            // A chunk of one byte carries two characters, a chunk of two
            // carries three; the rest is padding.
            if index <= chunk.len() {
                let shift = 18 - index * 6;
                out.push(char::from(ALPHABET[((bits >> shift) & 0b11_1111) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Unpack `archive` into `into`, dropping `source.strip` leading components.
fn unpack(source: &Source, archive: &Utf8Path, into: &Utf8Path) -> Result<(), EnvError> {
    let status = match source.format {
        Format::TarGz => Command::new("tar")
            .arg("-xzf")
            .arg(archive.as_str())
            .arg("-C")
            .arg(into.as_str())
            .args(["--strip-components", &source.strip.to_string()])
            .output(),
        // `unzip` has no strip, so it unpacks whole and the wrapper is
        // lifted afterwards.
        Format::Zip => Command::new("unzip")
            .args(["-q", "-o"])
            .arg(archive.as_str())
            .arg("-d")
            .arg(into.as_str())
            .output(),
    }
    .map_err(|error| EnvError::Program {
        program: match source.format {
            Format::TarGz => "tar",
            Format::Zip => "unzip",
        },
        source: error,
    })?;
    if !status.status.success() {
        return Err(EnvError::Unpack {
            archive: archive.to_path_buf(),
            detail: String::from_utf8_lossy(&status.stderr).trim().to_owned(),
        });
    }
    if matches!(source.format, Format::Zip) && source.strip > 0 {
        lift_wrapper(into)?;
    }
    Ok(())
}

/// Move the contents of a single wrapping directory up into `into`.
///
/// Only when there is exactly one entry and it is a directory — otherwise
/// the archive did not have a wrapper and lifting would destroy its layout.
fn lift_wrapper(into: &Utf8Path) -> Result<(), EnvError> {
    let mut entries: Vec<Utf8PathBuf> = Vec::new();
    for entry in std::fs::read_dir(into).map_err(|source| EnvError::Read {
        path: into.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| EnvError::Read {
            path: into.to_path_buf(),
            source,
        })?;
        let path =
            Utf8PathBuf::from_path_buf(entry.path()).map_err(|path| EnvError::NotUtf8 { path })?;
        if path.file_name().is_some_and(|name| name.starts_with('.')) {
            continue;
        }
        entries.push(path);
    }
    let [wrapper] = entries.as_slice() else {
        return Ok(());
    };
    if !wrapper.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(wrapper).map_err(|source| EnvError::Read {
        path: wrapper.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| EnvError::Read {
            path: wrapper.clone(),
            source,
        })?;
        let from = entry.path();
        let name = entry.file_name();
        std::fs::rename(&from, into.as_std_path().join(&name)).map_err(|source| {
            EnvError::Write {
                path: into.join(name.to_string_lossy().as_ref()),
                source,
            }
        })?;
    }
    let _ = std::fs::remove_dir(wrapper);
    Ok(())
}

/// Refuse early when a program this needs is not installed.
fn require(program: &'static str) -> Result<(), EnvError> {
    let found = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {program}"))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if found {
        return Ok(());
    }
    Err(EnvError::MissingProgram { program })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_is_found_by_name_not_by_position() {
        let listing = "\
aaaa  node-v24.14.0-linux-x64.tar.gz
bbbb  node-v24.14.0-darwin-arm64.tar.gz
cccc  SHASUMS256.txt
";
        assert_eq!(
            digest_for(listing, "node-v24.14.0-darwin-arm64.tar.gz").as_deref(),
            Some("bbbb")
        );
        assert_eq!(digest_for(listing, "node-v24.14.0-win-x64.zip"), None);
    }

    /// npm's `integrity` is SHA-512 in base64, and comparing it to a
    /// SHA-256 in hex called every download corrupt.
    #[test]
    fn base64_is_the_alphabet_npm_publishes_in() {
        // The empty string's SHA-512, as npm would write it.
        assert_eq!(
            base64(&Sha512::digest(b"")),
            "z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg=="
        );
        // Padding: one, two and three bytes are the three shapes.
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn npm_integrity_is_read_without_its_prefix() {
        let manifest = r#"{"dist":{"integrity":"sha512-abc==","shasum":"deadbeef"}}"#;
        assert_eq!(npm_integrity(manifest).as_deref(), Some("abc=="));
        // A version too old to carry `integrity` is refused rather than
        // installed on the strength of a SHA-1.
        assert_eq!(npm_integrity(r#"{"dist":{"shasum":"deadbeef"}}"#), None);
    }

    /// Some listings mark binary files with a leading `*`.
    #[test]
    fn a_binary_marker_is_not_part_of_the_name() {
        assert_eq!(
            digest_for("dddd *bun-darwin-aarch64.zip\n", "bun-darwin-aarch64.zip").as_deref(),
            Some("dddd")
        );
    }
}
