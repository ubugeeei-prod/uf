//! Shared helpers for the CLI integration tests.
//!
//! Every command is launched with a scrubbed terminal environment. Colour and
//! glyph selection reads `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR`, `TERM`, and the
//! locale, so a developer running the suite with `NO_COLOR=1` exported would
//! otherwise see different output from CI.
#![allow(dead_code)]

use assert_cmd::Command;

/// A `uf` invocation with a known terminal environment.
pub fn uf() -> Command {
    binary("uf")
}

/// One of the alias binaries, `ufr` or `ufx`.
///
/// The path comes from `CARGO_BIN_EXE_*`, which Cargo sets to the artifact it
/// guarantees is built before this test runs. `Command::cargo_bin` instead
/// guesses the path by convention, so it can hand back a binary that is still
/// being linked — which surfaces as an empty stdout and a non-zero exit that
/// looks like the tool failing rather than a race.
pub fn binary(name: &str) -> Command {
    let path = match name {
        "uf" => env!("CARGO_BIN_EXE_uf"),
        "ufr" => env!("CARGO_BIN_EXE_ufr"),
        "ufx" => env!("CARGO_BIN_EXE_ufx"),
        other => panic!("unknown uf binary: {other}"),
    };
    let mut command = Command::new(path);
    command
        .env_remove("NO_COLOR")
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env("TERM", "xterm-256color")
        .env("LC_ALL", "en_US.UTF-8");
    command
}

/// Assert that rendered text carries no ANSI escape sequences.
pub fn assert_plain(text: &str) {
    assert!(
        !text.contains('\u{1b}'),
        "expected plain text, found an escape sequence in:\n{text}"
    );
}

/// Create a React app in `path`, returning its stdout.
pub fn create_app(path: &std::path::Path) -> String {
    let output = uf()
        .args(["create", "app", "react"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
