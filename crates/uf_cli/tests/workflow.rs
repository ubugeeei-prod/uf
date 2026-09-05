//! The commands that run tests, install packages, and write release plans.

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
fn prepare_prints_lint_staged_and_codegen_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("prepare")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("lint-staged compatible  yes"));
    assert!(stdout.contains("GenerateRouterTypes"));
    assert!(stdout.contains("GenerateValidatorTypes"));
    assert!(stdout.contains("✓ prepare plan written"));
    assert!(dir.path().join(".uf/prepare.json").exists());
}

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

#[test]
fn upgrade_reports_package_and_runtime_manager_plan() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("upgrade")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("package resolver  UfNative"));
    assert!(stdout.contains("runtime engine    Node"));
    assert!(stdout.contains("acquisition       Auto"));
    assert!(stdout.contains("upgrade.json"));
    assert!(stdout.contains("✓ workspace upgraded"));
    assert!(dir.path().join("uf.lock").exists());
    assert!(dir.path().join(".uf/upgrade.json").exists());
}

#[test]
fn use_reports_xdg_runtime_switch_plan() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let config_home = dir.path().join("xdg-config");
    let data_home = dir.path().join("xdg-data");
    let cache_home = dir.path().join("xdg-cache");
    let state_home = dir.path().join("xdg-state");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["use", "uf@0.1.0"])
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("XDG_STATE_HOME", &state_home)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("uf use \u{b7} uf@0.1.0"), "{stdout}");
    assert!(stdout.contains("auto switch  enabled"));
    assert!(stdout.contains(".local/bin/uf"));
    assert!(stdout.contains("active-runtime.json"));
    assert!(stdout.contains("runtime.json"));
    assert!(stdout.contains("WriteShim"));
    assert!(stdout.contains("ActivateVersion"));
    assert!(stdout.contains("✓ now using uf@0.1.0"));
    assert!(
        data_home
            .join("uniflowed/runtimes/uf/0.1.0/bin/uf")
            .exists()
    );
    assert!(
        data_home
            .join("uniflowed/runtimes/uf/0.1.0/runtime.json")
            .exists()
    );
    assert!(state_home.join("uniflowed/active-runtime.json").exists());
    assert!(home.join(".local/bin/uf").exists());
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

    // Cutting the same release again replaces the section rather than
    // stacking a second one.
    let again = uf()
        .arg("--cwd")
        .arg(root)
        .args(["release", "alpha"])
        .output()
        .unwrap();
    assert!(again.status.success());
    let twice = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    similar_asserts::assert_eq!(twice, changelog);
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
