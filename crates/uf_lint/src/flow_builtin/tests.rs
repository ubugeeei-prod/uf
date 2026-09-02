//! Tests for the lint tables, pinning down the two invariants the rest of the
//! module quietly relies on: the table is indexed by the enum's own discriminant,
//! and every name round-trips through the perfect hash.

use super::*;

#[test]
fn lint_table_is_indexed_by_discriminant() {
    for (index, entry) in LINTS.iter().enumerate() {
        assert_eq!(
            entry.lint as usize, index,
            "{} is stored at the wrong index",
            entry.name
        );
    }
}

#[test]
fn every_lint_is_reachable_from_all() {
    assert_eq!(FlowBuiltinLint::all().len(), FlowBuiltinLint::COUNT);
    assert_eq!(LINTS.len(), FlowBuiltinLint::COUNT);
}

#[test]
fn rule_ids_are_the_namespaced_lint_names() {
    for lint in FlowBuiltinLint::all() {
        assert_eq!(
            lint.as_rule_id(),
            format!("{FLOW_NAMESPACE}{}", lint.as_name()),
            "{} has a mismatched rule id",
            lint.as_name()
        );
    }
}

#[test]
fn lint_names_are_sorted_and_unique() {
    let names = LINTS.iter().map(|entry| entry.name).collect::<Vec<_>>();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted);
}

#[test]
fn name_table_round_trips_through_the_perfect_hash() {
    for lint in FlowBuiltinLint::all() {
        assert_eq!(FlowBuiltinLint::from_lint_name(lint.as_name()), Some(lint));
        assert_eq!(FlowBuiltinLint::from_rule_id(lint.as_rule_id()), Some(lint));
    }
}

#[test]
fn perfect_hash_only_adds_the_two_flow_long_spellings() {
    assert_eq!(BY_NAME.len(), FlowBuiltinLint::COUNT + 2);
    assert_eq!(
        FlowBuiltinLint::from_lint_name("sketchy-number-and"),
        Some(FlowBuiltinLint::SketchyNumber)
    );
    assert_eq!(
        FlowBuiltinLint::from_lint_name("deprecated-type-bool"),
        Some(FlowBuiltinLint::DeprecatedType)
    );
}

#[test]
fn sketchy_null_is_the_only_umbrella() {
    let umbrellas = FlowBuiltinLint::all()
        .filter(|lint| lint.is_umbrella())
        .collect::<Vec<_>>();
    assert_eq!(umbrellas, vec![FlowBuiltinLint::SketchyNull]);
    assert_eq!(FlowBuiltinLint::SketchyNull.members().len(), 5);
}

#[test]
fn umbrella_members_are_themselves_leaves() {
    for member in FlowBuiltinLint::SketchyNull.members() {
        assert!(!member.is_umbrella());
        assert!(member.as_name().starts_with("sketchy-null-"));
    }
}

#[test]
fn from_str_accepts_bare_names_and_rule_ids() {
    assert_eq!(
        "unclear-type".parse::<FlowBuiltinLint>(),
        Ok(FlowBuiltinLint::UnclearType)
    );
    assert_eq!(
        "flow/unclear-type".parse::<FlowBuiltinLint>(),
        Ok(FlowBuiltinLint::UnclearType)
    );
}

#[test]
fn from_str_rejects_names_flow_does_not_ship() {
    for name in [
        "implicit-inexact-object",
        "unused-promise-in-async-scope",
        "require-explicit-import-type",
        "deprecated-class-static-blocks",
        "",
        "flow/",
        "sketchy",
        "FLOW/UNCLEAR-TYPE",
    ] {
        assert_eq!(
            name.parse::<FlowBuiltinLint>(),
            Err(FlowLintParseError::UnknownLint {
                name: CompactString::from(name),
            }),
            "{name} should not resolve"
        );
    }
}

#[test]
fn parse_error_message_names_the_offending_spelling() {
    let error = "implicit-inexact-object"
        .parse::<FlowBuiltinLint>()
        .expect_err("unknown lint");
    assert_eq!(
        error.to_string(),
        "`implicit-inexact-object` is not a Flow built-in lint"
    );
}
