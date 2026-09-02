//! Where an update stops: which client module accepts it, and which edge
//! the climb refuses to cross.

use super::*;

#[test]
fn a_component_module_accepts_its_own_update() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        (
            "app/Panel.js",
            "\"use client\";\n// @flow\nimport { Counter } from \"./Counter.js\";\n\
             export function Panel() { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/Counter.js");

    assert_eq!(invalidation.kind(), UpdateKind::Hot);
    assert_eq!(paths(&graph, invalidation.client()), ["app/Counter.js"]);
    assert_eq!(paths(&graph, invalidation.boundaries()), ["app/Counter.js"]);
}

#[test]
fn a_client_module_with_no_accepting_importer_falls_back_to_a_full_reload() {
    let graph = graph_with(&[
        (
            "app/config.js",
            "\"use client\";\n// @flow\nexport const LIMIT = 3;\n",
        ),
        (
            "app/page.js",
            "// @flow\nimport { LIMIT } from \"./config.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/config.js");

    assert_eq!(invalidation.kind(), UpdateKind::FullReload);
    assert_eq!(
        invalidation.reload_reason(),
        Some(ReloadReason::NoAcceptingBoundary)
    );
    assert!(invalidation.boundaries().is_empty());
}

#[test]
fn the_update_stops_at_the_first_accepting_boundary() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        (
            "app/Panel.js",
            "\"use client\";\n// @flow\nimport { Counter } from \"./Counter.js\";\n\
             export function Panel() { return null; }\n",
        ),
        (
            "app/Shell.js",
            "\"use client\";\n// @flow\nimport { Panel } from \"./Panel.js\";\n\
             export function Shell() { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/util.js");

    assert_eq!(
        paths(&graph, invalidation.client()),
        ["app/util.js", "app/Counter.js"]
    );
    assert!(!paths(&graph, invalidation.client()).contains(&String::from("app/Panel.js")));
    assert!(!paths(&graph, invalidation.client()).contains(&String::from("app/Shell.js")));
}

#[test]
fn a_non_accepting_client_module_propagates_to_the_component_above_it() {
    let graph = graph_with(&[
        (
            "app/tokens.js",
            "\"use client\";\n// @flow\nexport const SPACING = 4;\n",
        ),
        (
            "app/Card.js",
            "\"use client\";\n// @flow\nimport { SPACING } from \"./tokens.js\";\n\
             export function Card() { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/tokens.js");

    assert_eq!(invalidation.kind(), UpdateKind::Hot);
    assert_eq!(
        paths(&graph, invalidation.client()),
        ["app/tokens.js", "app/Card.js"]
    );
    assert_eq!(paths(&graph, invalidation.boundaries()), ["app/Card.js"]);
}

// -- boundary crossing --------------------------------------------------------

#[test]
fn a_server_importer_of_a_client_module_is_not_climbed_into() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        (
            "app/page.js",
            "// @flow\nimport { Counter } from \"./Counter.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/Counter.js");

    assert!(invalidation.server().is_empty());
    assert_eq!(paths(&graph, invalidation.client()), ["app/Counter.js"]);
}

#[test]
fn a_server_action_module_is_a_server_module_for_invalidation() {
    let graph = graph_with(&[
        (
            "app/actions.js",
            "\"use server\";\n// @flow\nexport async function save() {}\n",
        ),
        (
            "app/page.js",
            "// @flow\nimport { save } from \"./actions.js\";\n\
             export default function () { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/actions.js");

    assert_eq!(invalidation.kind(), UpdateKind::Route);
    assert_eq!(
        paths(&graph, invalidation.server()),
        ["app/actions.js", "app/page.js"]
    );
}

#[test]
fn a_client_module_reached_only_from_a_client_entry_never_touches_the_route() {
    let graph = graph_with(&[
        (
            "app/Root.js",
            "\"use client\";\n// @flow\nimport { Leaf } from \"./Leaf.js\";\n\
             export function Root() { return null; }\n",
        ),
        (
            "app/Leaf.js",
            "\"use client\";\n// @flow\nexport const LEAF = 1;\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/Leaf.js");

    assert!(invalidation.server().is_empty());
    assert_eq!(invalidation.kind(), UpdateKind::Hot);
}

#[test]
fn a_module_below_a_client_boundary_is_client_side_even_without_a_directive() {
    let graph = graph_with(&[
        (
            "app/format.js",
            "// @flow\nexport function fmt() { return 1; }\n",
        ),
        (
            "app/Counter.js",
            "\"use client\";\n// @flow\nimport { fmt } from \"./format.js\";\n\
             export function Counter() { return null; }\n",
        ),
    ]);

    let invalidation = changed(&graph, "app/format.js");

    assert_eq!(
        paths(&graph, invalidation.client()),
        ["app/format.js", "app/Counter.js"]
    );
    assert!(invalidation.server().is_empty());
    assert_eq!(invalidation.kind(), UpdateKind::Hot);
}

// -- roots and lone modules ---------------------------------------------------

#[test]
fn a_route_with_no_importers_refreshes_itself() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        ("app/page.js", ROUTE),
    ]);

    let invalidation = changed(&graph, "app/page.js");

    assert_eq!(invalidation.kind(), UpdateKind::Route);
    assert_eq!(paths(&graph, invalidation.server()), ["app/page.js"]);
}

#[test]
fn a_lone_client_component_is_its_own_boundary() {
    let graph = graph_with(&[(
        "app/Widget.js",
        "\"use client\";\n// @flow\nexport function Widget() { return null; }\n",
    )]);

    let invalidation = changed(&graph, "app/Widget.js");

    assert_eq!(invalidation.kind(), UpdateKind::Hot);
    assert_eq!(paths(&graph, invalidation.boundaries()), ["app/Widget.js"]);
}

#[test]
fn a_lone_opaque_client_module_reloads_the_page() {
    let graph = graph_with(&[(
        "app/tokens.js",
        "\"use client\";\n// @flow\nexport const SPACING = 4;\n",
    )]);

    let invalidation = changed(&graph, "app/tokens.js");

    assert_eq!(invalidation.kind(), UpdateKind::FullReload);
    assert_eq!(
        invalidation.reload_reason(),
        Some(ReloadReason::NoAcceptingBoundary)
    );
}

// -- deletes ------------------------------------------------------------------

#[test]
fn boundaries_are_a_subset_of_the_client_set() {
    let graph = graph_with(&[
        ("app/util.js", UTIL),
        ("app/Counter.js", COUNTER),
        ("app/page.js", ROUTE),
    ]);

    let invalidation = changed(&graph, "app/util.js");

    for boundary in invalidation.boundaries() {
        assert!(invalidation.client().contains(boundary));
    }
}
