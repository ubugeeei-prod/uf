//! The rule list: the catalogue, at this project's levels, with its fixes.

use super::*;

fn row<'a>(listed: &'a [ListedRule], id: &str) -> &'a ListedRule {
    listed
        .iter()
        .find(|rule| rule.descriptor.id == id)
        .unwrap_or_else(|| panic!("{id} is listed"))
}

#[test]
fn every_rule_is_listed_at_the_level_this_project_runs_it() {
    let mut config = UniflowedConfig::default();
    // Only worth asserting if turning the rule off changes what is listed.
    assert!(
        uf_lint::rule("flow/unclear-type")
            .expect("a rule uf has")
            .default_level
            .is_enabled()
    );
    config
        .lint
        .rules
        .insert("flow/unclear-type".into(), RuleLevel::Off);

    let listed = listed_rules(&config);

    assert_eq!(listed.len(), uf_lint::rules().len());
    assert_eq!(row(&listed, "flow/unclear-type").level, RuleLevel::Off);
}

#[test]
fn a_level_written_under_a_deprecated_name_is_the_level_listed() {
    // What a run does with it, so what the list has to say: reading the
    // config's map by canonical id would list this rule as off.
    let mut config = UniflowedConfig::default();
    config.lint.rules.remove("flow/unclear-type");
    config
        .lint
        .rules
        .insert("flow/type-aware/no-explicit-any".into(), RuleLevel::Warn);

    let listed = listed_rules(&config);

    assert_eq!(row(&listed, "flow/unclear-type").level, RuleLevel::Warn);
}

#[test]
fn each_rule_names_the_fix_that_answers_it() {
    let listed = listed_rules(&UniflowedConfig::default());

    assert_eq!(
        row(&listed, "flow/deprecated-type").fix,
        Some(RuleFix::Safe)
    );
    assert_eq!(
        row(&listed, "flow/non-const-var-export").fix,
        Some(RuleFix::Unsafe)
    );
    assert_eq!(
        row(&listed, "uniflowed/no-tabs").fix,
        Some(RuleFix::Formatter)
    );
    assert_eq!(row(&listed, "flow/unclear-type").fix, None);
}

#[test]
fn the_payload_is_the_catalogue_entry_with_this_projects_answers() {
    let mut config = UniflowedConfig::default();
    config
        .lint
        .rules
        .insert("flow/deprecated-type".into(), RuleLevel::Off);

    let payload = rules_payload(&listed_rules(&config));

    assert_eq!(payload["command"], json!("uf lint --rules"));
    let rules = payload["rules"].as_array().expect("an array of rules");
    assert_eq!(rules.len(), uf_lint::rules().len());
    let entry = |id: &str| {
        rules
            .iter()
            .find(|rule| rule["id"] == json!(id))
            .unwrap_or_else(|| panic!("{id} is in the payload"))
    };
    let deprecated = entry("flow/deprecated-type");
    assert_eq!(deprecated["level"], json!("off"));
    assert_eq!(deprecated["fix"], json!("safe"));
    assert_eq!(deprecated["category"], json!("flow"));
    assert_eq!(deprecated["requirement"], json!("source-text"));
    assert!(deprecated["defaultLevel"].is_string());
    assert!(deprecated["description"].is_string());
    // `null`, not absent: "no fix" is an answer.
    assert_eq!(entry("flow/unclear-type")["fix"], serde_json::Value::Null);
    assert!(
        entry("flow/unclear-type")
            .as_object()
            .is_some_and(|object| object.contains_key("fix"))
    );
    assert!(!payload.to_string().contains('\u{1b}'));
}
