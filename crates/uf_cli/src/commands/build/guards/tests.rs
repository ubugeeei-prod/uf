//! Which prerendered documents a guard does not reach.

use camino::Utf8PathBuf;
use uf_router::{Route, RouteParam, RouteParamKind};

use super::{UnguardedPage, unguarded_pages};

const ROOT: &str = "/project";

fn route(path: &str, directory: &str, middleware: &[&str]) -> Route {
    Route {
        path: path.into(),
        directory: Utf8PathBuf::from(format!("{ROOT}/{directory}")),
        page: Utf8PathBuf::from(format!("{ROOT}/{directory}/_uf.page.js")),
        params: path
            .split('/')
            .filter_map(|segment| segment.strip_prefix(':'))
            .map(|name| RouteParam {
                name: name.trim_end_matches('*').into(),
                kind: if name.ends_with('*') {
                    RouteParamKind::CatchAll
                } else {
                    RouteParamKind::Single
                },
            })
            .collect(),
        has_layout: false,
        middleware: middleware
            .iter()
            .map(|dir| Utf8PathBuf::from(format!("{ROOT}/{dir}/_uf.middleware.js")))
            .collect(),
    }
}

fn page(url: &str, file: &str) -> (String, String) {
    (url.to_owned(), file.to_owned())
}

fn found(routes: &[Route], pages: &[(String, String)]) -> Vec<UnguardedPage> {
    unguarded_pages(ROOT.into(), routes, pages)
}

/// The case #342 is about: a guarded route that is also a static file.
#[test]
fn a_guarded_route_that_was_prerendered_is_named() {
    let routes = vec![
        route("/", "app", &[]),
        route("/dashboard", "app/dashboard", &["app/dashboard"]),
    ];
    let pages = vec![
        page("/", "dist/index.html"),
        page("/dashboard", "dist/dashboard/index.html"),
    ];

    let found = found(&routes, &pages);

    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].url, "/dashboard");
    assert_eq!(found[0].file, "dist/dashboard/index.html");
    assert_eq!(found[0].middleware, ["app/dashboard/_uf.middleware.js"]);
}

/// A route below the guard is the one nobody would think to check, and the
/// per-directory answer route discovery used to give would have missed it.
#[test]
fn an_inherited_guard_counts() {
    let routes = vec![route(
        "/dashboard/settings",
        "app/dashboard/settings",
        &["app/dashboard"],
    )];
    let pages = vec![page(
        "/dashboard/settings",
        "dist/dashboard/settings/index.html",
    )];

    let found = found(&routes, &pages);

    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].middleware, ["app/dashboard/_uf.middleware.js"]);
}

/// A route with parameters is prerendered only when it has
/// `generateStaticParams`, and then it is prerendered once per set — so the
/// files are named by filled-in URL and have to be matched back to the route
/// that declared the guard.
#[test]
fn every_document_a_generated_parameter_wrote_is_named() {
    let routes = vec![route("/reports/:id", "app/reports/[id]", &["app/reports"])];
    let pages = vec![
        page("/reports/q1", "dist/reports/q1/index.html"),
        page("/reports/q2", "dist/reports/q2/index.html"),
    ];

    let found = found(&routes, &pages);

    assert_eq!(
        found
            .iter()
            .map(|page| page.url.as_str())
            .collect::<Vec<_>>(),
        ["/reports/q1", "/reports/q2"]
    );
}

/// Nothing is reported for a project with no middleware, and nothing is
/// reported for the `404.html` the prerender writes from a boundary rather
/// than from a route.
#[test]
fn an_unguarded_build_says_nothing() {
    let routes = vec![route("/", "app", &[]), route("/guide", "app/guide", &[])];
    let pages = vec![
        page("/", "dist/index.html"),
        page("/guide", "dist/guide/index.html"),
        page("/404", "dist/404.html"),
    ];

    assert!(found(&routes, &pages).is_empty());
}

/// Two routes can serve the same URL shape, and their guards are not the same
/// list. The one that answers a request is the more specific one, so it is the
/// one whose guards are reported.
#[test]
fn the_route_that_would_answer_is_the_one_reported() {
    let routes = vec![
        route("/posts/:slug", "app/posts/[slug]", &["app/posts"]),
        route(
            "/posts/new",
            "app/posts/new",
            &["app/posts", "app/posts/new"],
        ),
    ];
    let pages = vec![
        page("/posts/new", "dist/posts/new/index.html"),
        page("/posts/hello", "dist/posts/hello/index.html"),
    ];

    let found = found(&routes, &pages);

    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(found[0].url, "/posts/hello");
    assert_eq!(found[0].middleware, ["app/posts/_uf.middleware.js"]);
    assert_eq!(found[1].url, "/posts/new");
    assert_eq!(
        found[1].middleware,
        [
            "app/posts/_uf.middleware.js",
            "app/posts/new/_uf.middleware.js"
        ]
    );
}

/// And when the more specific route is the *unguarded* one, nothing is
/// reported for it.
///
/// A `(group)` segment is dropped from a route path and not from the directory
/// tree, so `app/(marketing)/posts/new/_uf.page.js` serves `/posts/new` and
/// inherits nothing from `app/posts/_uf.middleware.js`. Matching against the
/// guarded routes alone would have named a guard that does not apply to that
/// path — a warning about a document that is served exactly as intended, which
/// is how a report stops being read.
#[test]
fn a_more_specific_unguarded_route_is_not_reported() {
    let routes = vec![
        route("/posts/:slug", "app/posts/[slug]", &["app/posts"]),
        route("/posts/new", "app/(marketing)/posts/new", &[]),
    ];
    let pages = vec![
        page("/posts/new", "dist/posts/new/index.html"),
        page("/posts/hello", "dist/posts/hello/index.html"),
    ];

    let found = found(&routes, &pages);

    assert_eq!(
        found
            .iter()
            .map(|page| page.url.as_str())
            .collect::<Vec<_>>(),
        ["/posts/hello"],
        "{found:#?}"
    );
}

/// Two routes can have the same number of literal segments and still not be
/// equally specific. `/posts/archive/:z*` and `/posts/:a/:b/edit` both serve
/// `/posts/archive/foo/edit` and both have two literal segments; the router
/// answers with the parameter route, because a parameter outranks a catch-all.
/// Counting literals cannot see that, so it named the archive middleware for a
/// document the archive middleware never guards.
#[test]
fn a_parameter_outranks_a_catch_all_at_the_same_literal_count() {
    let routes = vec![
        route(
            "/posts/:a/:b/edit",
            "app/posts/[a]/[b]/edit",
            &["app/posts"],
        ),
        route(
            "/posts/archive/:z*",
            "app/posts/archive/[...z]",
            &["app/posts", "app/posts/archive"],
        ),
    ];
    let pages = vec![page(
        "/posts/archive/foo/edit",
        "dist/posts/archive/foo/edit/index.html",
    )];

    let found = found(&routes, &pages);

    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        found[0].middleware,
        ["app/posts/_uf.middleware.js"],
        "{found:#?}"
    );
}

/// And the same disagreement can lose the warning altogether: when the
/// catch-all is the unguarded route, picking it says the document is served
/// exactly as intended, and the guard on the route that actually answers goes
/// unmentioned.
#[test]
fn a_catch_all_does_not_hide_the_guard_on_the_route_that_answers() {
    let routes = vec![
        route(
            "/posts/:a/:b/edit",
            "app/posts/[a]/[b]/edit",
            &["app/posts/[a]"],
        ),
        route(
            "/posts/archive/:z*",
            "app/(legacy)/posts/archive/[...z]",
            &[],
        ),
    ];
    let pages = vec![page(
        "/posts/archive/foo/edit",
        "dist/posts/archive/foo/edit/index.html",
    )];

    let found = found(&routes, &pages);

    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].url, "/posts/archive/foo/edit");
    assert_eq!(found[0].middleware, ["app/posts/[a]/_uf.middleware.js"]);
}
