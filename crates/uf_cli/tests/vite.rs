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

/// A project with one page that throws, built under `target/`.
///
/// Under `target/` on purpose rather than in a `tests/fixtures` directory:
/// `@uniflowed/*`, `react` and `react-dom` are resolved by walking up to the
/// workspace's `node_modules`, so the project has to be inside the repository
/// — and everything inside it that is not `target/` is linted, formatted and
/// scanned for tests by uf's own tasks, which would report this page's
/// deliberate throw as this repository's defect.
fn project_with_a_throwing_page() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/uf-tests/throwing-page");
    fs::remove_dir_all(&root).ok();
    for (relative, contents) in [
        (
            "package.json",
            r#"{ "name": "uf-throwing-page", "private": true, "type": "module" }
"#,
        ),
        (
            "uf.config.js",
            r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: { router: { entry: "app.js", root: "app" } },
  build: { entries: ["app.js"], outDir: "dist" },
});
"#,
        ),
        (
            "app.js",
            r#"// @flow
import { routerView } from "@uniflowed/router";

export default routerView("./app");
"#,
        ),
        (
            "app/_uf.page.js",
            r#"// @flow
export default component Home() {
  return <h1>the home page rendered</h1>;
}
"#,
        ),
        (
            "app/fine/_uf.page.js",
            r#"// @flow
export default component Fine() {
  return <h1>this page is fine</h1>;
}
"#,
        ),
        (
            "app/broken/_uf.page.js",
            r#"// @flow
export default component Broken() {
  throw new Error("this page throws on purpose");
}
"#,
        ),
    ] {
        let file = root.join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, contents).unwrap();
    }
    root
}

/// A page that throws fails its own route, and nothing else.
///
/// Three assertions because they are one behaviour: the build fails, it says
/// which URL threw, and the routes that did render are still written. Before
/// this the prerender loop had no `try` — the first page to throw rejected out
/// of the driver, the message named the exception rather than the route, and
/// no page after it was written. See ubugeeei-prod/uf#257.
#[test]
fn a_page_that_throws_fails_its_route_and_not_the_others() {
    if !fixture_ready() {
        return;
    }
    let root = project_with_a_throwing_page();

    let output = uf().arg("--cwd").arg(&root).arg("build").output().unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let said = format!("{stdout}{stderr}");
    assert!(
        !output.status.success(),
        "a route that throws must fail the build:\n{said}"
    );
    assert!(
        said.contains("/broken"),
        "the build must name the route that threw:\n{said}"
    );
    assert!(
        said.contains("this page throws on purpose"),
        "the build must say why the route failed:\n{said}"
    );

    // The other routes are still written: one broken page is one broken page.
    let dist = root.join("dist");
    assert!(
        dist.join("index.html").is_file(),
        "the home page was not written:\n{said}"
    );
    assert!(
        dist.join("fine/index.html").is_file(),
        "a route after the broken one was not written:\n{said}"
    );
    // And the broken one is not: an error page in `dist/` is a build that
    // shipped its own failure.
    assert!(
        !dist.join("broken/index.html").exists(),
        "the route that threw was written anyway:\n{said}"
    );
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

/// A dev server that must not outlive the test, and says what it did.
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
struct DevServer {
    child: Child,
}

impl DevServer {
    /// Start `uf dev` on `port`, draining what it says into `said`.
    fn start<'scope, 'env: 'scope>(
        root: &Path,
        port: u16,
        scope: &'scope std::thread::Scope<'scope, 'env>,
        said: &'env Mutex<String>,
    ) -> Self {
        let mut child = Command::new(uf_path())
            .arg("--cwd")
            .arg(root)
            .args(["dev", "--port", &port.to_string()])
            .env_remove("NO_COLOR")
            .env("TERM", "xterm-256color")
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

impl Drop for DevServer {
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
            let mut server = DevServer::start(&root, port, scope, &said);
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
fn assert_page(server: &mut DevServer, port: u16, said: &Mutex<String>, body: &str) {
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

    // A missing page *inside* the manual is answered by the manual's own
    // boundary, inside the manual's layout. `_uf.not-found.js` was read at the
    // router root only, so this used to be the site's root 404 with the
    // sidebar and the prose column gone. See ubugeeei-prod/uf#263.
    let in_guide = get(server, port, "/guide/definitely-not-a-page/", said);
    assert!(
        in_guide.starts_with("HTTP/1.1 404"),
        "a missing guide page must be a 404:\n{in_guide}"
    );
    assert!(
        in_guide.contains("There is no such page in the manual."),
        "`app/guide/_uf.not-found.js` did not answer a path under /guide:\n{in_guide}"
    );
    assert!(
        in_guide.contains("class=\"manual\""),
        "the guide's 404 rendered outside `app/guide/_uf.layout.js`:\n{in_guide}"
    );

    // And the nearest-ancestor rule the other way: `/reference` declares no
    // boundary of its own, so it falls back to the site's root one — which is
    // what worked before and has to keep working.
    let in_reference = get(server, port, "/reference/definitely-not-a-page/", said);
    assert!(
        in_reference.starts_with("HTTP/1.1 404"),
        "a missing reference page must be a 404:\n{in_reference}"
    );
    assert!(
        in_reference.contains("There is no page here."),
        "/reference has no boundary, so the root one answers it:\n{in_reference}"
    );
}

/// One request to a server that has already answered once, with the server's
/// own account of itself if it will not answer this time.
///
/// The failure in ubugeeei-prod/uf#234 was here: `/` was served and then the
/// port stopped listening, and `http_get`'s bare `expect` reported
/// `ConnectionRefused` and nothing about the process that had refused it.
fn get(server: &mut DevServer, port: u16, path: &str, said: &Mutex<String>) -> String {
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
