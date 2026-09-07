//! What a check already worked out, kept between runs.
//!
//! Inference is the expensive half of `uf check` — three seconds against a
//! hundred milliseconds for the linter over the same files — and until this
//! existed every run paid it again, including a run over a checkout nothing
//! had touched. This is the memory that stops that.
//!
//! # The key, and why it is shaped like the transform cache's
//!
//! `@uniflowed/host`'s transform cache (`packages/host/internal/node-hooks.js`)
//! settled the hard half of this key first, in #219: **the compiler's own
//! identity has to be in it**. A content-addressed cache "has no invalidation
//! to get wrong" only if every other input to the computation is a constant,
//! and the largest one is the compiler. Edit `crates/uf_check`, rebuild, and a
//! key made of the source alone serves what the *previous* binary decided —
//! and the only symptom is an answer that makes no sense. So the identity of
//! the `uf` doing the work is in the key here too, and, exactly as there, a
//! process that cannot name its own binary reads nothing and writes nothing:
//! hashing the rest anyway would give every build one key again, and writing
//! under it would leave an entry for the next run to trust. Slower, never
//! wrong.
//!
//! Two things differ, and both follow from checking being linked in rather
//! than spawned.
//!
//! * The identity is [`std::env::current_exe`], read **once** when the cache is
//!   opened, not stat'd per file. The transform cache re-stats because the
//!   binary it is about to *spawn* can be replaced between two modules, so the
//!   compiler that answers the next module may not be the one that answered the
//!   last. The binary this process is *executing* was fixed at `exec`: a
//!   `cargo build` finishing halfway through a check replaces a file this run
//!   is no longer reading, and the answers it is producing still belong to the
//!   build that was read at open. Re-reading before each write would file them
//!   under the *next* build's name, which is the staleness this key exists to
//!   prevent, pointing the other way.
//! * An entry is not content-addressed by its own inputs alone, because a
//!   file's diagnostics are not a function of its own bytes. See below.
//!
//! # The half a transform does not have
//!
//! `uf transform` compiles a file from that file. Inference does not: an error
//! in `app.js` can be a consequence of a type declared in `mode.js`, and the
//! *location* it points at is a position inside `mode.js`. So a record carries
//! two independent parts:
//!
//! * a **key**, over the compiler identity, the limits, and the file's own path
//!   and text — what makes this record be about this file at all; and
//! * a **dependency digest** stored inside it, over the signature of every
//!   module the file reaches and how each of their specifiers resolved. The
//!   diagnostics are believed only while that digest still describes the batch.
//!
//! Splitting them is what makes a one-file edit cheap. The edited file's key
//! changes, so it is re-checked. Every file that reaches it keeps its key and
//! fails on the digest, so it is re-checked too. Everything else matches on
//! both and is read. A digest over *signatures* rather than over dependency
//! source text is what keeps that set small: editing a function body moves no
//! declaration and changes no exported type, so the files importing it are
//! still answered from disk.
//!
//! Getting that wrong is worse than having no cache at all — a stale
//! diagnostic, or a missing one, is a checker that lies — so every input that
//! can change an answer is in one of the two, and anything unrecognised is
//! treated as absent rather than believed.
//!
//! # What is on disk
//!
//! One JSON document per file, under `.uf/cache/check/`, which `.gitignore`
//! already covers. It is a file anything can write — a bad merge, a checked-in
//! artefact from another machine, a truncated write — so it is parsed
//! defensively and never trusted: bounded in bytes, in imports and in
//! diagnostics, required to declare the format version this crate understands,
//! and required to name the path its key was built from. An entry that fails
//! any of that is a miss, which costs one file's inference and cannot be wrong.
//!
//! A source edit orphans an entry and a rebuild of `uf` orphans a whole
//! generation, exactly as `.uf/cache/transform` does — about 330 records and
//! 2 MB per build on this repository, and for a long time nothing ever took
//! one back. [`CheckCache::sweep`] is the policy that bounds that, and it is
//! deliberately the *same* policy the other two caches use: see
//! [`uf_infra::cache`] for the argument, including why "keep only the current
//! compiler identity" is the tempting option and is wrong for the reason
//! `tests::cache::a_rebuilt_uf_is_not_served_what_the_previous_one_decided`
//! exists. See #218.

use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::diagnostic::TypeDiagnostic;

/// Format version this crate reads and writes.
///
/// Bumped when the *document* changes shape. Not when the checker changes:
/// that is the compiler identity in the key, which nobody has to remember.
pub(crate) const RECORD_VERSION: u32 = 1;

/// Largest record this crate will read, in bytes.
const MAX_RECORD_BYTES: u64 = 8 * 1024 * 1024;

/// Most imports one record may describe.
const MAX_RECORD_REQUIRES: usize = 100_000;

/// Most diagnostics one record may describe.
const MAX_RECORD_DIAGNOSTICS: usize = 100_000;

/// A SHA-256 digest.
pub(crate) type Digest = [u8; 32];

/// A digest as lowercase hex.
///
/// A table rather than `char::from_digit`, which is fallible: a digest that
/// spelled one nibble wrong would be two records filed under one name, and
/// there is no value here to fall back to that would be safe.
pub(crate) fn hex(digest: &Digest) -> String {
    const DIGITS: [u8; 16] = *b"0123456789abcdef";
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    out
}

/// A digest over anything upstream already knows how to hash.
///
/// Used for packed module signatures, which are upstream's types: they derive
/// [`Hash`] and nothing else this crate could rely on to compare two of them
/// cheaply, and re-deriving a structural equality here would be a second copy
/// of a data structure that changes every Flow release.
pub(crate) fn digest_of_hashable<T: Hash + ?Sized>(value: &T) -> Digest {
    let mut hasher = DigestHasher(Sha256::new());
    value.hash(&mut hasher);
    hasher.0.finalize().into()
}

/// A [`Hasher`] that keeps every byte instead of folding it to 64 bits.
///
/// [`Hash`] implementations only ever *write* through the hasher, so the
/// truncation `finish` is obliged to return is never observed — and it must
/// not be, because two signatures colliding in 64 bits would be two files the
/// cache confused for one.
struct DigestHasher(Sha256);

impl Hasher for DigestHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(&self) -> u64 {
        // Unreachable through `Hash::hash`, which is the only caller. Reading
        // the digest goes through `digest_of_hashable`.
        0
    }
}

/// A digest builder for the parts of a key this crate writes itself.
///
/// Every field is followed by a NUL so that two different splits of the same
/// bytes cannot produce the same digest — a path ending in `a` with a source
/// starting `b` is not a path ending `ab` with a source starting empty.
pub(crate) struct Fields(Sha256);

impl Fields {
    /// A builder tagged with what the digest is *for*, so that two digests over
    /// the same fields in two different roles cannot collide.
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
    pub(crate) fn finish(self) -> Digest {
        self.0.finalize().into()
    }
}

/// One import a record describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CachedRequire {
    /// The specifier as written.
    pub(crate) specifier: CompactString,
    /// Whether Flow's own library definitions declare this module.
    ///
    /// Recorded rather than recomputed because deciding it needs the merged
    /// builtin environment, and a run that answers every file from disk must
    /// not have to build one to find out it did not need it. It is a property
    /// of the specifier and the compiler, both of which are already in the key
    /// this record is filed under, so a recorded answer cannot go stale
    /// without the key changing first.
    pub(crate) declared: bool,
}

/// What one run worked out about one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    /// Format version; a document declaring any other is not read.
    pub(crate) version: u32,
    /// The path the key was built from, restated so a hash collision or a
    /// hand-edited file cannot silently answer for another file.
    pub(crate) path: CompactString,
    /// This file's packed signature, or [`None`] when it has none — it did not
    /// parse, or it said `@noflow`.
    pub(crate) signature: Option<String>,
    /// Every module specifier the file imports, in sorted order.
    pub(crate) requires: Vec<CachedRequire>,
    /// Whether the file opted out of inference with `@noflow`.
    pub(crate) skipped: bool,
    /// The dependency digest the diagnostics below were computed under.
    pub(crate) dependencies: String,
    /// The diagnostics, exactly as the run that computed them reported them.
    pub(crate) diagnostics: Vec<TypeDiagnostic>,
}

/// Where a check keeps what it has already worked out.
///
/// Opening one does not touch the disk beyond identifying the running binary:
/// the directory is created by the first write, so a project that is only ever
/// checked from a read-only checkout never grows one.
#[derive(Debug, Clone)]
pub struct CheckCache {
    directory: PathBuf,
    /// The binary this process is running, as path, size and modification
    /// time — read once, at open.
    identity: String,
}

impl CheckCache {
    /// Open the cache for the project rooted at `project_root`, or [`None`]
    /// when this process cannot say which `uf` it is.
    ///
    /// [`None`] is a cache that is off in both directions rather than one
    /// keyed by the rest: see this module's header for why a compiler that
    /// cannot be named must not be cached against.
    pub fn open(project_root: &Path) -> Option<Self> {
        Some(Self {
            directory: project_root.join(".uf").join("cache").join("check"),
            identity: binary_identity()?,
        })
    }

    /// A cache keyed by an identity that is given rather than read.
    ///
    /// Only tests build one. Proving that a rebuilt `uf` is not served what the
    /// previous one decided needs two compilers in one process, and a checkout
    /// has one — #219 answered the same problem for the transform cache by
    /// writing two stand-in binaries over one path, which a checker linked into
    /// the process running the test cannot do.
    #[cfg(test)]
    pub(crate) fn for_identity(project_root: &Path, identity: &str) -> Self {
        Self {
            directory: project_root.join(".uf").join("cache").join("check"),
            identity: identity.to_owned(),
        }
    }

    /// The compiler identity every key under this cache carries.
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    /// Bring the directory back under its byte bound, coldest entries first.
    ///
    /// Separate from [`CheckCache::open`] on purpose: opening is what a caller
    /// does to *read*, and it is documented as touching no disk. Removing
    /// files is not something that should happen because somebody constructed
    /// a value, so a caller that is about to add to the cache asks for this in
    /// as many words. `uf check` is that caller and does it once per run,
    /// which is where the cost — one `read_dir` and one `stat` per entry, and
    /// no sort at all while the directory is under the cap — is invisible
    /// beside inference.
    ///
    /// The bound and the eviction order are [`uf_infra::cache`]'s, shared with
    /// `.uf/cache/task` and `.uf/cache/transform` so that three caches with
    /// the same problem do not grow three answers to it.
    pub fn sweep(&self) {
        uf_infra::cache::sweep(&self.directory, uf_infra::cache::CacheBound::default());
    }

    /// The record filed under `key`, if there is a readable one about `path`.
    pub(crate) fn read(&self, key: &Digest, path: &str) -> Option<Record> {
        let entry = self.entry_path(key);
        // Read through the handle the size was taken from, so a file that grows
        // between the two cannot be read past the bound.
        let file = fs::File::open(&entry).ok()?;
        if file.metadata().ok()?.len() > MAX_RECORD_BYTES {
            return None;
        }
        let record: Record = serde_json::from_reader(std::io::BufReader::new(file)).ok()?;
        let sane = record.version == RECORD_VERSION
            && record.path == path
            && record.requires.len() <= MAX_RECORD_REQUIRES
            && record.diagnostics.len() <= MAX_RECORD_DIAGNOSTICS;
        sane.then_some(record)
    }

    /// File `record` under `key`, or give up quietly.
    ///
    /// Tolerant in every direction: a cache that cannot be written is a slower
    /// run and not a failed one, which is what lets `uf check` work in a
    /// read-only checkout at all.
    pub(crate) fn write(&self, key: &Digest, record: &Record) {
        let Ok(document) = serde_json::to_vec(record) else {
            return;
        };
        if fs::create_dir_all(&self.directory).is_err() {
            return;
        }
        // Written beside the entry and renamed onto it, so a reader — this
        // process next time, or another `uf check` running now — never sees
        // half a document. Both are in one directory, so the rename is within
        // one filesystem and is atomic.
        let entry = self.entry_path(key);
        let staging = entry.with_extension(format!("{}.tmp", std::process::id()));
        if fs::write(&staging, &document).is_ok() && fs::rename(&staging, &entry).is_err() {
            let _ = fs::remove_file(&staging);
        }
    }

    fn entry_path(&self, key: &Digest) -> PathBuf {
        self.directory.join(format!("{}.json", hex(key)))
    }
}

/// The `uf` this process is running, as path, size and modification time.
///
/// Size and modification time rather than a version, for #219's reason: every
/// build between two releases shares a version, and it is builds that change
/// what the checker decides. Not a hash of the binary, which is thirty
/// megabytes and would cost more than the check it is meant to save.
fn binary_identity() -> Option<String> {
    let path = std::env::current_exe().ok()?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!(
        "{}\0{}\0{modified}",
        path.display(),
        metadata.len()
    ))
}
