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
