//! Attribution, chains and sizes over graphs shaped like the ones
//! `@uniflowed/vite` writes, and the view that carries them.

use super::*;

const TINY: &str = "node_modules/tiny-plot/index.js";

fn module(id: &str, imports: &[&str], dynamic: &[&str]) -> GraphModule {
    GraphModule {
        id: id.to_owned(),
        imports: imports.iter().map(|id| (*id).to_owned()).collect(),
        dynamic_imports: dynamic.iter().map(|id| (*id).to_owned()).collect(),
    }
}

/// A chunk holding `modules`, each with code that compresses to something.
fn chunk(file: &str, facade: Option<&str>, modules: &[&str]) -> GraphChunk {
    GraphChunk {
        file: file.into(),
        facade: facade.map(str::to_owned),
        modules: modules
            .iter()
            .map(|id| ChunkModule {
                id: (*id).to_owned(),
                code: format!("export const source = {:?};\n", id.repeat(8)),
            })
            .collect(),
    }
}

fn route(path: &str, page: &str) -> RouteFiles {
    RouteFiles {
        path: path.into(),
        page: page.to_owned(),
        directory: page
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory)
            .to_owned(),
    }
}

fn routes() -> Vec<RouteFiles> {
    vec![
        route("/", "app/$page.js"),
        route("/chart", "app/chart/$page.js"),
    ]
}

/// An application without server components: every page is a client module
/// the route table imports lazily.
fn client_build() -> GraphBuild {
    GraphBuild {
        environment: "client".into(),
        entries: vec!["virtual:uf/client".to_owned()],
        modules: vec![
            module(
                "virtual:uf/client",
                &["virtual:uf/routes", "node_modules/react-dom/index.js"],
                &[],
            ),
            module(
                "virtual:uf/routes",
                &[],
                &["app/$layout.js", "app/$page.js", "app/chart/$page.js"],
            ),
            module("app/$layout.js", &[], &[]),
            module("app/$page.js", &["app/format.js"], &[]),
            module("app/format.js", &[], &[]),
            module("app/chart/$page.js", &["app/chart/Plot.js"], &[]),
            module("app/chart/Plot.js", &[TINY], &[]),
            module(TINY, &[], &[]),
            module("node_modules/react-dom/index.js", &[], &[]),
        ],
        chunks: vec![
            chunk(
                "assets/client.js",
                Some("virtual:uf/client"),
                &[
                    "virtual:uf/client",
                    "virtual:uf/routes",
                    "node_modules/react-dom/index.js",
                ],
            ),
            chunk("assets/layout.js", None, &["app/$layout.js"]),
            chunk("assets/home.js", None, &["app/$page.js", "app/format.js"]),
            chunk(
                "assets/chart.js",
                None,
                &["app/chart/$page.js", "app/chart/Plot.js", TINY],
            ),
        ],
    }
}

fn graph(builds: Vec<GraphBuild>) -> ModuleGraph {
    ModuleGraph {
        version: MODULE_GRAPH_VERSION,
        builds,
    }
}

fn find<'a>(analysis: &'a BundleAnalysis, path: &str) -> &'a RouteAnalysis {
    analysis
        .routes
        .iter()
        .find(|route| route.path == path)
        .unwrap_or_else(|| panic!("no {path}"))
}

fn listed<'a>(list: &'a ModuleList, id: &str) -> Option<&'a ModuleEntry> {
    list.modules.iter().find(|entry| entry.id == id)
}

#[test]
fn a_dependency_one_route_pulls_in_is_listed_for_that_route_with_its_chain() {
    let analysis = analyze(&graph(vec![client_build()]), &routes()).expect("analyzed");

    let chart = find(&analysis, "/chart");
    let tiny = listed(&chart.client, TINY).expect("tiny-plot is /chart's");
    assert_eq!(
        tiny.chain,
        ["app/chart/$page.js", "app/chart/Plot.js", TINY]
    );
    assert_eq!(tiny.routes, 1);
    assert_eq!(tiny.chunk, "assets/chart.js");
    assert!(!tiny.dynamic);

    let home = find(&analysis, "/");
    assert!(listed(&home.client, TINY).is_none(), "{home:#?}");
    assert!(
        listed(&home.client, "app/chart/$page.js").is_none(),
        "another route's page is never entered"
    );
    assert_eq!(
        listed(&home.client, "app/format.js").map(|entry| entry.chain.clone()),
        Some(vec!["app/$page.js".to_owned(), "app/format.js".to_owned()])
    );
    assert_eq!(chart.files, ["app/chart/$page.js", "app/$layout.js"]);
}

#[test]
fn what_every_page_loads_is_listed_once_under_shared() {
    let analysis = analyze(&graph(vec![client_build()]), &routes()).expect("analyzed");

    let react = listed(&analysis.shared.client, "node_modules/react-dom/index.js")
        .expect("react-dom is shared");
    assert_eq!(
        react.chain,
        ["virtual:uf/client", "node_modules/react-dom/index.js"]
    );
    assert_eq!(react.routes, 2);
    for route in &analysis.routes {
        assert!(
            listed(&route.client, "node_modules/react-dom/index.js").is_none(),
            "{} repeats a shared module",
            route.path
        );
    }
    assert!(
        listed(&analysis.shared.client, "app/$page.js").is_none(),
        "a page is not what every route loads"
    );
}

#[test]
fn a_layout_belongs_to_every_route_below_it_and_to_no_other() {
    let mut build = client_build();
    build.modules.push(module("app/chart/$layout.js", &[], &[]));
    build.chunks.push(chunk(
        "assets/chart-layout.js",
        None,
        &["app/chart/$layout.js"],
    ));
    build.modules[1]
        .dynamic_imports
        .push("app/chart/$layout.js".to_owned());

    let analysis = analyze(&graph(vec![build]), &routes()).expect("analyzed");

    for path in ["/", "/chart"] {
        let root_layout = listed(&find(&analysis, path).client, "app/$layout.js")
            .unwrap_or_else(|| panic!("the root layout is {path}'s"));
        assert_eq!(root_layout.routes, 2);
    }
    assert!(listed(&find(&analysis, "/chart").client, "app/chart/$layout.js").is_some());
    assert!(listed(&find(&analysis, "/").client, "app/chart/$layout.js").is_none());
}

/// The server component renders `Plot`, which is an entry of the client build
/// by reference: the browser loads it for `/chart` and for no other route, and
/// the chain crosses from the server page into it.
#[test]
fn a_client_reference_is_attributed_to_the_route_whose_server_component_renders_it() {
    let rsc = GraphBuild {
        environment: "rsc".into(),
        entries: vec!["virtual:uf/rsc".to_owned()],
        modules: vec![
            module("virtual:uf/rsc", &["virtual:uf/routes"], &[]),
            module(
                "virtual:uf/routes",
                &[],
                &["app/$page.js", "app/chart/$page.js"],
            ),
            module("app/$page.js", &[], &[]),
            module("app/chart/$page.js", &["app/chart/Plot.js"], &[]),
            module("app/chart/Plot.js", &[], &[]),
        ],
        chunks: vec![
            chunk(
                "rsc.js",
                Some("virtual:uf/rsc"),
                &["virtual:uf/rsc", "virtual:uf/routes"],
            ),
            chunk("home.js", None, &["app/$page.js"]),
            chunk(
                "chart.js",
                None,
                &["app/chart/$page.js", "app/chart/Plot.js"],
            ),
        ],
    };
    let client = GraphBuild {
        environment: "client".into(),
        entries: vec!["virtual:uf/client".to_owned()],
        modules: vec![
            module("virtual:uf/client", &[], &[]),
            module("app/chart/Plot.js", &[TINY], &[]),
            module(TINY, &[], &[]),
        ],
        chunks: vec![
            chunk(
                "assets/client.js",
                Some("virtual:uf/client"),
                &["virtual:uf/client"],
            ),
            chunk(
                "assets/plot.js",
                Some("app/chart/Plot.js"),
                &["app/chart/Plot.js", TINY],
            ),
        ],
    };

    // The client build first, to show the order in the graph is not the order
    // the walks need.
    let analysis = analyze(&graph(vec![client, rsc]), &routes()).expect("analyzed");

    let chart = find(&analysis, "/chart");
    assert_eq!(
        listed(&chart.client, TINY).map(|entry| entry.chain.clone()),
        Some(vec![
            "app/chart/$page.js".to_owned(),
            "app/chart/Plot.js".to_owned(),
            TINY.to_owned(),
        ])
    );
    assert_eq!(
        listed(&chart.client, "app/chart/Plot.js").map(|entry| entry.chunk.as_str()),
        Some("assets/plot.js")
    );
    assert_eq!(
        listed(&chart.server, "app/chart/$page.js").map(|entry| entry.build.as_str()),
        Some("rsc")
    );
    let home = find(&analysis, "/");
    assert!(listed(&home.client, TINY).is_none(), "{home:#?}");
    assert!(
        listed(&home.client, "app/chart/Plot.js").is_none(),
        "{home:#?}"
    );
    assert!(listed(&analysis.shared.client, "app/chart/Plot.js").is_none());
}

#[test]
fn a_module_with_no_code_is_walked_through_and_not_listed() {
    let mut build = client_build();
    // `Plot` now comes through a re-export the bundler left no code for.
    build.modules[5] = module("app/chart/$page.js", &["app/chart/index.js"], &[]);
    build
        .modules
        .push(module("app/chart/index.js", &["app/chart/Plot.js"], &[]));

    let analysis = analyze(&graph(vec![build]), &routes()).expect("analyzed");

    let chart = find(&analysis, "/chart");
    assert!(listed(&chart.client, "app/chart/index.js").is_none());
    assert_eq!(
        listed(&chart.client, TINY).map(|entry| entry.chain.len()),
        Some(4),
        "the chain still names the module it went through"
    );
}

#[test]
fn a_chain_through_an_import_call_says_so() {
    let mut build = client_build();
    build.modules[6] = module("app/chart/Plot.js", &[], &[TINY]);

    let analysis = analyze(&graph(vec![build]), &routes()).expect("analyzed");

    let chart = find(&analysis, "/chart");
    assert!(listed(&chart.client, TINY).is_some_and(|entry| entry.dynamic));
    assert!(listed(&chart.client, "app/chart/Plot.js").is_some_and(|entry| !entry.dynamic));
}

#[test]
fn a_route_costs_what_it_adds_and_what_is_shared() {
    let analysis = analyze(&graph(vec![client_build()]), &routes()).expect("analyzed");

    let chart = find(&analysis, "/chart");
    assert!(
        chart
            .client
            .modules
            .iter()
            .all(|entry| entry.size.gzip.bytes() > 0)
    );
    assert_eq!(
        chart.total.client,
        analysis
            .shared
            .client
            .size
            .saturating_add(chart.client.size)
    );
    let client = &analysis.builds[0];
    assert_eq!(
        (client.name.as_str(), client.side),
        ("client", Side::Client)
    );
    assert_eq!(client.modules, 9, "every module in a chunk has code");
}

#[test]
fn a_graph_of_another_version_is_refused() {
    let mut refused = graph(vec![client_build()]);
    refused.version = MODULE_GRAPH_VERSION + 1;

    assert!(matches!(
        analyze(&refused, &routes()),
        Err(AnalysisError::Version { .. })
    ));
}

#[test]
fn the_view_carries_the_analysis_and_nothing_in_it_can_close_the_script() {
    let json = r#"{"routes":[{"path":"/</script><script>alert(1)</script>"}]}"#;

    let view = render_view(json);

    assert!(!view.contains("</script><script>alert(1)"), "{view}");
    // Spelled with the character code, so the expectation is the escaped text
    // and not something an editor or a tool turned back into `<`.
    let backslash = char::from(92);
    assert!(view.contains(&format!(
        "/{backslash}u003c/script{backslash}u003e{backslash}u003cscript{backslash}u003ealert(1)"
    )));
    assert!(!view.contains(VIEW_DATA), "the placeholder was left in");
}

/// The client entry imports a registry that `import()`s every client
/// reference, which is how the browser finds a component it was sent by URL.
/// Reaching a reference that way says nothing about which route renders it, so
/// the shared walk must not take it — and a second server bundle, rendering
/// the same component to HTML, gets the attribution the browser gets.
#[test]
fn a_reference_registry_in_the_entry_does_not_make_every_reference_shared() {
    let rsc = GraphBuild {
        environment: "rsc".into(),
        entries: vec!["virtual:uf/rsc".to_owned()],
        modules: vec![
            module(
                "virtual:uf/rsc",
                &[],
                &["app/$page.js", "app/chart/$page.js"],
            ),
            module("app/$page.js", &[], &[]),
            module("app/chart/$page.js", &["app/chart/Plot.js"], &[]),
            module("app/chart/Plot.js", &[], &[]),
        ],
        chunks: vec![chunk(
            "rsc.js",
            Some("virtual:uf/rsc"),
            &[
                "virtual:uf/rsc",
                "app/$page.js",
                "app/chart/$page.js",
                "app/chart/Plot.js",
            ],
        )],
    };
    let registry = |environment: &str, entry: &str| GraphBuild {
        environment: environment.into(),
        entries: vec![entry.to_owned()],
        modules: vec![
            module(entry, &["virtual:uf/client-references"], &[]),
            module("virtual:uf/client-references", &[], &["app/chart/Plot.js"]),
            module("app/chart/Plot.js", &[TINY], &[]),
            module(TINY, &[], &[]),
        ],
        chunks: vec![
            chunk(
                "entry.js",
                Some(entry),
                &[entry, "virtual:uf/client-references"],
            ),
            chunk(
                "plot.js",
                (environment == "client").then_some("app/chart/Plot.js"),
                &["app/chart/Plot.js", TINY],
            ),
        ],
    };

    let analysis = analyze(
        &graph(vec![
            registry("client", "virtual:uf/client"),
            rsc,
            registry("ssr", "virtual:uf/server"),
        ]),
        &routes(),
    )
    .expect("analyzed");

    let expected = vec![
        "app/chart/$page.js".to_owned(),
        "app/chart/Plot.js".to_owned(),
        TINY.to_owned(),
    ];
    let chart = find(&analysis, "/chart");
    assert_eq!(
        listed(&chart.client, TINY).map(|entry| entry.chain.clone()),
        Some(expected.clone())
    );
    let rendered = chart
        .server
        .modules
        .iter()
        .find(|entry| entry.id == TINY && entry.build == "ssr");
    assert_eq!(rendered.map(|entry| entry.chain.clone()), Some(expected));
    for side in [&analysis.shared.client, &analysis.shared.server] {
        assert!(listed(side, TINY).is_none(), "{side:#?}");
        assert!(listed(side, "app/chart/Plot.js").is_none(), "{side:#?}");
    }
    let home = find(&analysis, "/");
    assert!(listed(&home.client, TINY).is_none(), "{home:#?}");
    assert!(listed(&home.server, TINY).is_none(), "{home:#?}");
}
