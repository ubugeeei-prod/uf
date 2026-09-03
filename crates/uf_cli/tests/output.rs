//! The rules the output layer must never break: `--json` stays machine
//! readable, colour obeys the flag and the environment, and the two streams
//! keep their jobs.

mod support;

use std::fs;

use support::{assert_plain, create_app, uf};

/// A project with one file the linter has something to say about.
fn lint_project(dir: &std::path::Path) {
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("props.js"),
        "// @flow\ntype Props = { id: string };\nexport const read = (p: Props) => p.id;\n",
    )
    .unwrap();
}

#[test]
fn inspect_json_is_pure_json_even_with_color_forced_on() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["inspect", "--json", "--color", "always"])
        .env("FORCE_COLOR", "3")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    assert!(
        stdout.starts_with('{'),
        "a banner leaked into --json output"
    );
    serde_json::from_str::<serde_json::Value>(&stdout).expect("--json output must parse");
}

#[test]
fn lint_json_is_pure_json_even_with_color_forced_on() {
    let dir = tempfile::tempdir().unwrap();
    lint_project(dir.path());

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["lint", "--json", "--color", "always"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("--json must parse");
    assert_eq!(value["command"], serde_json::json!("uf lint"));
    assert!(value["filesChecked"].as_u64().unwrap() >= 1);
    assert!(value["diagnostics"].is_array());
    assert!(value["unavailableRules"].is_array());
}

#[test]
fn check_json_reports_the_same_shape_under_its_own_name() {
    let dir = tempfile::tempdir().unwrap();
    lint_project(dir.path());

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["check", "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["command"], serde_json::json!("uf check"));
    assert!(value["errors"].is_number());
    assert!(value["warnings"].is_number());
}

#[test]
fn a_failing_json_run_still_writes_only_json_to_stdout() {
    let dir = tempfile::tempdir().unwrap();
    lint_project(dir.path());

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["lint", "--json"])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    serde_json::from_str::<serde_json::Value>(&stdout).expect("--json must parse when lint fails");
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.starts_with("error: "));
    }
}

#[test]
fn color_always_writes_escape_sequences_to_a_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    // `uf lint` runs entirely in this process, so the test needs no
    // JavaScript host or installed packages.
    let output = uf()
        .arg("--cwd")
        .arg(&app)
        .args(["lint", "--color", "always"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains('\u{1b}'),
        "colour was requested but not used"
    );
    assert!(stdout.contains("\u{1b}[0m"), "styles must be reset");
    // Styling must not change what the output says.
    assert!(stdout.contains("uf lint"));
}

#[test]
fn color_never_writes_no_escape_sequences() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf()
        .arg("--cwd")
        .arg(&app)
        .args(["lint", "--color", "never"])
        .env("FORCE_COLOR", "3")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_plain(&String::from_utf8(output.stdout).unwrap());
}

#[test]
fn info_renders_the_brand_system() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["info", "--color", "never"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    assert!(stdout.contains("Unified Toolchain for Flow"), "{stdout}");
    assert!(stdout.contains("--uf-color-cyan-500"), "{stdout}");
    assert!(
        stdout.contains("curl -fsSL https://setup.uniflowed.dev | sh"),
        "{stdout}"
    );
    assert!(
        stdout.contains("nix profile install github:ubugeeei-prod/uf#uf"),
        "{stdout}"
    );
}

#[test]
fn install_renders_the_brand_header() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf()
        .arg("--cwd")
        .arg(&app)
        .args(["install", "--color", "never"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    assert!(stdout.contains("Unified Toolchain for Flow"), "{stdout}");
    assert!(stdout.contains("uf install"), "{stdout}");
    assert!(stdout.contains("store entries"), "{stdout}");
}

#[test]
fn color_is_off_by_default_when_stdout_is_a_pipe() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("prepare")
        .output()
        .unwrap();

    assert_plain(&String::from_utf8(output.stdout).unwrap());
}

#[test]
fn force_color_enables_color_on_a_pipe() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("prepare")
        .env("FORCE_COLOR", "1")
        .output()
        .unwrap();

    assert!(String::from_utf8(output.stdout).unwrap().contains('\u{1b}'));
}

#[test]
fn no_color_beats_clicolor_force() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("prepare")
        .env("NO_COLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .output()
        .unwrap();

    assert_plain(&String::from_utf8(output.stdout).unwrap());
}

#[test]
fn no_color_also_falls_back_to_ascii_glyphs() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");

    let output = uf()
        .args(["create", "app", "react"])
        .arg(&app)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.is_ascii(), "NO_COLOR must not print box drawing");
    assert!(stdout.contains("|- app"));
    assert!(stdout.contains("`- uf.config.js"));
    assert!(stdout.contains("+ created 12 files"));
}

#[test]
fn term_dumb_falls_back_to_ascii_glyphs() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");

    let output = uf()
        .args(["create", "app", "react"])
        .arg(&app)
        .env("TERM", "dumb")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.is_ascii(), "TERM=dumb must not print box drawing");
    assert_plain(&stdout);
}

#[test]
fn a_non_utf8_locale_falls_back_to_ascii_glyphs() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");

    let output = uf()
        .args(["create", "app", "react", "--color", "always"])
        .arg(&app)
        .env("LC_ALL", "C")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains('\u{1b}'),
        "the locale must not disable colour"
    );
    let plain: String = stdout
        .split('\u{1b}')
        .map(|chunk| chunk.split_once('m').map_or(chunk, |(_, rest)| rest))
        .collect();
    assert!(
        plain.is_ascii(),
        "a non-UTF-8 locale must not get box drawing"
    );
}

#[test]
fn a_successful_run_writes_nothing_to_stderr() {
    let dir = tempfile::tempdir().unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("prepare")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "progress must stay silent when stderr is not a terminal: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn progress_control_characters_never_reach_a_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("app");
    create_app(&app);

    let output = uf().arg("--cwd").arg(&app).arg("build").output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(
        !stdout.contains('\r'),
        "no carriage returns in piped stdout"
    );
    assert!(
        !stderr.contains('\r'),
        "no carriage returns in piped stderr"
    );
    assert!(
        !stderr.contains("\u{1b}[?25l"),
        "the cursor is never hidden"
    );
}

#[test]
fn an_unknown_color_value_is_rejected() {
    let output = uf()
        .args(["build", "--color", "chartreuse"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("chartreuse")
    );
}
