//! `uf build` and `uf dev` end to end, through Vite on the real driver.
//!
//! The fixture is this repository's own docs site: a uf project whose
//! `@uniflowed/*` dependencies resolve to `packages/` through the npm
//! workspace. Building it exercises everything a user's build does — Flow
//! through `uf transform`, the route table, the client and server bundles,
//! prerendering — with no mocks anywhere.
//!
//! The tests skip, loudly, when Node or the workspace's `node_modules` are
//! absent, so a checkout that never ran `npm ci` still passes `cargo test`
//! and a CI runner that forgot to will say so rather than silently cover less.

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use support::{assert_plain, uf, uf_path};

/// The repository's `docs/` directory.
fn docs_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

/// Whether the fixture can be built here: Node on PATH and the workspace
/// installed.
fn fixture_ready() -> bool {
    let node = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let installed = docs_root()
        .join("../node_modules/@uniflowed/vite/driver.js")
        .is_file();
    if !node || !installed {
        eprintln!(
            "skipping: the docs fixture needs `node` on PATH and `npm ci` at the workspace root"
        );
    }
    node && installed
}

#[test]
fn build_renders_the_docs_site_through_vite() {
    if !fixture_ready() {
        return;
    }
    let root = docs_root();

    let output = uf()
        .arg("--cwd")
        .arg(&root)
        .args(["build", "--size-report"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    for expected in [
        "uf build",
        "engine",
        "vite",
        "prerendered pages",
        "shipped",
        "gzip",
        "✓ build succeeded in",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in:\n{stdout}"
        );
    }
    for phase in [
        "config",
        "routes",
        "rsc analysis",
        "vite",
        "manifest",
        "bundle size",
        "total",
    ] {
        assert!(
            stdout.contains(phase),
            "missing phase {phase} in:\n{stdout}"
        );
    }

    let dist = root.join("dist/docs");
    let index = fs::read_to_string(dist.join("index.html")).expect("the home page is prerendered");
    assert!(index.starts_with("<!doctype html>"), "{index}");
    assert!(
        index.contains("<script type=\"module\" src=\"/assets/"),
        "no hydration script:\n{index}"
    );
    assert!(
        index.contains("Unified Toolchain for Flow"),
        "the page did not render:\n{index}"
    );
    assert!(
        !index.contains("component "),
        "Flow syntax leaked into the document"
    );

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dist.join("uf-build-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["engine"], serde_json::json!("vite"));
    assert_eq!(manifest["transform"], serde_json::json!("uf transform"));
    assert_eq!(manifest["pages"][0]["url"], serde_json::json!("/"));

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dist.join("uf-bundle-report.json")).unwrap())
            .unwrap();
    assert_eq!(report["version"], 1);
    let paths: Vec<&str> = report["assets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|asset| asset["path"].as_str().unwrap())
        .collect();
    assert!(paths.iter().any(|path| path.ends_with(".js")), "{paths:?}");
    assert!(
        paths.iter().any(|path| path.ends_with("index.html")),
        "{paths:?}"
    );
    assert!(
        !paths
            .iter()
            .any(|path| path.ends_with("uf-bundle-report.json")),
        "{paths:?}"
    );
    assert!(root.join("router.js").exists());
}

/// A dev server that must not outlive the test.
struct DevServer(Child);

impl Drop for DevServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn dev_serves_the_docs_site_through_vite() {
    if !fixture_ready() {
        return;
    }
    let root = docs_root();

    let mut child = Command::new(uf_path())
        .arg("--cwd")
        .arg(&root)
        .args(["dev", "--port", "0"])
        .env_remove("NO_COLOR")
        .env("TERM", "xterm-256color")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let server = DevServer(child);

    // Read the banner until the server reports where it listens.
    let mut lines = BufReader::new(stdout).lines();
    let mut url = None;
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        let Some(Ok(line)) = lines.next() else {
            break;
        };
        if let Some(rest) = line.trim().strip_prefix("local") {
            url = Some(rest.trim().to_owned());
        }
        if line.contains("dev server ready") {
            break;
        }
    }
    let url = url.expect("the dev server reported a local URL");
    let (host, port) = {
        let without_scheme = url.trim_start_matches("http://").trim_end_matches('/');
        let (host, port) = without_scheme.rsplit_once(':').unwrap();
        (host.to_owned(), port.parse::<u16>().unwrap())
    };

    let body = http_get(&host, port, "/");
    assert!(body.starts_with("HTTP/1.1 200"), "{body}");
    assert!(body.contains("<!doctype html>"), "{body}");
    assert!(
        body.contains("Unified Toolchain for Flow"),
        "the page did not render:\n{body}"
    );
    assert!(
        body.contains("/@vite/client"),
        "Vite's client was not injected:\n{body}"
    );
    assert!(
        body.contains("@react-refresh"),
        "the refresh preamble was not injected:\n{body}"
    );

    drop(server);
}

/// One plain HTTP/1.1 request, so the test depends on nothing but the server.
fn http_get(host: &str, port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect((host, port)).expect("connect to the dev server");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: text/html\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}
