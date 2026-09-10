//! `router/reserved-files`: the file names the router gives meaning to, the
//! variants it accepts, and the near-misses it must still reject.

use super::*;

#[test]
fn router_reserved_files_are_constrained() {
    // `$handler.js` looks like a reserved name and is not one. `$route.js`
    // used to be this test's example, and it became a real role when route
    // handlers landed.
    let diagnostics = lint_one("router/reserved-files", "app/$handler.js", "// @flow\n");

    assert!(fired(&diagnostics, "router/reserved-files"));
}

/// `uf create app react` generates `$page.native.js` and `$page.test.js`,
/// and the rule used to reject both — a freshly scaffolded project failed its
/// own linter. The grammar now lives in `uf_router::reserved`, so the scaffold,
/// the router, and this rule cannot drift apart again.
#[test]
fn router_reserved_files_accepts_platform_and_test_variants() {
    for name in [
        "app/$layout.js",
        "app/$page.js",
        "app/$middleware.js",
        "app/api/$route.js",
        "app/$page.native.js",
        "app/$page.ios.js",
        "app/$page.android.js",
        "app/$page.web.js",
        "app/$page.test.js",
        "app/$layout.test.js",
    ] {
        let diagnostics = lint_one("router/reserved-files", name, "// @flow\n");

        assert!(
            !fired(&diagnostics, "router/reserved-files"),
            "{name} should be accepted"
        );
    }
}

#[test]
fn router_reserved_files_still_rejects_names_uf_does_not_define() {
    for name in [
        "app/$handler.js",
        "app/$page.server.js",
        "app/$page.native.test.js",
        "app/$page.ts",
        // A layout the build's router would never load, because a layout is a
        // component and Markdown cannot be one. `$page.mdx` *is* loaded and
        // is not here — see ubugeeei-prod/uf#437.
        "app/$layout.mdx",
    ] {
        let diagnostics = lint_one("router/reserved-files", name, "// @flow\n");

        assert!(
            fired(&diagnostics, "router/reserved-files"),
            "{name} should be rejected"
        );
    }
}

/// ubugeeei-prod/uf#437, #386: the build's router has accepted `.jsx` and
/// `.mdx` since it was written, and this rule called them names uf does not
/// define — so every `.mdx` page in a documentation site was a finding.
#[test]
fn router_reserved_files_accepts_every_extension_the_build_runs() {
    for name in [
        "app/$page.js",
        "app/$page.jsx",
        "app/$page.mdx",
        "app/$layout.jsx",
        "app/$not-found.mdx",
    ] {
        let diagnostics = lint_one("router/reserved-files", name, "// @flow\n");

        assert!(
            !fired(&diagnostics, "router/reserved-files"),
            "{name} is a file the build loads"
        );
    }
}

#[test]
fn router_reserved_files_leaves_project_owned_names_alone() {
    for name in ["app/page.js", "app/client/Counter.js", "app/_private.js"] {
        let diagnostics = lint_one("router/reserved-files", name, "// @flow\n");

        assert!(
            !fired(&diagnostics, "router/reserved-files"),
            "{name} should be untouched"
        );
    }
}

/// `router/unsupported-segment`: the directory spelling uf reserves without
/// serving.
///
/// The diagnostic lands on a file because a file is what `uf lint` can point
/// at; what is wrong is the directory the file is in. Before this, `@team` and
/// `(.)photo` were literal URL segments in both routers and nothing said so.
/// `@team` is a slot uf serves now, and this rule is what is left: interception
/// needs a navigation to carry where it came from.
#[test]
fn router_unsupported_segment_reports_an_interception() {
    for path in [
        "app/feed/(.)photo/$page.js",
        "app/feed/(..)photo/$layout.js",
        "app/feed/(..)(..)photo/$page.js",
    ] {
        let diagnostics = lint_one("router/unsupported-segment", path, "// @flow\n");

        assert!(
            fired(&diagnostics, "router/unsupported-segment"),
            "{path} should be reported"
        );
    }
}

/// The message has to say the directory is refused, not merely unrecognised:
/// "unsupported" reads as "ignored", and being ignored is what it used to be.
#[test]
fn router_unsupported_segment_says_what_the_spelling_is_and_that_it_is_refused() {
    let diagnostics = lint_one(
        "router/unsupported-segment",
        "app/feed/(.)photo/$page.js",
        "// @flow\n",
    );
    let message = &diagnostics
        .iter()
        .find(|diagnostic| diagnostic.rule == "router/unsupported-segment")
        .expect("a diagnostic")
        .message;

    assert!(message.contains("(.)photo"), "{message}");
    assert!(message.contains("intercepting route"), "{message}");
    assert!(message.contains("refused"), "{message}");
    assert!(message.contains("267"), "{message}");
}

#[test]
fn router_unsupported_segment_leaves_the_segments_uf_serves_alone() {
    for path in [
        "app/$page.js",
        "app/(marketing)/about/$page.js",
        "app/posts/[slug]/$page.js",
        "app/docs/[...path]/$page.js",
        // A slot is a route uf serves: it renders into the layout of the
        // segment that declares it, at that segment's own paths.
        "app/dashboard/@team/$page.js",
        "app/@team/$layout.js",
        "app/dashboard/@team/$default.js",
        // A private subtree: neither router walks into it, so an interception
        // there is not a route uf would have served.
        "app/_drafts/(.)photo/notes.js",
        // Outside the router root entirely. `@scope` is a directory, not a
        // route, and a rule that reported it would report every workspace.
        "packages/@uniflowed/router/index.js",
    ] {
        let diagnostics = lint_one("router/unsupported-segment", path, "// @flow\n");

        assert!(
            !fired(&diagnostics, "router/unsupported-segment"),
            "{path} should be untouched"
        );
    }
}
