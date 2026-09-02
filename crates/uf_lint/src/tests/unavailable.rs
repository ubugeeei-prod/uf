//! A rule that needs type inference must be reported as skipped rather than
//! quietly passing, because a silently disabled lint is worse than none.

use super::*;

#[test]
fn type_checker_rules_are_reported_as_unavailable() {
    let report = lint_source(&source("// @flow\n"), &UniflowedConfig::default()).expect("lint");

    assert!(
        report
            .unavailable
            .iter()
            .any(|entry| entry.rule == "flow/sketchy-null")
    );
    assert!(
        report
            .unavailable
            .iter()
            .all(|entry| entry.requirement == RuleRequirement::TypeChecker)
    );
    assert!(
        report
            .unavailable
            .iter()
            .all(|entry| entry.level.is_enabled())
    );
}

#[test]
fn unavailable_rules_do_not_count_as_errors() {
    let report = lint_source(&source("// @flow\n"), &UniflowedConfig::default()).expect("lint");

    assert!(!report.unavailable.is_empty());
    assert!(!report.has_errors());
}

#[test]
fn disabled_type_checker_rules_are_not_reported_as_unavailable() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    let report = lint_source(&source("// @flow\n"), &config).expect("lint");

    assert!(report.unavailable.is_empty());
}

#[test]
fn unavailable_rules_explain_themselves() {
    let report = lint_source(&source("// @flow\n"), &UniflowedConfig::default()).expect("lint");
    let entry = report
        .unavailable
        .iter()
        .find(|entry| entry.rule == "flow/unused-promise")
        .expect("unused-promise is enabled by default");

    assert!(entry.reason().contains("type inference"));
}

#[test]
fn unavailable_rules_are_listed_once_regardless_of_file_count() {
    let files = (0..8)
        .map(|index| at(&format!("app/{index}.js"), "// @flow\n"))
        .collect::<Vec<_>>();
    let report = lint_sources(&files, &UniflowedConfig::default()).expect("lint");

    let sketchy = report
        .unavailable
        .iter()
        .filter(|entry| entry.rule == "flow/sketchy-null")
        .count();
    assert_eq!(sketchy, 1);
    assert_eq!(report.files_checked, 8);
}
