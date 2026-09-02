//! `fetch/no-global-override`: replacing the global `fetch` silently unhooks the
//! instrumented client.

use super::*;

#[test]
fn global_fetch_override_is_rejected() {
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/app/page.jsx",
        "// @flow\nglobalThis.fetch = () => Promise.resolve();\n",
    );

    assert!(fired(&diagnostics, "fetch/no-global-override"));
}
