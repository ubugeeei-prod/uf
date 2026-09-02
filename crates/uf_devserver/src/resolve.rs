//! One canonicalization, one decision, one open.
//!
//! # Threat model
//!
//! [CVE-2025-30208], [CVE-2025-31125], [CVE-2025-32395] and [CVE-2025-62522]
//! are four bugs with one cause: **the access decision ran against a string
//! that was not the path that was eventually opened.** Each fix closed one
//! spelling of the gap — a query suffix, an extra decode round, a repaired
//! target, a Windows-only separator — and the next spelling arrived a few weeks
//! later.
//!
//! This module removes the gap instead of the spellings. A request path goes
//! through exactly one pipeline, in exactly this order, and there is no other
//! way into the filesystem in this crate:
//!
//! 1. **Percent-decode once.** Never in a loop, never "until stable". A decoded
//!    result that still contains `%` followed by two hex digits is rejected as
//!    a double-encoding attempt rather than decoded again (`/%252e%252e/.env`).
//! 2. **Reject what decoding revealed.** A NUL byte or any other control
//!    character in the decoded path is refused, not stripped — `%00` truncation
//!    is how `/.env%00.js` gets a suffix check to see `.js` and an `open(2)` to
//!    see `.env`.
//! 3. **Normalize lexically.** `/` and `\` are both separators, on every
//!    platform, because CVE-2025-62522 was a `\` that only one platform's
//!    normalizer recognized. `.` is dropped, `..` pops, and a `..` with nothing
//!    to pop is an error rather than a clamp — clamping is a repair, and
//!    repairs are what this crate refuses to do.
//! 4. **Resolve symlinks.** [`std::fs::canonicalize`] produces the real path.
//! 5. **Decide on the real path.** [`FsPolicy::decide`] sees the canonical path
//!    and nothing else.
//! 6. **Open that exact path**, and hand the caller the open handle.
//!
//! # Why the caller cannot re-derive a path
//!
//! [`ResolvedFile`] owns the [`File`] that was opened from the path the policy
//! approved, and its bytes are reachable only through
//! [`ResolvedFile::read`], which consumes it. The approved path is exposed
//! only as a [`CheckedPath`], which implements [`fmt::Display`] and nothing
//! else: no `Deref`, no `AsRef<Path>`, no accessor handing back a `Utf8Path`.
//! A caller therefore has no path-typed value to open, join, or re-check, so
//! "checked one path, opened another" is not an available mistake at the call
//! site — it is missing from the type.
//!
//! [CVE-2025-30208]: https://github.com/advisories/GHSA-x574-m823-4x7w
//! [CVE-2025-31125]: https://nvd.nist.gov/vuln/detail/CVE-2025-31125
//! [CVE-2025-32395]: https://nvd.nist.gov/vuln/detail/CVE-2025-32395
//! [CVE-2025-62522]: https://nvd.nist.gov/vuln/detail/CVE-2025-62522

use std::fmt;
use std::fs::File;
use std::io::{ErrorKind, Read};

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use smallvec::SmallVec;
use thiserror::Error;

use crate::media::MediaType;
use crate::policy::{FsPolicy, PolicyDenial};
use crate::target::{Loader, RequestTarget, TargetError};

#[cfg(test)]
mod tests;

/// Largest file the dev server will read into memory.
///
/// Rule 4 of the threat model: no unbounded allocation. A dev server that
/// `read_to_end`s whatever the client names is a one-request OOM.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Most path segments a request may contain after normalization.
pub const MAX_PATH_SEGMENTS: usize = 64;

/// The file served when a request names a directory root.
pub const DIRECTORY_INDEX: &str = "index.html";

/// The first path segment Vite reserves for "serve this absolute filesystem
/// path", and the entry point every dev server bypass in the threat model went
/// through. `uf` has no such escape hatch, and says so with a typed refusal
/// rather than by happening not to implement it.
pub const FILESYSTEM_PREFIX: &str = "@fs";

/// A canonical path an access decision was made about.
///
/// Intentionally *not* a path type. It exists so denials and logs can name the
/// file that was checked without handing anyone a value they could open. See
/// the module docs for why that matters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedPath(Utf8PathBuf);

impl fmt::Display for CheckedPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

/// A file that passed the whole pipeline, with the handle that was opened.
///
/// The only way to obtain one is [`resolve_request`] or
/// [`resolve_with_policy`], so possession of a `ResolvedFile` *is* the proof
/// that the access decision ran, on this path, before this handle existed.
#[derive(Debug)]
pub struct ResolvedFile {
    file: File,
    loader: Loader,
    media_type: MediaType,
    len: u64,
    path: CheckedPath,
}

impl ResolvedFile {
    /// The loader the request selected.
    pub fn loader(&self) -> Loader {
        self.loader
    }

    /// The media type, taken from the canonical path's extension.
    pub fn media_type(&self) -> MediaType {
        self.media_type
    }

    /// The size of the opened file, in bytes, as reported by the handle.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the opened file is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The canonical path the policy approved, for logging and diagnostics.
    pub fn checked_path(&self) -> &CheckedPath {
        &self.path
    }

    /// Read the whole file from the handle that was opened during resolution.
    ///
    /// Consumes the value: after this there is no handle left to re-read and no
    /// path to re-open.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if the read fails.
    pub fn read(mut self) -> std::io::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(self.len as usize);
        self.file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// Why a request was refused.
///
/// Every variant is a refusal. There is deliberately no variant that means
/// "accepted after repair".
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AccessDenied {
    /// The request target is not valid origin-form.
    #[error("invalid request target: {0}")]
    InvalidTarget(#[from] TargetError),
    /// A `%` was not followed by two hexadecimal digits.
    #[error("invalid percent-encoding at byte {index}")]
    InvalidPercentEncoding {
        /// Where the bad escape starts.
        index: usize,
    },
    /// Decoding once produced something that is still percent-encoded.
    #[error("request path is percent-encoded more than once")]
    DoubleEncoded,
    /// The decoded path is not valid UTF-8.
    #[error("request path is not valid UTF-8 once decoded")]
    NonUtf8,
    /// The decoded path contains a control character or NUL.
    #[error("decoded request path contains a forbidden byte {byte:#04x}")]
    ForbiddenByte {
        /// The offending byte.
        byte: u8,
    },
    /// A `..` segment climbed above the project root.
    #[error("request path climbs above the project root")]
    Escape,
    /// The request used the `/@fs/` absolute-path prefix.
    #[error("the /{FILESYSTEM_PREFIX}/ prefix is not served")]
    FilesystemPrefix,
    /// The path has more segments than [`MAX_PATH_SEGMENTS`].
    #[error("request path has more than {MAX_PATH_SEGMENTS} segments")]
    TooDeep,
    /// The policy refused the canonical path.
    #[error(transparent)]
    Denied(#[from] PolicyDenial),
    /// Nothing exists at the resolved path.
    #[error("no file at the requested path")]
    NotFound,
    /// The resolved path is a directory, device, socket, or fifo.
    #[error("{path} is not a regular file")]
    NotARegularFile {
        /// The canonical path that was checked.
        path: CheckedPath,
    },
    /// The file is larger than [`MAX_FILE_BYTES`].
    #[error("{path} is {len} bytes, over the {MAX_FILE_BYTES} byte limit")]
    TooLarge {
        /// The canonical path that was checked.
        path: CheckedPath,
        /// The size that was reported.
        len: u64,
    },
    /// The filesystem refused the operation.
    #[error("failed to resolve the requested path: {message}")]
    Io {
        /// The underlying failure.
        message: CompactString,
    },
}

/// Resolve a raw request target against `root` with the default policy.
///
/// This is the crate's narrow waist: it takes the target exactly as it arrived
/// on the wire, so validation cannot be skipped by a caller that already has a
/// "clean" string.
///
/// # Errors
///
/// Returns [`AccessDenied`] for every rejection, from a malformed target to a
/// deny-list match.
pub fn resolve_request(root: &Utf8Path, target: &str) -> Result<ResolvedFile, AccessDenied> {
    let policy = FsPolicy::with_defaults(root).map_err(|error| AccessDenied::Io {
        message: CompactString::new(error.to_string()),
    })?;
    let parsed = RequestTarget::parse(target)?;
    resolve_with_policy(&policy, &parsed)
}

/// Resolve a validated target against a prepared policy.
///
/// # Errors
///
/// Returns [`AccessDenied`] for every rejection.
pub fn resolve_with_policy(
    policy: &FsPolicy,
    target: &RequestTarget<'_>,
) -> Result<ResolvedFile, AccessDenied> {
    let decoded = percent_decode_once(target.path())?;
    let relative = normalize(&decoded)?;

    // Monotone pre-pass: a denied *name* answers the same way whether or not it
    // exists, so the deny list never doubles as an existence oracle. This can
    // only add denials; the authoritative decision below still runs on the
    // canonical path.
    if let Some(pattern) = policy.deny_pattern_for(&relative) {
        return Err(AccessDenied::Denied(PolicyDenial::DeniedByPattern {
            path: relative.clone(),
            pattern: CompactString::new(pattern),
        }));
    }

    let project_root = policy
        .roots()
        .first()
        .expect("a policy always has a project root");
    let candidate = project_root.join(&relative);
    let canonical = canonicalize(&candidate)?;

    policy.decide(&canonical)?;

    let media_type = MediaType::for_extension(canonical.extension());
    let path = CheckedPath(canonical);
    let file = File::open(&path.0).map_err(|error| map_io(&error))?;
    // Metadata comes from the handle, not from the path: it describes the object
    // this `ResolvedFile` will actually read.
    let metadata = file.metadata().map_err(|error| map_io(&error))?;
    if !metadata.is_file() {
        return Err(AccessDenied::NotARegularFile { path });
    }
    let len = metadata.len();
    if len > MAX_FILE_BYTES {
        return Err(AccessDenied::TooLarge { path, len });
    }

    Ok(ResolvedFile {
        file,
        loader: target.loader(),
        media_type,
        len,
        path,
    })
}

/// Percent-decode `encoded` exactly once.
///
/// # Errors
///
/// Rejects a malformed escape, a result that is still encoded, a result that is
/// not UTF-8, and any control byte the decode revealed.
fn percent_decode_once(encoded: &str) -> Result<String, AccessDenied> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(value) = hex_pair(bytes, index + 1) else {
                return Err(AccessDenied::InvalidPercentEncoding { index });
            };
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }

    let decoded = String::from_utf8(out).map_err(|_| AccessDenied::NonUtf8)?;

    // A second layer of encoding means the sender is trying to make two stages
    // disagree about what the path is. There is no legitimate request that
    // needs it, so this rejects rather than decodes again. The cost is that a
    // file genuinely named `50%AB.js` is unreachable; that is the right trade.
    let raw = decoded.as_bytes();
    for index in 0..raw.len() {
        if raw[index] == b'%' && hex_pair(raw, index + 1).is_some() {
            return Err(AccessDenied::DoubleEncoded);
        }
    }
    for &byte in raw {
        if byte < 0x20 || byte == 0x7f {
            return Err(AccessDenied::ForbiddenByte { byte });
        }
    }

    Ok(decoded)
}

fn hex_pair(bytes: &[u8], index: usize) -> Option<u8> {
    let high = hex_digit(*bytes.get(index)?)?;
    let low = hex_digit(*bytes.get(index + 1)?)?;
    Some(high << 4 | low)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Normalize a decoded path into a relative path with no `.` or `..` left.
///
/// `\` is a separator here on every platform. A file whose name genuinely
/// contains a backslash is therefore unreachable through the dev server, which
/// is the deliberate trade CVE-2025-62522 argues for: one separator rule
/// everywhere beats a rule that changes with the host OS.
fn normalize(decoded: &str) -> Result<Utf8PathBuf, AccessDenied> {
    let mut segments: SmallVec<[&str; 16]> = SmallVec::new();
    for segment in decoded.split(['/', '\\']) {
        match segment {
            "" | "." => continue,
            ".." => {
                if segments.pop().is_none() {
                    return Err(AccessDenied::Escape);
                }
            }
            other => {
                if segments.len() == MAX_PATH_SEGMENTS {
                    return Err(AccessDenied::TooDeep);
                }
                segments.push(other);
            }
        }
    }
    // Checked after normalization rather than on the raw string, so `/./@fs/…`
    // and `/x/../@fs/…` cannot spell their way past it.
    if segments.first() == Some(&FILESYSTEM_PREFIX) {
        return Err(AccessDenied::FilesystemPrefix);
    }
    if segments.is_empty() {
        return Ok(Utf8PathBuf::from(DIRECTORY_INDEX));
    }
    Ok(Utf8PathBuf::from(segments.join("/")))
}

fn canonicalize(candidate: &Utf8Path) -> Result<Utf8PathBuf, AccessDenied> {
    let resolved = std::fs::canonicalize(candidate).map_err(|error| map_io(&error))?;
    Utf8PathBuf::from_path_buf(resolved).map_err(|_| AccessDenied::Io {
        message: CompactString::const_new("resolved path is not valid UTF-8"),
    })
}

fn map_io(error: &std::io::Error) -> AccessDenied {
    match error.kind() {
        // "Missing", "a component is not a directory", and "you may not look"
        // all answer identically. Distinguishing them turns the resolver into
        // an oracle for the shape of the filesystem above the project root.
        ErrorKind::NotFound | ErrorKind::PermissionDenied | ErrorKind::NotADirectory => {
            AccessDenied::NotFound
        }
        _ => AccessDenied::Io {
            message: CompactString::new(error.to_string()),
        },
    }
}
