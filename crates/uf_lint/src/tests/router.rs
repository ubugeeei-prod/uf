//! `router/reserved-files`: the file names the router gives meaning to, the
//! variants it accepts, and the near-misses it must still reject.

use super::*;

#[test]
fn router_reserved_files_are_constrained() {
    // `_uf.handler.js` looks like a reserved name and is not one. `_uf.route.js`
    // used to be this test's example, and it became a real role when route
    // handlers landed.
    let diagnostics = lint_one("router/reserved-files", "app/_uf.handler.js", "// @flow\n");

    assert!(fired(&diagnostics, "router/reserved-files"));
}

/// `uf create app react` generates `_uf.page.native.js` and `_uf.page.test.js`,
/// and the rule used to reject both — a freshly scaffolded project failed its
/// own linter. The grammar now lives in `uf_router::reserved`, so the scaffold,
/// the router, and this rule cannot drift apart again.
#[test]
fn router_reserved_files_accepts_platform_and_test_variants() {
    for name in [
        "app/_uf.layout.js",
        "app/_uf.page.js",
        "app/_uf.middleware.js",
        "app/api/_uf.route.js",
        "app/_uf.page.native.js",
        "app/_uf.page.ios.js",
        "app/_uf.page.android.js",
        "app/_uf.page.web.js",
        "app/_uf.page.test.js",
        "app/_uf.layout.test.js",
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
        "app/_uf.handler.js",
        "app/_uf.page.server.js",
        "app/_uf.page.native.test.js",
        "app/_uf.page.ts",
        // A layout the build's router would never load, because a layout is a
        // component and Markdown cannot be one. `_uf.page.mdx` *is* loaded and
        // is not here — see ubugeeei-prod/uf#437.
        "app/_uf.layout.mdx",
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
        "app/_uf.page.js",
        "app/_uf.page.jsx",
        "app/_uf.page.mdx",
        "app/_uf.layout.jsx",
        "app/_uf.not-found.mdx",
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
