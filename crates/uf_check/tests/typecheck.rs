//! End-to-end checks that `uf` really runs Flow's own inference.
//!
//! Assertions are on error **codes** and **locations**, never on message text:
//! the codes are Flow's public contract and the messages are not, so asserting
//! on prose would turn every upstream wording change into a red build.

#![cfg(feature = "upstream-typecheck")]

use std::time::Duration;

use uf_check::{
    CheckError, CheckLimits, DiagnosticKind, Severity, Source, TypeDiagnostic, check_source,
    check_sources, prepare_builtins,
};

const CLEAN_COMPONENT: &str = include_str!("fixtures/clean_component.js");
const STRING_TO_NUMBER: &str = include_str!("fixtures/string_to_number.js");
const MISSING_PROP: &str = include_str!("fixtures/missing_prop.js");
const BAD_RENDERS: &str = include_str!("fixtures/bad_renders.js");
const UNHANDLED_NULL: &str = include_str!("fixtures/unhandled_null.js");
const TOP_LEVEL_AWAIT: &str = include_str!("fixtures/top_level_await.js");
const TOP_LEVEL_AWAIT_MISUSE: &str = include_str!("fixtures/top_level_await_misuse.js");

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(Source::new(path, source), &[], &limits()).expect("the checker runs")
}

fn codes(diagnostics: &[TypeDiagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.unwrap_or("<none>"))
        .collect()
}

fn find<'a>(diagnostics: &'a [TypeDiagnostic], code: &str) -> &'a TypeDiagnostic {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == Some(code))
        .unwrap_or_else(|| {
            panic!(
                "expected a `{code}` diagnostic, got {:?}",
                codes(diagnostics)
            )
        })
}

#[test]
fn a_modern_flow_react_file_checks_clean() {
    let diagnostics = check("clean_component.js", CLEAN_COMPONENT);

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {:#?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.primary.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_module_that_awaits_at_its_top_level_checks_clean() {
    let diagnostics = check("top_level_await.js", TOP_LEVEL_AWAIT);

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {:#?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.primary.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_top_level_await_has_the_type_the_promise_resolves_to() {
    // Not merely "it parses": the awaited value is a number, and a module
    // that calls it a string is wrong in the same way as any other bad
    // annotation. The caret belongs on the awaited expression.
    let diagnostics = check("top_level_await_misuse.js", TOP_LEVEL_AWAIT_MISUSE);

    let diagnostic = find(&diagnostics, "incompatible-type");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.kind, DiagnosticKind::Infer);
    assert_eq!(diagnostic.primary.path, "top_level_await_misuse.js");
    // `const retries: string = await Promise.resolve(3);`
    assert_eq!(diagnostic.primary.start.line, 6);
}

#[test]
fn assigning_a_string_to_a_number_is_an_incompatible_type_at_the_literal() {
    let diagnostics = check("string_to_number.js", STRING_TO_NUMBER);

    let diagnostic = find(&diagnostics, "incompatible-type");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.kind, DiagnosticKind::Infer);
    assert_eq!(diagnostic.primary.path, "string_to_number.js");
    // `const total: number = "twelve";` — the caret belongs on the literal.
    assert_eq!(diagnostic.primary.start.line, 2);
    assert_eq!(diagnostic.primary.start.column, 23);
    assert_eq!(diagnostic.primary.single_line_len(), Some(8));
}

#[test]
fn an_incompatible_type_keeps_the_locations_its_message_refers_to() {
    let diagnostics = check("string_to_number.js", STRING_TO_NUMBER);

    let diagnostic = find(&diagnostics, "incompatible-type");
    assert!(
        !diagnostic.related.is_empty(),
        "expected the annotation's location to be kept as a reference"
    );
    // Every reference the message points at must have a location, and every
    // location must be one the message points at.
    let referenced: Vec<u32> = diagnostic
        .message
        .iter()
        .filter_map(|segment| match segment {
            uf_check::MessageSegment::Reference { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    for related in &diagnostic.related {
        assert!(
            referenced.contains(&related.id),
            "reference {} has a location but no mention in the message",
            related.id
        );
    }
    assert!(
        diagnostic
            .related
            .iter()
            .any(|related| related.span.start.line == 2)
    );
}

#[test]
fn omitting_a_required_prop_is_an_error_on_the_element_that_points_at_the_component() {
    let diagnostics = check("missing_prop.js", MISSING_PROP);

    assert_eq!(diagnostics.len(), 1, "{:?}", codes(&diagnostics));
    let diagnostic = find(&diagnostics, "incompatible-type");
    assert_eq!(diagnostic.severity, Severity::Error);
    assert_eq!(diagnostic.primary.path, "missing_prop.js");
    // `  return <Greeting name="world" />;` — the caret belongs on the element.
    assert_eq!(diagnostic.primary.start.line, 9);
    assert_eq!(diagnostic.primary.start.column, 11);
    // ...and one of the references must be the component that declares `times`.
    assert!(
        diagnostic
            .related
            .iter()
            .any(|related| related.span.start.line == 4),
        "expected a reference to the component declaration, got {:?}",
        diagnostic.related
    );
}

#[test]
fn an_invalid_renders_type_is_reported_as_an_invalid_render() {
    let diagnostics = check("bad_renders.js", BAD_RENDERS);

    assert_eq!(diagnostics.len(), 1, "{:?}", codes(&diagnostics));
    let diagnostic = find(&diagnostics, "invalid-render");
    assert_eq!(diagnostic.severity, Severity::Error);
    // `export component Broken() renders NotAComponent {` — on the type.
    assert_eq!(diagnostic.primary.start.line, 6);
    assert_eq!(diagnostic.primary.start.column, 35);
    assert_eq!(diagnostic.primary.single_line_len(), Some(13));
}

#[test]
fn dereferencing_a_maybe_string_is_an_incompatible_use() {
    let diagnostics = check("unhandled_null.js", UNHANDLED_NULL);

    assert_eq!(diagnostics.len(), 1, "{:?}", codes(&diagnostics));
    let diagnostic = find(&diagnostics, "incompatible-use");
    // `  return label.length;` — the caret belongs on the property.
    assert_eq!(diagnostic.primary.start.line, 3);
    assert_eq!(diagnostic.primary.start.column, 16);
    assert_eq!(diagnostic.primary.single_line_len(), Some(6));
    // The reference points back at the `?string` annotation.
    assert_eq!(diagnostic.related[0].span.start.line, 2);
}

#[test]
fn a_message_does_not_repeat_the_error_code_the_diagnostic_already_carries() {
    let diagnostics = check("string_to_number.js", STRING_TO_NUMBER);

    let diagnostic = find(&diagnostics, "incompatible-type");
    let text = diagnostic.message_text();

    assert!(!text.contains("[incompatible-type]"), "{text}");
    assert!(text.ends_with('.'), "{text}");
}

#[test]
fn every_diagnostic_carries_a_code_and_a_located_primary_span() {
    for (path, source) in [
        ("string_to_number.js", STRING_TO_NUMBER),
        ("missing_prop.js", MISSING_PROP),
        ("bad_renders.js", BAD_RENDERS),
        ("unhandled_null.js", UNHANDLED_NULL),
    ] {
        for diagnostic in check(path, source) {
            assert!(
                diagnostic.code.is_some(),
                "{path} produced an uncoded error"
            );
            assert_eq!(diagnostic.primary.path, path);
            assert!(diagnostic.primary.start.line >= 1);
            assert!(diagnostic.primary.start.column >= 1);
            assert!(!diagnostic.message.is_empty());
        }
    }
}

#[test]
fn checking_the_same_input_twice_is_byte_identical() {
    let first = check("missing_prop.js", MISSING_PROP);
    let second = check("missing_prop.js", MISSING_PROP);

    assert_eq!(
        serde_json::to_string(&first).expect("serializes"),
        serde_json::to_string(&second).expect("serializes"),
    );
}

#[test]
fn a_batch_reports_diagnostics_in_the_order_it_was_given() {
    let sources = [
        Source::new("a.js", STRING_TO_NUMBER),
        Source::new("b.js", UNHANDLED_NULL),
    ];

    let report = check_sources(&sources, &[], &limits()).expect("the checker runs");

    assert_eq!(report.files_checked, 2);
    assert!(report.has_errors());
    let paths: Vec<&str> = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.primary.path.as_str())
        .collect();
    let first_b = paths.iter().position(|path| *path == "b.js");
    let last_a = paths.iter().rposition(|path| *path == "a.js");
    assert!(matches!((first_b, last_a), (Some(b), Some(a)) if a < b));
}

#[test]
fn a_clean_file_costs_nothing_to_report() {
    let report = check_sources(&[Source::new("clean.js", CLEAN_COMPONENT)], &[], &limits())
        .expect("the checker runs");

    assert_eq!(report.count(Severity::Error), 0);
    assert!(!report.has_errors());
}

#[test]
fn a_syntax_error_comes_back_as_a_parse_diagnostic_rather_than_a_failure() {
    let diagnostics = check("broken.js", "// @flow\ntype = ;\n");

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.kind == DiagnosticKind::Parse),
        "{:?}",
        codes(&diagnostics)
    );
    assert!(!diagnostics.is_empty());
    assert_eq!(diagnostics[0].primary.start.line, 2);
}

#[test]
fn an_empty_file_is_clean() {
    assert!(check("empty.js", "").is_empty());
}

#[test]
fn a_file_with_a_bom_and_crlf_line_endings_still_reports_real_lines() {
    let source = "\u{feff}// @flow\r\nconst total: number = \"twelve\";\r\n";

    let diagnostics = check("crlf.js", source);

    let diagnostic = find(&diagnostics, "incompatible-type");
    assert_eq!(diagnostic.primary.start.line, 2);
}

#[test]
fn non_ascii_identifiers_report_byte_columns() {
    let source = "// @flow\nconst \u{e9}l\u{e9}ment: number = \"x\";\n";

    let diagnostics = check("non_ascii.js", source);

    let diagnostic = find(&diagnostics, "incompatible-type");
    assert_eq!(diagnostic.primary.start.line, 2);
    // "const élément: number = " is 24 characters but 26 bytes.
    assert_eq!(diagnostic.primary.start.column, 27);
}

#[test]
fn a_source_over_the_limit_is_rejected_before_it_is_parsed() {
    let limits = limits().with_max_source_bytes(64);
    let source = format!("// @flow\n{}\n", "const x: number = 1;".repeat(64));

    let error = check_source(Source::new("big.js", &source), &[], &limits)
        .expect_err("the limit must be enforced");

    assert!(matches!(
        error,
        CheckError::SourceTooLarge { limit: 64, .. }
    ));
}

#[test]
fn a_five_megabyte_file_is_rejected_by_the_default_limits() {
    let source = "// @flow\n".to_owned() + &"const x: number = 1;\n".repeat(250_000);
    assert!(source.len() > 5_000_000);

    let error = check_source(
        Source::new("huge.js", &source),
        &[],
        &CheckLimits::default(),
    )
    .expect_err("the default limit must be enforced");

    assert!(matches!(error, CheckError::SourceTooLarge { .. }));
}

#[test]
fn ten_thousand_nested_generics_terminate_instead_of_overflowing() {
    let depth = 10_000;
    let mut source = String::with_capacity(depth * 8 + 64);
    source.push_str("// @flow\ntype Box<T> = { value: T };\ntype Deep = ");
    for _ in 0..depth {
        source.push_str("Box<");
    }
    source.push_str("number");
    for _ in 0..depth {
        source.push('>');
    }
    source.push_str(";\nexport type Out = Deep;\n");

    // The bar is that this returns at all. Whether Flow reports a recursion
    // limit, a type error, or nothing is upstream's call; hanging or aborting
    // the process is not an option.
    let outcome = check_source(
        Source::new("nested.js", &source),
        &[],
        &limits().with_file_timeout(Duration::from_secs(60)),
    );

    match outcome {
        Ok(diagnostics) => assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.primary.path == "nested.js")
        ),
        Err(error) => assert!(
            matches!(
                error,
                CheckError::Budget { .. } | CheckError::SourceTooLarge { .. }
            ),
            "unexpected failure: {error}"
        ),
    }
}

/// Two versions of one package in one batch: `b` at the root and `b` again
/// inside `a`, declaring the same name as two different types.
///
/// ubugeeei-prod/uf#486. Node resolves a bare specifier by climbing from the
/// importing file, so the file inside `a` is typed against `a`'s own copy and
/// the application against the hoisted one — in the same batch, with no
/// dependence on which was handed over first.
const TWO_VERSIONS: [(&str, &str); 6] = [
    (
        "app.js",
        "// @flow\nimport type { Value } from \"b\";\nexport const v: Value = \"a string\";\n",
    ),
    (
        "node_modules/a/index.js",
        "// @flow\nimport type { Value } from \"b\";\nexport const v: Value = \"a string\";\n",
    ),
    (
        "node_modules/a/package.json",
        r#"{ "name": "a", "exports": { ".": "./index.js" } }"#,
    ),
    (
        "node_modules/a/node_modules/b/index.js",
        "// @flow\nexport type Value = string;\n",
    ),
    (
        "node_modules/a/node_modules/b/package.json",
        r#"{ "name": "b", "version": "1.0.0", "exports": { ".": "./index.js" } }"#,
    ),
    (
        "node_modules/b/index.js",
        "// @flow\nexport type Value = number;\n",
    ),
];

/// The hoisted `b`'s manifest, kept apart so a test can put it first or last.
const HOISTED_MANIFEST: (&str, &str) = (
    "node_modules/b/package.json",
    r#"{ "name": "b", "version": "2.0.0", "exports": { ".": "./index.js" } }"#,
);

/// The `(path, code)` of every diagnostic a batch reports.
fn reported(sources: &[Source<'_>]) -> Vec<(String, &'static str)> {
    check_sources(sources, &[], &limits())
        .expect("the checker runs")
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind != DiagnosticKind::Parse)
        .map(|diagnostic| {
            (
                diagnostic.primary.path.to_string(),
                diagnostic.code.unwrap_or("<none>"),
            )
        })
        .collect()
}

#[test]
fn a_nested_dependency_is_typed_against_the_copy_installed_beside_it() {
    let mut batch: Vec<Source<'_>> = TWO_VERSIONS
        .iter()
        .map(|(path, source)| Source::new(path, source))
        .collect();
    batch.push(Source::new(HOISTED_MANIFEST.0, HOISTED_MANIFEST.1));

    // `a`'s own `b` says `Value` is a string, so `a` is clean; the root's says
    // it is a number, so the application is not. One batch, two answers.
    assert_eq!(
        reported(&batch),
        [("app.js".to_owned(), "incompatible-type")]
    );
}

#[test]
fn which_copy_a_file_sees_does_not_depend_on_the_batch_order() {
    // The hoisted manifest first, then last. Before #486 the index kept
    // whichever manifest the batch happened to hold first, so these two orders
    // disagreed about `a`.
    let mut hoisted_first: Vec<Source<'_>> =
        vec![Source::new(HOISTED_MANIFEST.0, HOISTED_MANIFEST.1)];
    hoisted_first.extend(
        TWO_VERSIONS
            .iter()
            .map(|(path, source)| Source::new(path, source)),
    );
    let mut hoisted_last: Vec<Source<'_>> = TWO_VERSIONS
        .iter()
        .map(|(path, source)| Source::new(path, source))
        .collect();
    hoisted_last.push(Source::new(HOISTED_MANIFEST.0, HOISTED_MANIFEST.1));

    assert_eq!(reported(&hoisted_first), reported(&hoisted_last));
}

#[test]
fn builtins_are_merged_once_and_then_free() {
    let first = prepare_builtins(&[]).expect("builtins merge");
    let second = prepare_builtins(&[]).expect("builtins are cached");

    assert!(!second.cold, "the second call must not rebuild");
    assert_eq!(first.cold_elapsed, second.cold_elapsed);
    assert!(
        first.cold_elapsed > Duration::ZERO,
        "the merge must be measured"
    );
    assert!(
        second.elapsed < first.cold_elapsed,
        "a warm call ({:?}) must be cheaper than the merge ({:?})",
        second.elapsed,
        first.cold_elapsed
    );
}

#[test]
fn a_report_separates_the_builtin_cost_from_the_check_cost() {
    let report = check_sources(&[Source::new("clean.js", CLEAN_COMPONENT)], &[], &limits())
        .expect("the checker runs");

    assert_eq!(report.files_checked, 1);
    assert!(report.builtins.cold_elapsed > Duration::ZERO);
    assert!(report.files_per_second().is_some());
}
