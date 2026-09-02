//! Edges: how a specifier becomes one, and what survives a rescan, a
//! removal, or a target that arrives late.

use super::*;

#[test]
fn a_fresh_graph_holds_nothing() {
    let graph = DevGraph::new();

    assert!(graph.is_empty());
    assert_eq!(graph.len(), 0);
    assert_eq!(graph.present_count(), 0);
    assert!(graph.find("app/page.js").is_none());
}

#[test]
fn inserting_a_module_reports_it_as_created() {
    let mut graph = DevGraph::new();
    let insertion = graph.insert("app/page.js", SERVER_HELPER).expect("inserts");

    assert!(insertion.is_new());
    assert_eq!(graph.present_count(), 1);
    assert_eq!(module(&graph, "app/page.js").revision(), 1);
}

#[test]
fn inserting_a_module_twice_reports_it_as_updated() {
    let mut graph = DevGraph::new();
    graph.insert("app/page.js", SERVER_HELPER).expect("inserts");
    let second = graph.insert("app/page.js", SERVER_HELPER).expect("inserts");

    assert!(!second.is_new());
    assert_eq!(graph.present_count(), 1);
    assert_eq!(module(&graph, "app/page.js").revision(), 2);
}

#[test]
fn a_relative_import_becomes_an_edge_in_both_directions() {
    let graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function Page() {}\n",
        ),
    ]);

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
    assert_eq!(importer_paths(&graph, "app/util.js"), ["app/page.js"]);
}

#[test]
fn an_import_that_arrives_before_its_target_links_when_the_target_appears() {
    let mut graph = DevGraph::new();
    graph
        .insert(
            "app/page.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function Page() {}\n",
        )
        .expect("inserts");
    assert!(import_paths(&graph, "app/page.js").is_empty());

    graph.insert("app/util.js", SERVER_HELPER).expect("inserts");

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
    assert_eq!(importer_paths(&graph, "app/util.js"), ["app/page.js"]);
}

#[test]
fn an_extensionless_specifier_resolves_through_the_js_fallback() {
    let graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./util\";\nexport function Page() {}\n",
        ),
    ]);

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
}

#[test]
fn a_directory_specifier_resolves_through_the_index_fallback() {
    let graph = graph_with(&[
        ("app/widgets/index.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./widgets\";\nexport function Page() {}\n",
        ),
    ]);

    assert_eq!(
        import_paths(&graph, "app/page.js"),
        ["app/widgets/index.js"]
    );
}

#[test]
fn a_bare_specifier_is_not_a_graph_edge() {
    let graph = graph_with(&[(
        "app/page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\nexport function Page() {}\n",
    )]);

    assert!(import_paths(&graph, "app/page.js").is_empty());
    assert_eq!(graph.len(), 1);
}

#[test]
fn a_specifier_that_climbs_out_of_the_project_is_not_an_edge() {
    let graph = graph_with(&[(
        "app/page.js",
        "// @flow\nimport secret from \"../../../.env\";\nexport function Page() {}\n",
    )]);

    assert!(import_paths(&graph, "app/page.js").is_empty());
    assert!(graph.find("../../.env").is_none());
    assert_eq!(graph.len(), 1);
}

#[test]
fn a_backslash_specifier_is_folded_like_a_forward_slash() {
    let graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \".\\\\util.js\";\nexport function Page() {}\n",
        ),
    ]);

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
}

#[test]
fn a_self_import_is_dropped_rather_than_recorded() {
    let graph = graph_with(&[(
        "app/page.js",
        "// @flow\nimport self from \"./page.js\";\nexport function Page() {}\n",
    )]);

    assert!(import_paths(&graph, "app/page.js").is_empty());
    assert!(importer_paths(&graph, "app/page.js").is_empty());
}

#[test]
fn a_repeated_specifier_produces_one_edge() {
    let graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport a from \"./util.js\";\nimport b from \"./util\";\n\
             export function Page() {}\n",
        ),
    ]);

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
    assert_eq!(importer_paths(&graph, "app/util.js"), ["app/page.js"]);
}

#[test]
fn rescanning_a_module_drops_the_edges_it_no_longer_names() {
    let mut graph = graph_with(&[
        ("app/a.js", SERVER_HELPER),
        ("app/b.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport a from \"./a.js\";\nimport b from \"./b.js\";\n\
             export function Page() {}\n",
        ),
    ]);
    assert_eq!(import_paths(&graph, "app/page.js").len(), 2);

    graph
        .insert(
            "app/page.js",
            "// @flow\nimport a from \"./a.js\";\nexport function Page() {}\n",
        )
        .expect("rescans");

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/a.js"]);
    assert!(importer_paths(&graph, "app/b.js").is_empty());
}

#[test]
fn removing_a_module_keeps_its_slot_and_its_importers() {
    let mut graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function Page() {}\n",
        ),
    ]);

    let removed = graph.remove("app/util.js").expect("the module was known");

    assert_eq!(
        graph.module(removed).expect("slot").state(),
        ModuleState::Absent
    );
    assert_eq!(graph.len(), 2);
    assert_eq!(graph.present_count(), 1);
    assert_eq!(importer_paths(&graph, "app/util.js"), ["app/page.js"]);
}

#[test]
fn removing_a_module_the_graph_never_saw_is_not_an_error() {
    let mut graph = DevGraph::new();

    assert!(graph.remove("app/ghost.js").is_none());
    assert!(graph.remove("../../.env").is_none());
    assert!(graph.is_empty());
}

#[test]
fn removing_a_module_twice_is_idempotent() {
    let mut graph = graph_with(&[("app/util.js", SERVER_HELPER)]);

    let first = graph.remove("app/util.js").expect("known");
    let second = graph.remove("app/util.js").expect("still known");

    assert_eq!(first, second);
    assert_eq!(graph.present_count(), 0);
}

#[test]
fn a_removed_module_that_returns_is_present_again() {
    let mut graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function Page() {}\n",
        ),
    ]);
    graph.remove("app/util.js");

    graph.insert("app/util.js", SERVER_HELPER).expect("returns");

    assert_eq!(module(&graph, "app/util.js").state(), ModuleState::Present);
    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
}

#[test]
fn a_removed_module_drops_its_own_outgoing_edges() {
    let mut graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function Page() {}\n",
        ),
    ]);

    graph.remove("app/page.js");

    assert!(import_paths(&graph, "app/page.js").is_empty());
    assert!(importer_paths(&graph, "app/util.js").is_empty());
}

#[test]
fn a_cycle_is_recorded_in_both_directions() {
    let graph = graph_with(&[
        (
            "app/a.js",
            "// @flow\nimport b from \"./b.js\";\nexport function a() {}\n",
        ),
        (
            "app/b.js",
            "// @flow\nimport a from \"./a.js\";\nexport function b() {}\n",
        ),
    ]);

    assert_eq!(import_paths(&graph, "app/a.js"), ["app/b.js"]);
    assert_eq!(import_paths(&graph, "app/b.js"), ["app/a.js"]);
    assert_eq!(importer_paths(&graph, "app/a.js"), ["app/b.js"]);
}

#[test]
fn a_re_export_is_an_edge() {
    let graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/index.js",
            "// @flow\nexport { helper } from \"./util.js\";\n",
        ),
    ]);

    assert_eq!(import_paths(&graph, "app/index.js"), ["app/util.js"]);
}

#[test]
fn a_dynamic_import_is_an_edge() {
    let graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nexport function Page() { import(\"./util.js\"); }\n",
        ),
    ]);

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
}

#[test]
fn edges_survive_a_rescan_that_changes_nothing() {
    let mut graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function Page() {}\n",
        ),
    ]);

    for _ in 0..5 {
        graph
            .insert(
                "app/page.js",
                "// @flow\nimport { helper } from \"./util.js\";\nexport function Page() {}\n",
            )
            .expect("rescans");
    }

    assert_eq!(import_paths(&graph, "app/page.js"), ["app/util.js"]);
    assert_eq!(importer_paths(&graph, "app/util.js"), ["app/page.js"]);
}

#[test]
fn two_importers_of_one_module_are_both_recorded() {
    let graph = graph_with(&[
        ("app/util.js", SERVER_HELPER),
        (
            "app/left.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function left() {}\n",
        ),
        (
            "app/right.js",
            "// @flow\nimport { helper } from \"./util.js\";\nexport function right() {}\n",
        ),
    ]);

    assert_eq!(
        importer_paths(&graph, "app/util.js"),
        ["app/left.js", "app/right.js"]
    );
}

#[test]
fn scanning_one_module_leaves_every_other_revision_alone() {
    let mut graph = graph_with(&[
        ("app/a.js", SERVER_HELPER),
        ("app/b.js", SERVER_HELPER),
        ("app/c.js", SERVER_HELPER),
    ]);
    let before: Vec<u32> = ["app/a.js", "app/b.js", "app/c.js"]
        .iter()
        .map(|path| module(&graph, path).revision())
        .collect();

    graph.insert("app/b.js", SERVER_HELPER).expect("rescans");

    let after: Vec<u32> = ["app/a.js", "app/b.js", "app/c.js"]
        .iter()
        .map(|path| module(&graph, path).revision())
        .collect();
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[1] + 1);
    assert_eq!(after[2], before[2]);
}

#[test]
fn insertion_reports_the_identifier_it_wrote() {
    let mut graph = DevGraph::new();
    let created = graph.insert("app/page.js", SERVER_HELPER).expect("inserts");
    let updated = graph.insert("app/page.js", SERVER_HELPER).expect("rescans");

    assert_eq!(created.id(), updated.id());
    assert!(created.is_new());
    assert!(!updated.is_new());
    assert_eq!(created.id().index(), 0);
}
