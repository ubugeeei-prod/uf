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

#[test]
fn a_sentence_naming_global_fetch_is_accepted() {
    // A string that says `globalThis.fetch` overrides nothing, and a package
    // whose subject is the global fetch is full of such strings.
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/app/page.jsx",
        "// @flow\nconst message = \"do not replace globalThis.fetch\";\n",
    );

    assert!(!fired(&diagnostics, "fetch/no-global-override"));
}

#[test]
fn an_override_after_a_string_on_the_same_line_is_still_rejected() {
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/app/page.jsx",
        "// @flow\nlog(\"replacing\"); globalThis.fetch = mine;\n",
    );

    assert!(fired(&diagnostics, "fetch/no-global-override"));
}
