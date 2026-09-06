//! `uf build`, `uf build --compile`, `uf dev`, `uf preview` and `uf start` end
//! to end, through Vite on the real driver.
//!
//! Two fixtures, because they answer different questions.
//!
//! The first is this repository's own docs site: a uf project whose
//! `@uniflowed/*` dependencies resolve to `packages/` through the npm
//! workspace. Building it exercises everything a user's build does — Flow
//! through `uf transform`, the route table, the client and server bundles,
//! prerendering — with no mocks anywhere.
//!
//! The second is `tests/fixtures/served-app`, and it exists because the docs
//! site is a *static* site: it has no route handler and no route with
//! parameters, so serving it proves only that files can be served. The
//! fixture has one of each, and they are exactly the two things `uf build`
//! produced and nothing could reach.
//!
//! The tests skip, loudly, when Node or the workspace's `node_modules` are
//! absent, so a checkout that never ran `npm ci` still passes `cargo test`
//! and a CI runner that forgot to will say so rather than silently cover less.
//! `uf build --compile` needs Bun as well, and skips on the same terms; see
//! [`bun_ready`].
//!
//! Two tests here assert about an *artefact* rather than about a server: the
//! directory `uf build --adapter node` writes and the file `uf build
//! --compile` writes are each copied somewhere with nothing else in it and
//! asked. Both keep the half that needs no socket unconditional, because that
//! half is the one that says whether the copy carries the application.
//!
//! One test here never reaches Vite:
//! [`a_contract_violation_fails_the_build_before_vite_runs`]. It belongs with
//! the others because what it asserts about is `uf build`, and because the
//! phase it asserts about is the one that decides whether Vite runs at all.

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use support::{Project, assert_plain, uf, uf_path};

/// The repository's `docs/` directory.
fn docs_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

/// The application that has a route handler and an unprerendered route.
fn served_app_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/served-app")
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

/// The same, for the served fixture, which now has two tests building it.
///
/// `preview_and_start_serve_the_whole_of_a_build` was the only one, so it
/// needed no lock. `the_node_adapter_writes_a_directory_that_serves_from_an_
/// empty_one` builds the same `dist/` and the same `.uf/`, and on a machine
/// that can bind a socket the two run at once.
static SERVED: Mutex<()> = Mutex::new(());

fn served_lock() -> std::sync::MutexGuard<'static, ()> {
    SERVED
        .lock()
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
    // A build that works and complains is a build people stop reading. Vite
    // deprecated `envFile: false` in 8.x and printed a line saying so on every
    // build — twice in this one, once per environment — until the driver moved
    // to `envDir: false`. This asserts the whole class rather than that one
    // sentence: uf passes Vite the options, so a warning Vite prints about them
    // is uf's to fix, not the reader's to learn to ignore.
    assert!(
        !stdout.contains("deprecated"),
        "the build must not report a deprecated option:\n{stdout}"
    );
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

/// A minimal uf application, as `(path, source)` pairs.
///
/// Small on purpose: these tests are about one phase of the build each, and a
/// page with anything in it would put the phase they are about behind a Flow
/// compile of somebody's idea of a demo.
fn minimal_app() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "app.js",
            "// @flow\nimport { routerView } from \"@uniflowed/router\";\n\nexport default routerView(\"./app\");\n",
        ),
        (
            "app/_uf.layout.js",
            "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport component Layout(children: React.Node) {\n  return (\n    <html lang=\"en\">\n      <body>{children}</body>\n    </html>\n  );\n}\n",
        ),
        (
            "app/_uf.page.js",
            "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport component Page() {\n  return <main>home</main>;\n}\n",
        ),
    ]
}

/// A middleware must run before the path it guards answers.
///
/// `_uf.middleware.js` was a reserved name in the Rust router, a reserved name
/// in the build's router, a documented file convention, a column in `uf
/// inspect --json` and a file the dev server invalidated the route table for —
/// and `routesModuleSource` dropped it, so it was never imported and never
/// called. Someone who wrote one to check a session got an unprotected page
/// and no diagnostic anywhere. See ubugeeei-prod/uf#260.
///
/// Asserted against the built server bundle rather than against a running
/// server: the whole chain — the directory scan, the generated table, the
/// bundle, the runner — is exercised either way, and this way the test needs
/// no socket, so it runs in the sandboxes where `TcpListener::bind` is
/// refused. `tests/library/middleware.test.js` owns the runner's own rules.
#[test]
fn a_middleware_guards_the_path_it_sits_under() {
    if !fixture_ready() {
        return;
    }

    let mut files = minimal_app();
    files.push((
        "app/dashboard/_uf.page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport component Page() {\n  return <main>secrets</main>;\n}\n",
    ));
    files.push((
        "app/dashboard/_uf.middleware.js",
        "// @flow\nconst SECRET_COOKIE_NAME = \"uf-fixture-session\";\n\nexport default function middleware(request: Request): Response | void {\n  const cookie = request.headers.get(\"cookie\") ?? \"\";\n  if (!cookie.includes(SECRET_COOKIE_NAME)) {\n    return Response.redirect(new URL(\"/sign-in\", request.url), 302);\n  }\n}\n",
    ));
    let project = Project::new(&files);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .arg("build")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // The bundled server entry, which is what a deployment runs.
    let server = project.path().join(".uf/build/server/server.js");
    assert!(server.is_file(), "the build wrote no server bundle");
    let probe = project.path().join("probe.mjs");
    fs::write(
        &probe,
        format!(
            r#"const {{ runMiddleware }} = await import({server:?});
const at = async (path, init) => {{
  const answer = await runMiddleware(new Request(`http://localhost${{path}}`, init));
  return answer == null ? null : {{ status: answer.status, location: answer.headers.get("location") }};
}};
console.log(JSON.stringify({{
  guarded: await at("/dashboard"),
  nested: await at("/dashboard/reports/2026"),
  missing: await at("/dashboard/typo"),
  withCookie: await at("/dashboard", {{ headers: {{ cookie: "uf-fixture-session=1" }} }}),
  home: await at("/"),
}}));
"#,
            server = server.to_string_lossy(),
        ),
    )
    .unwrap();

    let ran = Command::new("node").arg(&probe).output().unwrap();
    assert!(
        ran.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let answers: serde_json::Value =
        serde_json::from_slice(&ran.stdout).expect("the probe printed JSON");

    assert_eq!(
        answers["guarded"]["status"], 302,
        "the middleware did not run for the path it guards: {answers}"
    );
    assert!(
        answers["guarded"]["location"]
            .as_str()
            .is_some_and(|location| location.ends_with("/sign-in")),
        "{answers}"
    );
    // The subtree, and the paths under it that match no route: a guard that
    // only covered its own page would leave both open.
    assert_eq!(answers["nested"]["status"], 302, "{answers}");
    assert_eq!(answers["missing"]["status"], 302, "{answers}");
    // And it lets a request through when its own check passes, rather than
    // being a wall.
    assert!(answers["withCookie"].is_null(), "{answers}");
    assert!(answers["home"].is_null(), "{answers}");

    // Server-only, and not by convention: a middleware in the browser bundle
    // would ship the check to the reader it is meant to keep out.
    let shipped = fs::read_dir(project.path().join("dist/assets"))
        .expect("the client build wrote assets")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|kind| kind == "js"))
        .map(|entry| fs::read_to_string(entry.path()).unwrap_or_default())
        .collect::<String>();
    assert!(
        !shipped.contains("uf-fixture-session"),
        "the middleware reached the client bundle"
    );
}

/// A module that breaks the RSC contract fails the build, and says how.
///
/// `uf build` ran the analysis, got back typed diagnostics with a severity of
/// `error`, printed how many there were — `rsc diagnostics  5`, on this
/// repository's own documentation site — and exited 0, with the messages in a
/// JSON file nobody reads. See ubugeeei-prod/uf#281.
///
/// The build stops before Vite, so this test needs neither Node nor the
/// workspace: the phase under test is the one that decides whether the bundle
/// is worth building.
#[test]
fn a_contract_violation_fails_the_build_before_vite_runs() {
    let mut files = minimal_app();
    // A page is a Server Component by classification, and `localStorage` is
    // the browser's. `remember` is never called while the page renders, on
    // purpose: a violation that also crashes the prerender would fail the
    // build anyway, and this test would pass without reporting anything. This
    // one is exactly the case that used to succeed — measured on the pinned
    // `main` binary: `rsc diagnostics  1`, `✓ build succeeded`, exit 0.
    files[2] = (
        "app/_uf.page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport function remember(slug: string): void {\n  localStorage.setItem(\"last-seen\", slug);\n}\n\nexport component Page() {\n  return <main>home</main>;\n}\n",
    );
    let project = Project::new(&files);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .arg("build")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "a build with an RSC contract violation must fail\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for expected in [
        "app/_uf.page.js",
        "rsc/client-only-api-in-server",
        "uses client-only `localStorage`",
        "React Server Components contract violation",
    ] {
        assert!(said.contains(expected), "missing {expected:?} in:\n{said}");
    }
    // The count in the summary is what this used to be, and a build that
    // failed after bundling would have printed the summary anyway.
    assert!(
        !said.contains("rsc diagnostics"),
        "the build reached its summary despite a contract violation:\n{said}"
    );
    assert!(
        !project.path().join("dist/index.html").exists(),
        "the build prerendered a page despite a contract violation"
    );
}

/// A page that throws fails its own route, and nothing else.
///
/// Three assertions because they are one behaviour: the build fails, it says
/// which URL threw, and the routes that did render are still written. Before
/// this the prerender loop had no `try` — the first page to throw rejected out
/// of the driver, the message named the exception rather than the route, and
/// no page after it was written. See ubugeeei-prod/uf#257.
///
/// Built from [`minimal_app`] in a [`Project`] like the tests above, which
/// puts the deliberate throw under the repository's `.uf/`: inside the
/// workspace, because `@uniflowed/*`, `react` and `react-dom` are resolved by
/// walking up to its `node_modules`, and inside the one directory uf's own
/// lint, format and test discovery always ignore — anywhere else in the
/// repository this page would be reported as this repository's defect.
#[test]
fn a_page_that_throws_fails_its_route_and_not_the_others() {
    if !fixture_ready() {
        return;
    }
    let mut files = minimal_app();
    files.push((
        "app/fine/_uf.page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport component Page() {\n  return <main>this page is fine</main>;\n}\n",
    ));
    files.push((
        "app/broken/_uf.page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport component Page() {\n  throw new Error(\"this page throws on purpose\");\n}\n",
    ));
    let project = Project::new(&files);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .arg("build")
        .output()
        .unwrap();

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
    let dist = project.path().join("dist");
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

/// A not-found boundary that throws fails the build, and writes no `404.html`.
///
/// The root 404 is prerendered outside the loop above and had neither of the
/// loop's two checks. A boundary is a component like any other and can throw,
/// and when it does `prerender` does not reject — it renders the *error* page
/// and reports the exception on `result.error`. The driver wrote that HTML to
/// `dist/404.html`, emitted `page`, and exited 0.
///
/// Which is the worst place in the build for that to happen. A static host
/// serves `404.html` to everybody who mistypes a URL, so the page a project
/// wrote to say "no such page" would have been silently replaced by uf's error
/// page for the life of the deploy, and nothing between the throw and
/// production would have mentioned it.
#[test]
fn a_not_found_boundary_that_throws_fails_the_build_and_writes_no_file() {
    if !fixture_ready() {
        return;
    }
    let mut files = minimal_app();
    files.push((
        "app/_uf.not-found.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport default component NotFound() {\n  throw new Error(\"the 404 boundary throws on purpose\");\n}\n",
    ));
    let project = Project::new(&files);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .arg("build")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let said = format!("{stdout}{stderr}");
    assert!(
        !output.status.success(),
        "a 404 boundary that throws must fail the build:\n{said}"
    );
    assert!(
        said.contains("/404"),
        "the build must name the page that threw:\n{said}"
    );
    assert!(
        said.contains("the 404 boundary throws on purpose"),
        "the build must say why it failed:\n{said}"
    );
    let dist = project.path().join("dist");
    assert!(
        !dist.join("404.html").exists(),
        "the build published its own failure as 404.html:\n{said}"
    );
    // The rest of the build is untouched: one broken boundary is one broken
    // boundary, the same as one broken page.
    assert!(
        dist.join("index.html").is_file(),
        "the home page was not written:\n{said}"
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
/// It drives `uf dev`, it drives `uf preview` and `uf start`, and it drives a
/// compiled binary. No test here cares which process is listening, only that
/// something is and that it can be made to explain itself when it is not — so
/// there are two ways in: [`Server::start`] for a `uf` subcommand, and
/// [`Server::spawn`] for a command that is already built, which is the only
/// shape a standalone binary comes in.
struct Server {
    child: Child,
}

impl Server {
    /// Start `uf <args>` in `root`, draining what it says into `said`.
    fn start<'scope, 'env: 'scope>(
        root: &Path,
        args: &[&str],
        scope: &'scope std::thread::Scope<'scope, 'env>,
        said: &'env Mutex<String>,
    ) -> Self {
        let mut command = Command::new(uf_path());
        command
            .arg("--cwd")
            .arg(root)
            .args(args)
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

impl Drop for Server {
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
            let mut server =
                Server::start(&root, &["dev", "--port", &port.to_string()], scope, &said);
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
fn assert_page(server: &mut Server, port: u16, said: &Mutex<String>, body: &str) {
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

/// Both servers, against an application whose interesting half is not static.
///
/// One test rather than two, and one build rather than two, because the thing
/// being asserted is that `uf preview` and `uf start` *agree*: they are two
/// commands because one has Vite in the loop and the other does not, and the
/// moment their answers diverge the preview stops being worth running. Two
/// tests would also have raced — both would have rebuilt the same `dist/`.
#[test]
fn preview_and_start_serve_the_whole_of_a_build() {
    if !fixture_ready() || !loopback_ready() {
        return;
    }
    let _served = served_lock();
    let root = served_app_root();

    let build = uf().arg("--cwd").arg(&root).arg("build").output().unwrap();
    assert!(
        build.status.success(),
        "the fixture must build before it can be served\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    // The half a static host could already serve. Asserted here rather than
    // taken on trust, because every "the server rendered it" assertion below
    // is only interesting if the prerender is what did not happen.
    assert!(root.join("dist/guide/index.html").is_file());
    assert!(
        !root.join("dist/posts").exists(),
        "a route with parameters and no `generateStaticParams` must not be prerendered; \
         if it were, `uf start` would be serving a file rather than rendering"
    );
    assert!(
        !root.join("dist/slow").exists(),
        "the suspending route must not be prerendered either; a file would be served without \
         rendering and the streaming assertion below would pass without streaming"
    );

    for command in ["preview", "start"] {
        serve_and_assert(&root, command);
    }
}

/// The script the deployed directory is asked with, when no socket may be had.
///
/// It is written *beside* the copied directory rather than inside it, and
/// imports it by a relative path — which is the assertion, not the setup. A
/// probe living inside the artefact could be resolving something the artefact
/// happens to sit next to; one outside it can only reach what was copied.
const ASK_THE_ARTEFACT: &str = r#"import handler from "./app/handler.js";

const ask = async (label, url, init) => {
  const response = await handler.fetch(new Request(`http://127.0.0.1${url}`, init));
  const body = (await response.text()).replace(/\s+/g, " ");
  process.stdout.write(`${label} ${response.status} ${body}\n`);
};

await ask("handler-get", "/api/health");
await ask("handler-post", "/api/health", { method: "POST", body: JSON.stringify({ name: "uf" }) });
await ask("rendered", "/posts/hello-world");
await ask("missing", "/definitely-not-a-page/");
"#;

/// `uf build --adapter node`, copied somewhere that is not a checkout.
///
/// This is the assertion ubugeeei-prod/uf#335 asks for and the one that
/// distinguishes an artefact from a build: the directory is copied to a
/// temporary directory with no `node_modules` anywhere above it and no `uf`
/// anywhere near it, and it still answers a route handler and still renders a
/// route the build wrote no file for.
///
/// The blocker recorded in that issue — that the server bundle keeps
/// `@uniflowed/router/server` external, so serving it needs `uf transform`
/// alive — was not one. `packages/vite/index.js` has set
/// `ssr.noExternal: [/^@uniflowed\//]` since the plugin was written, because
/// Node cannot import Flow; the dependencies the ordinary server build leaves
/// external are `react` and `react-dom`, which are ordinary JavaScript. What
/// the adapter adds is `ssr.noExternal: true`, so those come in too and the
/// directory needs no `node_modules` at all.
///
/// Written to prove as much as the machine allows, like the `--compile` test
/// below. The half that needs no socket runs everywhere and is the half that
/// matters: whether the copied directory carries the application. Where a
/// socket can be bound, `server.js` is started from the copy and asked
/// everything `uf preview` and `uf start` are asked, by the same function — so
/// a deployment that answered differently from the command it was checked with
/// would fail here.
#[test]
fn the_node_adapter_writes_a_directory_that_serves_from_an_empty_one() {
    if !fixture_ready() {
        return;
    }
    let _served = served_lock();
    let root = served_app_root();

    let output = uf()
        .arg("--cwd")
        .arg(&root)
        .args(["build", "--adapter", "node"])
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
    for expected in ["adapter", ".uf/deploy/node", "node server.js"] {
        assert!(
            stdout.contains(expected),
            "the summary must say what was written and how to run it; missing {expected:?} in:\n{stdout}"
        );
    }

    // Copied out rather than driven in place, because in place proves nothing:
    // `dist/`, `node_modules` and the source are all still there, and an
    // artefact quietly reading one of them would pass.
    let empty = tempfile::tempdir().unwrap();
    let deployed = empty.path().join("app");
    copy_tree(&root.join(".uf/deploy/node"), &deployed);
    for ancestor in deployed.ancestors() {
        assert!(
            !ancestor.join("node_modules").exists(),
            "this test means nothing with a node_modules at {}",
            ancestor.display()
        );
    }

    // The static half came along: the prerendered documents and the hashed
    // client assets, which are what `server.js` serves before it renders
    // anything. And the route that was *not* prerendered is still not there,
    // which is what makes the render assertion below a render.
    assert!(deployed.join("static/index.html").is_file());
    assert!(deployed.join("static/guide/index.html").is_file());
    assert!(
        !deployed.join("static/posts").exists(),
        "a route with parameters and no `generateStaticParams` must reach the copy unprerendered"
    );
    assert!(
        deployed.join("package.json").is_file(),
        "`node server.js` reads `.js` as CommonJS without it"
    );

    let ask = empty.path().join("ask.mjs");
    fs::write(&ask, ASK_THE_ARTEFACT).unwrap();
    let answered = Command::new("node")
        .arg("ask.mjs")
        .current_dir(empty.path())
        .output()
        .unwrap();
    let said = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&answered.stdout),
        String::from_utf8_lossy(&answered.stderr)
    );
    assert!(answered.status.success(), "{said}");
    let answers = String::from_utf8_lossy(&answered.stdout).into_owned();
    for expected in [
        // A route handler, which is the clearest thing a build could not serve.
        "handler-get 200 {\"status\":\"ok\"}",
        // With a body, so the assertion is that the request reached the module
        // rather than that something answered 200.
        "handler-post 200 {\"echoed\":\"uf\"}",
    ] {
        assert!(
            answers.contains(expected),
            "missing {expected:?} in:\n{said}"
        );
    }
    let rendered = answers
        .lines()
        .find(|line| line.starts_with("rendered "))
        .unwrap_or_else(|| panic!("no rendered line in:\n{said}"));
    assert!(
        rendered.starts_with("rendered 200") && rendered.contains("post: hello-world"),
        "a route with no prerendered file has to be rendered per request:\n{rendered}"
    );
    let missing = answers
        .lines()
        .find(|line| line.starts_with("missing "))
        .unwrap_or_else(|| panic!("no missing line in:\n{said}"));
    assert!(
        missing.starts_with("missing 404") && missing.contains("served-app has no such page"),
        "an unrouted path is the project's own 404, not somebody else's page:\n{missing}"
    );

    if !loopback_ready() {
        return;
    }

    // And the whole of it, through the socket `server.js` takes: the static
    // half, the application half, and the streaming — asked by the function
    // that asks `uf preview` and `uf start`, because "the deployment answers
    // what the preview answered" is the only interesting thing left to say.
    let mut refused = Vec::new();
    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let said = Mutex::new(String::new());

        let served = std::thread::scope(|scope| {
            let mut command = Command::new("node");
            command
                .arg("server.js")
                .args(["--host", "127.0.0.1", "--port", &port.to_string()])
                .current_dir(&deployed);
            let mut server = Server::spawn(command, scope, &said);
            if let Some(body) = wait_for_http(port, "/", Duration::from_secs(90)) {
                assert_served(&mut server, port, &said, &body, "build --adapter node");
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
        "the deployed directory never answered, on {PORT_ATTEMPTS} different ports\n{}",
        refused.join("\n\n")
    );
}

/// Copy `from` to `to`, recursively.
///
/// The point of the copy is that the destination has nothing else in it, so
/// this is deliberately not a merge and deliberately not `cp -r`: a test that
/// shelled out would be asserting about the machine's coreutils on one of the
/// three platforms uf supports.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// Start one of the two servers and ask it everything, retrying the port the
/// way `dev_serves_the_docs_site_through_vite` does and for the same reason.
fn serve_and_assert(root: &Path, command: &str) {
    let mut refused = Vec::new();

    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let said = Mutex::new(String::new());
        // `uf start` binds every interface by default, which is right for a
        // production server and wrong for a test on somebody's laptop.
        let port_text = port.to_string();
        let args: Vec<&str> = vec![command, "--host", "127.0.0.1", "--port", &port_text];

        let served = std::thread::scope(|scope| {
            let mut server = Server::start(root, &args, scope, &said);
            if let Some(body) = wait_for_http(port, "/", Duration::from_secs(90)) {
                assert_served(&mut server, port, &said, &body, command);
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
        "`uf {command}` never answered, on {PORT_ATTEMPTS} different ports\n{}",
        refused.join("\n\n")
    );
}

/// Everything a built application has to answer, whichever server is answering.
///
/// The four questions are the four halves of a uf build, and before `uf
/// preview` and `uf start` existed a build could answer only the first two.
fn assert_served(server: &mut Server, port: u16, said: &Mutex<String>, body: &str, command: &str) {
    let context = |what: &str, response: &str| {
        format!("`uf {command}` {what}\n{response}\n{}", server_said(said))
    };

    // 1. The home page, prerendered to `dist/index.html`.
    assert!(
        body.starts_with("HTTP/1.1 200"),
        "{}",
        context("did not serve the home page", body)
    );
    assert!(
        body.contains("served-app home"),
        "{}",
        context("served something that is not the home page", body)
    );
    assert!(
        body.contains("<script type=\"module\" src=\"/assets/"),
        "{}",
        context("served a document with no hydration script", body)
    );

    // 2. A nested route, prerendered to `dist/guide/index.html`.
    let guide = get(server, port, "/guide/", said);
    assert!(
        guide.starts_with("HTTP/1.1 200") && guide.contains("served-app guide"),
        "{}",
        context("did not serve the nested route", &guide)
    );

    // 3. A route with a parameter and no `generateStaticParams`, which the
    //    build wrote no file for: the only way this can be a 200 is a render
    //    per request.
    let post = get(server, port, "/posts/hello-world", said);
    assert!(
        post.starts_with("HTTP/1.1 200"),
        "{}",
        context("did not render an unprerendered route", &post)
    );
    assert!(
        post.contains("post: hello-world"),
        "{}",
        context("rendered the wrong route, or ignored the parameter", &post)
    );

    // 4. A route handler, for both the method a page could have answered and
    //    the method only a handler can.
    let health = get(server, port, "/api/health", said);
    assert!(
        health.starts_with("HTTP/1.1 200") && health.contains("\"status\":\"ok\""),
        "{}",
        context("did not answer the route handler's GET", &health)
    );
    let posted = http_request(
        "127.0.0.1",
        port,
        "POST",
        "/api/health",
        Some("{\"name\":\"uf\"}"),
    );
    assert!(
        posted.starts_with("HTTP/1.1 200") && posted.contains("\"echoed\":\"uf\""),
        "{}",
        context("did not answer the route handler's POST", &posted)
    );

    // And a path with no route is a 404 rather than somebody else's page —
    // the failure Vite's own preview server has by default, where an SPA
    // fallback answers every unmatched path with the home page and a 200.
    let missing = get(server, port, "/definitely-not-a-page/", said);
    assert!(
        missing.starts_with("HTTP/1.1 404"),
        "{}",
        context(
            "answered an unrouted path with something other than a 404",
            &missing
        )
    );
    assert!(
        missing.contains("served-app has no such page"),
        "{}",
        context("did not serve the project's own not-found page", &missing)
    );

    // 5. A route that suspends: the layout and the fallback have to be on the
    //    wire before the page is. This is the one assertion in this file about
    //    *when* bytes arrived rather than what they said, and it is the only
    //    kind that can tell a streaming renderer from a buffering one — a
    //    document sent in one piece still has the fallback before the page in
    //    document order, because that is where React writes it.
    //
    //    `id` is the command's own, because the fixture keeps a resolved
    //    promise per id and a second request for the same one would answer
    //    without waiting; see `app/slow/[id]/_uf.page.js`.
    let slow = timed_get(port, &format!("/slow/{command}"));
    assert!(
        slow.text.starts_with("HTTP/1.1 200"),
        "{}",
        context("did not render the suspending route", &slow.text)
    );
    let shell = slow.first_at("slow: waiting").unwrap_or_else(|| {
        panic!(
            "{}",
            context("never sent the `_uf.loading.js` fallback", &slow.text)
        )
    });
    let page = slow
        .first_at(&format!("slow: {command}"))
        .unwrap_or_else(|| {
            panic!(
                "{}",
                context("the suspended page never arrived", &slow.text)
            )
        });
    assert!(
        shell + STREAMING_MARGIN <= page,
        "{}",
        context(
            &format!(
                "sent the fallback and the page together: the fallback was {}ms in and the \
                 page {}ms in, and the page waits {}ms — so nothing streamed",
                shell.as_millis(),
                page.as_millis(),
                SUSPENDING_ROUTE_DELAY.as_millis()
            ),
            &slow.text
        )
    );
}

/// How long `app/slow/[id]/_uf.page.js` waits before it renders.
const SUSPENDING_ROUTE_DELAY: Duration = Duration::from_millis(500);

/// How much of that gap has to survive for the response to have been streamed.
///
/// Well under the delay, because the question is "were these two in the same
/// write" and not "is this machine fast". A buffered response puts both strings
/// in the first read and the gap is zero; a streamed one cannot make the gap
/// smaller than the page's own wait, minus whatever the shell took to render.
const STREAMING_MARGIN: Duration = Duration::from_millis(200);

/// A response, and when each byte of it turned up.
///
/// `reads` is one entry per successful `read`, holding how much of the response
/// had arrived by then. That is enough to answer "when did this string first
/// appear", which is the only question asked of it, and it avoids having to
/// decide what a chunk is: the kernel decides, and the assertion is about a gap
/// far larger than any packetization difference.
struct TimedResponse {
    text: String,
    reads: Vec<(Duration, usize)>,
}

impl TimedResponse {
    /// When `needle` had first arrived, or `None` if it never did.
    fn first_at(&self, needle: &str) -> Option<Duration> {
        let end = self.text.find(needle)? + needle.len();
        self.reads
            .iter()
            .find(|(_, received)| *received >= end)
            .map(|(at, _)| *at)
    }
}

/// One request, read incrementally, timed from the moment it was sent.
///
/// `http_request` reads to the end and returns a string, which is the right
/// shape for every other assertion here and destroys the only evidence this one
/// needs. `Connection: close` is what makes the read loop end.
fn timed_get(port: u16, path: &str) -> TimedResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to the server");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/html\r\n\
         Connection: close\r\n\r\n"
    )
    .unwrap();

    let started = Instant::now();
    let mut bytes = Vec::new();
    let mut reads = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                bytes.extend_from_slice(&buffer[..read]);
                reads.push((started.elapsed(), bytes.len()));
            }
            Err(error) => panic!("reading the streamed response failed: {error}"),
        }
    }
    TimedResponse {
        text: String::from_utf8_lossy(&bytes).into_owned(),
        reads,
    }
}

fn server_said(said: &Mutex<String>) -> String {
    said.lock()
        .map_or_else(|_| "<the reader thread panicked>".to_owned(), |s| s.clone())
}

/// One request to a server that has already answered once, with the server's
/// own account of itself if it will not answer this time.
///
/// The failure in ubugeeei-prod/uf#234 was here: `/` was served and then the
/// port stopped listening, and `http_get`'s bare `expect` reported
/// `ConnectionRefused` and nothing about the process that had refused it.
fn get(server: &mut Server, port: u16, path: &str, said: &Mutex<String>) -> String {
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
    http_request(host, port, "GET", path, None)
}

/// The same, for a method and a body — which is the half a route handler is
/// the only thing that can answer, and therefore the half a build that serves
/// only files gets wrong.
fn http_request(host: &str, port: u16, method: &str, path: &str, body: Option<&str>) -> String {
    let mut stream = TcpStream::connect((host, port)).expect("connect to the server");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    // The body's headers only when there is a body: a `GET` carrying
    // `Content-Length: 0` is legal and is still not the request a browser
    // makes, and this is the request every other assertion here is made about.
    let entity = body.map_or_else(String::new, |body| {
        format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    });
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: text/html\r\n\
         Connection: close\r\n{}",
        if entity.is_empty() {
            String::from("\r\n")
        } else {
            entity
        }
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
            let mut server = Server::spawn(command, scope, &said);
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
        let mut server = Server::spawn(command, scope, &said);
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
fn assert_compiled_site(server: &mut Server, port: u16, said: &Mutex<String>, body: &str) {
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
