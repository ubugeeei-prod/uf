use serde_json::json;

use super::*;

#[test]
fn a_rendered_location_becomes_a_one_based_byte_span() {
    let value = json!({
        "source": "app.js",
        "type": "SourceFile",
        "start": { "line": 2, "column": 19 },
        "end": { "line": 2, "column": 32 },
    });

    let span = span(Some(&value)).expect("a located span");

    assert_eq!(span.path, "app.js");
    assert_eq!(
        span.start,
        Position {
            line: 2,
            column: 19
        }
    );
    // Flow reports the end column exclusive and zero-based; making it one-based
    // is what lets `end - start` be the byte length.
    assert_eq!(
        span.end,
        Position {
            line: 2,
            column: 33
        }
    );
    assert_eq!(span.single_line_len(), Some(14));
}

#[test]
fn a_location_without_a_source_is_dropped() {
    let value = json!({
        "source": null,
        "type": null,
        "start": { "line": 0, "column": 1 },
        "end": { "line": 0, "column": 0 },
    });

    assert_eq!(span(Some(&value)), None);
}

#[test]
fn a_missing_location_is_dropped() {
    assert_eq!(span(None), None);
    assert_eq!(span(Some(&json!(null))), None);
    assert_eq!(span(Some(&json!({ "source": "app.js" }))), None);
}

#[test]
fn negative_positions_clamp_instead_of_wrapping() {
    let value = json!({
        "source": "app.js",
        "start": { "line": -1, "column": -1 },
        "end": { "line": -1, "column": -1 },
    });

    let span = span(Some(&value)).expect("a located span");

    assert_eq!(span.start, Position { line: 0, column: 0 });
}

#[test]
fn flat_markup_becomes_typed_segments() {
    let markup = json!([
        { "kind": "Text", "text": "Cannot assign " },
        { "kind": "Code", "text": "\"x\"" },
        { "kind": "Reference", "referenceId": "1", "message": [
            { "kind": "Code", "text": "number" },
        ]},
    ]);

    let segments = segments(Some(&markup));

    assert_eq!(segments.len(), 3);
    assert!(matches!(&segments[0], MessageSegment::Text { text } if text == "Cannot assign "));
    assert!(matches!(&segments[1], MessageSegment::Code { text } if text == "\"x\""));
    assert!(
        matches!(&segments[2], MessageSegment::Reference { text, id } if text == "number" && *id == 1)
    );
}

#[test]
fn a_reference_joins_every_inline_fragment_it_carries() {
    let markup = json!([
        { "kind": "Reference", "referenceId": "2", "message": [
            { "kind": "Text", "text": "the " },
            { "kind": "Code", "text": "Props" },
        ]},
    ]);

    let segments = segments(Some(&markup));

    assert!(
        matches!(&segments[0], MessageSegment::Reference { text, id } if text == "the Props" && *id == 2)
    );
}

#[test]
fn grouped_markup_is_flattened_with_separators() {
    let markup = json!({
        "kind": "UnorderedList",
        "message": [{ "kind": "Text", "text": "head" }],
        "items": [
            [{ "kind": "Text", "text": "one" }],
            [{ "kind": "Text", "text": "two" }],
        ],
        "post_message": [{ "kind": "Text", "text": "tail" }],
    });

    let text: String = segments(Some(&markup))
        .iter()
        .map(MessageSegment::text)
        .collect();

    assert_eq!(text, "head one two tail");
}

#[test]
fn markup_recursion_is_bounded() {
    let mut markup = json!([{ "kind": "Text", "text": "deep" }]);
    for _ in 0..(MAX_MARKUP_DEPTH as usize * 4) {
        markup = json!({ "kind": "UnorderedList", "message": markup, "items": [] });
    }

    // The point is that this terminates at all; the bound also keeps the
    // result small rather than letting a nested union render forever.
    assert!(segments(Some(&markup)).len() <= MAX_MARKUP_DEPTH as usize);
}

#[test]
fn unknown_markup_kinds_are_ignored_rather_than_guessed_at() {
    let markup = json!([
        { "kind": "SomethingNew", "text": "?" },
        { "text": "no kind at all" },
        { "kind": "Text", "text": "kept" },
    ]);

    let segments = segments(Some(&markup));

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text(), "kept");
}

#[test]
fn reference_locations_come_out_in_reference_order() {
    let references = json!({
        "10": { "source": "b.js", "start": { "line": 1, "column": 1 }, "end": { "line": 1, "column": 2 } },
        "2": { "source": "a.js", "start": { "line": 3, "column": 1 }, "end": { "line": 3, "column": 2 } },
    });

    let related = related(Some(&references));

    assert_eq!(related.len(), 2);
    assert_eq!(related[0].id, 2);
    assert_eq!(related[0].span.path, "a.js");
    assert_eq!(related[1].id, 10);
}

#[test]
fn unlocatable_references_are_dropped_rather_than_faked() {
    let references = json!({
        "1": { "source": null, "start": { "line": 0, "column": 1 }, "end": { "line": 0, "column": 0 } },
        "not-a-number": { "source": "a.js", "start": { "line": 1, "column": 1 }, "end": { "line": 1, "column": 2 } },
    });

    assert!(related(Some(&references)).is_empty());
}

#[test]
fn missing_references_are_an_empty_list() {
    assert!(related(None).is_empty());
    assert!(related(Some(&json!([]))).is_empty());
}

#[test]
fn every_flow_error_kind_maps_onto_a_uf_kind() {
    use flow_common_errors::error_utils::InferWarningKind;
    use flow_lint_settings::lints::LintKind;

    assert_eq!(kind(ErrorKind::ParseError), DiagnosticKind::Parse);
    assert_eq!(kind(ErrorKind::PseudoParseError), DiagnosticKind::Parse);
    assert_eq!(kind(ErrorKind::InferError), DiagnosticKind::Infer);
    assert_eq!(
        kind(ErrorKind::InferWarning(InferWarningKind::OtherKind)),
        DiagnosticKind::Infer
    );
    assert_eq!(kind(ErrorKind::InternalError), DiagnosticKind::Internal);
    assert_eq!(
        kind(ErrorKind::DuplicateProviderError),
        DiagnosticKind::DuplicateProvider
    );
    assert_eq!(
        kind(ErrorKind::RecursionLimitError),
        DiagnosticKind::RecursionLimit
    );
    assert_eq!(
        kind(ErrorKind::LintError(LintKind::UnclearType)),
        DiagnosticKind::Lint
    );
}

#[test]
fn no_errors_means_no_diagnostics() {
    let empty = ConcreteLocPrintableErrorSet::empty();

    assert!(diagnostics(&empty, &empty, "app.js").is_empty());
}
