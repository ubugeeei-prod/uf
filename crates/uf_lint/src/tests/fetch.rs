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

#[test]
fn a_line_inside_a_template_literal_is_accepted() {
    // The scanner carries the template across lines; a rule that started its
    // own scan at each line would read this one as code.
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/app/page.jsx",
        "// @flow\nconst message = `\nglobalThis.fetch = mine;\n`;\n",
    );

    assert!(
        !fired(&diagnostics, "fetch/no-global-override"),
        "{diagnostics:?}"
    );
}

#[test]
fn a_quote_inside_a_regex_does_not_hide_the_override_after_it() {
    // Reading that quote as the start of a string would swallow the rest of
    // the line — the direction that loses a real finding.
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/app/page.jsx",
        "// @flow\nconst pattern = /\"/;\nglobalThis.fetch = mine;\n",
    );

    assert!(
        fired(&diagnostics, "fetch/no-global-override"),
        "{diagnostics:?}"
    );
}
