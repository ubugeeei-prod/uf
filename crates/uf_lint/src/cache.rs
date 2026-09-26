//! What the React tree rules worked out about a module, kept between runs.
//!
//! The `react-compiler/*` rules and `react/no-redundant-memo` are the only
//! rules in this crate that cost more than a line scan: each module they read
//! is lowered, rebuilt as a Babel tree, scope-analysed and handed to the
//! official React Compiler — about 27 ms of CPU per module on
//! `npm/ui`. `uf_transform::lint::cached` already remembered the
//! compiler's answer inside one process, and that was all: every `uf lint` and
//! every `uf check` did the whole thing again for every module, over files
//! nobody had touched, and on a warm `uf check` — whose type check is answered
//! from `.uf/cache/check` — it was nine tenths of the run.
//! ubugeeei-prod/uf#1442.
//!
//! # The key
//!
//! Built the way `uf_check::cache` and the host's transform cache build
//! theirs. What the tree rules compute for a module is a function of:
//!
//! * the **identity of the `uf` doing the work** — the largest input of all.
//!   A key without it serves a rebuilt `uf` what the previous build decided,
//!   and `uf_check::cache` has the whole argument. A process that cannot name
//!   its own binary gets no cache rather than one keyed by the rest;
//! * the module's **path**, which decides the compiler's options (a dependency
//!   is never compiled) and is where every finding is reported;
//! * its **text**, which is the tree;
//! * the **question**: which of the two analyses were asked for, the one
//!   compiler validation a project switches on through a rule, and the mode uf
//!   compiles in. `super::runner::react_tree` spells it, from the same values
//!   the analysis reads, so a switch added there cannot be left out here.
//!
//! What a project's rule *levels* do to a finding is not in the key, because
//! it is not in the answer: a record holds every raw finding, and each is
//! filed under its rule — or dropped, because the rule is off — after it is
//! read back, exactly as an answer that was just computed is. So switching a
//! `react-compiler/*` rule from `warn` to `error` reuses a record, and
//! switching on `react-compiler/exhaustive-effect-dependencies`, which changes
//! what the compiler is asked, does not.
//!
//! # What is on disk
//!
//! One small JSON document per module under `.uf/cache/lint/`, which the
//! project's `.uf/` ignore already covers. It is read defensively, as
//! `.uf/cache/check`'s records are: bounded in bytes, required
//! to declare [`RECORD_VERSION`] and to name the path it was filed for.
//! Anything that fails that, or does not parse, is a miss, which costs one
//! module's analysis and cannot be wrong. It is written beside the entry and
//! renamed onto it, so a concurrent run never reads half a record, and the
//! directory is bounded by the policy every cache under `.uf/cache/` shares:
//! [`uf_infra::cache`].

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// Format version this module reads and writes.
///
/// Bumped when the document changes shape. Not when the analysis changes: that
/// is the binary identity in the key, which nobody has to remember.
const RECORD_VERSION: u32 = 1;

/// Largest record this module will read, in bytes.
///
/// A module's findings are a handful of short lines; a megabyte is thousands of
/// them, and past it the document is not one this cache wrote.
const MAX_RECORD_BYTES: u64 = 1024 * 1024;

/// Where a lint keeps what the tree rules already worked out.
///
/// Opening one touches no disk beyond naming the running binary: the directory
/// is made by the first write, so a project only ever linted from a read-only
/// checkout never grows one.
#[derive(Debug, Clone)]
pub struct LintCache {
    directory: PathBuf,
    /// The binary this process is running, read once, at open.
    identity: String,
}

impl LintCache {
    /// The cache for the project rooted at `project_root`, or [`None`] when
    /// this process cannot say which `uf` it is.
    #[must_use]
    pub fn open(project_root: &Path) -> Option<Self> {
        Some(Self::for_identity(
            project_root,
            &uf_infra::cache::binary_identity()?,
        ))
    }

    /// A cache keyed by an identity that is given rather than read.
    ///
    /// For tests: proving that a rebuilt `uf` is not served what the previous
    /// one decided needs two identities in one process, and a process has one
    /// binary.
    #[must_use]
    pub fn for_identity(project_root: &Path, identity: &str) -> Self {
        Self {
            directory: project_root.join(".uf").join("cache").join("lint"),
            identity: identity.to_owned(),
        }
    }

    /// Bring the directory back under [`uf_infra::cache`]'s bound, coldest
    /// entries first.
    ///
    /// Separate from [`LintCache::open`] for `uf_check::cache`'s reason: a
    /// value being constructed is no reason to remove files. A command that is
    /// about to add to the cache asks for this once.
    pub fn sweep(&self) {
        uf_infra::cache::sweep(&self.directory, uf_infra::cache::CacheBound::default());
    }

    /// The answer filed for `question` about `path` with this exact `source`,
    /// if a readable one is there.
    pub(crate) fn read<T: DeserializeOwned>(
        &self,
        path: &str,
        source: &str,
        question: &str,
    ) -> Option<T> {
        let entry = self.entry(path, source, question);
        // Read through the handle the size was taken from, so a file that grows
        // between the two cannot be read past the bound.
        let file = fs::File::open(entry).ok()?;
        if file.metadata().ok()?.len() > MAX_RECORD_BYTES {
            return None;
        }
        let record: Record<String, T> =
            serde_json::from_reader(std::io::BufReader::new(file)).ok()?;
        (record.version == RECORD_VERSION && record.path == path).then_some(record.answer)
    }

    /// File `answer`, or give up quietly: a cache that cannot be written is a
    /// slower run and not a failed one, which is what lets `uf lint` work in a
    /// read-only checkout at all.
    pub(crate) fn write<T: Serialize>(&self, path: &str, source: &str, question: &str, answer: &T) {
        let record = Record {
            version: RECORD_VERSION,
            path,
            answer,
        };
        let Ok(document) = serde_json::to_vec(&record) else {
            return;
        };
        if fs::create_dir_all(&self.directory).is_err() {
            return;
        }
        // Beside the entry and renamed onto it, within one directory and so
        // within one filesystem: the rename is atomic, and a reader sees the
        // old document, the new one, or none.
        let entry = self.entry(path, source, question);
        let staging = entry.with_extension(uf_infra::into_string(uf_infra::cstr!(
            "{}.tmp",
            std::process::id()
        )));
        if fs::write(&staging, &document).is_err() || fs::rename(&staging, &entry).is_err() {
            let _ = fs::remove_file(&staging);
        }
    }

    /// The file the answer to one question about one text is kept in.
    pub(crate) fn entry(&self, path: &str, source: &str, question: &str) -> PathBuf {
        let mut key = Sha256::new();
        // Every field is followed by a NUL, so that two splits of the same
        // bytes cannot collide: a path ending `a` with a text starting `b` is
        // not a path ending `ab`. The text is last, so a NUL inside it cannot
        // be mistaken for a boundary either.
        for field in [
            "uf-lint/react-tree",
            &RECORD_VERSION.to_string(),
            &self.identity,
            path,
            question,
            source,
        ] {
            key.update(field.as_bytes());
            key.update([0]);
        }
        let digest: [u8; 32] = key.finalize().into();
        let mut name = String::with_capacity(digest.len() * 2 + ".json".len());
        for byte in digest {
            // Lowercase hex, from a table: `char::from_digit` is fallible, and
            // there is no safe name to fall back to.
            const DIGITS: [u8; 16] = *b"0123456789abcdef";
            name.push(char::from(DIGITS[usize::from(byte >> 4)]));
            name.push(char::from(DIGITS[usize::from(byte & 0xf)]));
        }
        name.push_str(".json");
        self.directory.join(name)
    }
}

/// One document under `.uf/cache/lint/`.
#[derive(Serialize, Deserialize)]
struct Record<P, T> {
    /// Format version; a document declaring any other is not read.
    version: u32,
    /// The path the key was built from, restated so that a hash collision or a
    /// hand-edited file cannot answer for another module.
    path: P,
    answer: T,
}
