//! What a report sorts by, what it draws, and what it escapes.

use std::time::Duration;

use super::*;

fn record(
    name: &'static str,
    self_ms: u64,
    inclusive_ms: u64,
    allocs: u64,
    byte_count: u64,
) -> ScopeRecord {
    ScopeRecord {
        name,
        hits: 1,
        inclusive: Duration::from_millis(inclusive_ms),
        self_time: Duration::from_millis(self_ms),
        allocations: allocs,
        bytes_allocated: byte_count,
        peak_above_baseline: byte_count,
        slowest: Duration::from_millis(inclusive_ms),
    }
}

fn iteration(spans: Vec<ScopeRecord>, elapsed_ms: u64) -> IterationRecord {
    IterationRecord {
        elapsed: Duration::from_millis(elapsed_ms),
        allocations: AllocDelta::default(),
        spans,
    }
}

/// One span per column, so each sort order has a different answer.
fn three_spans() -> Vec<IterationRecord> {
    vec![iteration(
        vec![
            // Most inclusive time, least self time: the root.
            record("root", 1, 100, 1, 10),
            // Most self time.
            record("hot", 60, 60, 2, 20),
            // Most allocations and bytes.
            record("greedy", 5, 5, 900, 90_000),
        ],
        100,
    )]
}

#[test]
fn the_default_order_is_self_time_rather_than_inclusive() {
    // Inclusive time always puts the root first, which every reader already
    // knew. Self time names the thing to fix.
    let report = Report::from_iterations("run", three_spans(), ReportConfig::default());
    assert_eq!(report.spans[0].name, "hot");

    let inclusive = ReportConfig {
        sort_by: SortBy::Inclusive,
        ..ReportConfig::default()
    };
    let report = Report::from_iterations("run", three_spans(), inclusive);
    assert_eq!(report.spans[0].name, "root");
}

#[test]
fn the_allocation_orders_find_the_span_that_allocates() {
    for sort_by in [SortBy::Bytes, SortBy::Allocations] {
        let config = ReportConfig {
            sort_by,
            ..ReportConfig::default()
        };
        let report = Report::from_iterations("run", three_spans(), config);
        assert_eq!(report.spans[0].name, "greedy", "sorted by {sort_by:?}");
    }
}

#[test]
fn iterations_are_folded_and_the_percentiles_come_out_in_order() {
    let runs = vec![
        iteration(vec![record("step", 1, 1, 1, 8)], 30),
        iteration(vec![record("step", 1, 1, 1, 8)], 10),
        iteration(vec![record("step", 1, 1, 1, 8)], 20),
    ];
    let report = Report::from_iterations("run", runs, ReportConfig::default());

    assert_eq!(report.iterations, 3);
    assert_eq!(report.fastest(), Duration::from_millis(10));
    assert_eq!(report.median(), Duration::from_millis(20));
    assert_eq!(report.slowest(), Duration::from_millis(30));
    assert_eq!(report.mean(), Duration::from_millis(20));
    // One record per name across every iteration.
    assert_eq!(report.spans.len(), 1);
    assert_eq!(report.spans[0].hits, 3);
}

#[test]
fn an_empty_report_answers_rather_than_dividing_by_zero() {
    let report = Report::from_iterations("nothing", Vec::new(), ReportConfig::default());
    assert_eq!(report.fastest(), Duration::ZERO);
    assert_eq!(report.median(), Duration::ZERO);
    assert_eq!(report.mean(), Duration::ZERO);
    assert_eq!(report.slowest(), Duration::ZERO);
    assert!(report.render_table().contains("no spans recorded"));
}

#[test]
fn the_row_limit_cuts_the_table_and_says_how_many_it_cut() {
    let config = ReportConfig {
        rows: Some(2),
        ..ReportConfig::default()
    };
    let report = Report::from_iterations("run", three_spans(), config);
    assert_eq!(report.rows().len(), 2);
    let table = report.render_table();
    assert!(table.contains("and 1 more"), "{table}");
    // The row that was cut is the least interesting one by the sort order.
    assert!(!table.contains("root"), "{table}");
}

#[test]
fn the_overhead_column_appears_only_when_it_was_measured() {
    let report = Report::from_iterations("run", three_spans(), ReportConfig::default());
    assert!(
        !report.render_table().contains("overhead"),
        "no calibration, no column"
    );

    let config = ReportConfig {
        overhead_ns: 25.0,
        ..ReportConfig::default()
    };
    let report = Report::from_iterations("run", three_spans(), config);
    let table = report.render_table();
    assert!(table.contains("overhead"), "{table}");
}

#[test]
fn a_duration_is_rendered_at_a_scale_a_reader_can_hold() {
    assert_eq!(duration(Duration::from_nanos(999)), "999 ns");
    assert_eq!(duration(Duration::from_nanos(1_500)), "1.50 \u{b5}s");
    assert_eq!(duration(Duration::from_micros(1_500)), "1.50 ms");
    assert_eq!(duration(Duration::from_millis(1_500)), "1.50 s");
}

#[test]
fn bytes_are_rendered_in_the_unit_a_reader_would_say() {
    assert_eq!(bytes(512), "512 B");
    assert_eq!(bytes(1024), "1.0 KiB");
    assert_eq!(bytes(1024 * 1024), "1.0 MiB");
    assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
}

#[test]
fn the_json_is_json_even_when_a_label_is_hostile() {
    // A label comes from the caller, and a caller can pass anything. An
    // unescaped quote would produce a document no reader can parse, which is
    // worse than a wrong number because nothing downstream even starts.
    let hostile = format!("a \"label\" with \\ and \n in it{}", '\u{7}');
    let report = Report::from_iterations(hostile, three_spans(), ReportConfig::default());
    let json = report.render_json();

    assert!(json.contains("\\\"label\\\""), "{json}");
    assert!(json.contains("\\\\"), "{json}");
    assert!(json.contains("\\n"), "{json}");
    assert!(
        json.contains("\\u0007"),
        "the bell was escaped rather than emitted: {json}"
    );
    assert!(
        !json.contains('\u{7}'),
        "a control character reached the document: {json}"
    );
    // Balanced, which is the cheap proof that nothing broke out of a string.
    assert_eq!(json.matches('{').count(), json.matches('}').count());
    assert_eq!(json.matches('[').count(), json.matches(']').count());
}

#[test]
fn the_json_carries_every_span_rather_than_only_the_drawn_rows() {
    // The table is for a person and is cut to the interesting rows; the
    // document is for a script comparing against last week, and a row that
    // vanished because it fell below the limit would read as a regression to
    // zero.
    let config = ReportConfig {
        rows: Some(1),
        ..ReportConfig::default()
    };
    let report = Report::from_iterations("run", three_spans(), config);
    assert_eq!(report.rows().len(), 1);
    let json = report.render_json();
    for name in ["root", "hot", "greedy"] {
        assert!(json.contains(name), "{name} is missing from {json}");
    }
}
