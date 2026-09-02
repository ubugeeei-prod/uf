//! The `uf check` payload, and how the two halves of it are combined.

use uf_lint::Diagnostic;

use super::*;

fn lint_report(errors: usize, warnings: usize) -> LintReport {
    let mut diagnostics = Vec::new();
    for index in 0..errors {
        diagnostics.push(diagnostic(index, Severity::Error));
    }
    for index in 0..warnings {
        diagnostics.push(diagnostic(index, Severity::Warn));
    }
    LintReport {
        diagnostics,
        files_checked: 2,
        unavailable: Vec::new(),
    }
}

fn diagnostic(index: usize, severity: Severity) -> Diagnostic {
    Diagnostic {
        rule: "flow/unclear-type",
        severity,
        path: Some("a.js".to_string()),
        line: index + 1,
        column: 1,
        message: "unclear type".to_string(),
    }
}

#[test]
fn the_payload_keeps_the_shape_uf_lint_emits() {
    let value = payload(&lint_report(1, 1), &TypeCheck::Unavailable);

    assert_eq!(value["command"], json!("uf check"));
    assert_eq!(value["filesChecked"], json!(2));
    assert_eq!(value["errors"], json!(1));
    assert_eq!(value["warnings"], json!(1));
    assert!(value["diagnostics"].is_array());
    assert!(value["unavailableRules"].is_array());
}

#[test]
fn the_payload_names_the_checker_backend_and_its_status() {
    let value = payload(&lint_report(0, 0), &TypeCheck::Unavailable);

    assert_eq!(value["typeCheck"]["status"], json!("unavailable"));
    assert_eq!(
        value["typeCheck"]["backend"],
        json!(backend_name(active_backend()))
    );
    assert_eq!(value["typeCheck"]["diagnostics"], json!([]));
}

#[test]
fn a_checker_failure_is_reported_without_losing_the_lint_counts() {
    let types = TypeCheck::Failed(CheckError::SourceTooLarge {
        path: "big.js".into(),
        size: 8,
        limit: 4,
    });

    let value = payload(&lint_report(2, 0), &types);

    assert_eq!(value["errors"], json!(2));
    assert_eq!(value["typeCheck"]["status"], json!("failed"));
    assert!(
        value["typeCheck"]["error"]
            .as_str()
            .expect("an error string")
            .contains("big.js")
    );
}

#[test]
fn an_unavailable_checker_contributes_no_counts() {
    let types = TypeCheck::Unavailable;

    assert_eq!(types.count(uf_check::Severity::Error), 0);
    assert_eq!(types.count(uf_check::Severity::Warning), 0);
    assert!(types.diagnostics().is_empty());
    assert!(types.report().is_none());
}

#[test]
fn statuses_are_stable() {
    assert_eq!(TypeCheck::Unavailable.status(), "unavailable");
    assert_eq!(
        TypeCheck::Failed(CheckError::Unavailable).status(),
        "failed"
    );
}

#[test]
fn a_build_with_a_checker_reports_it_and_one_without_says_so() {
    let types = type_check(&[]);

    if uf_check::is_available() {
        assert_eq!(types.status(), "checked");
        assert_eq!(
            value_of(&types)["filesChecked"],
            json!(0),
            "an empty project checks zero files"
        );
    } else {
        assert_eq!(types.status(), "unavailable");
    }
}

fn value_of(types: &TypeCheck) -> Value {
    payload(&lint_report(0, 0), types)["typeCheck"].clone()
}
