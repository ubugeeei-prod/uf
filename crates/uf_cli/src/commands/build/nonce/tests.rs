//! Which prerendered documents a nonce policy refuses, and which it does not.
//!
//! Every reported case here has a partner that is not reported: a check that
//! only ever fires is one nobody can act on, and the two ways out of this one
//! — scope the rule, or drop the nonce — are only advice if silence is
//! reachable by taking them.

use uf_config::HeaderRule;

use super::{NoncedPage, message, nonced_pages, source_matches};
use crate::commands::build::Prerendered;

/// The policy #1169 observed the failure under, verbatim.
const POLICY: &str = "script-src 'nonce-{uf.nonce}' 'strict-dynamic'; object-src 'none'";

const CONFIG: &str = "uf.config.js";

fn page(url: &str, file: &str) -> Prerendered {
    Prerendered {
        url: url.to_owned(),
        file: file.to_owned(),
        status: 200,
        regenerates: false,
        partial: false,
    }
}

fn rule(source: &str, headers: &[(&str, &str)]) -> HeaderRule {
    HeaderRule {
        source: source.into(),
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into()))
            .collect(),
    }
}

/// The case #1171 is about: a document on disk under a per-request nonce.
#[test]
fn a_prerendered_route_under_a_nonce_rule_is_named() {
    let pages = vec![
        page("/", "dist/index.html"),
        page("/guide", "dist/guide/index.html"),
    ];
    let rules = vec![rule("/:path*", &[("content-security-policy", POLICY)])];

    let found = nonced_pages(&pages, &rules);

    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(found[0].url, "/");
    assert_eq!(found[0].file, "dist/index.html");
    assert_eq!(found[0].source, "/:path*");
    assert_eq!(found[0].header, "content-security-policy");
    assert_eq!(found[1].url, "/guide");
}

/// The whole check, in one assertion.
///
/// `/:path*` is the canonical way to write a policy over a whole site, and the
/// document it covers that matters most is the root. A **rule's** catch-all
/// takes the rest of the path including none of it — `matchSegments` returns as
/// soon as it reaches one — while a **route's** catch-all needs a segment to
/// consume, which is why `/docs` is not `/docs/[...slug]`. Reading a rule with
/// the route's rule would have left `/` uncovered, and `/` is the exact
/// document #1171 was reported against: the check would have been silent on the
/// bug it exists to find.
#[test]
fn a_rules_catch_all_covers_the_root_the_way_the_server_does() {
    assert!(source_matches("/:path*", "/"));
    assert!(source_matches("/:path*", "/guide"));
    assert!(source_matches("/:path*", "/posts/hello/deep"));
}

/// A parameter is one segment and has to have one, which is where a rule's
/// catch-all and a rule's parameter part company.
#[test]
fn a_parameter_needs_a_segment_to_match() {
    assert!(!source_matches("/:slug", "/"));
    assert!(source_matches("/:slug", "/guide"));
    assert!(!source_matches("/:slug", "/posts/hello"));
}

/// A literal source covers its own path and nothing under or beside it.
#[test]
fn a_static_source_covers_only_its_own_path() {
    assert!(source_matches("/", "/"));
    assert!(!source_matches("/", "/guide"));
    assert!(source_matches("/guide", "/guide"));
    assert!(!source_matches("/guide", "/guides"));
    assert!(!source_matches("/guide", "/guide/deep"));
}

/// The first partner: the same documents, under a rule that names no nonce.
#[test]
fn a_rule_that_names_no_nonce_says_nothing() {
    let pages = vec![page("/", "dist/index.html")];
    let rules = vec![rule("/:path*", &[("x-frame-options", "DENY")])];

    assert!(nonced_pages(&pages, &rules).is_empty());
}

/// The second partner, and the fix the message recommends: the same nonce
/// policy, scoped to routes a server renders. A build that takes the advice has
/// to come out silent, or the advice is not advice.
#[test]
fn a_rule_scoped_to_the_routes_a_server_renders_says_nothing() {
    let pages = vec![
        page("/", "dist/index.html"),
        page("/guide", "dist/guide/index.html"),
    ];
    let rules = vec![rule(
        "/posts/:path*",
        &[("content-security-policy", POLICY)],
    )];

    let found = nonced_pages(&pages, &rules);

    assert!(found.is_empty(), "{found:#?}");
}

/// A project with no prerendered documents at all is the other silence: the
/// nonce policy is exactly what that project should be running.
#[test]
fn a_build_that_prerendered_nothing_says_nothing() {
    let rules = vec![rule("/:path*", &[("content-security-policy", POLICY)])];

    assert!(nonced_pages(&[], &rules).is_empty());
}

/// `withNonce` substitutes into any value that names the token, so the check
/// asks the same question of every header rather than of `content-security-
/// policy` alone. A project reporting the nonce somewhere else has the same
/// mismatch, minus the refusal.
#[test]
fn any_header_whose_value_names_the_token_is_named() {
    let pages = vec![page("/", "dist/index.html")];
    let rules = vec![rule("/:path*", &[("x-nonce", "{uf.nonce}")])];

    let found = nonced_pages(&pages, &rules);

    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].header, "x-nonce");
}

/// `headersFor` applies every matching rule rather than the first, so a
/// document can be covered twice. Naming one of the two would send a reader to
/// scope that one and leave the other in place.
#[test]
fn every_matching_rule_is_named_rather_than_the_first() {
    let pages = vec![page("/", "dist/index.html")];
    let rules = vec![
        rule("/:path*", &[("content-security-policy", POLICY)]),
        rule("/", &[("x-nonce", "{uf.nonce}")]),
    ];

    let found = nonced_pages(&pages, &rules);

    assert_eq!(found.len(), 2, "{found:#?}");
    let sources: Vec<&str> = found.iter().map(|page| page.source.as_str()).collect();
    assert_eq!(sources, ["/", "/:path*"]);
}

/// Two rules that say the same thing are one row: the reader has one thing to
/// fix, and a message that says it twice reads like two problems.
#[test]
fn a_document_covered_twice_by_one_rule_shape_is_one_row() {
    let pages = vec![page("/", "dist/index.html")];
    let rules = vec![
        rule("/:path*", &[("content-security-policy", POLICY)]),
        rule("/:path*", &[("content-security-policy", POLICY)]),
    ];

    assert_eq!(nonced_pages(&pages, &rules).len(), 1);
}

/// `dist/404.html` is a document that is served and is not a page, and under a
/// nonce policy it fails exactly the way a page does: the client entry in it is
/// refused and the not-found page never hydrates.
///
/// [`super::super::guards`] leaves it out, and rightly: a guard is a fact about
/// a route, and this document is rendered from a boundary rather than from one.
/// This check is about documents, and the 404 is one — a visitor reaches it by
/// the ordinary means of being wrong about a URL.
#[test]
fn the_not_found_document_is_named_too() {
    let pages = vec![Prerendered {
        url: "/404".to_owned(),
        file: "dist/404.html".to_owned(),
        status: 404,
        regenerates: false,
        partial: false,
    }];
    let rules = vec![rule("/:path*", &[("content-security-policy", POLICY)])];

    let found = nonced_pages(&pages, &rules);

    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].file, "dist/404.html");
}

/// The message names the document, the rule, the file the rule is written in,
/// and both ways out. A reader who has only this line has to be able to act.
#[test]
fn the_message_names_the_document_the_rule_and_the_way_out() {
    let found = nonced_pages(
        &[page("/", "dist/index.html")],
        &[rule("/:path*", &[("content-security-policy", POLICY)])],
    );

    let said = message(&found, CONFIG);

    assert!(said.contains("1 prerendered document"), "{said}");
    assert!(said.contains("/ (dist/index.html)"), "{said}");
    assert!(said.contains("`content-security-policy`"), "{said}");
    assert!(said.contains("`/:path*`"), "{said}");
    assert!(said.contains(CONFIG), "{said}");
    assert!(said.contains("{uf.nonce}"), "{said}");
    assert!(said.contains("Scope the rule's `source`"), "{said}");
    assert!(said.contains("hash policy"), "{said}");
}

/// Past the cap the rest are counted rather than listed, so a project that
/// prerendered a thousand documents gets a message a person can read.
#[test]
fn a_long_report_is_capped_and_the_rest_counted() {
    let pages: Vec<Prerendered> = (0..25)
        .map(|index| {
            page(
                &format!("/p/{index}"),
                &format!("dist/p/{index}/index.html"),
            )
        })
        .collect();
    let found = nonced_pages(
        &pages,
        &[rule("/:path*", &[("content-security-policy", POLICY)])],
    );

    let said = message(&found, CONFIG);

    assert_eq!(found.len(), 25);
    assert!(said.contains("25 prerendered documents"), "{said}");
    assert!(said.contains("… and 15 more"), "{said}");
}

/// The rows sort by URL, so the same tree reports the same order twice.
#[test]
fn the_rows_are_sorted_by_url() {
    let pages = vec![
        page("/guide", "dist/guide/index.html"),
        page("/", "dist/index.html"),
        page("/about", "dist/about/index.html"),
    ];
    let found: Vec<NoncedPage> = nonced_pages(
        &pages,
        &[rule("/:path*", &[("content-security-policy", POLICY)])],
    );

    let urls: Vec<&str> = found.iter().map(|page| page.url.as_str()).collect();
    assert_eq!(urls, ["/", "/about", "/guide"]);
}
