use super::*;
use crate::report::{AssetEntry, AssetKind, RouteEntry};

fn size(raw: u64, gzip: u64, brotli: u64) -> AssetSize {
    AssetSize {
        raw: ByteSize::from_bytes(raw),
        gzip: ByteSize::from_bytes(gzip),
        brotli: ByteSize::from_bytes(brotli),
    }
}

fn report() -> BundleReport {
    BundleReport {
        // Sorted by path, the way `build_report` emits them.
        assets: vec![
            AssetEntry {
                path: CompactString::const_new("assets/app.css"),
                kind: AssetKind::Stylesheet,
                size: size(100, 40, 30),
            },
            AssetEntry {
                path: CompactString::const_new("assets/app.js"),
                kind: AssetKind::JavaScript,
                size: size(300, 120, 100),
            },
        ],
        routes: vec![RouteEntry {
            path: CompactString::const_new("/"),
            initial_js: size(300, 120, 100),
            lazy_js: size(0, 0, 0),
            total: size(400, 160, 130),
            assets: vec![CompactString::const_new("assets/app.js")],
        }],
        total: size(400, 160, 130),
    }
}

#[test]
fn no_budgets_means_no_violations() {
    let outcome = evaluate(&report(), &BundleBudgets::default());

    assert!(outcome.is_within_budget());
}

#[test]
fn budgets_default_to_the_gzip_metric() {
    let budgets = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(150))),
        ..BundleBudgets::default()
    };

    let outcome = evaluate(&report(), &budgets);

    assert_eq!(outcome.violations.len(), 1);
    assert_eq!(outcome.violations[0].metric, BudgetMetric::Gzip);
    assert_eq!(outcome.violations[0].actual.bytes(), 160);
    assert_eq!(outcome.violations[0].overage.bytes(), 10);
}

#[test]
fn a_size_exactly_on_the_ceiling_passes() {
    let budgets = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(160))),
        ..BundleBudgets::default()
    };

    assert!(evaluate(&report(), &budgets).is_within_budget());
}

#[test]
fn an_explicit_metric_changes_which_number_is_checked() {
    let budgets = BundleBudgets {
        total: Some(SizeBudget::with_metric(
            ByteSize::from_bytes(150),
            BudgetMetric::Brotli,
        )),
        ..BundleBudgets::default()
    };

    assert!(evaluate(&report(), &budgets).is_within_budget());

    let budgets = BundleBudgets {
        total: Some(SizeBudget::with_metric(
            ByteSize::from_bytes(150),
            BudgetMetric::Raw,
        )),
        ..BundleBudgets::default()
    };

    assert_eq!(evaluate(&report(), &budgets).violations.len(), 1);
}

#[test]
fn every_violation_is_reported_not_just_the_first() {
    let budgets = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        initial_js: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        per_route: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        per_asset: Some(SizeBudget::new(ByteSize::from_bytes(1))),
    };

    let outcome = evaluate(&report(), &budgets);

    assert_eq!(outcome.violations.len(), 5);
    assert!(!outcome.is_within_budget());
}

#[test]
fn violation_order_is_deterministic() {
    let budgets = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        per_asset: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        ..BundleBudgets::default()
    };

    let first = evaluate(&report(), &budgets);
    let second = evaluate(&report(), &budgets);

    assert_eq!(first, second);
    assert_eq!(first.violations[0].scope, BudgetScope::Total);
    assert_eq!(
        first.violations[1].subject.as_deref(),
        Some("assets/app.css")
    );
    assert_eq!(
        first.violations[2].subject.as_deref(),
        Some("assets/app.js")
    );
}

#[test]
fn initial_js_is_budgeted_separately_from_route_total() {
    let budgets = BundleBudgets {
        initial_js: Some(SizeBudget::new(ByteSize::from_bytes(100))),
        per_route: Some(SizeBudget::new(ByteSize::from_bytes(1_000))),
        ..BundleBudgets::default()
    };

    let outcome = evaluate(&report(), &budgets);

    assert_eq!(outcome.violations.len(), 1);
    assert_eq!(outcome.violations[0].scope, BudgetScope::InitialJs);
    assert_eq!(outcome.violations[0].subject.as_deref(), Some("/"));
}

#[test]
fn violation_messages_name_the_subject_budget_actual_and_overage() {
    let budgets = BundleBudgets {
        per_asset: Some(SizeBudget::new(ByteSize::from_bytes(50))),
        ..BundleBudgets::default()
    };

    let outcome = evaluate(&report(), &budgets);

    // `app.css` is 40 bytes gzipped and fits; only `app.js` breaks the ceiling.
    assert_eq!(outcome.violations.len(), 1);
    let rendered = outcome.violations[0].to_string();

    assert!(rendered.contains("assets/app.js"), "{rendered}");
    assert!(rendered.contains("gzip"), "{rendered}");
    assert!(rendered.contains("asset"), "{rendered}");
}

#[test]
fn a_whole_build_violation_has_no_subject() {
    let budgets = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        ..BundleBudgets::default()
    };

    let outcome = evaluate(&report(), &budgets);

    assert!(outcome.violations[0].subject.is_none());
    assert!(!outcome.violations[0].to_string().contains("for "));
}

#[test]
fn empty_budgets_are_recognized() {
    assert!(BundleBudgets::default().is_empty());
    assert!(
        !BundleBudgets {
            total: Some(SizeBudget::new(ByteSize::from_bytes(1))),
            ..BundleBudgets::default()
        }
        .is_empty()
    );
}

#[test]
fn scope_identifiers_are_stable() {
    assert_eq!(BudgetScope::Total.as_str(), "total");
    assert_eq!(BudgetScope::InitialJs.as_str(), "initial-js");
    assert_eq!(BudgetScope::Route.as_str(), "route");
    assert_eq!(BudgetScope::Asset.as_str(), "asset");
}

#[test]
fn an_empty_report_never_violates() {
    let budgets = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(0))),
        initial_js: Some(SizeBudget::new(ByteSize::from_bytes(0))),
        per_route: Some(SizeBudget::new(ByteSize::from_bytes(0))),
        per_asset: Some(SizeBudget::new(ByteSize::from_bytes(0))),
    };

    let outcome = evaluate(&BundleReport::default(), &budgets);

    assert!(outcome.is_within_budget());
}
