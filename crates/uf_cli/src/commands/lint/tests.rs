//! Diagnostic grouping, caret spans, and the machine payload.

use super::*;

fn diagnostic(path: &str, line: usize, severity: Severity) -> Diagnostic {
    Diagnostic {
        rule: "flow/unclear-type",
        severity,
        path: Some(path.to_string()),
        line,
        column: 1,
        message: "unclear type".to_string(),
    }
}

#[test]
fn diagnostics_group_into_runs_that_share_a_path() {
    let diagnostics = vec![
        diagnostic("a.js", 1, Severity::Error),
        diagnostic("a.js", 2, Severity::Warn),
        diagnostic("b.js", 1, Severity::Warn),
    ];
    let groups = group_by_path(&diagnostics);

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].len(), 2);
    assert_eq!(groups[1].len(), 1);
}

#[test]
fn grouping_an_empty_report_yields_no_groups() {
    assert!(group_by_path(&[]).is_empty());
}

#[test]
fn grouping_keeps_a_single_file_together() {
    let diagnostics = vec![
        diagnostic("a.js", 1, Severity::Error),
        diagnostic("a.js", 2, Severity::Error),
    ];
    assert_eq!(group_by_path(&diagnostics).len(), 1);
}

#[test]
fn a_missing_path_reports_as_memory() {
    let mut diagnostic = diagnostic("a.js", 1, Severity::Error);
    diagnostic.path = None;
    assert_eq!(diagnostic_path(&diagnostic), "<memory>");
}

#[test]
fn identifier_spans_cover_the_offending_token() {
    assert_eq!(identifier_span("const value: any = 1;", 14), 3);
    assert_eq!(identifier_span("const value: any = 1;", 7), 5);
    assert_eq!(identifier_span("const x = $value;", 11), 6);
}

#[test]
fn identifier_spans_never_collapse_to_zero() {
    assert_eq!(identifier_span("const x = 1;", 9), 1, "punctuation");
    assert_eq!(identifier_span("", 1), 1, "empty line");
    assert_eq!(identifier_span("abc", 999), 1, "past the end");
    assert_eq!(identifier_span("abc", 0), 3, "zero column");
}

#[test]
fn identifier_spans_stop_at_multibyte_characters() {
    assert_eq!(identifier_span("const 日本 = 1;", 7), 1);
}

#[test]
fn severity_counts_split_errors_from_warnings() {
    let report = LintReport {
        diagnostics: vec![
            diagnostic("a.js", 1, Severity::Error),
            diagnostic("a.js", 2, Severity::Warn),
            diagnostic("a.js", 3, Severity::Warn),
        ],
        files_checked: 1,
        unavailable: Vec::new(),
    };

    assert_eq!(severity_count(&report, Severity::Error), 1);
    assert_eq!(severity_count(&report, Severity::Warn), 2);
}

#[test]
fn the_lint_payload_is_machine_shaped() {
    let report = LintReport {
        diagnostics: vec![diagnostic("a.js", 3, Severity::Error)],
        files_checked: 2,
        unavailable: Vec::new(),
    };
    let payload = lint_payload(LintCommand::Lint, &report);

    assert_eq!(payload["command"], json!("uf lint"));
    assert_eq!(payload["filesChecked"], json!(2));
    assert_eq!(payload["errors"], json!(1));
    assert_eq!(payload["warnings"], json!(0));
    assert_eq!(payload["diagnostics"][0]["severity"], json!("error"));
    assert_eq!(payload["diagnostics"][0]["line"], json!(3));
    assert!(!payload.to_string().contains('\u{1b}'));
}

#[test]
fn lint_command_titles_name_the_command() {
    assert_eq!(LintCommand::Lint.title(), "uf lint");
    assert_eq!(LintCommand::Check.title(), "uf check");
}

#[test]
fn no_pattern_selects_everything() {
    assert!(selects(&[], "packages/ui/dialog.js"));
}

#[test]
fn a_pattern_selects_by_substring_of_the_relative_path() {
    let patterns = vec!["packages/ui".to_string()];

    assert!(selects(&patterns, "packages/ui/dialog.js"));
    assert!(!selects(&patterns, "packages/form/use-form.js"));
}

#[test]
fn any_of_several_patterns_selects() {
    let patterns = vec!["packages/ui".to_string(), "tests/".to_string()];

    assert!(selects(&patterns, "tests/library/ui.test.js"));
    assert!(selects(&patterns, "packages/ui/menu.js"));
    assert!(!selects(&patterns, "packages/form/rules.js"));
}

#[test]
fn a_bare_file_name_selects_the_file_wherever_it_is() {
    // The whole point of substring matching: `uf lint dialog.js` is how a
    // reader asks about the file in front of them, without its directory.
    let patterns = vec!["dialog.js".to_string()];

    assert!(selects(&patterns, "packages/ui/dialog.js"));
}

#[test]
fn patterns_that_matched_nothing_are_named_in_order() {
    let patterns = vec!["a".to_string(), "b".to_string(), "c".to_string()];

    assert_eq!(quoted_list(&patterns), "`a`, `b` and `c`");
    assert_eq!(quoted_list(&patterns[..1]), "`a`");
    assert_eq!(quoted_list(&patterns[..2]), "`a` and `b`");
}
