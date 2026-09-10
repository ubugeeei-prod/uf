//! The four things that need a server, and the one project that needs none.

use camino::Utf8PathBuf;
use compact_str::CompactString;
use uf_router::{RouteParam, RouteParamKind};

use super::*;

fn root() -> Utf8PathBuf {
    Utf8PathBuf::from("/src/app")
}

fn page(path: &str, params: &[&str]) -> Route {
    let directory = root().join(format!("app{path}"));
    Route {
        path: CompactString::from(path),
        page: directory.join("$page.js"),
        directory,
        params: params
            .iter()
            .map(|name| RouteParam {
                name: CompactString::from(*name),
                kind: RouteParamKind::Single,
            })
            .collect(),
        has_layout: false,
        middleware: Vec::new(),
    }
}

fn rendered(url: &str) -> Prerendered {
    Prerendered {
        url: url.to_owned(),
        file: format!("dist{url}/index.html"),
        status: 200,
    }
}

/// A site whose every route has a document: the case the target exists for.
///
/// uf's own documentation is this project — 37 pages, no handler, no
/// middleware, no parameter — and it is what
/// `the_static_adapter_writes_the_site_and_nothing_else` builds.
#[test]
fn a_site_that_is_only_documents_is_servable() {
    let routes = [page("/", &[]), page("/guide", &[])];
    let pages = [rendered("/"), rendered("/guide")];

    assert!(unservable(&root(), &routes, &[], &pages, &[]).is_empty());
}

/// A route handler is in no `Route`, so the route table alone would have
/// called this project static.
#[test]
fn a_route_handler_is_named_with_its_file() {
    let modules = [ServerModule {
        path: CompactString::from("/api/health"),
        file: root().join("app/api/health/$route.js"),
        kind: ServerModuleKind::RouteHandler,
    }];

    let found = unservable(&root(), &[page("/", &[])], &modules, &[rendered("/")], &[]);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].subject, "/api/health");
    assert_eq!(found[0].file, "app/api/health/$route.js");
    assert_eq!(found[0].reason, Reason::RouteHandler);
}

/// The parameterised route with nothing to enumerate it, which is the finding
/// this target exists for: today that route builds, uploads and 404s.
#[test]
fn a_parameterised_route_with_no_document_is_named_as_one() {
    let routes = [page("/", &[]), page("/posts/:slug", &["slug"])];

    let found = unservable(&root(), &routes, &[], &[rendered("/")], &[]);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].subject, "/posts/:slug");
    assert_eq!(found[0].reason, Reason::NoStaticParams);
    assert!(
        found[0].reason.because().contains("generateStaticParams"),
        "the reader has to be told what would fix it"
    );
}

/// One document is enough. A `generateStaticParams` that named three slugs
/// wrote three files and the route is served for those three; whether it named
/// every slug is a question about the project's data that nothing here can
/// ask, and refusing on it would refuse every blog uf can actually host.
#[test]
fn a_parameterised_route_the_prerender_reached_is_servable() {
    let routes = [page("/posts/:slug", &["slug"])];
    let pages = [rendered("/posts/hello-world")];

    assert!(unservable(&root(), &routes, &[], &pages, &[]).is_empty());
}

/// A middleware guarding a subtree with no page of its own: the second finding
/// the route table cannot report, because `Route::middleware` is only ever
/// populated from a route that exists.
#[test]
fn a_middleware_over_an_empty_subtree_is_still_a_refusal() {
    let modules = [ServerModule {
        path: CompactString::from("/dashboard"),
        file: root().join("app/dashboard/$middleware.js"),
        kind: ServerModuleKind::Middleware,
    }];

    let found = unservable(&root(), &[page("/", &[])], &modules, &[rendered("/")], &[]);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].reason, Reason::Middleware);
}

/// An action is named by its export, because that is the name in the source a
/// reader has to go and look at — the URL it is dialled at is the page's.
#[test]
fn a_server_action_is_named_by_its_export() {
    let actions = [(
        String::from("increment"),
        root().join("app/counter/actions.js"),
    )];

    let found = unservable(&root(), &[page("/", &[])], &[], &[rendered("/")], &actions);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].subject, "increment");
    assert_eq!(found[0].file, "app/counter/actions.js");
    assert_eq!(found[0].reason, Reason::ServerAction);
}

/// The message says what, where, why, and what to do instead — the last of
/// which is the half a reader who chose this target on purpose actually needs.
#[test]
fn the_refusal_names_every_finding_and_an_alternative() {
    let modules = [ServerModule {
        path: CompactString::from("/api/health"),
        file: root().join("app/api/health/$route.js"),
        kind: ServerModuleKind::RouteHandler,
    }];
    let routes = [page("/", &[]), page("/posts/:slug", &["slug"])];

    let message = refusal(&unservable(
        &root(),
        &routes,
        &modules,
        &[rendered("/")],
        &[],
    ));

    assert!(message.contains("/api/health"), "{message}");
    assert!(
        message.contains("app/api/health/$route.js"),
        "the file, not just the URL: {message}"
    );
    assert!(message.contains("/posts/:slug"), "{message}");
    assert!(message.contains("--adapter node"), "{message}");
    assert!(message.contains("issues/335"), "{message}");
}

/// A project with a hundred parameterised routes is a project with one
/// problem, and a message that printed all hundred would be read by nobody.
#[test]
fn a_long_list_is_cut_off_and_says_how_much_was_cut() {
    let routes: Vec<Route> = (0..25)
        .map(|index| page(&format!("/posts{index}/:slug"), &["slug"]))
        .collect();

    let message = refusal(&unservable(&root(), &routes, &[], &[], &[]));

    assert!(message.contains("… and 15 more"), "{message}");
    assert!(
        !message.contains("/posts20/:slug"),
        "the cut-off has to actually cut: {message}"
    );
}
