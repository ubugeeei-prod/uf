//! What a finished build carries through, and that it is reproducible.

use super::*;

#[test]
fn function_level_actions_survive_into_the_graph() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(
        server("app/page.js")
            .with_function_action(FunctionOwner::Named(CompactString::const_new("save"))),
    );
    let graph = builder.build();
    assert_eq!(
        graph.module("app/page.js").unwrap().function_actions.len(),
        1
    );
}

#[test]
fn building_the_same_input_twice_gives_the_same_graph() {
    let build = || {
        let mut builder = RscGraphBuilder::new();
        builder.add_module(server("app/page.js").with_import("./Counter.js"));
        builder.add_module(client("app/Counter.js"));
        builder.add_entry("app/page.js", EntryKind::Server);
        builder.build()
    };
    let first = build();
    let second = build();
    assert_eq!(first.modules(), second.modules());
    assert_eq!(first.client_boundaries(), second.client_boundaries());
}
