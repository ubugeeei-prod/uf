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
//! `support::bun_ready`, which `tests/bun_host.rs` shares.
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

use support::{Project, assert_plain, bun_ready, uf, uf_path};

/// The repository's `docs/` directory.
fn docs_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

/// The application that has a route handler and an unprerendered route.
fn served_app_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/served-app")
}

/// The application whose two routes differ by one `"use client"` import.
fn rsc_split_app_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rsc-split-app")
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

    // Not in `dist/`. Everything in the output directory is served — by a
    // static host, by `uf preview` and by `uf start` — and between them these
    // three name every route including the ones nothing links to, the source
    // file behind each one, and the size of every chunk. See
    // ubugeeei-prod/uf#339.
    let meta = root.join(".uf/build/meta");
    for name in [
        "uf-build-manifest.json",
        "uf-rsc-manifest.json",
        "uf-bundle-report.json",
    ] {
        assert!(
            meta.join(name).is_file(),
            "{name} must be written beside the build"
        );
        assert!(
            !dist.join(name).exists(),
            "{name} must not be somewhere a static host would serve it"
        );
    }

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(meta.join("uf-build-manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["engine"], serde_json::json!("vite"));
    assert_eq!(manifest["transform"], serde_json::json!("uf transform"));
    assert_eq!(manifest["pages"][0]["url"], serde_json::json!("/"));

    // The two files uf's own site shipped without. `docs/uf.config.js` names
    // the origin, which is the whole of what a build cannot work out for
    // itself; see ubugeeei-prod/uf#269.
    let sitemap = fs::read_to_string(dist.join("sitemap.xml")).expect("a sitemap");
    assert!(
        sitemap.starts_with(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n"
        ),
        "{sitemap}"
    );
    for expected in [
        "<loc>https://docs.uniflowed.dev/</loc>",
        "<loc>https://docs.uniflowed.dev/guide</loc>",
        "<loc>https://docs.uniflowed.dev/guide/install</loc>",
        "<loc>https://docs.uniflowed.dev/reference/cli</loc>",
    ] {
        assert!(sitemap.contains(expected), "missing {expected}:\n{sitemap}");
    }
    // The error document is served and is not a page.
    assert!(
        !sitemap.contains("https://docs.uniflowed.dev/404"),
        "{sitemap}"
    );
    // One `<loc>` per page the prerender reported, and no more.
    assert_eq!(
        sitemap.matches("<loc>").count(),
        manifest["pages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|page| page["url"] != serde_json::json!("/404"))
            .count(),
        "the sitemap and the prerender disagree about what was built:\n{sitemap}"
    );

    assert_eq!(
        fs::read_to_string(dist.join("robots.txt")).expect("a robots.txt"),
        "User-agent: *\nDisallow:\n\nSitemap: https://docs.uniflowed.dev/sitemap.xml\n"
    );

    // And a page's own metadata reaches the document a crawler reads: the
    // canonical URL the home page declares, resolved against the root
    // layout's `metadataBase`, and the card the layout declares for every
    // page under it.
    assert!(
        index.contains("<link rel=\"canonical\" href=\"https://docs.uniflowed.dev/\"/>"),
        "no canonical URL:\n{index}"
    );
    assert!(
        index.contains("<meta name=\"twitter:card\" content=\"summary_large_image\"/>"),
        "no twitter card:\n{index}"
    );
    assert!(
        index.contains(
            "<meta property=\"og:image\" content=\"https://docs.uniflowed.dev/brand/og.png\"/>"
        ),
        "the og:image was not made absolute:\n{index}"
    );
    // And the card carries words. `og:title` and `og:description` fall back to
    // the document's own, so a page that said what it is called said what its
    // card is called — the site shipped thirty pages whose card was an image
    // and nothing else.
    assert!(
        index.contains("<meta property=\"og:title\" content="),
        "no og:title:\n{index}"
    );
    assert!(
        index.contains("<meta property=\"og:type\" content=\"website\"/>"),
        "no og:type:\n{index}"
    );

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(meta.join("uf-bundle-report.json")).unwrap())
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
///
/// The probe is a host, so it owns the request the way the four real ones do:
/// `beginRequest` from the bundle, `run` around the guard, `settle` after the
/// answer. That is not ceremony to satisfy an assertion — it is the second
/// thing this test now proves. A built bundle has its own inlined copy of
/// `@uniflowed/server`, so a host that established a request in any other copy
/// would leave every `cookies()` in the application outside one; taking
/// `beginRequest` from the bundle is what makes that impossible, and only a
/// real build can show it. And the `after()` below does not run until `settle`,
/// which is what the router used to get wrong. See ubugeeei-prod/uf#389.
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
        "// @flow\nimport { after } from \"@uniflowed/server\";\n\nconst SECRET_COOKIE_NAME = \"uf-fixture-session\";\n\nexport default function middleware(request: Request): Response | void {\n  after(() => {\n    globalThis.__ufAudited = (globalThis.__ufAudited ?? 0) + 1;\n  });\n  const cookie = request.headers.get(\"cookie\") ?? \"\";\n  if (!cookie.includes(SECRET_COOKIE_NAME)) {\n    return Response.redirect(new URL(\"/sign-in\", request.url), 302);\n  }\n}\n",
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
            r#"const {{ beginRequest, runMiddleware }} = await import({server:?});
// A host: begin the request, let the guard decide, "write" the answer, settle.
// `duringAnswer` is what `globalThis.__ufAudited` was before `settle` ran, so
// the probe can say whether the callback waited for the response or not.
const at = async (path, init) => {{
  const request = new Request(`http://localhost${{path}}`, init);
  const {{ run, settle }} = beginRequest(request);
  let duringAnswer = null;
  try {{
    const answer = await run(() => runMiddleware(request));
    duringAnswer = globalThis.__ufAudited ?? 0;
    return answer == null
      ? {{ duringAnswer }}
      : {{ status: answer.status, location: answer.headers.get("location"), duringAnswer }};
  }} finally {{
    await settle();
  }}
}};
const guarded = await at("/dashboard");
console.log(JSON.stringify({{
  guarded,
  auditedAfterSettle: globalThis.__ufAudited ?? 0,
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
    // being a wall. `null` is gone from the shape — a declining guard now
    // answers with what the probe observed rather than with nothing — so the
    // check is that no status came back.
    assert!(answers["withCookie"]["status"].is_null(), "{answers}");
    assert!(answers["home"]["status"].is_null(), "{answers}");

    // What `after()` promises, through a real build: the callback the guard
    // registered had not run while the guard's answer was being decided, and
    // had run once the host settled the request. The runner used to drain
    // before returning, so the first of these was 1 — a denial audited before
    // it was sent, and, on a request the chain let through, before there was a
    // response to audit at all. See ubugeeei-prod/uf#389.
    assert_eq!(
        answers["guarded"]["duringAnswer"], 0,
        "a middleware's after() ran before the response: {answers}"
    );
    assert_eq!(
        answers["auditedAfterSettle"], 1,
        "a middleware's after() did not run when the host settled: {answers}"
    );

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

/// A project whose only page is a Server Component, built under `target/` for
/// the reason [`project_with_a_throwing_page`] gives.
///
/// It starts clean on purpose: the thing being tested is that a violation
/// appearing while the dev server runs is *reported*, and a project that was
/// already wrong at start-up would prove only that the analysis runs once.
fn project_with_a_clean_server_component() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/uf-tests/rsc-dev");
    fs::remove_dir_all(&root).ok();
    for (relative, contents) in [
        (
            "package.json",
            r#"{ "name": "uf-rsc-dev", "private": true, "type": "module" }
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
import { greeting } from "./greeting.js";

export default component Home() {
  return <h1>{greeting()}</h1>;
}
"#,
        ),
        ("app/greeting.js", CLEAN_HELPER),
    ] {
        let file = root.join(relative);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, contents).unwrap();
    }
    root
}

/// A helper a Server Component imports, with nothing client-only in it.
const CLEAN_HELPER: &str = r#"// @flow
export function greeting(): string {
  return "hello from the server";
}
"#;

/// The same helper, reaching for a browser global. `_uf.page.js` is a server
/// entry and this module is reachable from it, so the graph says the server
/// runs `localStorage` — which it does not have.
const HELPER_THAT_TOUCHES_THE_BROWSER: &str = r#"// @flow
export function greeting(): string {
  return localStorage.getItem("greeting") ?? "hello";
}
"#;

/// `uf dev` runs the server-component analysis, and runs it again when a
/// module changes.
///
/// It ran it never. `uf build` counted the violations and `uf lint`'s
/// `server/*` rules are per-file scans that cannot see reachability, so the
/// only place a contract violation was visible was CI — after a push, about
/// code that worked when it was written, because nothing is split yet and
/// every module still runs in both places. See ubugeeei-prod/uf#347.
///
/// Both halves are asserted because both are the issue: a graph computed once
/// at start-up is a complete answer that is wrong the moment a file changes,
/// which is why this edits a file the server is already watching.
#[test]
fn dev_reports_a_contract_violation_when_one_appears() {
    if !fixture_ready() || !loopback_ready() {
        return;
    }
    let root = project_with_a_clean_server_component();
    let helper = root.join("app/greeting.js");
    let mut refused = Vec::new();

    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let said = Mutex::new(String::new());

        let served = std::thread::scope(|scope| {
            let mut server =
                Server::start(&root, &["dev", "--port", &port.to_string()], scope, &said);
            if wait_for_http(port, "/", Duration::from_secs(90)).is_none() {
                refused.push(format!(
                    "attempt {attempt} on port {port}: {}",
                    server.evidence(&said)
                ));
                drop(server);
                return false;
            }

            // A clean project says nothing. Asserted after the server has
            // answered a request, which is well after the start-up analysis.
            assert!(
                !said_contains(&said, "server components"),
                "a project with no violations must not report any:\n{}",
                server_said(&said)
            );

            fs::write(&helper, HELPER_THAT_TOUCHES_THE_BROWSER).unwrap();
            let reported = wait_for_said(
                &said,
                "rsc/client-only-api-in-server",
                Duration::from_secs(30),
            );
            assert!(
                reported,
                "the dev server did not report the violation that appeared:\n{}",
                server.evidence(&said)
            );
            let text = server_said(&said);
            assert!(
                text.contains("app/greeting.js"),
                "the report must name the module:\n{text}"
            );
            assert!(
                text.contains("localStorage"),
                "the report must name the API:\n{text}"
            );

            // And it goes away again: a report that only ever accumulates is a
            // report nobody can use to tell whether they fixed it.
            fs::write(&helper, CLEAN_HELPER).unwrap();
            let cleared = wait_for_said(
                &said,
                "the server-component contract holds",
                Duration::from_secs(30),
            );
            assert!(
                cleared,
                "the dev server never said the violation was gone:\n{}",
                server.evidence(&said)
            );
            true
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

/// Whether the server has said `needle` yet, waiting up to `budget` for it.
fn wait_for_said(said: &Mutex<String>, needle: &str, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if said_contains(said, needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn said_contains(said: &Mutex<String>, needle: &str) -> bool {
    said.lock().is_ok_and(|said| said.contains(needle))
}

/// A project with a route that is both guarded and static, built under
/// `target/` for the reason [`project_with_a_throwing_page`] gives.
///
/// `/dashboard/settings` is the interesting one: its own directory declares no
/// middleware, and `app/dashboard/_uf.middleware.js` guards it all the same.
fn project_with_a_guarded_page() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/uf-tests/guarded-page");
    fs::remove_dir_all(&root).ok();
    for (relative, contents) in [
        (
            "package.json",
            r#"{ "name": "uf-guarded-page", "private": true, "type": "module" }
"#,
        ),
        (
            "uf.config.js",
            r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: { router: { entry: "app.js", root: "app" } },
  build: { entries: ["app.js"], outDir: "dist" },
  site: { url: "https://guarded.example" },
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
  return <h1>the home page</h1>;
}
"#,
        ),
        (
            "app/dashboard/_uf.middleware.js",
            r#"// @flow
export default function middleware(): void {}
"#,
        ),
        (
            "app/dashboard/_uf.page.js",
            r#"// @flow
export default component Dashboard() {
  return <h1>the dashboard</h1>;
}
"#,
        ),
        (
            "app/dashboard/settings/_uf.page.js",
            r#"// @flow
export default component Settings() {
  return <h1>dashboard settings</h1>;
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

/// A route that is guarded *and* prerendered is named by the build.
///
/// `dist/dashboard/index.html` is a file. `app/dashboard/_uf.middleware.js` is
/// code that runs on a server, per request. A host that serves the file
/// answers without the guard, and the build said nothing about it at all —
/// which is #260's failure mode, an authorisation check that looks enforced
/// and is not, one step further down the pipeline and on the artifact that
/// actually ships. See ubugeeei-prod/uf#342.
///
/// It is a warning and not a failure, and the reason is in
/// `commands/build/guards.rs`: `uf build` writes the server bundle and the
/// static documents into the same `dist/`, so which of them is deployed — and
/// therefore whether the guard runs — is not a fact this build has.
#[test]
fn a_guarded_route_that_is_prerendered_is_reported() {
    if !fixture_ready() {
        return;
    }
    let root = project_with_a_guarded_page();

    let output = uf().arg("--cwd").arg(&root).arg("build").output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "a guarded page is a warning, not a failure\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    for expected in [
        "guards",
        "/dashboard",
        "/dashboard/settings",
        "app/dashboard/_uf.middleware.js",
        "without running the middleware that guards them",
    ] {
        assert!(
            stdout.contains(expected),
            "the build did not report the guarded routes: missing {expected:?} in:\n{stdout}"
        );
    }

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".uf/build/meta/uf-build-manifest.json")).unwrap(),
    )
    .unwrap();
    let reported = manifest["prerenderedUnderMiddleware"].as_array().unwrap();
    let urls: Vec<&str> = reported
        .iter()
        .map(|page| page["url"].as_str().unwrap())
        .collect();
    assert_eq!(
        urls,
        ["/dashboard", "/dashboard/settings"],
        "the inherited guard is the one that would be missed: {reported:#?}"
    );
    assert_eq!(
        reported[1]["middleware"],
        serde_json::json!(["app/dashboard/_uf.middleware.js"]),
        "a route below the guard is guarded by it: {reported:#?}"
    );
    assert_eq!(
        reported[1]["file"],
        serde_json::json!("dist/dashboard/settings/index.html")
    );

    // And the home page, which is prerendered and guarded by nothing, is not
    // in it: a report that named every static document would be a report
    // nobody reads.
    assert!(
        !urls.contains(&"/"),
        "an unguarded route was reported as guarded: {reported:#?}"
    );

    // The second reader of that same answer. A guard is a statement that a
    // route is not for everyone and a sitemap is a submission to search
    // engines, so the two documents the guard covers are the two the sitemap
    // leaves out — even though both are in `dist/` and a static host serves
    // them. See `commands/build/site.rs`.
    let sitemap = fs::read_to_string(root.join("dist/sitemap.xml")).unwrap();
    assert!(
        sitemap.contains("<loc>https://guarded.example/</loc>"),
        "{sitemap}"
    );
    for guarded in ["/dashboard", "/dashboard/settings"] {
        assert!(
            !sitemap.contains(&format!("https://guarded.example{guarded}")),
            "a guarded route was advertised: {sitemap}"
        );
    }

    // And it is not named in `robots.txt` either. `Disallow: /dashboard`
    // publishes `/dashboard` to everyone who fetches the file, which is the
    // opposite of what leaving it out of the sitemap achieved.
    let robots = fs::read_to_string(root.join("dist/robots.txt")).unwrap();
    assert!(!robots.contains("dashboard"), "{robots}");
    assert_eq!(
        robots,
        "User-agent: *\nDisallow:\n\nSitemap: https://guarded.example/sitemap.xml\n"
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

    /// The port the server announced, once it has announced one.
    ///
    /// `uf dev --port 0` binds a free port and prints it as the `local` URL,
    /// and reading it back here is what [`dev_serves_the_docs_site_through_
    /// vite`] does instead of choosing a port itself. Waiting for the line is
    /// also waiting for the server: a process that never gets as far as
    /// listening never prints one, so the budget covers both and the failure
    /// carries [`evidence`].
    fn bound_port(&mut self, said: &Mutex<String>, budget: Duration) -> Option<u16> {
        let deadline = Instant::now() + budget;
        loop {
            let announced = said.lock().ok().and_then(|said| announced_port(&said));
            if announced.is_some() {
                return announced;
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
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
/// Retrying is honest here because the subject is "this server serves this
/// application", not "binding a port works first time". It is capped, it only
/// covers the window *before* the first answer, and every attempt's output is
/// reported if the last one fails — so a genuinely broken server fails three
/// times and prints three servers' reasons, which is more than the one line
/// this used to give.
///
/// [`dev_serves_the_docs_site_through_vite`] no longer needs it: `uf dev
/// --port 0` asks the operating system for a port through the process that
/// then holds it, and prints the answer, so there is no window to lose. The
/// commands that have no such flag are still here, and this is still the best
/// available answer for them.
const PORT_ATTEMPTS: usize = 3;

/// The port in the first `http://host:port` URL a server has printed, if any.
///
/// Deliberately not a parse of the banner's *shape*: it looks for a URL and
/// reads the number off the end of its authority, so colour, the word in front
/// of it and the order of the lines are all free to change. A banner parse that
/// quietly found nothing is how a dev server answering every request with
/// "Cannot GET /" once passed this file, so the one caller treats `None` as a
/// failure with the server's own account attached rather than as "carry on".
fn announced_port(said: &str) -> Option<u16> {
    let start = said.find("http://")? + "http://".len();
    let authority = said[start..]
        .split(|ch: char| ch == '/' || ch == '\u{1b}' || ch.is_whitespace())
        .next()?;
    let (_, port) = authority.rsplit_once(':')?;
    port.parse().ok()
}

/// Reading the port a server announced, without a server that announced one.
///
/// A test of a test helper, which is unusual and is the honest way to write
/// this one: what [`announced_port`] has to get right is the *shapes* a banner
/// comes in — coloured, uncoloured, with a second URL under the first — and
/// arranging those through a real dev server would be arranging them through
/// the thing under test.
mod announced {
    use super::announced_port;

    #[test]
    fn the_port_is_read_off_the_first_url_whatever_is_around_it() {
        assert_eq!(
            announced_port("\n  local  http://127.0.0.1:51873/\n  routes 12\n"),
            Some(51873)
        );
        // `uf dev` renders the URL through a tone, so the line arrives wrapped
        // in escape sequences on a terminal and bare when `NO_COLOR` is set.
        // Both are the same answer.
        assert_eq!(
            announced_port("  local  \u{1b}[36mhttp://127.0.0.1:4321/\u{1b}[0m"),
            Some(4321)
        );
        // The first, not the last: `--host` adds a `network` URL underneath,
        // and it is the same server on the same port.
        assert_eq!(
            announced_port("local http://127.0.0.1:8080/\nnetwork http://10.0.0.2:8080/"),
            Some(8080)
        );
    }

    #[test]
    fn nothing_is_nothing_rather_than_a_number_out_of_the_host() {
        assert_eq!(announced_port(""), None);
        assert_eq!(announced_port("uf dev\n  engine vite\n"), None);
        // No port in the authority. Splitting on the last `:` would otherwise
        // read `1` out of `127.0.0.1` and send every request somewhere absurd.
        assert_eq!(announced_port("local http://127.0.0.1/"), None);
    }
}

#[test]
fn dev_serves_the_docs_site_through_vite() {
    if !fixture_ready() || !loopback_ready() {
        return;
    }
    let root = docs_root();
    let said = Mutex::new(String::new());

    std::thread::scope(|scope| {
        // `--port 0`, and the server says which port it got. The alternative —
        // bind zero, read the number, close the listener, and hand it to `uf
        // dev` — is a race nothing manages: anything on the machine can take
        // the port in between, and Vite moving to the next free one produces a
        // server that is up somewhere this test is not asking about. That is
        // the second half of ubugeeei-prod/uf#234, and asking the operating
        // system once, through the process that will hold the socket, is the
        // fix the issue prefers to a retry.
        let mut server = Server::start(&root, &["dev", "--port", "0"], scope, &said);
        let Some(port) = server.bound_port(&said, Duration::from_secs(90)) else {
            panic!(
                "the dev server never announced a port\n{}",
                server.evidence(&said)
            );
        };
        // Then wait for the port to answer rather than trusting the line that
        // named it: `listening` is emitted from the driver, and what this test
        // is about is whether a request reaches a rendered page.
        let Some(body) = wait_for_http(port, "/", Duration::from_secs(90)) else {
            panic!(
                "the dev server announced port {port} and did not answer on it\n{}",
                server.evidence(&said)
            );
        };
        assert_page(&mut server, port, &said, &body);
        // Inside the scope on purpose: the drain threads end when the pipes
        // close, and the pipes close when the child does.
        drop(server);
    });
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

/// The dev server answers what the built application answers.
///
/// The docs site is a static site: [`dev_serves_the_docs_site_through_vite`]
/// proves `uf dev` renders pages, and it cannot prove anything about the half
/// of an application that is not a page, because the docs site has none of it.
/// `served-app` is the other project, and asking `uf dev` its questions is what
/// makes "development answers what production answers" a test rather than a
/// claim. Every assertion below is one that [`assert_served`] also makes of
/// `uf preview` and `uf start`.
///
/// It found two things that had been true for as long as `uf dev` had existed:
///
///   * a route handler was unreachable from a browser, because two middlewares
///     rendered every document and the earlier one could not dispatch —
///     ubugeeei-prod/uf#349;
///   * a `redirect()` from a loader answered with a 307 and no `Location`,
///     because the same middleware dropped the render's headers — #338.
///
/// Both are about a request whose *headers* decide the answer, which is why
/// the assertions read the status line and the headers rather than the body.
#[test]
fn dev_answers_the_fixture_the_way_a_build_does() {
    if !fixture_ready() || !loopback_ready() {
        return;
    }
    // The same lock the build test takes: both write `.uf/` under the fixture,
    // and a router manifest written by two processes at once is a file neither
    // of them wrote.
    let _served = served_lock();
    let root = served_app_root();
    let mut refused = Vec::new();

    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let said = Mutex::new(String::new());

        let served = std::thread::scope(|scope| {
            let mut server =
                Server::start(&root, &["dev", "--port", &port.to_string()], scope, &said);
            if let Some(body) = wait_for_http(port, "/", Duration::from_secs(90)) {
                assert_dev_served(&mut server, port, &said, &body);
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
        "the dev server never answered `served-app`, on {PORT_ATTEMPTS} different ports\n{}",
        refused.join("\n\n")
    );
}

/// Everything `uf dev` has to answer for `served-app`, once it is listening.
fn assert_dev_served(server: &mut Server, port: u16, said: &Mutex<String>, body: &str) {
    let context =
        |what: &str, response: &str| format!("`uf dev` {what}\n{response}\n{}", server_said(said));

    // The page, and the two scripts that say this really is the dev server
    // rather than a file being served from somewhere.
    assert!(
        body.starts_with("HTTP/1.1 200") && body.contains("served-app home"),
        "{}",
        context("did not render the home page", body)
    );
    assert!(
        body.contains("/@vite/client"),
        "{}",
        context("served a document Vite had not transformed", body)
    );

    // A route handler, asked exactly the way a browser asks: `Accept:
    // text/html`, no extension, `GET`. That is a *document* request by every
    // test the renderer can apply to it, which is why the handler was invisible
    // — the earlier of `uf dev`'s two middlewares rendered it, and only the
    // later one could dispatch. The failure was silent: the reader got the
    // route table's page for `/api/health`, or the not-found page, with a 200
    // or a 404 and no diagnostic anywhere.
    let health = get(server, port, "/api/health", said);
    assert!(
        health.starts_with("HTTP/1.1 200") && health.contains("\"status\":\"ok\""),
        "{}",
        context(
            "did not reach `app/api/health/_uf.route.js` for a request that looks like a \
             navigation; a route handler has to answer a browser too",
            &health
        )
    );
    assert!(
        !health.contains("served-app has no such page"),
        "{}",
        context(
            "answered a route handler's path with the not-found page",
            &health
        )
    );

    // And the method only a handler can answer, which is the other half of
    // dispatch: it must run for every method rather than only for what did not
    // look like a document.
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

    // A `redirect()` from a loader. Asserted on the status line and the header
    // and deliberately not on the body: the meta-refresh document the renderer
    // also produces is the *fallback* for a static host that can only serve a
    // file, and a server that sent only that would still pass a test that read
    // the body. A browser papers over the difference by obeying the refresh one
    // paint late; `curl -I`, a fetch that follows redirects and every other
    // client see a 307 pointing nowhere.
    let moved = get(server, port, "/old/hello-world", said);
    assert!(
        moved.starts_with("HTTP/1.1 307"),
        "{}",
        context("did not answer a loader's `redirect()` with a 307", &moved)
    );
    assert!(
        redirects_to(&moved, "/posts/hello-world"),
        "{}",
        context(
            "answered a redirect with no `Location`, so only a browser could follow it",
            &moved
        )
    );

    // A path with no route is the project's own 404 and not somebody else's
    // page, which is what says the renderer ran the router rather than a
    // fallback.
    let missing = get(server, port, "/definitely-not-a-page/", said);
    assert!(
        missing.starts_with("HTTP/1.1 404") && missing.contains("served-app has no such page"),
        "{}",
        context("did not serve the project's own not-found page", &missing)
    );

    // The browser's own channel back. Both endpoints under `/__uf/` answer,
    // and what arrives is rendered in this terminal — which is the whole of
    // ubugeeei-prod/uf#557 and #583: a number and a diagnostic the browser
    // produces had nowhere to go, so they existed only in a window that may
    // not be in front.
    let reported = http_request(
        "127.0.0.1",
        port,
        "POST",
        "/__uf/vitals",
        Some(
            "{\"url\":\"http://127.0.0.1/\",\"vitals\":\
             [{\"name\":\"LCP\",\"value\":4200,\"rating\":\"poor\",\
             \"navigationType\":\"navigate\"}]}",
        ),
    );
    assert!(
        reported.starts_with("HTTP/1.1 204"),
        "{}",
        context("did not accept a web-vitals report", &reported)
    );
    assert!(
        wait_for_said(said, "web vitals: LCP is poor", Duration::from_secs(30)),
        "{}",
        context("accepted the vitals report and never showed it", &reported)
    );

    let diagnosed = http_request(
        "127.0.0.1",
        port,
        "POST",
        "/__uf/diagnostic",
        Some("{\"severity\":\"error\",\"message\":\"Hydration mismatch in <Posted>\"}"),
    );
    assert!(
        diagnosed.starts_with("HTTP/1.1 204"),
        "{}",
        context("did not accept a browser diagnostic", &diagnosed)
    );
    assert!(
        wait_for_said(
            said,
            "Hydration mismatch in <Posted>",
            Duration::from_secs(30)
        ),
        "{}",
        context("accepted the diagnostic and never printed it", &diagnosed)
    );

    // And nothing under `/__uf/` is reachable as an application path, which is
    // what makes the prefix safe as a default destination: a directory in
    // `app/` whose name begins with `_` is not a route, so the fixture cannot
    // have one and a `GET` here is the wrong method rather than a page.
    let wrong_method = get(server, port, "/__uf/vitals", said);
    assert!(
        wrong_method.starts_with("HTTP/1.1 405"),
        "{}",
        context(
            "answered a GET on the vitals endpoint with something",
            &wrong_method
        )
    );
}

/// Whether a response carries `Location: target`, however it spelled the name.
///
/// Header names are case-insensitive and the two servers do differ: `uf dev`
/// writes the render result's own `Location` through `setHeader`, and the
/// production handler puts the same value through a `Headers`, which lowercases
/// it. Asserting one spelling would be asserting the wrong thing about a
/// difference that is not one.
fn redirects_to(response: &str, target: &str) -> bool {
    response.to_ascii_lowercase().contains(&format!(
        "\r\nlocation: {}\r\n",
        target.to_ascii_lowercase()
    ))
}

/// A `.env` file edited while `uf dev` runs is read again.
///
/// uf reads the `.env` cascade itself, in Rust, and turns Vite's own env-file
/// loading off so that `uf dev`, `uf build`, `uf test` and `uf run` cannot get
/// two answers — which left nobody watching the files. Editing one changed
/// nothing until the command was restarted by hand, and the guide documented
/// it as a limitation. See ubugeeei-prod/uf#428.
///
/// The value is read by a route handler through `process.env`, which is the
/// thing that can only change when the process does: `import.meta.env` would
/// have been a weaker assertion, because a value substituted into a module can
/// look fresh after a module reload without the process having been given a new
/// environment at all.
#[test]
fn dev_rereads_an_env_file_that_changed_under_it() {
    if !fixture_ready() || !loopback_ready() {
        return;
    }
    let mut files = minimal_app();
    files.push((
        "app/api/env/_uf.route.js",
        "// @flow\n\n\
         export function GET(): Response {\n  \
         return Response.json({ greeting: String(process.env.UF_WATCHED_GREETING) });\n\
         }\n",
    ));
    files.push((".env", "UF_WATCHED_GREETING=before the edit\n"));
    let project = Project::new(&files);
    let root = project.path().to_path_buf();
    let mut refused = Vec::new();

    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let said = Mutex::new(String::new());
        fs::write(root.join(".env"), "UF_WATCHED_GREETING=before the edit\n").unwrap();

        let served = std::thread::scope(|scope| {
            let mut server =
                Server::start(&root, &["dev", "--port", &port.to_string()], scope, &said);
            if wait_for_http(port, "/", Duration::from_secs(90)).is_none() {
                refused.push(format!(
                    "attempt {attempt} on port {port}: {}",
                    server.evidence(&said)
                ));
                drop(server);
                return false;
            }

            let before = get(&mut server, port, "/api/env", &said);
            assert!(
                before.contains("before the edit"),
                "the handler must read the value uf loaded before anything changed:\n{}",
                server.evidence(&said)
            );

            fs::write(root.join(".env"), "UF_WATCHED_GREETING=after the edit\n").unwrap();

            // Tolerant of a refused connection, because a restart is exactly
            // what is being waited for and the port is closed in the middle of
            // one. The same port throughout: `--port` carries `--strict-port`,
            // so a server that came back somewhere else is a failure rather
            // than something this quietly follows. And twice the first start's
            // budget, because a restart is a whole Vite start again — the
            // honest cost of re-substituting a value that reaches the browser.
            let after =
                wait_for_answer(port, "/api/env", "after the edit", Duration::from_secs(180));
            assert!(
                after.is_some(),
                "`uf dev` never picked up the edited `.env`:\n{}",
                server.evidence(&said)
            );
            assert!(
                said_contains(&said, "restarting with the new environment"),
                "the restart has to be said out loud, or a value that changed under a \
                 developer is a mystery:\n{}",
                server.evidence(&said)
            );
            true
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

/// Ask until the answer contains `needle`, forgiving a server that is restarting.
///
/// [`get`] is right for a server that has answered once and must keep
/// answering: it turns a refused connection into a failure that says so. This
/// is for the one case where a refused connection is the expected middle of
/// what is being tested.
fn wait_for_answer(port: u16, path: &str, needle: &str, budget: Duration) -> Option<String> {
    let deadline = Instant::now() + budget;
    while Instant::now() < deadline {
        if let Some(answer) = try_http_get(port, path)
            && answer.contains(needle)
        {
            return Some(answer);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// One request, or `None` if anything about it did not work.
fn try_http_get(port: u16, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/html\r\n\
         Connection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
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
    assert!(
        !root.join("dist/old").exists(),
        "the redirecting route must not be prerendered; a file would be served with a 200 and \
         the `Location` assertion below would be about a document rather than a redirect"
    );

    for command in ["preview", "start"] {
        serve_and_assert(&root, command);
    }
}

/// The script a deployed directory is asked with, when no socket may be had.
///
/// It is written *beside* the copied directory rather than inside it, and
/// imports it by a relative path — which is the assertion, not the setup. A
/// probe living inside the artefact could be resolving something the artefact
/// happens to sit next to; one outside it can only reach what was copied.
///
/// One script for the four server adapters, because the whole claim of the
/// seam is that they differ in one file. `node` and `container` are asked through
/// `handler.js` and own the request themselves, the way `server.js` does;
/// `edge` is asked through `worker.js`'s default export, with the two
/// arguments Cloudflare passes; `serverless` is asked through `lambda.js`'s
/// `handler`, with the payload format 2.0 event a Function URL sends. The
/// questions below are the same questions for all of them, and none of them
/// has a file behind it — so what is being compared is the application, and the
/// static halves stay out of it. `tests/library/deploy.test.js` is where those
/// are compared, because there they can be driven side by side.
///
/// The `ASSETS` stub is the only part of a platform this stands in for, and it
/// is one line of Cloudflare's documentation: the binding answers a `Request`
/// with a `Response`, and with a `404` where there is no such asset, which is
/// what `"not_found_handling": "none"` means.
const ARTEFACT_DOORS: &str = r#"import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const staticDir = path.join(here, "app", "static");

const ASSETS = {
  fetch: async (request) => {
    const pathname = decodeURIComponent(new URL(request.url).pathname);
    const resolved = path.resolve(staticDir, `.${pathname}`);
    const candidates = pathname.endsWith("/")
      ? [path.join(resolved, "index.html")]
      : [resolved, path.join(resolved, "index.html"), `${resolved}.html`];
    for (const candidate of candidates) {
      if (!candidate.startsWith(staticDir)) continue;
      if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
        return new Response(fs.readFileSync(candidate), {
          headers: { "content-type": "text/html; charset=utf-8" },
        });
      }
    }
    return new Response("not found", { status: 404 });
  },
};

// The probe is the host, so it owns the request the way `server.js` does:
// `beginRequest` from the artefact's own handler, `run` around answering, and
// `settle` once the body has been read — which for a `Response` a host only
// returns is the moment it has been sent. That the artefact hands out a
// `beginRequest` at all is half of what this asserts: it is the copy inlined
// into `handler.js`, and a host that used any other would establish a request
// the application cannot see. See ubugeeei-prod/uf#389.
async function applicationDoor() {
  const handler = (await import("./app/handler.js")).default;
  return async (request) => {
    const { run, settle } = handler.beginRequest(request);
    try {
      return await run(() => handler.fetch(request));
    } finally {
      await settle();
    }
  };
}

// A Worker owns the request itself, so the probe is only the runtime: the
// bindings and an execution context whose `waitUntil` is where `after()` goes.
async function workerDoor() {
  const worker = (await import("./app/worker.js")).default;
  const pending = [];
  return async (request) => {
    const response = await worker.fetch(request, { ASSETS }, { waitUntil: (p) => pending.push(p) });
    await Promise.all(pending.splice(0));
    return response;
  };
}

// And a Lambda owns it too, so what the probe does is speak the event format.
async function lambdaDoor() {
  const { handler } = await import("./app/lambda.js");
  return async (request) => {
    const url = new URL(request.url);
    const headers = { host: url.host };
    for (const [name, value] of request.headers) headers[name] = value;
    const method = request.method.toUpperCase();
    const result = await handler({
      version: "2.0",
      rawPath: url.pathname,
      rawQueryString: url.search.replace(/^\?/, ""),
      cookies: [],
      headers,
      body: method === "GET" || method === "HEAD" ? undefined : await request.text(),
      isBase64Encoded: false,
      requestContext: { domainName: url.host, http: { method, path: url.pathname } },
    });
    return new Response(
      result.isBase64Encoded ? Buffer.from(result.body, "base64") : result.body,
      { status: result.statusCode, headers: result.headers },
    );
  };
}

const doors = {
  node: applicationDoor,
  container: applicationDoor,
  edge: workerDoor,
  serverless: lambdaDoor,
};
const adapter = process.argv[2];
const answer = await doors[adapter]();

const ask = async (label, url, init) => {
  const request = new Request(`http://127.0.0.1${url}`, init);
  const response = await answer(request);
  const body = (await response.text()).replace(/\s+/g, " ");
  process.stdout.write(`${label} ${response.status} ${body}\n`);
};

// The same question, asked of a header instead of a body. A redirect's whole
// content is one, and the document it carries is a fallback for a static host
// rather than the answer — so a door that wrote the status and the body and
// dropped the headers would pass every `ask` above and still be broken.
const askHeader = async (label, url, name, init) => {
  const request = new Request(`http://127.0.0.1${url}`, init);
  const response = await answer(request);
  // Read anyway: for the application door this is the moment `settle` waits
  // for, and skipping it here would make one question of the six behave
  // differently from the rest.
  await response.text();
  const value = response.headers.get(name);
  process.stdout.write(`${label} ${response.status} ${name}=${value ?? "-"}\n`);
};

"#;

/// What the `served-app` artefact is asked.
///
/// Five questions, chosen so that each says something a static host could not:
/// a route handler for a `GET` and for a `POST` with a body, a route with
/// parameters and no `generateStaticParams`, a loader's `redirect()`, and the
/// project's own 404.
///
/// The redirect is the newest of them and was the gap. `uf dev` answered it
/// with a 307 carrying no `Location` (ubugeeei-prod/uf#338), and #602 fixed
/// that by passing a render's status *and headers* through unchanged — but its
/// report closed by noting that the question had been asked of `uf dev`,
/// `uf preview` and `uf start` and of no adapter. An artefact is a fourth
/// front door, and the only thing that keeps four front doors agreeing is
/// asking each of them. It goes through `askHeader` because the answer *is* a
/// header: a door that wrote the status and the body and lost the headers
/// answers 307 pointing nowhere, which a browser papers over by obeying the
/// meta refresh one paint late and which `curl -I` cannot follow at all.
const SERVED_APP_QUESTIONS: &str = r#"
await ask("handler-get", "/api/health");
await ask("handler-post", "/api/health", { method: "POST", body: JSON.stringify({ name: "uf" }) });
await ask("rendered", "/posts/hello-world");
await askHeader("redirect", "/old/hello-world", "location");
await ask("missing", "/definitely-not-a-page/");
// The two adapters whose entry carries a static half of its own: Cloudflare's
// asset server through the binding, and the copy inside the Lambda package.
if (adapter === "edge" || adapter === "serverless") {
  await ask("prerendered", "/guide/");
}
"#;

/// What the `rsc-split-app` artefact is asked: one server action, five ways.
///
/// The id is read out of the manifest the build copied into `static/`, because
/// it is keyed on a per-build secret and there is nowhere else it could come
/// from — which is the point of it. The five are the call itself, the call
/// with a cookie the action reads, and the three refusals a browser can be
/// made to attempt from somewhere else: another origin, an id nobody has, and
/// a content type a cross-origin form could have produced.
///
/// A `POST` to the page's own URL, because that is where an action call goes:
/// no path is reserved for it, and the middleware guarding that page is the
/// one that runs above the call.
const SERVER_ACTION_QUESTIONS: &str = r#"
// Substituted by the test rather than read out of the artefact. The manifest
// is not in the deployed artefact and must not be: ubugeeei-prod/uf#339 moved
// every build manifest out of the served directory, and a list of every server
// action with the ids the server dials is the last file to hand a browser.
// `deployed_action_id` reads it from the project's own `.uf/build/meta/`.
const actionId = "__UF_ACTION_ID__";
const post = (headers, body) => ({ method: "POST", headers, body });
const dialable = {
  origin: "http://127.0.0.1",
  host: "127.0.0.1",
  "content-type": "application/json",
  "uf-action": actionId,
};

await ask("action", "/counter", post(dialable, JSON.stringify({ args: [4] })));
await ask(
  "action-cookie",
  "/counter",
  post({ ...dialable, cookie: "visitor=ada" }, JSON.stringify({ args: [1] })),
);
await ask(
  "action-cross-origin",
  "/counter",
  post({ ...dialable, origin: "http://evil.example" }, JSON.stringify({ args: [1] })),
);
await ask(
  "action-unknown-id",
  "/counter",
  post({ ...dialable, "uf-action": "0".repeat(64) }, JSON.stringify({ args: [1] })),
);
await ask(
  "action-form-content-type",
  "/counter",
  post({ ...dialable, "content-type": "text/plain" }, "args=1"),
);
"#;

/// Build one adapter's artefact and copy it out of the checkout.
///
/// The copy is the point rather than the setup: in place proves nothing,
/// because `dist/`, `node_modules` and the source are all still there and an
/// artefact quietly reading one of them would pass. Returns the build's stdout
/// and the temporary directory holding `app/`, which the caller keeps alive.
/// The single server action's id, out of the build that just ran.
///
/// `.uf/build/meta/`, never `dist/` or a deploy artefact's `static/`: the
/// manifest is not served, by design (ubugeeei-prod/uf#339), so a test that
/// found it there would be reporting a disclosure rather than reading a value.
///
/// Read again after every build, because the id is an HMAC over a per-build
/// secret and each build mints a new one.
fn deployed_action_id(root: &Path) -> String {
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".uf/build/meta/uf-rsc-manifest.json"))
            .expect("`uf build` writes the RSC manifest beside the output directory"),
    )
    .expect("the RSC manifest is JSON");
    manifest["serverActions"][0]["id"]
        .as_str()
        .expect("the fixture declares exactly one server action")
        .to_owned()
}

fn deploy_and_copy(root: &Path, adapter: &str) -> (String, tempfile::TempDir) {
    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["build", "--adapter", adapter])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`uf build --adapter {adapter}` failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    for expected in ["adapter", &format!(".uf/deploy/{adapter}")] {
        assert!(
            stdout.contains(expected),
            "the summary must say what was written; missing {expected:?} in:\n{stdout}"
        );
    }

    let empty = tempfile::tempdir().unwrap();
    let deployed = empty.path().join("app");
    copy_tree(&root.join(format!(".uf/deploy/{adapter}")), &deployed);
    for ancestor in deployed.ancestors() {
        assert!(
            !ancestor.join("node_modules").exists(),
            "this test means nothing with a node_modules at {}",
            ancestor.display()
        );
    }
    (stdout, empty)
}

/// Ask the copied artefact `questions`, through [`ARTEFACT_DOORS`].
///
/// The doors are one half and the questions the other, because two fixtures
/// now have something to ask and the four ways into an artefact are the same
/// for both of them. A second copy of those four would be a second place for
/// the seam to be described, which is the thing this test exists to deny.
fn ask_the_artefact(empty: &Path, adapter: &str, questions: &str) -> String {
    fs::write(
        empty.join("ask.mjs"),
        format!("{ARTEFACT_DOORS}{questions}"),
    )
    .unwrap();
    let answered = Command::new("node")
        .arg("ask.mjs")
        .arg(adapter)
        .current_dir(empty)
        .output()
        .unwrap();
    let said = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&answered.stdout),
        String::from_utf8_lossy(&answered.stderr)
    );
    assert!(
        answered.status.success(),
        "the `{adapter}` artefact could not answer\n{said}"
    );
    String::from_utf8_lossy(&answered.stdout).into_owned()
}

/// The half of a probe's answers every adapter has to give identically.
///
/// Everything but `prerendered`, which only the two adapters carrying a static
/// half are asked for — a `node` artefact's static half is `server.js`'s, and
/// `server.js` takes a socket rather than answering a function call.
fn shared_answers(said: &str) -> Vec<&str> {
    said.lines()
        .filter(|line| !line.starts_with("prerendered "))
        .collect()
}

/// Assert on the answers themselves, once, for whichever adapter produced them.
///
/// The five are chosen so that each says something a static host could not: a
/// route handler for a `GET` and for a `POST` with a body, a route with
/// parameters and no `generateStaticParams`, a loader's `redirect()`, and the
/// project's own 404.
fn assert_artefact_answers(answers: &str) {
    for expected in [
        // A route handler, which is the clearest thing a build could not serve.
        "handler-get 200 {\"status\":\"ok\"}",
        // With a body, so the assertion is that the request reached the module
        // rather than that something answered 200.
        "handler-post 200 {\"echoed\":\"uf\"}",
    ] {
        assert!(
            answers.contains(expected),
            "missing {expected:?} in:\n{answers}"
        );
    }
    let rendered = answers
        .lines()
        .find(|line| line.starts_with("rendered "))
        .unwrap_or_else(|| panic!("no rendered line in:\n{answers}"));
    assert!(
        rendered.starts_with("rendered 200") && rendered.contains("post: hello-world"),
        "a route with no prerendered file has to be rendered per request:\n{rendered}"
    );
    // A loader's `redirect()`, whose whole answer is a header. `/old/[slug]`
    // has parameters and no `generateStaticParams`, so the build wrote no file
    // for it and this is a render — the same question [`assert_served`] asks
    // `uf preview` and `uf start`, asked here of the fourth front door. See
    // ubugeeei-prod/uf#338 and #602.
    let redirect = answers
        .lines()
        .find(|line| line.starts_with("redirect "))
        .unwrap_or_else(|| panic!("no redirect line in:\n{answers}"));
    assert_eq!(
        redirect, "redirect 307 location=/posts/hello-world",
        "an artefact has to answer a loader's `redirect()` with the status and the \
         `Location`, the way every other front door does:\n{answers}"
    );
    let missing = answers
        .lines()
        .find(|line| line.starts_with("missing "))
        .unwrap_or_else(|| panic!("no missing line in:\n{answers}"));
    assert!(
        missing.starts_with("missing 404") && missing.contains("served-app has no such page"),
        "an unrouted path is the project's own 404, not somebody else's page:\n{missing}"
    );
}

/// What has to be in an adapter's directory, per adapter.
///
/// The shared half first, because it is the seam: `handler.js`, the copy of
/// the build, and the `package.json` without which `node` and Lambda read
/// every `.js` beside them as CommonJS. Then the one entry that differs and
/// the platform file, if any, beside it.
fn assert_artefact_shape(adapter: &str, deployed: &Path) {
    assert!(deployed.join("handler.js").is_file());
    assert!(
        deployed.join("package.json").is_file(),
        "`.js` is CommonJS without it, on Node and on Lambda alike"
    );
    // The static half came along: the prerendered documents and the hashed
    // client assets. And the route that was *not* prerendered is still not
    // there, which is what makes the render assertion a render.
    assert!(deployed.join("static/index.html").is_file());
    assert!(deployed.join("static/guide/index.html").is_file());
    assert!(
        !deployed.join("static/posts").exists(),
        "a route with parameters and no `generateStaticParams` must reach the copy unprerendered"
    );

    match adapter {
        "node" => assert!(deployed.join("server.js").is_file()),
        "container" => {
            assert!(deployed.join("server.js").is_file());
            let dockerfile = fs::read_to_string(deployed.join("Dockerfile")).unwrap();
            assert!(
                dockerfile.contains("CMD [\"node\", \"server.js\"]"),
                "the image has to start the server this directory carries:\n{dockerfile}"
            );
            assert!(
                dockerfile.contains("A template"),
                "the first line has to say what it is, because it is not a supported \
                 configuration:\n{dockerfile}"
            );
            let ignored = fs::read_to_string(deployed.join(".dockerignore")).unwrap();
            assert!(ignored.contains("Dockerfile"));
        }
        "edge" => assert_worker_shape(deployed),
        "serverless" => {
            let lambda = fs::read_to_string(deployed.join("lambda.js")).unwrap();
            assert!(
                lambda.contains("export { handler }"),
                "the function's configured handler is `lambda.handler`:\n{lambda}"
            );
        }
        other => panic!("no shape is written down for the `{other}` adapter"),
    }
}

/// The Worker's own half: `wrangler.json`, and what the bundle needs from it.
///
/// Every assertion here is a line of Cloudflare's documented configuration
/// schema, and each one is load-bearing rather than decorative — which is why
/// they are asserted rather than left to be read. The last is the one that
/// would otherwise rot silently: the bundle is linked with `workerd` first in
/// the export conditions so that React resolves to `server.edge.js`, and a
/// change that lost that would pull in `server.node.js`, whose `node:stream`
/// would arrive with no line here going red.
fn assert_worker_shape(deployed: &Path) {
    assert!(deployed.join("worker.js").is_file());
    assert!(
        !deployed.join("server.js").exists(),
        "a Worker takes no socket, so there is nothing for a `server.js` to do here"
    );

    let wrangler: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(deployed.join("wrangler.json")).unwrap()).unwrap();
    assert_eq!(wrangler["main"], "./worker.js");
    assert_eq!(wrangler["compatibility_flags"][0], "nodejs_compat");
    assert_eq!(wrangler["assets"]["directory"], "./static/");
    assert_eq!(wrangler["assets"]["binding"], "ASSETS");
    // The Worker asks for an asset before the application answers, which is
    // what puts the resolution order in uf rather than in a platform setting.
    assert_eq!(wrangler["assets"]["run_worker_first"], true);
    // And a miss falls through, so the 404 a visitor sees is the project's own.
    assert_eq!(wrangler["assets"]["not_found_handling"], "none");

    let mut imported = Vec::new();
    let mut files = vec![deployed.join("worker.js"), deployed.join("handler.js")];
    if let Ok(chunks) = fs::read_dir(deployed.join("chunks")) {
        files.extend(chunks.map(|entry| entry.unwrap().path()));
    }
    for file in files {
        let source = fs::read_to_string(&file).unwrap();
        for (at, _) in source.match_indices("\"node:") {
            let rest = &source[at + 1..];
            let name = &rest[..rest.find('"').unwrap_or(0)];
            if !imported.contains(&name.to_owned()) {
                imported.push(name.to_owned());
            }
        }
    }
    imported.sort();
    // `node:async_hooks` alone, and `nodejs_compat` in `wrangler.json` is what
    // provides it: the request context is an `AsyncLocalStorage`, so the flag
    // is not optional and the script does not link without it. Anything else
    // appearing here is a decision somebody has to make about a compatibility
    // date rather than a line to relax.
    assert_eq!(
        imported,
        vec!["node:async_hooks".to_owned()],
        "the edge bundle's Node built-ins decide what `wrangler.json` has to ask for"
    );
}

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

    let (stdout, empty) = deploy_and_copy(&root, "node");
    assert!(
        stdout.contains("node server.js"),
        "the summary must say how to run it; missing \"node server.js\" in:\n{stdout}"
    );
    let deployed = empty.path().join("app");
    assert_artefact_shape("node", &deployed);
    assert_artefact_answers(&ask_the_artefact(
        empty.path(),
        "node",
        SERVED_APP_QUESTIONS,
    ));

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

/// The other three adapters, and the one thing they may not differ in.
///
/// `uf build --adapter node` has its own test above, because it is the one
/// with a socket to take. This is the rest of ubugeeei-prod/uf#391: `edge`,
/// `serverless` and `container`, each built for real, each copied to a
/// directory with no `node_modules` anywhere above it, and each asked the same
/// four questions through the entry its platform would call — a Worker's
/// `export default { fetch }`, a Lambda's `handler(event)`, and for the
/// container the same `handler.js` the Node adapter writes.
///
/// The assertion is that the four answers are **byte-identical**, `node`
/// included. That is the whole claim of the seam: `createFetchHandler` is one
/// function, an adapter is the file wrapped around it, and an adapter that
/// answered differently would be a second application wearing the first one's
/// name. It is a stronger statement than it looks for `edge`, which is linked
/// against a different build of React — `server.edge.js` rather than
/// `server.node.js`, so the document comes out of `renderToReadableStream`
/// rather than `renderToPipeableStream` — and still comes out the same.
///
/// # None of this has run on Cloudflare or on AWS
///
/// Nor has the container been built: this sandbox has no Docker daemon, no
/// cloud credentials and no socket. What is established here is that the
/// directory is complete, that its shape is the platform's documented one, and
/// that the application inside it answers. Deploying it is a step nobody has
/// taken, and `docs/app/reference/cli/_uf.page.mdx` says so in those words.
#[test]
fn every_adapter_answers_exactly_what_the_node_adapter_answers() {
    if !fixture_ready() {
        return;
    }
    let _served = served_lock();
    let root = served_app_root();

    let mut reference: Option<(&str, Vec<String>)> = None;
    for adapter in ["node", "edge", "serverless", "container"] {
        let (_, empty) = deploy_and_copy(&root, adapter);
        assert_artefact_shape(adapter, &empty.path().join("app"));

        let answers = ask_the_artefact(empty.path(), adapter, SERVED_APP_QUESTIONS);
        assert_artefact_answers(&answers);

        let shared: Vec<String> = shared_answers(&answers)
            .iter()
            .map(|line| (*line).to_owned())
            .collect();
        match &reference {
            None => reference = Some((adapter, shared)),
            Some((first, expected)) => {
                similar_asserts::assert_eq!(
                    &shared,
                    expected,
                    "the `{}` adapter and the `{}` adapter answered differently",
                    adapter,
                    first
                );
            }
        }

        // And the static half, for the two whose entry carries one: the
        // Worker's through the `ASSETS` binding, the Lambda's out of the
        // deployment package. Both have to answer the prerendered document
        // rather than render the page again — which is the resolution order
        // `uf preview` fixes for everybody.
        if adapter == "edge" || adapter == "serverless" {
            let prerendered = answers
                .lines()
                .find(|line| line.starts_with("prerendered "))
                .unwrap_or_else(|| panic!("no prerendered line in:\n{answers}"));
            assert!(
                prerendered.starts_with("prerendered 200")
                    && prerendered.contains("served-app guide"),
                "a path the build wrote a file for is answered with the file:\n{prerendered}"
            );
        }
    }
}

/// `--adapter static` refuses the project it cannot serve, and names it.
///
/// The one adapter whose whole implementation is a sentence. `uf build` has
/// always written `dist/`, and a static host is a thing that returns files
/// from it — so this target emits nothing new and what it had to grow is the
/// refusal. Until it existed, `served-app` built, uploaded, and 404'd on its
/// route handler and on all three of its parameterised routes, with nothing
/// between the build and the visitor saying so. That is item 4 of
/// ubugeeei-prod/uf#335, and `ubugeeei-redundancy.md` in the words it uses:
/// static hosting does not become a server merely because an adapter exists,
/// and an unsupported configuration is rejected clearly rather than silently
/// changing semantics.
///
/// Four findings, from two different places. `/api/health` is a
/// `_uf.route.js`, which has no page and therefore appears in no `Route` at
/// all — the route table alone would have called this project static. The
/// three parameterised routes are in the table and have no prerendered
/// document, which is a fact only the prerender has.
///
/// And the directory is not written. A refusal that had already emptied
/// `.uf/deploy/static/` would have destroyed a previous artefact on its way to
/// telling somebody they cannot have a new one.
#[test]
fn the_static_adapter_refuses_a_project_a_static_host_cannot_serve() {
    if !fixture_ready() {
        return;
    }
    let _served = served_lock();
    let root = served_app_root();

    let output = uf()
        .arg("--cwd")
        .arg(&root)
        .args(["build", "--adapter", "static"])
        .output()
        .unwrap();

    let said = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "a project with a route handler and three unprerendered routes is not a static \
         site, and a build that said it was is the failure this target exists to \
         prevent\n{said}"
    );
    for expected in [
        // The handler: what, where, and why a file is not it.
        "/api/health",
        "app/api/health/_uf.route.js",
        "a route handler answers a request",
        // Every parameterised route, not the first one: a reader fixing this
        // wants the list rather than one round trip per route.
        "/posts/:slug",
        "/slow/:id",
        "/old/:slug",
        "generateStaticParams",
        // And what to do instead, which is the half a reader who chose this
        // target on purpose actually needs.
        "--adapter node",
        "issues/335",
    ] {
        assert!(said.contains(expected), "missing {expected:?} in:\n{said}");
    }
    assert!(
        !root.join(".uf/deploy/static").exists(),
        "a refusal must not have emptied the directory on its way to refusing"
    );
}

/// A server action is the fourth thing a static host cannot answer, and this
/// is the project that has one and nothing else.
///
/// `rsc-split-app` is two pages, both prerendered, with no handler, no
/// middleware and no parameter — so every other reason is absent and the one
/// finding is the action. That isolation is the assertion: an action is a
/// `POST` the browser makes back to the application, at the page's own URL,
/// and a directory of files has nothing to answer it with. A project that
/// deployed this statically would render a counter that silently did nothing
/// when it was clicked.
#[test]
fn the_static_adapter_names_a_server_action_a_static_host_cannot_answer() {
    if !fixture_ready() {
        return;
    }
    let _split = split_lock();
    let root = rsc_split_app_root();

    let output = uf()
        .arg("--cwd")
        .arg(&root)
        .args(["build", "--adapter", "static"])
        .output()
        .unwrap();

    let said = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "{said}");
    assert!(
        said.contains("a server action is a `POST`"),
        "the reason has to be the action's, not a route's:\n{said}"
    );
    // By its export and its module, because that is the name in the source a
    // reader has to go and look at — the URL it is dialled at is the page's.
    assert!(said.contains("recordCount"), "{said}");
    assert!(said.contains("app/counter/_actions/tally.js"), "{said}");
    // The two pages are prerendered and are not findings, which is what makes
    // the single row above mean the action rather than the project.
    assert!(
        !said.contains("generateStaticParams"),
        "this project has no parameterised route; a finding about one would mean \
         the check is reporting the project rather than the action:\n{said}"
    );
}

/// And the project it *can* serve: uf's own documentation.
///
/// The positive half, and it is uf's own site rather than a fixture on
/// purpose — 37 pages, no route handler, no middleware, no parameter and no
/// server action, which is what a static deployment is. `rendering.modes` says
/// `["ssg"]` and this is the command that makes that a checked claim rather
/// than a field in a config file.
///
/// Three things are asserted about the output, and each of them is a decision:
///
/// * it is `dist/docs` **file for file**, because the artefact of a static
///   target is the build's own output and anything else in it would be a
///   second answer to what the site contains;
/// * the copy is at the top of the directory rather than in a `static/`
///   beside a server, because here the directory *is* the site and a wrapper
///   would put every URL one segment deeper than the build decided; and
/// * there is no `handler.js`, no `server.js` and no `package.json`. The four
///   server targets need all three. A static host serves whatever is in the
///   directory, so a stray `package.json` is a file a visitor can fetch at a
///   URL the application never mentioned.
#[test]
fn the_static_adapter_writes_the_site_it_can_serve() {
    if !fixture_ready() {
        return;
    }
    let _dist = dist_lock();
    let root = docs_root();

    let output = uf()
        .arg("--cwd")
        .arg(&root)
        .args(["build", "--adapter", "static"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "uf's own documentation is a static site and has to build as one\nstdout:\n{stdout}\n\
         stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_plain(&stdout);
    assert!(
        stdout.contains(".uf/deploy/static"),
        "the summary must say what was written:\n{stdout}"
    );
    // Not `node server.js`: there is nothing to start, and naming a hosting
    // company's upload command would be uf choosing one on the reader's
    // behalf.
    assert!(
        stdout.contains("upload the contents of"),
        "what happens next to a static site is an upload:\n{stdout}"
    );

    let deployed = root.join(".uf/deploy/static");
    let dist = root.join("dist/docs");
    assert!(deployed.join("index.html").is_file());
    assert!(deployed.join("404.html").is_file());
    assert!(deployed.join("sitemap.xml").is_file());
    assert!(
        deployed.join("guide/routing/index.html").is_file(),
        "a nested page has to arrive at the URL the build gave it"
    );
    for absent in [
        "handler.js",
        "server.js",
        "package.json",
        "static",
        "chunks",
    ] {
        assert!(
            !deployed.join(absent).exists(),
            "`{absent}` belongs to a target that runs an application, and this one does not"
        );
    }

    let mut copied = relative_files(&deployed);
    let mut built = relative_files(&dist);
    copied.sort();
    built.sort();
    assert_eq!(
        copied, built,
        "the artefact of a static target is the build's own output, file for file"
    );
    assert!(
        built.len() > 30,
        "a docs site of {} files is not the one this asserts about",
        built.len()
    );
}

/// Every file under `directory`, as paths relative to it.
fn relative_files(directory: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                found.push(
                    path.strip_prefix(directory)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    found
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
/// The questions are the halves of a uf build, and before `uf preview` and
/// `uf start` existed a build could answer only the first two.
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

    // 4b. A `redirect()` from a loader, which is the one answer whose whole
    //     content is a header: `app/old/[slug]` has parameters and no
    //     `generateStaticParams`, so no file was written for it and this is a
    //     render. Asked of both servers because `uf dev` used to answer it
    //     with a 307 carrying no `Location` — see
    //     [`dev_answers_the_fixture_the_way_a_build_does`] — and the only way
    //     that stays fixed is if all three are asked the same question.
    let moved = get(server, port, "/old/hello-world", said);
    assert!(
        moved.starts_with("HTTP/1.1 307"),
        "{}",
        context("did not answer a loader's `redirect()` with a 307", &moved)
    );
    assert!(
        redirects_to(&moved, "/posts/hello-world"),
        "{}",
        context("answered a redirect with no `Location`", &moved)
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

    // 5a. The two files a crawler asks for, and the three it must not get.
    //
    //     `uf build` writes its own manifests beside the output directory
    //     rather than inside it, so this is what a deployed application
    //     answers for them: nothing. Asked of the server rather than of
    //     `dist/` because that is the shape of ubugeeei-prod/uf#339 — the
    //     build has written them there since the manifest existed, and it only
    //     became a disclosure once there was a server in front of the
    //     directory. Both servers are asked, because they are two front doors
    //     to one build.
    for leaked in [
        "/uf-build-manifest.json",
        "/uf-rsc-manifest.json",
        "/uf-bundle-report.json",
    ] {
        let response = get(server, port, leaked, said);
        assert!(
            response.starts_with("HTTP/1.1 404"),
            "{}",
            context(&format!("served {leaked}"), &response)
        );
    }

    let sitemap = get(server, port, "/sitemap.xml", said);
    assert!(
        sitemap.starts_with("HTTP/1.1 200"),
        "{}",
        context("did not serve the sitemap", &sitemap)
    );
    assert!(
        sitemap.contains("<loc>https://served.example/</loc>")
            && sitemap.contains("<loc>https://served.example/guide</loc>"),
        "{}",
        context("the sitemap is missing a prerendered page", &sitemap)
    );
    //     The three subtractions, on a live server. `/posts/:slug` and
    //     `/slow/:id` have no `generateStaticParams`, so nothing — this build
    //     included — knows what their URLs are; `/404` is a document that is
    //     served and is not a page.
    for absent in ["/posts", "/slow", "/old", "/404"] {
        assert!(
            !sitemap.contains(&format!("https://served.example{absent}")),
            "{}",
            context(
                &format!("the sitemap names {absent}, which is not a URL it can stand behind"),
                &sitemap
            )
        );
    }

    let robots = get(server, port, "/robots.txt", said);
    assert!(
        robots.starts_with("HTTP/1.1 200")
            && robots.contains("Sitemap: https://served.example/sitemap.xml"),
        "{}",
        context("did not serve a robots.txt naming the sitemap", &robots)
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
    //
    //    It is the command with every non-alphanumeric character replaced
    //    rather than the command itself, because `command` is a label as much
    //    as a key and one caller passes a whole command *line*: `build
    //    --adapter node`. Interpolated into the target that spells
    //    `GET /slow/build --adapter node HTTP/1.1`, which is not a request
    //    line at all — a target may not contain a space — so Node's parser
    //    refuses it and its default `clientError` handler answers a bare
    //    `400 Bad Request`, before `nodeListener` or any other uf code runs.
    //    That looked for a long time like a streaming failure in the adapter
    //    and was never anything but these bytes. Substituting keeps the only
    //    property the id needs, which is being different for each server;
    //    `preview` and `start` are unchanged by it.
    let slow_id: String = command
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slow = timed_get(port, &format!("/slow/{slow_id}"));
    assert!(
        slow.text.starts_with("HTTP/1.1 200"),
        "{}",
        context("did not render the suspending route", &slow.evidence())
    );
    let shell = slow.first_at("slow: waiting").unwrap_or_else(|| {
        panic!(
            "{}",
            context("never sent the `_uf.loading.js` fallback", &slow.evidence())
        )
    });
    let page = slow
        .first_at(&format!("slow: {slow_id}"))
        .unwrap_or_else(|| {
            panic!(
                "{}",
                context("the suspended page never arrived", &slow.evidence())
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
            &slow.evidence()
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
    /// The literal bytes written to the socket.
    ///
    /// Kept because the failure this struct is most likely to report is one
    /// where they are the whole answer; see [`TimedResponse::evidence`].
    request: String,
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

    /// What was sent, what came back, and on what connection.
    ///
    /// A bare `400 Bad Request` from this probe cost a long search through the
    /// streaming renderer, because the failure said only "did not render the
    /// suspending route" and printed a response with no body to say otherwise.
    /// Three facts end that search, and all three are here: the request as it
    /// actually went on the wire, since a request *target* with a space in it
    /// is not a request line and Node answers those itself; the whole
    /// response rather than the part an assertion looked at; and that this
    /// connection carried nothing before this request, which rules out the
    /// other way a bare `400` with `Connection: close` happens — a response
    /// whose framing left a reused connection out of sync.
    fn evidence(&self) -> String {
        let mut evidence = format!(
            "the request, as it went on the wire:\n{}\n\nthe whole response:\n{}\n\n\
             the connection was opened for this request alone and nothing was written to it \
             first, so a desynchronised reused connection is not what this is.",
            indent(&visible(&self.request)),
            indent(if self.text.is_empty() {
                "<nothing: the server closed without writing a byte>"
            } else {
                self.text.as_str()
            }),
        );
        // Node's `http.Server` writes exactly this, from its default
        // `clientError` handler, when the parser rejects the bytes before a
        // request object exists. `nodeListener` never runs — which is why the
        // body is empty and the server's stderr says nothing — and no uf code
        // can produce it, because uf answers a handler that threw with a 500
        // and a body.
        if self
            .text
            .starts_with("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n")
        {
            evidence.push_str(
                "\n\nthat response is byte for byte Node's own `clientError` reply, which \
                 means its parser refused the request above before any uf code ran. Read the \
                 request line first: a target containing a space is the usual reason.",
            );
        }
        evidence
    }
}

/// The diagnosis a bare `400` has to carry, checked without a socket.
///
/// This is the failure that cost the search: the response to the suspending
/// route was a `400` with no body, and the message printed that and nothing
/// else — so the search went to the streaming renderer and the adapter, and
/// the answer was in the request line all along.
///
/// The evidence is assembled from data rather than read off a connection, so
/// this runs on a machine that cannot bind one. That is deliberate: the
/// machine where the original failure could not be reproduced at all is
/// exactly the machine where the message explaining it has to be readable.
#[test]
fn a_bare_400_says_it_is_node_refusing_the_request_line() {
    let refused = TimedResponse {
        request: "GET /slow/build --adapter node HTTP/1.1\r\nHost: 127.0.0.1:46335\r\n\
                  Accept: text/html\r\nConnection: close\r\n\r\n"
            .to_owned(),
        text: "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n".to_owned(),
        reads: Vec::new(),
    };
    let evidence = refused.evidence();
    assert!(
        evidence.contains("GET /slow/build --adapter node HTTP/1.1"),
        "the request line is the answer, so it has to be in the message:\n{evidence}"
    );
    assert!(
        evidence.contains("clientError"),
        "a bare 400 is Node's own, and the message has to say so rather than leave it to be \
         rediscovered:\n{evidence}"
    );
    assert!(
        evidence.contains("reused connection"),
        "the other way a bare 400 with `Connection: close` happens has to be ruled out in the \
         message:\n{evidence}"
    );
}

/// CRLF made visible, so a request line can be read for what it is.
fn visible(raw: &str) -> String {
    raw.replace('\r', "\\r").replace('\n', "\\n\n")
}

/// Two spaces in front of every line, so a quoted document is not read as the
/// failure message's own words.
fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
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
    // Built before it is written, and kept, because it is the first thing a
    // reader of a failure here needs; see `TimedResponse::evidence`.
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/html\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).unwrap();

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
        request,
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

/// A project whose configuration comes from `.env` files, on both sides of the
/// client boundary.
///
/// The page reads two variables through `import.meta.env` — one behind the
/// client prefix and one not — and a route handler reads a third through
/// `process.env`, which is what server code does. Three files, so that which
/// value arrives says which mode the command ran in.
///
/// `UF_SECRET_TOKEN` is the one that matters. It is read by code that ships to
/// the browser, and its value must not be in `dist/` anywhere: the prefix is
/// the whole boundary between a build-time value and a credential in a public
/// bundle.
///
/// Which is why the page renders a `"use client"` component. A route with no
/// client boundary keeps its page out of the browser bundle entirely — see
/// [`the_client_bundle_loses_a_route_that_needs_no_javascript`] — and a bundle
/// with no page in it holds no substituted value either, so every assertion
/// here about what does and does not ship would pass by shipping nothing. The
/// boundary is what gives the negative assertions something to be false about.
fn project_reading_the_environment() -> Project {
    let mut files = minimal_app();
    files.push((
        "app/_uf.page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\n\
         import Reader from \"./_components/Reader.js\";\n\n\
         const server =\n  \
         typeof process === \"undefined\" ? \"no server here\" : \
         String(process.env.UF_SERVER_VALUE);\n\n\
         export component Page() {\n  \
         return (\n    \
         <main>\n      \
         <p>greeting: {String(import.meta.env.VITE_GREETING)}</p>\n      \
         <p>secret: {String(import.meta.env.UF_SECRET_TOKEN)}</p>\n      \
         <p>server: {server}</p>\n      \
         <Reader />\n    \
         </main>\n  \
         );\n\
         }\n",
    ));
    // The client half, reading the same two names the page does: the prefixed
    // one is substituted into what the browser downloads and the other is not,
    // and this module is unambiguously in that download.
    files.push((
        "app/_components/Reader.js",
        "\"use client\";\n// @flow\nimport * as React from \"@uniflowed/react\";\n\n\
         export default component Reader() {\n  \
         return (\n    \
         <aside>\n      \
         <p>client greeting: {String(import.meta.env.VITE_GREETING)}</p>\n      \
         <p>client secret: {String(import.meta.env.UF_SECRET_TOKEN)}</p>\n    \
         </aside>\n  \
         );\n\
         }\n",
    ));
    files.push((
        "app/api/env/_uf.route.js",
        "// @flow\n\n\
         export function GET(): Response {\n  \
         return Response.json({ server: String(process.env.UF_SERVER_VALUE) });\n\
         }\n",
    ));
    files.push((
        ".env",
        "VITE_GREETING=from .env\n\
         UF_SERVER_VALUE=from .env\n\
         UF_SECRET_TOKEN=this-must-not-be-in-the-bundle\n",
    ));
    files.push((
        ".env.development",
        "VITE_GREETING=greeting for development\nUF_SERVER_VALUE=server value for development\n",
    ));
    files.push((
        ".env.production",
        "VITE_GREETING=greeting for production\nUF_SERVER_VALUE=server value for production\n",
    ));
    Project::new(&files)
}

/// Every `.js` the browser downloads from a build, as one string.
fn client_assets(dist: &Path) -> String {
    let mut source = String::new();
    let assets = dist.join("assets");
    let entries = fs::read_dir(&assets)
        .unwrap_or_else(|error| panic!("the build wrote no {}: {error}", assets.display()));
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "js") {
            source.push_str(&fs::read_to_string(&path).unwrap());
        }
    }
    assert!(!source.is_empty(), "the build wrote no client JavaScript");
    source
}

/// The build reads `.env`, and only the prefixed half reaches the browser.
///
/// This is the half of ubugeeei-prod/uf#259 that is a security property rather
/// than a convenience: everything in the cascade is available to server code,
/// and a name without the client prefix must be absent from what ships. The
/// negative assertion is the point — a build that inlined the whole environment
/// would pass every other test in this file.
#[test]
fn the_build_reads_env_files_and_ships_only_the_prefixed_ones() {
    if !fixture_ready() {
        return;
    }
    let project = project_reading_the_environment();

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

    let dist = project.path().join("dist");
    let index = fs::read_to_string(dist.join("index.html")).expect("the home page is prerendered");
    // `production`, because that is `uf build`'s mode — so `.env.production`
    // won over `.env`, and the value a dev server would have used is nowhere.
    assert!(
        index.contains("greeting for production"),
        "the build did not read `.env.production`:\n{index}"
    );
    assert!(
        !index.contains("greeting for development"),
        "the build read the development file:\n{index}"
    );
    // A page reading a name without the prefix gets nothing. `undefined` and
    // not the value, with React's own comment separator in between.
    assert!(
        index.contains("undefined</p>"),
        "a variable without the client prefix must not reach the page:\n{index}"
    );

    // The server half of the same page: `process.env` is read by the module
    // that renders it, and the prerender ran on a process uf had given the
    // whole environment to. This is the half a route handler and a loader use.
    assert!(
        index.contains("server value for production"),
        "server code must read every variable through `process.env`:\n{index}"
    );

    let client = client_assets(&dist);
    assert!(
        client.contains("greeting for production"),
        "a prefixed variable is substituted into the browser bundle, and was not"
    );
    // And the value that page read on the server is *not* in what ships, even
    // though the module that reads it does: only a prefixed name is
    // substituted, everything else stays a lookup that finds nothing in a
    // browser.
    assert!(
        !client.contains("server value for production"),
        "a variable without the client prefix was inlined into the browser bundle"
    );
    // The assertion this test exists for.
    for file in walk_files(&dist) {
        let bytes = fs::read(&file).unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("this-must-not-be-in-the-bundle"),
            "a variable without the client prefix reached {}",
            file.display()
        );
    }
}

/// Every file under `directory`, however deep.
fn walk_files(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

/// Start `uf <args>` in `root`, wait for `/`, and ask it what it read.
///
/// The retries are [`PORT_ATTEMPTS`]' — the port is chosen by binding zero and
/// letting it go, so anything on the machine can take it in between, and a
/// typed port is a strict one, so losing that race is a server that never
/// starts rather than one on a port nobody asked about.
fn serve_and_ask(
    root: &Path,
    args: &[&str],
    check: impl Fn(&mut Server, u16, &Mutex<String>, &str),
) {
    let mut refused = Vec::new();
    for attempt in 1..=PORT_ATTEMPTS {
        let port = free_port();
        let port_text = port.to_string();
        let mut with_port: Vec<&str> = args.to_vec();
        with_port.extend(["--port", &port_text]);
        let said = Mutex::new(String::new());

        let served = std::thread::scope(|scope| {
            let mut server = Server::start(root, &with_port, scope, &said);
            if let Some(body) = wait_for_http(port, "/", Duration::from_secs(90)) {
                check(&mut server, port, &said, &body);
                return true;
            }
            refused.push(format!(
                "attempt {attempt} on port {port}: {}",
                server.evidence(&said)
            ));
            // Inside the scope: the drain threads end when the pipes close.
            drop(server);
            false
        });
        if served {
            return;
        }
    }
    panic!(
        "`uf {}` never answered, on {PORT_ATTEMPTS} different ports\n{}",
        args.join(" "),
        refused.join("\n\n")
    );
}

/// The dev server and `uf start` read the same files the build did, each in its
/// own mode.
///
/// Both halves in one test and one build, because the defect in
/// ubugeeei-prod/uf#259 is precisely that two commands can disagree: `uf dev`
/// must serve the development value and `uf start` the production one, from the
/// same project, without either being told anything the other was not. The
/// route handler is the server half — `process.env` in code that never reaches
/// a browser — and the page is the client half.
///
/// `uf start` is asked *after* the file it reads has been rewritten, which is
/// what says the loading happens when the server starts rather than when the
/// bundle was built. A deployment that had to rebuild to change a database URL
/// would not be a deployment.
#[test]
fn the_dev_server_and_the_production_server_each_read_their_own_mode() {
    if !fixture_ready() || !loopback_ready() {
        return;
    }
    let project = project_reading_the_environment();
    let root = project.path().to_path_buf();

    // The dev server first, on the source: `development` is its mode, so
    // `.env.development` is the file that wins.
    serve_and_ask(&root, &["dev"], |server, port, said, body| {
        assert!(
            body.contains("greeting for development"),
            "`uf dev` must read `.env.development`:\n{body}"
        );
        assert!(
            body.contains("undefined</p>"),
            "`uf dev` must not hand a page a variable without the client prefix:\n{body}"
        );
        assert!(
            !body.contains("this-must-not-be-in-the-bundle"),
            "`uf dev` served a variable that has no client prefix:\n{body}"
        );
        let api = get(server, port, "/api/env", said);
        assert!(
            api.contains("server value for development"),
            "a route handler must read the environment through `process.env`:\n{api}"
        );
    });

    let build = uf().arg("--cwd").arg(&root).arg("build").output().unwrap();
    assert!(
        build.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    // Rewritten after the build and before the server: what `uf start` answers
    // has to come from the file it read on the way up.
    fs::write(
        root.join(".env.production"),
        "VITE_GREETING=greeting for production\nUF_SERVER_VALUE=changed after the build\n",
    )
    .unwrap();

    // `uf start` binds every interface by default, which is right for a
    // production server and wrong for a test on somebody's laptop.
    serve_and_ask(
        &root,
        &["start", "--host", "127.0.0.1"],
        |server, port, said, body| {
            assert!(
                body.contains("greeting for production"),
                "`uf start` must serve the production build:\n{body}"
            );
            let api = get(server, port, "/api/env", said);
            assert!(
                api.contains("changed after the build"),
                "`uf start` must read `.env.production` when it starts, not at build time:\n{api}"
            );
        },
    );
}

/// The server/client split, asserted on the bundle rather than on the analysis.
///
/// `crates/uf_rsc` has been able to say which modules a `"use client"`
/// boundary is reachable from since it was written, and every test of that
/// answer passed while the build ignored it: `virtual:uf/routes` emitted
/// `page: () => import(<file>)` for every route and `virtual:uf/client`
/// imported that table, so every page in the application was a chunk of the
/// *client* bundle. A test over the analysis is a test that already passed.
/// This one reads the emitted JavaScript.
///
/// `tests/fixtures/rsc-split-app` has two routes and one import between them:
/// `/counter` renders a `"use client"` component and `/` renders a module in
/// `app/_content/`. Each carries a marker *string*, because a production
/// bundle renames identifiers and keeps string literals, so a marker is the
/// only thing a grep over `dist/assets/*.js` can be about.
///
/// Five things have to hold at once, and each of the first three is a way the
/// change could be wrong rather than absent:
///
/// 1. the counter's marker is in the client bundle — a split that dropped the
///    route the browser needs would be worse than no split;
/// 2. the almanac's is not, and neither is the static page's — the module and
///    the subtree only it reached are gone;
/// 3. both routes still prerender, and the interactive one still gets its
///    hydration script;
/// 4. `uf build`'s summary says `1 of 2`, and it says it because the bundler
///    reported what it emitted rather than because uf predicted it;
/// 5. the manifest published beside the build carries the same decision.
///
/// The stylesheet assertion inside (3) is the one that is not obvious. A uf
/// build links the CSS it finds in the *client* graph, so the first version of
/// this split — which removed a dropped route's page from that graph and left
/// it at that — silently unstyled the whole site. See the note in
/// `routesModuleSource`.
#[test]
fn the_client_bundle_loses_a_route_that_needs_no_javascript() {
    if !fixture_ready() {
        return;
    }
    let _split = split_lock();
    let root = rsc_split_app_root();

    let output = uf().arg("--cwd").arg(&root).arg("build").output().unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let dist = root.join("dist");
    let scripts = client_scripts(&dist);
    assert!(
        !scripts.is_empty(),
        "the build emitted no client JavaScript at all, so nothing below proves anything"
    );
    let bundle = scripts
        .iter()
        .map(|(name, source)| format!("// {name}\n{source}"))
        .collect::<Vec<_>>()
        .join("\n");

    // 1. The route that needs the browser is still whole.
    assert!(
        bundle.contains("counter-marker-the-browser-needs-this"),
        "the `\"use client\"` counter is missing from the client bundle:\n{}",
        script_names(&scripts)
    );

    // 2. The route that does not is gone, and so is what only it imported.
    assert!(
        !bundle.contains("almanac-marker-only-the-server-reads-this"),
        "`app/_content/almanac.js` is server-only and reached the browser:\n{}",
        script_names(&scripts)
    );
    assert!(
        !bundle.contains("rsc-split-app home"),
        "the static route's page reached the browser:\n{}",
        script_names(&scripts)
    );

    // 3. Both routes are still documents, and the interactive one still says
    //    how it is going to become interactive.
    let home = fs::read_to_string(dist.join("index.html")).expect("the home page is prerendered");
    assert!(
        home.contains("almanac-marker-only-the-server-reads-this"),
        "the static route stopped rendering its server-only content:\n{home}"
    );
    // And it is still styled. A uf build links the stylesheets it finds in the
    // *client* graph, so the first version of this split — which removed the
    // page from that graph outright — took the rules off every page in the
    // site and said nothing. The page is imported for its side effects for
    // exactly this, and the assertion is on the emitted CSS rather than on the
    // import, because the import is the mechanism and this is the promise.
    assert!(
        home.contains("rel=\"stylesheet\" href=\"/assets/"),
        "the static route's document links no stylesheet:\n{home}"
    );
    let css = stylesheets(&dist);
    assert!(
        css.contains(".almanac-note"),
        "the static route's stylesheet was dropped with its JavaScript:\n{css}"
    );
    let counter =
        fs::read_to_string(dist.join("counter/index.html")).expect("the counter is prerendered");
    assert!(
        counter.contains("counter-marker-the-browser-needs-this"),
        "the counter did not render on the server:\n{counter}"
    );
    assert!(
        counter.contains("<script type=\"module\" src=\"/assets/"),
        "the interactive route lost its hydration script:\n{counter}"
    );

    // 4. The summary is the bundler's own count of what it emitted, not a
    //    second implementation of the decision above.
    assert_eq!(
        summary_value(&stdout, "pages in the client bundle"),
        "1 of 2",
        "the summary must say what the bundler emitted:\n{stdout}"
    );

    // 5. And the manifest carries the field the bundler read, so a future
    //    reader of `.uf/build/meta/uf-rsc-manifest.json` sees the same
    //    decision.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".uf/build/meta/uf-rsc-manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["version"], serde_json::json!(2));
    let proximity = |path: &str| {
        manifest["modules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|module| module["path"] == serde_json::json!(path))
            .unwrap_or_else(|| panic!("no {path} in the manifest"))["proximity"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(proximity("app/counter/_uf.page.js"), "reaches-boundary");
    assert_eq!(proximity("app/_uf.page.js"), "isolated");
    assert_eq!(proximity("app/_content/almanac.js"), "isolated");
}

/// A `"use server"` module becomes a reference in the browser and a function
/// on the server, and nothing of it reaches `dist/`.
///
/// The half of ubugeeei-prod/uf#252 that is smaller than a route.
/// `the_client_bundle_loses_a_route_that_needs_no_javascript` above asserts
/// that a route no boundary reaches keeps its page out of the browser; this
/// asserts the other unit — one module, inside a route the browser certainly
/// does need, replaced by an id and a `fetch`.
///
/// Three things have to be true of the emitted JavaScript at once, and the
/// third is the one the issue names:
///
/// 1. the reference is there, so the button has something to call;
/// 2. the action's own body and the module it reached for are not, because a
///    `"use server"` module is code the browser never evaluates;
/// 3. `node:async_hooks` is not there either. `@uniflowed/server` imports it,
///    the action calls `cookies()`, and "a page that calls `cookies()` puts
///    that import in the browser's graph, and nothing stops it" is the
///    sentence in ubugeeei-prod/uf#252 this replaces.
///
/// Marker strings rather than identifiers, for the reason the fixture's README
/// gives: a production bundle renames identifiers and keeps string literals.
#[test]
fn a_server_action_is_a_reference_in_the_browser_and_a_module_on_the_server() {
    if !fixture_ready() {
        return;
    }
    let _split = split_lock();
    let root = rsc_split_app_root();

    let output = uf().arg("--cwd").arg(&root).arg("build").output().unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let dist = root.join("dist");
    let scripts = client_scripts(&dist);
    assert!(
        !scripts.is_empty(),
        "the build emitted no client JavaScript at all, so nothing below proves anything"
    );
    let bundle = scripts
        .iter()
        .map(|(name, source)| format!("// {name}\n{source}"))
        .collect::<Vec<_>>()
        .join("\n");

    // 1. The reference, named by the `module#export` the manifest keys.
    assert!(
        bundle.contains("app/counter/_actions/tally.js#recordCount"),
        "the client bundle has no reference to the action the counter calls:\n{}",
        script_names(&scripts)
    );

    // 2. And nothing of the module the reference stands in for.
    for absent in [
        "tally-marker-only-the-server-runs-this",
        "ledger-marker-the-browser-must-never-see",
    ] {
        assert!(
            !bundle.contains(absent),
            "{absent:?} is server-only and reached the browser:\n{}",
            script_names(&scripts)
        );
    }

    // 3. The import that made this worth doing.
    assert!(
        !bundle.contains("async_hooks"),
        "`@uniflowed/server` reached the browser through the action:\n{}",
        script_names(&scripts)
    );

    // The server bundle is the other half of the same sentence: what the
    // browser does not have, the server does, and it is the same build. Read
    // as a whole rather than one file, because the action module is a lazy
    // `import()` in the generated table and Rollup gives it a chunk of its own
    // — which is what `virtual:uf/actions` asked for, so that a project's
    // actions are not imported by every request.
    let server = server_bundle(&root.join(".uf/build/server"));
    assert!(
        server.contains("tally-marker-only-the-server-runs-this"),
        "the action is missing from the server bundle, so nothing could answer a call"
    );

    // The id in the browser is the id the server dials into. Read out of the
    // manifest rather than recomputed: it is an HMAC over a per-build secret,
    // so there is nowhere else it could come from — which is the point of it.
    // From `.uf/build/meta/`, not from `dist/`. Everything in the output
    // directory is served, and ubugeeei-prod/uf#339 moved the build's own
    // metadata out of it for exactly that reason — a manifest of every server
    // action, with the ids the server dials, is the last file to hand a
    // browser.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".uf/build/meta/uf-rsc-manifest.json")).unwrap(),
    )
    .unwrap();
    let actions = manifest["serverActions"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "manifest: {manifest}");
    assert_eq!(actions[0]["module"], "app/counter/_actions/tally.js");
    assert_eq!(actions[0]["export"], "recordCount");
    let id = actions[0]["id"].as_str().unwrap();
    assert_eq!(
        id.len(),
        64,
        "an action id is 64 hex characters, got {id:?}"
    );
    assert!(
        id.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "an action id is lowercase hexadecimal, got {id:?}"
    );
    assert!(
        bundle.contains(id),
        "the browser holds a different id from the one the manifest published:\n{}",
        script_names(&scripts)
    );
    assert!(
        server.contains(id),
        "the server's table does not carry the id the browser was given"
    );

    // And the summary counts it, so a reader sees that the build produced an
    // endpoint rather than only an analysis.
    assert_eq!(
        summary_value(&stdout, "server actions"),
        "1",
        "the summary must count the callable action:\n{stdout}"
    );
}

/// Every adapter serves the same server action, and refuses the same calls.
///
/// ubugeeei-prod/uf#447 established that the four deploy targets answer
/// identically, and `handler.js` being byte-for-byte the same file in all of
/// them is why. A server action is answered by that file, so it is answered by
/// all four or by none — and this is what says so, rather than the reasoning.
///
/// It is a second four-adapter test rather than five more questions in the
/// first because it needs a different fixture: `served-app` has no
/// `"use client"` module anywhere, so it can declare no callable action, and
/// giving it one would change what the split does to it and what three other
/// tests measure.
#[test]
fn every_adapter_answers_the_same_server_action_call() {
    if !fixture_ready() {
        return;
    }
    let _split = split_lock();
    let root = rsc_split_app_root();

    let mut answers: Option<(String, String)> = None;
    for adapter in ["node", "edge", "serverless", "container"] {
        let (_, empty) = deploy_and_copy(&root, adapter);
        // After the build, and once per adapter. The id is an HMAC over a
        // per-build secret, so every one of these four builds mints its own —
        // which is the property that makes an id from a previous build useless
        // to somebody who kept one, and the reason this cannot be hoisted out
        // of the loop.
        let questions =
            SERVER_ACTION_QUESTIONS.replace("__UF_ACTION_ID__", &deployed_action_id(&root));
        let said = ask_the_artefact(empty.path(), adapter, &questions);

        // The action ran, inside the request the host began, and answered with
        // what the server made of the argument — `tallyFor(4)` is `9`, and the
        // marker is the string the browser's copy of the bundle does not have.
        assert!(
            said.contains("action 200")
                && said.contains("\"total\":9")
                && said.contains("tally-marker-only-the-server-runs-this"),
            "the `{adapter}` artefact did not run the action:\n{said}"
        );
        // `cookies()` inside an action answers about the request that carried
        // the call, which is the whole reason an action is not a static import.
        assert!(
            said.contains("\"visitor\":\"ada\""),
            "the `{adapter}` artefact did not run the action inside its request:\n{said}"
        );
        // And the three refusals, each with its own status so that a reader of
        // this file can see which guard is which.
        for expected in [
            "action-cross-origin 403",
            "action-unknown-id 404",
            "action-form-content-type 415",
        ] {
            assert!(
                said.contains(expected),
                "the `{adapter}` artefact answered {expected:?} differently:\n{said}"
            );
        }
        // Nothing about the build leaks out of a refusal.
        assert!(
            !said.contains("tally-marker-only-the-server-runs-this/ledger")
                || said
                    .matches("tally-marker-only-the-server-runs-this")
                    .count()
                    == 2,
            "a refusal carried something about the build:\n{said}"
        );

        match &answers {
            None => answers = Some((adapter.to_owned(), said)),
            Some((first, expected)) => similar_asserts::assert_eq!(
                expected,
                &said,
                "the `{}` and `{}` artefacts answered differently",
                first,
                adapter
            ),
        }
    }
}

/// One output directory, one test building it — with the lock the other two
/// fixtures have, because the next test written against this one will race it.
static SPLIT: Mutex<()> = Mutex::new(());

fn split_lock() -> std::sync::MutexGuard<'static, ()> {
    SPLIT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Every `.js` file the client build emitted, as `(name, source)`.
///
/// `.js` and not `.js.map`: a source map carries the *source* of every module
/// in the chunk, so a bundle that dropped a module still has its text in the
/// map beside it. What ships to a browser as code is the question.
fn client_scripts(dist: &Path) -> Vec<(String, String)> {
    let assets = dist.join("assets");
    let mut scripts = Vec::new();
    let Ok(entries) = fs::read_dir(&assets) else {
        return scripts;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".js") {
            continue;
        }
        if let Ok(source) = fs::read_to_string(entry.path()) {
            scripts.push((name, source));
        }
    }
    scripts.sort();
    scripts
}

/// Every `.js` file the server build emitted, concatenated.
///
/// `.js` and not `.js.map`, for the reason [`client_scripts`] gives, and the
/// whole directory rather than `server.js`: the generated action and route
/// tables load their modules with `import()`, so an action is a chunk beside
/// the entry rather than inside it.
fn server_bundle(directory: &Path) -> String {
    let mut sources = Vec::new();
    for root in [directory.to_path_buf(), directory.join("assets")] {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().ends_with(".js") {
                continue;
            }
            if let Ok(source) = fs::read_to_string(entry.path()) {
                sources.push(source);
            }
        }
    }
    sources.sort();
    sources.join("\n")
}

/// Every stylesheet the client build emitted, concatenated.
fn stylesheets(dist: &Path) -> String {
    let assets = dist.join("assets");
    let mut sheets = Vec::new();
    let Ok(entries) = fs::read_dir(&assets) else {
        return String::new();
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().ends_with(".css") {
            continue;
        }
        if let Ok(source) = fs::read_to_string(entry.path()) {
            sheets.push(source);
        }
    }
    sheets.sort();
    sheets.join("\n")
}

/// The emitted file names, for a failure that has to say what it looked at.
fn script_names(scripts: &[(String, String)]) -> String {
    scripts
        .iter()
        .map(|(name, source)| format!("  {name} ({} bytes)", source.len()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `crates/uf_cli/tests/fixtures/paper-builder`.
fn paper_builder_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/paper-builder")
}

/// A page with parameters and no `generateStaticParams`, which is the one
/// route shape a prerender cannot produce a file for.
const UNPRERENDERABLE_PAGE: (&str, &str) = (
    "app/posts/[slug]/_uf.page.js",
    "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport default component Post(params: { readonly slug: string }) {\n  return <h1>{params.slug}</h1>;\n}\n",
);

/// A `uf.config.js` with `body` merged into `defineConfig`.
fn config_with(body: &str) -> String {
    format!(
        "// @flow\nimport {{ defineConfig }} from \"@uniflowed/config\";\n\nexport default defineConfig({{\n{body}}});\n"
    )
}

/// `uf build` in `root`, as `(succeeded, stdout + stderr)`.
fn build_output(root: &Path) -> (bool, String) {
    let output = uf().arg("--cwd").arg(root).arg("build").output().unwrap();
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), said)
}

/// `rendering.modes: ["ssg"]` is a project saying it deploys to a static host.
///
/// Before ubugeeei-prod/uf#336 the list was read by nothing: this project
/// built, `dist/` had no document for `/posts/:slug`, and the first anyone
/// heard of it was a 404 from the CDN. The build now refuses, names the route
/// and names both ways out — which is the guide's "reject unsupported
/// configurations clearly rather than silently changing semantics", applied to
/// the config that was silently changed.
#[test]
fn a_route_that_needs_a_server_fails_a_build_that_allows_only_ssg() {
    if !fixture_ready() {
        return;
    }
    let mut files = minimal_app();
    files.push(UNPRERENDERABLE_PAGE);
    let project = Project::new(&files);
    project.write(
        "uf.config.js",
        &config_with("  app: { rendering: { modes: [\"ssg\"] } },\n"),
    );

    let (succeeded, said) = build_output(project.path());
    assert!(!succeeded, "the build should have refused:\n{said}");
    assert!(
        said.contains("/posts/:slug"),
        "the route is not named:\n{said}"
    );
    assert!(
        said.contains("generateStaticParams"),
        "the first way out is not named:\n{said}"
    );
    assert!(
        said.contains("\"ssr\"") && said.contains("app.rendering.modes"),
        "the second way out is not named:\n{said}"
    );
    // No half-built output: the refusal happens before a document is written,
    // so a project that has just narrowed the list does not end up with a
    // `dist/` that looks complete and is not.
    assert!(
        !project.path().join("dist/index.html").exists(),
        "a refused build wrote a document anyway"
    );
}

/// The other half of #336's "Done": the same project, both modes allowed.
#[test]
fn allowing_ssr_beside_ssg_builds_the_same_project() {
    if !fixture_ready() {
        return;
    }
    let mut files = minimal_app();
    files.push(UNPRERENDERABLE_PAGE);
    let project = Project::new(&files);
    project.write(
        "uf.config.js",
        &config_with("  app: { rendering: { modes: [\"ssg\", \"ssr\"] } },\n"),
    );

    let (succeeded, said) = build_output(project.path());
    assert!(succeeded, "{said}");
    // Prerendered where it could be, and the rest reported rather than
    // silently missing — the report that did not exist before #250.
    assert!(project.path().join("dist/index.html").is_file());
    assert!(
        said.contains("answered by a server") && said.contains("/posts/:slug"),
        "the build did not say which routes need one:\n{said}"
    );
}

/// `rendering.modes: ["ssr"]` used to mean SSG, because SSG was all there was.
#[test]
fn allowing_only_ssr_prerenders_nothing() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&minimal_app());
    project.write(
        "uf.config.js",
        &config_with("  app: { rendering: { modes: [\"ssr\"] } },\n"),
    );

    let (succeeded, said) = build_output(project.path());
    assert!(succeeded, "{said}");
    assert!(
        !project.path().join("dist/index.html").exists(),
        "a build that allows no `ssg` prerendered a document anyway:\n{said}"
    );
    // And there is still something to answer with: the whole point of the
    // setting is that the server renders every request.
    assert!(
        project.path().join(".uf/build/server/server.js").is_file(),
        "no server bundle:\n{said}"
    );
}

/// `build.staticBuild` is documented as "prerender everything and emit no
/// server bundle", and was read by nothing. See ubugeeei-prod/uf#385.
#[test]
fn a_static_build_emits_no_server_bundle() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&minimal_app());
    project.write(
        "uf.config.js",
        &config_with("  build: { staticBuild: true },\n"),
    );

    let (succeeded, said) = build_output(project.path());
    assert!(succeeded, "{said}");
    assert!(
        project.path().join("dist/index.html").is_file(),
        "the documents are the whole output, and there are none:\n{said}"
    );
    assert!(
        !project.path().join(".uf/build/server/server.js").exists(),
        "the build emitted the server bundle it said it would not:\n{said}"
    );

    // And `uf start` refuses by name, rather than failing later on a missing
    // file. It is the one command that reads the bundle this build removed,
    // and for a project deploying documents there is no deployment that runs
    // uf at all: a `uf start` that quietly served the files would be uf
    // answering requests the real host answers.
    let start = uf()
        .arg("--cwd")
        .arg(project.path())
        .arg("start")
        .output()
        .unwrap();
    assert!(!start.status.success());
    let said = String::from_utf8_lossy(&start.stderr).to_string();
    assert!(said.contains("staticBuild"), "{said}");
    assert!(said.contains("static host"), "{said}");

    // `uf explain start` says the same thing without running anything, which
    // is what red line 7 asks of every stage of every command.
    let explained = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["explain", "start"])
        .output()
        .unwrap();
    assert!(explained.status.success());
    let plan = String::from_utf8(explained.stdout).unwrap();
    assert!(plan.contains("refuses"), "{plan}");
}

/// A middleware under a build with no server is two declarations that cannot
/// both be true, and #385 says to refuse rather than warn.
#[test]
fn a_static_build_refuses_the_middleware_it_could_never_run() {
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
        "// @flow\n\nexport default function middleware(request: Request): Response | void {\n  if (!request.headers.has(\"cookie\")) {\n    return Response.redirect(new URL(\"/\", request.url), 302);\n  }\n}\n",
    ));
    let project = Project::new(&files);
    project.write(
        "uf.config.js",
        &config_with("  build: { staticBuild: true },\n"),
    );

    let (succeeded, said) = build_output(project.path());
    assert!(!succeeded, "the build should have refused:\n{said}");
    assert!(
        said.contains("/dashboard"),
        "the guard is not named:\n{said}"
    );
    assert!(
        said.contains("once per request"),
        "the refusal does not say why:\n{said}"
    );
}

/// The fourth thing that needs a process, and the one that is not a route.
///
/// A `"use server"` export the browser can reach is an endpoint: the client
/// bundle carries a `createServerReference` that `fetch`es it, so the button
/// is wired whether or not anything answers. A build that emits no server
/// emits no answer, and nothing between that build and the first click said
/// so.
///
/// The project is `rsc-split-app` rather than a page written for this test.
/// That fixture is what the server-action split is already measured against
/// and `app/counter/_actions/tally.js` is exactly the module this refusal is
/// about; it is copied rather than built in place because the assertion is
/// about a `uf.config.js` the fixture does not have and should not grow.
///
/// The other direction — a `staticBuild` project with no actions builds, and
/// builds without a server bundle — is
/// `a_static_build_emits_no_server_bundle` above.
#[test]
fn a_static_build_refuses_the_server_actions_it_could_never_answer() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&[(
        "app.js",
        "// @flow\nimport { routerView } from \"@uniflowed/router\";\n\nexport default routerView(\"./app\");\n",
    )]);
    copy_tree(
        &rsc_split_app_root().join("app"),
        &project.path().join("app"),
    );
    project.write(
        "uf.config.js",
        &config_with("  build: { staticBuild: true },\n"),
    );

    let (succeeded, said) = build_output(project.path());
    assert!(!succeeded, "the build should have refused:\n{said}");
    assert!(
        said.contains("app/counter/_actions/tally.js"),
        "the module is not named:\n{said}"
    );
    assert!(
        said.contains("staticBuild"),
        "the declaration that caused it is not quoted:\n{said}"
    );
    assert!(
        said.contains("--adapter") && said.contains("--compile"),
        "the two builds that do answer an action are not named:\n{said}"
    );
    // Refused before the bundle, so the reader does not pay for a build that
    // was never going to be one — and there is no half-written `dist/` to
    // mistake for a finished deployment.
    assert!(
        !project.path().join("dist/index.html").exists(),
        "a refused build wrote a document anyway"
    );
}

/// The third rendering decision, and the one that had no name.
///
/// `generateStaticParams` says "prerender these"; nothing said "never
/// prerender this", so a route with no parameters whose content depends on the
/// request could not be kept out of `dist/`. See ubugeeei-prod/uf#336.
#[test]
fn a_page_that_forces_dynamic_is_left_to_the_server() {
    if !fixture_ready() {
        return;
    }
    let mut files = minimal_app();
    files.push((
        "app/now/_uf.page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport const dynamic = \"force-dynamic\";\n\nexport component Page() {\n  return <main>now</main>;\n}\n",
    ));
    let project = Project::new(&files);

    let (succeeded, said) = build_output(project.path());
    assert!(succeeded, "{said}");
    assert!(project.path().join("dist/index.html").is_file());
    assert!(
        !project.path().join("dist/now/index.html").exists(),
        "a page that said not to prerender it was prerendered:\n{said}"
    );
    assert!(said.contains("/now"), "the route is not reported:\n{said}");
}

/// A `dynamic` uf does not implement is refused rather than ignored.
#[test]
fn a_dynamic_value_uf_does_not_implement_is_named() {
    if !fixture_ready() {
        return;
    }
    let mut files = minimal_app();
    files.push((
        "app/now/_uf.page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\n\nexport const dynamic = \"force-static\";\n\nexport component Page() {\n  return <main>now</main>;\n}\n",
    ));
    let project = Project::new(&files);

    let (succeeded, said) = build_output(project.path());
    assert!(!succeeded, "the build should have refused:\n{said}");
    assert!(said.contains("force-static"), "{said}");
    assert!(said.contains("force-dynamic"), "{said}");
}

/// The seam is a seam: a builder that is not `@uniflowed/vite` runs.
///
/// `paper-builder` has no bundler in it — it walks the router root and writes
/// a document per route — so nothing it satisfies can be Vite-shaped. That is
/// the whole assertion, and it is ubugeeei-prod/uf#549's: until this, Vite was
/// not one implementation of a contract, it was reached by name from four
/// commands.
#[test]
fn a_second_builder_is_resolved_named_and_driven() {
    if !fixture_ready() {
        return;
    }
    let project = Project::new(&minimal_app());
    copy_tree(
        &paper_builder_root(),
        &project.path().join("tools/paper-builder"),
    );
    project.write(
        "uf.config.js",
        &config_with("  builder: { module: \"./tools/paper-builder\" },\n"),
    );

    // Named before it runs, which is red line 7: a person should be able to
    // ask which provider a command will use without running it.
    let explained = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["explain", "build"])
        .output()
        .unwrap();
    assert!(explained.status.success());
    let plan = String::from_utf8(explained.stdout).unwrap();
    assert!(
        plan.contains("./tools/paper-builder 0.1.0"),
        "`uf explain build` did not name the builder and its version:\n{plan}"
    );

    let (succeeded, said) = build_output(project.path());
    assert!(succeeded, "{said}");
    let index = fs::read_to_string(project.path().join("dist/index.html")).unwrap();
    assert!(
        index.contains("data-paper-builder=\"/\""),
        "the second builder did not write the document:\n{index}"
    );
}

/// A builder the project named and did not install is a sentence, not a stack.
#[test]
fn a_builder_that_is_not_there_is_refused_by_name() {
    let project = Project::new(&minimal_app());
    project.write(
        "uf.config.js",
        &config_with("  builder: { module: \"@someone/rolldown-builder\" },\n"),
    );

    let (succeeded, said) = build_output(project.path());
    assert!(!succeeded, "{said}");
    assert!(said.contains("@someone/rolldown-builder"), "{said}");
    assert!(said.contains("uf install"), "{said}");
}

/// The served fixture's build says which of its routes need a server.
///
/// The fixture exists because the docs site is static: it has a route handler
/// and two routes with parameters and no `generateStaticParams`, which are
/// exactly the things `uf build` used to leave out of `dist/` without saying
/// so. Asserted against the build manifest rather than a running server, so it
/// needs no socket.
#[test]
fn the_served_fixture_records_every_route_a_server_has_to_answer() {
    if !fixture_ready() {
        return;
    }
    let _served = served_lock();
    let root = served_app_root();

    let (succeeded, said) = build_output(&root);
    assert!(succeeded, "{said}");

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".uf/build/meta/uf-build-manifest.json")).unwrap(),
    )
    .unwrap();
    let rendering = &manifest["rendering"];
    assert_eq!(rendering["prerender"], serde_json::json!("possible"));
    assert_eq!(rendering["server"], serde_json::json!(true));
    let per_request: Vec<&str> = rendering["perRequest"]
        .as_array()
        .expect("the manifest records what the build left to a server")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    for expected in ["/posts/:slug", "/slow/:id", "/api/health"] {
        assert!(
            per_request.contains(&expected),
            "{expected} is missing from {per_request:?}"
        );
    }
    // And it is reported, not only recorded.
    assert!(
        said.contains("answered by a server"),
        "the build did not report them:\n{said}"
    );
}
