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
//!   `tools/ci/whatever.sh` opens once it is running — [`crate::command`] says
//!   what a task starts, not what it reads. So the input set is declared, and a
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
//! already covers, plus one note per task under `.uf/cache/task/notes/`
//! recording what the previous run keyed on — that note is what lets `--why`
//! answer "because `packages/core/index.js` changed" instead of "because the
//! key was different". Neither holds a value from the task's environment: the
//! note names each variable beside a digest, for the reason
//! [`crate::environment`] gives. Both are files anything can write, so both are read
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
//! The notes are outside it. A sweep only looks at files directly in
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
use crate::environment::Environment;
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
    /// The environment the task was given: names, and a digest of each value
    /// in place of the value. See [`crate::environment`].
    pub(crate) environment: Environment,
    /// The tasks in other packages the key was built on, in plan order.
    ///
    /// Left out of every note a run without a workspace writes, so those notes
    /// are byte for byte what they were before there was a field to leave out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) dependencies: Vec<Upstream>,
    /// The inputs and their digests, in path order.
    pub(crate) inputs: Vec<InputFile>,
    /// Whether [`Self::inputs`] was cut short by [`MAX_REMEMBERED_INPUTS`].
    pub(crate) inputs_truncated: bool,
}

/// A task in another package that a key was built on, and the key that task
/// had at the time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Upstream {
    /// As a reader writes it: `ui#build`.
    pub(crate) task: String,
    pub(crate) key: String,
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
    /// The mode the `.env` files are selected for is different.
    Mode,
    /// The task runs in a different directory.
    Directory,
    /// A variable uf gives the task has a different value.
    VariableChanged(String),
    /// A variable uf gives the task was not given last time.
    VariableAdded(String),
    /// A variable uf gave the task last time is not given now.
    VariableRemoved(String),
    /// A file that was an input is gone.
    InputRemoved(String),
    /// A file that was not an input is one now.
    InputAdded(String),
    /// An input's contents are different.
    InputChanged(String),
    /// A task in another package this one is keyed on is not what it was: its
    /// own key changed, or it was not depended on before. Carries its label.
    Dependency(String),
    /// Something changed that the note is too small to name.
    Unnamed,
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dependency(task) => write!(f, "{task} changed"),
            Self::NeverRun => f.write_str("no previous run to compare against"),
            Self::Command => f.write_str("the command changed"),
            Self::Mode => f.write_str("the mode changed"),
            Self::Directory => f.write_str("the directory it runs in changed"),
            Self::VariableChanged(name) => {
                write!(f, "the environment changed: {name} has a different value")
            }
            Self::VariableAdded(name) => write!(f, "the environment changed: {name} is new"),
            Self::VariableRemoved(name) => write!(f, "the environment changed: {name} is gone"),
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
        if let Some(change) = self.environment.diff(&now.environment) {
            return change;
        }
        // The task's own inputs before another package's result: a file the
        // reader changed in this package is the nearer cause, and the one they
        // are more likely to be asking about.
        match self.input_change(now) {
            Change::Unnamed => self.dependency_change(now).unwrap_or(Change::Unnamed),
            named => named,
        }
    }

    /// The first task in another package that is keyed on differently now.
    fn dependency_change(&self, now: &LastRun) -> Option<Change> {
        now.dependencies
            .iter()
            .find(|dependency| !self.dependencies.contains(dependency))
            .or_else(|| {
                self.dependencies
                    .iter()
                    .find(|dependency| !now.dependencies.contains(dependency))
            })
            .map(|dependency| Change::Dependency(dependency.task.clone()))
    }

    /// The first difference between the two input lists.
    fn input_change(&self, now: &LastRun) -> Change {
        if self.inputs_truncated || now.inputs_truncated {
            return Change::Unnamed;
        }
        match first_difference(named(&self.inputs), named(&now.inputs)) {
            None => Change::Unnamed,
            Some(Difference::Removed(path)) => Change::InputRemoved(path.to_owned()),
            Some(Difference::Added(path)) => Change::InputAdded(path.to_owned()),
            Some(Difference::Changed(path)) => Change::InputChanged(path.to_owned()),
        }
    }
}

/// Inputs as `(path, digest)`, in path order.
fn named(files: &[InputFile]) -> impl Iterator<Item = (&str, &str)> {
    files
        .iter()
        .map(|file| (file.path.as_str(), file.digest.as_str()))
}

/// How two lists of named digests first differ.
pub(crate) enum Difference<'a> {
    Removed(&'a str),
    Added(&'a str),
    Changed(&'a str),
}

/// The first difference between two lists of `(name, digest)`, each in name
/// order — a note's inputs, or its variables.
///
/// Both are in order, so one pass over the two finds it without building a
/// map of either.
pub(crate) fn first_difference<'a>(
    before: impl Iterator<Item = (&'a str, &'a str)>,
    after: impl Iterator<Item = (&'a str, &'a str)>,
) -> Option<Difference<'a>> {
    let mut before = before.peekable();
    let mut after = after.peekable();
    loop {
        match (before.peek().copied(), after.peek().copied()) {
            (None, None) => return None,
            (Some((old, _)), None) => return Some(Difference::Removed(old)),
            (None, Some((new, _))) => return Some(Difference::Added(new)),
            (Some((old, was)), Some((new, is))) => {
                if old < new {
                    return Some(Difference::Removed(old));
                }
                if new < old {
                    return Some(Difference::Added(new));
                }
                if was != is {
                    return Some(Difference::Changed(old));
                }
                before.next();
                after.next();
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
    ///
    /// It also removes what a uf from before #1006 kept: see
    /// [`TaskCache::remove_plaintext_notes`].
    pub fn sweep(&self) {
        uf_infra::cache::sweep(&self.directory, uf_infra::cache::CacheBound::default());
        self.remove_plaintext_notes();
    }

    /// Remove the notes an earlier uf kept in `last/`, which held every `.env`
    /// value a task was given in plain text.
    ///
    /// Removed rather than left for the next run of each task to overwrite:
    /// a task that is renamed, or never run again, would keep its note — and
    /// its credentials — for as long as the checkout exists. The notes that
    /// replaced them are in `notes/`, so finding the old directory is the
    /// whole test, and nothing in it has to be opened to find out what it is.
    /// An older uf run in the same project writes it again, and the next run
    /// of this one removes it again.
    ///
    /// [`fs::remove_dir_all`] does not follow a symbolic link, at the top or
    /// below it, so a `last` planted as a link loses the link and nothing it
    /// points at.
    fn remove_plaintext_notes(&self) {
        let _ = fs::remove_dir_all(self.directory.join("last"));
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
            .join("notes")
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
            environment: Environment::new("development"),
            dependencies: Vec::new(),
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

    /// Another package's task is named when it is what changed — and a file of
    /// the task's own is named ahead of it when both did, because that is the
    /// nearer cause.
    #[test]
    fn a_dependency_in_another_package_is_named_after_the_tasks_own_inputs() {
        let upstream = |key: &str| {
            vec![Upstream {
                task: String::from("ui#build"),
                key: key.to_owned(),
            }]
        };
        let mut before = note("c", &[("a", "1")]);
        before.dependencies = upstream("old");
        let mut after = note("c", &[("a", "1")]);
        after.dependencies = upstream("new");
        assert_eq!(
            before.diff(&after),
            Change::Dependency(String::from("ui#build"))
        );

        let mut both = note("c", &[("a", "2")]);
        both.dependencies = upstream("new");
        assert_eq!(before.diff(&both), Change::InputChanged(String::from("a")));
    }

    /// A run with no workspace writes the note it always wrote.
    #[test]
    fn a_note_with_no_dependencies_does_not_mention_them() {
        let json = serde_json::to_string(&note("c", &[])).unwrap();
        assert!(!json.contains("dependencies"), "{json}");
        let read: LastRun = serde_json::from_str(&json).unwrap();
        assert!(read.dependencies.is_empty());
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

    #[test]
    fn a_sweep_removes_the_plaintext_notes_an_earlier_uf_kept() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join(".uf/cache/task/last");
        fs::create_dir_all(&old).unwrap();
        fs::write(
            old.join("note.json"),
            r#"{"environment":"developmentAPI_TOKEN=x"}"#,
        )
        .unwrap();
        TaskCache::open(Utf8Path::from_path(dir.path()).unwrap()).sweep();
        assert!(!old.exists());
    }

    #[cfg(unix)]
    #[test]
    fn removing_them_does_not_follow_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("keep.txt"), "mine").unwrap();
        let task = dir.path().join(".uf/cache/task");
        fs::create_dir_all(&task).unwrap();
        std::os::unix::fs::symlink(outside.path(), task.join("last")).unwrap();
        TaskCache::open(Utf8Path::from_path(dir.path()).unwrap()).sweep();
        assert!(outside.path().join("keep.txt").is_file());
    }
}
