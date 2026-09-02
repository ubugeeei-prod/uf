use super::*;
use crate::action::BuildId;
use crate::graph::{EntryKind, RscGraphBuilder};

fn fixture() -> (RscGraph, ServerActionRegistry) {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/_uf.page.js",
        "// @flow\nimport Counter from \"./client/Counter.js\";\nimport { refresh } from \"../server/actions.js\";\nexport default function Page() {}\n",
    );
    builder.add_source(
        "app/client/Counter.js",
        "\"use client\";\n// @flow\nimport { useState } from \"@uniflowed/react\";\nexport default function Counter() {\n const [n] = useState(0);\n return n;\n}\n",
    );
    builder.add_source(
        "server/actions.js",
        "\"use server\";\n// @flow\nexport async function refresh() {}\n",
    );
    builder.add_entry("app/_uf.page.js", EntryKind::Server);
    let graph = builder.build();
    let build_id = BuildId::new("fixture-build-id").expect("valid build id");
    let registry = ServerActionRegistry::from_graph(&graph, &build_id);
    (graph, registry)
}

#[test]
fn the_manifest_matches_its_snapshot() {
    let (graph, registry) = fixture();
    let manifest = RscManifest::new(&graph, &registry);
    similar_asserts::assert_eq!(
        manifest.to_json().unwrap(),
        include_str!("../../tests/fixtures/uf-rsc-manifest.json")
    );
}

#[test]
fn the_manifest_is_byte_identical_across_builds() {
    let (first_graph, first_registry) = fixture();
    let (second_graph, second_registry) = fixture();
    assert_eq!(
        RscManifest::new(&first_graph, &first_registry)
            .to_json()
            .unwrap(),
        RscManifest::new(&second_graph, &second_registry)
            .to_json()
            .unwrap()
    );
}

#[test]
fn the_manifest_never_contains_the_build_id() {
    let (graph, registry) = fixture();
    let json = RscManifest::new(&graph, &registry).to_json().unwrap();
    assert!(!json.contains("fixture-build-id"));
    assert!(json.contains("buildFingerprint"));
}

#[test]
fn unreachable_actions_are_not_written_to_the_manifest() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "server/orphan.js",
        "\"use server\";\nexport async function drop() {}\n",
    );
    let graph = builder.build();
    let registry =
        ServerActionRegistry::from_graph(&graph, &BuildId::new("fixture-build-id").unwrap());
    let manifest = RscManifest::new(&graph, &registry);
    assert_eq!(registry.len(), 1);
    assert!(manifest.server_actions.is_empty());
}

#[test]
fn the_manifest_round_trips_through_serde() {
    let (graph, registry) = fixture();
    let manifest = RscManifest::new(&graph, &registry);
    let json = manifest.to_json().unwrap();
    let parsed: RscManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, manifest);
}

#[test]
fn collections_are_sorted() {
    let (graph, registry) = fixture();
    let manifest = RscManifest::new(&graph, &registry);
    assert!(
        manifest
            .modules
            .windows(2)
            .all(|pair| pair[0].path <= pair[1].path)
    );
    assert!(
        manifest
            .client_bundle_roots
            .windows(2)
            .all(|pair| pair[0] <= pair[1])
    );
    assert!(
        manifest
            .server_actions
            .windows(2)
            .all(|pair| pair[0].id <= pair[1].id)
    );
}

#[test]
fn writing_the_manifest_creates_the_output_directory() {
    let (graph, registry) = fixture();
    let manifest = RscManifest::new(&graph, &registry);
    let dir = tempfile::tempdir().unwrap();
    let out_dir = Utf8PathBuf::from_path_buf(dir.path().join("dist/nested")).unwrap();

    let path = write_manifest(&out_dir, &manifest).unwrap();
    assert_eq!(path.file_name(), Some(RSC_MANIFEST_FILE_NAME));
    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(written, manifest.to_json().unwrap());
    assert!(written.ends_with('\n'));
}

#[test]
fn writing_the_manifest_twice_is_idempotent() {
    let (graph, registry) = fixture();
    let manifest = RscManifest::new(&graph, &registry);
    let dir = tempfile::tempdir().unwrap();
    let out_dir = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

    let first = write_manifest(&out_dir, &manifest).unwrap();
    let second = write_manifest(&out_dir, &manifest).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        fs::read_to_string(&first).unwrap(),
        fs::read_to_string(&second).unwrap()
    );
}

#[test]
fn an_empty_graph_produces_an_empty_manifest() {
    let graph = RscGraphBuilder::new().build();
    let registry =
        ServerActionRegistry::from_graph(&graph, &BuildId::new("fixture-build-id").unwrap());
    let manifest = RscManifest::new(&graph, &registry);
    assert!(manifest.modules.is_empty());
    assert!(manifest.client_boundaries.is_empty());
    assert!(manifest.server_actions.is_empty());
    assert!(manifest.diagnostics.is_empty());
    assert_eq!(manifest.version, RSC_MANIFEST_VERSION);
}
