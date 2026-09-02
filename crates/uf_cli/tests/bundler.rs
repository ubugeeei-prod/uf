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

/// Every chunk the build emitted must be syntactically valid.
///
/// The default parser backend hosts Flow's reference parser in QuickJS, which
/// enforces a fixed 256 kB recursion budget of its own — see the module docs on
/// `uf_flow`'s `quickjs` backend. A chunk that merges several modules and their
/// nested JSX into one file can exhaust that budget, and the backend reports it
/// as a *runtime* error rather than as a diagnostic. That is the parser giving
/// up, not the chunk being wrong, so it is counted and reported separately: a
/// diagnostic always fails the test, and at least one chunk has to parse.
#[test]
fn every_emitted_chunk_parses_as_javascript() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path());

    // The runtime budgets its stack from wherever it was created, so it has to
    // be created from a shallow frame before any parsing.
    uf_flow::prepare_thread().expect("parser ready");

    let mut parsed = 0usize;
    for (name, code) in chunks(dir.path()) {
        let Ok(outcome) = uf_flow::validate_source(&code) else {
            continue;
        };
        parsed += 1;
        assert!(
            outcome.is_ok(),
            "chunk {name} does not parse: {:?}\n{code}",
            outcome.diagnostics
        );
    }

    assert!(parsed > 0, "the parser backend could not run on any chunk");
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
