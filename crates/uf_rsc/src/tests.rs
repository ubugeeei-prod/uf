use super::*;

#[test]
fn the_crate_reexports_the_public_surface() {
    assert_eq!(module_environment(""), ModuleEnvironment::Server);
    assert_eq!(RSC_MANIFEST_FILE_NAME, "uf-rsc-manifest.json");
    assert_eq!(RSC_MANIFEST_VERSION, 1);
    assert_eq!(ACTION_ID_HEX_LEN, 64);
}

#[test]
fn errors_render_with_their_path() {
    let error = RscError::NonUtf8Source {
        path: Utf8PathBuf::from("app/page.js"),
    };
    assert_eq!(error.to_string(), "module app/page.js is not valid UTF-8");
}

#[test]
fn a_full_analysis_flows_from_sources_to_a_manifest() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/_uf.page.js",
        "import Counter from \"./Counter.js\";\nimport { save } from \"../server/actions.js\";\n",
    );
    builder.add_source("app/Counter.js", "\"use client\";\n");
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function save() {}\n",
    );
    builder.add_entry("app/_uf.page.js", EntryKind::Server);

    let graph = builder.build();
    let build_id = BuildId::new("lib-test-build-id").unwrap();
    let registry = ServerActionRegistry::from_graph(&graph, &build_id);
    let manifest = RscManifest::new(&graph, &registry);

    assert_eq!(manifest.modules.len(), 3);
    assert_eq!(manifest.client_boundaries.len(), 1);
    assert_eq!(manifest.server_actions.len(), 1);
    assert!(manifest.diagnostics.is_empty());
}
