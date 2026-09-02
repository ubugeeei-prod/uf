//! Module identity, specifier resolution and the escapes it must refuse.

use super::*;

#[test]
fn an_empty_graph_has_no_modules_or_diagnostics() {
    let graph = RscGraphBuilder::new().build();
    assert!(graph.modules().is_empty());
    assert!(graph.diagnostics().is_empty());
    assert!(!graph.has_errors());
}

#[test]
fn modules_are_ordered_by_path() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("z.js"));
    builder.add_module(server("a.js"));
    builder.add_module(server("m.js"));
    let graph = builder.build();
    let paths: Vec<_> = graph
        .modules()
        .iter()
        .map(|module| module.path.as_str())
        .collect();
    assert_eq!(paths, vec!["a.js", "m.js", "z.js"]);
}

#[test]
fn duplicate_module_paths_are_collapsed() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js"));
    builder.add_module(server("a.js"));
    assert_eq!(builder.build().modules().len(), 1);
}

#[test]
fn relative_imports_resolve_to_modules() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./widget.js"));
    builder.add_module(server("app/widget.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    let page = graph.module("app/page.js").unwrap();
    assert_eq!(page.imports.len(), 1);
    assert_eq!(
        graph.module_by_id(page.imports[0]).unwrap().path,
        "app/widget.js"
    );
}

#[test]
fn extensionless_imports_resolve_to_the_js_file() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./widget"));
    builder.add_module(server("app/widget.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}

#[test]
fn directory_imports_resolve_to_index_js() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./widget"));
    builder.add_module(server("app/widget/index.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}

#[test]
fn parent_relative_imports_resolve() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("../server/actions.js"));
    builder.add_module(actions("server/actions.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}

#[test]
fn bare_specifiers_are_external() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("@uniflowed/react"));
    let graph = builder.build();
    let page = graph.module("app/page.js").unwrap();
    assert!(page.imports.is_empty());
    assert_eq!(page.external_imports.as_slice(), &["@uniflowed/react"]);
}

#[test]
fn an_import_climbing_out_of_the_project_is_rejected() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("../../../../etc/passwd"));
    let graph = builder.build();
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/import-escapes-project-root"
    );
}

#[test]
fn a_module_path_outside_the_project_is_dropped() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("../outside.js"));
    let graph = builder.build();
    assert!(graph.modules().is_empty());
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/module-outside-project-root"
    );
}

#[test]
fn an_absolute_module_path_is_dropped() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("/etc/passwd"));
    assert!(builder.build().modules().is_empty());
}

#[test]
fn server_only_specifier_detection_covers_packages_and_files() {
    assert!(is_server_only_specifier("@uniflowed/server"));
    assert!(is_server_only_specifier("@uniflowed/server/db"));
    assert!(is_server_only_specifier("server-only"));
    assert!(is_server_only_specifier("./secrets.server.js"));
    assert!(!is_server_only_specifier("@uniflowed/react"));
    assert!(!is_server_only_specifier("./server.js"));
    assert!(!is_server_only_specifier("@uniflowed/serverless"));
}

#[test]
fn windows_style_paths_are_normalized() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app\\page.js").with_import(".\\widget.js"));
    builder.add_module(server("app/widget.js"));
    let graph = builder.build();
    assert_eq!(graph.module("app/page.js").unwrap().imports.len(), 1);
}
