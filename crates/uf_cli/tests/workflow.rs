//! The commands that run tests, install packages, and write release plans.
//!
//! `uf prepare` was here when it wrote a plan and nothing else. It runs five
//! steps now, and what they do is `prepare.rs`.

mod support;

use std::fs;

use support::{assert_plain, uf};

fn write_test_file(dir: &std::path::Path, name: &str, body: &str) {
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join(name), body).unwrap();
}

#[test]
fn test_list_discovers_native_test_import_shape() {
    let dir = tempfile::tempdir().unwrap();
    write_test_file(
        dir.path(),
        "index.test.js",
        "// @flow\nimport { it } from '@uniflowed/test';\nit('runs', () => {});\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["test", "--list"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("src/index.test.js"));
    assert!(stdout.contains("discovered 1 runnable test"));
    assert!(stdout.contains("FasterThanBun"));
    assert!(stdout.contains("location"));
    assert_plain(&stdout);
}

// What `uf test` does when it *runs* a suite — assertions, failures, hooks,
// filters, retries, bail — is covered in `testing.rs`, against a real host.
// Those tests need the workspace's `node_modules`, so they build their
// projects inside this repository rather than in a system temp directory.

#[test]
fn publish_and_release_report_trusted_publish_plan() {
    let dir = tempfile::tempdir().unwrap();

    let publish = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("publish")
        .output()
        .unwrap();

    assert!(
        publish.status.success(),
        "{}",
        String::from_utf8_lossy(&publish.stderr)
    );
    let stdout = String::from_utf8(publish.stdout).unwrap();
    assert!(stdout.contains("first publish     Local"));
    assert!(stdout.contains("local bootstrap   yes"));
    assert!(stdout.contains("trusted provider  GitHubActionsOidc"));
    assert!(stdout.contains("tokenless         yes"));
    assert!(stdout.contains("trigger           TagPush"));
    assert!(stdout.contains("publish.json"));
    assert!(dir.path().join(".uf/publish.json").exists());

    let release = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["release", "alpha"])
        .output()
        .unwrap();

    assert!(
        release.status.success(),
        "{}",
        String::from_utf8_lossy(&release.stderr)
    );
    let stdout = String::from_utf8(release.stdout).unwrap();
    // `uf release` bumps this crate's own version, so a literal tag here would
    // fail on the next version bump rather than when the plan is wrong. The
    // bump arithmetic itself is pinned by the unit tests next to
    // `bump_semver`; this only has to predict the same answer for whatever
    // version the workspace is on right now — including one that is already a
    // prerelease, which the previous `strip_suffix(".0")` got wrong the first
    // time a release moved off `0.0.0-alpha.0`.
    let current = env!("CARGO_PKG_VERSION");
    let expected = match current.split_once("-alpha.") {
        Some((core, count)) => {
            let count: u64 = count.parse().expect("the alpha count is numeric");
            format!("{core}-alpha.{}", count + 1)
        }
        None => format!("{current}-alpha.0"),
    };
    assert!(stdout.contains("bump             Alpha"));
    assert!(stdout.contains(&format!("tag              uf@{expected}")));
    assert!(stdout.contains("command          uf release alpha"));
    assert!(stdout.contains("publish          yes"));
    assert!(stdout.contains("release.json"));
    assert!(dir.path().join(".uf/release.json").exists());
}

#[test]
fn install_runs_the_package_manager_that_drives_the_project() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "name": "install-demo",
  "dependencies": {
    "definitely-not-a-real-package-ufsdfkj": "1.0.0"
  }
}
"#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("install")
        .output()
        .unwrap();

    // The point of the test is that something really tried to install. uf used
    // to write a lockfile, print "installed 1 package" and exit 0 without
    // reaching a registry, so a green exit proved nothing; a dependency that
    // cannot exist must now make the command fail.
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !output.status.success(),
        "installing a package that does not exist must fail:\n{stdout}{stderr}"
    );
    assert!(
        stdout.contains("uf install"),
        "the banner should still be rendered:\n{stdout}"
    );

    // The package and runtime plan `uf upgrade` used to write is `uf install`'s
    // now (ubugeeei-prod/uf#424). It records the delegated manager before that
    // manager fails, without forcing a uf-native lockfile into an npm project.
    let plan = fs::read_to_string(dir.path().join(".uf/install.json")).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&plan).unwrap();
    assert_eq!(plan["packageManager"]["resolver"], "delegated");
    assert_eq!(plan["packageManager"]["manager"], "npm");
    assert!(
        plan["packageManager"]["lockfile"]
            .as_str()
            .unwrap()
            .ends_with("package-lock.json")
    );
    assert_eq!(plan["runtimeManager"]["engine"], "node");
    assert_eq!(plan["runtimeManager"]["acquisition"], "auto");
    // And the hosts it records are hosts. `runtimeManager.hosts` is documented
    // as the hosts that must be available, and every project uf installed used
    // to be told it ran on `edge`, `serverless` and `container` — three rows
    // `uf_runtime::HOSTS` grades planned with no Flow loader, so three
    // runtimes that cannot import the project's first file. A claim uf writes
    // into a file it hands the reader is the exact shape ubugeeei-prod/uf#246
    // is named after.
    let hosts = plan["runtimeManager"]["hosts"].as_array().unwrap();
    let hosts: Vec<&str> = hosts.iter().map(|host| host.as_str().unwrap()).collect();
    assert_eq!(hosts, vec!["node", "bun", "deno"], "{hosts:?}");
}

#[test]
fn install_rejects_npm_scripts() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{
  "name": "scripted",
  "scripts": {
    "test": "jest"
  }
}
"#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("install")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("error: "));
    assert!(stderr.contains("declares scripts"));
    assert!(stderr.contains("uf tasks"));
}

/// `uf upgrade` is retired, and the answer names the three commands it could
/// have meant rather than clap's guess at a spelling.
///
/// Exit 2, not 1: uf did not run a command and find a problem, it does not
/// have the command. See `docs/app/reference/cli/$page.mdx`.
#[test]
fn upgrade_is_retired_and_names_what_replaced_it() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("upgrade")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("uf self-update"), "{stderr}");
    assert!(stderr.contains("uf update"), "{stderr}");
    assert!(stderr.contains("uf install"), "{stderr}");
    assert!(
        !dir.path().join(".uf/upgrade.json").exists(),
        "a retired command wrote a file"
    );
}

#[test]
fn an_undefined_task_reports_an_error_on_stderr() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["run", "nope"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("error: task \"nope\" is not defined"));
    assert_plain(&stderr);
}

/// `uf release` writes the changelog for the version it is cutting.
///
/// The tag alone is a version number; the changelog is what is in it. Before
/// this, the only answer to "what changed" was `gh release --generate-notes`,
/// which is pull request titles in merge order, on the release page, not in
/// the repository.
///
/// A real repository with real tags, because the interesting parts are the
/// range (`<last tag>..HEAD`) and the grouping, and neither exists without
/// history to read.
#[test]
fn release_writes_the_changelog_for_the_version_it_cuts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "uf")
            .env("GIT_AUTHOR_EMAIL", "uf@example.com")
            .env("GIT_COMMITTER_NAME", "uf")
            .env("GIT_COMMITTER_EMAIL", "uf@example.com")
            .output()
            .expect("git runs");
        assert!(
            status.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    };

    git(&["init", "--quiet", "--initial-branch", "main"]);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "--quiet", "-m", "feat(cli): the first thing"]);
    git(&["tag", "uf@0.0.0-alpha.2"]);

    for (file, subject) in [
        ("b.txt", "fix(fmt): a spread keeps its parentheses"),
        ("c.txt", "docs: say what it does"),
        ("d.txt", "rename"),
    ] {
        fs::write(root.join(file), "x\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "--quiet", "-m", subject]);
    }

    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["release", "alpha"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("changelog"), "{stdout}");
    assert!(
        stdout.contains("3 changes written to the changelog"),
        "{stdout}"
    );
    assert_plain(&stdout);

    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.starts_with("# Changelog\n"), "{changelog}");
    // Since the tag, and not before it: the `feat` is in `uf@0.0.0-alpha.2`.
    assert!(!changelog.contains("the first thing"), "{changelog}");
    assert!(changelog.contains("### Fixed"), "{changelog}");
    assert!(
        changelog.contains("- **fmt**: a spread keeps its parentheses"),
        "{changelog}"
    );
    assert!(changelog.contains("### Documentation"), "{changelog}");
    assert!(changelog.contains("- say what it does"), "{changelog}");
    // A subject that is not conventional is kept rather than dropped.
    assert!(changelog.contains("### Other"), "{changelog}");
    assert!(changelog.contains("- rename"), "{changelog}");
    assert!(!changelog.contains("### Added"), "{changelog}");

    // Every one of those subjects carries no `(#NNN)`, and the run says so:
    // they are the lines nothing downstream can find by number. See #443.
    assert!(
        stdout.contains("3 commits in the range carry no pull request number"),
        "{stdout}"
    );
    assert!(stdout.contains("- rename"), "{stdout}");
    let plan: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".uf/release.json")).unwrap()).unwrap();
    assert_eq!(
        plan["unnumbered"].as_array().map(Vec::len),
        Some(3),
        "{plan}"
    );

    // Cutting the same release again would replace the section, and a section
    // that is already there is not rewritten without being asked. #457.
    let again = uf()
        .arg("--cwd")
        .arg(root)
        .args(["release", "alpha"])
        .output()
        .unwrap();
    assert!(!again.status.success());
    let stderr = String::from_utf8(again.stderr).unwrap();
    assert!(stderr.contains("already has a section"), "{stderr}");
    assert!(stderr.contains("--force"), "{stderr}");
    let untouched = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    similar_asserts::assert_eq!(untouched, changelog);

    // And with `--force` it replaces the section rather than stacking a second
    // one, which is what a release being prepared needs.
    let forced = uf()
        .arg("--cwd")
        .arg(root)
        .args(["release", "alpha", "--force"])
        .output()
        .unwrap();
    assert!(
        forced.status.success(),
        "{}",
        String::from_utf8_lossy(&forced.stderr)
    );
    let twice = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    similar_asserts::assert_eq!(twice, changelog);
}

/// The changelog date is UTC, so two releases cannot be dated out of order.
///
/// `%cs` renders a commit in the timezone *that commit* recorded. This
/// repository has both, and the pair came out backwards: alpha.13 was squashed
/// from a `+09:00` commit and dated 2026-09-08, alpha.14 from a `+00:00`
/// commit five hours later and dated 2026-09-07 — the later release above the
/// earlier date, each correct by the rule that produced it. See #630.
///
/// The two commits here are the same shape: `02:00+09:00` is `17:00Z`, and
/// `18:00+00:00` is an hour after it. Under `%cs` they render a day apart in
/// the wrong direction; in one timezone they are the same day.
#[test]
fn the_changelog_date_is_utc_rather_than_the_commit_s_own_timezone() {
    let changelog_date = |committed: &str| -> String {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .env("GIT_AUTHOR_NAME", "uf")
                .env("GIT_AUTHOR_EMAIL", "uf@example.com")
                .env("GIT_COMMITTER_NAME", "uf")
                .env("GIT_COMMITTER_EMAIL", "uf@example.com")
                .env("GIT_AUTHOR_DATE", committed)
                .env("GIT_COMMITTER_DATE", committed)
                .output()
                .expect("git runs");
            assert!(
                output.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };

        git(&["init", "--quiet", "--initial-branch", "main"]);
        fs::write(root.join("a.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "--quiet", "-m", "feat(cli): the first thing"]);
        git(&["tag", "uf@0.0.0-alpha.2"]);
        fs::write(root.join("b.txt"), "two\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "--quiet", "-m", "fix(cli): the second thing"]);

        let output = uf()
            .arg("--cwd")
            .arg(root)
            .args(["release", "alpha"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let changelog = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
        let date = changelog
            .lines()
            .find_map(|line| line.strip_prefix('_')?.strip_suffix('_'))
            .unwrap_or_else(|| panic!("no dated section in {changelog}"))
            .to_owned();
        assert_ne!(date, "unreleased", "{changelog}");
        date
    };

    // 2026-09-08T02:00:00+09:00 is 2026-09-07T17:00:00Z.
    let tokyo = changelog_date("2026-09-08T02:00:00+09:00");
    // An hour after it, recorded in a different offset.
    let utc = changelog_date("2026-09-07T18:00:00+00:00");

    assert_eq!(tokyo, "2026-09-07", "the commit's own timezone leaked in");
    assert_eq!(utc, "2026-09-07");
    // The point of the pair: later commit, not an earlier date.
    assert!(tokyo <= utc, "{tokyo} then {utc} is backwards");
}

/// `uf release` refuses to rewrite a version that has already gone out.
///
/// The version comes from `env!("CARGO_PKG_VERSION")` — the binary that cuts a
/// release is the binary being released — and there was no guard against being
/// an *old* binary. `uf@0.0.0-alpha.7`'s binary in a tree already released as
/// alpha.8 planned alpha.8 again, rewrote the published section with the
/// commits since that tag, deleted the thirty-eight lines of summary and
/// hand-placed entries in it, and reported success. See #457.
///
/// The tree here is arranged the same way: the tag for the version this binary
/// is about to plan already exists, which can only mean the tree is ahead of
/// the binary.
#[test]
fn release_refuses_a_version_that_is_already_tagged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "uf")
            .env("GIT_AUTHOR_EMAIL", "uf@example.com")
            .env("GIT_COMMITTER_NAME", "uf")
            .env("GIT_COMMITTER_EMAIL", "uf@example.com")
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };

    git(&["init", "--quiet", "--initial-branch", "main"]);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "--quiet", "-m", "feat(cli): the first thing"]);

    // The version this binary will plan, so that the tag for it can exist
    // before it runs. `uf release alpha` moves the alpha component by one.
    let current = env!("CARGO_PKG_VERSION");
    let next = current.split_once("-alpha.").map_or_else(
        || format!("{current}-alpha.0"),
        |(core, alpha)| format!("{core}-alpha.{}", alpha.parse::<u64>().unwrap() + 1),
    );
    let tag = format!("uf@{next}");

    // The published section, with a summary a person wrote by hand.
    let published = format!(
        "# Changelog\n\n## {tag}\n\n_2026-09-07_\n\nThe release that made the published tool work.\n\n### Fixed\n\n- **host**: the file `@uniflowed/host` exports is one it publishes (#410)\n"
    );
    fs::write(root.join("CHANGELOG.md"), &published).unwrap();
    git(&["add", "-A"]);
    git(&["commit", "--quiet", "-m", "chore(release): the notes"]);
    git(&["tag", &tag]);

    fs::write(root.join("b.txt"), "two\n").unwrap();
    git(&["add", "-A"]);
    git(&[
        "commit",
        "--quiet",
        "-m",
        "fix(fmt): a later release's commit",
    ]);

    for args in [
        vec!["release", "alpha"],
        // `--force` does not cover a version that has been tagged.
        vec!["release", "alpha", "--force"],
    ] {
        let output = uf().arg("--cwd").arg(root).args(&args).output().unwrap();
        assert!(!output.status.success(), "{args:?} was allowed");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains(&format!("{tag} is already released")),
            "{stderr}"
        );
        assert!(stderr.contains(current), "{stderr}");
        assert!(stderr.contains("older than the tree"), "{stderr}");
        // And the published section is exactly as it was.
        similar_asserts::assert_eq!(
            fs::read_to_string(root.join("CHANGELOG.md")).unwrap(),
            published
        );
        assert!(
            !root.join(".uf/release.json").exists(),
            "a plan was written"
        );
    }
}

/// A directory with no git history still gets a release plan.
///
/// `uf release` is not only run inside this repository, and a missing
/// changelog is not a reason to refuse to cut a release.
#[test]
fn release_without_a_repository_still_writes_its_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["release", "alpha"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("planned"), "{stdout}");
    assert!(!stdout.contains("changelog"), "{stdout}");
    assert!(!dir.path().join("CHANGELOG.md").exists());
    assert!(dir.path().join(".uf/release.json").exists());
}
