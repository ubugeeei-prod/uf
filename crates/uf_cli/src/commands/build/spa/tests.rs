//! What a single-page build refuses, and — the half that matters more — what
//! it does not.
//!
//! A refusal that fires on a project it should have built is worse than no
//! refusal at all: it is a rule people learn to work around. So each case that
//! is refused has a partner here that is not.

use camino::Utf8PathBuf;
use compact_str::CompactString;
use uf_router::{Route, ServerModule, ServerModuleKind};
use uf_rsc::{EntryKind, ModuleEnvironment, RscGraph, RscGraphBuilder, RscModuleInput};

use super::{Unanswerable, refuse, unanswerable};

const ROOT: &str = "/project";

fn route(path: &str, directory: &str) -> Route {
    Route {
        path: path.into(),
        directory: Utf8PathBuf::from(format!("{ROOT}/{directory}")),
        page: Utf8PathBuf::from(format!("{ROOT}/{directory}/$page.js")),
        params: Vec::new(),
        has_layout: false,
        middleware: Vec::new(),
    }
}

fn server_module(path: &str, file: &str, kind: ServerModuleKind) -> ServerModule {
    ServerModule {
        path: CompactString::from(path),
        file: Utf8PathBuf::from(format!("{ROOT}/{file}")),
        kind,
    }
}

fn module(path: &str) -> RscModuleInput {
    RscModuleInput::new(path, ModuleEnvironment::Server)
}

fn found(routes: &[Route], modules: &[ServerModule], graph: &RscGraph) -> Vec<Unanswerable> {
    unanswerable(ROOT.into(), routes, modules, graph)
}

/// A project whose home page reads the request, through one helper.
///
/// Through a helper rather than directly, because that is the case a check on
/// the page's own imports would miss and the one this is for: `cookies()` is
/// nearly always called from a module the page imports rather than from the
/// page.
fn reads_the_request() -> RscGraph {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(module("app/$page.js").with_import("../session.js"));
    builder.add_module(module("session.js").with_import("@uniflowed/server"));
    builder.add_entry("app/$page.js", EntryKind::Server);
    builder.build()
}

#[test]
fn a_route_whose_loader_reads_the_request_is_named() {
    let graph = reads_the_request();
    let findings = found(&[route("/", "app")], &[], &graph);

    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].route.as_deref(), Some("/"));
    assert_eq!(findings[0].file, "session.js");
    assert!(
        findings[0].because.contains("@uniflowed/server"),
        "{}",
        findings[0].because
    );
}

#[test]
fn the_refusal_names_the_route_and_quotes_the_setting() {
    // The requirement in one assertion: found at build time, and said in words
    // that identify what to go and look at.
    let graph = reads_the_request();
    let error = refuse(
        ROOT.into(),
        &[route("/", "app")],
        &[],
        &graph,
        "`app.rendering.modes` is `[\"csr\"]`",
    )
    .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("app.rendering.modes"), "{message}");
    assert!(message.contains("session.js"), "{message}");
    assert!(message.contains("ssr"), "{message}");
}

#[test]
fn a_project_that_reads_no_request_builds() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(module("app/$page.js").with_import("../format.js"));
    builder.add_module(module("format.js"));
    builder.add_entry("app/$page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(found(&[route("/", "app")], &[], &graph), Vec::new());
    refuse(ROOT.into(), &[route("/", "app")], &[], &graph, "because").unwrap();
}

#[test]
fn a_server_only_module_nothing_imports_is_not_a_refusal() {
    // Dead code. Refusing over a module that never runs is a refusal whose
    // only fix is deleting a file the project was not using, and the analysis
    // already knows the difference.
    let mut builder = RscGraphBuilder::new();
    builder.add_module(module("app/$page.js"));
    builder.add_module(module("old/session.js").with_import("@uniflowed/server"));
    builder.add_entry("app/$page.js", EntryKind::Server);
    let graph = builder.build();

    assert_eq!(found(&[route("/", "app")], &[], &graph), Vec::new());
}

#[test]
fn a_module_named_server_js_counts_too() {
    // The other half of `uf_rsc`'s rule, and the one a specifier check misses:
    // `*.server.js` is server-only by name rather than by what it imports.
    let mut builder = RscGraphBuilder::new();
    builder.add_module(module("app/$page.js").with_import("../orders.server.js"));
    builder.add_module(module("orders.server.js"));
    builder.add_entry("app/$page.js", EntryKind::Server);
    let graph = builder.build();

    let findings = found(&[route("/", "app")], &[], &graph);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].file, "orders.server.js");
    assert!(
        findings[0].because.contains(".server.js"),
        "{}",
        findings[0].because
    );
}

#[test]
fn a_module_no_page_reaches_is_reported_by_its_file() {
    // A layout is the ordinary case: no page imports the layout it renders
    // inside, so the walk from the pages never arrives. Naming a route it does
    // not belong to would be a worse answer than naming none.
    let mut builder = RscGraphBuilder::new();
    builder.add_module(module("app/$page.js"));
    builder.add_module(module("app/$layout.js").with_import("@uniflowed/server"));
    builder.add_entry("app/$page.js", EntryKind::Server);
    builder.add_entry("app/$layout.js", EntryKind::Server);
    let graph = builder.build();

    let findings = found(&[route("/", "app")], &[], &graph);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].route, None);
    assert_eq!(findings[0].file, "app/$layout.js");
}

#[test]
fn a_route_handler_and_a_middleware_are_both_refused() {
    let mut builder = RscGraphBuilder::new();
    builder.add_module(module("app/$page.js"));
    builder.add_entry("app/$page.js", EntryKind::Server);
    let graph = builder.build();

    let findings = found(
        &[route("/", "app")],
        &[
            server_module("/api", "app/api/$route.js", ServerModuleKind::RouteHandler),
            server_module(
                "/dashboard",
                "app/dashboard/$middleware.js",
                ServerModuleKind::Middleware,
            ),
        ],
        &graph,
    );

    let subjects = findings
        .iter()
        .map(|finding| finding.route.clone().unwrap_or_default())
        .collect::<Vec<_>>();
    // The middleware by the subtree it guards, the way `deploy::static_host`
    // reports one: it runs for a page, for a handler, and for a path under it
    // that is neither.
    assert!(subjects.contains(&String::from("/api")), "{subjects:?}");
    assert!(
        subjects.contains(&String::from("/dashboard/*")),
        "{subjects:?}"
    );
}

#[test]
fn one_helper_two_routes_is_reported_for_both() {
    // Not deduplicated down to the module: the reader is deciding what to do
    // about an application, and "these two routes cannot be rendered in a
    // browser" is the shape of that decision.
    let mut builder = RscGraphBuilder::new();
    builder.add_module(module("app/$page.js").with_import("../session.js"));
    builder.add_module(module("app/orders/$page.js").with_import("../../session.js"));
    builder.add_module(module("session.js").with_import("@uniflowed/server"));
    builder.add_entry("app/$page.js", EntryKind::Server);
    builder.add_entry("app/orders/$page.js", EntryKind::Server);
    let graph = builder.build();

    let findings = found(
        &[route("/", "app"), route("/orders", "app/orders")],
        &[],
        &graph,
    );

    assert_eq!(findings.len(), 2, "{findings:?}");
    assert_eq!(findings[0].route.as_deref(), Some("/"));
    assert_eq!(findings[1].route.as_deref(), Some("/orders"));
}
