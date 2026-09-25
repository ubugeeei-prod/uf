//! The native route table and the web build's router choose the same files.
//!
//! There are two scanners over one router root, and there have to be: the web
//! build scans in `@uniflowed/vite` (`packages/vite/internal/routes.js`) while
//! Vite runs, and a native route table is written by `uf` for Metro, which runs
//! no uf JavaScript at all. Two implementations of one precedence rule are a
//! drift risk, and this test is what makes them not one: for every target it
//! runs both over the same tree and requires the same routes, the same page
//! files, the same layouts in the same order and the same boundaries.
//!
//! It runs the JavaScript scanner on Node, which that module needs and nothing
//! else — it imports only `node:fs` and `node:path`.

use std::fs;
use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use uf_config::UniflowedConfig;
use uf_router::RouteTarget;
use uf_router::native::discover_route_table;

/// The same canonical lines the Rust side prints, from `scanRoutes`.
///
/// The boundaries the web table synthesises at `/` when a project declares
/// none carry no module, and are left out: the native table does not have
/// them, on purpose.
const JS_SCANNER: &str = r#"
import path from "node:path";
import { scanRoutes } from "__ROUTES_JS__";

const root = process.env.UF_SCAN_ROOT;
const table = scanRoutes(path.join(root, "app"), { target: process.env.UF_SCAN_TARGET });
const rel = (file) => path.relative(root, file).split(path.sep).join("/");
const layouts = (files) => `[${files.map(rel).join(",")}]`;
const lines = [];
for (const route of table.routes) {
  lines.push(`route ${route.path} ${rel(route.page)} ${layouts(route.layouts)}`);
}
for (const boundary of table.notFound) {
  if (boundary.page != null) {
    lines.push(`not-found ${boundary.path} ${rel(boundary.page)} ${layouts(boundary.layouts)}`);
  }
}
for (const boundary of table.errors) {
  if (boundary.module != null) {
    lines.push(`error ${boundary.path} ${rel(boundary.module)} ${layouts(boundary.layouts)}`);
  }
}
process.stdout.write(lines.sort().join("\n"));
"#;

/// Whether `node` can run here.
///
/// A skipped comparison reads exactly like a passing one, so this asserts
/// unless `UF_ALLOW_FIXTURE_SKIP` says the machine genuinely has no Node. CI
/// sets nothing, and so cannot skip.
fn node_ready() -> bool {
    if Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return true;
    }
    assert!(
        std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
        "this test needs `node` on PATH and there is none, so it would prove nothing"
    );
    eprintln!("skipping: `node` is not on PATH");
    false
}

fn router_tree() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    for file in [
        "app/$layout.js",
        "app/$layout.native.js",
        "app/$page.js",
        "app/$page.web.js",
        "app/$not-found.js",
        "app/$not-found.native.js",
        "app/(tabs)/$layout.js",
        "app/(tabs)/$layout.ios.js",
        "app/(tabs)/feed/$page.js",
        "app/(tabs)/feed/$page.ios.js",
        "app/(tabs)/profile/$page.native.js",
        "app/users/$error.js",
        "app/users/$error.android.js",
        "app/users/[id]/$page.native.js",
        "app/users/[id]/$page.android.js",
        "app/users/[id]/settings/$page.jsx",
        "app/docs/[...slug]/$page.js",
        "app/docs/[...slug]/$not-found.web.js",
        "app/guide/[[...slug]]/$page.js",
        "app/guide/[[...slug]]/$page.native.jsx",
        "app/web-only/$page.web.js",
        "app/android-only/$page.android.js",
    ] {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "// @flow\n").unwrap();
    }
    (dir, root)
}

fn rust_lines(root: &Utf8Path, target: RouteTarget) -> Vec<String> {
    let table = discover_route_table(root, &UniflowedConfig::default(), target).unwrap();
    let rel = |file: &Utf8Path| {
        file.strip_prefix(root)
            .unwrap()
            .components()
            .map(|component| component.as_str())
            .collect::<Vec<_>>()
            .join("/")
    };
    let layouts = |files: &[Utf8PathBuf]| {
        files
            .iter()
            .map(|file| rel(file))
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut lines = Vec::new();
    for route in &table.routes {
        lines.push(format!(
            "route {} {} [{}]",
            route.route.path,
            rel(&route.route.page),
            layouts(&route.layouts)
        ));
    }
    for boundary in &table.not_found {
        lines.push(format!(
            "not-found {} {} [{}]",
            boundary.path,
            rel(&boundary.file),
            layouts(&boundary.layouts)
        ));
    }
    for boundary in &table.errors {
        lines.push(format!(
            "error {} {} [{}]",
            boundary.path,
            rel(&boundary.file),
            layouts(&boundary.layouts)
        ));
    }
    lines.sort();
    lines
}

fn js_lines(root: &Utf8Path, target: RouteTarget) -> Vec<String> {
    let routes_js = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/vite/internal/routes.js")
        .canonicalize_utf8()
        .unwrap();
    let script = JS_SCANNER.replace("__ROUTES_JS__", &format!("file://{routes_js}"));
    let output = Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .env("UF_SCAN_ROOT", root.as_str())
        .env("UF_SCAN_TARGET", target.as_str())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the build router's scanner failed for {}: {}",
        target.as_str(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_target_chooses_the_same_routes_layouts_and_boundaries_in_both_scanners() {
    if !node_ready() {
        return;
    }
    let (_dir, root) = router_tree();

    for target in [
        RouteTarget::Web,
        RouteTarget::Native,
        RouteTarget::Ios,
        RouteTarget::Android,
    ] {
        let rust = rust_lines(&root, target);
        assert!(
            !rust.is_empty(),
            "the fixture has routes for {}",
            target.as_str()
        );
        similar_asserts::assert_eq!(
            js: js_lines(&root, target),
            rust: rust,
            "{} target",
            target.as_str()
        );
    }
}
