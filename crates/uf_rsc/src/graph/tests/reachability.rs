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

/// The chain a module's path into the client bundle is, spelled out.
fn reason(graph: &RscGraph, path: &str) -> ClientBundleReason {
    let id = graph
        .module_id(path)
        .unwrap_or_else(|| panic!("no module at {path}"));
    graph.client_bundle_reason(id)
}

/// The chain as paths, which is what a report prints.
fn chain(graph: &RscGraph, path: &str) -> Vec<String> {
    match reason(graph, path) {
        ClientBundleReason::Imports(chain) => chain
            .into_iter()
            .map(|id| graph.module_by_id(id).unwrap().path.to_string())
            .collect(),
        other => panic!("expected a chain for {path}, got {other:?}"),
    }
}

/// The answer for the module the directive is on: itself.
#[test]
fn a_use_client_module_is_its_own_reason() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(
        reason(&graph, "app/Counter.js"),
        ClientBundleReason::Declared
    );
}

/// And for a module above one: the imports that get there, in order.
#[test]
fn a_module_above_a_boundary_names_the_imports_that_reach_it() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./section.js"));
    builder.add_module(server("app/section.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(
        chain(&graph, "app/page.js"),
        ["app/page.js", "app/section.js", "app/Counter.js"]
    );
    assert_eq!(
        chain(&graph, "app/section.js"),
        ["app/section.js", "app/Counter.js"]
    );
}

/// Server code that reaches no boundary has no chain, and says so.
#[test]
fn a_module_the_browser_never_evaluates_is_isolated() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./almanac.js"));
    builder.add_module(server("app/almanac.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(reason(&graph, "app/page.js"), ClientBundleReason::Isolated);
    assert_eq!(
        reason(&graph, "app/almanac.js"),
        ClientBundleReason::Isolated
    );
}

/// The shortest of several, because the point of the chain is to be read.
///
/// Both routes below reach the same boundary, one in a hop and one in three.
/// A depth-first walk would answer with whichever import was written first,
/// which is a fact about the source order and not about the graph.
#[test]
fn the_chain_is_the_shortest_way_to_a_boundary() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        server("app/page.js")
            .with_import("./long/a.js")
            .with_import("./Counter.js"),
    );
    builder.add_module(server("app/long/a.js").with_import("../long/b.js"));
    builder.add_module(server("app/long/b.js").with_import("../Counter.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(
        chain(&graph, "app/page.js"),
        ["app/page.js", "app/Counter.js"]
    );
}

/// A cycle above a boundary terminates, and the chain out of it is acyclic.
///
/// The import graph this crate walks contains cycles; the chain a reader is
/// handed must not. One predecessor per module is what makes that true by
/// construction rather than by a check afterwards.
#[test]
fn a_cycle_above_a_boundary_still_answers_with_an_acyclic_chain() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(server("app/page.js").with_import("./a.js"));
    builder.add_module(
        server("app/a.js")
            .with_import("./b.js")
            .with_import("./Counter.js"),
    );
    builder.add_module(server("app/b.js").with_import("./a.js"));
    builder.add_module(client("app/Counter.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    let chain = chain(&graph, "app/b.js");
    assert_eq!(chain, ["app/b.js", "app/a.js", "app/Counter.js"]);
    let mut unique = chain.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), chain.len(), "a chain never repeats a module");
}

/// The invariant that keeps the explanation and the split one decision.
///
/// `requires_client_bundle` is what the bundler asks and
/// `client_bundle_reason` is what a person asks, and the moment they can
/// disagree there are two analyses of the same question. Asserted over every
/// module of a graph that has all three answers in it rather than over a
/// module chosen to make the point.
#[test]
fn every_module_the_bundler_ships_has_a_reason_and_no_other_module_does() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        server("app/page.js")
            .with_import("./section.js")
            .with_import("./almanac.js"),
    );
    builder.add_module(server("app/section.js").with_import("./Counter.js"));
    builder.add_module(client("app/Counter.js").with_import("./format.js"));
    builder.add_module(server("app/format.js"));
    builder.add_module(server("app/almanac.js"));
    builder.add_module(server("app/orphan.js"));
    builder.add_entry("app/page.js", EntryKind::Server);
    let graph = builder.build();

    for module in graph.modules() {
        let id = graph.module_id(&module.path).unwrap();
        let isolated = graph.client_bundle_reason(id) == ClientBundleReason::Isolated;
        assert_eq!(
            module.requires_client_bundle(),
            !isolated,
            "{} disagrees with itself",
            module.path
        );
    }
}
