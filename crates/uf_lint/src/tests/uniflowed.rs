//! The `uniflowed/*` house rules: tabs, trailing whitespace, and package-manager
//! invocations that belong in `uf.config.js`.

use super::*;

#[test]
fn npm_script_invocations_are_rejected() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\nspawn('npm run build');\nspawn('pnpm install');\n",
    );

    assert_eq!(diagnostics.len(), 2);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 8));
}

#[test]
fn uf_task_invocations_are_accepted() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\nexport const tasks = { build: 'uf build' };\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn npm_mentions_in_comments_are_not_invocations() {
    let diagnostics = lint_js(
        "uniflowed/no-npm-script-invocation",
        "// @flow\n// migrated away from npm run build\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn reports_tabs_and_trailing_whitespace() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-tabs"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-trailing-whitespace"),
        RuleLevel::Error,
    );

    let report =
        lint_source(&source("// @flow\n\tconst x: number = 1;  \n"), &config).expect("lint");

    assert!(report.has_errors());
    assert_eq!(report.diagnostics.len(), 2);
    assert_eq!(report.diagnostics[0].rule, "uniflowed/no-tabs");
    assert_eq!(
        report.diagnostics[1].rule,
        "uniflowed/no-trailing-whitespace"
    );
}
