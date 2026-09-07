//! What a task already did, kept between runs.
//!
//! # The key, and where it departs from `uf check`'s
//!
//! `uf_check::cache` and `@uniflowed/host`'s transform cache settled the shape
//! of a key here first: a content digest over everything that can change the
//! answer, one document per key under `.uf/cache/`, written through a
//! temporary file and renamed, parsed defensively, and **never** answered from
//! when one of the inputs cannot be named. This follows all of that. Three
//! things differ, and each is a consequence of a task being a command uf
//! spawns rather than a computation uf performs.
//!
//! * **The compiler identity is not in the key, because uf is not the
//!   compiler.** `uf check` puts [`std::env::current_exe`] in every key because
//!   the binary running the check is the largest input to it. A task's answer
//!   is produced by `cargo`, or `node`, or a shell script — uf contributes the
//!   scheduling and nothing else, so keying on uf's own mtime would invalidate
//!   every task on every `cargo build` for no reason. What *does* matter is
//!   when the task runs `./target/release/uf`, and then the honest way to say
//!   so is to name that path in `inputs`, where it is hashed by content. Every
//!   task in this repository that runs uf does exactly that.
//! * **Nothing is cached by default.** `uf check` can cache every file it sees
//!   because it knows what it read: it *is* the reader. Nothing here knows what
//!   `sh -c "tools/ci/whatever.sh"` opens. So the input set is declared, and a
//!   task that declares none always runs. See
//!   [`uf_config::TaskDefinition::is_cacheable`].
//! * **A record is not answered from unless the declared outputs are still
//!   there, unchanged.** Vite+'s `output` is restored from the cache on a hit;
//!   uf's is verified against it. Restoring means storing every artefact a
//!   task produces, and this repository's `docs:build` produces a website. The
//!   conservative half is the half worth having: a `dist/` somebody deleted
//!   gets rebuilt rather than reported as already done.
//!
//! # What is *not* in the key
//!
//! The ambient process environment. A key over it would have to include
//! `TERM_SESSION_ID` and `SHLVL` and whatever else the shell exports, and no
//! entry would ever be hit twice. The project's `.env` files are in the key,
//! and so is the task's own `env` block — so a task whose answer depends on a
//! variable has a place to say so, and saying so is what puts it in the key.
//! Nothing else about the machine is: not the Node on `PATH`, not the C
//! compiler, not the shell. A task whose answer depends on one of those and
//! cannot name it as a file is a task that should not declare `inputs`.
//!
//! # What is on disk
//!
//! One JSON document per key under `.uf/cache/task/`, which `.gitignore`
//! already covers, plus one note per task under `.uf/cache/task/last/`
//! recording what the previous run keyed on — that note is what lets `--why`
//! answer "because `packages/core/index.js` changed" instead of "because the
//! key was different". Both are files anything can write, so both are read
//! defensively: bounded in bytes, required to declare the version this crate
//! understands, and required to restate the task and key they are filed under.
//!
//! Entries are evicted by [`TaskCache::sweep`], which is the same byte bound
//! and the same least-recently-used order the other two caches use — see
//! [`uf_infra::cache`] for the policy and the argument behind it. #218 asked
//! the question for all three at once, and one answer is the point: three
//! caches with the same shape must not grow three eviction policies a reader
//! has to learn separately.
//!
//! The `last/` notes are outside it. A sweep only looks at files directly in
//! the directory, so they are neither counted nor removed: there is one per
//! task, each rewritten in place, so they are not what grows — and evicting
//! one would take away `--why`'s answer to save a kilobyte.

use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use camino::Utf8Path;
use serde::{Deserialize, Serialize};

use crate::digest::{Digest, hex};
use crate::inputs::InputFile;

/// Format version this crate reads and writes.
pub(crate) const RECORD_VERSION: u32 = 1;

/// Largest document this crate will read, in bytes.
const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;

/// Most captured output one record may hold, in bytes per stream.
///
/// A task that prints more than this is not cached at all rather than cached
/// with its output cut short: a replayed result whose output is missing its
/// last half is a run that looks like it passed and cannot be read.
pub(crate) const MAX_RECORDED_OUTPUT: usize = 8 * 1024 * 1024;

/// What one run of one task produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Record {
    /// Format version; a document declaring any other is not read.
    pub(crate) version: u32,
    /// The task this is about, restated so a hand-edited file or a collision
    /// cannot answer for another task.
    pub(crate) task: String,
    /// The key it is filed under, restated for the same reason.
    pub(crate) key: String,
    /// Everything it wrote to stdout, base64.
    pub(crate) stdout: String,
    /// Everything it wrote to stderr, base64.
    pub(crate) stderr: String,
    /// The files it declared as `outputs`, and what was in them when it
    /// finished.
    pub(crate) outputs: Vec<InputFile>,
    /// How long it took, in microseconds — reported when it is replayed, so
    /// the reader can see what the hit saved.
    pub(crate) duration_micros: u64,
}

impl Record {
    /// The recorded stdout, or empty when the document's base64 is not.
    pub(crate) fn stdout_bytes(&self) -> Vec<u8> {
        BASE64.decode(&self.stdout).unwrap_or_default()
    }

    /// The recorded stderr, likewise.
    pub(crate) fn stderr_bytes(&self) -> Vec<u8> {
        BASE64.decode(&self.stderr).unwrap_or_default()
    }
}

/// What the previous run of a task keyed on.
///
/// Kept separately from the record because it is looked up by *task* rather
/// than by key: on a miss there is by definition no record to read, and the
/// question the reader has is which of the things they changed caused it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LastRun {
    pub(crate) version: u32,
    pub(crate) task: String,
    pub(crate) key: String,
    /// The command as it was, so a changed command can be named as such.
    pub(crate) command: String,
    /// A digest over the environment the task was given.
    pub(crate) environment: String,
    /// The inputs and their digests, in path order.
    pub(crate) inputs: Vec<InputFile>,
    /// Whether [`Self::inputs`] was cut short by [`MAX_REMEMBERED_INPUTS`].
    pub(crate) inputs_truncated: bool,
}

/// How many input files a note remembers.
///
/// Enough to name the file that changed in any project a person reads the
/// output of, and small enough that the note stays a note. Past it the diff
/// says "inputs changed" without naming one, which is what it could honestly
/// say anyway.
pub(crate) const MAX_REMEMBERED_INPUTS: usize = 20_000;

/// What changed between the last run and this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Nothing was remembered about a previous run.
    NeverRun,
    /// The command text is different.
    Command,
    /// The environment uf gives the task is different.
    Environment,
    /// A file that was an input is gone.
    InputRemoved(String),
    /// A file that was not an input is one now.
    InputAdded(String),
    /// An input's contents are different.
    InputChanged(String),
    /// Something changed that the note is too small to name.
    Unnamed,
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeverRun => f.write_str("no previous run to compare against"),
            Self::Command => f.write_str("the command changed"),
            Self::Environment => f.write_str("the environment changed"),
            Self::InputRemoved(path) => write!(f, "{path} is gone"),
            Self::InputAdded(path) => write!(f, "{path} is new"),
            Self::InputChanged(path) => write!(f, "{path} changed"),
            Self::Unnamed => f.write_str("the inputs changed"),
        }
    }
}

impl LastRun {
    /// The first difference between this note and `now`, in the order a reader
    /// would look for them.
    pub(crate) fn diff(&self, now: &LastRun) -> Change {
        if self.command != now.command {
            return Change::Command;
        }
        if self.environment != now.environment {
            return Change::Environment;
        }
        if self.inputs_truncated || now.inputs_truncated {
            return Change::Unnamed;
        }
        let mut before = self.inputs.iter().peekable();
        let mut after = now.inputs.iter().peekable();
        // Both lists are in path order, so one pass over the two finds the
        // first difference without building a map of either.
        loop {
            match (before.peek(), after.peek()) {
                (None, None) => return Change::Unnamed,
                (Some(old), None) => return Change::InputRemoved(old.path.clone()),
                (None, Some(new)) => return Change::InputAdded(new.path.clone()),
                (Some(old), Some(new)) => {
                    if old.path < new.path {
                        return Change::InputRemoved(old.path.clone());
                    }
                    if new.path < old.path {
                        return Change::InputAdded(new.path.clone());
                    }
                    if old.digest != new.digest {
                        return Change::InputChanged(old.path.clone());
                    }
                    before.next();
                    after.next();
                }
            }
        }
    }
}

/// Where a project keeps what its tasks have already done.
#[derive(Debug, Clone)]
pub struct TaskCache {
    directory: PathBuf,
}

impl TaskCache {
    /// The cache for the project rooted at `root`.
    ///
    /// Opening one touches no disk: the directory is created by the first
    /// write, so a project whose tasks all declare nothing never grows one.
    #[must_use]
    pub fn open(root: &Utf8Path) -> Self {
        Self {
            directory: root
                .join(".uf")
                .join("cache")
                .join("task")
                .into_std_path_buf(),
        }
    }

    /// Bring the directory back under its byte bound, coldest entries first.
    ///
    /// Separate from [`TaskCache::open`] because opening is documented as
    /// touching no disk and must stay that way: removing files is not
    /// something that should happen because somebody constructed a value. A
    /// caller about to run tasks asks for this in as many words, once per run,
    /// where one `read_dir` is invisible beside spawning a command.
    ///
    /// Shared with `.uf/cache/check` and `.uf/cache/transform`; the policy is
    /// [`uf_infra::cache`]'s.
    pub fn sweep(&self) {
        uf_infra::cache::sweep(&self.directory, uf_infra::cache::CacheBound::default());
    }

    /// The record filed under `key` for `task`, if there is a readable one.
    pub(crate) fn read(&self, key: &Digest, task: &str) -> Option<Record> {
        let key = hex(key);
        let record: Record = read_document(&self.entry_path(&key))?;
        let sane = record.version == RECORD_VERSION && record.task == task && record.key == key;
        sane.then_some(record)
    }

    /// File `record` under `key`, or give up quietly.
    ///
    /// A cache that cannot be written is a slower run, not a failed one — the
    /// same tolerance `uf check`'s has, and what lets a task run from a
    /// read-only checkout at all.
    pub(crate) fn write(&self, key: &Digest, record: &Record) {
        write_document(&self.entry_path(&hex(key)), record);
    }

    /// What the last run of `task` keyed on, if it was recorded.
    pub(crate) fn read_last(&self, task: &str) -> Option<LastRun> {
        let note: LastRun = read_document(&self.last_path(task))?;
        (note.version == RECORD_VERSION && note.task == task).then_some(note)
    }

    /// Record what this run keyed on.
    pub(crate) fn write_last(&self, note: &LastRun) {
        write_document(&self.last_path(&note.task), note);
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.directory.join(format!("{key}.json"))
    }

    /// A task's note, filed under a digest of its name.
    ///
    /// Hashed rather than used as written: a task may be called `build#docs`
    /// or `a/b`, and neither is a file name. The name is restated inside the
    /// document, which is what the read checks.
    fn last_path(&self, task: &str) -> PathBuf {
        let mut fields = crate::digest::Fields::new("uf task name v1");
        fields.push(task);
        self.directory
            .join("last")
            .join(format!("{}.json", hex(&fields.finish())))
    }
}

/// Read and parse a bounded JSON document, or [`None`].
fn read_document<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    // Through the handle the size was taken from, so a file that grows between
    // the two cannot be read past the bound.
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_RECORD_BYTES {
        return None;
    }
    serde_json::from_reader(std::io::BufReader::new(file)).ok()
}

/// Write a JSON document where a reader never sees half of it.
fn write_document<T: Serialize>(path: &Path, document: &T) {
    let Ok(bytes) = serde_json::to_vec(document) else {
        return;
    };
    let Some(directory) = path.parent() else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    // Written beside the entry and renamed onto it: both are in one directory,
    // so the rename is within one filesystem and is atomic.
    let staging = path.with_extension(format!("{}.tmp", std::process::id()));
    if fs::write(&staging, &bytes).is_ok() && fs::rename(&staging, path).is_err() {
        let _ = fs::remove_file(&staging);
    }
}

/// Encode captured output for a record.
pub(crate) fn encode(bytes: &[u8]) -> String {
    BASE64.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(command: &str, inputs: &[(&str, &str)]) -> LastRun {
        LastRun {
            version: RECORD_VERSION,
            task: String::from("t"),
            key: String::from("k"),
            command: command.to_owned(),
            environment: String::from("e"),
            inputs: inputs
                .iter()
                .map(|(path, digest)| InputFile {
                    path: (*path).to_owned(),
                    digest: (*digest).to_owned(),
                })
                .collect(),
            inputs_truncated: false,
        }
    }

    #[test]
    fn a_changed_input_is_named() {
        let before = note("c", &[("a", "1"), ("b", "2")]);
        let after = note("c", &[("a", "1"), ("b", "3")]);
        assert_eq!(before.diff(&after), Change::InputChanged(String::from("b")));
    }

    #[test]
    fn an_added_and_a_removed_input_are_told_apart() {
        let before = note("c", &[("a", "1")]);
        let after = note("c", &[("a", "1"), ("b", "2")]);
        assert_eq!(before.diff(&after), Change::InputAdded(String::from("b")));
        assert_eq!(after.diff(&before), Change::InputRemoved(String::from("b")));
    }

    #[test]
    fn the_command_is_looked_at_before_the_files() {
        let before = note("one", &[("a", "1")]);
        let after = note("two", &[("a", "2")]);
        assert_eq!(before.diff(&after), Change::Command);
    }

    #[test]
    fn a_truncated_note_says_so_rather_than_guessing() {
        let before = note("c", &[("a", "1")]);
        let mut after = note("c", &[("a", "2")]);
        after.inputs_truncated = true;
        assert_eq!(before.diff(&after), Change::Unnamed);
    }

    #[test]
    fn a_record_filed_under_one_task_does_not_answer_for_another() {
        let dir = tempfile::tempdir().unwrap();
        let cache = TaskCache::open(Utf8Path::from_path(dir.path()).unwrap());
        let key = [7u8; 32];
        cache.write(
            &key,
            &Record {
                version: RECORD_VERSION,
                task: String::from("mine"),
                key: hex(&key),
                stdout: encode(b"hello"),
                stderr: String::new(),
                outputs: Vec::new(),
                duration_micros: 1,
            },
        );
        assert!(cache.read(&key, "mine").is_some());
        assert!(cache.read(&key, "theirs").is_none());
    }

    #[test]
    fn a_document_from_another_version_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = TaskCache::open(Utf8Path::from_path(dir.path()).unwrap());
        let key = [9u8; 32];
        cache.write(
            &key,
            &Record {
                version: RECORD_VERSION + 1,
                task: String::from("mine"),
                key: hex(&key),
                stdout: String::new(),
                stderr: String::new(),
                outputs: Vec::new(),
                duration_micros: 1,
            },
        );
        assert!(cache.read(&key, "mine").is_none());
    }

    #[test]
    fn a_task_name_that_is_not_a_file_name_still_gets_a_note() {
        let dir = tempfile::tempdir().unwrap();
        let cache = TaskCache::open(Utf8Path::from_path(dir.path()).unwrap());
        let mut written = note("c", &[]);
        written.task = String::from("build#docs/thing");
        cache.write_last(&written);
        assert_eq!(cache.read_last("build#docs/thing"), Some(written));
    }
}
