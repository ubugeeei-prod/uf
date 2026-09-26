//! `uf test --changed <ref>`: only the test files a change since `<ref>`
//! reaches.
//!
//! A pull request's CI asks one question — did this change break anything? —
//! and on a large suite most files cannot have been touched by the change at
//! all. This is the selection that lets CI skip them, and it is only worth
//! having if it never skips a file the change could break: a smaller green run
//! that missed a failure is worse than a slow one.
//!
//! # What changed
//!
//! Every path that differs from the commit where `HEAD`'s history left `<ref>`
//! — committed since, staged, or still only in the working tree — and every
//! untracked file git does not ignore. The merge base rather than `<ref>`
//! itself: on a branch cut from `main` a week ago, `main` has moved, and
//! diffing against its tip would count everything that landed there since as
//! this branch's change.
//!
//! # What a change reaches
//!
//! [`uf_test::ImportGraph`], the graph `uf test --watch` re-runs from, built
//! over every file the project scan found. A test file runs when it is a
//! changed file or imports one through any number of modules. A deleted file is
//! not in the scan and still reaches the tests that import it — they are the
//! tests the deletion breaks — and a changed snapshot reaches the test file it
//! belongs to.
//!
//! # When the answer is everything
//!
//! Some files decide every result without any test importing them:
//! `uf.config.js`, and a `package.json` or lockfile, which decide what an
//! import resolves to; and the `.env` files a run loads. The graph cannot see
//! through them, so a change to one runs the whole suite, and the run names the
//! file that decided it.
//!
//! # What it cannot see
//!
//! The graph follows relative specifiers. A module reached through a package
//! name — a workspace package included — and a file a test reads from disk
//! rather than imports are outside it, exactly as they are for watch mode.

use std::collections::BTreeSet;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8Path;
use uf_project::ProjectFile;
use uf_term::KeyValue;
use uf_test::ImportGraph;

use crate::support::plural;
use crate::ui::Ui;

/// File names that decide what an import resolves to, wherever they are.
///
/// Every level rather than the root alone: a workspace package's manifest
/// decides what its own imports resolve to.
const WHOLE_SUITE_FILES: &[&str] = &[
    "package.json",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "uf.lock",
];

/// The directory a snapshot is written to, beside the test that took it.
const SNAPSHOT_DIRECTORY: &str = "__snapshots__";

/// Said when `uf.config.js` turns coverage on and a `--changed` run skips it.
const COVERAGE_NOT_COLLECTED: &str =
    "not collected: a run over the files a change reaches is not the project's coverage";

/// What `--changed` chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    /// The ref as it was given.
    reference: String,
    /// The commit the change is measured from.
    base: String,
    /// How many paths changed.
    changed: usize,
    /// The changed file that runs the whole suite, when one does.
    whole_suite: Option<String>,
    /// Every project path a change reaches, when not the whole suite.
    reached: BTreeSet<String>,
}

impl Selection {
    /// Whether the test file at `path` runs.
    pub(crate) fn admits(&self, path: &str) -> bool {
        self.whole_suite.is_some() || self.reached.contains(path)
    }

    /// What the run's `changed` row says: `running` of the project's `tests`
    /// test files run.
    pub(crate) fn describe(&self, running: usize, tests: usize) -> String {
        let base = self.base.get(..12).unwrap_or(&self.base);
        let since = uf_infra::into_string(uf_infra::cstr!(
            "since {} (merge base {base})",
            self.reference
        ));
        match &self.whole_suite {
            Some(file) => uf_infra::into_string(uf_infra::cstr!(
                "{file} {since}, and every test depends on it · running all {}",
                plural(tests, "test file")
            )),
            None => uf_infra::into_string(uf_infra::cstr!(
                "{} {since} · {running} of {} reach them",
                plural(self.changed, "file"),
                plural(tests, "test file")
            )),
        }
    }
}

/// The test files `selection` runs, out of every test file in `tests`, after
/// the rows that say how many that is.
///
/// `coverage_configured` is whether `uf.config.js` turns coverage on, which a
/// `--changed` run does not collect, and says so rather than going quiet about
/// a report the project asked for.
pub(crate) fn narrow(
    ui: &mut Ui,
    selection: &Selection,
    tests: Vec<ProjectFile>,
    coverage_configured: bool,
) -> Vec<ProjectFile> {
    let total = tests.len();
    let running: Vec<ProjectFile> = tests
        .into_iter()
        .filter(|file| selection.admits(&file.relative_path))
        .collect();
    let summary = selection.describe(running.len(), total);
    ui.render(|renderer, out| {
        let mut rows = vec![KeyValue::new("changed", &summary)];
        if coverage_configured {
            rows.push(KeyValue::new("coverage", COVERAGE_NOT_COLLECTED));
        }
        renderer.key_values(out, 2, &rows);
    });
    running
}

/// Ask git what changed since `reference`, and select what that reaches.
///
/// `files` is every file the project scan found, not only the tests: the graph
/// has to hold the modules between a test and the file that changed.
pub(crate) fn select(root: &Utf8Path, reference: &str, files: &[ProjectFile]) -> Result<Selection> {
    let (base, changed) = changed_since(root, reference)?;
    Ok(selection(reference, base, &changed, files))
}

/// The selection for `changed`, paths relative to the project root.
///
/// Apart from [`select`] so that it is a pure function of what git said, which
/// is what the tests below hold it to without a repository.
fn selection(
    reference: &str,
    base: String,
    changed: &[String],
    files: &[ProjectFile],
) -> Selection {
    let whole_suite = changed
        .iter()
        .find(|path| decides_every_result(path))
        .cloned();
    let reached = if whole_suite.is_some() {
        BTreeSet::new()
    } else {
        let graph = ImportGraph::build(
            files
                .iter()
                .map(|file| (file.relative_path.as_str(), file.source.as_str())),
        );
        let starts: Vec<String> = changed
            .iter()
            .map(|path| snapshot_owner(path).unwrap_or_else(|| path.clone()))
            .collect();
        graph
            .affected(starts.iter().map(String::as_str))
            .into_iter()
            .map(|path| path.to_string())
            .collect()
    };
    Selection {
        reference: reference.to_owned(),
        base,
        changed: changed.len(),
        whole_suite,
        reached,
    }
}

/// Whether a change to `path` can change a test's result without any test
/// importing it.
fn decides_every_result(path: &str) -> bool {
    let Some(name) = Utf8Path::new(path).file_name() else {
        return false;
    };
    WHOLE_SUITE_FILES.contains(&name)
        || uf_config::CONFIG_FILES.contains(&name)
        || name == ".env"
        || name.starts_with(".env.")
}

/// The test file a snapshot belongs to: `a/__snapshots__/b.test.js.snap` was
/// taken by `a/b.test.js`.
fn snapshot_owner(path: &str) -> Option<String> {
    let path = Utf8Path::new(path);
    let test = path.file_name()?.strip_suffix(".snap")?;
    let directory = path.parent()?;
    if directory.file_name()? != SNAPSHOT_DIRECTORY {
        return None;
    }
    Some(directory.parent()?.join(test).into_string())
}

/// The merge base with `reference`, and every path that differs from it,
/// relative to `root` and sorted.
fn changed_since(root: &Utf8Path, reference: &str) -> Result<(String, Vec<String>)> {
    // git reads an argument that starts with `-` as an option, and
    // `--output=<file>` is an option that writes a file.
    if reference.starts_with('-') {
        bail!(uf_infra::cstr!(
            "`uf test --changed {reference}`: a ref cannot start with `-`"
        ));
    }
    let base = git(root, &["merge-base", reference, "HEAD"]).with_context(|| {
        uf_infra::into_string(uf_infra::cstr!(
            "`uf test --changed {reference}` measures a change from the commit where HEAD's \
             history left `{reference}`, and git could not name one. Check that \
             `{reference}` exists; a shallow CI clone also needs the history where the two meet \
             (`fetch-depth: 0` for `actions/checkout`)"
        ))
    })?;
    let base = base.trim().to_owned();
    let mut changed = paths(&git(
        root,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "--relative",
            "-z",
            &base,
            "--",
        ],
    )?);
    changed.extend(paths(&git(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?));
    changed.sort_unstable();
    changed.dedup();
    Ok((base, changed))
}

/// Run git in `root`, and return what it printed.
fn git(root: &Utf8Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root.as_str())
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                anyhow!(uf_infra::cstr!(
                    "`uf test --changed` asks git what changed, and there is no `git` on PATH"
                ))
            }
            _ => anyhow!(uf_infra::cstr!("could not run git: {error}")),
        })?;
    if !output.status.success() {
        // git's own first line: "Not a valid object name", "not a git
        // repository". `merge-base` with no common ancestor prints nothing.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("it exited without naming a commit");
        bail!(uf_infra::cstr!(
            "git {}: {first}",
            args.first().copied().unwrap_or_default()
        ));
    }
    String::from_utf8(output.stdout).context("git named a path that is not UTF-8")
}

/// The paths in git's `-z` output.
fn paths(output: &str) -> Vec<String> {
    output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use uf_project::SourceKind;

    fn file(path: &str, source: &str) -> ProjectFile {
        ProjectFile {
            relative_path: path.to_owned(),
            absolute_path: Utf8PathBuf::from(path),
            source: source.to_owned(),
            kind: SourceKind::JavaScript,
        }
    }

    /// `a.test.js` reaches `deep.js` through `shared.js`; `b.test.js` imports
    /// `other.js` alone.
    fn project() -> Vec<ProjectFile> {
        vec![
            file("src/deep.js", "export const deep = 1;\n"),
            file(
                "src/shared.js",
                "import { deep } from './deep.js';\nexport const shared = deep;\n",
            ),
            file(
                "src/a.test.js",
                "import { shared } from './shared.js';\nit('a', () => {});\n",
            ),
            file(
                "src/b.test.js",
                "import { other } from './other.js';\nit('b', () => {});\n",
            ),
            file("src/other.js", "export const other = 1;\n"),
        ]
    }

    fn chosen(changed: &[&str], files: &[ProjectFile]) -> Selection {
        let changed: Vec<String> = changed.iter().map(|path| (*path).to_owned()).collect();
        selection("main", "0123456789abcdef".to_owned(), &changed, files)
    }

    /// The test files `changed` runs, in path order.
    fn admitted(changed: &[&str], files: &[ProjectFile]) -> Vec<String> {
        let selection = chosen(changed, files);
        let mut tests: Vec<String> = files
            .iter()
            .map(|file| file.relative_path.clone())
            .filter(|path| path.ends_with(".test.js") && selection.admits(path))
            .collect();
        tests.sort();
        tests
    }

    #[test]
    fn a_change_reaches_the_tests_that_import_it_through_other_modules() {
        assert_eq!(admitted(&["src/deep.js"], &project()), ["src/a.test.js"]);
        assert_eq!(admitted(&["src/shared.js"], &project()), ["src/a.test.js"]);
        assert_eq!(admitted(&["src/b.test.js"], &project()), ["src/b.test.js"]);
    }

    #[test]
    fn a_deleted_module_reaches_the_tests_that_still_import_it() {
        let files: Vec<ProjectFile> = project()
            .into_iter()
            .filter(|file| file.relative_path != "src/other.js")
            .collect();

        assert_eq!(admitted(&["src/other.js"], &files), ["src/b.test.js"]);
    }

    #[test]
    fn a_change_no_test_reaches_runs_nothing() {
        assert!(admitted(&["README.md"], &project()).is_empty());
        assert!(admitted(&[], &project()).is_empty());
    }

    #[test]
    fn a_snapshot_change_runs_the_test_that_took_it() {
        assert_eq!(
            admitted(&["src/__snapshots__/b.test.js.snap"], &project()),
            ["src/b.test.js"]
        );
        assert_eq!(
            snapshot_owner("__snapshots__/top.test.js.snap").as_deref(),
            Some("top.test.js")
        );
        assert_eq!(snapshot_owner("src/notes.snap"), None);
    }

    #[test]
    fn a_manifest_lockfile_config_or_env_change_runs_every_test() {
        for path in [
            "package.json",
            "npm/ui/package.json",
            "pnpm-lock.yaml",
            "bun.lock",
            "uf.lock",
            "uf.config.js",
            ".env",
            ".env.test.local",
        ] {
            let selection = chosen(&[path, "src/deep.js"], &project());

            assert_eq!(selection.whole_suite.as_deref(), Some(path), "{path}");
            assert!(selection.admits("src/b.test.js"), "{path}");
        }
        assert!(!decides_every_result("src/environment.js"));
        assert!(!decides_every_result(".envrc"));
    }

    #[test]
    fn the_changed_row_says_what_changed_and_how_much_runs() {
        assert_eq!(
            chosen(&["src/deep.js", "README.md"], &project()).describe(1, 2),
            "2 files since main (merge base 0123456789ab) · 1 of 2 test files reach them"
        );
        assert_eq!(
            chosen(&["bun.lock"], &project()).describe(2, 2),
            "bun.lock since main (merge base 0123456789ab), and every test depends on it · \
             running all 2 test files"
        );
    }

    #[test]
    fn a_ref_git_would_read_as_an_option_is_refused_before_git_runs() {
        let error = changed_since(Utf8Path::new("/nonexistent"), "--output=/tmp/x")
            .expect_err("an option is not a ref");

        assert!(
            error.to_string().contains("cannot start with `-`"),
            "{error:#}"
        );
    }
}
