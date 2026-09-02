//! Creates, deletes, the depth bound, and the vocabulary the verdict is
//! reported in.

use super::*;

#[test]
fn deleting_a_client_module_forces_a_full_reload() {
    let mut graph = graph_with(&[
        (
            "app/Badge.js",
            "\"use client\";\n// @flow\nexport function Badge() { return null; }\n",
        ),
        (
            "app/Counter.js",
            "\"use client\";\n// @flow\nimport { Badge } from \"./Badge.js\";\n\
             export function Counter() { return null; }\n",
        ),
    ]);
    let badge = graph.find("app/Badge.js").expect("known");
    graph.remove("app/Badge.js");

    let invalidation = invalidate(&graph, badge, ChangeKind::Deleted);

    assert_eq!(invalidation.kind(), UpdateKind::FullReload);
    assert_eq!(
        invalidation.reload_reason(),
        Some(ReloadReason::ModuleRemoved)
    );
}

#[test]
fn deleting_a_type_only_module_is_not_inert() {
    let mut graph = graph_with(&[
        ("app/types.js", TYPES),
        (
            "app/page.js",
            "// @flow\nimport type { User } from \"./types.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);
    let types = graph.find("app/types.js").expect("known");
    graph.remove("app/types.js");

    let invalidation = invalidate(&graph, types, ChangeKind::Deleted);

    assert!(!invalidation.is_empty());
    assert_eq!(invalidation.kind(), UpdateKind::Route);
}

#[test]
fn creating_a_module_that_answers_a_broken_import_updates_the_importer() {
    let mut graph = graph_with(&[(
        "app/Counter.js",
        "\"use client\";\n// @flow\nimport { fmt } from \"./format.js\";\n\
         export function Counter() { return null; }\n",
    )]);
    let created = graph
        .insert(
            "app/format.js",
            "// @flow\nexport function fmt() { return 1; }\n",
        )
        .expect("inserts");

    let invalidation = invalidate(&graph, created.id(), ChangeKind::Created);

    assert_eq!(invalidation.kind(), UpdateKind::Hot);
    assert_eq!(
        paths(&graph, invalidation.client()),
        ["app/Counter.js", "app/format.js"]
    );
}

#[test]
fn creating_a_type_only_module_is_inert() {
    let mut graph = DevGraph::new();
    let created = graph.insert("app/types.js", TYPES).expect("inserts");

    let invalidation = invalidate(&graph, created.id(), ChangeKind::Created);

    assert_eq!(invalidation.kind(), UpdateKind::Inert);
}

// -- bounds -------------------------------------------------------------------

#[test]
fn an_importer_chain_deeper_than_the_bound_degrades_to_a_full_reload() {
    let mut graph = DevGraph::new();
    let depth = MAX_INVALIDATION_DEPTH + 4;
    graph
        .insert("app/m0.js", "// @flow\nexport function m0() {}\n")
        .expect("inserts");
    for index in 1..depth {
        let source = format!(
            "// @flow\nimport m from \"./m{}.js\";\nexport function m{index}() {{}}\n",
            index - 1
        );
        graph
            .insert(&format!("app/m{index}.js"), &source)
            .expect("inserts");
    }

    let invalidation = changed(&graph, "app/m0.js");

    assert_eq!(
        invalidation.reload_reason(),
        Some(ReloadReason::DepthExceeded)
    );
    assert_eq!(invalidation.kind(), UpdateKind::FullReload);
}

#[test]
fn a_chain_inside_the_bound_does_not_degrade() {
    let mut graph = DevGraph::new();
    graph
        .insert("app/m0.js", "// @flow\nexport function m0() {}\n")
        .expect("inserts");
    for index in 1..8 {
        let source = format!(
            "// @flow\nimport m from \"./m{}.js\";\nexport function m{index}() {{}}\n",
            index - 1
        );
        graph
            .insert(&format!("app/m{index}.js"), &source)
            .expect("inserts");
    }

    let invalidation = changed(&graph, "app/m0.js");

    assert_eq!(invalidation.reload_reason(), None);
    assert_eq!(invalidation.len(), 8);
}

// -- shapes of the verdict ----------------------------------------------------

#[test]
fn update_kinds_have_stable_names() {
    assert_eq!(UpdateKind::Inert.as_str(), "inert");
    assert_eq!(UpdateKind::Hot.as_str(), "hot");
    assert_eq!(UpdateKind::Route.as_str(), "route");
    assert_eq!(UpdateKind::HotAndRoute.as_str(), "hot-and-route");
    assert_eq!(UpdateKind::FullReload.as_str(), "full-reload");
}

#[test]
fn change_kinds_have_stable_names() {
    assert_eq!(ChangeKind::Created.as_str(), "created");
    assert_eq!(ChangeKind::Modified.as_str(), "modified");
    assert_eq!(ChangeKind::Deleted.as_str(), "deleted");
}

#[test]
fn reload_reasons_carry_a_sentence_and_a_name() {
    for reason in [
        ReloadReason::NoAcceptingBoundary,
        ReloadReason::ModuleRemoved,
        ReloadReason::DepthExceeded,
        ReloadReason::Unservable,
        ReloadReason::TooManyModules,
    ] {
        assert!(!reason.message().is_empty());
        assert!(!reason.as_str().is_empty());
        assert!(
            reason
                .as_str()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-')
        );
    }
}

#[test]
fn a_full_reload_is_reported_as_one_whatever_else_was_invalidated() {
    let graph = graph_with(&[(
        "app/tokens.js",
        "\"use client\";\n// @flow\nexport const SPACING = 4;\n",
    )]);

    let invalidation = changed(&graph, "app/tokens.js");

    assert!(invalidation.kind().is_full_reload());
    assert!(!invalidation.kind().is_inert());
    assert!(!invalidation.is_empty());
}

#[test]
fn an_inert_invalidation_reports_itself_as_empty() {
    let graph = graph_with(&[("app/types.js", TYPES)]);

    let invalidation = changed(&graph, "app/types.js");

    assert!(invalidation.is_empty());
    assert_eq!(invalidation.len(), 0);
    assert!(invalidation.kind().is_inert());
}

#[test]
fn update_sides_are_distinct_values() {
    assert_ne!(UpdateSide::Client, UpdateSide::Server);
}
