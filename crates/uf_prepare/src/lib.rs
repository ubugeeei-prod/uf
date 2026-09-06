#![deny(missing_docs)]
//! What `uf prepare` does before a commit, and the record it leaves behind.
//!
//! `uf prepare` is a pre-commit command: it narrows to the files git has
//! staged, regenerates the types that are derived from sources rather than
//! written by hand, and then runs the checks a commit should not go without.
//! This crate owns the plan, the staged-file discovery, and the vocabulary the
//! run is reported in; `uf_cli`'s `prepare` module runs the steps and renders
//! them.
//!
//! # The plan is what happens
//!
//! [`PreparePlan::steps`] is not a description of an intent. Every step in it
//! is executed, in order, and every step that is executed is in it. A step
//! that cannot be done honestly does not belong here — the list used to name
//! six steps and perform one, and a reader of the output was told six things
//! had happened.
//!
//! One step was removed rather than implemented, and the reason is recorded on
//! [`PrepareStep`] so it is not proposed again.
//!
//! # What "staged" means here
//!
//! [`discover_staged_files`] asks git for the files in the index. It reads the
//! *working tree* copy of those files, not the staged blob: uf does not stash
//! unstaged changes the way `lint-staged` does, so a file that is half staged
//! is checked as it is on disk. That is stated rather than hidden because it
//! is the one place `uf prepare` differs from the tool it is compatible with,
//! and a partially staged file is the case where the difference shows.
//!
//! A project with no git — an exported tarball, a fresh directory, a CI job
//! that unpacked an archive — has no staged set at all. There the run widens
//! to the whole project instead of narrowing to nothing, because "check
//! everything" is the safe direction and refusing to run would make
//! `uf prepare` unusable outside a repository.

use std::process::Command;

use camino::Utf8Path;
use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// The steps of a plan, inline up to eight.
pub type PrepareSteps = SmallVec<[PrepareStep; 8]>;

/// What a run of `uf prepare` will do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparePlan {
    /// Whether the run narrows to the files git has staged.
    pub lint_staged_compatible: bool,
    /// Whether the run regenerates derived types.
    pub code_generator: bool,
    /// Whether those types are written to disk rather than only computed.
    pub write_generated_files: bool,
    /// How results are cached between runs.
    pub cache: PrepareCacheMode,
    /// The steps, in the order they run.
    pub steps: PrepareSteps,
}

impl Default for PreparePlan {
    fn default() -> Self {
        Self {
            lint_staged_compatible: true,
            code_generator: true,
            write_generated_files: true,
            cache: PrepareCacheMode::OptIn,
            steps: smallvec::smallvec![
                PrepareStep::DiscoverStagedFiles,
                PrepareStep::GenerateRouterTypes,
                PrepareStep::GenerateServerActionTypes,
                PrepareStep::RunLint,
                PrepareStep::RunFormatCheck,
            ],
        }
    }
}

/// One step of the plan.
///
/// # The step that is not here
///
/// `generate-validator-types` was in this list and was never implemented. It
/// is not missing work; it was the wrong idea, and the reason is worth keeping
/// so that the next reader of the roadmap does not add it back.
///
/// A validator schema is a *value*: `object({ email: string() })` is a call,
/// and nothing outside a JavaScript runtime knows what it produced. Generating
/// a form's value and error types from it would mean evaluating the module at
/// prepare time and writing down a second copy of a type that already exists.
///
/// It already exists because `@uniflowed/validator/infer` computes it in the
/// checker: `InferOutput<typeof Account>` is the parsed value's type and
/// `InferInput<typeof Account>` is the type the controls produce, both derived
/// from the schema by Flow itself, with no generated file, no build step, and
/// no chance of the generated copy disagreeing with the schema it came from.
/// `@uniflowed/form`'s `validatorResolver` carries those through
/// `handleSubmit`, so a form's values are already typed from its schema today.
///
/// The error half cannot be generated at all. `@uniflowed/form` keys errors by
/// the dotted field path `register` was given, and Flow has no
/// template-literal types, so "the error for `items.0.name`" is not a type any
/// generator could emit — `packages/form/resolver.js` and
/// `packages/validator/infer.js` both say so at length. A generated
/// `FormErrors` that was really `{ [string]: FieldError }` would look like it
/// checked something and would not.
///
/// So the step is gone rather than stubbed. The roadmap line it came from is
/// answered in `docs/issue-roadmap.md` with the same reasoning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrepareStep {
    /// Ask git which files are staged, and narrow the checks to them.
    DiscoverStagedFiles,
    /// Write the route table's Flow types from the reserved page files.
    GenerateRouterTypes,
    /// Write the Flow types of every callable server action.
    GenerateServerActionTypes,
    /// Lint the staged files.
    RunLint,
    /// Check that the staged files are formatted.
    RunFormatCheck,
}

impl PrepareStep {
    /// The step's name, as it is printed and as it appears in `prepare.json`.
    ///
    /// The same spelling in all three places — here, the JSON, and
    /// `@uniflowed/prepare`'s `PrepareStep` union — so that a reader who sees
    /// a step fail on screen can find it in the file and in the Flow type
    /// without translating between three casings.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DiscoverStagedFiles => "discover-staged-files",
            Self::GenerateRouterTypes => "generate-router-types",
            Self::GenerateServerActionTypes => "generate-server-action-types",
            Self::RunLint => "run-lint",
            Self::RunFormatCheck => "run-format-check",
        }
    }
}

/// How `uf prepare` caches between runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrepareCacheMode {
    /// Nothing is cached unless the project asks for it.
    OptIn,
}

impl PrepareCacheMode {
    /// The mode's name, as it is printed and written.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OptIn => "opt-in",
        }
    }
}

/// A file `uf prepare` wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    /// Where it was written, relative to the project root.
    pub path: CompactString,
    /// What it holds.
    pub kind: GeneratedFileKind,
}

/// What a generated file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratedFileKind {
    /// The route table: `RoutePath`, `RouteParams`, `route`.
    RouterTypes,
    /// The server action table: `ServerActions` and the types over it.
    ServerActionTypes,
}

/// How a step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepStatus {
    /// It ran and found nothing to report.
    Ok,
    /// It ran and reported a problem, or could not finish.
    Failed,
    /// It had nothing to do, and that is not a problem.
    Skipped,
    /// An earlier step failed, so this one never started.
    ///
    /// The distinction from [`StepStatus::Skipped`] is the whole point of
    /// writing the record at all: `prepare.json` must not say a check passed
    /// when it was never run.
    NotRun,
}

impl StepStatus {
    /// The status as it is printed and written.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::NotRun => "not-run",
        }
    }

    /// Whether this status should fail the command.
    #[must_use]
    pub const fn is_failure(self) -> bool {
        matches!(self, Self::Failed)
    }
}

/// What one step did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepOutcome {
    /// Which step.
    pub step: PrepareStep,
    /// How it ended.
    pub status: StepStatus,
    /// One line saying what it did, or why it did not.
    pub detail: CompactString,
}

impl StepOutcome {
    /// A step that ran and found nothing to report.
    #[must_use]
    pub fn ok(step: PrepareStep, detail: impl Into<CompactString>) -> Self {
        Self {
            step,
            status: StepStatus::Ok,
            detail: detail.into(),
        }
    }

    /// A step that reported a problem.
    #[must_use]
    pub fn failed(step: PrepareStep, detail: impl Into<CompactString>) -> Self {
        Self {
            step,
            status: StepStatus::Failed,
            detail: detail.into(),
        }
    }

    /// A step with nothing to do.
    #[must_use]
    pub fn skipped(step: PrepareStep, detail: impl Into<CompactString>) -> Self {
        Self {
            step,
            status: StepStatus::Skipped,
            detail: detail.into(),
        }
    }

    /// A step an earlier failure kept from starting.
    #[must_use]
    pub fn not_run(step: PrepareStep) -> Self {
        Self {
            step,
            status: StepStatus::NotRun,
            detail: CompactString::const_new("an earlier step failed"),
        }
    }
}

/// Which files a run is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StagedFiles {
    /// Git answered: exactly these paths, relative to the project root,
    /// sorted and deduplicated.
    ///
    /// Empty means git has an index and nothing is in it. That is not the same
    /// as [`StagedFiles::Unavailable`] and must not be treated as it: a commit
    /// with nothing staged has nothing to check.
    Staged(Vec<CompactString>),
    /// There is no staged set to read, for the stated reason.
    Unavailable(NoStagedSet),
}

impl StagedFiles {
    /// Whether a project-relative path is one this run should check.
    ///
    /// Exact equality rather than the substring match `uf lint PATH` uses.
    /// The two are answering different questions: a person who types
    /// `uf lint packages/ui` means "everything under there", and git names
    /// one file. Substring matching a git path would pull in `src/app.js`
    /// because `app.js` was staged.
    #[must_use]
    pub fn selects(&self, relative_path: &str) -> bool {
        match self {
            Self::Staged(files) => files.iter().any(|file| file == relative_path),
            Self::Unavailable(_) => true,
        }
    }

    /// How many files are staged, or [`None`] when there is no staged set.
    ///
    /// Not `len`: `Some(0)` and `None` are the two answers that matter here
    /// and they mean opposite things — nothing to check, and everything to
    /// check — so a name that invites `is_empty` beside it would invite the
    /// one confusion this type exists to prevent.
    #[must_use]
    pub fn count(&self) -> Option<usize> {
        match self {
            Self::Staged(files) => Some(files.len()),
            Self::Unavailable(_) => None,
        }
    }

    /// Whether git answered with an empty index.
    ///
    /// The case where there is nothing to check at all, as opposed to
    /// [`StagedFiles::Unavailable`], where there is everything to check.
    #[must_use]
    pub fn is_empty_index(&self) -> bool {
        matches!(self, Self::Staged(files) if files.is_empty())
    }
}

/// Why there is no staged set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoStagedSet {
    /// `git` is not on `PATH`.
    GitMissing,
    /// The directory is not inside a git working tree.
    NotAWorkingTree,
    /// `git` was there, and said no.
    GitFailed(CompactString),
}

impl NoStagedSet {
    /// One line naming the reason, for the report and the record.
    #[must_use]
    pub fn reason(&self) -> CompactString {
        match self {
            Self::GitMissing => CompactString::const_new("git is not on PATH"),
            Self::NotAWorkingTree => {
                CompactString::const_new("this directory is not a git working tree")
            }
            Self::GitFailed(message) => message.clone(),
        }
    }
}

/// What went wrong reading the index.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StagedFilesError {
    /// A staged path is not valid UTF-8.
    ///
    /// Fatal rather than skipped, and for the same reason discovery treats a
    /// non-UTF-8 path as fatal: a file uf cannot name is one it cannot report
    /// on either, and silently dropping it from a pre-commit check is how a
    /// hook starts lying about what it looked at.
    #[error("{count} staged path(s) are not valid UTF-8; uf cannot check a file it cannot name")]
    NonUtf8Path {
        /// How many there were.
        count: usize,
    },
}

/// Which files git has staged in `root`.
///
/// Runs `git diff --cached --name-only -z --relative --diff-filter=ACMR`, with
/// `git -C root` so that `--relative` makes every path project-relative and
/// drops anything staged outside the project — a repository can hold more than
/// one uf project, and the one being prepared is only responsible for its own
/// files.
///
/// `--diff-filter=ACMR` is added, copied, modified and renamed. A deleted file
/// is deliberately absent: there is nothing left to lint, and including it
/// would make every delete look like an unreadable file.
///
/// Whether this is a working tree is asked separately, with `rev-parse`,
/// rather than inferred from `diff` failing. Outside a repository `git diff`
/// falls back to `--no-index`, exits 129 and prints its entire usage message —
/// which is not a thing to put in front of somebody who ran a commit hook.
///
/// # Errors
///
/// [`StagedFilesError::NonUtf8Path`] when git names a path uf cannot render.
/// Every other failure — no git, no repository, a git that refused — is a
/// [`NoStagedSet`], not an error: they are all "there is no staged set", and
/// the caller widens to the whole project rather than stopping.
pub fn discover_staged_files(root: &Utf8Path) -> Result<StagedFiles, StagedFilesError> {
    match inside_work_tree(root) {
        Ok(true) => {}
        Ok(false) => {
            return Ok(StagedFiles::Unavailable(NoStagedSet::NotAWorkingTree));
        }
        Err(reason) => return Ok(StagedFiles::Unavailable(reason)),
    }

    let output = match git(
        root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--relative",
            "--diff-filter=ACMR",
        ],
    ) {
        Ok(output) => output,
        Err(reason) => return Ok(StagedFiles::Unavailable(reason)),
    };

    let mut files = Vec::new();
    let mut non_utf8 = 0usize;
    for entry in output.stdout.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        match std::str::from_utf8(entry) {
            Ok(path) => files.push(path.to_compact_string()),
            Err(_) => non_utf8 += 1,
        }
    }
    if non_utf8 > 0 {
        return Err(StagedFilesError::NonUtf8Path { count: non_utf8 });
    }

    files.sort_unstable();
    files.dedup();
    Ok(StagedFiles::Staged(files))
}

/// Whether `root` is inside a git working tree.
///
/// Any answer from git that is not `true` is a no, including a failure. That
/// is deliberate: outside a repository git says
/// `fatal: not a git repository (or any of the parent directories): .git`,
/// which is correct, is in whatever language git is configured for, and is not
/// a line to paste into the output of a commit hook. The question asked was
/// "is this a working tree", and uf has its own sentence for no.
///
/// A missing `git` is the one failure kept apart, because "install git" and
/// "this is not a repository" are different things to tell somebody.
fn inside_work_tree(root: &Utf8Path) -> Result<bool, NoStagedSet> {
    match git(root, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(output) => Ok(String::from_utf8_lossy(&output.stdout).trim() == "true"),
        Err(NoStagedSet::GitMissing) => Err(NoStagedSet::GitMissing),
        Err(_) => Ok(false),
    }
}

/// Run git in `root`, or say why it could not be run.
fn git(root: &Utf8Path, args: &[&str]) -> Result<std::process::Output, NoStagedSet> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root.as_str())
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => NoStagedSet::GitMissing,
            _ => NoStagedSet::GitFailed(error.to_compact_string()),
        })?;
    if !output.status.success() {
        // git's own first line, not its usage dump: `rev-parse` outside a
        // repository says "not a git repository", which is the useful half.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("git exited with a failure");
        return Err(NoStagedSet::GitFailed(first.to_compact_string()));
    }
    Ok(output)
}

/// The plan every run follows.
#[must_use]
pub fn default_plan() -> PreparePlan {
    PreparePlan::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_names_every_step_that_runs_and_no_others() {
        let plan = default_plan();

        assert!(plan.lint_staged_compatible);
        assert!(plan.code_generator);
        assert!(plan.write_generated_files);
        assert_eq!(plan.cache, PrepareCacheMode::OptIn);
        assert_eq!(
            plan.steps.as_slice(),
            [
                PrepareStep::DiscoverStagedFiles,
                PrepareStep::GenerateRouterTypes,
                PrepareStep::GenerateServerActionTypes,
                PrepareStep::RunLint,
                PrepareStep::RunFormatCheck,
            ]
        );
    }

    /// The names are a contract with `prepare.json` and `@uniflowed/prepare`.
    #[test]
    fn every_step_name_is_the_kebab_case_of_its_variant() {
        assert_eq!(
            PrepareStep::DiscoverStagedFiles.name(),
            "discover-staged-files"
        );
        assert_eq!(
            PrepareStep::GenerateServerActionTypes.name(),
            "generate-server-action-types"
        );
        assert_eq!(PrepareStep::RunFormatCheck.name(), "run-format-check");
        for step in default_plan().steps {
            let json = serde_json::to_string(&step).expect("a step serializes");
            assert_eq!(json, format!("\"{}\"", step.name()));
        }
        assert_eq!(PrepareCacheMode::OptIn.name(), "opt-in");
    }

    #[test]
    fn a_staged_set_selects_by_whole_path_not_by_substring() {
        let staged = StagedFiles::Staged(vec![CompactString::const_new("app.js")]);

        assert!(staged.selects("app.js"));
        // The substring rule `uf lint PATH` uses would have said yes here.
        assert!(!staged.selects("src/app.js"));
        assert!(!staged.selects("app.js.bak"));
        assert_eq!(staged.count(), Some(1));
        assert!(!staged.is_empty_index());
    }

    #[test]
    fn no_staged_set_means_every_file_rather_than_none() {
        let none = StagedFiles::Unavailable(NoStagedSet::GitMissing);

        assert!(none.selects("anything.js"));
        assert_eq!(none.count(), None);
        assert!(!none.is_empty_index());
        assert_eq!(none, StagedFiles::Unavailable(NoStagedSet::GitMissing));
    }

    #[test]
    fn an_empty_index_is_not_the_same_as_no_index() {
        let empty = StagedFiles::Staged(Vec::new());

        assert!(!empty.selects("anything.js"));
        assert_eq!(empty.count(), Some(0));
        assert!(empty.is_empty_index());
    }

    #[test]
    fn every_reason_for_having_no_staged_set_reads_as_a_sentence() {
        assert_eq!(NoStagedSet::GitMissing.reason(), "git is not on PATH");
        assert_eq!(
            NoStagedSet::NotAWorkingTree.reason(),
            "this directory is not a git working tree"
        );
        assert_eq!(
            NoStagedSet::GitFailed(CompactString::const_new("fatal: bad object")).reason(),
            "fatal: bad object"
        );
    }

    #[test]
    fn only_a_failure_fails_the_command() {
        assert!(StepStatus::Failed.is_failure());
        assert!(!StepStatus::Ok.is_failure());
        assert!(!StepStatus::Skipped.is_failure());
        // A step that never ran is not a passing step, and it is not a failing
        // one either: the failure it is downstream of is the one to report.
        assert!(!StepStatus::NotRun.is_failure());
        assert_eq!(StepStatus::NotRun.name(), "not-run");
    }

    #[test]
    fn an_outcome_carries_the_step_it_is_about() {
        let outcome = StepOutcome::failed(PrepareStep::RunLint, "2 errors");
        assert_eq!(outcome.step, PrepareStep::RunLint);
        assert_eq!(outcome.detail, "2 errors");
        assert!(outcome.status.is_failure());

        let skipped = StepOutcome::skipped(PrepareStep::RunLint, "nothing staged");
        assert_eq!(skipped.status, StepStatus::Skipped);

        let not_run = StepOutcome::not_run(PrepareStep::RunFormatCheck);
        assert_eq!(not_run.status, StepStatus::NotRun);
        assert_eq!(not_run.detail, "an earlier step failed");

        let ok = StepOutcome::ok(PrepareStep::GenerateRouterTypes, "wrote router.js");
        assert_eq!(ok.status, StepStatus::Ok);
    }

    /// A directory that is not a repository is answered, not crashed on.
    #[test]
    fn a_directory_outside_a_repository_has_no_staged_set() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = camino::Utf8Path::from_path(dir.path()).expect("a UTF-8 path");

        let staged = discover_staged_files(root).expect("an answer");

        // `git diff --cached` outside a repository exits 129 and prints its
        // whole usage message; `rev-parse` is asked first so that never
        // reaches a reader.
        match staged {
            StagedFiles::Unavailable(reason) => {
                assert!(
                    matches!(
                        reason,
                        NoStagedSet::NotAWorkingTree | NoStagedSet::GitMissing
                    ),
                    "{reason:?}"
                );
                // Not git's usage dump, and not git's "fatal:" either.
                assert!(!reason.reason().contains("usage:"), "{}", reason.reason());
                assert!(!reason.reason().contains("fatal:"), "{}", reason.reason());
            }
            // A machine whose temporary directory happens to sit inside a
            // checkout answers as a working tree. A brand new directory in it
            // still has nothing in the index, which is the other half of what
            // this is checking: an answer, not a usage dump.
            StagedFiles::Staged(files) => assert!(files.is_empty(), "{files:?}"),
        }
    }

    /// Run git in `root`, panicking with its stderr when it refuses.
    fn run_git(root: &camino::Utf8Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(root.as_str())
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A repository with `files` written and staged, and one file left alone.
    fn staged_repository() -> (tempfile::TempDir, camino::Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = camino::Utf8Path::from_path(dir.path())
            .expect("a UTF-8 path")
            .to_path_buf();
        run_git(&root, &["init", "--quiet"]);
        std::fs::create_dir_all(root.join("src")).expect("a source directory");
        for (path, source) in [
            ("src/a.js", "// @flow\n"),
            ("src/b.js", "// @flow\n"),
            ("untracked.js", "// @flow\n"),
        ] {
            std::fs::write(root.join(path), source).expect("a file");
        }
        run_git(&root, &["add", "src/a.js", "src/b.js"]);
        (dir, root)
    }

    /// The index, exactly: staged files in, untracked files out.
    ///
    /// A repository with no commit at all is the case a fresh `git init`
    /// leaves, and it is where `git diff --cached` against `HEAD` would fail
    /// if the implementation reached for `HEAD` rather than the index.
    #[test]
    fn the_staged_set_is_the_index_and_nothing_else() {
        let (_dir, root) = staged_repository();

        let staged = discover_staged_files(&root).expect("an answer");

        assert_eq!(
            staged,
            StagedFiles::Staged(vec![
                CompactString::const_new("src/a.js"),
                CompactString::const_new("src/b.js"),
            ])
        );
        assert!(staged.selects("src/a.js"));
        assert!(!staged.selects("untracked.js"));
    }

    /// A deleted file is not in the set: there is nothing left to check.
    #[test]
    fn a_staged_deletion_is_not_a_file_to_check() {
        let (_dir, root) = staged_repository();
        run_git(
            &root,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "--quiet",
                "-m",
                "first",
            ],
        );
        std::fs::remove_file(root.join("src/a.js")).expect("to delete a file");
        run_git(&root, &["add", "src/a.js"]);

        let staged = discover_staged_files(&root).expect("an answer");

        assert_eq!(staged, StagedFiles::Staged(Vec::new()));
        assert!(staged.is_empty_index());
    }

    /// A project below the repository root sees only its own files.
    ///
    /// One checkout can hold more than one uf project, and the one being
    /// prepared is not responsible for a file staged in the one beside it.
    #[test]
    fn a_project_below_the_repository_root_sees_only_its_own_files() {
        let (_dir, root) = staged_repository();
        std::fs::create_dir_all(root.join("packages/inner/src")).expect("a nested project");
        std::fs::write(root.join("packages/inner/src/c.js"), "// @flow\n").expect("a file");
        run_git(&root, &["add", "packages/inner/src/c.js"]);

        let inner = discover_staged_files(&root.join("packages/inner")).expect("an answer");

        assert_eq!(
            inner,
            StagedFiles::Staged(vec![CompactString::const_new("src/c.js")])
        );
    }
}
