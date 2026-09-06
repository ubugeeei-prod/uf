//! What a coverage number means, and the ways it could quietly be wrong.
//!
//! The fixture every mapping test is built on is a `component` — the case the
//! whole feature exists for. Its transform *moves lines*: the JSX runtime
//! import the compiler adds at the top pushes everything down by one, and the
//! destructuring preamble it writes for the component's props is a generated
//! line the author never typed. A report that counted the output would say
//! line 5 was covered when the author's line 4 is the one that ran, and would
//! report a line of code where the author wrote a prop list.
//!
//! Nothing here needs Node: a coverage document is JSON and a source map is
//! JSON, so the whole mapping is exercised from data. What needs Node is
//! whether the *real* transform produces the map this assumes, and that is
//! `crates/uf_cli/tests/coverage.rs`.

use camino::Utf8Path;
use oxc_sourcemap::SourceMapBuilder;
use serde_json::json;

use crate::coverage::{Coverage, CoverageScope, Metric, Ratio, Thresholds, parse_document};
use crate::reporters::{cobertura, lcov, text_rows};

/// The project every fixture pretends to live in.
const ROOT: &str = "/project";

/// What the author wrote.
///
/// ```text
/// 1  // @flow
/// 2  component Widget(count: number) {
/// 3    if (count > 0) {
/// 4      return <b>{count}</b>;
/// 5    }
/// 6    return null;
/// 7  }
/// ```
const AUTHOR_URL: &str = "file:///project/src/widget.js";

/// What `uf transform` printed for it — one line longer, and with a line in the
/// middle the author did not write.
const GENERATED: &str = "import { jsx as _jsx } from \"react/jsx-runtime\";\n\
                         function Widget($$props) {\n\
                         \x20 const { count } = $$props;\n\
                         \x20 if (count > 0) {\n\
                         \x20   return _jsx(\"b\", { children: count });\n\
                         \x20 }\n\
                         \x20 return null;\n\
                         }\n";

/// The offset of `needle` in the generated module.
fn at(needle: &str) -> u32 {
    u32::try_from(GENERATED.find(needle).expect("the fixture contains it")).expect("small file")
}

/// The offset just past `needle`.
fn past(needle: &str) -> u32 {
    at(needle) + u32::try_from(needle.len()).expect("small file")
}

/// Line lengths as Node measures them: without the terminator, one entry per
/// line, and a trailing empty one for a file that ends in a newline.
fn line_lengths(text: &str) -> Vec<usize> {
    text.split('\n').map(str::len).collect()
}

/// The map the printer would produce for [`GENERATED`].
///
/// One token per node the author wrote and none for the two the compiler
/// invented — the runtime import on generated line 0 and the props preamble on
/// generated line 2 — which is the invariant `docs/architecture.md` states and
/// the one every rule in `crate::coverage` leans on.
fn source_map() -> serde_json::Value {
    let mut builder = SourceMapBuilder::default();
    let source = builder.add_source_and_content(AUTHOR_URL, "");
    // (generated line, generated column) -> (author line, author column), both
    // zero-based, as a source map counts.
    for (dst_line, dst_col, src_line, src_col) in [
        (1_u32, 0_u32, 1_u32, 0_u32), // function Widget   <- component Widget
        (3, 2, 2, 2),                 // if (count > 0)    <- if (count > 0)
        (4, 4, 3, 4),                 // return _jsx(...)  <- return <b>…</b>
        (6, 2, 5, 2),                 // return null       <- return null
    ] {
        builder.add_token(dst_line, dst_col, src_line, src_col, Some(source), None);
    }
    serde_json::from_str(&builder.into_sourcemap().to_json_string()).expect("a map is JSON")
}

/// A document in which `Widget` ran three times and its `if` body never did.
fn widget_document() -> String {
    document(&[
        // The module's own top level. V8 reports it as a nameless function at
        // offset zero, and it is not one.
        json!({
            "functionName": "",
            "isBlockCoverage": true,
            "ranges": [{
                "startOffset": 0,
                "endOffset": GENERATED.len(),
                "count": 1,
            }],
        }),
        json!({
            "functionName": "Widget",
            "isBlockCoverage": true,
            "ranges": [
                {
                    "startOffset": at("function Widget"),
                    "endOffset": past("  return null;\n}"),
                    "count": 3,
                },
                {
                    "startOffset": at("{\n    return _jsx"),
                    "endOffset": past("count });\n  }"),
                    "count": 0,
                },
            ],
        }),
    ])
}

/// One coverage document over the fixture, with these functions.
fn document(functions: &[serde_json::Value]) -> String {
    json!({
        "result": [{
            "scriptId": "1",
            "url": AUTHOR_URL,
            "functions": functions,
        }],
        "source-map-cache": {
            AUTHOR_URL: {
                "lineLengths": line_lengths(GENERATED),
                "data": source_map(),
                "url": null,
            },
        },
    })
    .to_string()
}

fn widget() -> Coverage {
    parse_document(&widget_document(), Utf8Path::new(ROOT)).expect("the fixture parses")
}

#[test]
fn counts_land_on_the_authors_lines_and_not_the_transforms() {
    let coverage = widget();
    let file = coverage
        .file("src/widget.js")
        .expect("the fixture's only file");

    // The author's lines, not the generated ones. Every number here is one
    // less than the generated line it came from, because the compiler's
    // runtime import moved the file down — which is the whole reason this
    // mapping exists.
    let lines: Vec<(u32, u64)> = file.lines().collect();
    assert_eq!(lines, vec![(2, 3), (3, 3), (4, 0), (6, 3)]);
}

#[test]
fn a_line_the_compiler_invented_is_not_a_line_of_the_authors() {
    let coverage = widget();
    let file = coverage.file("src/widget.js").expect("the fixture's file");
    let covered: Vec<u32> = file.lines().map(|(line, _)| line).collect();

    // Generated line 2 is `const { count } = $$props;`, which the author did
    // not write and the printer did not map. Nothing on it may reach the
    // report — not as a covered line, and not as a missed one. The author's
    // line 5 (`}`) and line 7 (`}`) are absent for the same reason: they
    // printed nothing of their own.
    assert_eq!(covered, vec![2, 3, 4, 6]);
    assert!(!covered.contains(&1), "`// @flow` is not executable");
}

#[test]
fn an_uncalled_block_inside_a_called_function_is_the_innermost_count() {
    let coverage = widget();
    let file = coverage.file("src/widget.js").expect("the fixture's file");
    let lines: std::collections::BTreeMap<u32, u64> = file.lines().collect();

    // The module ran once and `Widget` ran three times, so both of those
    // ranges contain the `return <b>` on author line 4 with a count above
    // zero. The `if` body's own range does not, and it is the deepest one, so
    // it is the one that decides.
    assert_eq!(lines[&4], 0);
    assert_eq!(lines[&3], 3);
}

#[test]
fn a_component_is_one_function_at_the_line_that_declares_it() {
    let coverage = widget();
    let file = coverage.file("src/widget.js").expect("the fixture's file");
    let functions: Vec<_> = file.functions().collect();

    assert_eq!(
        functions.len(),
        1,
        "the module's top level is not a function"
    );
    assert_eq!(functions[0].name, "Widget");
    assert_eq!(functions[0].at.line, 2);
    assert_eq!(functions[0].hits, 3);
}

#[test]
fn a_block_is_a_branch_at_the_first_line_the_author_wrote_inside_it() {
    let coverage = widget();
    let file = coverage.file("src/widget.js").expect("the fixture's file");
    let branches: Vec<_> = file.branches().collect();

    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].at.line, 4);
    assert_eq!(branches[0].hits, 0);
    assert_eq!(
        file.totals().branches,
        Ratio {
            covered: 0,
            total: 1
        }
    );
}

#[test]
fn a_generated_block_the_author_did_not_write_is_not_a_branch() {
    // The shape a `match` lowering produces: a guard the compiler emitted,
    // holding nothing but more generated code. It has a V8 range and a count,
    // and it maps to nothing — so it is neither a covered branch nor a missed
    // one, because it is not the author's branch.
    let invented_start = at("import { jsx");
    let coverage = parse_document(
        &document(&[
            json!({
                "functionName": "",
                "isBlockCoverage": true,
                "ranges": [{ "startOffset": 0, "endOffset": GENERATED.len(), "count": 1 }],
            }),
            json!({
                "functionName": "_guard",
                "isBlockCoverage": true,
                "ranges": [{
                    "startOffset": invented_start,
                    "endOffset": past("\"react/jsx-runtime\";"),
                    "count": 0,
                }],
            }),
        ]),
        Utf8Path::new(ROOT),
    )
    .expect("the fixture parses");

    let file = coverage.file("src/widget.js").expect("the fixture's file");
    assert_eq!(file.functions().len(), 0);
    assert_eq!(file.branches().len(), 0);
}

#[test]
fn a_project_script_with_no_source_map_is_left_out_and_counted() {
    let coverage = parse_document(
        &json!({
            "result": [{
                "scriptId": "9",
                "url": "file:///project/src/plain.js",
                "functions": [{
                    "functionName": "",
                    "isBlockCoverage": false,
                    "ranges": [{ "startOffset": 0, "endOffset": 10, "count": 1 }],
                }],
            }],
            "source-map-cache": {},
        })
        .to_string(),
        Utf8Path::new(ROOT),
    )
    .expect("the document parses");

    assert_eq!(coverage.file_count(), 0);
    assert_eq!(coverage.unmapped(), vec!["src/plain.js"]);
}

#[test]
fn a_dependency_under_the_project_root_is_not_the_projects_coverage() {
    // `node_modules` is inside the root and is not the project. The rule
    // mirrors `isFlowModule` in `packages/host/transform.js`, which is what
    // decides whether uf compiled the module in the first place — measuring a
    // wider set than uf compiles would report every dependency as an unmapped
    // script and bury the ones that matter.
    let coverage = parse_document(
        &json!({
            "result": [{
                "scriptId": "9",
                "url": "file:///project/node_modules/react/index.js",
                "functions": [{
                    "functionName": "",
                    "isBlockCoverage": false,
                    "ranges": [{ "startOffset": 0, "endOffset": 10, "count": 1 }],
                }],
            }],
            "source-map-cache": {},
        })
        .to_string(),
        Utf8Path::new(ROOT),
    )
    .expect("the document parses");

    assert_eq!(coverage.file_count(), 0);
    assert!(coverage.unmapped().is_empty());
}

#[test]
fn a_file_that_was_measured_is_not_also_reported_as_unmapped() {
    // The same module loaded twice under two specifiers — the worker's
    // `?uf-run=N` cache-buster is one — is two V8 scripts and one file. Only
    // one of the two carried a map here; the file is measured, so it is not a
    // gap in the report.
    let mut coverage = widget();
    let unmapped = parse_document(
        &json!({
            "result": [{
                "scriptId": "3",
                "url": format!("{AUTHOR_URL}?uf-run=2"),
                "functions": [],
            }],
            "source-map-cache": {},
        })
        .to_string(),
        Utf8Path::new(ROOT),
    )
    .expect("the document parses");
    coverage.merge(&unmapped);

    assert!(coverage.file("src/widget.js").is_some());
    assert!(coverage.unmapped().is_empty());
}

#[test]
fn a_script_outside_the_project_is_passed_over_in_silence() {
    // Not counted as skipped, because it was never a candidate. A run loads
    // thousands of these — Node's internals alone are tens of thousands of
    // scripts across a dozen workers — and a report that counted them would
    // put a five-figure number in front of a reader that told them only that
    // Node is large.
    let coverage = parse_document(&widget_document(), Utf8Path::new("/elsewhere"))
        .expect("the document parses");

    assert_eq!(coverage.file_count(), 0);
    assert!(coverage.unmapped().is_empty());
}

// --- merging ------------------------------------------------------------

/// Coverage for one file with these line counts, built without going through
/// a document: the merge is about the model, not the parser.
fn measured(document: &str) -> Coverage {
    parse_document(document, Utf8Path::new(ROOT)).expect("the fixture parses")
}

#[test]
fn two_workers_that_both_ran_a_file_sum_its_counts_and_report_it_once() {
    // The ordinary case, and the one a naive merge gets wrong in both
    // directions: a merge keyed by anything but the author's position would
    // either double the file or drop one worker's half of it.
    let mut merged = measured(&widget_document());
    merged.merge(&measured(&widget_document()));

    assert_eq!(merged.file_count(), 1, "one file, not two");
    let file = merged.file("src/widget.js").expect("the merged file");
    let lines: Vec<(u32, u64)> = file.lines().collect();
    assert_eq!(lines, vec![(2, 6), (3, 6), (4, 0), (6, 6)]);
    assert_eq!(
        file.functions().next().expect("the component").hits,
        6,
        "a function two workers entered three times each was entered six times"
    );
}

#[test]
fn a_file_only_one_worker_loaded_survives_the_merge() {
    let mut merged = measured(&widget_document());
    // A second worker that ran a different file entirely: nothing of the first
    // may be lost, and nothing of the second may be invented.
    let other = parse_document(
        &json!({
            "result": [{
                "scriptId": "2",
                "url": "file:///project/src/other.js",
                "functions": [{
                    "functionName": "",
                    "isBlockCoverage": true,
                    "ranges": [{ "startOffset": 0, "endOffset": 40, "count": 1 }],
                }],
            }],
            "source-map-cache": {
                "file:///project/src/other.js": {
                    "lineLengths": [20, 19, 0],
                    "data": {
                        "version": 3,
                        "sources": ["file:///project/src/other.js"],
                        "names": [],
                        "mappings": "AAAA",
                    },
                    "url": null,
                },
            },
        })
        .to_string(),
        Utf8Path::new(ROOT),
    )
    .expect("the second document parses");
    merged.merge(&other);

    assert_eq!(merged.file_count(), 2);
    assert!(merged.file("src/widget.js").is_some());
    assert!(merged.file("src/other.js").is_some());
    // Untouched by the merge: the second worker never loaded it.
    let lines: Vec<(u32, u64)> = merged
        .file("src/widget.js")
        .expect("the first file")
        .lines()
        .collect();
    assert_eq!(lines, vec![(2, 3), (3, 3), (4, 0), (6, 3)]);
}

#[test]
fn merging_is_the_same_whichever_order_the_workers_finished_in() {
    let first = measured(&widget_document());
    let second = parse_document(
        &document(&[json!({
            "functionName": "Widget",
            "isBlockCoverage": true,
            "ranges": [{
                "startOffset": at("function Widget"),
                "endOffset": past("  return null;\n}"),
                "count": 5,
            }],
        })]),
        Utf8Path::new(ROOT),
    )
    .expect("the fixture parses");

    let mut forwards = first.clone();
    forwards.merge(&second);
    let mut backwards = second;
    backwards.merge(&first);

    assert_eq!(forwards, backwards);
}

// --- thresholds ---------------------------------------------------------

#[test]
fn a_project_threshold_the_run_missed_is_reported_once_per_metric() {
    let coverage = widget();
    let violations = coverage.violations(
        Thresholds {
            lines: Some(80),
            functions: Some(100),
            branches: Some(50),
        },
        Thresholds::default(),
    );

    // Lines are 3 of 4, which is 75%; functions are 1 of 1; branches are 0 of
    // 1. So lines and branches fail and functions does not.
    let metrics: Vec<Metric> = violations
        .iter()
        .map(|violation| violation.metric)
        .collect();
    assert_eq!(metrics, vec![Metric::Lines, Metric::Branches]);
    assert!(violations.iter().all(|violation| violation.file.is_none()));
}

#[test]
fn a_threshold_is_compared_exactly_rather_than_through_a_rounded_percentage() {
    // 2 of 3 is 66.666…%, which prints as 66.67% and must not pass a
    // threshold of 67 because of how it printed.
    let two_thirds = Ratio {
        covered: 2,
        total: 3,
    };
    assert!(two_thirds.meets(66));
    assert!(!two_thirds.meets(67));
    // Nothing to cover is not a failure.
    assert!(Ratio::default().meets(100));
}

#[test]
fn a_file_the_suite_never_loaded_fails_a_per_file_threshold() {
    let coverage = widget().with_never_loaded(["src/forgotten.js"]);
    let violations = coverage.violations(
        Thresholds::default(),
        Thresholds {
            lines: Some(50),
            functions: None,
            branches: None,
        },
    );

    // `src/widget.js` is at 75% and passes. `src/forgotten.js` has no measured
    // line at all, and that is the case the per-file gate exists for: without
    // this it would be `0/0`, which every other ratio here calls a hundred per
    // cent, and a file nobody imports would be the best-covered file in the
    // project.
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].file.as_deref(), Some("src/forgotten.js"));
    assert_eq!(violations[0].actual, Ratio::default());
}

#[test]
fn a_scope_keeps_what_it_includes_and_drops_what_it_excludes() {
    let scope = CoverageScope::new().with_exclude(["widget"]);
    assert!(widget().within(&scope).file("src/widget.js").is_none());

    let scope = CoverageScope::new().with_include(["src/"]);
    assert!(widget().within(&scope).file("src/widget.js").is_some());

    let scope = CoverageScope::new().with_include(["packages/"]);
    assert!(widget().within(&scope).file("src/widget.js").is_none());

    // Exclusion wins, so a project can include a directory and still drop the
    // tests inside it — which is the default configuration.
    let scope = CoverageScope::new()
        .with_include(["src/"])
        .with_exclude(["widget"]);
    assert!(widget().within(&scope).file("src/widget.js").is_none());
}

// --- the documents a machine reads --------------------------------------

#[test]
fn lcov_carries_the_authors_lines_functions_and_branches() {
    assert_eq!(
        lcov(&widget()),
        "TN:\n\
         SF:src/widget.js\n\
         FN:2,Widget\n\
         FNDA:3,Widget\n\
         FNF:1\n\
         FNH:1\n\
         BRDA:4,0,0,-\n\
         BRF:1\n\
         BRH:0\n\
         DA:2,3\n\
         DA:3,3\n\
         DA:4,0\n\
         DA:6,3\n\
         LF:4\n\
         LH:3\n\
         end_of_record\n"
    );
}

#[test]
fn cobertura_carries_the_same_numbers_and_no_clock() {
    let xml = cobertura(&widget());

    assert!(xml.contains("lines-valid=\"4\" lines-covered=\"3\" line-rate=\"0.7500\""));
    assert!(xml.contains("branches-valid=\"1\" branches-covered=\"0\" branch-rate=\"0.0000\""));
    assert!(xml.contains("filename=\"src/widget.js\""));
    assert!(xml.contains(
        "<line number=\"4\" hits=\"0\" branch=\"true\" condition-coverage=\"0% (0/1)\"/>"
    ));
    // Two runs of one suite produce the same file, byte for byte.
    assert!(xml.contains("timestamp=\"0\""));
    assert_eq!(xml, cobertura(&widget()));
}

#[test]
fn the_terminal_row_names_the_lines_that_never_ran() {
    let rows = text_rows(&widget());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].file, "src/widget.js");
    assert_eq!(rows[0].uncovered, "4");
    assert_eq!(
        rows[0].of(Metric::Lines),
        Ratio {
            covered: 3,
            total: 4
        }
    );
}
