//! What goes in the two files, and what a wrong one would have looked like.

use serde_json::json;
use uf_config::RobotsConfig;

use super::{Prerendered, SiteUrl, UnguardedPage, indexable, robots, sitemap};

/// A `site.robots` block, read the way `uf.config.js` is read.
///
/// `RobotsConfig` is `#[non_exhaustive]`, so a struct literal is not available
/// outside `uf_config` — which is the right constraint and the better test:
/// what these cases describe is a project's configuration file, and this is
/// the path that file actually takes.
fn robots_config(value: serde_json::Value) -> RobotsConfig {
    serde_json::from_value(value).expect("a robots configuration")
}

fn page(url: &str, status: u16) -> Prerendered {
    Prerendered {
        url: url.to_owned(),
        file: format!("dist{url}/index.html"),
        status,
    }
}

fn guarded(url: &str) -> UnguardedPage {
    UnguardedPage {
        url: url.to_owned(),
        file: format!("dist{url}/index.html"),
        middleware: vec![String::from("app/$middleware.js")],
    }
}

fn site() -> SiteUrl {
    SiteUrl::parse("https://docs.uniflowed.dev").expect("a site url")
}

// --- `site.url` -------------------------------------------------------

#[test]
fn a_trailing_slash_is_not_kept_twice() {
    assert_eq!(
        SiteUrl::parse("https://docs.uniflowed.dev/").expect("parses"),
        SiteUrl::parse("https://docs.uniflowed.dev").expect("parses")
    );
}

#[test]
fn a_site_under_a_path_keeps_the_path() {
    // A project deployed under a prefix — GitHub Pages for a repository, most
    // often — has one, and dropping it would name a page that does not exist.
    let site = SiteUrl::parse("https://example.com/docs/").expect("parses");

    assert_eq!(site.join("/guide"), "https://example.com/docs/guide");
    assert_eq!(site.join("/"), "https://example.com/docs/");
}

#[test]
fn the_root_keeps_its_slash() {
    // `https://example.com` is an origin; a `<loc>` is a URL, and a URL with
    // an authority has a path.
    assert_eq!(site().join("/"), "https://docs.uniflowed.dev/");
}

#[test]
fn a_relative_or_schemeless_url_is_refused_rather_than_guessed() {
    for wrong in [
        "docs.uniflowed.dev",
        "//docs.uniflowed.dev",
        "/docs",
        "ftp://docs.uniflowed.dev",
        "https://",
        "https:///docs",
    ] {
        assert!(
            SiteUrl::parse(wrong).is_err(),
            "{wrong:?} is not something a <loc> can be built from"
        );
    }
}

#[test]
fn a_query_or_a_fragment_is_refused() {
    assert!(SiteUrl::parse("https://example.com/?utm=1").is_err());
    assert!(SiteUrl::parse("https://example.com/#top").is_err());
}

// --- which URLs ---------------------------------------------------------

#[test]
fn the_error_document_is_not_a_page() {
    // `404.html` is written from the root not-found boundary and reported as
    // `/404`. It is served, and submitting it to a search engine would be
    // asking for the error page to be indexed.
    let pages = [page("/", 200), page("/guide", 200), page("/404", 404)];

    assert_eq!(indexable(&pages, &[]), ["/", "/guide"]);
}

#[test]
fn a_guarded_route_is_not_advertised() {
    // The route is prerendered — the file is in `dist/` and a static host will
    // serve it — but a `$middleware.js` says it is not for everyone, and a
    // sitemap is a submission to search engines.
    let pages = [page("/", 200), page("/dashboard", 200)];

    assert_eq!(indexable(&pages, &[guarded("/dashboard")]), ["/"]);
}

#[test]
fn the_order_is_the_sort_and_not_the_prerender() {
    // Two builds of the same sources have to produce the same bytes, and the
    // prerender's order is whatever order the route table was walked in.
    let pages = [page("/guide", 200), page("/", 200), page("/about", 200)];

    assert_eq!(indexable(&pages, &[]), ["/", "/about", "/guide"]);
}

// --- the sitemap --------------------------------------------------------

#[test]
fn the_sitemap_is_the_urls_and_nothing_else() {
    let urls = [String::from("/"), String::from("/guide")];

    let xml = sitemap(&site(), &urls).expect("writes");

    assert_eq!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n\
         \x20 <url>\n    <loc>https://docs.uniflowed.dev/</loc>\n  </url>\n\
         \x20 <url>\n    <loc>https://docs.uniflowed.dev/guide</loc>\n  </url>\n\
         </urlset>\n"
    );
}

#[test]
fn no_url_carries_a_lastmod_a_changefreq_or_a_priority() {
    // Each of the three is a claim, and this build has no fact behind any of
    // them. The argument is in the module documentation; this is the assertion
    // that stops one being added because a generator elsewhere emits it.
    let xml = sitemap(&site(), &[String::from("/")]).expect("writes");

    for absent in ["lastmod", "changefreq", "priority"] {
        assert!(
            !xml.contains(absent),
            "{absent} is not a fact this build has"
        );
    }
}

#[test]
fn an_ampersand_in_a_url_is_escaped_and_not_left_to_break_the_document() {
    // A `generateStaticParams` value can be anything, and one `&` in a `<loc>`
    // is a sitemap no parser will read past.
    let urls = [String::from("/posts/tom-&-jerry")];

    let xml = sitemap(&site(), &urls).expect("writes");

    assert!(
        xml.contains("<loc>https://docs.uniflowed.dev/posts/tom-&amp;-jerry</loc>"),
        "{xml}"
    );
}

#[test]
fn a_space_is_percent_encoded_and_an_encoded_byte_is_left_alone() {
    // The prerender runs `encodeURIComponent` over parameter values, so a path
    // can arrive already encoded; encoding the `%` again would name a
    // different page than the one that was built.
    let urls = [
        String::from("/posts/a b"),
        String::from("/posts/caf%C3%A9"),
        String::from("/posts/100%"),
    ];

    let xml = sitemap(&site(), &urls).expect("writes");

    assert!(xml.contains("/posts/a%20b"), "{xml}");
    assert!(xml.contains("/posts/caf%C3%A9"), "{xml}");
    assert!(xml.contains("/posts/100%25"), "{xml}");
}

#[test]
fn a_site_over_the_protocol_limit_fails_rather_than_shipping_a_rejected_file() {
    // Over 50,000 the whole file is thrown away by the consumer, so a build
    // that wrote one would have shipped nothing while appearing to ship
    // everything.
    let urls: Vec<String> = (0..50_001).map(|index| format!("/{index}")).collect();

    let error = sitemap(&site(), &urls).expect_err("refuses");

    assert!(error.to_string().contains("sitemap index"), "{error}");
}

// --- robots.txt ---------------------------------------------------------

#[test]
fn robots_says_where_the_sitemap_is() {
    let text = robots(&site(), &RobotsConfig::default(), true)
        .expect("writes")
        .expect("a file");

    assert_eq!(
        text,
        "User-agent: *\nDisallow:\n\nSitemap: https://docs.uniflowed.dev/sitemap.xml\n"
    );
}

#[test]
fn no_sitemap_and_no_rules_is_no_file() {
    // A `robots.txt` that disallows nothing and points nowhere is exactly
    // equivalent to not having one, so writing it would add a request to every
    // crawl for no answer.
    assert!(
        robots(&site(), &RobotsConfig::default(), false)
            .expect("decides")
            .is_none()
    );
}

#[test]
fn a_rule_is_enough_on_its_own() {
    let config = robots_config(json!({ "disallow": ["/internal"] }));

    let text = robots(&site(), &config, false)
        .expect("writes")
        .expect("a file");

    assert_eq!(text, "User-agent: *\nDisallow: /internal\n");
}

#[test]
fn an_allow_is_written_before_the_disallow_it_carves_out_of() {
    let config = robots_config(json!({
        "allow": ["/internal/public"],
        "disallow": ["/internal"],
    }));

    let text = robots(&site(), &config, true)
        .expect("writes")
        .expect("a file");

    assert_eq!(
        text,
        "User-agent: *\nAllow: /internal/public\nDisallow: /internal\n\n\
         Sitemap: https://docs.uniflowed.dev/sitemap.xml\n"
    );
}

#[test]
fn disabling_it_is_believed_even_when_there_is_a_sitemap() {
    let config = robots_config(json!({ "enabled": false }));

    assert!(robots(&site(), &config, true).expect("decides").is_none());
}

#[test]
fn a_rule_that_is_not_a_path_is_refused() {
    // `robots.txt` matches a rule against a request path, so a value that does
    // not begin with `/` matches nothing and the file would be a silent no-op.
    let config = robots_config(json!({ "disallow": ["internal"] }));

    assert!(robots(&site(), &config, true).is_err());
}

#[test]
fn a_rule_may_not_smuggle_a_second_line_into_the_file() {
    let config = robots_config(json!({ "disallow": ["/a\nUser-agent: *"] }));

    assert!(robots(&site(), &config, true).is_err());
}
