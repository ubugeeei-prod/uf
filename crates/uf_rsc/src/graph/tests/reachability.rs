//! Entry colouring, client boundaries, bundle roots and cycle termination.

use super::*;

#[test]
fn without_entries_every_module_is_unreachable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js"));
    let graph = builder.build();
    assert_eq!(
        graph.module("app/page.js").unwrap().reachability,
        ModuleReachability::Unreachable
    );
}

#[test]
fn a_server_entry_makes_its_imports_server_reachable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./data.js"));
    builder.add_module(server("app/data.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/data.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
}

#[test]
fn a_client_module_imported_by_a_server_module_is_a_boundary() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(graph.client_boundaries().len(), 1);
    let boundary = graph.client_boundaries()[0];
    assert_eq!(
        graph.module_by_id(boundary.importer).unwrap().path,
        "app/page.js"
    );
    assert_eq!(
        graph.module_by_id(boundary.client_module).unwrap().path,
        "app/Counter.js"
    );
    assert_eq!(graph.client_bundle_roots().len(), 1);
    assert_eq!(
        graph.module("app/Counter.js").unwrap().reachability,
        ModuleReachability::ClientOnly
    );
}

#[test]
fn code_below_a_client_boundary_is_client_reachable() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("./format.js"));
    builder.add_module(server("app/format.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/format.js").unwrap().reachability,
        ModuleReachability::ClientOnly
    );
}

#[test]
fn shared_code_is_reachable_from_both_halves() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        server("app/page.js")
            .with_import("./Counter.js")
            .with_import("./format.js"),
    );
    builder.add_module(client("app/Counter.js").with_import("./format.js"));
    builder.add_module(server("app/format.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/format.js").unwrap().reachability,
        ModuleReachability::ServerAndClient
    );
}

#[test]
fn a_client_entry_marks_its_module_a_bundle_root() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/entry.js"));
    builder.add_entry("app/entry.js", EntryKind::Client);
    let graph = builder.build();
    assert_eq!(graph.client_bundle_roots().len(), 1);
}

#[test]
fn a_client_module_named_as_a_server_entry_is_still_a_client_module() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(client("app/entry.js"));
    builder.add_entry("app/entry.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("app/entry.js").unwrap().reachability,
        ModuleReachability::ClientOnly
    );
}

#[test]
fn a_two_module_cycle_terminates() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js").with_import("./b.js"));
    builder.add_module(server("b.js").with_import("./a.js"));
    builder.add_entry("a.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("b.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
}

#[test]
fn a_self_import_terminates() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js").with_import("./a.js"));
    builder.add_entry("a.js", EntryKind::Server);
    assert_eq!(
        builder.build().module("a.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
}

#[test]
fn a_cycle_crossing_a_client_boundary_terminates() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("a.js").with_import("./b.js"));
    builder.add_module(client("b.js").with_import("./c.js"));
    builder.add_module(server("c.js").with_import("./a.js"));
    builder.add_entry("a.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(
        graph.module("a.js").unwrap().reachability,
        ModuleReachability::ServerAndClient
    );
    assert_eq!(graph.client_boundaries().len(), 1);
}

#[test]
fn a_long_cycle_terminates() {
    let mut builder = RscGraphBuilder::new();
    let size = 5_000usize;
    for position in 0..size {
        let next = (position + 1) % size;
        builder.add_module(server(format!("m{position}.js")).with_import(format!("./m{next}.js")));
    }
    builder.add_entry("m0.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(graph.modules().len(), size);
    assert!(
        graph
            .modules()
            .iter()
            .all(|module| module.reachability == ModuleReachability::ServerOnly)
    );
}

#[test]
fn a_ten_thousand_module_graph_builds() {
    let mut builder = RscGraphBuilder::new();
    let size = 10_000usize;
    for position in 0..size {
        let mut module = server(format!("m{position}.js"));
        if position + 1 < size {
            module = module.with_import(format!("./m{}.js", position + 1));
        }
        if position + 7 < size {
            module = module.with_import(format!("./m{}.js", position + 7));
        }
        builder.add_module(module);
    }
    builder.add_entry("m0.js", EntryKind::Server);
    let graph = builder.build();
    assert_eq!(graph.modules().len(), size);
    assert!(
        graph
            .modules()
            .iter()
            .all(|module| module.reachability.is_server_reachable())
    );
}

#[test]
fn proximity_marks_modules_that_can_hand_a_closure_to_the_client() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./section.js"));
    builder.add_module(server("app/section.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_module(server("app/lonely.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(
        graph.module("app/page.js").unwrap().proximity,
        ClientBoundaryProximity::ReachesBoundary
    );
    assert_eq!(
        graph.module("app/section.js").unwrap().proximity,
        ClientBoundaryProximity::ReachesBoundary
    );
    assert_eq!(
        graph.module("app/lonely.js").unwrap().proximity,
        ClientBoundaryProximity::Isolated
    );
}
