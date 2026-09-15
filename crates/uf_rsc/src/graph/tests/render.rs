//! What a route's render reaches: a read of the request, and a stated lifetime.
//! See ubugeeei-prod/uf#996.

use super::*;

/// The ids of `paths`, in order.
fn ids(graph: &RscGraph, paths: &[&str]) -> Vec<ModuleId> {
    paths
        .iter()
        .map(|path| graph.module_id(path).expect("the module is in the graph"))
        .collect()
}

/// A reach's chain, as paths.
fn chain(graph: &RscGraph, reach: &RenderReach) -> Vec<String> {
    reach
        .chain
        .iter()
        .map(|id| {
            graph
                .module_by_id(*id)
                .expect("a chain names modules")
                .path
                .to_string()
        })
        .collect()
}

#[test]
fn a_read_of_the_request_is_found_through_the_chain_that_reaches_it() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/account/$page.js",
        "// @flow\nimport { Greeting } from \"./Greeting.js\";\n",
    );
    builder.add_source(
        "app/account/Greeting.js",
        "// @flow\nimport { session } from \"../session.js\";\n",
    );
    builder.add_source(
        "app/session.js",
        "// @flow\nimport { cookies } from \"@uniflowed/server\";\n",
    );
    let graph = builder.build();

    let read = graph
        .request_state_read(&ids(&graph, &["app/account/$page.js"]))
        .expect("the read is reached");
    assert_eq!(read.site.name, "cookies");
    assert_eq!(read.site.line, 2);
    assert_eq!(
        chain(&graph, &read),
        [
            "app/account/$page.js",
            "app/account/Greeting.js",
            "app/session.js"
        ]
    );
}

#[test]
fn the_nearest_read_is_named_and_a_layout_renders_the_route_too() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/$layout.js",
        "// @flow\nimport { headers } from \"@uniflowed/server\";\n",
    );
    builder.add_source(
        "app/posts/$page.js",
        "// @flow\nimport { a } from \"./a.js\";\n",
    );
    builder.add_source(
        "app/posts/a.js",
        "// @flow\nimport { b } from \"./b.js\";\n",
    );
    builder.add_source(
        "app/posts/b.js",
        "// @flow\nimport { cookies } from \"@uniflowed/server\";\n",
    );
    let graph = builder.build();

    let read = graph
        .request_state_read(&ids(&graph, &["app/$layout.js", "app/posts/$page.js"]))
        .expect("the layout's read is reached");
    assert_eq!(read.site.name, "headers");
    assert_eq!(chain(&graph, &read), ["app/$layout.js"]);

    let page_alone = graph
        .request_state_read(&ids(&graph, &["app/posts/$page.js"]))
        .expect("the page's own read is reached");
    assert_eq!(page_alone.site.name, "cookies");
    assert_eq!(
        chain(&graph, &page_alone),
        ["app/posts/$page.js", "app/posts/a.js", "app/posts/b.js"]
    );
}

#[test]
fn a_client_component_or_a_server_action_does_not_make_a_render_read_the_request() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/$page.js",
        "// @flow\nimport { Counter } from \"./Counter.js\";\nimport { signOut } from \"./actions.js\";\n",
    );
    builder.add_source(
        "app/Counter.js",
        "\"use client\";\nimport { label } from \"./label.js\";\n",
    );
    builder.add_source(
        "app/label.js",
        "// @flow\nimport { cookies } from \"@uniflowed/server\";\n",
    );
    builder.add_source(
        "app/actions.js",
        "\"use server\";\nimport { cookies } from \"@uniflowed/server\";\n",
    );
    let graph = builder.build();

    assert_eq!(
        graph.request_state_read(&ids(&graph, &["app/$page.js"])),
        None
    );
}

#[test]
fn a_namespace_import_or_another_export_of_the_server_package_is_not_a_read() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/$page.js",
        "// @flow\nimport * as server from \"@uniflowed/server\";\nimport { redirect } from \"@uniflowed/server\";\n",
    );
    let graph = builder.build();

    assert_eq!(
        graph.request_state_read(&ids(&graph, &["app/$page.js"])),
        None
    );
}

#[test]
fn a_cache_lifetime_is_found_where_cache_life_is_imported_and_a_tag_alone_is_not_one() {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/posts/$page.js",
        "// @flow\nimport * as React from \"@uniflowed/react\";\nimport { cacheLife, cacheTag } from \"@uniflowed/server/cache\";\n",
    );
    builder.add_source(
        "app/tags/$page.js",
        "// @flow\nimport { cacheTag } from \"@uniflowed/server/cache\";\n",
    );
    let graph = builder.build();

    let stated = graph
        .cache_lifetime_statement(&ids(&graph, &["app/posts/$page.js"]))
        .expect("the lifetime is found");
    assert_eq!(stated.site.name, "cacheLife");
    assert_eq!(stated.site.line, 3);
    assert_eq!(chain(&graph, &stated), ["app/posts/$page.js"]);
    assert_eq!(
        graph.cache_lifetime_statement(&ids(&graph, &["app/tags/$page.js"])),
        None
    );
}

#[test]
fn the_diagnostic_names_the_routes_the_read_and_the_chain() {
    let prerendered = RscDiagnostic::RequestStateInStaticRoute {
        routes: vec!["/account".into(), "/settings".into()],
        reason: StaticRouteReason::Prerendered,
        api: "cookies".into(),
        module: "app/session.js".into(),
        line: 2,
        chain: vec!["app/$layout.js".into(), "app/session.js".into()],
    };
    let message = prerendered.to_string();
    for expected in [
        "routes `/account`, `/settings` are prerendered",
        "reads `cookies()` through `app/$layout.js` → `app/session.js`",
        "`app/session.js` imports it from `@uniflowed/server` at line 2",
        "force-dynamic",
    ] {
        assert!(
            message.contains(expected),
            "missing {expected:?} in {message}"
        );
    }
    assert_eq!(prerendered.rule(), "rsc/request-state-in-static-route");
    assert_eq!(prerendered.line(), 2);
    assert_eq!(prerendered.module(), "app/session.js");
    assert_eq!(prerendered.severity(), RscSeverity::Error);

    let cached = RscDiagnostic::RequestStateInStaticRoute {
        routes: vec!["/posts/:slug".into()],
        reason: StaticRouteReason::Cached {
            module: "app/posts/[slug]/$page.js".into(),
            line: 3,
        },
        api: "headers".into(),
        module: "app/posts/[slug]/$page.js".into(),
        line: 2,
        chain: vec!["app/posts/[slug]/$page.js".into()],
    };
    let message = cached.to_string();
    for expected in [
        "route `/posts/:slug` states a cache lifetime through `cacheLife`",
        "which `app/posts/[slug]/$page.js` imports at line 3",
        "reads `headers()`, which `app/posts/[slug]/$page.js` imports from `@uniflowed/server` at \
         line 2",
        "never stored",
    ] {
        assert!(
            message.contains(expected),
            "missing {expected:?} in {message}"
        );
    }
}

#[test]
fn an_exported_string_is_read_past_a_flow_annotation_and_nothing_else_is() {
    use crate::scan::scan_exported_string;

    assert_eq!(
        scan_exported_string("export const dynamic = \"force-dynamic\";\n", "dynamic").as_deref(),
        Some("force-dynamic")
    );
    assert_eq!(
        scan_exported_string("export const dynamic: string = 'auto';\n", "dynamic").as_deref(),
        Some("auto")
    );
    assert_eq!(
        scan_exported_string("const dynamic = \"force-dynamic\";\n", "dynamic"),
        None
    );
    assert_eq!(
        scan_exported_string("export const dynamic = pick();\n", "dynamic"),
        None
    );
    assert_eq!(
        scan_exported_string("export const other = \"force-dynamic\";\n", "dynamic"),
        None
    );
}
