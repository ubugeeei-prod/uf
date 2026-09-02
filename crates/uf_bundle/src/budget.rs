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
mod tests;
