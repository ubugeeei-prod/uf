//! Suppression comments end to end: what they silence, how far they reach, and
//! what happens when they name a rule that does not exist.

use super::*;

#[test]
fn disable_next_line_suppresses_the_diagnostic() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = any;\n",
    );

    assert!(diagnostics.is_empty());
}

#[test]
fn disable_next_line_does_not_leak_to_later_lines() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = any;\ntype B = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 4);
}

#[test]
fn block_suppression_covers_a_range() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable flow/unclear-type\ntype A = any;\n// uf-lint-enable flow/unclear-type\ntype B = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 5);
}

#[test]
fn suppressing_one_rule_leaves_others_reporting() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/unclear-type"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("flow/deprecated-type"),
        RuleLevel::Error,
    );

    let report = lint_source(
        &at(
            "app/index.js",
            "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = { a: any, b: bool };\n",
        ),
        &config,
    )
    .expect("lint");

    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].rule, "flow/deprecated-type");
}

#[test]
fn an_unknown_suppression_rule_id_is_its_own_diagnostic() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/unknown-lint-suppression"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("flow/unclear-type"),
        RuleLevel::Error,
    );

    let report = lint_source(
        &at(
            "app/index.js",
            "// @flow\n// uf-lint-disable-next-line flow/unclear-typo\ntype A = any;\n",
        ),
        &config,
    )
    .expect("lint");

    assert_eq!(report.diagnostics.len(), 2);
    assert_eq!(
        report.diagnostics[0].rule,
        "uniflowed/unknown-lint-suppression"
    );
    assert_eq!(report.diagnostics[0].line, 2);
    assert_eq!(report.diagnostics[1].rule, "flow/unclear-type");
}

#[test]
fn an_unknown_suppression_rule_id_never_suppresses_anything() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/unclear-typo\ntype A = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].rule, "flow/unclear-type");
}

#[test]
fn the_deprecated_alias_works_in_suppression_comments() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/type-aware/no-explicit-any\ntype A = any;\n",
    );

    assert!(diagnostics.is_empty());
}
