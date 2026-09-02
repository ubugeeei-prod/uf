//! The cases the feature is judged on: a type-only edit, a client
//! boundary, a server module, a cycle, a deleted file, an unrelated edit,
//! and a shared dependency.

use super::*;

#[test]
fn a_type_only_module_invalidates_nothing_at_runtime() {
    let graph = graph_with(&[
        ("app/types.js", TYPES),
        (
            "app/Counter.js",
            "\"use client\";\n// @flow\nimport type { User } from \"./types.js\";\n\
             export function Counter() { return null; }\n",
        ),
        ("app/page.js", ROUTE),
    ]);

    let invalidation = changed(&graph, "app/types.js");

    assert!(invalidation.is_empty());
    assert_eq!(invalidation.kind(), UpdateKind::Inert);
    assert!(invalidation.client().is_empty());
    assert!(invalidation.server().is_empty());
    assert_eq!(invalidation.reload_reason(), None);
}

#[test]
fn a_client_boundary_invalidates_the_client_graph_above_it() {
    let graph = graph_with(&[
        (
            "app/Badge.js",
            "\"use client\";\n// @flow\nexport const BADGE = 1;\n",
        ),
        (
            "app/Counter.js",
            "\"use client\";\n// @flow\nimport { BADGE } from \"./Badge.js\";\n\
             export function Counter() { return null; }\n",
        ),
        (
            "app/page.js",
            "// @flow\nimport Counter from \"./Counter.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/Badge.js");

    assert_eq!(invalidation.kind(), UpdateKind::Hot);
    assert_eq!(
        paths(&graph, invalidation.client()),
        ["app/Badge.js", "app/Counter.js"]
    );
    assert_eq!(paths(&graph, invalidation.boundaries()), ["app/Counter.js"]);
    // The route above the boundary is untouched: the server holds a reference
    // to the client module, not its code.
    assert!(invalidation.server().is_empty());
}

#[test]
fn a_server_module_invalidates_the_route_not_the_browser_bundle() {
    let graph = graph_with(&[
        (
            "app/data.js",
            "// @flow\nexport function load() { return 1; }\n",
        ),
        (
            "app/page.js",
            "// @flow\nimport { load } from \"./data.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/data.js");

    assert_eq!(invalidation.kind(), UpdateKind::Route);
    assert_eq!(
        paths(&graph, invalidation.server()),
        ["app/data.js", "app/page.js"]
    );
    assert!(invalidation.client().is_empty());
    assert!(invalidation.boundaries().is_empty());
}

#[test]
fn a_cycle_terminates() {
    let graph = graph_with(&[
        (
            "app/a.js",
            "// @flow\nimport b from \"./b.js\";\nexport function a() {}\n",
        ),
        (
            "app/b.js",
            "// @flow\nimport c from \"./c.js\";\nexport function b() {}\n",
        ),
        (
            "app/c.js",
            "// @flow\nimport a from \"./a.js\";\nexport function c() {}\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/a.js");

    assert_eq!(invalidation.len(), 3);
    assert_eq!(
        paths(&graph, invalidation.server()),
        ["app/a.js", "app/b.js", "app/c.js"]
    );
    assert_eq!(invalidation.kind(), UpdateKind::Route);
}

#[test]
fn a_self_referential_cycle_of_two_terminates() {
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

    assert_eq!(changed(&graph, "app/a.js").len(), 2);
    assert_eq!(changed(&graph, "app/b.js").len(), 2);
}

#[test]
fn a_file_deleted_between_the_event_and_the_read_is_not_a_crash() {
    let mut graph = graph_with(&[
        ("app/util.js", UTIL),
        (
            "app/page.js",
            "// @flow\nimport { helper } from \"./util.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);
    let util = graph.find("app/util.js").expect("known");
    graph.remove("app/util.js");

    let invalidation = invalidate(&graph, util, ChangeKind::Deleted);

    assert_eq!(
        paths(&graph, invalidation.server()),
        ["app/util.js", "app/page.js"]
    );
    assert_eq!(invalidation.kind(), UpdateKind::Route);
}

#[test]
fn invalidating_a_module_the_graph_does_not_hold_is_empty_rather_than_a_panic() {
    let graph = graph_with(&[("app/page.js", ROUTE)]);
    let other = graph_with(&[("app/a.js", UTIL), ("app/b.js", UTIL), ("app/c.js", UTIL)]);
    let stranger = other.find("app/c.js").expect("known in the other graph");

    let invalidation = invalidate(&graph, stranger, ChangeKind::Modified);

    assert!(invalidation.is_empty());
}

// -- the two the bar names ----------------------------------------------------

#[test]
fn an_unrelated_edit_invalidates_nothing() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        ("app/page.js", ROUTE),
        (
            "app/elsewhere/Widget.js",
            "\"use client\";\n// @flow\nexport function Widget() { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/elsewhere/Widget.js");

    // The widget itself is the whole update: nothing else imports it, and it
    // accepts its own change.
    assert_eq!(
        paths(&graph, invalidation.client()),
        ["app/elsewhere/Widget.js"]
    );
    assert!(invalidation.server().is_empty());
    assert!(!paths(&graph, invalidation.client()).contains(&String::from("app/page.js")));
    assert!(!paths(&graph, invalidation.client()).contains(&String::from("app/Counter.js")));
    assert_eq!(invalidation.kind(), UpdateKind::Hot);
}

#[test]
fn a_shared_dependency_edit_invalidates_both_dependents() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        ("app/page.js", ROUTE),
    ]);

    let invalidation = changed(&graph, "app/util.js");

    assert_eq!(invalidation.kind(), UpdateKind::HotAndRoute);
    assert_eq!(
        paths(&graph, invalidation.client()),
        ["app/util.js", "app/Counter.js"]
    );
    assert_eq!(
        paths(&graph, invalidation.server()),
        ["app/util.js", "app/page.js"]
    );
    assert_eq!(paths(&graph, invalidation.boundaries()), ["app/Counter.js"]);
}

// -- Fast Refresh acceptance --------------------------------------------------

#[test]
fn a_diamond_reports_each_module_once_per_side() {
    let graph = graph_with(&[
        ("app/base.js", UTIL),
        (
            "app/left.js",
            "// @flow\nimport { helper } from \"./base.js\";\nexport function left() {}\n",
        ),
        (
            "app/right.js",
            "// @flow\nimport { helper } from \"./base.js\";\nexport function right() {}\n",
        ),
        (
            "app/page.js",
            "// @flow\nimport l from \"./left.js\";\nimport r from \"./right.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/base.js");

    // Identifier order, which is the order the modules were scanned in.
    let server = paths(&graph, invalidation.server());
    assert_eq!(
        server,
        ["app/base.js", "app/left.js", "app/right.js", "app/page.js"]
    );
    let mut deduped = server.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped.len(), server.len());
}

#[test]
fn invalidation_is_deterministic_across_repeated_runs() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        ("app/page.js", ROUTE),
    ]);

    let first = changed(&graph, "app/util.js");
    for _ in 0..8 {
        assert_eq!(changed(&graph, "app/util.js"), first);
    }
}
