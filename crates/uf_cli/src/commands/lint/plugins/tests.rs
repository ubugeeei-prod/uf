//! Which enabled ids are a project's rules, how a host's reply becomes
//! diagnostics and fixes, and how those fixes are planned and applied.

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

fn fix(start: usize, end: usize, text: &str, safety: Safety) -> ProjectFix {
    ProjectFix {
        rule: intern("acme/no-foo"),
        safety,
        start,
        end,
        text: text.to_owned(),
        line: 1,
        column: start + 1,
    }
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
fn a_reply_becomes_diagnostics_and_fixes_at_the_level_the_project_set() {
    // `é` before the finding, so a fix range counted in UTF-16 units lands on
    // the right bytes only if it was converted.
    let file = SourceFile {
        path: String::from("app.js"),
        source: String::from("// é\n\nconst foo = 1;\n"),
    };
    let levels: FxHashMap<&'static str, RuleLevel> = [(intern("acme/no-foo"), RuleLevel::Warn)]
        .into_iter()
        .collect();
    let reply = json!({
        "type": "linted",
        "diagnostics": [
            {
                "rule": "acme/no-foo", "message": "no foo", "start": 12, "end": 15,
                "fix": { "start": 12, "end": 15, "text": "bar", "kind": "code" },
            },
            { "rule": "other/rule", "message": "not enabled here", "start": 0, "end": 1, "fix": null },
        ],
        "micros": { "acme/no-foo": 12.5, "other/rule": 99.0 },
        "problems": ["`acme/no-foo` threw on app.js: boom"],
    });
    let mut answer = Answer::default();
    let mut problems = Vec::new();
    let mut micros = FxHashMap::default();

    collect(
        &reply,
        &file,
        &levels,
        &mut answer,
        &mut problems,
        &mut micros,
    );

    assert_eq!(
        answer.diagnostics.len(),
        1,
        "a rule the project did not enable reports nothing"
    );
    let diagnostic = &answer.diagnostics[0];
    assert_eq!(diagnostic.rule, "acme/no-foo");
    assert_eq!((diagnostic.line, diagnostic.column), (3, 7));
    assert_eq!(diagnostic.severity, Severity::Warn);
    let fix = &answer.fixes[0];
    assert_eq!(&file.source[fix.start..fix.end], "foo");
    assert_eq!(
        fix.safety,
        Safety::Unsafe,
        "`code` fixes wait to be asked for"
    );
    assert_eq!((fix.line, fix.column), (3, 7));
    assert_eq!(micros.len(), 1);
    assert_eq!(micros.get("acme/no-foo"), Some(&12.5));
    assert_eq!(problems, ["`acme/no-foo` threw on app.js: boom"]);
}

#[test]
fn a_whitespace_fix_is_safe_and_a_code_fix_waits_for_fix_unsafe() {
    let fixes = [fix(0, 1, "a", Safety::Unsafe), fix(4, 5, " ", Safety::Safe)];

    let safe: Vec<usize> = plan_fixes(&fixes, false)
        .iter()
        .map(|fix| fix.start)
        .collect();
    let both: Vec<usize> = plan_fixes(&fixes, true)
        .iter()
        .map(|fix| fix.start)
        .collect();

    assert_eq!(safe, [4]);
    assert_eq!(both, [0, 4]);
}

#[test]
fn overlapping_fixes_keep_the_first_and_apply_on_the_right_bytes() {
    let fixes = [
        fix(6, 9, "bar", Safety::Unsafe),
        fix(0, 5, "let", Safety::Unsafe),
        fix(7, 8, "x", Safety::Unsafe),
    ];

    let planned = plan_fixes(&fixes, true);

    assert_eq!(
        planned.len(),
        2,
        "the edit inside `foo` overlaps the one over it"
    );
    assert_eq!(apply_fixes("const foo = 1;", &planned), "let bar = 1;");
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
