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

#[test]
fn reading_the_global_as_a_default_is_accepted() {
    // `@uniflowed/fetch`'s own client, and the only way for it to have a
    // default. The rule is named after reassignment and used to fire on any
    // mention of the name in code, which made it loudest in the package it
    // exists for.
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/client.js",
        "// @flow\nconst doFetch = settings.fetch ?? globalThis.fetch;\n",
    );

    assert!(
        !fired(&diagnostics, "fetch/no-global-override"),
        "{diagnostics:#?}"
    );
}

#[test]
fn asking_whether_the_global_exists_is_accepted() {
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/client.js",
        "// @flow\nif (globalThis.fetch === undefined) throw new Error(\"no fetch\");\n",
    );

    assert!(
        !fired(&diagnostics, "fetch/no-global-override"),
        "{diagnostics:#?}"
    );
}

#[test]
fn passing_the_global_somewhere_is_accepted() {
    let diagnostics = lint_one(
        "fetch/no-global-override",
        "src/client.js",
        "// @flow\nconst client = createClient({ fetch: globalThis.fetch });\n",
    );

    assert!(
        !fired(&diagnostics, "fetch/no-global-override"),
        "{diagnostics:#?}"
    );
}

#[test]
fn installing_the_global_when_it_is_missing_is_rejected() {
    // `??=` installs one just as surely as `=` does, and reads like a
    // capability check while doing it.
    for source in [
        "// @flow\nglobalThis.fetch ??= polyfill;\n",
        "// @flow\nwindow.fetch ||= polyfill;\n",
        "// @flow\nglobal.fetch &&= wrapped;\n",
    ] {
        let diagnostics = lint_one("fetch/no-global-override", "src/client.js", source);
        assert!(fired(&diagnostics, "fetch/no-global-override"), "{source}");
    }
}
