//! The rule catalogue and the default config have to agree with each other,
//! and a rule id that was renamed has to keep answering to its old spelling.

use super::*;

#[test]
fn every_default_rule_has_a_descriptor() {
    let config = UniflowedConfig::default();
    for id in config.lint.rules.keys() {
        assert!(
            rule(id.as_str()).is_some(),
            "{id} is configured by default but has no RuleDescriptor"
        );
    }
}

#[test]
fn every_descriptor_has_a_default_rule_level() {
    let config = UniflowedConfig::default();
    for descriptor in rules() {
        assert!(
            config.lint.rules.contains_key(descriptor.id),
            "{} has a RuleDescriptor but no default level",
            descriptor.id
        );
    }
}

#[test]
fn default_levels_match_the_descriptor_catalogue() {
    let config = UniflowedConfig::default();
    for descriptor in rules() {
        assert_eq!(
            config.lint.rules.get(descriptor.id).copied(),
            Some(descriptor.default_level),
            "{} disagrees between uf_config and uf_lint",
            descriptor.id
        );
    }
}

#[test]
fn catalogue_and_config_have_the_same_size() {
    assert_eq!(UniflowedConfig::default().lint.rules.len(), rules().len());
}

#[test]
fn every_flow_builtin_lint_is_enabled_or_deliberately_off() {
    let config = UniflowedConfig::default();
    for lint in FlowBuiltinLint::all() {
        assert!(
            config.lint.rules.contains_key(lint.as_rule_id()),
            "{} has no default level",
            lint.as_rule_id()
        );
    }
}

#[test]
fn rules_are_enumerable_for_inspect() {
    let descriptor = rule("flow/unclear-type").expect("descriptor");
    assert_eq!(descriptor.category, RuleCategory::Flow);
    assert_eq!(descriptor.requirement, RuleRequirement::SourceText);
    assert!(!descriptor.description.is_empty());
}

#[test]
fn deprecated_any_rule_id_still_configures_unclear_type() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/type-aware/no-explicit-any"),
        RuleLevel::Error,
    );

    let report = lint_source(&source("// @flow\ntype P = { v: any };\n"), &config).expect("lint");

    assert!(fired(&report.diagnostics, "flow/unclear-type"));
    assert!(!fired(
        &report.diagnostics,
        "flow/type-aware/no-explicit-any"
    ));
}

#[test]
fn deprecated_any_rule_id_can_still_switch_the_rule_off() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/type-aware/no-explicit-any"),
        RuleLevel::Off,
    );

    let report = lint_source(&source("// @flow\ntype P = { v: any };\n"), &config).expect("lint");

    assert!(report.diagnostics.is_empty());
}

#[test]
fn the_canonical_id_wins_over_the_deprecated_alias() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config.lint.rules.insert(
        CompactString::const_new("flow/type-aware/no-explicit-any"),
        RuleLevel::Error,
    );
    config.lint.rules.insert(
        CompactString::const_new("flow/unclear-type"),
        RuleLevel::Off,
    );

    let report = lint_source(&source("// @flow\ntype P = { v: any };\n"), &config).expect("lint");

    assert!(report.diagnostics.is_empty());
}

#[test]
fn rule_levels_can_disable_builtin_rules() {
    let mut config = UniflowedConfig::default();
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/no-tabs"),
        RuleLevel::Off,
    );

    let report = lint_source(&source("// @flow\n\tconst x: number = 1;\n"), &config).expect("lint");

    assert!(!report.has_errors());
    assert!(report.diagnostics.is_empty());
}

#[test]
fn type_aware_rule_blocks_explicit_any() {
    let report = lint_source(
        &source("// @flow\ntype Props = { value: any };\n"),
        &UniflowedConfig::default(),
    )
    .expect("lint");

    assert!(report.has_errors());
    assert!(fired(&report.diagnostics, "flow/unclear-type"));
}
