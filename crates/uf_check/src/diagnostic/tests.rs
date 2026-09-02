use compact_str::CompactString;
use smallvec::smallvec;

use super::*;

fn span(start: (u32, u32), end: (u32, u32)) -> Span {
    Span {
        path: CompactString::const_new("app.js"),
        start: Position {
            line: start.0,
            column: start.1,
        },
        end: Position {
            line: end.0,
            column: end.1,
        },
    }
}

fn diagnostic(message: MessageFeatures, related: RelatedLocations) -> TypeDiagnostic {
    TypeDiagnostic {
        severity: Severity::Error,
        kind: DiagnosticKind::Infer,
        code: Some("incompatible-type"),
        primary: span((2, 19), (2, 33)),
        root: None,
        message,
        related,
    }
}

#[test]
fn severity_ids_are_stable() {
    assert_eq!(Severity::Error.as_str(), "error");
    assert_eq!(Severity::Warning.as_str(), "warning");
}

#[test]
fn diagnostic_kind_ids_are_stable() {
    assert_eq!(DiagnosticKind::Parse.as_str(), "parse");
    assert_eq!(DiagnosticKind::Infer.as_str(), "infer");
    assert_eq!(DiagnosticKind::Lint.as_str(), "lint");
    assert_eq!(DiagnosticKind::Internal.as_str(), "internal");
    assert_eq!(
        DiagnosticKind::DuplicateProvider.as_str(),
        "duplicate-provider"
    );
    assert_eq!(DiagnosticKind::RecursionLimit.as_str(), "recursion-limit");
}

#[test]
fn a_span_on_one_line_reports_its_byte_length() {
    assert_eq!(span((2, 19), (2, 33)).single_line_len(), Some(14));
}

#[test]
fn an_empty_span_still_covers_one_column() {
    assert_eq!(span((2, 19), (2, 19)).single_line_len(), Some(1));
}

#[test]
fn a_multi_line_span_has_no_single_line_length() {
    assert_eq!(span((2, 19), (5, 3)).single_line_len(), None);
}

#[test]
fn a_reversed_span_does_not_underflow() {
    assert_eq!(span((2, 33), (2, 19)).single_line_len(), Some(1));
}

#[test]
fn message_segments_expose_their_text() {
    assert_eq!(
        MessageSegment::Text {
            text: CompactString::const_new("Cannot assign ")
        }
        .text(),
        "Cannot assign "
    );
    assert_eq!(
        MessageSegment::Code {
            text: CompactString::const_new("string")
        }
        .text(),
        "string"
    );
    assert_eq!(
        MessageSegment::Reference {
            text: CompactString::const_new("number"),
            id: 1
        }
        .text(),
        "number"
    );
}

#[test]
fn rendering_a_message_marks_each_reference() {
    let diagnostic = diagnostic(
        smallvec![
            MessageSegment::Text {
                text: CompactString::const_new("Cannot assign "),
            },
            MessageSegment::Code {
                text: CompactString::const_new("\"x\""),
            },
            MessageSegment::Text {
                text: CompactString::const_new(" to "),
            },
            MessageSegment::Reference {
                text: CompactString::const_new("number"),
                id: 1,
            },
        ],
        RelatedLocations::new(),
    );

    assert_eq!(
        diagnostic.message_text(),
        "Cannot assign \"x\" to number [1]"
    );
}

#[test]
fn rendering_an_empty_message_yields_an_empty_string() {
    assert_eq!(
        diagnostic(MessageFeatures::new(), RelatedLocations::new()).message_text(),
        ""
    );
}

#[test]
fn rendering_handles_multi_digit_and_zero_reference_ids() {
    let diagnostic = diagnostic(
        smallvec![
            MessageSegment::Reference {
                text: CompactString::const_new("a"),
                id: 0,
            },
            MessageSegment::Reference {
                text: CompactString::const_new("b"),
                id: 1234,
            },
        ],
        RelatedLocations::new(),
    );

    assert_eq!(diagnostic.message_text(), "a [0]b [1234]");
}

#[test]
fn rendering_a_message_is_idempotent() {
    let diagnostic = diagnostic(
        smallvec![MessageSegment::Text {
            text: CompactString::const_new("boom"),
        }],
        RelatedLocations::new(),
    );

    assert_eq!(diagnostic.message_text(), diagnostic.message_text());
}

#[test]
fn rendering_keeps_non_ascii_message_text_intact() {
    let diagnostic = diagnostic(
        smallvec![MessageSegment::Code {
            text: CompactString::const_new("\"日本語\""),
        }],
        RelatedLocations::new(),
    );

    assert_eq!(diagnostic.message_text(), "\"日本語\"");
}

#[test]
fn only_errors_fail_the_run() {
    let mut diagnostic = diagnostic(MessageFeatures::new(), RelatedLocations::new());
    assert!(diagnostic.is_error());

    diagnostic.severity = Severity::Warning;
    assert!(!diagnostic.is_error());
}

#[test]
fn diagnostics_serialize_with_camel_case_keys_and_kebab_case_enums() {
    let diagnostic = diagnostic(
        smallvec![MessageSegment::Reference {
            text: CompactString::const_new("number"),
            id: 1,
        }],
        smallvec![RelatedLocation {
            id: 1,
            span: span((3, 1), (3, 7)),
        }],
    );

    let json = serde_json::to_value(&diagnostic).expect("serializes");

    assert_eq!(json["severity"], serde_json::json!("error"));
    assert_eq!(json["kind"], serde_json::json!("infer"));
    assert_eq!(json["code"], serde_json::json!("incompatible-type"));
    assert_eq!(json["primary"]["start"]["line"], serde_json::json!(2));
    assert_eq!(json["message"][0]["kind"], serde_json::json!("reference"));
    assert_eq!(json["related"][0]["id"], serde_json::json!(1));
}

#[test]
fn diagnostics_order_by_severity_then_kind() {
    let mut error = diagnostic(MessageFeatures::new(), RelatedLocations::new());
    let mut warning = error.clone();
    warning.severity = Severity::Warning;
    error.kind = DiagnosticKind::Infer;

    assert!(error < warning);
}
