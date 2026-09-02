//! What one scan tells the graph about a module: the environment it runs
//! in, and whether React Fast Refresh can swap it in place.

use super::*;

#[test]
fn the_client_directive_sets_the_module_environment() {
    let graph = graph_with(&[
        ("app/Counter.js", CLIENT_COMPONENT),
        ("app/util.js", SERVER_HELPER),
        (
            "app/actions.js",
            "\"use server\";\n// @flow\nexport async function save() {}\n",
        ),
    ]);

    assert_eq!(
        module(&graph, "app/Counter.js").environment(),
        ModuleEnvironment::Client
    );
    assert_eq!(
        module(&graph, "app/util.js").environment(),
        ModuleEnvironment::Server
    );
    assert_eq!(
        module(&graph, "app/actions.js").environment(),
        ModuleEnvironment::ServerActions
    );
}

#[test]
fn a_directive_inside_a_comment_is_not_a_directive() {
    let graph = graph_with(&[(
        "app/page.js",
        "// @flow\n// \"use client\";\nexport function Page() {}\n",
    )]);

    assert_eq!(
        module(&graph, "app/page.js").environment(),
        ModuleEnvironment::Server
    );
}

#[test]
fn a_module_of_type_exports_has_an_erased_surface() {
    let graph = graph_with(&[("app/types.js", TYPES_ONLY)]);
    let types = module(&graph, "app/types.js");

    assert_eq!(types.surface(), ModuleSurface::Erased);
    assert!(!types.surface().is_observable());
    assert!(!types.surface().accepts_update());
}

#[test]
fn a_component_module_accepts_updates() {
    let graph = graph_with(&[("app/Counter.js", CLIENT_COMPONENT)]);
    let counter = module(&graph, "app/Counter.js");

    assert_eq!(counter.surface(), ModuleSurface::Component);
    assert!(counter.surface().accepts_update());
}

#[test]
fn a_lowercase_export_is_not_a_component() {
    let graph = graph_with(&[("app/util.js", SERVER_HELPER)]);

    assert_eq!(
        module(&graph, "app/util.js").surface(),
        ModuleSurface::Opaque
    );
}

#[test]
fn a_constant_export_is_not_a_component() {
    let graph = graph_with(&[("app/limits.js", CLIENT_CONSTANT)]);

    assert_eq!(
        module(&graph, "app/limits.js").surface(),
        ModuleSurface::Opaque
    );
}

#[test]
fn one_non_component_export_makes_the_whole_module_opaque() {
    let graph = graph_with(&[(
        "app/Counter.js",
        "\"use client\";\n// @flow\nexport function Counter() {}\nexport const LIMIT = 3;\n",
    )]);

    assert_eq!(
        module(&graph, "app/Counter.js").surface(),
        ModuleSurface::Opaque
    );
}

#[test]
fn a_default_export_is_judged_by_the_file_stem() {
    let graph = graph_with(&[
        (
            "app/Counter.js",
            "\"use client\";\n// @flow\nexport default function () { return null; }\n",
        ),
        (
            "app/helpers.js",
            "// @flow\nexport default function () { return 1; }\n",
        ),
    ]);

    assert_eq!(
        module(&graph, "app/Counter.js").surface(),
        ModuleSurface::Component
    );
    assert_eq!(
        module(&graph, "app/helpers.js").surface(),
        ModuleSurface::Opaque
    );
}

#[test]
fn a_class_export_named_in_pascal_case_is_a_component() {
    let graph = graph_with(&[(
        "app/Boundary.js",
        "\"use client\";\n// @flow\nexport class Boundary {}\n",
    )]);

    assert_eq!(
        module(&graph, "app/Boundary.js").surface(),
        ModuleSurface::Component
    );
}

#[test]
fn an_async_export_is_not_a_fast_refresh_component() {
    let graph = graph_with(&[(
        "app/Page.js",
        "// @flow\nexport async function Page() { return null; }\n",
    )]);

    assert_eq!(
        module(&graph, "app/Page.js").surface(),
        ModuleSurface::Opaque
    );
}

#[test]
fn an_empty_source_is_an_erased_module_rather_than_an_error() {
    let mut graph = DevGraph::new();
    graph.insert("app/empty.js", "").expect("inserts");

    assert_eq!(
        module(&graph, "app/empty.js").surface(),
        ModuleSurface::Erased
    );
}

#[test]
fn a_crlf_source_scans_the_same_as_an_lf_one() {
    let lf = graph_with(&[(
        "app/page.js",
        "// @flow\nimport a from \"./a.js\";\nexport function Page() {}\n",
    )]);
    let crlf = graph_with(&[(
        "app/page.js",
        "// @flow\r\nimport a from \"./a.js\";\r\nexport function Page() {}\r\n",
    )]);

    assert_eq!(
        module(&lf, "app/page.js").surface(),
        module(&crlf, "app/page.js").surface()
    );
    assert_eq!(
        module(&lf, "app/page.js").imports().len(),
        module(&crlf, "app/page.js").imports().len()
    );
}

#[test]
fn a_bom_and_a_hashbang_do_not_hide_the_client_directive() {
    let graph = graph_with(&[(
        "app/Counter.js",
        "\u{feff}#!/usr/bin/env node\n\"use client\";\nexport function Counter() {}\n",
    )]);

    assert_eq!(
        module(&graph, "app/Counter.js").environment(),
        ModuleEnvironment::Client
    );
}

#[test]
fn module_surface_names_are_stable() {
    assert_eq!(ModuleSurface::Erased.as_str(), "erased");
    assert_eq!(ModuleSurface::Component.as_str(), "component");
    assert_eq!(ModuleSurface::Opaque.as_str(), "opaque");
}
