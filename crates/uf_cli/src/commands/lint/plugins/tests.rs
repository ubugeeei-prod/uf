//! Which enabled ids are a project's rules, and how a host's reply becomes
//! diagnostics.

use super::*;

fn config_with(plugins: &[&str], rules: &[(&str, RuleLevel)]) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.plugins = plugins
        .iter()
        .map(|name| uf_config::PluginEntry::Name((*name).into()))
        .collect();
    for (id, level) in rules {
        config.lint.rules.insert((*id).into(), *level);
    }
    config
}

#[test]
fn without_a_plugin_no_rule_is_a_project_rule() {
    let config = config_with(&[], &[("acme/no-foo", RuleLevel::Error)]);

    assert!(enabled_project_rules(&config).is_empty());
}

#[test]
fn a_project_rule_is_enabled_and_in_a_namespace_uf_does_not_own() {
    let config = config_with(
        &["./rules/acme.js"],
        &[
            ("acme/no-foo", RuleLevel::Error),
            ("acme/quiet", RuleLevel::Off),
            ("flow/unclear-type", RuleLevel::Error),
            // A misspelt uf rule, which no plugin should be asked to define.
            ("react/hooks-rule", RuleLevel::Error),
            ("no-namespace", RuleLevel::Warn),
        ],
    );

    let ids: Vec<&str> = enabled_project_rules(&config)
        .iter()
        .map(|(id, _)| *id)
        .collect();

    assert_eq!(ids, ["acme/no-foo"]);
}

#[test]
fn offsets_are_counted_in_the_utf16_units_estree_uses() {
    assert_eq!(byte_offset("abc", 2), 2);
    // `é` is one unit and two bytes; `😀` is two units and four bytes.
    assert_eq!(byte_offset("éx", 1), 2);
    assert_eq!(byte_offset("a😀b", 3), 5);
    assert_eq!(byte_offset("ab", 99), 2);
}

#[test]
fn a_reply_becomes_diagnostics_at_the_level_the_project_set() {
    let file = SourceFile {
        path: String::from("app.js"),
        source: String::from("// @flow\n\nconst foo = 1;\n"),
    };
    let levels: FxHashMap<&'static str, RuleLevel> = [(intern("acme/no-foo"), RuleLevel::Warn)]
        .into_iter()
        .collect();
    let reply = json!({
        "type": "linted",
        "diagnostics": [
            { "rule": "acme/no-foo", "message": "no foo", "start": 16, "end": 19, "fix": null },
            { "rule": "other/rule", "message": "not enabled here", "start": 0, "end": 1, "fix": null },
        ],
        "micros": { "acme/no-foo": 12.5, "other/rule": 99.0 },
        "problems": ["`acme/no-foo` threw on app.js: boom"],
    });
    let mut outcome = ProjectRules::default();
    let mut micros = FxHashMap::default();

    collect(&reply, &file, &levels, &mut outcome, &mut micros);

    assert_eq!(
        outcome.diagnostics.len(),
        1,
        "a rule the project did not enable reports nothing"
    );
    let diagnostic = &outcome.diagnostics[0];
    assert_eq!(diagnostic.rule, "acme/no-foo");
    assert_eq!((diagnostic.line, diagnostic.column), (3, 7));
    assert_eq!(diagnostic.severity, Severity::Warn);
    assert_eq!(diagnostic.path.as_deref(), Some("app.js"));
    assert_eq!(micros.len(), 1);
    assert_eq!(micros.get("acme/no-foo"), Some(&12.5));
    assert_eq!(outcome.problems, ["`acme/no-foo` threw on app.js: boom"]);
}

#[test]
fn project_findings_sort_among_the_rest_by_place() {
    let at = |path: &str, line: usize, rule: &'static str| Diagnostic {
        rule,
        severity: Severity::Error,
        path: Some(path.to_owned()),
        line,
        column: 1,
        message: String::new(),
    };
    let mut diagnostics = vec![
        at("b.js", 1, "acme/no-foo"),
        at("a.js", 9, "flow/unclear-type"),
        at("a.js", 2, "acme/no-foo"),
    ];

    sort(&mut diagnostics);

    let order: Vec<(Option<&str>, usize)> = diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.path.as_deref(), diagnostic.line))
        .collect();
    assert_eq!(
        order,
        [(Some("a.js"), 2), (Some("a.js"), 9), (Some("b.js"), 1)]
    );
}
