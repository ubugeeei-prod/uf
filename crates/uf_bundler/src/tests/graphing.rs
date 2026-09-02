//! Building the module graph, and the facts it carries.

use camino::Utf8Path;
use uf_rsc::{ModuleEnvironment, ModuleReachability};

use super::fixture::Fixture;
use crate::graph::{Edge, relative_specifier};
use crate::resolve::Resolver;
use crate::{BundlerLimits, build_graph};

fn graph_of(fixture: &Fixture) -> crate::ModuleGraph {
    let mut resolver = Resolver::new(fixture.root.clone(), BundlerLimits::small());
    build_graph(
        &mut resolver,
        &fixture.container(),
        &fixture.entries,
        &BundlerLimits::small(),
    )
    .expect("graph builds")
}

#[test]
fn a_relative_specifier_is_computed_between_two_module_paths() {
    assert_eq!(
        relative_specifier(Utf8Path::new("app/page.js"), Utf8Path::new("app/util.js")),
        "./util.js"
    );
    assert_eq!(
        relative_specifier(Utf8Path::new("app/page.js"), Utf8Path::new("lib/util.js")),
        "../lib/util.js"
    );
    assert_eq!(
        relative_specifier(Utf8Path::new("app.js"), Utf8Path::new("lib/util.js")),
        "./lib/util.js"
    );
    assert_eq!(
        relative_specifier(
            Utf8Path::new("app/deep/page.js"),
            Utf8Path::new("shared.js")
        ),
        "../../shared.js"
    );
}

#[test]
fn a_relative_specifier_always_starts_with_a_dot() {
    let specifier = relative_specifier(Utf8Path::new("a.js"), Utf8Path::new("b.js"));

    assert!(specifier.starts_with("./"), "{specifier}");
}

#[test]
fn the_graph_holds_every_reachable_module() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const a = 1;\n");
    fixture.entry(
        "app.js",
        "import { a } from \"./util.js\";\nexport const b = a;\n",
    );

    let graph = graph_of(&fixture);

    assert_eq!(graph.modules().len(), 2);
    assert!(graph.index_of(Utf8Path::new("util.js")).is_some());
    fixture.keep();
}

#[test]
fn a_module_is_loaded_once_however_many_times_it_is_imported() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const a = 1;\n");
    fixture.write(
        "one.js",
        "import { a } from \"./util.js\";\nexport const one = a;\n",
    );
    fixture.write(
        "two.js",
        "import { a } from \"./util.js\";\nexport const two = a;\n",
    );
    fixture.entry(
        "app.js",
        "import { one } from \"./one.js\";\nimport { two } from \"./two.js\";\nexport const sum = one + two;\n",
    );

    let graph = graph_of(&fixture);

    assert_eq!(graph.modules().len(), 4);
    fixture.keep();
}

#[test]
fn a_cyclic_import_graph_terminates() {
    let mut fixture = Fixture::new();
    fixture.write("a.js", "import \"./b.js\";\nexport const a = 1;\n");
    fixture.write("b.js", "import \"./a.js\";\nexport const b = 2;\n");
    fixture.entry("app.js", "import \"./a.js\";\nexport const c = 3;\n");

    let graph = graph_of(&fixture);

    assert_eq!(graph.modules().len(), 3);
    fixture.keep();
}

#[test]
fn a_self_import_terminates() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "import \"./app.js\";\nexport const a = 1;\n");

    let graph = graph_of(&fixture);

    assert_eq!(graph.modules().len(), 1);
    fixture.keep();
}

#[test]
fn an_unresolved_bare_import_becomes_an_external_edge() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "import { useState } from \"react\";\nexport const a = useState;\n",
    );

    let graph = graph_of(&fixture);

    let module = graph.module(graph.entries()[0]);
    assert_eq!(module.edges.len(), 1);
    assert!(matches!(&module.edges[0], Edge::External(specifier) if specifier == "react"));
    fixture.keep();
}

#[test]
fn a_module_records_its_depth_from_the_entry() {
    let mut fixture = Fixture::new();
    fixture.write("deep.js", "export const deep = 1;\n");
    fixture.write("mid.js", "import \"./deep.js\";\nexport const mid = 1;\n");
    fixture.entry("app.js", "import \"./mid.js\";\nexport const top = 1;\n");

    let graph = graph_of(&fixture);

    let deep = graph.index_of(Utf8Path::new("deep.js")).expect("module");
    assert_eq!(graph.module(deep).depth, 2);
    fixture.keep();
}

#[test]
fn the_environment_comes_from_the_source_before_any_transform() {
    let mut fixture = Fixture::new();
    fixture.write("client.js", "\"use client\";\nexport const a = 1;\n");
    fixture.entry(
        "app.js",
        "import { a } from \"./client.js\";\nexport const b = a;\n",
    );

    let graph = graph_of(&fixture);

    let client = graph.index_of(Utf8Path::new("client.js")).expect("module");
    assert_eq!(graph.module(client).environment, ModuleEnvironment::Client);
    fixture.keep();
}

#[test]
fn the_rsc_graph_sees_the_resolved_edges() {
    let mut fixture = Fixture::new();
    fixture.write(
        "client.js",
        "\"use client\";\nexport default function Counter() {\n  return 1;\n}\n",
    );
    fixture.entry(
        "app.js",
        "import Counter from \"./client.js\";\nexport default Counter;\n",
    );

    let graph = graph_of(&fixture);

    assert_eq!(graph.rsc().client_boundaries().len(), 1);
    assert_eq!(graph.client_roots().len(), 1);
    assert_eq!(
        graph
            .rsc()
            .module("client.js")
            .expect("module")
            .reachability,
        ModuleReachability::ClientOnly
    );
    fixture.keep();
}

#[test]
fn a_server_only_module_is_marked_server_only() {
    let mut fixture = Fixture::new();
    fixture.write("db.server.js", "export const query = () => 1;\n");
    fixture.entry(
        "app.js",
        "import { query } from \"./db.server.js\";\nexport const run = query;\n",
    );

    let graph = graph_of(&fixture);

    assert_eq!(
        graph
            .rsc()
            .module("db.server.js")
            .expect("module")
            .reachability,
        ModuleReachability::ServerOnly
    );
    fixture.keep();
}

#[test]
fn a_module_reachable_from_both_halves_is_marked_shared() {
    let mut fixture = Fixture::new();
    fixture.write("shared.js", "export const shared = 1;\n");
    fixture.write(
        "client.js",
        "\"use client\";\nimport { shared } from \"./shared.js\";\nexport default function C() {\n  return shared;\n}\n",
    );
    fixture.entry(
        "app.js",
        "import { shared } from \"./shared.js\";\nimport C from \"./client.js\";\nexport default [shared, C];\n",
    );

    let graph = graph_of(&fixture);

    assert_eq!(
        graph
            .rsc()
            .module("shared.js")
            .expect("module")
            .reachability,
        ModuleReachability::ServerAndClient
    );
    assert!(graph.is_client_reachable(graph.index_of(Utf8Path::new("shared.js")).expect("module")));
    fixture.keep();
}

#[test]
fn a_module_that_only_declares_has_no_side_effects() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "import { a } from \"react\";\nconst b = 1;\nfunction c() { d(); }\nexport { b, c };\n",
    );

    let graph = graph_of(&fixture);

    assert!(graph.module(graph.entries()[0]).shakeable);
    fixture.keep();
}

#[test]
fn a_top_level_call_is_a_side_effect() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "register();\nexport const a = 1;\n");

    let graph = graph_of(&fixture);

    assert!(!graph.module(graph.entries()[0]).shakeable);
    fixture.keep();
}

#[test]
fn a_directive_prologue_is_not_a_side_effect() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "\"use strict\";\nexport const a = 1;\n");

    let graph = graph_of(&fixture);

    assert!(graph.module(graph.entries()[0]).shakeable);
    fixture.keep();
}
