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

/// The question the bundler asks, which proximity alone does not answer.
///
/// A `"use client"` module reaches no boundary of its own — it *is* the far
/// side of one — so `proximity` says `Isolated` about the one module that is a
/// client bundle root by definition. Anything deciding what to ship has to ask
/// both halves, which is what `requires_client_bundle` is.
#[test]
fn the_client_bundle_takes_a_client_module_and_everything_above_one() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_module(server("app/static.js").with_import("./almanac.js"));
    builder.add_module(server("app/almanac.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    builder.add_entry("app/static.js", EntryKind::Server);
    let graph = builder.build();

    assert!(
        graph
            .module("app/page.js")
            .unwrap()
            .requires_client_bundle()
    );
    assert!(
        graph
            .module("app/Counter.js")
            .unwrap()
            .requires_client_bundle()
    );
    assert!(
        !graph
            .module("app/static.js")
            .unwrap()
            .requires_client_bundle()
    );
    assert!(
        !graph
            .module("app/almanac.js")
            .unwrap()
            .requires_client_bundle()
    );
}

#[test]
fn a_use_server_module_a_client_module_imports_stays_on_the_server() {
    // The mirror of `a_client_module_imported_by_a_server_module_is_a_boundary`.
    // The server does not execute a `"use client"` module, and the client does
    // not execute a `"use server"` one: `@uniflowed/vite` gives the browser one
    // `createServerReference` per callable export and nothing else, so the
    // module's body and everything it imported are the server's.
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("./actions.js"));
    builder.add_source(
        "app/actions.js",
        "\"use server\";\nimport { rows } from \"./db.js\";\nexport async function count() {}\n",
    );
    builder.add_module(server("app/db.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(
        graph.module("app/actions.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
    // And the walk continues with the server colour, so the data layer an
    // action reaches is not shared code either.
    assert_eq!(
        graph.module("app/db.js").unwrap().reachability,
        ModuleReachability::ServerOnly
    );
    // It is not a client bundle root and nothing about it is: the reference is
    // what the browser gets, and a reference is not a module of this graph.
    assert_eq!(graph.client_bundle_roots().len(), 1);
    assert_eq!(
        graph
            .module_by_id(graph.client_bundle_roots()[0])
            .unwrap()
            .path,
        "app/Counter.js"
    );
}

#[test]
fn a_server_only_import_reached_only_through_an_action_is_not_a_client_leak() {
    // The same rule seen from the diagnostic that used to fire. Before the
    // client colour stopped at a `"use server"` module, `app/actions.js` was
    // client-reachable, so importing `@uniflowed/server` from it was reported
    // as server code in the browser's graph — which it was, because there was
    // no transform to make it otherwise. There is one now, and the analysis
    // says what the bundle does.
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("./actions.js"));
    builder.add_source(
        "app/actions.js",
        "\"use server\";\nimport { cookies } from \"@uniflowed/server\";\n\
         export async function whoami() {}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(
        graph.diagnostics(),
        &[],
        "a `use server` module is not in the client graph, so its server-only imports are not a leak"
    );
}

#[test]
fn a_client_module_importing_server_only_code_directly_is_still_a_leak() {
    // And the rule it must not have widened: the exemption is for a module
    // that becomes a reference, not for anything a client component imports.
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("@uniflowed/server"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(graph.diagnostics().len(), 1);
    assert_eq!(
        graph.diagnostics()[0].rule(),
        "rsc/server-only-import-in-client"
    );
}
