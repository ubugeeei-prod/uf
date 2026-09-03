//! Shared helpers for the CLI integration tests.
//!
//! Every command is launched with a scrubbed terminal environment. Colour and
//! glyph selection reads `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR`, `TERM`, and the
//! locale, so a developer running the suite with `NO_COLOR=1` exported would
//! otherwise see different output from CI.
#![allow(dead_code)]

use assert_cmd::Command;

/// Path to the built `uf` binary, for callers that drive its stdio directly.
pub fn uf_path() -> &'static str {
    env!("CARGO_BIN_EXE_uf")
}

/// A `uf` invocation with a known terminal environment.
pub fn uf() -> Command {
    binary("uf")
}

/// One of the alias binaries, `ufr` or `ufx`.
pub fn binary(name: &str) -> Command {
    let mut command = Command::cargo_bin(name).unwrap();
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

/// A throwaway uf project inside this repository's workspace.
///
/// A project that imports `@uniflowed/test` has to sit under a directory with
/// `node_modules` above it, because that is how every JavaScript host resolves
/// a bare specifier. A system temp directory has none, so these fixtures live
/// under the repository's own `.uf/`, which is git-ignored and which
/// `uf_project` always excludes from discovery.
pub struct Project {
    root: std::path::PathBuf,
}

impl Project {
    /// Create a project holding `files`, as `(relative path, source)` pairs.
    pub fn new(files: &[(&str, &str)]) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);

        let root = repo_root().join(".uf/test-projects").join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("uf.config.js"),
            "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\nexport default defineConfig({});\n",
        )
        .unwrap();
        let project = Self { root };
        for (name, source) in files {
            project.write(name, source);
        }
        project
    }

    /// Write one more file into the project.
    pub fn write(&self, name: &str, source: &str) {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }

    /// The project root.
    pub fn path(&self) -> &std::path::Path {
        &self.root
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// This repository's root, from the crate manifest.
pub fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate has a parent")
}

/// Whether tests that execute JavaScript can run here.
///
/// They need Node on PATH and the workspace installed, and they say so when
/// they skip: a silently reduced suite is how a runner starts lying.
pub fn host_ready() -> bool {
    let node = std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    let installed = repo_root()
        .join("node_modules/@uniflowed/test/worker.js")
        .is_file();
    if !node || !installed {
        eprintln!("skipping: `uf test` needs `node` on PATH and `npm ci` at the workspace root");
    }
    node && installed
}
