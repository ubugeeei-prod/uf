//! `flow/syntax`: which files are handed to the Flow parser, and what a parse
//! error looks like once it reaches the report.

use super::*;

#[test]
fn reports_flow_parse_errors() {
    let diagnostics = lint_one("flow/syntax", "src/app/page.jsx", "// @flow\ntype = ;\n");

    assert!(!diagnostics.is_empty());
    assert_eq!(diagnostics[0].rule, "flow/syntax");
}

#[test]
fn flow_syntax_rule_ignores_declaration_file_extensions() {
    for path in [
        "src/app/page.js.flow",
        "src/app/types.flow",
        "src/app/page.server.flow",
    ] {
        let diagnostics = lint_one("flow/syntax", path, "// @flow\ntype = ;\n");

        assert!(
            !fired(&diagnostics, "flow/syntax"),
            "{path} must not be treated as Flow source"
        );
    }
}

#[test]
fn flow_syntax_rule_still_matches_js_spellings() {
    for path in [
        "src/app/page.js",
        "src/app/page.jsx",
        "src/app/page.mjs",
        "src/app/page.cjs",
    ] {
        let diagnostics = lint_one("flow/syntax", path, "// @flow\ntype = ;\n");

        assert!(
            fired(&diagnostics, "flow/syntax"),
            "{path} must be treated as Flow source"
        );
    }
}

#[test]
fn a_module_that_awaits_at_its_top_level_is_not_a_parse_error() {
    // ubugeeei-prod/uf#204. `uf lint` was the loudest of the three commands
    // that refused such a module, because a linter is what a project runs
    // over every file it has.
    let diagnostics = lint_js(
        "flow/syntax",
        "// @flow\nimport { load } from \"./io.js\";\nconst settings = await load();\nexport { settings };\n",
    );

    assert!(
        !fired(&diagnostics, "flow/syntax"),
        "a module may await at its top level: {diagnostics:?}"
    );
}

#[test]
fn await_outside_an_async_function_is_still_a_parse_error() {
    // The same lines without the `import` and the `export`, which are the only
    // things that made the file a module: in a script `await` is an ordinary
    // identifier and this is two expressions with nothing between them.
    let diagnostics = lint_js("flow/syntax", "// @flow\nconst settings = await load();\n");

    assert!(fired(&diagnostics, "flow/syntax"), "{diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("only allowed at the top level of a module"),
        "{}",
        diagnostics[0].message
    );
}
