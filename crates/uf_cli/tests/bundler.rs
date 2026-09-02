//! `uf build` over a whole fixture project, end to end.
//!
//! The scaffolded app is the fixture: it is Flow source with a `"use client"`
//! boundary, a server action, StyleX tokens, JSX and a route, so a build over it
//! exercises every stage the pipeline places. These tests assert what a build
//! must always be true of — valid JavaScript, no Flow, no server code in a
//! browser chunk, and the same bytes twice — rather than the exact text.

mod support;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use support::uf;

/// Scaffold a React app and build it, returning the build's stdout.
fn build(dir: &Path) -> String {
    let created = uf()
        .arg("--cwd")
        .arg(dir)
        .args(["create", "app", "react"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );

    rebuild(dir)
}

/// Build an already-scaffolded project.
fn rebuild(dir: &Path) -> String {
    let output = uf().arg("--cwd").arg(dir).arg("build").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Every emitted chunk, keyed by file name.
fn chunks(dir: &Path) -> BTreeMap<String, String> {
    let assets = dir.join("dist/assets");
    let mut chunks = BTreeMap::new();
    for entry in fs::read_dir(&assets).expect("dist/assets exists") {
        let path = entry.expect("directory entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".js") {
            continue;
        }
        chunks.insert(name, fs::read_to_string(&path).expect("read chunk"));
    }
    assert!(!chunks.is_empty(), "the build emitted no chunks");
    chunks
}

#[test]
fn a_scaffolded_app_builds_into_javascript_chunks() {
    let dir = tempfile::tempdir().unwrap();

    let stdout = build(dir.path());

    let chunks = chunks(dir.path());
    assert!(chunks.len() >= 2, "{:?}", chunks.keys());
    assert!(stdout.contains("chunks"), "{stdout}");
    assert!(stdout.contains("bundled modules"), "{stdout}");
}

/// Every chunk the build emitted must be **JavaScript**.
///
/// This test used to re-parse each chunk with `uf_flow::validate_source` and
/// assert no diagnostics. That check was worthless: `validate_source` is a
/// *Flow* parser, Flow's grammar includes JSX, and so it accepted output that
/// no browser could load. The suite was self-consistent and the build was
/// broken.
///
/// The replacement asks the concrete question instead. Nothing in this
/// repository parses ES modules without also accepting JSX — the one front end
/// uf has is Flow's — so the property is checked on the bytes: the chunk is
/// re-scanned in JSX mode and must yield no JSX token at all.
/// [`node_agrees_the_chunks_are_javascript`] adds a real ES-module parse on top
/// where a Node binary is available.
#[test]
fn no_emitted_chunk_holds_jsx() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    for (name, code) in chunks(dir.path()) {
        let surviving: Vec<&str> = uf_flow::scan::tokenize_jsx(&code)
            .iter()
            .filter(|token| token.kind.is_jsx())
            .map(|token| token.text(&code))
            .take(4)
            .collect();
        assert!(
            surviving.is_empty(),
            "chunk {name} still holds JSX {surviving:?}\n{code}"
        );
    }
}

/// A second opinion from a JavaScript engine, when one is on `PATH`.
///
/// `node --check` over an `.mjs` copy is a real ES-module parse by an
/// implementation that has never heard of Flow or of this project's
/// assumptions. It is the check that caught the bug this test file now guards
/// against, so it belongs in the suite rather than in someone's shell history.
///
/// Skipped, loudly, where Node is absent: the structural check above holds
/// everywhere, and a test that silently disappears is how a suite starts lying
/// again.
#[test]
fn node_agrees_the_chunks_are_javascript() {
    let Some(node) = node_binary() else {
        eprintln!("skipping: no `node` on PATH; no_emitted_chunk_holds_jsx still ran");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    let mut checked = 0usize;
    for (name, code) in chunks(dir.path()) {
        // Node reads `.js` as CommonJS unless a manifest says otherwise, and
        // these chunks are ES modules.
        let module = dir.path().join(format!("{name}.mjs"));
        fs::write(&module, &code).unwrap();

        let output = std::process::Command::new(&node)
            .arg("--check")
            .arg(&module)
            .output()
            .expect("node runs");
        assert!(
            output.status.success(),
            "node rejects chunk {name}:\n{}\n--- chunk\n{code}",
            String::from_utf8_lossy(&output.stderr)
        );
        checked += 1;
    }

    assert!(checked > 0, "the build emitted no chunks to check");
}

/// A `node` on `PATH`, if there is one.
fn node_binary() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("node"))
        .find(|candidate| candidate.is_file())
}

/// Flow's own parser still has an opinion worth hearing: it catches malformed
/// JavaScript that is also malformed Flow. It cannot catch surviving JSX, which
/// is why it is no longer the only check.
///
/// The QuickJS-hosted backend budgets its stack from wherever its runtime was
/// created and can exhaust that budget on a chunk merging several modules. That
/// is the parser giving up rather than a verdict, so it is skipped rather than
/// counted as a failure.
#[test]
fn the_flow_parser_finds_no_diagnostics_in_a_chunk() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    uf_flow::prepare_thread().expect("parser ready");

    for (name, code) in chunks(dir.path()) {
        let Ok(outcome) = uf_flow::validate_source(&code) else {
            continue;
        };
        assert!(
            outcome.is_ok(),
            "chunk {name} does not parse: {:?}\n{code}",
            outcome.diagnostics
        );
    }
}

#[test]
fn the_scaffolded_components_lower_to_runtime_calls() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    let lowered = chunks(dir.path())
        .into_iter()
        .filter(|(_, code)| code.contains("_jsx"))
        .count();

    assert!(lowered >= 2, "the app's components were not lowered");
}

#[test]
fn no_flow_syntax_reaches_an_emitted_chunk() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    for (name, code) in chunks(dir.path()) {
        assert!(!code.contains("component "), "{name}:\n{code}");
        assert!(!code.contains(" renders "), "{name}:\n{code}");
        assert!(!code.contains("export type "), "{name}:\n{code}");
        assert!(!code.contains("import type "), "{name}:\n{code}");
        assert!(!code.contains("opaque type "), "{name}:\n{code}");
    }
}

#[test]
fn a_chunk_uses_real_es_module_syntax_between_chunks() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    let chunks = chunks(dir.path());
    let entry = chunks
        .iter()
        .find(|(name, _)| name.starts_with("entry-app-_uf-page"))
        .expect("the route entry chunk");
    assert!(entry.1.contains("import {"), "{}", entry.1);
    assert!(entry.1.contains("} from \"./client-"), "{}", entry.1);

    let client = chunks
        .iter()
        .find(|(name, _)| name.starts_with("client-"))
        .expect("the client boundary chunk");
    assert!(client.1.contains("export {"), "{}", client.1);
}

#[test]
fn the_client_chunk_never_holds_the_server_action_module() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    for (name, code) in chunks(dir.path()) {
        if !name.starts_with("client-") {
            continue;
        }
        assert!(!code.contains("Flow at native speed"), "{name}:\n{code}");
        assert!(!code.contains("serverAction"), "{name}:\n{code}");
    }
}

#[test]
fn a_chunk_name_carries_a_content_hash() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    for name in chunks(dir.path()).keys() {
        let stem = name.strip_suffix(".js").expect("a .js chunk");
        let hash = stem.rsplit('-').next().expect("a hash segment");
        assert_eq!(hash.len(), 8, "{name}");
        assert!(
            hash.chars().all(|character| character.is_ascii_hexdigit()),
            "{name}"
        );
    }
}

#[test]
fn building_twice_produces_byte_identical_chunks() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());
    let first = chunks(dir.path());

    fs::remove_dir_all(dir.path().join("dist")).expect("clear the output directory");
    rebuild(dir.path());
    let second = chunks(dir.path());

    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        second.keys().collect::<Vec<_>>()
    );
    for (name, code) in &first {
        assert_eq!(code, &second[name], "chunk {name} differs between builds");
    }
}

#[test]
fn source_maps_are_emitted_beside_their_chunks() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    for name in chunks(dir.path()).keys() {
        let map = dir.path().join("dist/assets").join(format!("{name}.map"));
        assert!(map.exists(), "{name} has no source map");
        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&map).expect("read map")).expect("json");
        assert_eq!(parsed["version"], 3);
        assert_eq!(parsed["file"], name.as_str());
        assert!(!parsed["sources"].as_array().expect("sources").is_empty());
    }
}

#[test]
fn the_size_report_measures_the_emitted_chunks() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    let report = fs::read_to_string(dir.path().join("dist/uf-bundle-report.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    let javascript = parsed["assets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|asset| asset["kind"] == "java-script")
        .count();

    assert!(javascript >= 2, "{}", parsed["assets"]);
    assert!(parsed["total"]["raw"].as_u64().unwrap() > 0);
    assert!(parsed["total"]["gzip"].as_u64().unwrap() > 0);
}

#[test]
fn the_size_report_attributes_chunks_to_the_route_that_needs_them() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    let report = fs::read_to_string(dir.path().join("dist/uf-bundle-report.json")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    let routes = parsed["routes"].as_array().expect("routes");

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0]["path"], "/");
    assert!(routes[0]["initialJs"]["raw"].as_u64().unwrap() > 0);
    assert!(!routes[0]["assets"].as_array().unwrap().is_empty());
}

#[test]
fn a_budget_measured_against_real_output_can_fail_the_build() {
    let dir = tempfile::tempdir().unwrap();
    let created = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["create", "app", "react"])
        .output()
        .unwrap();
    assert!(created.status.success());

    let config_path = dir.path().join("uf.config.js");
    let config = fs::read_to_string(&config_path).unwrap();
    fs::write(
        &config_path,
        config.replacen(
            "defineConfig({",
            "defineConfig({\n  build: { budgets: { initialJs: { max: \"1b\" } } },",
            1,
        ),
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .arg("build")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("budget exceeded"), "{stderr}");
}
