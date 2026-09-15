//! What `app.router`'s three lists read, and every spelling they refuse.
//!
//! The refusals are the half worth a table: each is a rule a Next.js project
//! already has, and each would otherwise be read as a literal that matches
//! nothing — a redirect that silently stops happening.

use std::fs;

use camino::Utf8PathBuf;
use compact_str::CompactString;

use crate::{ConfigError, UniflowedConfig, load_config_file};

/// Load a config whose `app.router` is `router`, written as JavaScript.
fn load(router: &str) -> Result<UniflowedConfig, ConfigError> {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().join("uf.config.js")).unwrap();
    fs::write(
        &path,
        format!("export default defineConfig({{ app: {{ router: {router} }} }});"),
    )
    .unwrap();
    load_config_file(&path)
}

fn refusal(router: &str) -> String {
    match load(router) {
        Ok(_) => panic!("{router} was accepted"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn reads_the_three_lists_a_next_js_project_writes() {
    let config = load(
        r#"{
          redirects: [
            { source: "/old-blog/:slug", destination: "/blog/:slug", permanent: true },
            { source: "/docs/:path*", destination: "https://docs.example.com/:path*?from=app", permanent: false },
          ],
          rewrites: [{ source: "/@me", destination: "/users/me" }],
          headers: [{ source: "/:path*", headers: { "X-Frame-Options": "DENY" } }],
        }"#,
    )
    .unwrap();

    let router = &config.app.router;
    assert_eq!(router.redirects.len(), 2);
    assert_eq!(router.redirects[0].destination, "/blog/:slug");
    assert!(router.redirects[0].permanent);
    assert!(!router.redirects[1].permanent);
    assert_eq!(router.rewrites[0].source, "/@me");
    assert_eq!(
        router.headers[0]
            .headers
            .get(&CompactString::from("X-Frame-Options"))
            .map(CompactString::as_str),
        Some("DENY")
    );
    assert!(router.has_request_rules());
}

#[test]
fn a_project_that_writes_none_has_none() {
    let config = load("{ root: \"app\" }").unwrap();
    assert!(!config.app.router.has_request_rules());
}

/// `deny_unknown_fields`: a rule whose `source` is misspelled is not a rule
/// that matches nothing.
#[test]
fn a_misspelled_field_is_an_error_at_the_file() {
    let message =
        refusal(r#"{ redirects: [{ sorce: "/a", destination: "/b", permanent: true }] }"#);
    assert!(message.contains("sorce"), "{message}");
}

#[test]
fn a_redirect_says_whether_it_is_permanent() {
    let message = refusal(r#"{ redirects: [{ source: "/a", destination: "/b" }] }"#);
    assert!(message.contains("permanent"), "{message}");
}

#[test]
fn refuses_every_spelling_the_matcher_would_read_as_a_literal() {
    let redirect = |source: &str, destination: &str| {
        format!(
            r#"{{ redirects: [{{ source: "{source}", destination: "{destination}", permanent: true }}] }}"#
        )
    };
    let rewrite = |source: &str, destination: &str| {
        format!(r#"{{ rewrites: [{{ source: "{source}", destination: "{destination}" }}] }}"#)
    };
    let cases: Vec<(String, &str)> = vec![
        (redirect("old", "/new"), "starts with `/`"),
        (redirect("/blog/(.*)", "/news"), "regular expression"),
        (redirect("/blog/:slug+", "/news/:slug"), "Next.js modifier"),
        (redirect("/blog/:slug?", "/news/:slug"), "Next.js modifier"),
        (redirect("/blog?page=1", "/news"), "the query"),
        (redirect("/@:user", "/users/:user"), "inside a segment"),
        (redirect("/:path*/edit", "/edit/:path*"), "last segment"),
        (redirect("/:id/:id", "/x/:id"), "twice"),
        (redirect("/:1st", "/x"), "does not name a parameter"),
        (redirect("/old", "new"), "starts with `/`"),
        (
            redirect("/old", "ftp://example.com/new"),
            "`http` or `https`",
        ),
        (redirect("/old", "/new/:slug"), "declares no `:slug`"),
        (redirect("/old/:path*", "/new/:path"), "write `:path*` here"),
        (redirect("/old/:slug", "/new/:slug*"), "write `:slug` here"),
        (redirect("/same", "/same"), "to itself"),
        (
            rewrite("/api/:path*", "https://api.example.com/:path*"),
            "another origin",
        ),
        (
            rewrite("/api/:path*", "//api.example.com/:path*"),
            "another origin",
        ),
        (rewrite("/a", "b"), "starts with `/`"),
        (
            r#"{ headers: [{ source: "/:path*", headers: {} }] }"#.to_owned(),
            "sets no header",
        ),
        (
            r#"{ headers: [{ source: "/:path*", headers: { "x frame": "DENY" } }] }"#.to_owned(),
            "not a header name",
        ),
        (
            r#"{ headers: [{ source: "/:path*", headers: { "x-note": "a\nb" } }] }"#.to_owned(),
            "line break",
        ),
    ];

    for (router, expected) in cases {
        let message = refusal(&router);
        assert!(
            message.contains(expected),
            "{router}\nshould be refused with {expected:?}, and was refused with:\n{message}"
        );
    }
}

/// The message names the list and the entry, so the reader goes to the object
/// rather than searching a file for a source that may appear twice.
#[test]
fn names_the_list_and_the_entry() {
    let message = refusal(
        r#"{ rewrites: [
          { source: "/a", destination: "/b" },
          { source: "/c/(.*)", destination: "/d" },
        ] }"#,
    );
    assert!(message.contains("app.router.rewrites[1]"), "{message}");
}

#[test]
fn reads_a_base_path_and_a_trailing_slash_policy() {
    let config = load(r#"{ basePath: "/docs", trailingSlash: "always" }"#).unwrap();
    assert_eq!(config.app.router.base_path, "/docs");
    assert_eq!(
        config.app.router.trailing_slash,
        crate::TrailingSlash::Always
    );

    // What every project that says nothing gets: the root, and both spellings.
    let unset = load("{ root: \"app\" }").unwrap();
    assert_eq!(unset.app.router.base_path, "");
    assert_eq!(
        unset.app.router.trailing_slash,
        crate::TrailingSlash::Ignore
    );
}

/// A boolean is Next.js's spelling, and uf's is one of three words.
#[test]
fn a_trailing_slash_policy_uf_does_not_have_is_an_error_at_the_file() {
    let message = refusal("{ trailingSlash: true }");
    assert!(message.contains("uf.config.js"), "{message}");
}

#[test]
fn refuses_a_base_path_the_hosts_would_compare_as_characters() {
    for (written, expected) in [
        ("docs", "starts with `/`"),
        ("/docs/", "no trailing slash"),
        ("/", "the root"),
        ("/docs?v=1", "no query"),
        ("/docs//api", "empty segment"),
        ("/docs/../x", "`..`"),
        ("/:tenant", "a pattern"),
    ] {
        let message = refusal(&format!(r#"{{ basePath: "{written}" }}"#));
        assert!(
            message.contains("app.router.basePath") && message.contains(expected),
            "{written}: {message}"
        );
    }
}

/// The spelling a sitemap writes, which has to be the address a server answers
/// without a redirect.
#[test]
fn a_policy_spells_a_path() {
    use crate::TrailingSlash;

    assert_eq!(TrailingSlash::Always.spell("/guide"), "/guide/");
    assert_eq!(TrailingSlash::Never.spell("/guide/"), "/guide");
    assert_eq!(TrailingSlash::Ignore.spell("/guide/"), "/guide/");
    assert_eq!(TrailingSlash::Always.spell("/"), "/");
    assert_eq!(TrailingSlash::Never.spell("/"), "/");
    // A file is not a page, whatever the policy says about pages.
    assert_eq!(TrailingSlash::Always.spell("/sitemap.xml"), "/sitemap.xml");
}
