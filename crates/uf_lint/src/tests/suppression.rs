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

// --- uniflowed/unused-lint-suppression -------------------------------------

/// `flow/unclear-type` and `uniflowed/unused-lint-suppression`, both at
/// `error`, plus whatever `extra` names.
fn with_unused(extra: &[(&str, RuleLevel)]) -> UniflowedConfig {
    let mut config = only("flow/unclear-type");
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/unused-lint-suppression"),
        RuleLevel::Error,
    );
    for (rule, level) in extra {
        config.lint.rules.insert(CompactString::from(*rule), *level);
    }
    config
}

fn lint_with(config: &UniflowedConfig, source: &str) -> Vec<Diagnostic> {
    lint_sources(&[at("app/index.js", source)], config)
        .expect("lint")
        .diagnostics
}

#[test]
fn a_suppression_that_silences_a_finding_is_used() {
    let diagnostics = lint_with(
        &with_unused(&[]),
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = any;\n\
         // uf-lint-disable flow/unclear-type\ntype B = any;\n// uf-lint-enable flow/unclear-type\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// The case the rule exists for: a finding went away — here the line was
/// rewritten, in this repository it was usually a false positive the rule
/// stopped reporting — and the comment arguing for it stayed.
#[test]
fn a_suppression_that_silences_nothing_is_reported_where_it_names_the_rule() {
    let diagnostics = lint_with(
        &with_unused(&[]),
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = mixed;\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].rule, "uniflowed/unused-lint-suppression");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 30));
}

#[test]
fn an_unused_block_suppression_is_reported_once() {
    let diagnostics = lint_with(
        &with_unused(&[]),
        "// @flow\n// uf-lint-disable flow/unclear-type\ntype A = mixed;\n// uf-lint-enable flow/unclear-type\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 2);
}

/// One comment naming two rules is two suppressions, judged apart.
#[test]
fn each_rule_a_comment_names_is_judged_on_its_own() {
    let diagnostics = lint_with(
        &with_unused(&[("security/no-eval", RuleLevel::Error)]),
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type, security/no-eval\ntype A = any;\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].rule, "uniflowed/unused-lint-suppression");
    assert!(diagnostics[0].message.contains("security/no-eval"));
}

/// A rule that did not run found nothing because it did not look, so a
/// suppression of it cannot be called unused: switched off in this project,
/// or waiting on type inference uf does not have.
#[test]
fn a_suppression_of_a_rule_that_did_not_run_is_not_judged() {
    let unavailable = crate::rules()
        .iter()
        .find(|descriptor| !descriptor.requirement.is_available())
        .expect("a rule that needs type inference")
        .id;
    let config = with_unused(&[(unavailable, RuleLevel::Error)]);
    let source = format!(
        "// @flow\n// uf-lint-disable-next-line security/no-eval\neval(x);\n\
         // uf-lint-disable-next-line {unavailable}\nconst y = 1;\n"
    );

    assert!(lint_with(&config, &source).is_empty());
}

/// A file linted on its own — an editor's question — cannot see the manifests
/// and the import graph the project rules read.
#[test]
fn a_single_file_does_not_judge_suppressions_of_project_rules() {
    let config = with_unused(&[("import/no-cycle", RuleLevel::Error)]);
    let source =
        "// @flow\n// uf-lint-disable-next-line import/no-cycle\nimport { a } from './a.js';\n";

    let alone = lint_source(&at("app/index.js", source), &config)
        .expect("lint")
        .diagnostics;
    assert!(alone.is_empty(), "{alone:?}");

    let project = lint_with(&config, source);
    assert_eq!(project.len(), 1, "{project:?}");
}

/// Off is off: a project that does not want the report does not get it.
#[test]
fn an_unused_suppression_is_not_reported_when_the_rule_is_off() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\n// uf-lint-disable-next-line flow/unclear-type\ntype A = mixed;\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
