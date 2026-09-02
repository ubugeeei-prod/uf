//! Size budgets and their evaluation against a [`BundleReport`].

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::report::BundleReport;
use crate::size::{AssetSize, BudgetMetric, ByteSize};

/// A single budget: a ceiling plus the metric it applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeBudget {
    /// Largest size permitted.
    pub max: ByteSize,
    /// Which measurement the ceiling applies to.
    #[serde(default)]
    pub metric: BudgetMetric,
}

impl SizeBudget {
    /// Build a budget over the default metric.
    #[must_use]
    pub const fn new(max: ByteSize) -> Self {
        Self {
            max,
            metric: BudgetMetric::Gzip,
        }
    }

    /// Build a budget over an explicit metric.
    #[must_use]
    pub const fn with_metric(max: ByteSize, metric: BudgetMetric) -> Self {
        Self { max, metric }
    }

    /// Whether `size` fits.
    #[must_use]
    pub const fn admits(self, size: AssetSize) -> bool {
        size.get(self.metric).bytes() <= self.max.bytes()
    }
}

/// The `build.budgets` section of `uf.config.js`.
///
/// Every budget is optional and unset by default: a toolchain that fails a build
/// nobody asked it to police is worse than one that reports and moves on. Teams
/// that want the gate opt into it, and then it is an error, not a warning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BundleBudgets {
    /// Ceiling for every emitted asset added together.
    pub total: Option<SizeBudget>,
    /// Ceiling for the JavaScript a route loads before it can render.
    pub initial_js: Option<SizeBudget>,
    /// Ceiling applied to each route's total weight.
    pub per_route: Option<SizeBudget>,
    /// Ceiling applied to each individual asset.
    pub per_asset: Option<SizeBudget>,
}

impl BundleBudgets {
    /// Whether any budget is set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.total.is_none()
            && self.initial_js.is_none()
            && self.per_route.is_none()
            && self.per_asset.is_none()
    }
}

/// What a budget was measuring when it was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BudgetScope {
    /// The sum of every emitted asset.
    Total,
    /// The JavaScript a route loads before first render.
    InitialJs,
    /// One route's total weight.
    Route,
    /// One asset.
    Asset,
}

impl BudgetScope {
    /// Stable identifier used in reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Total => "total",
            Self::InitialJs => "initial-js",
            Self::Route => "route",
            Self::Asset => "asset",
        }
    }
}

/// One budget that was exceeded.
///
/// Carries everything needed to act without opening a profiler: what was
/// measured, which thing exceeded it, by how much, and under which metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetViolation {
    /// What the budget was measuring.
    pub scope: BudgetScope,
    /// The route or asset that exceeded it, or `None` for a whole-build budget.
    pub subject: Option<CompactString>,
    /// Metric the budget applied to.
    pub metric: BudgetMetric,
    /// The ceiling.
    pub budget: ByteSize,
    /// The measured size.
    pub actual: ByteSize,
    /// How far over the ceiling the measurement landed.
    pub overage: ByteSize,
}

impl std::fmt::Display for BudgetViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.subject {
            Some(subject) => write!(
                f,
                "{} budget exceeded for {subject}: {} {} over a {} ceiling (+{})",
                self.scope.as_str(),
                self.actual,
                self.metric.as_str(),
                self.budget,
                self.overage
            ),
            None => write!(
                f,
                "{} budget exceeded: {} {} over a {} ceiling (+{})",
                self.scope.as_str(),
                self.actual,
                self.metric.as_str(),
                self.budget,
                self.overage
            ),
        }
    }
}

/// Violations found while evaluating a report.
pub type BudgetViolations = SmallVec<[BudgetViolation; 4]>;

/// The result of checking a report against its budgets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetOutcome {
    /// Every violation, in a deterministic order.
    pub violations: BudgetViolations,
}

impl BudgetOutcome {
    /// Whether the build stays inside its budgets.
    #[must_use]
    pub fn is_within_budget(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Check `report` against `budgets`, collecting **every** violation.
///
/// Reporting only the first failure makes a size regression a game of
/// whack-a-mole across CI runs, so the whole list comes back at once. Ordering
/// is deterministic: whole-build scopes first, then routes and assets in the
/// report's own sorted order.
#[must_use]
pub fn evaluate(report: &BundleReport, budgets: &BundleBudgets) -> BudgetOutcome {
    let mut violations = BudgetViolations::new();

    if let Some(budget) = budgets.total {
        push_if_over(
            &mut violations,
            BudgetScope::Total,
            None,
            budget,
            report.total,
        );
    }

    for route in &report.routes {
        if let Some(budget) = budgets.initial_js {
            push_if_over(
                &mut violations,
                BudgetScope::InitialJs,
                Some(route.path.clone()),
                budget,
                route.initial_js,
            );
        }
        if let Some(budget) = budgets.per_route {
            push_if_over(
                &mut violations,
                BudgetScope::Route,
                Some(route.path.clone()),
                budget,
                route.total,
            );
        }
    }

    if let Some(budget) = budgets.per_asset {
        for asset in &report.assets {
            push_if_over(
                &mut violations,
                BudgetScope::Asset,
                Some(asset.path.clone()),
                budget,
                asset.size,
            );
        }
    }

    BudgetOutcome { violations }
}

fn push_if_over(
    violations: &mut BudgetViolations,
    scope: BudgetScope,
    subject: Option<CompactString>,
    budget: SizeBudget,
    size: AssetSize,
) {
    if budget.admits(size) {
        return;
    }

    let actual = size.get(budget.metric);
    violations.push(BudgetViolation {
        scope,
        subject,
        metric: budget.metric,
        budget: budget.max,
        actual,
        overage: actual.saturating_sub(budget.max),
    });
}

#[cfg(test)]
mod tests {
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
}
