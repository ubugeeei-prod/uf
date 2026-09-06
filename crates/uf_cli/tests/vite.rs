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
use std::sync::Mutex;
use std::time::{Duration, Instant};

use support::{assert_plain, uf, uf_path};

/// The repository's `docs/` directory.
fn docs_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

/// Whether the fixture can be built here: Node on PATH and the workspace
/// installed.
///
/// A missing fixture is a failure, not a skip. These two tests are the only
/// thing standing between a broken dev server or build and a release, and when
/// they skipped themselves they did it silently — cargo hides a passing test's
/// output, so "1 passed" was printed for a `uf dev` that answered every request
/// with "Cannot GET /". Set `UF_ALLOW_FIXTURE_SKIP=1` to opt out on a machine
/// that genuinely cannot run them; CI sets nothing and so can never skip.
fn fixture_ready() -> bool {
    let mut missing = Vec::new();
    if !Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        missing.push("`node` is not on PATH".to_owned());
    }
    let driver = docs_root().join("../node_modules/@uniflowed/vite/driver.js");
    if !driver.is_file() {
        missing.push(format!("{} does not exist; run `npm ci`", driver.display()));
    }

    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "the docs fixture is not available, so this test would prove nothing: {}",
        missing.join("; ")
    );
    eprintln!("skipping: {}", missing.join("; "));
    false
}

/// The docs fixture has one output directory, and two tests build into it.
///
/// Cargo runs the tests in a file on threads of one process, so without this
/// they race: one `uf build` empties `dist/docs` while the other is reading
/// what it found there. The lock covers the build *and* the assertions, which
/// together are the only window in which that directory means anything.
static DIST: Mutex<()> = Mutex::new(());

/// Take [`DIST`], stepping over a poisoning left by an unrelated failure.
///
/// The panic that poisoned it has already been reported; turning it into a
/// second failure here would only bury the first one under this one.
fn dist_lock() -> std::sync::MutexGuard<'static, ()> {
    DIST.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn build_renders_the_docs_site_through_vite() {
    if !fixture_ready() {
        return;
    }
    let _dist = dist_lock();
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

/// Whether a loopback socket can be bound here.
///
/// The same policy as [`fixture_ready`], for the same reason: a sandbox that
/// refuses `bind` makes this test permanently red, and a suite with a known
/// failure in it teaches everyone to read `1 failed` as `0 failed` — which is
/// how a *genuinely* flaky one goes unnoticed. `UF_ALLOW_FIXTURE_SKIP=1` opts
/// out on such a machine; CI sets nothing and so can never skip.
fn loopback_ready() -> bool {
    match std::net::TcpListener::bind(("127.0.0.1", 0)) {
        Ok(_) => true,
        Err(error) => {
            assert!(
                std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
                "this test needs a loopback socket and could not bind one: {error}"
            );
            eprintln!("skipping: cannot bind a loopback socket: {error}");
            false
        }
    }
}

/// A server that must not outlive the test, and says what it did.
///
/// Both streams are drained, by scoped threads borrowing the caller's buffer
/// rather than sharing one — `Arc` is a disallowed type here, and the reason
/// given for it ("prefer scoped references") is exactly this shape.
///
/// Draining is not only for the message: a piped stream nobody reads fills its
/// buffer and wedges the child, so leaving `stderr` unread was a way to cause
/// the failure as well as a way to be unable to explain it. The threads end
/// when the pipes close, which is when the child does — so the server has to be
/// dropped inside the scope, or the scope waits for a process nobody killed.
///
/// It drives `uf dev` and it drives a compiled binary. Neither test cares which
/// process is listening, only that something is and that it can be made to
/// explain itself when it is not.
struct TestServer {
    child: Child,
}

impl TestServer {
    /// Start `uf dev` on `port`, draining what it says into `said`.
    fn dev<'scope, 'env: 'scope>(
        root: &Path,
        port: u16,
        scope: &'scope std::thread::Scope<'scope, 'env>,
        said: &'env Mutex<String>,
    ) -> Self {
        let mut command = Command::new(uf_path());
        command
            .arg("--cwd")
            .arg(root)
            .args(["dev", "--port", &port.to_string()])
            .env_remove("NO_COLOR")
            .env("TERM", "xterm-256color");
        Self::spawn(command, scope, said)
    }

    /// Start an already-built server, draining what it says into `said`.
    fn spawn<'scope, 'env: 'scope>(
        mut command: Command,
        scope: &'scope std::thread::Scope<'scope, 'env>,
        said: &'env Mutex<String>,
    ) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();

        for stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            scope.spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let Ok(mut said) = said.lock() else { return };
                    said.push_str(&line);
                    said.push('\n');
                }
            });
        }

        Self { child }
    }

    /// Everything the server has said, and whether it is still running.
    ///
    /// This is the whole point of the change: the failure that sent me here was
    /// `ConnectionRefused` and nothing else — no exit status, no output — so
    /// the only way to act on it was to guess. See ubugeeei-prod/uf#234.
    fn evidence(&mut self, said: &Mutex<String>) -> String {
        let status = match self.child.try_wait() {
            Ok(Some(status)) => format!("the server exited: {status}"),
            Ok(None) => "the server is still running".to_owned(),
            Err(error) => format!("could not ask whether the server is running: {error}"),
        };
        let said = said
            .lock()
            .map_or_else(|_| "<the reader thread panicked>".to_owned(), |s| s.clone());
        format!("{status}\nwhat it said:\n{said}")
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// How many ports to try before giving up on getting one to ourselves.
///
/// The port is chosen by binding zero and letting the listener go, so between
/// choosing it and `uf dev` binding it, anything on the machine can take it —
/// and Vite's default is to move to the next free port rather than fail, so a
/// lost race is a server that is up somewhere this test is not asking about.
/// That is the shape of both CI failures so far: one where nothing ever
/// answered, and one where something answered and then went away.
///
/// Retrying is honest here because the subject is "the dev server serves the
/// docs site", not "binding a port works first time". It is capped, it only
/// covers the window *before* the first answer, and every attempt's output is
/// reported if the last one fails — so a genuinely broken dev server fails
/// three times and prints three servers' reasons, which is more than the one
/// line this used to give.
const PORT_ATTEMPTS: usize = 3;

#[test]
fn dev_serves_the_docs_site_through_vite() {
    if !fixture_ready() || !loopback_ready() {
        return;
    }
    let root = docs_root();
    let mut refused = Vec::new();

    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let said = Mutex::new(String::new());

        let served = std::thread::scope(|scope| {
            // Wait for the port to answer rather than for a line of the banner
            // to look a particular way. Parsing the rendered banner made this
            // test depend on colour and on the exact wording, and a parse that
            // quietly found nothing ended the test before it asserted anything
            // — which is how a dev server that answered every request with
            // "Cannot GET /" passed it.
            let mut server = TestServer::dev(&root, port, scope, &said);
            if let Some(body) = wait_for_http(port, "/", Duration::from_secs(90)) {
                assert_page(&mut server, port, &said, &body);
                return true;
            }
            refused.push(format!(
                "attempt {attempt} on port {port}: {}",
                server.evidence(&said)
            ));
            // Inside the scope on purpose: the drain threads end when the
            // pipes close, and the pipes close when the child does.
            drop(server);
            false
        });

        if served {
            return;
        }
    }

    panic!(
        "the dev server never answered, on {PORT_ATTEMPTS} different ports\n{}",
        refused.join("\n\n")
    );
}

/// Everything the served page and the routes have to be, once one is served.
fn assert_page(server: &mut TestServer, port: u16, said: &Mutex<String>, body: &str) {
    assert!(
        body.starts_with("HTTP/1.1 200"),
        "the dev server must render the page, not 404:\n{body}"
    );
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

    // A nested route proves the router ran, not just that something answered.
    let guide = get(server, port, "/guide/", said);
    assert!(guide.starts_with("HTTP/1.1 200"), "{guide}");
    assert!(guide.contains("What uf is"), "{guide}");

    // And a path with no route must not be answered with somebody else's page.
    let missing = get(server, port, "/definitely-not-a-page/", said);
    assert!(
        missing.starts_with("HTTP/1.1 404"),
        "an unrouted path must be a 404:\n{missing}"
    );
}

/// One request to a server that has already answered once, with the server's
/// own account of itself if it will not answer this time.
///
/// The failure in ubugeeei-prod/uf#234 was here: `/` was served and then the
/// port stopped listening, and `http_get`'s bare `expect` reported
/// `ConnectionRefused` and nothing about the process that had refused it.
fn get(server: &mut TestServer, port: u16, path: &str, said: &Mutex<String>) -> String {
    if TcpStream::connect(("127.0.0.1", port)).is_err() {
        panic!(
            "the dev server answered `/` and then stopped listening, before {path}\n{}",
            server.evidence(said)
        );
    }
    http_get("127.0.0.1", port, path)
}

/// A port nothing is listening on, released before the server binds it.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

/// Poll until the server answers, or give up.
fn wait_for_http(port: u16, path: &str, budget: Duration) -> Option<String> {
    let started = Instant::now();
    while started.elapsed() < budget {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            let body = http_get("127.0.0.1", port, path);
            if !body.is_empty() {
                return Some(body);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
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

/// Whether a standalone binary can be produced here: Bun on PATH.
///
/// The same policy as [`fixture_ready`] and for the same reason. `uf build
/// --compile` embeds Bun's runtime, so a machine without `bun` cannot produce
/// one — and a test that quietly passed on such a machine would be the second
/// way this repository has learned that a silent skip reads exactly like a
/// green run.
fn bun_ready() -> bool {
    if Command::new("bun")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "`uf build --compile` needs `bun` on PATH and there is none, so this test would \
         prove nothing"
    );
    eprintln!("skipping: `bun` is not on PATH");
    false
}

/// The whole claim, end to end: one file, an empty directory, a served page.
///
/// Nothing about this test is a stand-in. It runs the real `uf build
/// --compile` on the real docs site, copies the *only* file it produced into a
/// directory that has nothing else in it — no `dist/`, no `node_modules`, not
/// even the project — starts it, and asks it for pages. That is the shape
/// `tools/release/test-install.sh` uses to prove the installer works from
/// nothing, and it is the only shape in which "runs anywhere" is a claim
/// rather than a hope.
///
/// It is written to prove as much as the machine allows. A sandbox that
/// refuses `bind` cannot host the request half — but it can still host the
/// half that matters most for a *binary*, which is whether the file carries
/// the site at all, and that half runs unconditionally. What the requests add
/// on top is covered without a socket by `tests/library/standalone.test.js`,
/// which drives the same handler directly.
#[test]
fn compile_writes_one_file_that_serves_the_site_from_an_empty_directory() {
    if !fixture_ready() || !bun_ready() {
        return;
    }
    let _dist = dist_lock();
    let root = docs_root();

    let output = uf()
        .arg("--cwd")
        .arg(&root)
        .args(["build", "--compile"])
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
    for expected in ["standalone", "binary", "bun", "✓ build succeeded in"] {
        assert!(
            stdout.contains(expected),
            "missing {expected:?} in:\n{stdout}"
        );
    }
    let embedded = summary_value(&stdout, "embedded assets");
    assert!(
        embedded.parse::<u32>().is_ok_and(|count| count > 0),
        "the summary must report how many assets went in, not {embedded:?}:\n{stdout}"
    );

    // Copied out rather than run in place, because running it in place would
    // prove nothing: `dist/`, `node_modules` and the source are all still
    // there, and a binary quietly reading one of them would pass.
    let empty = tempfile::tempdir().unwrap();
    let binary = empty.path().join("docs");
    fs::copy(root.join("dist/docs/docs"), &binary).expect("`--compile` writes dist/docs/docs");
    assert_eq!(
        fs::read_dir(empty.path()).unwrap().count(),
        1,
        "the directory must hold the binary and nothing else"
    );

    // The line the binary prints before it takes a socket. Matching it against
    // the number the *build* reported is what proves the embedded copy of
    // `dist/` survived the link — from a directory where no copy of `dist/`
    // exists to be found by accident.
    let inventory = format!("uf: {embedded} embedded files");

    if !loopback_ready() {
        let said = run_briefly(&binary, empty.path());
        assert!(
            said.contains(&inventory),
            "the binary must carry all {embedded} assets, and said:\n{said}"
        );
        return;
    }

    let mut refused = Vec::new();
    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let said = Mutex::new(String::new());

        let served = std::thread::scope(|scope| {
            let mut command = Command::new(&binary);
            command
                .current_dir(empty.path())
                .args(["--port", &port.to_string()]);
            let mut server = TestServer::spawn(command, scope, &said);
            if let Some(body) = wait_for_http(port, "/", Duration::from_secs(60)) {
                assert_compiled_site(&mut server, port, &said, &body);
                let said = said.lock().unwrap();
                assert!(
                    said.contains(&inventory),
                    "the binary must carry all {embedded} assets, and said:\n{said}"
                );
                return true;
            }
            refused.push(format!(
                "attempt {attempt} on port {port}: {}",
                server.evidence(&said)
            ));
            drop(server);
            false
        });

        if served {
            return;
        }
    }

    panic!(
        "the compiled binary never answered, on {PORT_ATTEMPTS} different ports\n{}",
        refused.join("\n\n")
    );
}

/// The value beside `key` in the build summary's aligned key/value block.
fn summary_value(stdout: &str, key: &str) -> String {
    stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix(key))
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| panic!("no {key:?} in the summary:\n{stdout}"))
}

/// Start the binary, let it say what it holds, and stop it.
///
/// For the machine that cannot bind: the process gets far enough to print its
/// inventory and then fails on the socket, and the inventory is the thing
/// being read. Everything it says is returned, including the failure, because
/// a binary that died for some *other* reason must not look like a pass.
fn run_briefly(binary: &Path, cwd: &Path) -> String {
    let said = Mutex::new(String::new());
    std::thread::scope(|scope| {
        let mut command = Command::new(binary);
        command.current_dir(cwd).args(["--port", "0"]);
        let mut server = TestServer::spawn(command, scope, &said);
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if said
                .lock()
                .is_ok_and(|said| said.contains("embedded files"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let evidence = server.evidence(&said);
        drop(server);
        evidence
    })
}

/// Everything the binary has to serve, once it is listening.
///
/// Four things, and each one is a different part of the file: the prerendered
/// document, an embedded asset, a route the router has to resolve, and a path
/// that must not be answered with somebody else's page.
fn assert_compiled_site(server: &mut TestServer, port: u16, said: &Mutex<String>, body: &str) {
    assert!(
        body.starts_with("HTTP/1.1 200"),
        "the binary must serve the home page:\n{body}"
    );
    assert!(body.contains("<!doctype html>"), "{body}");
    assert!(
        body.contains("Unified Toolchain for Flow"),
        "the page did not render:\n{body}"
    );
    assert!(
        !body.contains("/@vite/client"),
        "a compiled binary must serve the production document, not the dev one:\n{body}"
    );

    // The hydration script, fetched from the binary. This is the assertion that
    // says the embedded copy of `dist/` is really in there and reachable: the
    // file exists nowhere on this machine but inside the executable.
    let script = body
        .split_once("<script type=\"module\" src=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(url, _)| url.to_owned())
        .unwrap_or_else(|| panic!("no hydration script in the document:\n{body}"));
    let asset = get(server, port, &script, said);
    let head = &asset[..asset.len().min(400)];
    assert!(
        asset.starts_with("HTTP/1.1 200"),
        "the embedded asset {script} was not served:\n{head}"
    );
    assert!(
        asset.contains("text/javascript"),
        "the embedded asset {script} was served with the wrong type:\n{head}"
    );

    // A nested route proves the router ran, not just that a file was found.
    let guide = get(server, port, "/guide/", said);
    assert!(guide.starts_with("HTTP/1.1 200"), "{guide}");
    assert!(guide.contains("What uf is"), "{guide}");

    let missing = get(server, port, "/definitely-not-a-page/", said);
    assert!(
        missing.starts_with("HTTP/1.1 404"),
        "an unrouted path must be a 404:\n{missing}"
    );
}

/// A project that cannot be one file, written out so the build can refuse it.
///
/// The arrangement matters more than the addon does, and it took a wrong guess
/// to find the right one. The first version imported the addon from a route
/// handler and expected the ordinary build to be untroubled by it, on the
/// reasoning that a handler is server-only. It is not: the generated route
/// table lists handlers beside pages, so the *client* build resolves a
/// handler's imports even though it tree-shakes them back out — and the plain
/// build failed too, which would have made the guard below untestable and the
/// claim about it untrue.
///
/// So the addon is behind a package that ships a browser build, which is how
/// every real native dependency is packaged. The `browser` condition gives the
/// client bundle a shim, the ordinary SSR build leaves the bare import alone,
/// and only the standalone link — which has to resolve everything for real —
/// ever reaches the `.node` file. That is exactly the project that builds
/// today and cannot become one file, which is the project this guard exists
/// for.
///
/// It lives under `CARGO_TARGET_TMPDIR` rather than in a system temporary
/// directory because module resolution has to be able to walk up to this
/// repository's `node_modules` for `@uniflowed/*`, `react` and `react-dom`. A
/// project outside the tree would fail for want of dependencies, and would
/// prove nothing about native addons.
fn native_addon_project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-addon");
    let _ = fs::remove_dir_all(&root);

    let write = |path: &str, contents: &str| {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, contents).unwrap();
    };

    write(
        "package.json",
        r#"{
  "name": "native-addon-app",
  "private": true,
  "type": "module",
  "dependencies": { "fake-native": "1.0.0" }
}
"#,
    );
    write(
        "uf.config.js",
        r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: { router: { entry: "app.js", root: "app" } },
  build: { entries: ["app.js"], outDir: "dist" },
});
"#,
    );
    write(
        "app.js",
        r#"// @flow
import { routerView } from "@uniflowed/router";

export default routerView("./app");
"#,
    );
    write(
        "app/_uf.layout.js",
        r#"// @flow
import * as React from "@uniflowed/react";

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
      </head>
      <body>{children}</body>
    </html>
  );
}
"#,
    );
    write(
        "app/_uf.page.js",
        r#"// @flow
import * as React from "@uniflowed/react";

export default component Home() {
  return <p>a project with a native addon</p>;
}
"#,
    );
    write(
        "app/api/_uf.route.js",
        r#"// @flow
import { reading } from "fake-native";

export function GET(): Response {
  return new Response(String(reading()));
}
"#,
    );

    // The dependency, packaged the way a native one really is: a browser build
    // for bundlers that cannot load a shared object, and a Node build that
    // reaches for it. Nothing in this fixture is contrived except the addon's
    // emptiness.
    write(
        "node_modules/fake-native/package.json",
        r#"{
  "name": "fake-native",
  "version": "1.0.0",
  "type": "module",
  "exports": {
    ".": {
      "browser": "./browser.js",
      "default": "./index.js"
    }
  }
}
"#,
    );
    write(
        "node_modules/fake-native/index.js",
        "import bindings from \"./sensor.node\";\n\n         export function reading() {\n  return bindings.read();\n}\n",
    );
    write(
        "node_modules/fake-native/browser.js",
        "export function reading() {\n  return 0;\n}\n",
    );
    // Empty on purpose. The build must refuse it on sight, without reading it:
    // a real `.node` file is a shared object for one platform, and nothing
    // about this test should depend on having one.
    write("node_modules/fake-native/sensor.node", "");

    root
}

/// A project that cannot be one file is told which dependency made it so.
///
/// This is the difference between a feature and a trap. Without it the build
/// either fails somewhere inside the bundler with a complaint about an
/// unexpected character, or — worse — succeeds and produces a binary that dies
/// on the first request that reaches the addon. Naming the file and the
/// importer at build time is what makes `--compile` safe to reach for.
#[test]
fn compile_refuses_a_native_addon_and_names_it() {
    if !fixture_ready() || !bun_ready() {
        return;
    }
    let root = native_addon_project();

    let output = uf()
        .arg("--cwd")
        .arg(&root)
        .args(["build", "--compile"])
        .output()
        .unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !output.status.success(),
        "a project with a native addon must not compile to one file:\n{said}"
    );
    assert!(
        said.contains("sensor.node"),
        "the build must name the addon it cannot embed:\n{said}"
    );
    assert!(
        said.contains("single executable"),
        "the build must say what it was unable to do:\n{said}"
    );

    // The ordinary build still works: a native addon is a limit of `--compile`
    // and not a limit of uf, and the message says so by telling the user what
    // to do instead. If this ever fails, the guard has started rejecting
    // projects that were fine.
    let plain = uf().arg("--cwd").arg(&root).arg("build").output().unwrap();
    assert!(
        plain.status.success(),
        "the same project must still build without `--compile`:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&plain.stdout),
        String::from_utf8_lossy(&plain.stderr)
    );
}
