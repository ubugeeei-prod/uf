//! Catalogue tests. The catalogue is data, and data drifts silently, so these
//! check the shape rather than the content: sorted, unique, complete, and every
//! deprecated alias still pointing at a rule that exists.

use super::*;

#[test]
fn catalogue_covers_flow_builtins_and_uf_rules() {
    assert_eq!(rules().len(), FlowBuiltinLint::COUNT + OWN_RULES.len());
}

#[test]
fn catalogue_is_sorted_and_free_of_duplicate_ids() {
    let ids = rules()
        .iter()
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids, sorted);
}

#[test]
fn every_flow_builtin_lint_has_a_descriptor() {
    for lint in FlowBuiltinLint::all() {
        let descriptor = rule(lint.as_rule_id())
            .unwrap_or_else(|| panic!("{} has no descriptor", lint.as_rule_id()));
        assert_eq!(descriptor.category, RuleCategory::Flow);
    }
}

#[test]
fn every_flow_namespaced_descriptor_is_a_builtin_or_flow_syntax() {
    for descriptor in rules() {
        let Some(name) = descriptor.id.strip_prefix("flow/") else {
            continue;
        };
        if name == "syntax" {
            continue;
        }
        assert!(
            FlowBuiltinLint::from_rule_id(descriptor.id).is_some(),
            "{} is not a Flow built-in lint",
            descriptor.id
        );
    }
}

#[test]
fn descriptions_are_one_line_and_non_empty() {
    for descriptor in rules() {
        assert!(!descriptor.description.is_empty(), "{}", descriptor.id);
        assert!(!descriptor.description.contains('\n'), "{}", descriptor.id);
    }
}

#[test]
fn deprecated_alias_points_at_flow_unclear_type() {
    assert_eq!(
        canonical_rule_id("flow/type-aware/no-explicit-any"),
        Some(FlowBuiltinLint::UnclearType.as_rule_id())
    );
    assert_eq!(
        deprecated_aliases_for(FlowBuiltinLint::UnclearType.as_rule_id()).collect::<Vec<_>>(),
        vec!["flow/type-aware/no-explicit-any"]
    );
}

#[test]
fn every_deprecated_alias_resolves_to_a_real_rule() {
    for (alias, target) in DEPRECATED_ALIASES.entries() {
        assert!(rule(target).is_some(), "{alias} points at unknown {target}");
        assert!(rule(alias).is_none(), "{alias} must not be a rule itself");
    }
}

#[test]
fn canonical_rule_id_rejects_unknown_ids() {
    assert_eq!(canonical_rule_id("flow/does-not-exist"), None);
    assert_eq!(canonical_rule_id(""), None);
}

#[test]
fn type_checker_rules_are_reported_as_unavailable() {
    assert!(!RuleRequirement::TypeChecker.is_available());
    assert!(RuleRequirement::SourceText.is_available());
}

#[test]
fn sketchy_null_variants_default_off_so_violations_report_once() {
    for member in FlowBuiltinLint::SketchyNull.members() {
        let descriptor = rule(member.as_rule_id()).expect("descriptor");
        assert_eq!(descriptor.default_level, RuleLevel::Off);
    }
    let umbrella = rule(FlowBuiltinLint::SketchyNull.as_rule_id()).expect("descriptor");
    assert_eq!(umbrella.default_level, RuleLevel::Error);
}

#[test]
fn security_rules_are_errors_by_default() {
    for descriptor in rules() {
        if descriptor.category == RuleCategory::Security {
            assert_eq!(
                descriptor.default_level,
                RuleLevel::Error,
                "{}",
                descriptor.id
            );
        }
    }
}
