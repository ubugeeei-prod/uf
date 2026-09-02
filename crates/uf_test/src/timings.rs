//! How long each file took last time, remembered across runs.
//!
//! The scheduler wants one number per file. That number lives in
//! `.uf/test-timings.json`, which is a file on disk that anything can write:
//! a `postinstall` script, a bad merge, a checked-in artefact from another
//! machine. It is therefore parsed defensively and never trusted:
//!
//! * the document must declare the version this crate understands;
//! * the whole document is bounded in bytes and in entry count;
//! * every key must be a project-relative path ([`crate::path::is_safe_relative`]),
//!   so a recorded name can never become a read of `../../../../etc/passwd`;
//! * every value must be a non-negative integer inside [`MAX_TIMING_MICROS`].
//!
//! An entry that fails any of those is dropped and counted in
//! [`TimingsAudit::rejected`]; the file falls back to the cold heuristic. A
//! document that fails as a whole is an error, and the run schedules cold. In
//! neither case does a hostile number reach the scheduler.

use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uf_infra::FxHashMap;

use crate::path::is_safe_relative;

/// Directory `uf` keeps its per-project caches in.
pub const CACHE_DIRECTORY: &str = ".uf";

/// File name recorded durations are kept under, inside [`CACHE_DIRECTORY`].
pub const TIMINGS_FILE_NAME: &str = "test-timings.json";

/// Format version this crate reads and writes.
pub const TIMINGS_VERSION: u32 = 1;

/// Most files a timings document may describe.
pub const MAX_TIMING_ENTRIES: usize = 100_000;

/// Largest duration that will be believed, in microseconds (one day).
///
/// A file that "took" a year is a corrupt record, and believing it would pin
/// that file to the front of every future schedule.
pub const MAX_TIMING_MICROS: u64 = 24 * 60 * 60 * 1_000_000;

/// Largest timings document that will be read, in bytes.
pub const MAX_TIMINGS_BYTES: u64 = 8 * 1024 * 1024;

/// Anything that stops a timings document from being read or written.
#[derive(Debug, Error)]
pub enum TimingsError {
    /// The file could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// Path that was being read.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// Path that was being written.
        path: Utf8PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file is larger than [`MAX_TIMINGS_BYTES`].
    #[error("{path} is {bytes} bytes, past the {limit} byte limit for recorded timings")]
    TooLarge {
        /// Path that was being read.
        path: Utf8PathBuf,
        /// The file's size in bytes.
        bytes: u64,
        /// The accepted limit.
        limit: u64,
    },
    /// The document is not the JSON shape this crate writes.
    #[error("{path} is not a valid timings document: {source}")]
    Malformed {
        /// Path that was being read.
        path: Utf8PathBuf,
        /// Underlying deserialization error.
        #[source]
        source: serde_json::Error,
    },
    /// The document declares a version this crate does not understand.
    #[error("{path} declares timings format version {found}, but only {expected} is understood")]
    UnsupportedVersion {
        /// Path that was being read.
        path: Utf8PathBuf,
        /// Version found in the document.
        found: u32,
        /// Version this crate understands.
        expected: u32,
    },
    /// The document describes more files than [`MAX_TIMING_ENTRIES`].
    #[error("{path} describes {found} files, past the {limit} entry limit")]
    TooManyEntries {
        /// Path that was being read.
        path: Utf8PathBuf,
        /// Entry count found in the document.
        found: usize,
        /// The accepted limit.
        limit: usize,
    },
}

/// What validation threw away while reading a document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingsAudit {
    /// Entries dropped because the key was not a safe project-relative path.
    pub rejected_paths: usize,
    /// Entries dropped because the value was not a believable duration.
    pub rejected_durations: usize,
}

impl TimingsAudit {
    /// Total entries dropped.
    pub fn rejected(self) -> usize {
        self.rejected_paths + self.rejected_durations
    }

    /// Whether the document was accepted whole.
    pub fn is_clean(self) -> bool {
        self.rejected() == 0
    }
}

/// Recorded per-file durations, in microseconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestTimings {
    entries: FxHashMap<CompactString, u64>,
}

impl TestTimings {
    /// An empty record, which schedules every file cold.
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded duration for `file`, in microseconds.
    pub fn get(&self, file: &str) -> Option<u64> {
        self.entries.get(file).copied()
    }

    /// Record how long `file` took, clamped to [`MAX_TIMING_MICROS`].
    ///
    /// A new file is not recorded once the document is full, so a runaway
    /// project cannot grow the cache without bound; an existing entry is always
    /// updated, so a full document still tracks the suite it describes.
    pub fn record(&mut self, file: &str, micros: u64) {
        let micros = micros.min(MAX_TIMING_MICROS);
        if let Some(slot) = self.entries.get_mut(file) {
            *slot = micros;
            return;
        }
        if !is_safe_relative(file) || self.entries.len() >= MAX_TIMING_ENTRIES {
            return;
        }
        self.entries.insert(CompactString::from(file), micros);
    }

    /// Drop every entry `keep` rejects, so deleted files do not live forever.
    pub fn retain_files(&mut self, keep: impl Fn(&str) -> bool) {
        self.entries.retain(|file, _| keep(file.as_str()));
    }

    /// How many files are described.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is described.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Parse and validate a timings document.
    pub fn from_json(path: &Utf8Path, text: &str) -> Result<(Self, TimingsAudit), TimingsError> {
        let document: TimingsDocument =
            serde_json::from_str(text).map_err(|source| TimingsError::Malformed {
                path: path.to_path_buf(),
                source,
            })?;

        if document.version != TIMINGS_VERSION {
            return Err(TimingsError::UnsupportedVersion {
                path: path.to_path_buf(),
                found: document.version,
                expected: TIMINGS_VERSION,
            });
        }
        if document.files.len() > MAX_TIMING_ENTRIES {
            return Err(TimingsError::TooManyEntries {
                path: path.to_path_buf(),
                found: document.files.len(),
                limit: MAX_TIMING_ENTRIES,
            });
        }

        let mut timings = Self::new();
        let mut audit = TimingsAudit::default();
        for (file, value) in document.files {
            if !is_safe_relative(&file) {
                audit.rejected_paths += 1;
                continue;
            }
            match believable_micros(&value) {
                Some(micros) => {
                    timings.entries.insert(CompactString::from(file), micros);
                }
                None => audit.rejected_durations += 1,
            }
        }

        Ok((timings, audit))
    }

    /// Serialize to the document shape, with keys in sorted order so two runs
    /// of one suite write byte-identical files.
    pub fn to_json(&self) -> String {
        let mut files: Vec<(&CompactString, &u64)> = self.entries.iter().collect();
        files.sort_unstable_by(|a, b| a.0.cmp(b.0));

        let mut out = String::with_capacity(32 + files.len() * 48);
        out.push_str("{\n  \"version\": ");
        out.push_str(&TIMINGS_VERSION.to_string());
        out.push_str(",\n  \"files\": {");
        for (index, (file, micros)) in files.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("\n    ");
            push_json_string(&mut out, file);
            out.push_str(": ");
            out.push_str(&micros.to_string());
        }
        if !files.is_empty() {
            out.push_str("\n  ");
        }
        out.push_str("}\n}\n");
        out
    }
}

/// Append `value` as a JSON string literal.
///
/// A path is validated before it reaches the map, so it holds no control
/// characters, but escaping is done properly rather than assumed.
fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// Accept only a non-negative integral duration inside the believable range.
///
/// `serde_json` would happily hand back `-1`, `1e400` or `1.5`; none of those is
/// a measurement this crate produced.
fn believable_micros(value: &serde_json::Value) -> Option<u64> {
    let micros = value.as_u64()?;
    (micros <= MAX_TIMING_MICROS).then_some(micros)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimingsDocument {
    version: u32,
    #[serde(default)]
    files: serde_json::Map<String, serde_json::Value>,
}

/// Where recorded timings live for a project rooted at `root`.
pub fn timings_path(root: &Utf8Path) -> Utf8PathBuf {
    root.join(CACHE_DIRECTORY).join(TIMINGS_FILE_NAME)
}

/// Read recorded timings, if any exist.
///
/// A missing file is not an error: it is a cold run.
pub fn load_timings(root: &Utf8Path) -> Result<(TestTimings, TimingsAudit), TimingsError> {
    let path = timings_path(root);
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((TestTimings::new(), TimingsAudit::default()));
        }
        Err(source) => return Err(TimingsError::Read { path, source }),
    };
    if metadata.len() > MAX_TIMINGS_BYTES {
        return Err(TimingsError::TooLarge {
            path,
            bytes: metadata.len(),
            limit: MAX_TIMINGS_BYTES,
        });
    }

    let text = fs::read_to_string(&path).map_err(|source| TimingsError::Read {
        path: path.clone(),
        source,
    })?;
    TestTimings::from_json(&path, &text)
}

/// Write recorded timings, creating `.uf/` if it does not exist.
///
/// Written to a sibling temporary file and renamed, so a run interrupted
/// mid-write leaves the previous document intact rather than a truncated one
/// that the next run has to reject.
pub fn save_timings(root: &Utf8Path, timings: &TestTimings) -> Result<(), TimingsError> {
    let path = timings_path(root);
    let directory = root.join(CACHE_DIRECTORY);
    fs::create_dir_all(&directory).map_err(|source| TimingsError::Write {
        path: directory.clone(),
        source,
    })?;

    let temporary = directory.join(format!("{TIMINGS_FILE_NAME}.tmp"));
    fs::write(&temporary, timings.to_json()).map_err(|source| TimingsError::Write {
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, &path).map_err(|source| TimingsError::Write {
        path: path.clone(),
        source,
    })
}
