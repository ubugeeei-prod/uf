//! Shared helpers for the CLI integration tests.
//!
//! Every command is launched with a scrubbed terminal environment. Colour
//! selection reads `NO_COLOR`, `FORCE_COLOR`, `CLICOLOR` and `TERM`, and glyph
//! selection reads `TERM` and the locale — `NO_COLOR` is not among the second
//! set, which is ubugeeei-prod/uf#393 — so a developer running the suite with
//! any of them exported would otherwise see different output from CI.
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

/// The command `uf test` would build to run workers for a project at `root`.
///
/// The installed `@uniflowed/test/worker.js`, the Node loader that transforms
/// Flow on import, and *this* build of `uf` as the transform behind it — the
/// same three the command assembles, so a test that drives a worker directly
/// drives the one a real run uses rather than a stand-in for it.
///
/// For the tests whose subject is the worker protocol, or what one worker
/// carries from one file to the next: neither is observable through `uf test`,
/// which fans files across workers by size and reuses them in an order the
/// schedule decides.
pub fn worker_command(root: &std::path::Path) -> uf_test::HostCommand {
    let root = camino::Utf8PathBuf::from_path_buf(root.to_path_buf()).expect("a UTF-8 path");
    let modules =
        camino::Utf8PathBuf::from_path_buf(repo_root().join("node_modules")).expect("a UTF-8 path");
    let host = modules.join("@uniflowed/host");
    uf_test::HostCommand::new(
        uf_test::HostKind::Node,
        camino::Utf8PathBuf::from("node"),
        modules.join("@uniflowed/test/worker.js"),
        root,
    )
    .with_flow_loader(
        camino::Utf8Path::new("@uniflowed/host/register"),
        &host.join("bun-preload.js"),
    )
    .with_uf_binary(camino::Utf8PathBuf::from(uf_path()))
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

/// Whether a test that runs Bun can run here: `bun` on PATH.
///
/// The same policy as [`host_ready`] and for the same reason, one step
/// stricter: this asserts rather than returning quietly, because the two
/// things that need Bun — `uf build --compile`, which embeds Bun's runtime,
/// and `tests/bun_host.rs`, which is the only place any Bun claim uf makes is
/// checked — are both of the kind that reads exactly like a green run when it
/// skips. Set `UF_ALLOW_FIXTURE_SKIP=1` to opt out on a machine that genuinely
/// has no Bun; CI sets nothing and so can never skip.
pub fn bun_ready() -> bool {
    if std::process::Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "this test needs `bun` on PATH and there is none, so it would prove nothing"
    );
    eprintln!("skipping: `bun` is not on PATH");
    false
}

/// The Node `crates/uf_cli/src/commands/compile.rs` refuses anything below.
pub const NODE_SEA_FLOOR: (u64, u64, u64) = (25, 5, 0);

/// The `node` on PATH, as it spells itself and as three numbers.
///
/// `None` when there is none. The numbers are what a comparison against
/// [`NODE_SEA_FLOOR`] needs, and the string is what a skipped test prints —
/// two forms of one answer, because a message saying `(25, 8, 1)` would be the
/// test telling a reader about its own arithmetic.
pub fn node_version() -> Option<(String, (u64, u64, u64))> {
    let output = std::process::Command::new("node")
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let numbers: Vec<u64> = version
        .trim_start_matches('v')
        .split(['.', '-'])
        .filter_map(|part| part.parse().ok())
        .collect();
    Some((
        version,
        (
            numbers.first().copied().unwrap_or_default(),
            numbers.get(1).copied().unwrap_or_default(),
            numbers.get(2).copied().unwrap_or_default(),
        ),
    ))
}

/// Whether the `node` on PATH can build a single-executable application.
///
/// Three questions, and only the first is a misconfigured machine:
///
/// * **`node` is not on PATH.** That is the same failure [`bun_ready`] guards,
///   and it asserts on the same terms — a checkout with no Node is a checkout
///   where most of this suite proves nothing.
/// * **`node` is older than 25.5.** `--build-sea` landed there, and before it
///   the procedure needed `postject` installed into the project. A skip here
///   is not a machine that was set up wrong, it is a runtime that cannot do
///   the thing, so it needs no opt-out — and CI's Node 24 is exactly this
///   case. `crates/uf_cli/src/commands/compile.rs` refuses the same version by
///   name, which is what a project on it is actually told.
/// * **`node` was built without single-executable support.** This is a
///   *build-time* option, so a recent Node can still lack it: Homebrew's
///   `node@25.8.1` answers `--build-sea` with `sentinel NODE_SEA_FUSE_… not
///   found`. The only cheap way to ask is to look for the fuse in the binary,
///   which is what this does — a hundred-megabyte read once per test run, and
///   the alternative is a red test on a machine that was never able to pass
///   it. uf itself deliberately does *not* do this: keying a build on a string
///   inside somebody else's binary is a coupling a toolchain should not have,
///   and forwarding Node's own message costs a reader nothing.
pub fn node_sea_ready() -> bool {
    let Some((version, numbers)) = node_version() else {
        assert!(
            std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
            "this test needs `node` on PATH and there is none, so it would prove nothing"
        );
        eprintln!("skipping: `node` is not on PATH");
        return false;
    };
    if numbers < NODE_SEA_FLOOR {
        eprintln!("skipping: node {version} is older than 25.5, which is where `--build-sea` is");
        return false;
    }
    let Ok(program) = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
    else {
        eprintln!("skipping: node {version} would not say where it is");
        return false;
    };
    let program = String::from_utf8_lossy(&program.stdout).trim().to_owned();
    let Ok(bytes) = std::fs::read(&program) else {
        eprintln!("skipping: {program} could not be read");
        return false;
    };
    if bytes
        .windows(b"NODE_SEA_FUSE".len())
        .any(|window| window == b"NODE_SEA_FUSE")
    {
        return true;
    }
    eprintln!(
        "skipping: node {version} at {program} was built without single-executable support, so \
         `--build-sea` cannot work here"
    );
    false
}

/// Whether a test that runs Deno can run here: `deno` on PATH.
///
/// The same policy as [`bun_ready`], word for word, and for a reason that is
/// almost the opposite. Bun's tests check that a host uf claims to support
/// works; Deno's check the *boundary* — which of uf's parts run there, which do
/// not, and that the permission set uf translates is one Deno actually
/// enforces. A skip there is worse than a skip elsewhere, because
/// `uf_runtime::HostSupport`'s Deno row names this file as the thing that keeps
/// its claims honest, and a row backed by a test nobody ran is the state of
/// affairs the table exists to end.
///
/// Set `UF_ALLOW_FIXTURE_SKIP=1` on a machine that genuinely has no Deno; CI
/// sets nothing and so can never skip.
pub fn deno_ready() -> bool {
    if std::process::Command::new("deno")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "this test needs `deno` on PATH and there is none, so it would prove nothing"
    );
    eprintln!("skipping: `deno` is not on PATH");
    false
}
